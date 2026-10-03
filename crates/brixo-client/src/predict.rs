//! Client-side prediction: online, your own character moves the moment you
//! press a key, instead of waiting for the server to hear about it and
//! answer (a beat later on a far-away server).
//!
//! Brixo Player runs its own copy of the character physics for you alone,
//! against the world the server sent, with the same keys it sends the
//! server. It remembers where it put you over the last second. When the
//! server says where you are, that should be somewhere on that recent path
//! (the server is simply behind): if it is, nothing needs fixing but any
//! small drift sideways, which is eased out. If it's nowhere near (the
//! server moved you: a respawn, a teleporter, a jump pad), prediction gives
//! way and starts again from where the server says.
//!
//! Everyone else, and every part, is still drawn from the server.
//!
//! With a server that takes numbered steps (`step_steps`), walking is
//! predicted the exact way, Server Authority style: every physics step's
//! keys are numbered and sent; the server applies them in order and says
//! where you were after each (`You`); if that isn't where we had you, we
//! rewind to the server's word and replay our keys since. The server stays
//! the referee, and corrections are only for what we couldn't know (a
//! push, a door, a teleport), eased out on screen instead of snapped.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use brixo_core::{CharacterState, DataModel, InstanceId, Vec3};
use brixo_runtime::physics::{Physics, PHYSICS_DT};
use brixo_runtime::PlayerInput;

/// The server's word is off our recent path by more than this: it moved
/// us itself, so start again from where it says.
const SNAP: f32 = 3.0;
/// How much of our path to remember, in seconds (more than any sane lag).
const HISTORY: f64 = 1.0;
/// Drift smaller than this isn't worth correcting.
const DEAD_ZONE: f32 = 0.03;
/// How fast drift is eased out, per second (of what's left).
const EASE: f32 = 10.0;

pub struct Predictor {
    physics: Physics,
    me: Option<InstanceId>,
    /// Driving: your kart, predicted the same way (see `step_kart`).
    kart: Option<KartPrediction>,
    /// Where we've put the character, and when: newest last.
    history: VecDeque<(f64, Vec3)>,
    /// What we predict now: position, facing (degrees), speed, in the air.
    now: Option<(Vec3, f32, f32, bool)>,
    /// Where the server last had us.
    last_server: Option<Vec3>,
    base: Instant,
    /// Rewind-and-replay, with a server that takes numbered steps.
    rb: Rollback,
}

/// Rewind-and-replay state (see `step_steps`).
#[derive(Default)]
struct Rollback {
    /// The number of our newest step.
    seq: u64,
    /// Time not yet made into a whole step.
    acc: f32,
    /// Our steps the server hasn't confirmed: (number, keys, where that
    /// step left us).
    history: VecDeque<(u64, PlayerInput, CharacterState)>,
    /// The newest step the server has told us about.
    last_ack: u64,
    /// Drawn = predicted + this: a correction, eased out over a few frames.
    offset: glam::Vec3,
    /// Corrections so far (for measuring).
    corrections: u64,
}

/// How many of our steps we like waiting on the server: enough to ride out
/// a little jitter, few enough not to add lag.
const QUEUE_TARGET: f32 = 1.0;
/// Most unconfirmed steps kept (4 seconds): more than any sane lag.
const MAX_HISTORY: usize = 240;
/// How fast a correction's leftover is eased out, per second.
const OFFSET_EASE: f32 = 12.0;
/// A correction bigger than this is a teleport: shown at once, not eased.
const TELEPORT: f32 = 10.0;

/// Whether our prediction for a step and the server's word differ enough
/// to rewind.
fn disagree(ours: &CharacterState, server: &CharacterState) -> bool {
    dist(ours.position, server.position) > 0.01 || (ours.vertical_speed - server.vertical_speed).abs() > 0.05 || dist(ours.push, server.push) > 0.05
}

impl Default for Predictor {
    fn default() -> Self {
        Predictor { physics: Physics::new(), me: None, kart: None, history: VecDeque::new(), now: None, last_server: None, base: Instant::now(), rb: Rollback::default() }
    }
}

