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

use brixo_core::{DataModel, InstanceId, PartProps, Vec3 as BVec3};
use glam::{EulerRot, Quat, Vec3};
use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};
use rapier3d::prelude::*;

/// Physics runs at a fixed rate so it behaves the same on every machine.
pub const PHYSICS_DT: f32 = 1.0 / 60.0;
/// Downward acceleration in studs per second squared. With 1 stud at about
/// 0.28 m (Roblox's scale), this is Earth's gravity.
pub const GRAVITY: f32 = 35.0;
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
pub const MAX_FALL_SPEED: f32 = 70.0;

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
    /// Where we last put the character, to spot scripts teleporting it.
    synced_position: BVec3,
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
    steps_taken: u64,
    character: Option<Character>,
}

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
            steps_taken: 0,
            character: None,
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
        input: PlayerInput,
    ) -> Vec<(InstanceId, InstanceId)> {
        self.pull_from_world(world, listeners);
        self.sync_character(world);

        self.accumulator = (self.accumulator + dt.max(0.0)).min(MAX_CATCH_UP);
        let mut touches = Vec::new();
        while self.accumulator >= PHYSICS_DT {
            self.accumulator -= PHYSICS_DT;
            self.move_character(world, input);
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
        touches.extend(self.character_touches(world));
        touches
    }

    // --- the player's character ---

    /// Creates, removes or teleports the character to match the world.
    fn sync_character(&mut self, world: &DataModel) {
        let player = world
            .walk()
            .into_iter()
            .find(|id| world.player(*id).is_some());

        let same = matches!((&self.character, player), (Some(c), Some(p)) if c.id == p);
        if !same {
            if let Some(old) = self.character.take() {
                self.bodies.remove(
                    old.body,
                    &mut self.islands,
                    &mut self.colliders,
                    &mut self.impulse_joints,
                    &mut self.multibody_joints,
                    true,
                );
            }
            if let Some(id) = player {
                self.character = Some(self.new_character(id, world.player(id).unwrap().body.position));
            }
            return;
        }

        // A script (or a respawn) moved the player: teleport the capsule.
        let Some(c) = self.character.as_mut() else { return };
        let Some(p) = world.player(c.id) else { return };
        if p.body.position != c.synced_position {
            c.synced_position = p.body.position;
            c.vertical_speed = 0.0;
            if let Some(body) = self.bodies.get_mut(c.body) {
                body.set_translation(to_glam(p.body.position), true);
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
            synced_position: position,
            touching: Default::default(),
        }
    }

    /// One physics step of walking, jumping and falling.
    fn move_character(&mut self, world: &DataModel, input: PlayerInput) {
        let Some(c) = self.character.as_mut() else { return };
        let Some(player) = world.player(c.id) else { return };
        let dt = PHYSICS_DT;

        let mut dir = Vec3::new(input.move_x, 0.0, input.move_z);
        if dir.length_squared() > 1.0 {
            dir = dir.normalize();
        }
        if dir.length_squared() > 0.0001 {
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
            let g = if rising_and_held { GRAVITY * JUMP_HOLD_GRAVITY } else { GRAVITY };
            c.vertical_speed = (c.vertical_speed - g * dt).max(-MAX_FALL_SPEED);
        }

        let desired = dir * player.walk_speed * dt + Vec3::Y * c.vertical_speed * dt;

        let Some(body) = self.bodies.get(c.body) else { return };
        let pose = *body.position();
        let Some(collider) = self.colliders.get(c.collider) else { return };
        let shape = collider.shared_shape().clone();

        let dispatcher = self.narrow_phase.query_dispatcher();
        let filter = QueryFilter::default()
            .exclude_rigid_body(c.body)
            .exclude_sensors();
        let mut collisions = Vec::new();
        let movement = {
            let queries =
                self.broad_phase
                    .as_query_pipeline(dispatcher, &self.bodies, &self.colliders, filter);
            c.controller
                .move_shape(dt, &queries, &*shape, &pose, desired, |hit| collisions.push(hit))
        };

        c.grounded = movement.grounded;
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
    }

    /// Parts the character just started touching, including ones it can
    /// pass through (like coins), as (part, player) pairs.
    fn character_touches(&mut self, world: &mut DataModel) -> Vec<(InstanceId, InstanceId)> {
        let Some(c) = self.character.as_mut() else { return Vec::new() };
        let Some(body) = self.bodies.get(c.body) else { return Vec::new() };

        // Write the character back so scripts and the renderer see it.
        let position = from_glam(body.translation());
        if let Some(p) = world.player_mut(c.id) {
            p.body.position = position;
            p.body.rotation = BVec3::new(0.0, c.yaw.to_degrees(), 0.0);
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
    pub fn character_position(&self) -> Option<Vec3> {
        let c = self.character.as_ref()?;
        self.bodies.get(c.body).map(|b| b.translation())
    }

    pub fn character_grounded(&self) -> bool {
        self.character.as_ref().map(|c| c.grounded).unwrap_or(false)
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
        let body = self.bodies.insert(builder.pose(pose_of(&props)).build());
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
        if old.size != props.size || old.can_collide != props.can_collide {
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

        let moved = old.position != props.position || old.rotation != props.rotation;
        if moved {
            // A teleport. Dynamic parts keep their velocity, like in Roblox.
            body.set_position(pose_of(&props), true);
        }
        body.wake_up(true);

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

fn collider_for(props: &PartProps, listening: bool) -> ColliderBuilder {
    let (events, types) = touch_settings(listening);
    ColliderBuilder::cuboid(props.size.x / 2.0, props.size.y / 2.0, props.size.z / 2.0)
        .friction(0.6)
        .restitution(0.0)
        .density(1.0)
        .sensor(!props.can_collide)
        .active_events(events)
        .active_collision_types(types)
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
