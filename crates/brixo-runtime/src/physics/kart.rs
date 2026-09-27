//! Arcade karts: the engine drives each kart's chassis like the character
//! controller drives a player: a kinematic box slid through the world by
//! Rapier's character controller (so it climbs ramps, slides along walls
//! and lands on the ground), with its own tuned speed, grip, drifting and
//! boosts. Tight and predictable, never flips; tuned for fun, not realism.

use std::collections::{HashMap, HashSet};

use brixo_core::{is_kart, kart_chassis, Attribute, DataModel, InstanceId};
use glam::{Quat, Vec3};
use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};
use rapier3d::prelude::*;

use super::{Physics, GRAVITY, PHYSICS_DT};
use crate::kart::{bv, euler_of, quat_of, record_offsets, v, KartInput};

/// Fastest a kart goes without a boost, in studs a second (`top_speed`
/// on the kart changes it).
pub const TOP_SPEED: f32 = 70.0;
const ACCEL: f32 = 42.0;
const BRAKE: f32 = 110.0;
const COAST: f32 = 16.0;
const REVERSE_TOP: f32 = 22.0;
/// How fast it turns at speed (radians a second).
const TURN: f32 = 2.3;
/// Sideways grip: how fast a sideways slide dies away (per second).
const GRIP: f32 = 11.0;
const DRIFT_GRIP: f32 = 2.4;
const HOP: f32 = 11.0;
/// Seconds of drifting for a mini-boost, and for a big one.
pub const DRIFT_MINI: f32 = 0.8;
pub const DRIFT_SUPER: f32 = 1.7;
const BOOST_SPEED: f32 = 1.4;
const SPIN_TIME: f32 = 1.1;

/// What scripts ask of karts, carried out at the start of the next step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KartCommand {
    /// Faster for this many seconds.
    Boost(InstanceId, f32),
    /// Spins out: a second of spinning, slowed right down.
    SpinOut(InstanceId),
    /// Picks the kart up and puts it here, facing `yaw` degrees, stopped.
    Place(InstanceId, Vec3, f32),
}

pub(crate) struct Kart {
    chassis: InstanceId,
    body: RigidBodyHandle,
    collider: ColliderHandle,
    controller: KinematicCharacterController,
    /// The box the kart fills: its middle (in the chassis's own space) and
    /// half-size.
    middle: Vec3,
    half: Vec3,
    pub(crate) yaw: f32,
    /// Moving along the ground (horizontal), and up/down.
    pub(crate) velocity: Vec3,
    vy: f32,
    grounded: bool,
    /// Climbing (or dropping) with the ground last step: kept as upward
    /// speed on leaving it, so a ramp throws you.
    climb: f32,
    /// Drifting: -1 or 1 (the way it slides), or 0.
    drift: f32,
    charge: f32,
    drift_was_held: bool,
    boost: f32,
    spin: f32,
    odo: f32,
    steer_shown: f32,
    pitch: f32,
    roll: f32,
    /// Where we last put the chassis: if a script moved it, it's a teleport.
    placed: brixo_core::Vec3,
}

/// A kart's box: the parts' extent in the chassis's own space.
fn kart_box(world: &DataModel, kart: InstanceId, chassis: InstanceId) -> (Vec3, Vec3) {
    let c = world.part(chassis).unwrap();
    let (cp, inv) = (v(c.position), quat_of(c.rotation).inverse());
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for id in world.parts_under(kart) {
        let Some(p) = world.part(id) else { continue };
        let (at, q, h) = (inv * (v(p.position) - cp), inv * quat_of(p.rotation), v(p.size) / 2.0);
        for sx in [-1.0, 1.0] {
            for sy in [-1.0, 1.0] {
                for sz in [-1.0, 1.0] {
                    let corner = at + q * Vec3::new(sx * h.x, sy * h.y, sz * h.z);
                    lo = lo.min(corner);
                    hi = hi.max(corner);
                }
            }
        }
    }
    ((lo + hi) / 2.0, ((hi - lo) / 2.0).max(Vec3::splat(0.3)))
}

