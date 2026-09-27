//! Rigid-body physics for a running game, using Rapier.
//!
//! The DataModel stays the single source of truth that scripts and the
//! renderer see. Each step:
//!   1. anything a script changed (moved, resized, anchored...) is copied
//!      into the physics world,
//!   2. physics advances in fixed 1/60 s steps,
//!   3. where unanchored parts ended up is copied back.
//!
//! To tell a script's change from physics' own motion, we remember the
//! exact properties we last synced for each part. If the world differs from
//! that, a script did it.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver};

use brixo_core::{DataModel, Hinge, InstanceId, PartProps, Vec3 as BVec3, Shape};
use glam::{EulerRot, Quat, Vec3};
use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};
use rapier3d::prelude::*;
#[allow(unused_imports)]
use rapier3d::prelude::CoefficientCombineRule;

mod kart;
pub use kart::{KartCommand, DRIFT_MINI, DRIFT_SUPER, TOP_SPEED};

/// Physics runs at a fixed rate so it behaves the same on every machine.
pub const PHYSICS_DT: f32 = 1.0 / 60.0;
/// Downward acceleration in studs per second squared. With 1 stud at about
/// 0.28 m (Roblox's scale), this is Earth's gravity.
pub const GRAVITY: f32 = 110.0;
/// A slow frame is caught up at most this far; beyond that the simulation
/// just runs a little slow instead of freezing trying to catch up.
const MAX_CATCH_UP: f32 = 0.5;

/// What the player is pressing this frame. `move_x`/`move_z` is a world
/// direction (already turned to match the camera), at most 1 long.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PlayerInput {
    pub move_x: f32,
    pub move_z: f32,
    pub jump: bool,
}

/// The character's capsule: 5 studs tall, 2 wide (radius 1).
const CHARACTER_RADIUS: f32 = 1.0;
const CHARACTER_HALF_HEIGHT: f32 = 2.5 - CHARACTER_RADIUS;
/// How long after walking off a ledge you can still jump.
pub const COYOTE_TIME: f32 = 0.12;
/// How long a jump press is remembered before landing.
pub const JUMP_BUFFER: f32 = 0.12;
/// Gravity is multiplied by this while rising with jump held, so holding
/// jumps higher than tapping: the floaty, forgiving feel.
pub const JUMP_HOLD_GRAVITY: f32 = 0.6;
/// Fastest the character falls, studs per second.
pub const MAX_FALL_SPEED: f32 = 150.0;
/// Gravity is multiplied by this while falling: a jump comes down faster
/// than it goes up, which feels snappy instead of floaty.
pub const FALL_GRAVITY: f32 = 1.4;

/// How hard the character shoves loose parts it walks into.
const CHARACTER_MASS: f32 = 60.0;

struct Character {
    id: InstanceId,
    body: RigidBodyHandle,
    collider: ColliderHandle,
    controller: KinematicCharacterController,
    vertical_speed: f32,
    grounded: bool,
    /// Time left to jump after leaving the ground.
    coyote: f32,
    /// Time left on a remembered jump press.
    jump_buffer: f32,
    jump_was_held: bool,
    /// Facing direction, radians around Y.
    yaw: f32,
    /// Sideways momentum from a launch (a jump pad, a blast): it carries the
    /// character along and dies away, quickly on the ground, slowly in the air.
    push: Vec3,
    /// Where we last put the character, to spot scripts teleporting it.
    synced_position: BVec3,
    /// What the character stood on last step, and where that was: if it
    /// moves (a platform) or is a conveyor, the character goes along.
    ground: Option<(ColliderHandle, Vec3)>,
    /// Walking speed last step, for the animation.
    speed: f32,
    /// Parts the character is overlapping, to find new touches.
    touching: std::collections::HashSet<InstanceId>,
}

struct Tracked {
    body: RigidBodyHandle,
    collider: ColliderHandle,
    /// The properties as of the last sync in either direction.
    synced: PartProps,
    /// Whether a script is listening for this part's touches.
    listening: bool,
}

pub struct Physics {
    pipeline: PhysicsPipeline,
    params: IntegrationParameters,
    islands: IslandManager,
    broad_phase: BroadPhaseBvh,
    narrow_phase: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd: CCDSolver,
    events: ChannelEventCollector,
    collisions: Receiver<CollisionEvent>,
    // Rapier also reports contact forces; we don't use them yet, but the
    // receiver has to stay alive for the collector to work.
    _forces: Receiver<ContactForceEvent>,
    parts: HashMap<InstanceId, Tracked>,
    owners: HashMap<ColliderHandle, InstanceId>,
    accumulator: f32,
    /// How many fixed steps have run, ever.
    steps_run: u64,
    steps_taken: u64,
    /// One character per player, by the player's id.
    characters: HashMap<InstanceId, Character>,
    /// Parts in a Model are welded to the Model's first part:
    /// part -> (the part it's welded to, the joint).
    welds: HashMap<InstanceId, (InstanceId, ImpulseJointHandle)>,
    /// Players who face a fixed way whatever way they walk (shift lock:
    /// the way the camera looks), by player id.
    held_yaw: HashMap<InstanceId, f32>,
    /// Hinged parts: the joint holding each one.
    hinges: HashMap<InstanceId, HingeJoint>,
    /// Karts, by their Model, and what drives each this step.
    karts: HashMap<InstanceId, kart::Kart>,
    kart_inputs: HashMap<InstanceId, crate::kart::KartInput>,
    /// Who was driving which kart last step (to let them out when they stop).
    seated: HashMap<InstanceId, InstanceId>,
}