/// Your kart, predicted: which it is, where we've put its chassis (and
/// the kart's own facts: speed, steering...), and its recent path.
struct KartPrediction {
    kart: InstanceId,
    chassis: InstanceId,
    pose: Option<brixo_core::PartProps>,
    facts: Vec<(String, brixo_core::Attribute)>,
    seat: Option<(Vec3, Vec3)>,
    history: VecDeque<(f64, Vec3, f32)>,
    last_server: Option<Vec3>,
    /// The step before `pose` and `seat` (drawn blended between the two:
    /// physics steps 60 times a second, screens draw more often, and
    /// drawing the newest step makes a fast kart hop), and how far
    /// between them to draw.
    before: Option<(brixo_core::PartProps, Option<(Vec3, Vec3)>)>,
    blend: f32,
    /// When we last copied a spin-out from the server (only once each).
    spun_at: f64,
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

fn lerp(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    Vec3::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t)
}

/// Blends rotations (degrees), each the short way round.
fn lerp_angles(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    let one = |a: f32, b: f32| a + ((b - a + 540.0).rem_euclid(360.0) - 180.0) * t;
    Vec3::new(one(a.x, b.x), one(a.y, b.y), one(a.z, b.z))
}

fn dist(a: Vec3, b: Vec3) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

impl Predictor {
    /// Starts over (a new game, or rejoining).
    pub fn reset(&mut self) {
        *self = Predictor::default();
    }

    /// Moves your character `me` on by `dt` seconds with `input` (and
    /// shift lock's `facing`), starting from the world the server last sent.
    pub fn step(&mut self, server: &DataModel, me: Option<InstanceId>, input: PlayerInput, facing: Option<f32>, dt: f32) {
        self.step_at(server, me, input, facing, dt, Instant::now());
    }

    pub fn step_at(&mut self, server: &DataModel, me: Option<InstanceId>, input: PlayerInput, facing: Option<f32>, dt: f32, at: Instant) {
        let t = at.saturating_duration_since(self.base).as_secs_f64();
        let Some(me) = me else {
            self.me = None;
            self.now = None;
            return;
        };
        if self.me != Some(me) {
            self.reset();
            self.me = Some(me);
        }
        let Some(said) = server.player(me) else {
            self.now = None;
            return;
        };
        // Driving: predict the kart instead.
        if let Some(kart) = said.kart.filter(|k| brixo_core::is_kart(server, *k)) {
            self.now = None;
            self.history.clear();
            self.step_kart(server, me, kart, input, dt, t);
            return;
        }
        self.kart = None;
        // Knocked out: the server's in charge until you're back.
        if said.dead > 0.0 || said.health <= 0.0 {
            self.now = None;
            self.history.clear();
            return;
        }
        let server_at = said.body.position;
        let server_still = self.last_server.is_some_and(|l| dist(l, server_at) < 0.001);
        self.last_server = Some(server_at);

        // A copy of the world to run the physics on, with us where we
        // predicted (so the capsule carries on from there).
        let mut world = server.clone();
        let here = match self.now {
            Some((p, ..)) => p,
            None => server_at,
        };
        if let Some(p) = world.player_mut(me) {
            p.body.position = here;
        }

        // Where are we, going by the server? Find where on our recent path
        // it has us.
        let nearest = self.history.iter().map(|(_, p)| (*p, dist(*p, server_at))).min_by(|a, b| a.1.total_cmp(&b.1));
        let idle = input.move_x == 0.0 && input.move_z == 0.0 && !input.jump;
        let predicted_still = self.now.is_some_and(|(.., speed, airborne)| speed < 0.01 && !airborne);
        let fix = match nearest {
            // Nowhere near where we've been: the server moved us. Start
            // again from there (a teleport, which also stops any fall).
            Some((_, d)) if d > SNAP => {
                if let Some(p) = world.player_mut(me) {
                    p.body.position = server_at;
                }
                self.history.clear();
                None
            }
            // Both standing still: we should be exactly where it says.
            _ if idle && server_still && predicted_still && dist(here, server_at) > DEAD_ZONE => Some(sub(server_at, here)),
            // On the path, give or take a little drift: ease that out.
            Some((p, d)) if d > DEAD_ZONE => Some(sub(server_at, p)),
            _ => None,
        };
        // Sync the physics with the world first (so a nudge moves the
        // capsule that's really there).
        self.physics.step(&mut world, 0.0, &HashSet::new(), &HashMap::new());
        if let Some(off) = fix {
            let k = (EASE * dt).min(1.0);
            let off = Vec3::new(off.x * k, off.y * k, off.z * k);
            self.physics.nudge(&mut world, me, off);
            for (_, h) in self.history.iter_mut() {
                *h = Vec3::new(h.x + off.x, h.y + off.y, h.z + off.z);
            }
        }

        self.physics.hold_facing(me, facing);
        let inputs: HashMap<InstanceId, PlayerInput> = [(me, input)].into_iter().collect();
        self.physics.step(&mut world, dt, &HashSet::new(), &inputs);
        let Some(p) = world.player(me) else { return };
        let predicted = p.body.position;
        self.now = Some((predicted, p.body.rotation.y, p.speed, p.airborne));
        self.history.push_back((t, predicted));
        while self.history.front().is_some_and(|(when, _)| t - when > HISTORY) {
            self.history.pop_front();
        }
    }