impl Physics {
    /// Players who got out of their kart (a script set `kart` to nil, or
    /// the kart went): out beside it, walking again.
    pub(crate) fn release_drivers(&mut self, world: &mut DataModel) {
        let seated: Vec<(InstanceId, InstanceId)> = self.seated.iter().map(|(p, k)| (*p, *k)).collect();
        for (player, kart) in seated {
            let still = world.player(player).and_then(|p| p.kart) == Some(kart) && is_kart(world, kart);
            if still {
                continue;
            }
            self.seated.remove(&player);
            let dead = world.player(player).is_none_or(|p| p.dead > 0.0);
            if dead {
                continue;
            }
            let beside = self.kart_state(kart).map(|(at, yaw, _)| at + Vec3::new(yaw.cos(), 0.0, -yaw.sin()) * 3.5 + Vec3::Y * 2.5);
            if let (Some(to), Some(p)) = (beside, world.player_mut(player)) {
                p.body.position = bv(to);
            }
        }
        for id in world.walk() {
            if let Some(k) = world.player(id).and_then(|p| p.kart) {
                if is_kart(world, k) {
                    self.seated.insert(id, k);
                }
            }
        }
    }

    /// Makes the karts match the world: new ones get a body, gone ones lose
    /// it, and a chassis a script moved is a teleport.
    pub(crate) fn sync_karts(&mut self, world: &mut DataModel) {
        let karts: Vec<InstanceId> = world.walk().into_iter().filter(|id| is_kart(world, *id)).collect();
        let now: HashSet<InstanceId> = karts.iter().copied().collect();
        let gone: Vec<InstanceId> = self.karts.keys().filter(|k| !now.contains(k)).copied().collect();
        for k in gone {
            let kart = self.karts.remove(&k).unwrap();
            self.owners.remove(&kart.collider);
            self.bodies.remove(kart.body, &mut self.islands, &mut self.colliders, &mut self.impulse_joints, &mut self.multibody_joints, true);
        }
        for k in karts {
            let Some(chassis) = kart_chassis(world, k) else { continue };
            let Some(c) = world.part(chassis).copied() else { continue };
            if !self.karts.contains_key(&k) {
                record_offsets(world, k);
                let (middle, half) = kart_box(world, k, chassis);
                let yaw = c.rotation.y.to_radians();
                let at = v(c.position) + Quat::from_rotation_y(yaw) * middle;
                let body = self.bodies.insert(RigidBodyBuilder::kinematic_position_based().pose(Pose::from_parts(at, Quat::from_rotation_y(yaw))).build());
                // Touches: the kart's box stands for all its parts, as its chassis.
                let collider = self.colliders.insert_with_parent(
                    ColliderBuilder::cuboid(half.x, half.y, half.z)
                        .friction(0.0)
                        .active_events(ActiveEvents::COLLISION_EVENTS)
                        .active_collision_types(ActiveCollisionTypes::all()),
                    body,
                    &mut self.bodies,
                );
                self.owners.insert(collider, chassis);
                let controller = KinematicCharacterController {
                    offset: CharacterLength::Absolute(0.04),
                    autostep: Some(CharacterAutostep {
                        max_height: CharacterLength::Absolute(0.5),
                        min_width: CharacterLength::Absolute(0.3),
                        include_dynamic_bodies: false,
                    }),
                    snap_to_ground: Some(CharacterLength::Absolute(0.6)),
                    max_slope_climb_angle: 42f32.to_radians(),
                    min_slope_slide_angle: 50f32.to_radians(),
                    ..KinematicCharacterController::default()
                };
                self.karts.insert(
                    k,
                    Kart {
                        chassis,
                        body,
                        collider,
                        controller,
                        middle,
                        half,
                        yaw,
                        velocity: Vec3::ZERO,
                        vy: 0.0,
                        grounded: false,
                        climb: 0.0,
                        drift: 0.0,
                        charge: 0.0,
                        drift_was_held: false,
                        boost: 0.0,
                        spin: 0.0,
                        odo: 0.0,
                        steer_shown: 0.0,
                        pitch: 0.0,
                        roll: 0.0,
                        placed: c.position,
                    },
                );
            } else {
                let kart = self.karts.get_mut(&k).unwrap();
                if kart.chassis != chassis || c.position != kart.placed {
                    // A script moved it (or swapped its chassis): put it there.
                    kart.chassis = chassis;
                    let yaw = c.rotation.y.to_radians();
                    let at = v(c.position) + Quat::from_rotation_y(yaw) * kart.middle;
                    kart.yaw = yaw;
                    kart.velocity = Vec3::ZERO;
                    kart.vy = 0.0;
                    kart.placed = c.position;
                    if let Some(b) = self.bodies.get_mut(kart.body) {
                        b.set_position(Pose::from_parts(at, Quat::from_rotation_y(yaw)), true);
                    }
                }
            }
        }
    }