/// A hinged part's joint, and what it was built from (a change rebuilds it).
struct HingeJoint {
    joint: ImpulseJointHandle,
    /// The part it's hinged to, or None for the spot it was in.
    to: Option<InstanceId>,
    /// For a hinge to the world: the fixed body it hangs on.
    anchor: Option<RigidBodyHandle>,
    built_from: (Hinge, brixo_core::Side, BVec3),
    /// The motor settings last applied.
    motor: (f32, Option<f32>),
}

/// How hard hinge motors push (their acceleration model ignores mass, so
/// a big door and a small one move alike).
const MOTOR_STIFFNESS: f32 = 300.0;
const MOTOR_DAMPING: f32 = 40.0;
const MOTOR_FACTOR: f32 = 30.0;
/// Hinged parts turn with a little friction, so a swinging sign or a
/// pushed door slows down and settles like a real one.
const HINGE_DAMPING: f32 = 3.0;

impl Default for Physics {
    fn default() -> Self {
        Self::new()
    }
}

impl Physics {
    pub fn new() -> Self {
        let (collision_tx, collisions) = mpsc::channel();
        let (force_tx, forces) = mpsc::channel();
        let mut params = IntegrationParameters::default();
        params.dt = PHYSICS_DT;
        Self {
            pipeline: PhysicsPipeline::new(),
            params,
            islands: IslandManager::default(),
            broad_phase: BroadPhaseBvh::default(),
            narrow_phase: NarrowPhase::default(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd: CCDSolver::new(),
            events: ChannelEventCollector::new(collision_tx, force_tx),
            collisions,
            _forces: forces,
            parts: HashMap::new(),
            owners: HashMap::new(),
            accumulator: 0.0,
            steps_run: 0,
            steps_taken: 0,
            characters: HashMap::new(),
            welds: HashMap::new(),
            held_yaw: HashMap::new(),
            hinges: HashMap::new(),
            karts: HashMap::new(),
            kart_inputs: HashMap::new(),
            seated: HashMap::new(),
        }
    }

    /// Advances by `dt` seconds: syncs changes in, simulates, syncs results
    /// out. `listeners` are the parts with `on touched` scripts. Returns the
    /// pairs of parts that started touching (at least one is a listener).
    pub fn step(
        &mut self,
        world: &mut DataModel,
        dt: f32,
        listeners: &HashSet<InstanceId>,
        inputs: &HashMap<InstanceId, PlayerInput>,
    ) -> Vec<(InstanceId, InstanceId)> {
        self.pull_from_world(world, listeners);
        self.sync_welds(world);
        self.sync_hinges(world);
        self.release_drivers(world);
        self.sync_characters(world);
        self.sync_karts(world);

        self.accumulator = (self.accumulator + dt.max(0.0)).min(MAX_CATCH_UP);
        let mut touches = Vec::new();
        while self.accumulator >= PHYSICS_DT {
            self.accumulator -= PHYSICS_DT;
            self.steps_run += 1;
            let ids: Vec<InstanceId> = self.characters.keys().copied().collect();
            for id in ids {
                // (Drivers sit in their kart instead.)
                if world.player(id).and_then(|p| p.kart).is_some_and(|k| self.karts.contains_key(&k)) {
                    continue;
                }
                let input = inputs.get(&id).copied().unwrap_or_default();
                self.move_character(world, id, input);
            }
            self.move_karts(world);
            self.apply_conveyors();
            self.pipeline.step(
                Vec3::new(0.0, -GRAVITY, 0.0),
                &self.params,
                &mut self.islands,
                &mut self.broad_phase,
                &mut self.narrow_phase,
                &mut self.bodies,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                &mut self.ccd,
                &(),
                &self.events,
            );
            self.steps_taken += 1;

            while let Ok(event) = self.collisions.try_recv() {
                // Contacts found on the very first step were already there
                // when Play started, so they aren't new touches.
                if self.steps_taken == 1 {
                    continue;
                }
                if let CollisionEvent::Started(a, b, _) = event {
                    if let (Some(&x), Some(&y)) = (self.owners.get(&a), self.owners.get(&b)) {
                        touches.push((x, y));
                    }
                }
            }
        }

        self.push_to_world(world);
        self.push_karts(world);
        let ids: Vec<InstanceId> = self.characters.keys().copied().collect();
        for id in ids {
            touches.extend(self.character_touches(world, id));
        }
        touches
    }

    // --- conveyors ---

    /// An explosion: loose parts within `radius` of `center` are thrown
    /// outward (and a little upward), harder the closer they are, and
    /// set spinning. Anchored parts don't move.
    pub fn blast(&mut self, center: Vec3, radius: f32, strength: f32) {
        for (_, body) in self.bodies.iter_mut() {
            if !body.is_dynamic() {
                continue;
            }
            let away = body.translation() - center;
            let distance = away.length();
            if distance >= radius {
                continue;
            }
            let dir = if distance > 0.01 { away / distance } else { Vec3::Y };
            let push = strength * (1.0 - distance / radius);
            let kick = (dir + Vec3::Y * 0.35).normalize() * push;
            body.set_linvel(body.linvel() + kick, true);
            body.set_angvel(body.angvel() + Vec3::new(dir.z, 0.6, -dir.x) * push * 0.25, true);
        }
    }

    /// Anchored parts with a velocity are conveyor belts: loose things
    /// touching them are carried along at that speed.
    fn apply_conveyors(&mut self) {
        let belts: Vec<(ColliderHandle, Vec3)> = self
            .parts
            .values()
            .filter(|t| t.synced.anchored && t.synced.velocity != BVec3::ZERO)
            .map(|t| (t.collider, to_glam(t.synced.velocity)))
            .collect();
        for (belt, v) in belts {
            let riders: Vec<ColliderHandle> = self
                .narrow_phase
                .contact_pairs_with(belt)
                .filter(|p| p.has_any_active_contact())
                .map(|p| if p.collider1 == belt { p.collider2 } else { p.collider1 })
                .collect();
            for c in riders {
                let Some(parent) = self.colliders.get(c).and_then(|c| c.parent()) else { continue };
                if let Some(body) = self.bodies.get_mut(parent) {
                    if body.is_dynamic() {
                        let now = body.linvel();
                        body.set_linvel(Vec3::new(v.x, now.y, v.z), true);
                    }
                }
            }
        }
    }

    // --- welds ---

    /// Makes the joints match the Models: every part in a Model is held to
    /// the Model's first part, exactly where it was placed.
    fn sync_welds(&mut self, world: &DataModel) {
        let mut groups: HashMap<InstanceId, Vec<InstanceId>> = HashMap::new();
        for id in world.walk() {
            if self.parts.contains_key(&id) {
                if let Some(model) = world.weld_group(id) {
                    groups.entry(model).or_default().push(id);
                }
            }
        }
        let mut wanted: HashMap<InstanceId, InstanceId> = HashMap::new();
        for members in groups.values() {
            // A hinged part turns on its own, so it isn't welded; a hinged
            // first part takes the rest of the Model with it (a door made
            // of several parts).
            for &part in &members[1..] {
                if self.parts.get(&part).is_some_and(|t| t.synced.hinge != Hinge::Off) {
                    continue;
                }
                wanted.insert(part, members[0]);
            }
        }

        // Drop welds that shouldn't exist, or whose joint went away with a
        // removed body.
        let stale: Vec<InstanceId> = self
            .welds
            .iter()
            .filter(|(part, (to, joint))| wanted.get(*part) != Some(to) || self.impulse_joints.get(*joint).is_none())
            .map(|(part, _)| *part)
            .collect();
        for part in stale {
            let (_, joint) = self.welds.remove(&part).unwrap();
            self.impulse_joints.remove(joint, true);
        }

        for (part, to) in wanted {
            if self.welds.contains_key(&part) {
                continue;
            }
            let (Some(a), Some(b)) = (self.parts.get(&to), self.parts.get(&part)) else { continue };
            let (body_a, body_b) = (a.body, b.body);
            let (Some(ra), Some(rb)) = (self.bodies.get(body_a), self.bodies.get(body_b)) else { continue };
            // Hold `part` wherever it sits relative to `to` right now.
            let relative = ra.position().inverse() * *rb.position();
            let joint = FixedJointBuilder::new()
                .local_frame1(relative)
                .local_frame2(Pose::IDENTITY)
                .contacts_enabled(false);
            let handle = self.impulse_joints.insert(body_a, body_b, joint, true);
            self.welds.insert(part, (to, handle));
        }
    }

    // --- hinges ---

    /// Makes the hinge joints match the parts: each loose part with a hinge
    /// is held, turning around its hinge line, to the part it touches
    /// nearest its hinge point (or to where it is, touching nothing).
    fn sync_hinges(&mut self, world: &DataModel) {
        let wanted: HashMap<InstanceId, PartProps> = self
            .parts
            .iter()
            .filter(|(_, t)| t.synced.hinge != Hinge::Off && !t.synced.anchored)
            .map(|(id, t)| (*id, t.synced))
            .collect();
        let stale: Vec<InstanceId> = self
            .hinges
            .iter()
            .filter(|(id, h)| {
                let Some(p) = wanted.get(*id) else { return true };
                h.built_from != (p.hinge, p.hinge_at, p.size)
                    || self.impulse_joints.get(h.joint).is_none()
                    || h.to.is_some_and(|to| !self.parts.contains_key(&to))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            self.drop_hinge(id);
        }
        let mut ids: Vec<InstanceId> = wanted.keys().copied().collect();
        ids.sort_by_key(|id| id.raw());
        for id in ids {
            let props = wanted[&id];
            if !self.hinges.contains_key(&id) {
                self.build_hinge(world, id, &props);
            }
            let Some(h) = self.hinges.get_mut(&id) else { continue };
            let motor = (props.motor_speed, props.swing_to);
            if h.motor != motor {
                h.motor = motor;
                let joint = h.joint;
                if let Some(j) = self.impulse_joints.get_mut(joint, true) {
                    set_motor(&mut j.data, motor);
                }
            }
            // A running motor keeps its part awake (a sleeping body ignores it).
            if props.motor_speed != 0.0 || props.swing_to.is_some() {
                if let Some(b) = self.parts.get(&id).and_then(|t| self.bodies.get_mut(t.body)) {
                    b.wake_up(true);
                }
            }
        }
    }

    fn drop_hinge(&mut self, id: InstanceId) {
        let Some(h) = self.hinges.remove(&id) else { return };
        if let Some(b) = self.parts.get(&id).and_then(|t| self.bodies.get_mut(t.body)) {
            b.set_angular_damping(0.0);
        }
        self.impulse_joints.remove(h.joint, true);
        if let Some(anchor) = h.anchor {
            self.bodies.remove(anchor, &mut self.islands, &mut self.colliders, &mut self.impulse_joints, &mut self.multibody_joints, true);
        }
    }

    fn build_hinge(&mut self, world: &DataModel, id: InstanceId, props: &PartProps) {
        let Some(axis) = props.hinge.axis() else { return };
        let Some(body) = self.parts.get(&id).map(|t| t.body) else { return };
        let Some(pose) = self.bodies.get(body).map(|b| *b.position()) else { return };
        let pivot_local = to_glam(props.hinge_at.point(props.size));
        // The joint's free axis is its X: turn X onto the hinge line.
        let frame2 = Pose::from_parts(pivot_local, Quat::from_rotation_arc(Vec3::X, to_glam(axis)));
        let pivot_world = pose * frame2;
        let to = self.hinge_partner(world, id, props, pivot_world.translation);
        let (body1, anchor, frame1) = match to.and_then(|t| self.parts.get(&t)).map(|t| t.body) {
            Some(b1) => {
                let p1 = *self.bodies.get(b1).unwrap().position();
                (b1, None, p1.inverse() * pivot_world)
            }
            None => {
                let a = self.bodies.insert(RigidBodyBuilder::fixed().build());
                (a, Some(a), pivot_world)
            }
        };
        let mut data: GenericJoint = GenericJointBuilder::new(JointAxesMask::LOCKED_REVOLUTE_AXES)
            .local_frame1(frame1)
            .local_frame2(frame2)
            .contacts_enabled(false)
            .build();
        let motor = (props.motor_speed, props.swing_to);
        set_motor(&mut data, motor);
        let joint = self.impulse_joints.insert(body1, body, data, true);
        if let Some(b) = self.bodies.get_mut(body) {
            b.set_angular_damping(HINGE_DAMPING);
        }
        self.hinges.insert(id, HingeJoint { joint, to: if anchor.is_some() { None } else { to }, anchor, built_from: (props.hinge, props.hinge_at, props.size), motor });
    }

    /// What a hinged part hangs on: of the parts it touches, the one
    /// nearest its hinge point. Not the parts welded to it (they turn
    /// with it), and not its own Model's other hinged parts (a car's
    /// wheels hang on the car, not on each other).
    fn hinge_partner(&self, world: &DataModel, id: InstanceId, props: &PartProps, pivot: Vec3) -> Option<InstanceId> {
        let (lo, hi) = world_box(props);
        let mine: HashSet<InstanceId> = self.welds.iter().filter(|(_, (to, _))| *to == id).map(|(p, _)| *p).collect();
        let group = world.weld_group(id);
        let mut best: Option<(f32, InstanceId)> = None;
        for (&other, t) in &self.parts {
            if other == id || mine.contains(&other) || !t.synced.can_collide && t.synced.transparency >= 1.0 {
                continue;
            }
            if t.synced.hinge != Hinge::Off && group.is_some() && world.weld_group(other) == group {
                continue;
            }
            let (olo, ohi) = world_box(&t.synced);
            let gap = 0.05;
            let touching = lo.x <= ohi.x + gap && hi.x + gap >= olo.x && lo.y <= ohi.y + gap && hi.y + gap >= olo.y && lo.z <= ohi.z + gap && hi.z + gap >= olo.z;
            if !touching {
                continue;
            }
            let d = pivot.clamp(olo, ohi).distance(pivot);
            if best.is_none_or(|(bd, bid)| d < bd - 1e-4 || (d - bd).abs() <= 1e-4 && other.raw() < bid.raw()) {
                best = Some((d, other));
            }
        }
        best.map(|(_, id)| id)
    }

    /// Each hinged part's angle right now, in degrees from where it started.
    pub fn hinge_angles(&self) -> HashMap<InstanceId, f32> {
        let mut out = HashMap::new();
        for (id, h) in &self.hinges {
            let Some(j) = self.impulse_joints.get(h.joint) else { continue };
            let (Some(b1), Some(b2)) = (self.bodies.get(j.body1()), self.bodies.get(j.body2())) else { continue };
            if let Some(r) = j.data.as_revolute() {
                out.insert(*id, r.angle(b1.rotation(), b2.rotation()).to_degrees());
            }
        }
        out
    }

    // --- the player's character ---

    /// Creates, removes or teleports characters to match the world's players.
    fn sync_characters(&mut self, world: &DataModel) {
        let players: HashSet<InstanceId> =
            world.walk().into_iter().filter(|id| world.player(*id).is_some()).collect();

        // Players who left (or were destroyed) lose their capsule.
        let gone: Vec<InstanceId> = self.characters.keys().filter(|id| !players.contains(id)).copied().collect();
        for id in gone {
            let c = self.characters.remove(&id).unwrap();
            self.bodies.remove(
                c.body,
                &mut self.islands,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                true,
            );
        }

        for id in players {
            let position = world.player(id).unwrap().body.position;
            match self.characters.get_mut(&id) {
                None => {
                    let c = self.new_character(id, position);
                    self.characters.insert(id, c);
                }
                // A script (or a respawn) moved the player: teleport the capsule.
                Some(c) if position != c.synced_position => {
                    c.synced_position = position;
                    c.vertical_speed = 0.0;
                    c.ground = None; // a teleport leaves whatever it stood on behind
                    if let Some(body) = self.bodies.get_mut(c.body) {
                        body.set_translation(to_glam(position), true);
                    }
                }
                Some(_) => {}
            }
        }
    }

    fn new_character(&mut self, id: InstanceId, position: BVec3) -> Character {
        let body = self.bodies.insert(
            RigidBodyBuilder::kinematic_position_based()
                .translation(to_glam(position))
                .build(),
        );
        let collider = self.colliders.insert_with_parent(
            ColliderBuilder::capsule_y(CHARACTER_HALF_HEIGHT, CHARACTER_RADIUS).friction(0.0),
            body,
            &mut self.bodies,
        );
        let controller = KinematicCharacterController {
            offset: CharacterLength::Absolute(0.05),
            // Walks up anything a stud high, like stairs, without jumping.
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(1.1),
                min_width: CharacterLength::Absolute(0.4),
                include_dynamic_bodies: false,
            }),
            snap_to_ground: Some(CharacterLength::Absolute(0.4)),
            max_slope_climb_angle: 50f32.to_radians(),
            ..KinematicCharacterController::default()
        };
        Character {
            id,
            body,
            collider,
            controller,
            vertical_speed: 0.0,
            grounded: false,
            coyote: 0.0,
            jump_buffer: 0.0,
            jump_was_held: false,
            yaw: 0.0,
            push: Vec3::ZERO,
            synced_position: position,
            ground: None,
            speed: 0.0,
            touching: Default::default(),
        }
    }