    /// Corrections made so far by rewind-and-replay (for measuring).
    pub fn corrections(&self) -> u64 {
        self.rb.corrections
    }

    /// Server Authority prediction, for a server that takes numbered steps
    /// (NetClient::server_steps). `you` is the server's newest word on your
    /// character (NetClient::you). Gives back the keys to send for the
    /// physics steps this frame made: (first step's number, keys), for
    /// NetClient::send_steps. Driving still uses the kart prediction.
    pub fn step_steps(
        &mut self,
        server: &DataModel,
        me: Option<InstanceId>,
        you: Option<(u64, CharacterState)>,
        queued: u32,
        input: PlayerInput,
        facing: Option<f32>,
        dt: f32,
    ) -> (u64, Vec<PlayerInput>) {
        let t = Instant::now().saturating_duration_since(self.base).as_secs_f64();
        // Whole physics steps this frame, numbered; their keys go to the
        // server whatever we're doing (walking, driving, knocked out).
        // Keep about two waiting on the server: a little slower when more
        // pile up (they'd be old by the time they're used), a little faster
        // when it's run out (it repeats our last keys meanwhile).
        let pace = 1.0 + 0.03 * (QUEUE_TARGET - queued as f32).clamp(-3.0, 2.0);
        self.rb.acc = (self.rb.acc + dt.max(0.0) * pace).min(0.25);
        let mut keys = Vec::new();
        while self.rb.acc >= PHYSICS_DT {
            self.rb.acc -= PHYSICS_DT;
            keys.push(input);
        }
        let first = self.rb.seq + 1;
        self.rb.seq += keys.len() as u64;
        let numbered: Vec<(u64, PlayerInput)> = keys.iter().enumerate().map(|(i, k)| (first + i as u64, *k)).collect();

        let Some(me) = me else {
            self.me = None;
            self.now = None;
            return (first, keys);
        };
        if self.me != Some(me) {
            let rb = std::mem::take(&mut self.rb);
            self.reset();
            self.rb = Rollback { seq: rb.seq, ..Rollback::default() };
            self.me = Some(me);
        }
        let Some(said) = server.player(me) else {
            self.now = None;
            return (first, keys);
        };
        if let Some(kart) = said.kart.filter(|k| brixo_core::is_kart(server, *k)) {
            self.now = None;
            self.rb.history.clear();
            self.step_kart(server, me, kart, input, dt, t);
            return (first, keys);
        }
        if self.kart.take().is_some() {
            self.physics = Physics::new(); // back on foot: start the walking physics fresh
        }
        if said.dead > 0.0 || said.health <= 0.0 {
            self.now = None;
            self.rb.history.clear();
            return (first, keys);
        }

        // A copy of the world to run the physics in, with us where we have us.
        let mut world = server.clone();
        if let (Some(ours), Some(p)) = (self.physics.character_state(me), world.player_mut(me)) {
            p.body.position = ours.position;
        }
        self.physics.hold_facing(me, facing);
        // Sync the physics with the world (no time passes).
        self.physics.step(&mut world, 0.0, &HashSet::new(), &HashMap::new());

        // The server's word on a step of ours: if it isn't where we had
        // us, rewind to it and replay our keys since.
        if let Some((ack, theirs)) = you.filter(|(ack, _)| *ack > self.rb.last_ack) {
            self.rb.last_ack = ack;
            let mut ours = None;
            while let Some(&(n, _, state)) = self.rb.history.front() {
                if n > ack {
                    break;
                }
                self.rb.history.pop_front();
                if n == ack {
                    ours = Some(state);
                }
            }
            if ours.is_none_or(|o| disagree(&o, &theirs)) {
                let before = self.physics.character_state(me).map(|s| s.position);
                self.physics.set_character_state(&mut world, me, &theirs);
                let replay: Vec<(u64, PlayerInput)> = self.rb.history.drain(..).map(|(n, k, _)| (n, k)).collect();
                for (n, k) in replay {
                    self.one_step(&mut world, me, n, k);
                }
                if let (Some(b), Some(a)) = (before, self.physics.character_state(me).map(|s| s.position)) {
                    let jump = glam::Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z);
                    // Small: ease it out. A teleport: show it at once.
                    self.rb.offset = if (self.rb.offset + jump).length() < TELEPORT { self.rb.offset + jump } else { glam::Vec3::ZERO };
                }
                self.rb.corrections += 1;
            }
        }