    /// Carries out what scripts asked.
    pub fn kart_command(&mut self, command: KartCommand) {
        match command {
            KartCommand::Boost(k, seconds) => {
                if let Some(kart) = self.karts.get_mut(&k) {
                    kart.boost = kart.boost.max(seconds.clamp(0.0, 10.0));
                }
            }
            KartCommand::SpinOut(k) => {
                if let Some(kart) = self.karts.get_mut(&k) {
                    kart.spin = SPIN_TIME;
                    kart.drift = 0.0;
                    kart.charge = 0.0;
                    kart.boost = 0.0;
                }
            }
            KartCommand::Place(k, at, yaw) => {
                if let Some(kart) = self.karts.get_mut(&k) {
                    kart.yaw = yaw.to_radians();
                    kart.velocity = Vec3::ZERO;
                    kart.vy = 0.0;
                    kart.spin = 0.0;
                    kart.drift = 0.0;
                    let pose = Pose::from_parts(at + Quat::from_rotation_y(kart.yaw) * kart.middle, Quat::from_rotation_y(kart.yaw));
                    if let Some(b) = self.bodies.get_mut(kart.body) {
                        b.set_position(pose, true);
                    }
                }
            }
        }
    }

    /// Sets what drives each kart this step (players' keys, bots' AI).
    pub fn set_kart_inputs(&mut self, inputs: HashMap<InstanceId, KartInput>) {
        self.kart_inputs = inputs;
    }

    /// A kart's boost and spin-out, in seconds left.
    pub fn kart_timers(&self, kart: InstanceId) -> Option<(f32, f32)> {
        self.karts.get(&kart).map(|k| (k.boost, k.spin))
    }

    /// Where a kart is and which way it faces (its chassis), and its speed.
    pub fn kart_state(&self, kart: InstanceId) -> Option<(Vec3, f32, f32)> {
        let k = self.karts.get(&kart)?;
        let b = self.bodies.get(k.body)?;
        Some((b.translation() - Quat::from_rotation_y(k.yaw) * k.middle, k.yaw, k.velocity.length()))
    }

    /// Nudges a kart (prediction easing out drift from the server).
    pub fn nudge_kart(&mut self, kart: InstanceId, by: Vec3, yaw_by: f32) {
        let Some(k) = self.karts.get_mut(&kart) else { return };
        k.yaw += yaw_by;
        if let Some(b) = self.bodies.get_mut(k.body) {
            let t = b.translation() + by;
            b.set_position(Pose::from_parts(t, Quat::from_rotation_y(k.yaw)), true);
        }
    }