    /// One physics step of walking, jumping and falling.
    fn move_character(&mut self, world: &DataModel, id: InstanceId, input: PlayerInput) {
        let Some(c) = self.characters.get_mut(&id) else { return };
        let Some(player) = world.player(c.id) else { return };
        if player.dead > 0.0 {
            return; // fallen apart: stays put until respawning
        }
        let dt = PHYSICS_DT;

        let mut dir = Vec3::new(input.move_x, 0.0, input.move_z);
        if dir.length_squared() > 1.0 {
            dir = dir.normalize();
        }
        if let Some(yaw) = self.held_yaw.get(&c.id) {
            // Shift lock: face the camera's way, walking sideways and back.
            c.yaw = *yaw;
        } else if dir.length_squared() > 0.0001 {
            c.yaw = dir.x.atan2(dir.z);
        }

        // A fresh press is remembered briefly, so pressing just before
        // landing still jumps.
        if input.jump && !c.jump_was_held {
            c.jump_buffer = JUMP_BUFFER;
        }
        c.jump_was_held = input.jump;
        // Coyote time: a moment after leaving a ledge where jumping still works.
        if c.grounded {
            c.coyote = COYOTE_TIME;
        }
        // Holding jump keeps hopping on landing, like Roblox.
        let wants_jump = c.jump_buffer > 0.0 || (input.jump && c.grounded);
        if wants_jump && c.coyote > 0.0 && c.vertical_speed <= 0.0 {
            c.vertical_speed = player.jump_power;
            c.grounded = false;
            c.coyote = 0.0;
            c.jump_buffer = 0.0;
        }
        c.jump_buffer = (c.jump_buffer - dt).max(0.0);
        if !c.grounded {
            c.coyote = (c.coyote - dt).max(0.0);
        }

        // No gravity while standing: pushing down into the ground makes the
        // controller snag on it instead of sliding along. Snap-to-ground
        // keeps the character on slopes and small drops instead.
        if c.grounded {
            c.vertical_speed = c.vertical_speed.max(0.0);
        } else {
            let rising_and_held = c.vertical_speed > 0.0 && input.jump;
            let g = if rising_and_held {
                GRAVITY * JUMP_HOLD_GRAVITY
            } else if c.vertical_speed > 0.0 {
                GRAVITY
            } else {
                GRAVITY * FALL_GRAVITY
            };
            c.vertical_speed = (c.vertical_speed - g * dt).max(-MAX_FALL_SPEED);
        }

        // Standing on something that moves: go along with it.
        let mut carry = Vec3::ZERO;
        // (What's underfoot comes from a short ray down: steadier than the
        // controller's grounded flag, which flickers while standing still.)
        if let Some((col, last)) = c.ground {
            if let Some(body) = self.colliders.get(col).and_then(|k| k.parent()).and_then(|b| self.bodies.get(b)) {
                // A jump of more than a couple of studs in one step was the
                // ground being teleported, not something to ride along with.
                let moved = body.translation() - last;
                if moved.length() < 2.0 {
                    carry += moved;
                }
            }
            if let Some(t) = self.owners.get(&col).and_then(|id| self.parts.get(id)) {
                if t.synced.anchored && t.synced.velocity != BVec3::ZERO {
                    carry += to_glam(t.synced.velocity) * dt;
                }
            }
        }
        let desired = dir * player.walk_speed * dt + Vec3::Y * c.vertical_speed * dt + carry + c.push * dt;
        c.push *= (-(if c.grounded { 8.0 } else { 0.6 }) * dt).exp();
        if c.push.length_squared() < 0.01 {
            c.push = Vec3::ZERO;
        }

        let Some(body) = self.bodies.get(c.body) else { return };
        let pose = *body.position();
        let Some(collider) = self.colliders.get(c.collider) else { return };
        let shape = collider.shared_shape().clone();

        let dispatcher = self.narrow_phase.query_dispatcher();
        // Loose things resting on the character (above its waist) don't
        // block walking: a crate dropped on your head shouldn't pin you.
        let waist = pose.translation.y;
        let riders: HashSet<ColliderHandle> = self
            .colliders
            .iter()
            .filter(|(_, col)| {
                let dynamic = col.parent().and_then(|b| self.bodies.get(b)).is_some_and(|b| b.is_dynamic());
                dynamic && col.position().translation.y > waist
            })
            .map(|(h, _)| h)
            .collect();
        let not_rider = |h: ColliderHandle, _: &Collider| !riders.contains(&h);
        let filter = QueryFilter::default()
            .exclude_rigid_body(c.body)
            .exclude_sensors()
            .predicate(&not_rider);
        let mut collisions = Vec::new();
        let movement = {
            let queries =
                self.broad_phase
                    .as_query_pipeline(dispatcher, &self.bodies, &self.colliders, filter);
            c.controller
                .move_shape(dt, &queries, &*shape, &pose, desired, |hit| collisions.push(hit))
        };

        c.grounded = movement.grounded;
        c.speed = Vec3::new(movement.translation.x - carry.x, 0.0, movement.translation.z - carry.z).length() / dt;
        if c.grounded && c.vertical_speed < 0.0 {
            c.vertical_speed = 0.0;
        }
        // Bumped a ceiling: stop rising.
        if c.vertical_speed > 0.0 && movement.translation.y < desired.y * 0.5 {
            c.vertical_speed = 0.0;
        }

        // Walking into loose parts pushes them.
        {
            let mut queries = self.broad_phase.as_query_pipeline_mut(
                dispatcher,
                &mut self.bodies,
                &mut self.colliders,
                filter,
            );
            c.controller.solve_character_collision_impulses(
                dt,
                &mut queries,
                &*shape,
                CHARACTER_MASS,
                collisions.iter(),
            );
        }

        if let Some(body) = self.bodies.get_mut(c.body) {
            body.set_next_kinematic_translation(pose.translation + movement.translation);
        }

        // Remember what's underneath for next step's carrying.
        let under = {
            let queries = self.broad_phase.as_query_pipeline(dispatcher, &self.bodies, &self.colliders, filter);
            let ray = Ray::new(pose.translation + movement.translation, -Vec3::Y);
            queries.cast_ray(&ray, CHARACTER_HALF_HEIGHT + CHARACTER_RADIUS + 0.4, true).map(|(h, _)| h)
        };
        // (Other players are never ground: standing beside a teammate who
        // respawns mustn't drag you across the map with them.)
        let character_bodies: HashSet<RigidBodyHandle> = self.characters.values().map(|o| o.body).collect();
        let c = self.characters.get_mut(&id).unwrap();
        c.ground = under.and_then(|h| {
            let body = self.colliders.get(h)?.parent()?;
            if character_bodies.contains(&body) {
                return None;
            }
            Some((h, self.bodies.get(body)?.translation()))
        });
    }