        // This frame's new steps.
        for (n, k) in numbered {
            self.one_step(&mut world, me, n, k);
        }
        while self.rb.history.len() > MAX_HISTORY {
            self.rb.history.pop_front();
        }

        self.rb.offset *= (-OFFSET_EASE * dt).exp();
        if self.rb.offset.length() < 0.001 {
            self.rb.offset = glam::Vec3::ZERO;
        }
        if let Some(s) = self.physics.character_state(me) {
            let o = self.rb.offset;
            let shown = Vec3::new(s.position.x + o.x, s.position.y + o.y, s.position.z + o.z);
            let speed = world.player(me).map(|p| p.speed).unwrap_or(0.0);
            self.now = Some((shown, s.yaw.to_degrees(), speed, !s.grounded));
        }
        (first, keys)
    }

    /// One physics step of ours, remembered.
    fn one_step(&mut self, world: &mut DataModel, me: InstanceId, n: u64, keys: PlayerInput) {
        let inputs: HashMap<InstanceId, PlayerInput> = [(me, keys)].into_iter().collect();
        self.physics.step(world, PHYSICS_DT, &HashSet::new(), &inputs);
        if let Some(state) = self.physics.character_state(me) {
            self.rb.history.push_back((n, keys, state));
        }
    }

    /// Your kart, one step: run the kart physics on a copy of the world
    /// with the chassis where we predicted it, then check the server's
    /// chassis against our recent path, like the character above.
    fn step_kart(&mut self, server: &DataModel, me: InstanceId, kart: InstanceId, input: PlayerInput, dt: f32, t: f64) {
        let Some(chassis) = brixo_core::kart_chassis(server, kart) else { return };
        if self.kart.as_ref().is_none_or(|k| k.kart != kart || k.chassis != chassis) {
            self.physics = Physics::new();
            self.kart = Some(KartPrediction { kart, chassis, pose: None, facts: Vec::new(), seat: None, history: VecDeque::new(), last_server: None, before: None, blend: 1.0, spun_at: -10.0 });
        }
        let Some(server_pose) = server.part(chassis).copied() else { return };
        let server_at = server_pose.position;
        let k = self.kart.as_mut().unwrap();
        let mut world = server.clone();
        // Carry on from our own prediction.
        if let (Some(pose), Some(p)) = (k.pose, world.part_mut(chassis)) {
            *p = pose;
        }
        // Where's the server, on our path?
        let nearest = k.history.iter().map(|(_, p, yaw)| (*p, *yaw, dist(*p, server_at))).min_by(|a, b| a.2.total_cmp(&b.2));
        let mut snap = false;
        let mut fix = None;
        match nearest {
            None => {}
            // (Far off our path: it went somewhere we didn't, like a
            // respawn. Nearer, it's ease-able drift.)
            Some((_, _, d)) if d > 10.0 => snap = true,
            Some((p, yaw, d)) if d > DEAD_ZONE => {
                let turn = server_pose.rotation.y.to_radians() - yaw;
                let turn = (turn + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                fix = Some((sub(server_at, p), turn));
            }
            _ => {}
        }
        if snap {
            // It went somewhere we didn't: follow the server (placed, spun out...).
            if let Some(p) = world.part_mut(chassis) {
                *p = server_pose;
            }
            k.history.clear();
            k.before = None;
        }
        // Bring the physics up to date first when it has to be (a new
        // kart, or a snap), so the nudge below moves the right thing.
        if snap || self.physics.kart_timers(kart).is_none() {
            self.physics.step(&mut world, 0.0, &HashSet::new(), &HashMap::new());
        }
        if let Some((off, turn)) = fix {
            let e = (EASE * dt).min(1.0);
            // Never more than a little a frame: a nudge moves the kart
            // straight there, walls or not, and a big one could put it
            // through a wall.
            let step = glam::Vec3::new(off.x * e, off.y * e, off.z * e).clamp_length_max(0.25);
            self.physics.nudge_kart(kart, step, turn * e);
            for (_, h, yaw) in k.history.iter_mut() {
                *h = Vec3::new(h.x + step.x, h.y + step.y, h.z + step.z);
                *yaw += turn * e;
            }
        }
        // Boosts and spin-outs a script gave the server's kart (a boost pad,
        // an item): ours too, or we'd fall behind (or run on) and snap.
        let num = |key: &str| match server.get(kart).and_then(|i| i.attributes.get(key)) {
            Some(brixo_core::Attribute::Num(n)) => *n as f32,
            _ => 0.0,
        };
        if let Some((boost, spin)) = self.physics.kart_timers(kart) {
            if num("boost_left") > boost + 0.15 {
                self.physics.kart_command(brixo_runtime::physics::KartCommand::Boost(kart, num("boost_left")));
            }
            if num("spin_left") > spin + 0.15 && spin <= 0.0 && t - k.spun_at > 1.5 {
                self.physics.kart_command(brixo_runtime::physics::KartCommand::SpinOut(kart));
                k.spun_at = t;
            }
        }
        let mut inputs = HashMap::new();
        inputs.insert(kart, brixo_runtime::kart::KartInput::from_player(input));
        self.physics.set_kart_inputs(inputs);
        let players: HashMap<InstanceId, PlayerInput> = [(me, input)].into_iter().collect();
        let steps_before = self.physics.steps_run();
        self.physics.step(&mut world, dt, &HashSet::new(), &players);
        let stepped = self.physics.steps_run() > steps_before;
        let Some(pose) = world.part(chassis).copied() else { return };
        let seat = world.player(me).map(|p| (p.body.position, p.body.rotation));
        // A new physics step: the one we had becomes the one before. (With
        // no step, a drift correction still moves both, together.)
        if stepped {
            k.before = k.pose.map(|old| (old, k.seat));
        } else if let (Some(old), Some((then, _))) = (k.pose, k.before.as_mut()) {
            then.position = Vec3::new(then.position.x + pose.position.x - old.position.x, then.position.y + pose.position.y - old.position.y, then.position.z + pose.position.z - old.position.z);
        }
        k.blend = self.physics.step_fraction();
        k.pose = Some(pose);
        k.facts = world.get(kart).map(|i| i.attributes.iter().map(|(a, b)| (a.clone(), b.clone())).collect()).unwrap_or_default();
        k.seat = seat;
        k.history.push_back((t, pose.position, pose.rotation.y.to_radians()));
        while k.history.front().is_some_and(|(when, _, _)| t - when > HISTORY) {
            k.history.pop_front();
        }
        k.last_server = Some(server_at);
    }

    /// Draws your character where we predict, in `view` (the smoothed copy
    /// of the server's world that's about to be drawn).
    pub fn apply(&self, view: &mut DataModel) {
        if let (Some(k), Some(me)) = (&self.kart, self.me) {
            // Between the last two steps (see `before`).
            let (pose, seat) = match (k.pose, k.before) {
                (Some(now), Some((then, then_seat))) => {
                    let t = k.blend;
                    let mut p = now;
                    p.position = lerp(then.position, now.position, t);
                    p.rotation = lerp_angles(then.rotation, now.rotation, t);
                    let seat = match (then_seat, k.seat) {
                        (Some((a, ar)), Some((b, br))) => Some((lerp(a, b, t), lerp_angles(ar, br, t))),
                        _ => k.seat,
                    };
                    (Some(p), seat)
                }
                _ => (k.pose, k.seat),
            };
            if let (Some(pose), Some(p)) = (pose, view.part_mut(k.chassis)) {
                *p = pose;
            }
            if let Some(inst) = view.get_mut(k.kart) {
                for (key, value) in &k.facts {
                    inst.attributes.insert(key.clone(), value.clone());
                }
            }
            if let (Some((at, turn)), Some(p)) = (seat, view.player_mut(me)) {
                p.body.position = at;
                p.body.rotation = turn;
            }
            return;
        }
        let (Some(me), Some((position, yaw, speed, airborne))) = (self.me, self.now) else { return };
        if let Some(p) = view.player_mut(me) {
            p.body.position = position;
            p.body.rotation.y = yaw;
            p.speed = speed;
            p.airborne = airborne;
        }
    }

    /// Where we predict your kart's chassis is (while driving).
    pub fn kart_position(&self) -> Option<Vec3> {
        self.kart.as_ref()?.pose.map(|p| p.position)
    }

    /// Where we predict your character is right now.
    pub fn position(&self) -> Option<Vec3> {
        self.now.map(|(p, ..)| p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brixo_core::Class;
    use brixo_runtime::Game;
    use std::time::Duration;

    fn arena() -> DataModel {
        let mut dm = DataModel::new();
        let root = dm.root();
        let floor = dm.create(Class::Part, "Floor", root).unwrap();
        let p = dm.part_mut(floor).unwrap();
        p.size = Vec3::new(200.0, 1.0, 200.0);
        p.position = Vec3::new(0.0, -0.5, 0.0);
        let wall = dm.create(Class::Part, "Wall", root).unwrap();
        let w = dm.part_mut(wall).unwrap();
        w.size = Vec3::new(40.0, 10.0, 1.0);
        w.position = Vec3::new(0.0, 5.0, 25.0);
        let spawn = dm.create(Class::SpawnLocation, "Spawn", root).unwrap();
        dm.part_mut(spawn).unwrap().position = Vec3::new(0.0, 0.5, 0.0);
        dm
    }

    /// A server that hears our keys `lag` frames late, and a predictor
    /// that hears them straight away. Returns (predicted, server) each frame.
    fn play(game: &mut Game, me: InstanceId, pred: &mut Predictor, frames: usize, lag: usize, keys: impl Fn(usize) -> PlayerInput) -> Vec<(Vec3, Vec3)> {
        let start = Instant::now();
        let mut out = Vec::new();
        let mut queue: VecDeque<PlayerInput> = VecDeque::from(vec![PlayerInput::default(); lag]);
        for f in 0..frames {
            let input = keys(f);
            queue.push_back(input);
            game.set_input_for(me, queue.pop_front().unwrap());
            game.step(1.0 / 60.0);
            let server = game.world().clone();
            pred.step_at(&server, Some(me), input, None, 1.0 / 60.0, start + Duration::from_secs_f64(f as f64 / 60.0));
            out.push((pred.position().unwrap(), server.player(me).unwrap().body.position));
        }
        out
    }

    #[test]
    fn your_character_moves_at_once_and_ends_up_where_the_server_says() {
        let mut game = Game::start_server(arena());
        let me = game.add_player("Ann");
        for _ in 0..30 {
            game.step(1.0 / 60.0);
        }
        let mut pred = Predictor::default();
        let forward = PlayerInput { move_z: 1.0, ..Default::default() };
        // 12 frames (200 ms) of lag: walk for a second, then stop.
        let frames = play(&mut game, me, &mut pred, 150, 12, |f| if f < 60 { forward } else { PlayerInput::default() });
        let (p5, s5) = frames[5];
        assert!(p5.z > s5.z + 0.5, "moving at once, ahead of the server: predicted {p5:?}, server {s5:?}");
        let (p_end, s_end) = *frames.last().unwrap();
        assert!(dist(p_end, s_end) < 0.2, "both agree once stopped: predicted {p_end:?}, server {s_end:?}");
        // Smooth all the way: never pulled back while walking.
        for w in frames[..60].windows(2) {
            assert!(w[1].0.z >= w[0].0.z - 1e-3, "pulled back: {:?} then {:?}", w[0].0, w[1].0);
        }
    }

    #[test]
    fn jumps_are_predicted_and_land_in_the_same_place() {
        let mut game = Game::start_server(arena());
        let me = game.add_player("Ann");
        for _ in 0..30 {
            game.step(1.0 / 60.0);
        }
        let mut pred = Predictor::default();
        let jump = |f: usize| PlayerInput { move_x: 1.0, jump: f < 10, ..Default::default() };
        let frames = play(&mut game, me, &mut pred, 120, 9, jump);
        let (p8, s8) = frames[8];
        assert!(p8.y > s8.y + 0.3, "up at once: {p8:?} vs {s8:?}");
        let tail = play(&mut game, me, &mut pred, 60, 9, |_| PlayerInput::default());
        let (p, s) = *tail.last().unwrap();
        assert!(dist(p, s) < 0.2, "landed together: {p:?} {s:?}");
    }

    #[test]
    fn walls_stop_the_predicted_character_too() {
        let mut game = Game::start_server(arena());
        let me = game.add_player("Ann");
        let mut pred = Predictor::default();
        let forward = PlayerInput { move_z: 1.0, ..Default::default() };
        let frames = play(&mut game, me, &mut pred, 240, 6, |_| forward);
        let (p, s) = *frames.last().unwrap();
        assert!(p.z < 24.0 && s.z < 24.0, "neither walked through the wall: {p:?} {s:?}");
        assert!(dist(p, s) < 0.3, "and they agree at the wall: {p:?} {s:?}");
    }

    #[test]
    fn when_the_server_moves_you_prediction_follows() {
        let mut game = Game::start_server(arena());
        let me = game.add_player("Ann");
        let mut pred = Predictor::default();
        play(&mut game, me, &mut pred, 30, 6, |_| PlayerInput::default());
        // A teleporter (a script) moves us far away.
        game.world().player_mut(me).unwrap().body.position = Vec3::new(60.0, 3.5, -40.0);
        let frames = play(&mut game, me, &mut pred, 10, 6, |_| PlayerInput::default());
        let (p, s) = *frames.last().unwrap();
        assert!(dist(p, s) < 0.5, "followed the teleport: {p:?} {s:?}");
    }

    fn kart_arena() -> (DataModel, InstanceId) {
        let mut dm = arena();
        let root = dm.root();
        let k = dm.create(Class::Model, "Kart", root).unwrap();
        dm.get_mut(k).unwrap().attributes.insert("kart".into(), brixo_core::Attribute::Bool(true));
        let c = dm.create(Class::Part, "Chassis", k).unwrap();
        let p = dm.part_mut(c).unwrap();
        p.size = Vec3::new(3.0, 0.8, 5.0);
        p.position = Vec3::new(40.0, 0.45, 0.0);
        (dm, k)
    }

    #[test]
    fn your_kart_is_predicted_too() {
        let (dm, k) = kart_arena();
        let mut game = Game::start_server(dm);
        let me = game.add_player("Ann");
        game.world().player_mut(me).unwrap().kart = Some(k);
        for _ in 0..30 {
            game.step(1.0 / 60.0);
        }
        let chassis = |w: &DataModel| {
            let c = brixo_core::kart_chassis(w, k).unwrap();
            w.part(c).unwrap().position
        };
        let mut pred = Predictor::default();
        let start = Instant::now();
        let lag = 12;
        let mut queue: VecDeque<PlayerInput> = VecDeque::from(vec![PlayerInput::default(); lag]);
        let throttle = PlayerInput { move_z: 1.0, ..Default::default() };
        let mut frames = Vec::new();
        for f in 0..240 {
            // A second of throttle, then the brakes, then nothing.
            let input = if f < 60 {
                throttle
            } else if f < 90 {
                PlayerInput { move_z: -1.0, ..Default::default() }
            } else {
                PlayerInput::default()
            };
            queue.push_back(input);
            game.set_input_for(me, queue.pop_front().unwrap());
            game.step(1.0 / 60.0);
            let server = game.world().clone();
            pred.step_at(&server, Some(me), input, None, 1.0 / 60.0, start + Duration::from_secs_f64(f as f64 / 60.0));
            frames.push((pred.kart_position().unwrap(), chassis(&server)));
        }
        let (p, s) = frames[8];
        assert!(p.z > s.z + 0.2, "moving at once: predicted {p:?}, server {s:?}");
        let (p, s) = frames[50];
        assert!(dist(p, s) < 12.0 && p.z > s.z + 2.0, "ahead of the server by about the lag: {p:?} {s:?}");
        let (p, s) = *frames.last().unwrap();
        assert!(dist(p, s) < 0.5, "they agree once stopped: {p:?} {s:?}");
        // Never pulled back while driving.
        for w in frames[..60].windows(2) {
            assert!(w[1].0.z >= w[0].0.z - 0.05, "pulled back: {:?} then {:?}", w[0].0, w[1].0);
        }
    }

    #[test]
    fn your_kart_moves_smoothly_on_a_fast_screen() {
        // Physics steps 60 times a second; the screen draws 144. Drawn,
        // the kart should move about the same distance every frame, not
        // sit still for a frame and then hop.
        let (dm, k) = kart_arena();
        let mut game = Game::start_server(dm);
        let me = game.add_player("Ann");
        game.world().player_mut(me).unwrap().kart = Some(k);
        for _ in 0..30 {
            game.step(1.0 / 60.0);
        }
        let mut pred = Predictor::default();
        let start = Instant::now();
        let throttle = PlayerInput { move_z: 1.0, ..Default::default() };
        let mut drawn = Vec::new();
        let mut server_time = 0.0;
        for f in 0..(144 * 3) {
            let now = f as f64 / 144.0;
            while server_time < now {
                game.set_input_for(me, throttle);
                game.step(1.0 / 60.0);
                server_time += 1.0 / 60.0;
            }
            let server = game.world().clone();
            pred.step_at(&server, Some(me), throttle, None, 1.0 / 144.0, start + Duration::from_secs_f64(now));
            let mut view = server.clone();
            pred.apply(&mut view);
            let c = brixo_core::kart_chassis(&view, k).unwrap();
            drawn.push(view.part(c).unwrap().position.z);
        }
        // At speed (the last second), every frame moves on by about the same.
        let steps: Vec<f32> = drawn.windows(2).skip(144 * 2).map(|w| w[1] - w[0]).collect();
        let mean = steps.iter().sum::<f32>() / steps.len() as f32;
        assert!(mean > 0.3, "moving: {mean}");
        for s in &steps {
            assert!((s - mean).abs() < mean * 0.35, "uneven: {s} against {mean}: {steps:?}");
        }
    }
}