    /// One physics step of every kart.
    pub(crate) fn move_karts(&mut self, world: &DataModel) {
        let dt = PHYSICS_DT;
        let ids: Vec<InstanceId> = self.karts.keys().copied().collect();
        let character_bodies: HashSet<RigidBodyHandle> = self.characters.values().map(|c| c.body).collect();
        for id in ids {
            let input = self.kart_inputs.get(&id).copied().unwrap_or_default();
            let locked = matches!(world.get(id).and_then(|i| i.attributes.get("locked")), Some(Attribute::Bool(true)));
            let top = match world.get(id).and_then(|i| i.attributes.get("top_speed")) {
                Some(Attribute::Num(n)) => (*n as f32).clamp(5.0, 200.0),
                _ => TOP_SPEED,
            };
            let k = self.karts.get_mut(&id).unwrap();
            let forward = Vec3::new(k.yaw.sin(), 0.0, k.yaw.cos());
            let mut along = k.velocity.dot(forward);
            let mut side = k.velocity - forward * along;
            let input = if locked || k.spin > 0.0 { KartInput::default() } else { input };

            if k.spin > 0.0 {
                k.spin -= dt;
                k.yaw += 13.0 * dt;
                along *= (-3.0 * dt).exp();
                side *= (-3.0 * dt).exp();
            } else if k.grounded {
                // Throttle, brakes and reverse.
                let boosting = k.boost > 0.0;
                let top_now = if boosting { top * BOOST_SPEED } else { top };
                if boosting {
                    along = (along + ACCEL * 3.0 * dt).min(top_now);
                } else if input.throttle > 0.0 {
                    // (Over the top speed, after a boost: eased down below, not cut.)
                    along = if along < 0.0 { along + BRAKE * dt } else if along < top_now { (along + ACCEL * dt).min(top_now) } else { along };
                } else if input.throttle < 0.0 {
                    along = if along > 0.0 { along - BRAKE * dt } else { (along - ACCEL * 0.7 * dt).max(-REVERSE_TOP) };
                } else {
                    along = if along > 0.0 { (along - COAST * dt).max(0.0) } else { (along + COAST * dt).min(0.0) };
                }
                if along > top_now {
                    along = (along - 40.0 * dt).max(top_now);
                }
                // Drifting: a hop on pressing, a slide while held.
                if input.drift && !k.drift_was_held && input.steer != 0.0 && along > 25.0 {
                    k.drift = input.steer.signum();
                    k.charge = 0.0;
                    k.vy = HOP;
                    k.grounded = false;
                }
                if k.drift != 0.0 && (!input.drift || along < 15.0) {
                    // Let go: a boost, if it was a good drift.
                    if along >= 15.0 {
                        k.boost = k.boost.max(if k.charge >= DRIFT_SUPER { 1.3 } else if k.charge >= DRIFT_MINI { 0.6 } else { 0.0 });
                    }
                    k.drift = 0.0;
                    k.charge = 0.0;
                }
                let grip = if k.drift != 0.0 { DRIFT_GRIP } else { GRIP };
                side *= (-grip * dt).exp();
                // Turning: none standing still, sharpest at middling speed.
                let pace = (along.abs() / 18.0).min(1.0) * (1.0 - 0.25 * (along.abs() / top).min(1.0));
                let turn = if k.drift != 0.0 {
                    k.charge += dt * (1.0 + 0.6 * (input.steer * k.drift).max(0.0));
                    // Drifting turns the drift's way, tighter or wider with the stick.
                    k.drift * (1.5 + 0.9 * input.steer * k.drift) * pace
                } else {
                    input.steer * TURN * pace * along.signum()
                };
                k.yaw += turn * dt;
            }
            k.drift_was_held = input.drift;
            k.boost = (k.boost - dt).max(0.0);
            k.steer_shown += (input.steer - k.steer_shown) * (10.0 * dt).min(1.0);
            // On the ground, turning re-aims the forward part of the motion
            // (grip); spinning out or in the air, it carries on as it was.
            if k.spin > 0.0 {
                k.velocity = forward * along + side;
            } else if k.grounded {
                let nose = Vec3::new(k.yaw.sin(), 0.0, k.yaw.cos());
                k.velocity = nose * along + side;
            }
            let forward = Vec3::new(k.yaw.sin(), 0.0, k.yaw.cos());
            if !k.grounded {
                k.vy -= GRAVITY * 0.85 * dt;
            }

            // Move the box.
            let Some(body) = self.bodies.get(k.body) else { continue };
            let pose = Pose::from_parts(body.translation(), Quat::from_rotation_y(k.yaw));
            let shape = SharedShape::cuboid(k.half.x, k.half.y, k.half.z);
            let desired = k.velocity * dt + Vec3::Y * k.vy * dt;
            let own = k.body;
            let drivers: HashSet<RigidBodyHandle> = character_bodies.clone();
            let skip = |h: ColliderHandle, c: &Collider| {
                let _ = h;
                !c.parent().is_some_and(|b| drivers.contains(&b))
            };
            let filter = QueryFilter::default().exclude_rigid_body(own).exclude_sensors().predicate(&skip);
            let dispatcher = self.narrow_phase.query_dispatcher();
            let movement = {
                let queries = self.broad_phase.as_query_pipeline(dispatcher, &self.bodies, &self.colliders, filter);
                k.controller.move_shape(dt, &queries, &*shape, &pose, desired, |_| {})
            };
            let was_grounded = k.grounded;
            k.grounded = movement.grounded;
            if k.grounded {
                // Following the ground: remember how fast it rises, so
                // leaving a ramp's lip throws the kart up.
                k.climb = (movement.translation.y / dt).clamp(-30.0, 40.0);
                if k.vy <= 0.0 {
                    k.vy = 0.0;
                }
            } else if was_grounded && k.vy <= 0.0 {
                k.vy = k.climb.max(0.0);
            }
            // A wall: the box went less far than it meant to. Keep what it
            // really did (sliding along it), and lose a little.
            let flat_meant = Vec3::new(desired.x, 0.0, desired.z);
            let flat_did = Vec3::new(movement.translation.x, 0.0, movement.translation.z);
            if flat_meant.length() > 0.001 && flat_did.length() < flat_meant.length() * 0.85 {
                k.velocity = flat_did / dt * 0.9;
            }
            k.odo += flat_did.dot(forward);
            let next = pose.translation + movement.translation;

            // Tilt with the ground under it (only for show).
            let (mut pitch, mut roll) = (0.0, 0.0);
            {
                let queries = self.broad_phase.as_query_pipeline(dispatcher, &self.bodies, &self.colliders, filter);
                let ray = Ray::new(next, -Vec3::Y);
                if let Some((_, hit)) = queries.cast_ray_and_get_normal(&ray, k.half.y + 2.5, true) {
                    let n = hit.normal;
                    let right = Vec3::new(k.yaw.cos(), 0.0, -k.yaw.sin());
                    pitch = -(n.dot(forward)).atan2(n.y).clamp(-0.7, 0.7);
                    roll = -(n.dot(right)).atan2(n.y).clamp(-0.7, 0.7);
                    if !k.grounded {
                        pitch *= 0.5;
                        roll *= 0.5;
                    }
                }
            }
            let ease = (12.0 * dt).min(1.0);
            k.pitch += (pitch - k.pitch) * ease;
            k.roll += (roll - k.roll) * ease;
            if let Some(b) = self.bodies.get_mut(k.body) {
                b.set_next_kinematic_position(Pose::from_parts(next, Quat::from_rotation_y(k.yaw)));
            }
        }
    }