    /// Parts the character just started touching, including ones it can
    /// pass through (like coins), as (part, player) pairs.
    fn character_touches(&mut self, world: &mut DataModel, id: InstanceId) -> Vec<(InstanceId, InstanceId)> {
        let Some(c) = self.characters.get_mut(&id) else { return Vec::new() };
        let Some(body) = self.bodies.get(c.body) else { return Vec::new() };

        // Write the character back so scripts and the renderer see it.
        let position = from_glam(body.translation());
        if let Some(p) = world.player_mut(c.id) {
            p.body.position = position;
            p.body.rotation = BVec3::new(0.0, c.yaw.to_degrees(), 0.0);
            p.speed = c.speed;
            p.airborne = !c.grounded;
        }
        c.synced_position = position;

        // A slightly bigger capsule, so standing on or brushing against
        // something counts as touching it.
        let probe = SharedShape::capsule_y(CHARACTER_HALF_HEIGHT + 0.1, CHARACTER_RADIUS + 0.1);
        let dispatcher = self.narrow_phase.query_dispatcher();
        let filter = QueryFilter::default().exclude_rigid_body(c.body);
        let queries = self
            .broad_phase
            .as_query_pipeline(dispatcher, &self.bodies, &self.colliders, filter);
        let now: std::collections::HashSet<InstanceId> = queries
            .intersect_shape(*body.position(), &*probe)
            .filter_map(|(handle, _)| self.owners.get(&handle).copied())
            .collect();

        let started: Vec<(InstanceId, InstanceId)> = now
            .difference(&c.touching)
            .map(|part| (*part, c.id))
            .collect();
        c.touching = now;
        started
    }

