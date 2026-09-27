//! Karts: what's shared between the server's engine and every player's
//! copy of the world.
//!
//! A kart is a Model with `kart = true` (see `brixo_core::is_kart`). The
//! engine drives its chassis (physics/kart.rs); every other part of it is
//! posed from the chassis by `pose_parts`, from offsets recorded once as
//! custom fields on each part. That's why the server only needs to send the
//! chassis: players pose the rest themselves, wheels spinning and steering.
//!
//! What a kart does each moment is on the Model, for scripts and players:
//! `speed` (studs/s), `steer` (-1 right to 1 left), `odo` (studs driven,
//! turning the wheels), `drift` (0 none, 1 and 2 for charged mini-boosts)
//! and `boosting`.

use brixo_core::{kart_chassis, is_kart, Attribute, DataModel, InstanceId, Vec3 as BVec3, KART_AT, KART_Q};
use glam::{EulerRot, Quat, Vec3};

/// What drives a kart this moment.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct KartInput {
    /// 1 full throttle, -1 brake and reverse.
    pub throttle: f32,
    /// 1 hard left, -1 hard right.
    pub steer: f32,
    /// Held to drift (a hop, then a slide; letting go gives a boost).
    pub drift: bool,
}

impl KartInput {
    /// A driving player's keys. In a kart, a player's input is local, not
    /// a direction in the world: `move_z` is throttle (W is 1, S is -1),
    /// `move_x` is steering (A is 1, D is -1), jump is drift. Read as keys,
    /// so a diagonal (shortened on its way in) is still full throttle.
    pub fn from_player(input: crate::PlayerInput) -> KartInput {
        let key = |v: f32| if v > 0.3 { 1.0 } else if v < -0.3 { -1.0 } else { 0.0 };
        KartInput { throttle: key(input.move_z), steer: key(input.move_x), drift: input.jump }
    }
}

pub(crate) fn quat_of(rotation: BVec3) -> Quat {
    Quat::from_euler(EulerRot::YXZ, rotation.y.to_radians(), rotation.x.to_radians(), rotation.z.to_radians())
}

pub(crate) fn euler_of(q: Quat) -> BVec3 {
    let (y, x, z) = q.to_euler(EulerRot::YXZ);
    BVec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
}

pub(crate) fn v(p: BVec3) -> Vec3 {
    Vec3::new(p.x, p.y, p.z)
}

pub(crate) fn bv(p: Vec3) -> BVec3 {
    BVec3::new(p.x, p.y, p.z)
}

fn nums(text: &str) -> Vec<f32> {
    text.split(',').filter_map(|n| n.trim().parse().ok()).collect()
}

/// Every kart in the world.
pub fn karts(world: &DataModel) -> Vec<InstanceId> {
    world.walk().into_iter().filter(|id| is_kart(world, *id)).collect()
}

/// Records where each part of a kart sits on its chassis, the first time
/// (as custom fields, so it travels to every player with the parts).
pub fn record_offsets(world: &mut DataModel, kart: InstanceId) {
    let Some(chassis) = kart_chassis(world, kart) else { return };
    let Some(c) = world.part(chassis).copied() else { return };
    let (cp, cq) = (v(c.position), quat_of(c.rotation));
    let inv = cq.inverse();
    let parts: Vec<InstanceId> = world.parts_under(kart).into_iter().filter(|p| *p != chassis).collect();
    for id in parts {
        if world.get(id).is_some_and(|i| i.attributes.contains_key(KART_AT)) {
            continue;
        }
        let Some(p) = world.part(id).copied() else { continue };
        let at = inv * (v(p.position) - cp);
        let q = inv * quat_of(p.rotation);
        let inst = world.get_mut(id).unwrap();
        inst.attributes.insert(KART_AT.into(), Attribute::Str(format!("{:.3},{:.3},{:.3}", at.x, at.y, at.z)));
        inst.attributes.insert(KART_Q.into(), Attribute::Str(format!("{:.5},{:.5},{:.5},{:.5}", q.x, q.y, q.z, q.w)));
    }
}