    /// Writes the karts into the world: chassis pose, what they're doing
    /// (for scripts and players), their parts, and their drivers.
    pub(crate) fn push_karts(&mut self, world: &mut DataModel) {
        let drivers: Vec<(InstanceId, InstanceId)> = world
            .walk()
            .into_iter()
            .filter_map(|id| Some((id, world.player(id)?.kart?)))
            .collect();
        let ids: Vec<InstanceId> = self.karts.keys().copied().collect();
        for id in ids {
            let k = self.karts.get_mut(&id).unwrap();
            let Some(body) = self.bodies.get(k.body) else { continue };
            // (The body's own next position, if it has one this step.)
            let at = body.next_position().translation;
            let q = Quat::from_rotation_y(k.yaw) * Quat::from_rotation_x(k.pitch) * Quat::from_rotation_z(k.roll);
            let chassis_at = at - Quat::from_rotation_y(k.yaw) * k.middle;
            if let Some(p) = world.part_mut(k.chassis) {
                p.position = bv(chassis_at);
                p.rotation = euler_of(q);
                k.placed = p.position;
            }
            let speed = k.velocity.length();
            let drift_level = if k.drift == 0.0 { 0.0 } else if k.charge >= DRIFT_SUPER { 2.0 } else if k.charge >= DRIFT_MINI { 1.0 } else { 0.5 };
            let facts = [
                ("speed", Attribute::Num((speed as f64 * 10.0).round() / 10.0)),
                ("steer", Attribute::Num((k.steer_shown as f64 * 100.0).round() / 100.0)),
                ("odo", Attribute::Num(((k.odo as f64) * 100.0).round() / 100.0)),
                ("drift", Attribute::Num(drift_level)),
                ("boosting", Attribute::Bool(k.boost > 0.0)),
                ("spinning", Attribute::Bool(k.spin > 0.0)),
                // How long they've got left (so a player's predicted kart
                // boosts and spins when a script makes the server's do).
                ("boost_left", Attribute::Num((k.boost as f64 * 20.0).round() / 20.0)),
                ("spin_left", Attribute::Num((k.spin as f64 * 20.0).round() / 20.0)),
            ];
            if let Some(inst) = world.get_mut(id) {
                for (key, value) in facts {
                    inst.attributes.insert(key.into(), value);
                }
            }
            crate::kart::pose_parts(world, id);
            // The driver sits in the seat.
            for (player, kart) in &drivers {
                if *kart != id {
                    continue;
                }
                let seat = world
                    .get(id)
                    .and_then(|m| m.children.iter().copied().find(|c| world.get(*c).is_some_and(|i| i.name == "Seat")))
                    .and_then(|s| world.part(s).copied());
                let (sp, sh) = match seat {
                    Some(s) => (v(s.position), s.size.y / 2.0),
                    None => (chassis_at, 0.5),
                };
                let up = q * Vec3::Y;
                let sit = sp + up * (sh + 1.55);
                if let Some(p) = world.player_mut(*player) {
                    p.body.position = bv(sit);
                    p.body.rotation = brixo_core::Vec3::new(0.0, k.yaw.to_degrees(), 0.0);
                    p.speed = 0.0;
                    p.airborne = false;
                }
                if let Some(c) = self.characters.get_mut(player) {
                    c.synced_position = bv(sit);
                    c.yaw = k.yaw;
                    if let Some(b) = self.bodies.get_mut(c.body) {
                        b.set_translation(sit, true);
                    }
                }
            }
        }
    }
}