    /// Where the player's character is, if there is one.
    /// Throws a character: `velocity.y` up (or down), the rest sideways as
    /// momentum that wears off. What a script's `player.velocity = ...` does.
    pub fn launch(&mut self, id: InstanceId, velocity: Vec3) {
        if let Some(c) = self.characters.get_mut(&id) {
            c.vertical_speed = velocity.y;
            c.push = Vec3::new(velocity.x, 0.0, velocity.z);
            if velocity.y > 0.0 {
                c.grounded = false;
                c.coyote = 0.0;
            }
        }
    }

    /// Turns a character to face `yaw` (radians, 0 = +Z), the way it faces
    /// where it walks: a tool aimed with the mouse turns you toward the shot.
    pub fn face(&mut self, id: InstanceId, yaw: f32) {
        if let Some(c) = self.characters.get_mut(&id) {
            c.yaw = yaw;
        }
    }

    /// Keeps a character facing `yaw` (radians, 0 = +Z) whichever way it
    /// walks, or (None) lets it face where it walks again. Shift lock.
    pub fn hold_facing(&mut self, id: InstanceId, yaw: Option<f32>) {
        match yaw.filter(|y| y.is_finite()) {
            Some(y) => {
                self.held_yaw.insert(id, y);
                if let Some(c) = self.characters.get_mut(&id) {
                    c.yaw = y;
                }
            }
            None => {
                self.held_yaw.remove(&id);
            }
        }
    }