fn num(world: &DataModel, id: InstanceId, key: &str) -> f32 {
    match world.get(id).and_then(|i| i.attributes.get(key)) {
        Some(Attribute::Num(n)) => *n as f32,
        _ => 0.0,
    }
}

/// Poses a kart's parts from its chassis: each where it was built on it,
/// wheels (parts named "Wheel...") turning with the distance driven, and
/// front wheels ("Front Wheel...") steering.
pub fn pose_parts(world: &mut DataModel, kart: InstanceId) {
    let Some(chassis) = kart_chassis(world, kart) else { return };
    let Some(c) = world.part(chassis).copied() else { return };
    let (cp, cq) = (v(c.position), quat_of(c.rotation));
    let odo = num(world, kart, "odo");
    let steer = num(world, kart, "steer");
    let parts: Vec<InstanceId> = world.parts_under(kart).into_iter().filter(|p| *p != chassis).collect();
    for id in parts {
        let Some(inst) = world.get(id) else { continue };
        let (Some(Attribute::Str(at)), Some(Attribute::Str(q))) = (inst.attributes.get(KART_AT), inst.attributes.get(KART_Q)) else { continue };
        let (at, q) = (nums(at), nums(q));
        if at.len() != 3 || q.len() != 4 {
            continue;
        }
        let name = inst.name.clone();
        let local_at = Vec3::new(at[0], at[1], at[2]);
        let mut local_q = Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize();
        let Some(p) = world.part_mut(id) else { continue };
        if name.starts_with("Wheel") || name.starts_with("Front Wheel") {
            let radius = (p.size.x.max(p.size.y).max(p.size.z) / 2.0).max(0.2);
            local_q = Quat::from_rotation_x(odo / radius) * local_q;
            if name.starts_with("Front Wheel") {
                local_q = Quat::from_rotation_y(steer * 0.45) * local_q;
            }
        }
        p.position = bv(cp + cq * local_at);
        p.rotation = euler_of(cq * local_q);
    }
}

/// Poses every kart's parts (players do this each frame, after the world
/// update and smoothing).
pub fn pose_all(world: &mut DataModel) {
    for k in karts(world) {
        pose_parts(world, k);
    }
}

/// Whether a part is one the engine poses from its chassis (the server
/// doesn't send these: each player poses them).
pub fn is_posed_part(world: &DataModel, id: InstanceId) -> bool {
    world.get(id).is_some_and(|i| i.attributes.contains_key(KART_AT))
}

/// A bot's driving: along the racing line (the Workspace's `racing_line`,
/// a Folder of parts named 1, 2, 3... in order around the track), aiming
/// a few points ahead, `lane` studs to the side.
pub fn bot_input(line: &[Vec3], at: Vec3, yaw: f32, speed: f32, lane: f32, next: &mut usize) -> KartInput {
    if line.len() < 2 {
        return KartInput::default();
    }
    let n = line.len();
    // Far from where we were heading (put back on the grid, or at a
    // checkpoint): start again from the nearest point.
    if line[*next % n].distance(at) > 60.0 {
        *next = (0..n).min_by(|a, b| line[*a].distance_squared(at).total_cmp(&line[*b].distance_squared(at))).unwrap_or(0);
    }
    // Move on past points we've reached (or passed).
    for _ in 0..n {
        let p = line[*next % n];
        let q = line[(*next + 1) % n];
        let along = (q - p).normalize_or_zero();
        if (at - p).dot(along) > -2.0 || at.distance(p) < 10.0 {
            *next = (*next + 1) % n;
        } else {
            break;
        }
    }
    let ahead = (1.0 + speed / 25.0).round() as usize;
    let i = (*next + ahead) % n;
    let p = line[i];
    let q = line[(i + 1) % n];
    let along = (q - p).normalize_or_zero();
    let side = Vec3::new(along.z, 0.0, -along.x);
    let target = p + side * lane;
    let to = target - at;
    let want = to.x.atan2(to.z);
    let mut diff = want - yaw;
    while diff > std::f32::consts::PI {
        diff -= std::f32::consts::TAU;
    }
    while diff < -std::f32::consts::PI {
        diff += std::f32::consts::TAU;
    }
    let steer = (diff * 2.5).clamp(-1.0, 1.0);
    // Slowing for the bends coming up: each bend's tightness sets how fast
    // it can be taken (a kart turns at most about 1.7 radians a second),
    // and the brakes go on in time to get down to that.
    let mut throttle = 1.0;
    for k in 1..n.min(20) {
        let a = line[(*next + k - 1) % n];
        let b = line[(*next + k) % n];
        let c = line[(*next + k + 1) % n];
        let (u, w) = ((b - a).normalize_or_zero(), (c - b).normalize_or_zero());
        let angle = u.dot(w).clamp(-1.0, 1.0).acos();
        let gap = b.distance(at);
        if gap > 30.0 + speed * speed / 120.0 {
            break;
        }
        if angle < 0.01 {
            continue;
        }
        let radius = c.distance(b).max(a.distance(b)) / angle;
        let ok = (radius * 1.6).clamp(30.0, 200.0);
        if speed > ok && gap < (speed * speed - ok * ok) / 140.0 + 8.0 {
            throttle = if speed > ok + 6.0 { -1.0 } else { 0.0 };
            break;
        }
    }
    if diff.abs() > 1.2 && speed > 30.0 {
        throttle = -1.0;
    }
    KartInput { throttle, steer, drift: false }
}