    /// How far into the next physics step the clock has got, 0 to 1:
    /// physics moves in fixed 1/60 s steps, so a smooth picture on a faster
    /// screen blends the last two steps by this much.
    pub fn step_fraction(&self) -> f32 {
        (self.accumulator / PHYSICS_DT).clamp(0.0, 1.0)
    }

    /// How many fixed steps have run so far.
    pub fn steps_run(&self) -> u64 {
        self.steps_run
    }

    /// Slides a character by `by` without disturbing its jump or fall (a
    /// small correction; a teleport, by contrast, stops it dead). Also moves
    /// it in `world`, so the next step doesn't take it for a teleport.
    pub fn nudge(&mut self, world: &mut DataModel, id: InstanceId, by: BVec3) {
        let Some(c) = self.characters.get_mut(&id) else { return };
        let Some(body) = self.bodies.get_mut(c.body) else { return };
        let to = body.translation() + to_glam(by);
        body.set_translation(to, true);
        let p = from_glam(to);
        c.synced_position = p;
        if let Some(player) = world.player_mut(id) {
            player.body.position = p;
        }
    }

    pub fn character_position(&self, id: InstanceId) -> Option<Vec3> {
        let c = self.characters.get(&id)?;
        self.bodies.get(c.body).map(|b| b.translation())
    }