/// A bot's driving, from one moment to the next: where it's got to on the
/// racing line, and getting unstuck (backing up, turned the other way,
/// when it's been up against something for a second).
#[derive(Debug, Clone, Copy, Default)]
pub struct BotDriver {
    pub next: usize,
    stuck: f32,
    reverse: f32,
    /// Going round a kart in the way: how far to the side, and how long for.
    dodge: f32,
    dodge_time: f32,
}

impl BotDriver {
    /// `others`: where the other karts are.
    pub fn drive(&mut self, line: &[Vec3], at: Vec3, yaw: f32, speed: f32, lane: f32, others: &[Vec3], dt: f32) -> KartInput {
        let ahead = Vec3::new(yaw.sin(), 0.0, yaw.cos());
        let left = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
        self.dodge_time -= dt;
        if self.dodge_time <= 0.0 {
            self.dodge = 0.0;
            let reach = 10.0 + speed * 0.6;
            for o in others {
                let d = *o - at;
                let (fwd, side) = (d.dot(ahead), d.dot(left));
                if fwd > 0.0 && fwd < reach && side.abs() < 5.5 && d.y.abs() < 4.0 {
                    self.dodge = if side > 0.0 { -8.0 } else { 8.0 };
                    self.dodge_time = 0.7;
                    break;
                }
            }
        }
        let input = bot_input(line, at, yaw, speed, (lane + self.dodge).clamp(-10.0, 10.0), &mut self.next);
        if self.reverse > 0.0 {
            self.reverse -= dt;
            return KartInput { throttle: -1.0, steer: -input.steer.signum(), drift: false };
        }
        if input.throttle > 0.0 && speed < 3.0 {
            self.stuck += dt;
            if self.stuck > 1.0 {
                self.stuck = 0.0;
                self.reverse = 0.9;
            }
        } else {
            self.stuck = 0.0;
        }
        input
    }
}

/// The racing line: the Workspace's `racing_line` Folder's parts, 1, 2, 3...
pub fn racing_line(world: &DataModel) -> Vec<Vec3> {
    let Some(Attribute::Str(name)) = world.get(world.root()).and_then(|w| w.attributes.get("racing_line")) else { return Vec::new() };
    let Some(folder) = world.find_first(name) else { return Vec::new() };
    let mut points: Vec<(u32, Vec3)> = world
        .get(folder)
        .map(|f| f.children.clone())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|id| Some((world.get(id)?.name.parse::<u32>().ok()?, v(world.part(id)?.position))))
        .collect();
    points.sort_by_key(|(n, _)| *n);
    points.into_iter().map(|(_, p)| p).collect()
}