    pub fn character_grounded(&self, id: InstanceId) -> bool {
        self.characters.get(&id).map(|c| c.grounded).unwrap_or(false)
    }

    /// How many parts physics is tracking.
    pub fn body_count(&self) -> usize {
        self.parts.len()
    }

    /// Copies script changes (and new or deleted parts) into physics.
    fn pull_from_world(&mut self, world: &DataModel, listeners: &HashSet<InstanceId>) {
        // Parts that no longer exist leave the simulation.
        let gone: Vec<InstanceId> = self
            .parts
            .keys()
            .filter(|id| world.part(**id).is_none())
            .copied()
            .collect();
        for id in gone {
            self.remove(id);
        }

        // Tree order, not HashMap order: the order bodies enter Rapier
        // affects the result, and the same scene should play out the same
        // way every time you press Play.
        for id in world.walk() {
            let Some(props) = world.part(id) else { continue };
            // A kart's parts aren't bodies of their own: the kart's box
            // stands for them all (see physics/kart.rs).
            if brixo_core::kart_of(world, id).is_some() {
                if self.parts.contains_key(&id) {
                    self.remove(id);
                }
                continue;
            }
            let listening = listeners.contains(&id);
            match self.parts.get(&id) {
                None => self.add(id, *props, listening),
                Some(tracked) => {
                    if tracked.synced != *props {
                        self.update(id, *props);
                    }
                    if tracked_listening(&self.parts, id) != listening {
                        self.set_listening(id, listening);
                    }
                }
            }
        }
    }

    /// Only parts someone is listening to report touches. Checking every
    /// pair of anchored parts (walls against floors...) would be wasted work.
    fn set_listening(&mut self, id: InstanceId, listening: bool) {
        let Some(tracked) = self.parts.get_mut(&id) else { return };
        tracked.listening = listening;
        if let Some(collider) = self.colliders.get_mut(tracked.collider) {
            let (events, types) = touch_settings(listening);
            collider.set_active_events(events);
            collider.set_active_collision_types(types);
        }
    }

    /// Copies where unanchored parts moved back into the world.
    fn push_to_world(&mut self, world: &mut DataModel) {
        for (id, tracked) in self.parts.iter_mut() {
            let Some(body) = self.bodies.get(tracked.body) else { continue };
            if !body.is_dynamic() || body.is_sleeping() {
                continue;
            }
            let Some(props) = world.part_mut(*id) else { continue };
            props.position = from_glam(body.translation());
            props.rotation = euler_degrees(*body.rotation());
            props.velocity = from_glam(body.linvel());
            tracked.synced = *props;
        }
    }

    fn add(&mut self, id: InstanceId, props: PartProps, listening: bool) {
        // Anchored parts are fixed bodies: that's what Rapier (and its
        // character controller) expects for static level geometry.
        let builder = if props.anchored {
            RigidBodyBuilder::fixed()
        } else {
            RigidBodyBuilder::dynamic()
        };
        let body = self.bodies.insert(
            builder
                .pose(pose_of(&props))
                .linvel(to_glam(props.velocity))
                .gravity_scale(if props.floating { 0.0 } else { 1.0 })
                // Small loose parts (pellets, paintballs) move further in one
                // step than they are wide: check their whole path, or they
                // slip through players and walls without touching them.
                .ccd_enabled(!props.anchored && props.size.x.min(props.size.y).min(props.size.z) < 1.0)
                .build(),
        );
        let collider = self.colliders.insert_with_parent(
            collider_for(&props, listening),
            body,
            &mut self.bodies,
        );
        self.owners.insert(collider, id);
        self.parts.insert(
            id,
            Tracked {
                body,
                collider,
                synced: props,
                listening,
            },
        );
    }

    fn remove(&mut self, id: InstanceId) {
        if let Some(tracked) = self.parts.remove(&id) {
            self.owners.remove(&tracked.collider);
            self.bodies.remove(
                tracked.body,
                &mut self.islands,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                true,
            );
        }
    }

    /// Applies whatever a script changed about a part.
    fn update(&mut self, id: InstanceId, props: PartProps) {
        let Some(tracked) = self.parts.get_mut(&id) else { return };
        let old = tracked.synced;
        tracked.synced = props;
        let (body_handle, old_collider, listening) = (tracked.body, tracked.collider, tracked.listening);

        // A new size or collision setting gets a brand-new collider. That
        // goes through the same path as a new part, so contacts and the
        // part's mass are recomputed properly.
        if old.size != props.size || old.can_collide != props.can_collide || old.shape != props.shape || old.bounce != props.bounce {
            self.owners.remove(&old_collider);
            self.colliders
                .remove(old_collider, &mut self.islands, &mut self.bodies, true);
            let collider = self.colliders.insert_with_parent(
                collider_for(&props, listening),
                body_handle,
                &mut self.bodies,
            );
            self.owners.insert(collider, id);
            if let Some(tracked) = self.parts.get_mut(&id) {
                tracked.collider = collider;
            }
        }

        let Some(body) = self.bodies.get_mut(body_handle) else { return };
        if old.anchored != props.anchored {
            let kind = if props.anchored {
                RigidBodyType::Fixed
            } else {
                RigidBodyType::Dynamic
            };
            body.set_body_type(kind, true);
        }

        if old.floating != props.floating {
            body.set_gravity_scale(if props.floating { 0.0 } else { 1.0 }, true);
        }
        // A script set a loose part's velocity: launch it.
        if old.velocity != props.velocity && body.is_dynamic() {
            body.set_linvel(to_glam(props.velocity), true);
        }
        let moved = old.position != props.position || old.rotation != props.rotation;
        if moved {
            // A teleport. Dynamic parts keep their velocity, like in Roblox.
            body.set_position(pose_of(&props), true);
        }
        body.wake_up(true);
        // A hinged part a script moved hangs from where it now is.
        if moved && self.hinges.contains_key(&id) {
            self.drop_hinge(id);
        }

        // Loose parts resting on or leaning against this one would otherwise
        // stay asleep, floating where it used to be.
        if moved || old.size != props.size || old.anchored != props.anchored {
            self.wake_neighbours(id);
        }
    }

    fn wake_neighbours(&mut self, id: InstanceId) {
        let Some(tracked) = self.parts.get(&id) else { return };
        let mine = tracked.collider;
        let others: Vec<ColliderHandle> = self
            .narrow_phase
            .contact_pairs_with(mine)
            .map(|pair| if pair.collider1 == mine { pair.collider2 } else { pair.collider1 })
            .collect();
        for collider in others {
            let parent = self.colliders.get(collider).and_then(|c| c.parent());
            if let Some(body) = parent.and_then(|b| self.bodies.get_mut(b)) {
                body.wake_up(true);
            }
        }
    }
}

fn tracked_listening(parts: &HashMap<InstanceId, Tracked>, id: InstanceId) -> bool {
    parts.get(&id).map(|t| t.listening).unwrap_or(false)
}

/// Listening parts report touches with anything, including other anchored
/// parts (a script may be moving one into the other). Rapier checks a pair
/// if either part asks, so this costs nothing for parts nobody listens to.
fn touch_settings(listening: bool) -> (ActiveEvents, ActiveCollisionTypes) {
    if listening {
        (ActiveEvents::COLLISION_EVENTS, ActiveCollisionTypes::all())
    } else {
        (ActiveEvents::empty(), ActiveCollisionTypes::default())
    }
}

/// The corners of a wedge filling a box of half-size (hx, hy, hz): a ramp
/// rising toward +Z. The renderer draws exactly this shape.
fn wedge_points(hx: f32, hy: f32, hz: f32) -> Vec<Vec3> {
    vec![
        Vec3::new(-hx, -hy, -hz),
        Vec3::new(hx, -hy, -hz),
        Vec3::new(hx, -hy, hz),
        Vec3::new(-hx, -hy, hz),
        Vec3::new(-hx, hy, hz),
        Vec3::new(hx, hy, hz),
    ]
}

fn collider_for(props: &PartProps, listening: bool) -> ColliderBuilder {
    let (events, types) = touch_settings(listening);
    let (hx, hy, hz) = (props.size.x / 2.0, props.size.y / 2.0, props.size.z / 2.0);
    let block = || ColliderBuilder::cuboid(hx, hy, hz);
    let shape = match props.shape {
        Shape::Block => block(),
        // Round shapes use the smallest side, so they never poke outside
        // their box.
        Shape::Ball => ColliderBuilder::ball(hx.min(hy).min(hz)),
        Shape::Cylinder => ColliderBuilder::cylinder(hy, hx.min(hz)),
        Shape::Wedge => ColliderBuilder::convex_hull(&wedge_points(hx, hy, hz)).unwrap_or_else(block),
    };
    shape
        .friction(0.6)
        // The bouncier of two touching things decides the bounce.
        .restitution(props.bounce.clamp(0.0, 1.0))
        .restitution_combine_rule(CoefficientCombineRule::Max)
        .density(1.0)
        .sensor(!props.can_collide)
        .active_events(events)
        .active_collision_types(types)
}

/// Sets a hinge's motor: swing to an angle and hold, keep turning, or
/// nothing (swings freely).
fn set_motor(data: &mut GenericJoint, (speed, swing_to): (f32, Option<f32>)) {
    match swing_to {
        Some(angle) => {
            data.set_motor_position(JointAxis::AngX, angle.to_radians(), MOTOR_STIFFNESS, MOTOR_DAMPING);
            data.set_motor_max_force(JointAxis::AngX, f32::MAX);
        }
        None if speed != 0.0 => {
            data.set_motor_velocity(JointAxis::AngX, speed.to_radians(), MOTOR_FACTOR);
            data.set_motor_max_force(JointAxis::AngX, f32::MAX);
        }
        // Free (the part's own damping slows it, see HINGE_DAMPING).
        None => {
            data.set_motor(JointAxis::AngX, 0.0, 0.0, 0.0, 0.0);
            data.set_motor_max_force(JointAxis::AngX, 0.0);
        }
    }
}

/// A part's box in the world (around it, however it's turned).
fn world_box(props: &PartProps) -> (Vec3, Vec3) {
    let q = rotation_of(props);
    let h = to_glam(props.size) / 2.0;
    let c = to_glam(props.position);
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for sx in [-1.0, 1.0] {
        for sy in [-1.0, 1.0] {
            for sz in [-1.0, 1.0] {
                let p = c + q * Vec3::new(sx * h.x, sy * h.y, sz * h.z);
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
    }
    (lo, hi)
}

fn to_glam(v: BVec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

fn from_glam(v: Vec3) -> BVec3 {
    BVec3::new(v.x, v.y, v.z)
}

/// Same rotation order as the renderer and the studio's gizmos.
fn rotation_of(props: &PartProps) -> Quat {
    Quat::from_euler(
        EulerRot::YXZ,
        props.rotation.y.to_radians(),
        props.rotation.x.to_radians(),
        props.rotation.z.to_radians(),
    )
}

fn pose_of(props: &PartProps) -> Pose {
    Pose::from_parts(to_glam(props.position), rotation_of(props))
}

fn euler_degrees(q: Quat) -> BVec3 {
    let (y, x, z) = q.to_euler(EulerRot::YXZ);
    BVec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
}
