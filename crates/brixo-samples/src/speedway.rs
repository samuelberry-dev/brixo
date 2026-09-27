//! Brickport Speedway: kart racing for up to 8, with bots filling the grid.
//!
//! A 2.3 km figure-8 built from a spline: a banked sweeper, a bridge the
//! track crosses under, a canyon jump over the river, a town street, a
//! hairpin and a shortcut through a barn. Around it: a grandstand with a
//! crowd, a jumbotron, the start lights, the pits and a podium with
//! fireworks. Races go lobby, grid, lights, three laps, results, and the
//! sun moves on between races (sunset, night, afternoon).
//!
//! Shows off karts (`kart = true` Models, `p.kart`, `boost`, `spin_out`,
//! `place_kart`, `locked`, `top_speed`), bots (`add_bot`, the racing
//! line), keys (`on key`), per-player GUI, saving and the leaderboard.

use brixo_core::{Attribute, Class, Color, DataModel, InstanceId, Material, Shape, Vec3};

type Rgb = (u8, u8, u8);

/// The track's middle line, as (x, y, z) control points: y is the road's
/// height. Driven in this order, starting down the main straight (+X).
pub const CONTROL: [(f32, f32, f32); 31] = [
    (-150.0, 0.0, 0.0), (0.0, 0.0, 0.0), (150.0, 0.0, 0.0),        // the main straight
    (235.0, 0.0, 30.0), (275.0, 0.0, 120.0), (240.0, 0.0, 215.0),   // the banked sweeper
    (150.0, 0.0, 250.0), (70.0, 3.0, 250.0),                        // back straight, climbing
    (0.0, 12.0, 225.0), (-60.0, 14.0, 185.0), (-120.0, 12.0, 150.0), // the bridge
    (-185.0, 4.0, 140.0), (-240.0, 0.0, 165.0),                     // down, and north
    (-265.0, 0.0, 240.0), (-265.0, 0.0, 300.0),                     // the canyon jump
    (-262.0, 0.0, 370.0), (-225.0, 0.0, 420.0),                     // landing, turning east
    (-150.0, 0.0, 440.0), (-60.0, 0.0, 440.0), (20.0, 0.0, 425.0),  // the town street
    (66.0, 0.0, 385.0), (44.0, 0.0, 336.0),                         // the hairpin
    (-10.0, 0.0, 300.0), (-60.0, 0.0, 230.0), (-90.0, 0.0, 120.0),  // south, under the bridge
    (-130.0, 0.0, 62.0), (-200.0, 0.0, 75.0), (-250.0, 0.0, 58.0),  // the last corner
    (-268.0, 0.0, 25.0), (-245.0, 0.0, -5.0), (-200.0, 0.0, -4.0),
];
/// Road width, wall to wall.
pub const WIDTH: f32 = 28.0;
/// How far apart the track's samples are.
const STEP: f32 = 4.0;
/// Road surface is this far above its line (clear of the grass).
const LIFT: f32 = 0.2;
/// How far the road may lean into a bend, in degrees. None: leaning road
/// pieces left little lips where they met, and karts caught on them.
const MAX_BANK: f32 = 0.0;
/// Laps in a race.
pub const LAPS: u32 = 3;
/// The river: a channel running in from the west into a lake.
const RIVER: (f32, f32, f32, f32) = (-480.0, -175.0, 300.0, 330.0); // x from, x to, z from, z to
const LAKE: (f32, f32, f32, f32) = (-175.0, -105.0, 300.0, 380.0);
/// The jump: the kicker's lip and where the road starts again.
const LIP_Z: f32 = 296.0;
const LAND_Z: f32 = 332.0;
/// The barn shortcut, from the last corner onto the main straight.
const CUT_FROM: (f32, f32) = (-150.0, 57.0);
const CUT_TO: (f32, f32) = (-108.0, 2.0);
/// Where the pits and the plaza are (x from, x to, z from, z to).
const PLAZA: (f32, f32, f32, f32) = (-70.0, 70.0, 15.0, 62.0);
const PODIUM: (f32, f32) = (60.0, 112.0);
/// The Drift Park: a practice pad in the infield (x from, x to, z from, z to).
pub const DRIFT: (f32, f32, f32, f32) = (105.0, 212.0, 58.0, 188.0);

pub const KART_COLORS: [(&str, Rgb, Rgb); 8] = [
    ("Red", (220, 45, 40), (255, 210, 60)),
    ("Blue", (30, 110, 220), (240, 240, 240)),
    ("Green", (40, 170, 80), (250, 250, 250)),
    ("Yellow", (250, 200, 30), (30, 30, 36)),
    ("Orange", (250, 130, 30), (30, 30, 36)),
    ("Purple", (140, 70, 200), (255, 210, 60)),
    ("Pink", (240, 110, 180), (250, 250, 250)),
    ("Cyan", (40, 200, 220), (30, 30, 36)),
];

type V = (f32, f32, f32);

fn add(a: V, b: V) -> V {
    (a.0 + b.0, a.1 + b.1, a.2 + b.2)
}
fn sub(a: V, b: V) -> V {
    (a.0 - b.0, a.1 - b.1, a.2 - b.2)
}
fn mul(a: V, k: f32) -> V {
    (a.0 * k, a.1 * k, a.2 * k)
}
fn len(a: V) -> f32 {
    (a.0 * a.0 + a.1 * a.1 + a.2 * a.2).sqrt()
}
fn flat_dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Turns a vector by Brixo's rotation (degrees; yaw, then pitch, then roll).
fn rotate(v: V, rot: V) -> V {
    let (x, y, z) = (rot.0.to_radians(), rot.1.to_radians(), rot.2.to_radians());
    // Roll about Z.
    let v = (v.0 * z.cos() - v.1 * z.sin(), v.0 * z.sin() + v.1 * z.cos(), v.2);
    // Pitch about X.
    let v = (v.0, v.1 * x.cos() - v.2 * x.sin(), v.1 * x.sin() + v.2 * x.cos());
    // Yaw about Y.
    (v.0 * y.cos() + v.2 * y.sin(), v.1, -v.0 * y.sin() + v.2 * y.cos())
}

/// Distance from `p` to the segment a-b, on the ground.
fn seg_dist(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dz) = (b.0 - a.0, b.1 - a.1);
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dz) / (dx * dx + dz * dz)).clamp(0.0, 1.0);
    flat_dist(p, (a.0 + dx * t, a.1 + dz * t))
}

fn in_rect(p: (f32, f32), r: (f32, f32, f32, f32), pad: f32) -> bool {
    p.0 > r.0 - pad && p.0 < r.1 + pad && p.1 > r.2 - pad && p.1 < r.3 + pad
}

fn in_water(p: (f32, f32), pad: f32) -> bool {
    in_rect(p, RIVER, pad) || in_rect(p, LAKE, pad)
}

/// Whether a stretch of track is over the canyon (no road there).
fn over_gap(p: (f32, f32)) -> bool {
    p.0 < -200.0 && p.1 > LIP_Z && p.1 < LAND_Z
}

/// One point along the track.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub at: V,
    /// Facing, degrees (0 is +Z, 90 is +X).
    pub yaw: f32,
    /// Nose down, degrees (negative going uphill).
    pub pitch: f32,
    /// Lean, degrees (the outside of a bend up).
    pub bank: f32,
}

fn catmull(p0: V, p1: V, p2: V, p3: V, t: f32) -> V {
    let (t2, t3) = (t * t, t * t * t);
    let f = |a: f32, b: f32, c: f32, d: f32| 0.5 * (2.0 * b + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (-a + 3.0 * b - 3.0 * c + d) * t3);
    (f(p0.0, p1.0, p2.0, p3.0), f(p0.1, p1.1, p2.1, p3.1), f(p0.2, p1.2, p2.2, p3.2))
}

fn wrap_deg(mut a: f32) -> f32 {
    while a > 180.0 {
        a -= 360.0;
    }
    while a < -180.0 {
        a += 360.0;
    }
    a
}

/// The track, sampled every few studs, starting at the finish line (0, 0, 0).
pub fn track() -> Vec<Sample> {
    let n = CONTROL.len();
    let mut pts: Vec<V> = Vec::new();
    let mut start = 0;
    for i in 0..n {
        let (p0, p1, p2, p3) = (CONTROL[(i + n - 1) % n], CONTROL[i], CONTROL[(i + 1) % n], CONTROL[(i + 2) % n]);
        if i == 1 {
            start = pts.len();
        }
        let k = ((len(sub(p2, p1)) / STEP) as usize).max(2);
        for j in 0..k {
            pts.push(catmull(p0, p1, p2, p3, j as f32 / k as f32));
        }
    }
    pts.rotate_left(start);
    let m = pts.len();
    let yaw_at = |i: usize| {
        let d = sub(pts[(i + 1) % m], pts[(i + m - 1) % m]);
        d.0.atan2(d.2).to_degrees()
    };
    let yaws: Vec<f32> = (0..m).map(yaw_at).collect();
    // How fast it turns (degrees a stud), smoothed, for banking.
    let turn: Vec<f32> = (0..m)
        .map(|i| {
            let a = yaws[(i + m - 2) % m];
            let b = yaws[(i + 2) % m];
            wrap_deg(b - a) / (4.0 * STEP)
        })
        .collect();
    let banks: Vec<f32> = (0..m)
        .map(|i| {
            let mut bank = 0.0;
            for k in 0..9 {
                bank += turn[(i + m + k - 4) % m];
            }
            bank /= 9.0;
            // The outside of the bend up: turning left (yaw growing), the
            // right side rises.
            // (Gentle bends and straights stay flat.)
            let b = (-bank * 9.0).clamp(-MAX_BANK, MAX_BANK);
            b.signum() * (b.abs() - 1.5).max(0.0)
        })
        .collect();
    // Where it leans, lift it so the low side stays above the grass.
    for i in 0..m {
        pts[i].1 += bank_lift(banks[i]);
    }
    (0..m)
        .map(|i| {
            let d = sub(pts[(i + 1) % m], pts[(i + m - 1) % m]);
            let pitch = -(d.1.atan2((d.0 * d.0 + d.2 * d.2).sqrt())).to_degrees();
            Sample { at: pts[i], yaw: yaws[i], pitch, bank: banks[i] }
        })
        .collect()
}

/// How much a leaning road is lifted (its low edge's drop).
fn bank_lift(bank: f32) -> f32 {
    WIDTH / 2.0 * bank.to_radians().sin().abs()
}

/// Stretches of track to build as one piece: `(first, last)` sample
/// indices, while it's nearly straight (and not over the canyon).
fn stretches(t: &[Sample], most: f32) -> Vec<(usize, usize)> {
    let m = t.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < m {
        let mut j = i + 1;
        while j < m
            && len(sub(t[j].at, t[i].at)) < most
            && wrap_deg(t[j].yaw - t[i].yaw).abs() < 2.5
            && (t[j].pitch - t[i].pitch).abs() < 1.0
            && (t[j].bank - t[i].bank).abs() < 1.0
            && over_gap((t[j].at.0, t[j].at.2)) == over_gap((t[i].at.0, t[i].at.2))
        {
            j += 1;
        }
        out.push((i, j % m));
        i = j;
    }
    out
}

struct Rand(u64);

impl Rand {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f32) / (1u64 << 31) as f32
    }
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.next()
    }
}

struct B {
    dm: DataModel,
}

impl B {
    fn root(&self) -> InstanceId {
        self.dm.root()
    }
    fn folder(&mut self, parent: InstanceId, name: &str) -> InstanceId {
        self.dm.create(Class::Folder, name, parent).unwrap()
    }
    #[allow(clippy::too_many_arguments)]
    fn part(&mut self, parent: InstanceId, name: &str, at: V, size: V, color: Rgb, material: Material) -> InstanceId {
        let id = self.dm.create(Class::Part, name, parent).unwrap();
        let p = self.dm.part_mut(id).unwrap();
        p.position = Vec3::new(at.0, at.1, at.2);
        p.size = Vec3::new(size.0, size.1, size.2);
        p.color = Color::new(color.0, color.1, color.2);
        p.material = material;
        p.anchored = true;
        id
    }
    fn turned(&mut self, id: InstanceId, rot: V) -> InstanceId {
        self.dm.part_mut(id).unwrap().rotation = Vec3::new(rot.0, rot.1, rot.2);
        id
    }
    fn shape(&mut self, id: InstanceId, shape: Shape) -> InstanceId {
        self.dm.part_mut(id).unwrap().shape = shape;
        id
    }
    fn ghost(&mut self, id: InstanceId, transparency: f32) -> InstanceId {
        let p = self.dm.part_mut(id).unwrap();
        p.can_collide = false;
        p.transparency = transparency;
        id
    }
    fn script(&mut self, parent: InstanceId, name: &str, source: &str) {
        let s = self.dm.create(Class::Script, name, parent).unwrap();
        self.dm.script_mut(s).unwrap().source = source.trim_start().to_string();
    }
    fn attr(&mut self, id: InstanceId, key: &str, value: Attribute) {
        self.dm.get_mut(id).unwrap().attributes.insert(key.into(), value);
    }
    #[allow(clippy::too_many_arguments)]
    fn label(&mut self, parent: InstanceId, name: &str, text: &str, rect: (f32, f32, f32, f32), size: f32, color: Rgb, background: Option<Rgb>) -> InstanceId {
        let id = self.dm.create(Class::TextLabel, name, parent).unwrap();
        let g = self.dm.gui_mut(id).unwrap();
        g.text = text.into();
        (g.x, g.y, g.width, g.height) = rect;
        g.text_size = size;
        g.text_color = Color::new(color.0, color.1, color.2);
        g.background = background.is_some();
        if let Some(c) = background {
            g.background_color = Color::new(c.0, c.1, c.2);
        }
        id
    }
    /// A block along the line from `a` to `b`, `across` wide and `high`.
    fn beam(&mut self, parent: InstanceId, name: &str, a: V, b: V, across: f32, high: f32, color: Rgb, material: Material) -> InstanceId {
        let d = sub(b, a);
        let flat = (d.0 * d.0 + d.2 * d.2).sqrt();
        let yaw = d.0.atan2(d.2).to_degrees();
        let pitch = -(d.1.atan2(flat)).to_degrees();
        let id = self.part(parent, name, mul(add(a, b), 0.5), (across, high, len(d)), color, material);
        self.turned(id, (pitch, yaw, 0.0))
    }
}

const ROAD: Rgb = (58, 60, 66);
const WALL_RED: Rgb = (205, 40, 40);
const WALL_WHITE: Rgb = (238, 238, 238);
const GRASS: Rgb = (86, 160, 72);
const ROCK: Rgb = (126, 112, 98);
const CONCRETE: Rgb = (170, 168, 162);
const NAVY: Rgb = (13, 42, 74);
const GOLD: Rgb = (245, 205, 48);

/// The road, walls, curbs, the kicker and the landing, and the pillars
/// under the bridge.
fn build_track(b: &mut B, t: &[Sample]) {
    let root = b.root();
    let track = b.folder(root, "Track");
    let road = b.folder(track, "Road");
    let walls = b.folder(track, "Walls");
    let curbs = b.folder(track, "Curbs");
    let m = t.len();
    let half = WIDTH / 2.0;

    // The road: one slab per stretch, its top through the line.
    for (i, j) in stretches(t, 36.0) {
        let (a, z) = (t[i], t[j]);
        let mid2 = mul(add(a.at, z.at), 0.5);
        if over_gap((mid2.0, mid2.2)) || over_gap((a.at.0, a.at.2)) {
            continue;
        }
        let d = sub(z.at, a.at);
        let flat = (d.0 * d.0 + d.2 * d.2).sqrt();
        let rot = (-(d.1.atan2(flat)).to_degrees(), d.0.atan2(d.2).to_degrees(), (a.bank + z.bank) / 2.0);
        // Down to the ground where the road's lifted for leaning.
        let thick = 1.2 + 2.0 * bank_lift((a.bank + z.bank) / 2.0);
        let mid = add(mul(add(a.at, z.at), 0.5), (0.0, LIFT, 0.0));
        let centre = add(mid, rotate((0.0, -thick / 2.0, 0.0), rot));
        let id = b.part(road, "Road", centre, (WIDTH + 1.0, thick, len(d) + 1.2), ROAD, Material::Concrete);
        b.turned(id, rot);
    }

    // The walls and curbs: shorter pieces, striped.
    let cut = |p: (f32, f32)| seg_dist(p, CUT_FROM, CUT_TO) < 10.0;
    let pit_gap = |p: (f32, f32)| p.0 > PLAZA.0 && p.0 < PLAZA.1 && p.1 > 5.0 && p.1 < 25.0;
    for (n, (i, j)) in stretches(t, 12.0).into_iter().enumerate() {
        let (a, z) = (t[i], t[j]);
        let d = sub(z.at, a.at);
        let flat = (d.0 * d.0 + d.2 * d.2).sqrt();
        let rot = (-(d.1.atan2(flat)).to_degrees(), d.0.atan2(d.2).to_degrees(), (a.bank + z.bank) / 2.0);
        let mid = add(mul(add(a.at, z.at), 0.5), (0.0, LIFT, 0.0));
        let bendy = (a.bank + z.bank).abs() / 2.0 > 3.5;
        for side in [1.0f32, -1.0] {
            let at = add(mid, rotate((side * (half + 0.75), 1.5, 0.0), rot));
            let p = (at.0, at.2);
            if over_gap(p) || over_gap((a.at.0, a.at.2)) || cut(p) || pit_gap(p) {
                continue;
            }
            let color = if n % 2 == 0 { WALL_RED } else { WALL_WHITE };
            let id = b.part(walls, "Wall", at, (1.5, 3.0 + 0.4, len(d) + 0.6), color, Material::Plastic);
            b.turned(id, rot);
            if bendy {
                let at = add(mid, rotate((side * (half - 1.2), 0.08, 0.0), rot));
                let color = if n % 2 == 0 { WALL_RED } else { WALL_WHITE };
                let id = b.part(curbs, "Curb", at, (2.4, 0.2, len(d) + 0.2), color, Material::Plastic);
                b.turned(id, rot);
                b.ghost(id, 0.0);
            }
        }
    }

    // The kicker and the landing, over the canyon.
    let lip = t.iter().filter(|s| s.at.0 < -200.0).min_by(|a, b| (a.at.2 - LIP_Z).abs().total_cmp(&(b.at.2 - LIP_Z).abs())).unwrap();
    let x = lip.at.0;
    let kicker = b.part(track, "Kicker", (x, LIFT + 3.0, LIP_Z - 8.0), (WIDTH - 1.0, 6.0, 16.0), (250, 200, 30), Material::Metal);
    b.shape(kicker, Shape::Wedge);
    b.turned(kicker, (0.0, lip.yaw, 0.0));
    let landing = b.part(track, "Landing", (x, LIFT + 1.5, LAND_Z + 8.0), (WIDTH - 1.0, 3.0, 16.0), (250, 200, 30), Material::Metal);
    b.shape(landing, Shape::Wedge);
    b.turned(landing, (0.0, lip.yaw + 180.0, 0.0));
    // A boost strip before it, the full width: everyone flies.
    let strip = b.part(track, "Jump Boost", (x, LIFT + 0.06, LIP_Z - 34.0), (WIDTH - 2.0, 0.12, 8.0), (255, 150, 30), Material::Neon);
    b.turned(strip, (0.0, lip.yaw, 0.0));
    b.ghost(strip, 0.0);
    let trigger = b.part(strip, "Boost Trigger", (x, LIFT + 1.0, LIP_Z - 34.0), (WIDTH - 2.0, 3.0, 8.0), (255, 150, 30), Material::Neon);
    b.turned(trigger, (0.0, lip.yaw, 0.0));
    b.ghost(trigger, 1.0);
    b.script(trigger, "Boost", BOOST_PAD);
    // Chevrons on the kicker face.
    for k in 0..3 {
        let z = LIP_Z - 13.0 + k as f32 * 4.5;
        let y = LIFT + (z - (LIP_Z - 16.0)) / 16.0 * 6.0 + 0.05;
        let id = b.part(track, "Chevron", (x, y, z), (WIDTH - 4.0, 0.1, 1.2), (30, 30, 36), Material::Plastic);
        b.turned(id, (-20.5, lip.yaw, 0.0));
        b.ghost(id, 0.0);
    }

    // Pillars under the raised road, clear of the road going underneath.
    let pillars = b.folder(track, "Pillars");
    let low: Vec<(f32, f32)> = t.iter().filter(|s| s.at.1 < 2.0).map(|s| (s.at.0, s.at.2)).collect();
    for i in (0..m).step_by(6) {
        let s = t[i];
        if s.at.1 < 2.5 {
            continue;
        }
        for side in [1.0f32, -1.0] {
            let at = add(s.at, rotate((side * (half - 2.0), 0.0, 0.0), (0.0, s.yaw, 0.0)));
            if low.iter().any(|p| flat_dist(*p, (at.0, at.2)) < half + 4.0) {
                continue;
            }
            // (Below the low edge where the road leans.)
            let h = s.at.1 + LIFT - 1.2 - s.bank.to_radians().sin().abs() * half - 0.3;
            b.part(pillars, "Pillar", (at.0, h / 2.0, at.2), (2.5, h, 2.5), CONCRETE, Material::Concrete);
        }
    }
}

/// The ground, cut by the river and the lake, with rocky banks.
fn build_land(b: &mut B, rng: &mut Rand) {
    let root = b.root();
    let land = b.folder(root, "Land");
    let (x0, x1, z0, z1) = (-480.0f32, 480.0f32, -240.0f32, 620.0f32);
    let depth = 12.0;
    let ground = |b: &mut B, xa: f32, xb: f32, za: f32, zb: f32| {
        b.part(land, "Ground", ((xa + xb) / 2.0, -depth / 2.0, (za + zb) / 2.0), (xb - xa, depth, zb - za), GRASS, Material::Grass);
    };
    ground(b, x0, x1, z0, RIVER.2);
    ground(b, LAKE.1, x1, RIVER.2, z1);
    ground(b, x0, LAKE.0, RIVER.3, z1);
    ground(b, LAKE.0, LAKE.1, LAKE.3, z1);
    // The bed, the water, and rock on the banks.
    let bed = b.part(land, "River Bed", ((x0 + LAKE.1) / 2.0, -depth - 0.5, (RIVER.2 + LAKE.3) / 2.0), (LAKE.1 - x0, 1.0, LAKE.3 - RIVER.2), (96, 84, 70), Material::Concrete);
    let _ = bed;
    let water = |b: &mut B, r: (f32, f32, f32, f32)| {
        let id = b.part(land, "Water", ((r.0 + r.1) / 2.0, -5.5, (r.2 + r.3) / 2.0), (r.1 - r.0, 1.0, r.3 - r.2), (50, 120, 210), Material::Plastic);
        b.ghost(id, 0.25);
    };
    water(b, RIVER);
    water(b, LAKE);
    // Rocky faces down the banks.
    let face = |b: &mut B, a: (f32, f32), c: (f32, f32), rng: &mut Rand| {
        let d = flat_dist(a, c);
        let n = (d / 16.0).ceil() as usize;
        for k in 0..n {
            let t0 = k as f32 / n as f32;
            let t1 = (k + 1) as f32 / n as f32;
            let p = (a.0 + (c.0 - a.0) * (t0 + t1) / 2.0, a.1 + (c.1 - a.1) * (t0 + t1) / 2.0);
            let shade = rng.range(0.85, 1.1);
            let col = ((ROCK.0 as f32 * shade) as u8, (ROCK.1 as f32 * shade) as u8, (ROCK.2 as f32 * shade) as u8);
            let along_x = (c.1 - a.1).abs() < 0.01;
            let size = if along_x { (d / n as f32 + 0.5, depth - 0.3, 1.2) } else { (1.2, depth - 0.3, d / n as f32 + 0.5) };
            b.part(land, "Cliff", (p.0, -depth / 2.0 - 0.1, p.1), size, col, Material::Concrete);
        }
    };
    face(b, (x0, RIVER.2 + 0.5), (LAKE.0, RIVER.2 + 0.5), rng);
    face(b, (x0, RIVER.3 - 0.5), (LAKE.0, RIVER.3 - 0.5), rng);
    face(b, (LAKE.0, LAKE.2 + 0.5), (LAKE.1, LAKE.2 + 0.5), rng);
    face(b, (LAKE.0 + 0.5, RIVER.3), (LAKE.0 + 0.5, LAKE.3), rng);
    face(b, (LAKE.1 - 0.5, LAKE.2), (LAKE.1 - 0.5, LAKE.3), rng);
    face(b, (LAKE.0, LAKE.3 - 0.5), (LAKE.1, LAKE.3 - 0.5), rng);
}

/// Somewhere clear of the track, water, and the places built on.
fn clear(t: &[Sample], p: (f32, f32), r: f32) -> bool {
    if in_water(p, r + 2.0) || in_rect(p, PLAZA, r + 4.0) || in_rect(p, DRIFT, r + 6.0) || in_rect(p, (-125.0, 125.0, -85.0, -15.0), r) || flat_dist(p, PODIUM) < r + 22.0 {
        return false;
    }
    if seg_dist(p, CUT_FROM, CUT_TO) < r + 12.0 {
        return false;
    }
    if flat_dist(p, (145.0, -45.0)) < r + 22.0 {
        return false;
    }
    t.iter().all(|s| flat_dist((s.at.0, s.at.2), p) > WIDTH / 2.0 + 5.0 + r)
}

fn tree(b: &mut B, parent: InstanceId, x: f32, z: f32, rng: &mut Rand) {
    let t = b.dm.create(Class::Model, "Tree", parent).unwrap();
    let h = rng.range(6.0, 11.0);
    let s = rng.range(6.0, 10.0);
    let g = rng.range(0.8, 1.15);
    b.part(t, "Trunk", (x, h / 2.0, z), (1.6, h, 1.6), (105, 70, 44), Material::Wood);
    let leaves = b.part(t, "Leaves", (x, h + s * 0.35, z), (s, s * 1.1, s), ((50.0 * g) as u8, (135.0 * g) as u8, (60.0 * g) as u8), Material::Grass);
    if rng.next() < 0.35 {
        // A pine: a cone of stacked wedges would be nicer; a tall ball will do.
        b.dm.part_mut(leaves).unwrap().size = Vec3::new(s * 0.7, s * 1.6, s * 0.7);
    }
    b.shape(leaves, Shape::Ball);
}

fn house(b: &mut B, parent: InstanceId, x: f32, z: f32, yaw: f32, rng: &mut Rand) {
    let h = b.dm.create(Class::Model, "House", parent).unwrap();
    let colors: [Rgb; 6] = [(236, 222, 190), (200, 220, 240), (240, 200, 190), (210, 236, 200), (245, 235, 160), (230, 230, 230)];
    let wall = colors[(rng.next() * 6.0) as usize % 6];
    let roof: Rgb = if rng.next() < 0.5 { (150, 60, 50) } else { (70, 80, 100) };
    let (w, d, hh) = (rng.range(12.0, 16.0), rng.range(10.0, 12.0), rng.range(7.0, 10.0));
    let at = |dx: f32, dy: f32, dz: f32| add((x, 0.0, z), rotate((dx, dy, dz), (0.0, yaw, 0.0)));
    let body = b.part(h, "Walls", at(0.0, hh / 2.0, 0.0), (w, hh, d), wall, Material::Brick);
    b.turned(body, (0.0, yaw, 0.0));
    for (dz, turn) in [(-d / 4.0, 0.0), (d / 4.0, 180.0)] {
        let r = b.part(h, "Roof", at(0.0, hh + 2.0, dz), (w + 1.0, 4.0, d / 2.0 + 0.5), roof, Material::Wood);
        b.shape(r, Shape::Wedge);
        b.turned(r, (0.0, yaw + turn, 0.0));
    }
    // The front faces -Z of the house (turned by yaw): a door and two lit windows.
    let door = b.part(h, "Door", at(0.0, 2.0, -d / 2.0 - 0.1), (2.2, 4.0, 0.2), (90, 60, 40), Material::Wood);
    b.turned(door, (0.0, yaw, 0.0));
    for dx in [-w / 3.0, w / 3.0] {
        let win = b.part(h, "Window", at(dx, hh * 0.55, -d / 2.0 - 0.1), (2.4, 2.2, 0.2), (255, 230, 150), Material::Neon);
        b.turned(win, (0.0, yaw, 0.0));
    }
}

/// Trees, rocks, the town, lamp posts, and the barn on the shortcut.
fn build_scenery(b: &mut B, t: &[Sample], rng: &mut Rand) {
    let root = b.root();
    let scenery = b.folder(root, "Scenery");

    // The town: two rows of houses along the street (z = 440).
    let town = b.folder(scenery, "Town");
    let mut x = -215.0;
    while x < 30.0 {
        for (z, yaw) in [(478.0, 180.0), (404.0, 0.0)] {
            let p = (x + rng.range(-3.0, 3.0), z + rng.range(-2.0, 2.0));
            if clear(t, p, 9.0) {
                house(b, town, p.0, p.1, yaw, rng);
            }
        }
        x += 24.0;
    }

    // Lamp posts along the track (they glow at night).
    let lamps = b.folder(scenery, "Lamps");
    for (n, i) in (0..t.len()).step_by(12).enumerate() {
        let s = t[i];
        let side = if n % 2 == 0 { 1.0 } else { -1.0 };
        let at = add(s.at, rotate((side * (WIDTH / 2.0 + 3.5), 0.0, 0.0), (0.0, s.yaw, 0.0)));
        let p = (at.0, at.2);
        if in_water(p, 3.0) || in_rect(p, PLAZA, 2.0) || seg_dist(p, CUT_FROM, CUT_TO) < 10.0 || in_rect(p, (-125.0, 125.0, -85.0, -15.0), 0.0) {
            continue;
        }
        if t.iter().any(|q| flat_dist((q.at.0, q.at.2), p) < WIDTH / 2.0 + 2.0) {
            continue;
        }
        let base = at.1 + LIFT;
        b.part(lamps, "Lamp Post", (at.0, base + 5.0, at.2), (0.6, 10.0, 0.6), (40, 44, 52), Material::Metal);
        let lamp = b.part(lamps, "Lamp", (at.0, base + 10.4, at.2), (1.6, 1.0, 1.6), (255, 236, 170), Material::Neon);
        b.shape(lamp, Shape::Ball);
    }

    // Trees everywhere there's room.
    let trees = b.folder(scenery, "Trees");
    let mut placed = 0;
    let mut tries = 0;
    while placed < 170 && tries < 5000 {
        tries += 1;
        let p = (rng.range(-470.0, 470.0), rng.range(-230.0, 610.0));
        if clear(t, p, 5.0) {
            tree(b, trees, p.0, p.1, rng);
            placed += 1;
        }
    }
    // Boulders on the canyon rims.
    let rocks = b.folder(scenery, "Rocks");
    for _ in 0..40 {
        let x = rng.range(-470.0, -120.0);
        let z = if rng.next() < 0.5 { RIVER.2 - rng.range(2.0, 7.0) } else { RIVER.3 + rng.range(2.0, 7.0) };
        let p = (x, z);
        if in_water(p, 0.5) || t.iter().any(|s| flat_dist((s.at.0, s.at.2), p) < WIDTH / 2.0 + 4.0) {
            continue;
        }
        let s = rng.range(2.5, 6.0);
        let shade = rng.range(0.8, 1.1);
        let id = b.part(rocks, "Rock", (x, s * 0.3, z), (s * 1.3, s, s), ((ROCK.0 as f32 * shade) as u8, (ROCK.1 as f32 * shade) as u8, (ROCK.2 as f32 * shade) as u8), Material::Concrete);
        b.shape(id, Shape::Ball);
        b.turned(id, (rng.range(-20.0, 20.0), rng.range(0.0, 360.0), rng.range(-20.0, 20.0)));
    }

    // The barn shortcut: a dirt lane through a barn, with hay bales in the way.
    let barn = b.dm.create(Class::Model, "Barn", root).unwrap();
    let a = (CUT_FROM.0, 0.0, CUT_FROM.1);
    let c = (CUT_TO.0, 0.0, CUT_TO.1);
    let lane = b.beam(barn, "Dirt Lane", add(a, (0.0, 0.07, 0.0)), add(c, (0.0, 0.07, 0.0)), 12.0, 0.14, (140, 104, 64), Material::Wood);
    b.ghost(lane, 0.0);
    let d = sub(c, a);
    let yaw = d.0.atan2(d.2).to_degrees();
    let mid = mul(add(a, c), 0.5);
    let at = |dx: f32, dy: f32, dz: f32| add(mid, rotate((dx, dy, dz), (0.0, yaw, 0.0)));
    let red: Rgb = (170, 40, 36);
    for side in [1.0f32, -1.0] {
        let w = b.part(barn, "Barn Wall", at(side * 8.0, 6.0, 0.0), (1.0, 12.0, 30.0), red, Material::Wood);
        b.turned(w, (0.0, yaw, 0.0));
        let r = b.part(barn, "Barn Roof", at(side * 4.6, 14.0, 0.0), (10.5, 0.8, 32.0), (60, 60, 64), Material::Metal);
        b.turned(r, (0.0, yaw, side * -28.0));
        // White trim at the doors.
        for dz in [-15.2, 15.2] {
            let trim = b.part(barn, "Trim", at(side * 7.0, 6.0, dz), (1.2, 12.0, 0.4), (240, 240, 240), Material::Wood);
            b.turned(trim, (0.0, yaw, 0.0));
        }
    }
    let gable_top = b.part(barn, "Barn Beam", at(0.0, 16.4, 0.0), (0.8, 0.8, 32.0), (240, 240, 240), Material::Wood);
    b.turned(gable_top, (0.0, yaw, 0.0));
    for (dx, dz) in [(-3.0, -7.0), (3.0, 1.0), (-2.5, 9.0)] {
        let h = b.part(barn, "Hay Bale", at(dx, 1.3, dz), (3.2, 2.6, 4.2), (225, 190, 90), Material::Grass);
        b.turned(h, (0.0, yaw + rng.range(-12.0, 12.0), 0.0));
    }
    // The slow zone (scripts read it): the middle of the barn.
    let zone = b.part(barn, "Barn Floor", at(0.0, 2.0, 0.0), (15.0, 4.0, 30.0), (0, 0, 0), Material::Plastic);
    b.turned(zone, (0.0, yaw, 0.0));
    b.ghost(zone, 1.0);
    b.attr(zone, "slow_radius", Attribute::Num(24.0));
}

/// The grandstand, the crowd, its roof and the jumbotron; the start
/// gantry and the chequered line.
fn build_venue(b: &mut B, rng: &mut Rand) {
    let root = b.root();
    let venue = b.folder(root, "Grandstand");
    let (x0, x1) = (-110.0f32, 110.0f32);
    let tiers = 8;
    for k in 0..tiers {
        let top = 1.2 + k as f32 * 2.2;
        let z = -22.0 - k as f32 * 5.0;
        let shade = if k % 2 == 0 { CONCRETE } else { (150, 148, 144) };
        b.part(venue, "Tier", ((x0 + x1) / 2.0, top / 2.0, z - 2.5), (x1 - x0, top, 5.0), shade, Material::Concrete);
        // Seat rows: team colours in blocks.
        for s in 0..11 {
            let sx = x0 + 10.0 + s as f32 * 20.0;
            let colour = if s % 2 == 0 { (30, 110, 220) } else { (220, 45, 40) };
            b.part(venue, "Seats", (sx, top + 0.3, z - 3.6), (18.0, 0.6, 1.4), colour, Material::Plastic);
        }
    }
    let back_z = -22.0 - tiers as f32 * 5.0;
    b.part(venue, "Back Wall", (0.0, 14.0, back_z - 1.0), (x1 - x0 + 4.0, 28.0, 2.0), (45, 60, 90), Material::Concrete);
    b.part(venue, "Roof", (0.0, 28.5, (back_z - 18.0) / 2.0), (x1 - x0 + 6.0, 1.0, -back_z + 18.0 - 14.0), (240, 240, 240), Material::Metal);
    b.part(venue, "Roof Trim", (0.0, 28.5, -18.5), (x1 - x0 + 6.0, 1.6, 1.2), (220, 45, 40), Material::Metal);
    for x in [x0 - 1.0, -37.0, 37.0, x1 + 1.0] {
        b.part(venue, "Roof Post", (x, 14.0, -19.0), (1.0, 28.0, 1.0), (240, 240, 240), Material::Metal);
    }
    // A fence between the track and the stands.
    let fence = b.part(venue, "Fence", (0.0, 3.0, -18.0), (x1 - x0, 6.0, 0.3), (200, 200, 210), Material::Metal);
    b.ghost(fence, 0.55);
    // Floodlights at the ends (they glow at night).
    for x in [x0 - 8.0, x1 + 8.0] {
        b.part(venue, "Light Tower", (x, 20.0, -30.0), (1.4, 40.0, 1.4), (70, 74, 82), Material::Metal);
        b.part(venue, "Floodlight", (x, 40.5, -29.0), (7.0, 3.0, 1.2), (255, 250, 220), Material::Neon);
    }

    // The crowd: a body and a head each, shirts of every colour.
    let crowd = b.folder(root, "Crowd");
    for k in 0..tiers {
        let top = 1.2 + k as f32 * 2.2;
        let z = -22.0 - k as f32 * 5.0 - 2.0;
        let mut x = x0 + 3.0 + rng.range(0.0, 4.0);
        while x < x1 - 3.0 {
            if rng.next() < 0.78 {
                let fan = b.dm.create(Class::Model, "Fan", crowd).unwrap();
                let shirt = ((rng.next() * 230.0) as u8 + 20, (rng.next() * 230.0) as u8 + 20, (rng.next() * 230.0) as u8 + 20);
                let skins: [Rgb; 4] = [(245, 205, 160), (215, 160, 120), (160, 110, 75), (105, 70, 50)];
                let skin = skins[(rng.next() * 4.0) as usize % 4];
                b.part(fan, "Body", (x, top + 1.2, z), (1.6, 2.2, 1.0), shirt, Material::Plastic);
                let head = b.part(fan, "Head", (x, top + 2.9, z), (1.2, 1.2, 1.2), skin, Material::Plastic);
                b.shape(head, Shape::Ball);
            }
            x += rng.range(2.4, 3.6);
        }
    }

    // The jumbotron, at the east end of the stands, facing the straight.
    let tron = b.dm.create(Class::Model, "Jumbotron", root).unwrap();
    for dx in [-12.0, 12.0] {
        b.part(tron, "Leg", (145.0 + dx, 13.0, -45.0), (1.6, 26.0, 1.6), (60, 64, 72), Material::Metal);
    }
    b.part(tron, "Frame", (145.0, 34.0, -45.0), (38.0, 18.0, 2.0), (30, 32, 38), Material::Metal);
    let screen = b.part(tron, "Screen", (145.0, 34.0, -43.9), (35.0, 15.5, 0.3), (10, 20, 40), Material::Neon);
    let label = b.label(root, "Jumbotron Text", "BRICKPORT SPEEDWAY", (0.0, 0.0, 0.15, 0.17), 13.0, (255, 255, 255), Some((10, 20, 40)));
    b.dm.gui_mut(label).unwrap().attached_to = Some(screen);

    // The start gantry over the line, with five lights.
    let gantry = b.dm.create(Class::Model, "Start Gantry", root).unwrap();
    for z in [-16.5, 16.5] {
        b.part(gantry, "Gantry Leg", (0.0, 8.0, z), (1.4, 16.0, 1.4), (40, 44, 52), Material::Metal);
    }
    b.part(gantry, "Gantry Beam", (0.0, 15.0, 0.0), (2.0, 2.4, 34.4), (40, 44, 52), Material::Metal);
    let banner_part = b.part(gantry, "Gantry Banner", (0.3, 18.2, 0.0), (0.4, 4.0, 30.0), NAVY, Material::Plastic);
    let banner_label = b.label(root, "Gantry Sign", "BRICKPORT SPEEDWAY", (0.0, 0.0, 0.16, 0.04), 16.0, GOLD, Some(NAVY));
    b.dm.gui_mut(banner_label).unwrap().attached_to = Some(banner_part);
    let lights = b.folder(gantry, "Lights");
    for k in 0..5 {
        let z = -8.0 + k as f32 * 4.0;
        b.part(gantry, "Light Box", (-1.0, 15.0, z), (0.6, 2.6, 2.6), (20, 20, 24), Material::Metal);
        let l = b.part(lights, &format!("Light {}", k + 1), (-1.4, 15.0, z), (1.9, 1.9, 1.9), (70, 12, 12), Material::Neon);
        b.shape(l, Shape::Ball);
    }
    // The chequered line.
    let line = b.folder(root, "Finish Line");
    for row in 0..2 {
        for k in 0..14 {
            let white = (row + k) % 2 == 0;
            let id = b.part(line, "Check", (-1.0 + row as f32 * 2.0, LIFT + 0.03, -13.0 + k as f32 * 2.0), (2.0, 0.06, 2.0), if white { (245, 245, 245) } else { (20, 20, 24) }, Material::Plastic);
            b.ghost(id, 0.0);
        }
    }
}

/// The pits (where you join), the garages and the podium.
fn build_pits(b: &mut B) {
    let root = b.root();
    let pits = b.folder(root, "Pits");
    let (x0, x1, z0, z1) = PLAZA;
    b.part(pits, "Pit Lane", ((x0 + x1) / 2.0, LIFT / 2.0 - 0.02, (z0 + z1) / 2.0), (x1 - x0, LIFT, z1 - z0), (80, 82, 90), Material::Concrete);
    // Pit boxes painted on the lane.
    for k in 0..8 {
        let x = x0 + 10.0 + k as f32 * 17.0;
        let id = b.part(pits, "Pit Box", (x, LIFT + 0.02, 48.0), (9.0, 0.04, 9.0), KART_COLORS[k].1, Material::Plastic);
        b.ghost(id, 0.3);
    }
    // Garages behind, and low walls round the sides.
    b.part(pits, "Garage Back", ((x0 + x1) / 2.0, 6.0, z1 + 14.0), (x1 - x0, 12.0, 1.0), (220, 222, 228), Material::Concrete);
    b.part(pits, "Garage Roof", ((x0 + x1) / 2.0, 12.4, z1 + 7.0), (x1 - x0 + 2.0, 0.8, 16.0), NAVY, Material::Metal);
    for k in 0..=8 {
        let x = x0 + k as f32 * (x1 - x0) / 8.0;
        b.part(pits, "Garage Divider", (x, 6.0, z1 + 7.0), (0.6, 12.0, 14.0), (220, 222, 228), Material::Concrete);
    }
    for x in [x0 - 0.75, x1 + 0.75] {
        b.part(pits, "Pit Wall", (x, 1.5, (z0 + 6.0 + z1) / 2.0), (1.5, 3.0, z1 - z0 - 6.0), (205, 40, 40), Material::Plastic);
    }
    let spawn = b.dm.create(Class::SpawnLocation, "Pit Spawn", pits).unwrap();
    {
        let p = b.dm.part_mut(spawn).unwrap();
        p.position = Vec3::new(0.0, 0.3, 32.0);
        p.size = Vec3::new(10.0, 0.4, 6.0);
        p.color = Color::new(GOLD.0, GOLD.1, GOLD.2);
        p.material = Material::Neon;
    }

    // The podium.
    let podium = b.dm.create(Class::Model, "Podium", root).unwrap();
    let (px, pz) = PODIUM;
    b.part(podium, "Stage", (px, 0.4, pz), (34.0, 0.8, 16.0), NAVY, Material::Concrete);
    for (name, dx, h, c) in [("Step 1", 0.0, 6.0, GOLD), ("Step 2", 9.0, 4.2, (200, 200, 210)), ("Step 3", -9.0, 2.8, (205, 127, 50))] {
        b.part(podium, name, (px + dx, 0.8 + h / 2.0, pz), (8.0, h, 8.0), c, Material::Metal);
    }
    b.part(podium, "Podium Back", (px, 8.0, pz + 7.0), (34.0, 16.0, 1.0), (30, 30, 36), Material::Plastic);
    let stripe = b.part(podium, "Podium Stripe", (px, 14.0, pz + 6.4), (34.0, 2.0, 0.3), GOLD, Material::Neon);
    let sign = b.label(root, "Podium Sign", "WINNERS", (0.0, 0.0, 0.1, 0.045), 20.0, NAVY, Some(GOLD));
    b.dm.gui_mut(sign).unwrap().attached_to = Some(stripe);
    for dx in [-20.0, 20.0] {
        b.part(podium, "Launcher", (px + dx, 1.0, pz + 2.0), (2.0, 2.0, 2.0), (40, 40, 44), Material::Metal);
    }
}

/// A kart facing +Z with its chassis at `at`, in `colour` with `trim`.
pub fn build_kart(dm: &mut DataModel, parent: InstanceId, name: &str, at: V, colour: Rgb, trim: Rgb) -> InstanceId {
    let mut b = B { dm: std::mem::take(dm) };
    let k = b.dm.create(Class::Model, name, parent).unwrap();
    b.attr(k, "kart", Attribute::Bool(true));
    let p = |dx: f32, dy: f32, dz: f32| (at.0 + dx, at.1 + dy, at.2 + dz);
    let dark: Rgb = (30, 30, 34);
    let metal: Rgb = (160, 164, 172);
    b.part(k, "Chassis", p(0.0, 0.0, 0.0), (3.2, 0.6, 5.2), colour, Material::Plastic);
    let nose = b.part(k, "Nose", p(0.0, 0.05, 3.25), (3.0, 0.7, 1.4), colour, Material::Plastic);
    b.shape(nose, Shape::Wedge);
    b.turned(nose, (0.0, 180.0, 0.0));
    b.part(k, "Bumper", p(0.0, -0.1, 4.1), (3.6, 0.4, 0.4), trim, Material::Plastic);
    b.part(k, "Seat", p(0.0, 0.5, -0.5), (1.6, 0.4, 1.6), dark, Material::Plastic);
    b.part(k, "Seat Back", p(0.0, 1.1, -1.35), (1.6, 1.4, 0.3), dark, Material::Plastic);
    let wheel = b.part(k, "Steering Wheel", p(0.0, 1.0, 0.9), (0.9, 0.15, 0.9), dark, Material::Plastic);
    b.shape(wheel, Shape::Cylinder);
    b.turned(wheel, (-55.0, 0.0, 0.0));
    for side in [1.0f32, -1.0] {
        b.part(k, "Side Pod", p(side * 1.85, -0.05, 0.0), (0.6, 0.5, 2.8), trim, Material::Plastic);
        let pipe = b.part(k, "Exhaust", p(side * 0.55, 0.7, -2.9), (0.35, 0.9, 0.35), metal, Material::Metal);
        b.shape(pipe, Shape::Cylinder);
        b.turned(pipe, (70.0, 0.0, 0.0));
        b.part(k, "Spoiler Strut", p(side * 1.0, 1.2, -2.6), (0.15, 1.0, 0.15), dark, Material::Metal);
    }
    b.part(k, "Engine", p(0.0, 0.6, -2.2), (1.4, 0.8, 1.0), metal, Material::Metal);
    b.part(k, "Spoiler", p(0.0, 1.75, -2.65), (3.4, 0.15, 0.9), trim, Material::Plastic);
    for (n, dx, dz, r, w) in [("Front Wheel L", 1.95, 1.7, 0.9, 0.8), ("Front Wheel R", -1.95, 1.7, 0.9, 0.8), ("Wheel L", 1.95, -1.7, 0.95, 1.0), ("Wheel R", -1.95, -1.7, 0.95, 1.0)] {
        // Wheel bottoms touch the road (the chassis sits 1.1 above it).
        let id = b.part(k, n, p(dx, r - 1.1, dz), (r * 2.0, w, r * 2.0), dark, Material::Plastic);
        b.shape(id, Shape::Cylinder);
        b.turned(id, (0.0, 0.0, 90.0));
        let hub = b.part(k, &format!("{n} Hub"), p(dx + dx.signum() * (w / 2.0), r - 1.1, dz), (r * 0.9, 0.1, r * 0.9), metal, Material::Metal);
        b.shape(hub, Shape::Cylinder);
        b.turned(hub, (0.0, 0.0, 90.0));
    }
    *dm = b.dm;
    k
}

/// The main straight's grid slot `i` (0 is pole): chassis position.
pub fn grid_slot(i: usize) -> V {
    (-14.0 - i as f32 * 9.0, LIFT + 1.1 + 0.2, if i % 2 == 0 { -5.5 } else { 5.5 })
}

fn build_race_bits(b: &mut B, t: &[Sample]) {
    let root = b.root();
    let m = t.len();

    // Checkpoints: gates the race counts you through, in order; 1 is the
    // finish line. None over the canyon or where the barn skips to.
    let cps = b.folder(root, "Checkpoints");
    let skip_cut = |p: V| p.0 < -100.0 && p.0 > -290.0 && p.2 > -20.0 && p.2 < 90.0;
    let mut n = 0;
    let mut i = 0;
    while i < m {
        let s = t[i];
        let p = (s.at.0, s.at.2);
        let near_gap = p.0 < -200.0 && p.1 > LIP_Z - 30.0 && p.1 < LAND_Z + 30.0;
        if (near_gap || skip_cut(s.at)) && i != 0 {
            i += 1;
            continue;
        }
        n += 1;
        let id = b.part(cps, &n.to_string(), add(s.at, (0.0, LIFT + 4.0, 0.0)), (WIDTH + 4.0, 8.0, 2.0), (255, 255, 0), Material::Neon);
        b.ghost(id, 1.0);
        b.turned(id, (0.0, s.yaw, 0.0));
        b.attr(id, "index", Attribute::Num(n as f64));
        i += 18;
    }

    // The racing line bots follow.
    let line = b.folder(root, "Racing Line");
    for (n, i) in (0..m).step_by(3).enumerate() {
        let s = t[i];
        let id = b.part(line, &(n + 1).to_string(), add(s.at, (0.0, LIFT + 1.0, 0.0)), (1.0, 1.0, 1.0), (255, 0, 255), Material::Neon);
        b.ghost(id, 1.0);
    }
    b.attr(root, "racing_line", Attribute::Str("Racing Line".into()));

    // The grid.
    let grid = b.folder(root, "Grid");
    for k in 0..8 {
        let at = grid_slot(k);
        let id = b.part(grid, &format!("Slot {}", k + 1), (at.0, LIFT + 0.03, at.2), (5.0, 0.06, 0.4), (245, 245, 245), Material::Plastic);
        b.ghost(id, 0.0);
        b.turned(id, (0.0, 90.0, 0.0));
    }

    // Boost pads and item boxes, at points round the lap.
    let pads = b.folder(root, "Boost Pads");
    let boxes = b.folder(root, "Item Boxes");
    let at_frac = |f: f32| t[((m as f32 * f) as usize) % m];
    for f in [0.215, 0.44, 0.62, 0.9] {
        let s = at_frac(f);
        for off in [-6.0f32, 6.0] {
            let at = add(add(s.at, rotate((off, 0.0, 0.0), (s.pitch, s.yaw, s.bank))), (0.0, LIFT + 0.06, 0.0));
            let id = b.part(pads, "Boost Pad", at, (7.0, 0.12, 8.0), (255, 150, 30), Material::Neon);
            b.turned(id, (s.pitch, s.yaw, s.bank));
            b.ghost(id, 0.0);
            // What karts touch: a taller, invisible box over the pad.
            let trigger = b.part(id, "Boost Trigger", add(at, (0.0, 1.0, 0.0)), (7.0, 3.0, 8.0), (255, 150, 30), Material::Neon);
            b.turned(trigger, (s.pitch, s.yaw, s.bank));
            b.ghost(trigger, 1.0);
            b.script(trigger, "Boost", BOOST_PAD);
        }
    }
    for f in [0.12, 0.35, 0.56, 0.8] {
        let s = at_frac(f);
        for (k, off) in [-9.0f32, -3.0, 3.0, 9.0].into_iter().enumerate() {
            let at = add(add(s.at, rotate((off, 0.0, 0.0), (0.0, s.yaw, 0.0))), (0.0, LIFT + 2.2, 0.0));
            let colours: [Rgb; 4] = [(255, 90, 90), (90, 200, 255), (255, 220, 70), (140, 255, 120)];
            let id = b.part(boxes, "Item Box", at, (2.6, 2.6, 2.6), colours[k], Material::Neon);
            b.turned(id, (35.0, s.yaw + 45.0, 35.0));
            b.ghost(id, 0.2);
            b.script(id, "Box", ITEM_BOX);
        }
    }
}

const BOOST_PAD: &str = r#"
-- Drive over it: a burst of speed.
on touched(other)
    if other.class == "player" and other.kart != nil then
        boost(other.kart, 1.2)
        play_sound("whoosh", other)
    end
end
"#;

const ITEM_BOX: &str = r#"
-- Drive through it for an item (E to use it). It comes back after a bit.
ITEMS = ["boost", "boost", "rocket", "rocket", "oil", "oil", "shield"]
on touched(other)
    if self.transparency > 0.9 or other.class != "player" or other.kart == nil or other.item != nil then
        return
    end
    self.transparency = 1
    pick = ITEMS[random(1, len(ITEMS))]
    -- Out in front, no rockets: those are for catching up.
    if other.race_place != nil and other.race_place == 1 and pick == "rocket" then
        pick = "oil"
    end
    -- At the back, more boosts.
    if other.race_place != nil and other.race_place >= 6 and random(1, 3) == 1 then
        pick = "boost"
    end
    other.item = pick
    play_sound("pop", other)
    wait(3)
    self.transparency = 0.2
end
"#;

/// The race: the lobby, the grid, the lights, laps, items, the podium.
pub const RACE: &str = r#"-- Brickport Speedway: kart racing for up to 8, bots filling the grid.
--
-- Between races (the lobby): walk round the pits, get in any kart with F
-- and drive the track, or practise in the Drift Park (always open). Click
-- "Race next" (or press G) to sit the next race out.
-- A race: a flyover of the track, the grid, five red lights, three laps,
-- the podium. Then back to the lobby.
LAPS = 3
BOT_NAMES = ["Bolt", "Turbo", "Nitro", "Blaze", "Zippy", "Comet", "Rocket", "Dash", "Sparky", "Flash"]
TIMES = [18.0, 21.8, 13.5]
ITEM_NAMES = {boost = "BOOST", rocket = "ROCKET", mine = "SPIKE MINE", shield = "SHIELD"}
world = find("Workspace")
world.phase = "lobby"
-- How long between races (a Workspace field, lobby_seconds, can change it).
LOBBY_SECONDS = world.lobby_seconds or 45

-- The checkpoints, in order, with which way each faces.
fn read_checkpoints()
    list = []
    for cp in find("Checkpoints").children do
        facing = cp.rotation.y / 57.2958
        push(list, {x = cp.position.x, y = cp.position.y - 4, z = cp.position.z, fx = sin(facing), fz = cos(facing), yaw = cp.rotation.y})
    end
    return list
end
cps = read_checkpoints()
NCP = len(cps)
karts = find("Race Karts").children
practice = find("Practice Karts").children
lights = find("Lights").children
fans = find("Crowd").children
barn = find("Barn Floor")
park = find("Drift Park Floor")
flycam = find("Flyover Camera")

-- Everyone racing, by name: {p, kart, ch (chassis), passed, next, ...}
racers = {}
order = []
fx = []
race_no = 0
go_time = 0
first_finish = 0
finished_count = 0
cheer_until = 0
lobby_ends = 0
final_lap_on = false
retire_asked = {}

fn chassis_of(kart)
    for c in kart.children do
        if c.name == "Chassis" then
            return c
        end
    end
    return nil
end

fn fmt(t)
    if t == nil then
        return "-"
    end
    mins = floor(t / 60)
    secs = t - mins * 60
    whole = floor(secs)
    tenth = floor((secs - whole) * 10)
    pad = ""
    if whole < 10 then
        pad = "0"
    end
    return mins + ":" + pad + whole + "." + tenth
end

fn say(text)
    banner = find("Banner")
    banner.text = text
    banner.visible = text != ""
end

fn is_bot(p)
    return p.bot == true
end

fn humans()
    list = []
    for p in players() do
        if not is_bot(p) then
            push(list, p)
        end
    end
    return list
end

fn bots()
    list = []
    for p in players() do
        if is_bot(p) then
            push(list, p)
        end
    end
    return list
end

fn is_practice(kart)
    return kart != nil and kart.practice == true
end

fn free_race_kart()
    for k in karts do
        if k.driver == nil then
            return k
        end
    end
    return nil
end

fn child(p, name)
    for c in p.children do
        if c.name == name then
            return c
        end
    end
    return nil
end

-- Whether someone's in for the next race (everyone is, until they say no).
fn racing_next(p)
    return p.sit_out != true
end

-- Each human's own screen: lap, place, times, their item, and the button
-- to sit a race out.
fn make_hud(p)
    hud = create("TextLabel", p)
    hud.name = "Race HUD"
    hud.x = 0.01
    hud.y = 0.12
    hud.width = 0.2
    hud.height = 0.11
    hud.text_size = 18
    hud.background = true
    hud.background_color = {r = 13, g = 42, b = 74}
    hud.text_color = {r = 255, g = 255, b = 255}
    hud.text = ""
    hud.visible = false
    item = create("TextLabel", p)
    item.name = "Item Slot"
    item.x = 0.41
    item.y = 0.84
    item.width = 0.18
    item.height = 0.06
    item.text_size = 20
    item.background = true
    item.background_color = {r = 245, g = 205, b = 48}
    item.text_color = {r = 13, g = 42, b = 74}
    item.visible = false
    button = clone(find("Race Toggle Template"))
    button.name = "Race Toggle"
    button.parent = p
    button.visible = true
end

-- Puts a player in a kart at a spot.
fn seat(p, kart, at, facing)
    place_kart(kart, at, facing)
    p.kart = kart
end

-- Where race kart n parks in the pits, and where bots wait between races.
fn pit_box(n)
    return {x = -60 + (n - 1) * 17, y = 1.5, z = 48}
end

fn slot_at(i)
    s = find("Slot " + i)
    return {x = s.position.x, y = 1.5, z = s.position.z}
end

-- Out of whatever kart they're in, standing at `at` (a moment later, once
-- the engine's let them out).
fn to_foot(p, at)
    if p.kart != nil then
        p.kart = nil
        wait(0.25)
    end
    if at != nil then
        p.position = at
    end
end

-- F: in or out of the nearest kart.
fn toggle_kart(p)
    if p.kart != nil then
        s = racers[p.name]
        if s != nil and world.phase == "race" and not s.done then
            -- Leaving a race: ask first.
            if retire_asked[p.name] == nil or time() - retire_asked[p.name] > 3 then
                retire_asked[p.name] = time()
                tell(p, "Press F again to leave the race")
                return
            end
            remove(racers, p.name)
            drop_from_order(p.name)
            k = p.kart
            p.kart = nil
            wait(0.3)
            park_karts()
            tell(p, "You left the race")
            return
        end
        if s != nil and (world.phase == "intro" or world.phase == "grid") then
            tell(p, "The race is about to start!")
            return
        end
        p.kart = nil
        return
    end
    best = nil
    best_d = 81
    for k in karts do
        best_d = nearer(p, k, best_d)
        if best_d < 0 then
            best = k
            best_d = -best_d
        end
    end
    for k in practice do
        best_d = nearer(p, k, best_d)
        if best_d < 0 then
            best = k
            best_d = -best_d
        end
    end
    if best == nil then
        tell(p, "Walk up to a kart, then press F")
        return
    end
    if not is_practice(best) and world.phase != "lobby" then
        tell(p, "Those karts are racing: try the Drift Park!")
        return
    end
    best.locked = nil
    best.top_speed = nil
    p.kart = best
end

-- Gives back minus the squared distance to kart k if it's free and nearer
-- than `best` (squared), otherwise `best`.
fn nearer(p, k, best)
    if k.driver != nil then
        return best
    end
    c = chassis_of(k)
    dx = c.position.x - p.position.x
    dy = c.position.y - p.position.y
    dz = c.position.z - p.position.z
    d = dx * dx + dy * dy + dz * dz
    if d < best then
        return -d
    end
    return best
end

fn drop_from_order(name)
    list = []
    for n in order do
        if n != name then
            push(list, n)
        end
    end
    order = list
end

-- A message just for one player, for a few seconds.
fn tell(p, text)
    note = child(p, "Note")
    if note == nil then
        note = create("TextLabel", p)
        note.name = "Note"
        note.x = 0.33
        note.y = 0.3
        note.width = 0.34
        note.height = 0.06
        note.text_size = 22
        note.background = true
        note.background_color = {r = 13, g = 42, b = 74}
        note.text_color = {r = 245, g = 205, b = 48}
    end
    note.text = text
    note.visible = true
    p.note_until = time() + 3
end

-- The race karts back in the pits, empty, ready for anyone.
fn park_karts()
    n = 0
    for k in karts do
        n += 1
        if k.driver == nil then
            place_kart(k, pit_box(n), 90)
        end
        k.locked = nil
        k.top_speed = nil
    end
end

-- A mine: a glowing red orb with a pulsing ring round it.
fn drop_mine(x, y, z, owner)
    m = create("Model", find("Effects"))
    m.name = "Spike Mine"
    orb = create("Part", m)
    orb.name = "Orb"
    orb.shape = "ball"
    orb.material = "neon"
    orb.color = {r = 255, g = 40, b = 30}
    orb.size = {x = 2.4, y = 2.4, z = 2.4}
    orb.position = {x = x, y = y + 1.3, z = z}
    orb.can_collide = false
    orb.anchored = true
    for i in 1..4 do
        spike = create("Part", m)
        spike.name = "Spike"
        spike.material = "neon"
        spike.color = {r = 255, g = 220, b = 40}
        spike.size = {x = 0.4, y = 3.6, z = 0.4}
        spike.position = {x = x, y = y + 1.3, z = z}
        spike.rotation = {x = 45 * (i % 2), y = 45 * i, z = 45 * floor(i / 2)}
        spike.can_collide = false
        spike.anchored = true
    end
    ring = create("Part", m)
    ring.name = "Ring"
    ring.shape = "cylinder"
    ring.material = "neon"
    ring.color = {r = 255, g = 60, b = 40}
    ring.size = {x = 6, y = 0.1, z = 6}
    ring.position = {x = x, y = y + 0.08, z = z}
    ring.transparency = 0.4
    ring.can_collide = false
    ring.anchored = true
    push(fx, {kind = "mine", part = m, ring = ring, owner = owner, born = time(), x = x, y = y, z = z})
end

fn progress(s)
    if s.done then
        return 1000000 - s.finish
    end
    c = cps[s.next]
    dx = s.ch.position.x - c.x
    dz = s.ch.position.z - c.z
    return s.passed * 1000 - sqrt(dx * dx + dz * dz)
end

-- Sorts the racers into order, best first.
fn rank()
    list = []
    for name in racers do
        s = racers[name]
        s.score = progress(s)
        i = 1
        while i <= len(list) and racers[list[i]].score >= s.score do
            i += 1
        end
        insert(list, i, name)
    end
    order = list
    n = 0
    for name in order do
        n += 1
        racers[name].place = n
        racers[name].p.race_place = n
    end
end

-- Back on the track at the last checkpoint passed.
fn respawn(s)
    if time() - s.respawned < 1.5 then
        return
    end
    s.respawned = time()
    i = s.next - 1
    if i < 1 then
        i = NCP
    end
    if s.passed == 0 then
        place_kart(s.kart, slot_at(1), 90)
        return
    end
    c = cps[i]
    place_kart(s.kart, {x = c.x, y = c.y + 1.8, z = c.z}, c.yaw)
end

fn ordinal(n)
    if n == 1 then
        return "1st"
    elseif n == 2 then
        return "2nd"
    elseif n == 3 then
        return "3rd"
    end
    return n + "th"
end

fn finish(s)
    s.done = true
    s.finish = time() - go_time
    finished_count += 1
    if finished_count == 1 then
        first_finish = time()
        cheer_until = time() + 5
        say(s.p.name + " wins!")
        play_sound("win")
    else
        play_sound("coin", s.p)
    end
    s.kart.top_speed = 30
end

-- Through a checkpoint? Count it, and the laps.
fn check_gates(s)
    if s.done then
        return
    end
    c = cps[s.next]
    dx = s.ch.position.x - c.x
    dz = s.ch.position.z - c.z
    if abs(s.ch.position.y - c.y) > 9 or dx * dx + dz * dz > 38 * 38 or dx * c.fx + dz * c.fz < 0 then
        return
    end
    s.passed += 1
    s.next = s.next % NCP + 1
    if s.next == 2 and s.passed > 1 then
        lap = time() - s.lap_start
        s.lap_start = time()
        if s.best == nil or lap < s.best then
            s.best = lap
        end
        if s.passed >= LAPS * NCP + 1 then
            finish(s)
        elseif s.passed == (LAPS - 1) * NCP + 1 and not is_bot(s.p) then
            play_sound("twang", s.p)
        end
    end
end

fn hit(s)
    if s.shield > time() then
        s.shield = 0
        if s.bubble != nil then
            destroy(s.bubble)
            s.bubble = nil
        end
        play_sound("bonk", s.p)
        return
    end
    spin_out(s.kart)
    play_sound("splat", s.p)
end

-- Using what's in your item slot.
fn use_item(p)
    s = racers[p.name]
    if s == nil or p.item == nil or world.phase != "race" then
        return
    end
    what = p.item
    p.item = nil
    at = s.ch.position
    yaw = s.ch.rotation.y / 57.2958
    fwd = {x = sin(yaw), z = cos(yaw)}
    if what == "boost" then
        boost(s.kart, 1.6)
        play_sound("whoosh", p)
    elseif what == "shield" then
        s.shield = time() + 10
        if s.bubble != nil then
            destroy(s.bubble)
        end
        b = create("Part", find("Effects"))
        b.name = "Shield"
        b.shape = "ball"
        b.material = "neon"
        b.color = {r = 90, g = 200, b = 255}
        b.transparency = 0.7
        b.size = {x = 7, y = 7, z = 7}
        b.can_collide = false
        b.anchored = true
        b.carried_by = p.name
        b.carry_y = -1.5
        s.bubble = b
        play_sound("pop", p)
    elseif what == "mine" then
        drop_mine(at.x - fwd.x * 6, at.y - 1.1, at.z - fwd.z * 6, p.name)
        play_sound("click", p)
    elseif what == "rocket" then
        -- Homes in on whoever's just ahead (straight on, if nobody is).
        target = nil
        if s.place != nil and s.place > 1 then
            target = order[s.place - 1]
        end
        r = create("Part", find("Effects"))
        r.name = "Rocket"
        r.material = "neon"
        r.color = {r = 255, g = 80, b = 40}
        r.size = {x = 0.9, y = 0.9, z = 2.6}
        r.position = {x = at.x + fwd.x * 4, y = at.y + 0.6, z = at.z + fwd.z * 4}
        r.rotation = {x = 0, y = s.ch.rotation.y, z = 0}
        r.can_collide = false
        r.anchored = true
        push(fx, {kind = "rocket", part = r, owner = p.name, target = target, dx = fwd.x, dz = fwd.z, born = time()})
        play_sound("boom", p)
    end
end

fn firework(x, z)
    shell = create("Part", find("Effects"))
    shell.name = "Firework"
    shell.shape = "ball"
    shell.material = "neon"
    shell.color = {r = 255, g = 240, b = 200}
    shell.size = {x = 0.8, y = 0.8, z = 0.8}
    shell.position = {x = x, y = 2, z = z}
    shell.can_collide = false
    shell.anchored = true
    push(fx, {kind = "shell", part = shell, born = time(), vy = 38 + random() * 12})
end

fn burst(at, colour)
    if colour == nil then
        colours = [{r = 255, g = 70, b = 70}, {r = 80, g = 180, b = 255}, {r = 255, g = 220, b = 60}, {r = 120, g = 255, b = 120}, {r = 255, g = 110, b = 230}]
        colour = colours[random(1, 5)]
    end
    for i in 1..14 do
        a = i / 14 * 6.2832
        up = random() * 2 - 1
        spark = create("Part", find("Effects"))
        spark.name = "Spark"
        spark.shape = "ball"
        spark.material = "neon"
        spark.color = colour
        spark.size = {x = 1.2, y = 1.2, z = 1.2}
        spark.position = {x = at.x, y = at.y, z = at.z}
        spark.can_collide = false
        spark.anchored = true
        push(fx, {kind = "spark", part = spark, born = time(), vx = cos(a) * 22, vy = up * 18, vz = sin(a) * 22})
    end
    play_sound("boom")
end

-- One moment of rockets, mines, fireworks. Gives back false when it's done.
fn step_fx(f, dt)
    age = time() - f.born
    part = f.part
    if f.kind == "rocket" then
        if age > 4 then
            return false
        end
        s = nil
        if f.target != nil then
            s = racers[f.target]
        end
        if s != nil then
            tx = s.ch.position.x - part.position.x
            ty = s.ch.position.y + 0.5 - part.position.y
            tz = s.ch.position.z - part.position.z
            d = sqrt(tx * tx + tz * tz)
            if d < 4 and abs(ty) < 4 then
                hit(s)
                return false
            end
            f.dx = tx / d
            f.dz = tz / d
            part.position.y += ty * 0.3
        end
        part.position = {x = part.position.x + f.dx * 115 * dt, y = part.position.y, z = part.position.z + f.dz * 115 * dt}
        part.rotation = {x = 0, y = atan2(f.dx, f.dz) * 57.2958, z = 0}
        return true
    elseif f.kind == "mine" then
        if age > 40 then
            return false
        end
        -- The ring pulses, so it shows from a long way off.
        grow = 6 + 2.5 * sin(age * 8)
        f.ring.size = {x = grow, y = 0.1, z = grow}
        f.ring.transparency = 0.35 + 0.25 * sin(age * 8)
        for name in racers do
            s = racers[name]
            dx = s.ch.position.x - f.x
            dz = s.ch.position.z - f.z
            if dx * dx + dz * dz < 16 and abs(s.ch.position.y - f.y) < 3 and (name != f.owner or age > 1.5) then
                hit(s)
                burst({x = f.x, y = f.y + 1, z = f.z}, {r = 255, g = 60, b = 40})
                return false
            end
        end
        return true
    elseif f.kind == "shell" then
        part.position.y += f.vy * dt
        f.vy -= 30 * dt
        if f.vy < 4 then
            burst(part.position, nil)
            return false
        end
        return true
    elseif f.kind == "spark" then
        if age > 1.3 then
            return false
        end
        f.vy -= 20 * dt
        part.position = {x = part.position.x + f.vx * dt, y = part.position.y + f.vy * dt, z = part.position.z + f.vz * dt}
        part.transparency = age / 1.3
        return true
    end
    return false
end

-- The big screen: the top five.

fn jumbotron_text()
    text = "BRICKPORT SPEEDWAY"
    n = 0
    for name in order do
        n += 1
        if n <= 5 then
            text = text + "\n" + n + "  " + name
        end
    end
    return text
end

fn standings_text()
    text = ""
    lap = 1
    if len(order) > 0 then
        lead = racers[order[1]]
        lap = min(LAPS, max(1, floor((lead.passed - 1) / NCP) + 1))
    end
    if world.phase == "race" then
        text = "LAP " + lap + " / " + LAPS
    else
        text = "RACE " + race_no
    end
    n = 0
    for name in order do
        n += 1
        s = racers[name]
        mark = ""
        if s.done then
            mark = "   " + fmt(s.finish)
        end
        text = text + "\n" + n + "  " + name + mark
    end
    return text
end

-- Each second or so: the standings board and the jumbotron.
fn show_standings()
    board = find("Standings")
    tron = find("Jumbotron Text")
    if world.phase == "race" or world.phase == "results" then
        t = standings_text()
        board.text = t
        board.visible = true
        tron.text = jumbotron_text()
    else
        board.visible = false
        tron.text = "BRICKPORT SPEEDWAY\nRace " + (race_no + 1) + " soon!"
    end
end


-- Each human's own HUD, and their race button.
fn show_hud(p)
    hud = child(p, "Race HUD")
    item = child(p, "Item Slot")
    button = child(p, "Race Toggle")
    note = child(p, "Note")
    if hud == nil then
        return
    end
    if note != nil and p.note_until != nil and time() > p.note_until then
        note.visible = false
    end
    s = racers[p.name]
    if s == nil or world.phase == "lobby" or world.phase == "intro" then
        hud.visible = false
    else
        lap = min(LAPS, max(1, floor((s.passed - 1) / NCP) + 1))
        t = time() - go_time
        if s.done then
            t = s.finish
        end
        if world.phase == "grid" then
            t = 0
        end
        place = s.place or 1
        hud.text = "LAP " + lap + "/" + LAPS + "     " + ordinal(place) + " of " + len(order) + "\nTIME " + fmt(t) + "   BEST " + fmt(s.best)
        hud.visible = true
    end
    if p.item != nil and p.kart != nil and s != nil then
        item.text = "E:  " + ITEM_NAMES[p.item]
        item.visible = true
    else
        item.visible = false
    end
    if button != nil then
        -- Between races: whether you're in the next one.
        button.visible = world.phase == "lobby" or (world.phase != "lobby" and s == nil)
        if racing_next(p) then
            button.text = "Next race: I'm IN  (G)"
            button.background_color = {r = 40, g = 150, b = 70}
        else
            button.text = "Next race: sitting out  (G)"
            button.background_color = {r = 120, g = 60, b = 60}
        end
    end
end

on player_joined(p)
    if is_bot(p) then
        return
    end
    p.wins = load(p, "wins") or 0
    best = load(p, "best_lap")
    if best != nil then
        p.best_lap = fmt(best)
    else
        p.best_lap = "-"
    end
    make_hud(p)
    if world.phase == "lobby" or world.phase == "results" then
        play_music(find("Lobby Music"), p)
        tell(p, "Walk up to a kart and press F to drive!")
    else
        tell(p, "A race is on: watch, or practise in the Drift Park")
    end
end

on player_left(p)
    s = racers[p.name]
    if s != nil then
        remove(racers, p.name)
        if s.bubble != nil then
            destroy(s.bubble)
        end
        drop_from_order(p.name)
    end
end

on key(p, k)
    if k == "e" then
        use_item(p)
    elseif k == "f" then
        toggle_kart(p)
    elseif k == "g" then
        if racing_next(p) then
            p.sit_out = true
        else
            p.sit_out = nil
        end
        show_hud(p)
    elseif k == "r" then
        s = racers[p.name]
        if s != nil and world.phase == "race" then
            respawn(s)
        elseif is_practice(p.kart) then
            place_kart(p.kart, {x = park.position.x - 40, y = 1.5, z = park.position.z}, 90)
        elseif p.kart != nil then
            place_kart(p.kart, {x = 0, y = 1.5, z = 40}, 90)
        end
    end
end

-- Fast: rockets, mines, fireworks.
every 0.05 seconds
    i = 1
    while i <= len(fx) do
        f = fx[i]
        if step_fx(f, 0.05) then
            i += 1
        else
            destroy(f.part)
            remove(fx, i)
        end
    end
end

-- Whether a point's in the Drift Park (with a little room round it).
fn in_park(pos)
    return abs(pos.x - park.position.x) < park.size.x / 2 + 4 and abs(pos.z - park.position.z) < park.size.z / 2 + 4
end

-- Often: checkpoints, places, speed, the barn, falls, practice karts.
every 0.1 seconds
    for p in players() do
        -- Anyone who ends up in the river is fished out.
        if p.kart == nil and p.position.y < -3 then
            p.position = {x = 0, y = 3, z = 32}
        end
    end
    -- Practice karts stay in the Drift Park.
    for k in practice do
        c = chassis_of(k)
        if not in_park(c.position) or c.position.y < -3 then
            place_kart(k, {x = park.position.x - 40, y = 1.5, z = park.position.z}, 90)
            if k.driver != nil then
                tell(k.driver, "Practice karts stay in the Drift Park")
            end
        end
    end
    if world.phase == "lobby" then
        for k in karts do
            if chassis_of(k).position.y < -3 then
                place_kart(k, {x = 0, y = 1.5, z = 40}, 90)
            end
        end
    end
    if world.phase != "race" and world.phase != "results" then
        return
    end
    for name in racers do
        s = racers[name]
        check_gates(s)
        if s.ch.position.y < -3 and not s.done then
            respawn(s)
        end
        -- Speed: bots by how they're doing, everyone slower in the barn.
        if not s.done then
            top = nil
            if is_bot(s.p) then
                top = s.band or s.p.skill or 66
            end
            dx = s.ch.position.x - barn.position.x
            dz = s.ch.position.z - barn.position.z
            if dx * dx + dz * dz < 24 * 24 then
                top = 40
            end
            s.kart.top_speed = top
        end
        -- Bots stuck for a while go back to the last checkpoint.
        if is_bot(s.p) and not s.done and s.kart.speed != nil and s.kart.speed < 2 and world.phase == "race" then
            s.slow = (s.slow or 0) + 0.1
            if s.slow > 4 then
                s.slow = 0
                respawn(s)
            end
        else
            s.slow = 0
        end
        if s.bubble != nil and s.shield < time() then
            destroy(s.bubble)
            s.bubble = nil
        end
    end
    rank()
    -- The last lap: the music turns up.
    if world.phase == "race" and not final_lap_on and len(order) > 0 then
        lead = racers[order[1]]
        if lead.passed >= (LAPS - 1) * NCP + 1 then
            final_lap_on = true
            play_sound(find("Final Lap Sting"))
            play_music(find("Final Lap Music"))
            say("FINAL LAP!")
            show_banner_until = time() + 2
        end
    end
end

show_banner_until = 0

-- Now and then: screens, bots' items, rubber bands, the crowd.
every 0.5 seconds
    show_standings()
    for p in humans() do
        show_hud(p)
    end
    if world.phase == "lobby" then
        left = max(0, floor(lobby_ends - time()))
        ins = 0
        for p in humans() do
            if racing_next(p) then
                ins += 1
            end
        end
        if ins == 0 then
            say("")
            find("Countdown").text = "Next race: waiting for racers (click I'm IN)"
        else
            find("Countdown").text = "Next race in " + left + "s   ·   " + ins + " racing"
        end
        find("Countdown").visible = true
    else
        find("Countdown").visible = false
    end
    if world.phase == "race" and show_banner_until > 0 and time() > show_banner_until then
        show_banner_until = 0
        say("")
    end
    if world.phase == "race" then
        best_human = nil
        for name in racers do
            s = racers[name]
            if not is_bot(s.p) and (best_human == nil or s.passed > best_human) then
                best_human = s.passed
            end
        end
        for name in racers do
            s = racers[name]
            if is_bot(s.p) then
                base = s.p.skill or 66
                if best_human != nil then
                    gap = s.passed - best_human
                    base = base - max(-6, min(8, gap * 2))
                end
                s.band = base
                if s.p.item != nil and random(1, 6) == 1 then
                    use_item(s.p)
                end
            end
        end
    end
    -- The crowd cheers: fans bob up and down.
    if time() < cheer_until then
        for i in 1..14 do
            fan = fans[random(1, len(fans))]
            for part in fan.children do
                part.position.y += 0.7
            end
            fan.up = (fan.up or 0) + 1
        end
    end
    for fan in fans do
        if fan.up != nil and random(1, 2) == 1 then
            for part in fan.children do
                part.position.y -= 0.7 * fan.up
            end
            fan.up = nil
        end
    end
end

-- The lobby: race karts back in the pits, bots waiting in the garages,
-- everyone free to drive about.
fn to_lobby()
    world.phase = "lobby"
    lobby_ends = time() + LOBBY_SECONDS
    for p in players() do
        p.item = nil
        p.race_place = nil
        p.camera_part = nil
    end
    n = 0
    for b in bots() do
        n += 1
        to_foot(b, {x = -60 + ((n - 1) % 8) * 17, y = 3, z = 70})
    end
    park_karts()
end

-- Lines up the grid: everyone who's in, in a race kart (humans at random
-- slots), and bots to make at least 4 and up to 8. Anyone else driving a
-- race kart gets out; practice karts carry on.
fn to_grid()
    racers = {}
    order = []
    final_lap_on = false
    list = []
    for p in humans() do
        if racing_next(p) and len(list) < 8 then
            insert(list, random(1, len(list) + 1), p)
        elseif p.kart != nil and not is_practice(p.kart) then
            to_foot(p, {x = random(-40, 40), y = 3, z = 32})
        end
    end
    -- Humans in practice karts come out of them for the race.
    for p in list do
        if is_practice(p.kart) then
            to_foot(p, nil)
        end
    end
    want = min(8 - len(list), max(3, 8 - len(list)))
    have = bots()
    while len(have) < want do
        b = add_bot(BOT_NAMES[len(have) + 1])
        b.lane = random(-5, 5)
        b.skill = 64 + random(0, 5)
        push(have, b)
    end
    n = 0
    for b in have do
        n += 1
        if n <= want then
            push(list, b)
        end
    end
    -- Everyone into a race kart on their slot.
    for p in list do
        if p.kart != nil and is_practice(p.kart) then
            p.kart = nil
        end
    end
    wait(0.3)
    n = 0
    for p in list do
        n += 1
        k = p.kart
        if k == nil then
            k = free_race_kart()
        end
        if k != nil then
            seat(p, k, slot_at(n), 90)
            k.locked = true
            k.top_speed = nil
            p.item = nil
            racers[p.name] = {p = p, kart = k, ch = chassis_of(k), passed = 0, next = 1, lap_start = 0, best = nil, done = false, finish = nil, respawned = 0, shield = 0, bubble = nil}
            push(order, p.name)
        end
    end
    finished_count = 0
    first_finish = 0
end

-- Moves the flyover camera from one pose to another over `seconds`.
fn glide(from, to, seconds)
    steps = floor(seconds / 0.03)
    for i in 0..steps do
        t = i / steps
        -- Ease in and out.
        e = t * t * (3 - 2 * t)
        flycam.position = {x = from.position.x + (to.position.x - from.position.x) * e, y = from.position.y + (to.position.y - from.position.y) * e, z = from.position.z + (to.position.z - from.position.z) * e}
        turn = to.rotation.y - from.rotation.y
        while turn > 180 do
            turn -= 360
        end
        while turn < -180 do
            turn += 360
        end
        flycam.rotation = {x = from.rotation.x + (to.rotation.x - from.rotation.x) * e, y = from.rotation.y + turn * e, z = 0}
        wait(0.03)
    end
end

-- Who watches the flyover: everyone not in a practice kart.
fn watchers()
    list = []
    for p in humans() do
        if not is_practice(p.kart) then
            push(list, p)
        end
    end
    return list
end

-- The show before a race: a flyover of the track to its own music, a
-- moment's quiet on the grid, then the five lights.
fn intro()
    world.phase = "intro"
    stop_music()
    shots = find("Flyover").children
    first = shots[1]
    flycam.position = find_in(first, "From").position
    for p in watchers() do
        p.camera_part = flycam
    end
    say("Race " + race_no + "  ·  Brickport Speedway  ·  " + LAPS + " laps")
    play_sound(find("Intro Music"))
    n = 0
    for shot in shots do
        n += 1
        if n == 2 then
            say("")
        end
        glide(find_in(shot, "From"), find_in(shot, "To"), shot.seconds)
    end
    world.phase = "grid"
    -- Racers look at their own kart; everyone else keeps the grid view.
    for name in racers do
        racers[name].p.camera_part = nil
    end
    wait(1.2)
    for l in lights do
        l.color = {r = 70, g = 12, b = 12}
    end
    for l in lights do
        l.color = {r = 255, g = 30, b = 30}
        play_sound(find("Light Beep"))
        wait(0.8)
    end
    wait(0.4 + random() * 0.8)
    for l in lights do
        l.color = {r = 40, g = 255, b = 80}
    end
    play_sound(find("Go Beep"))
    play_music(find("Race Music"))
    say("GO!")
    go_time = time()
    for name in racers do
        s = racers[name]
        s.lap_start = go_time
        s.kart.locked = nil
    end
    for p in humans() do
        p.camera_part = nil
    end
    world.phase = "race"
    cheer_until = time() + 4
    show_banner_until = time() + 1.2
end

fn find_in(parent, name)
    for c in parent.children do
        if c.name == name then
            return c
        end
    end
    return nil
end

-- The results: the podium, fireworks, and wins and best laps saved.
fn results()
    world.phase = "results"
    rank()
    play_sound(find("Victory Fanfare"))
    play_music(find("Lobby Music"))
    cheer_until = time() + 8
    text = "RESULTS"
    n = 0
    for name in order do
        n += 1
        s = racers[name]
        time_text = "DNF"
        if s.done then
            time_text = fmt(s.finish)
        end
        text = text + "\n" + ordinal(n) + "  " + name + "   " + time_text
        p = s.p
        if not is_bot(p) then
            if n == 1 and s.done then
                p.wins = (p.wins or 0) + 1
                save(p, "wins", p.wins)
            end
            if s.best != nil then
                old = load(p, "best_lap")
                if old == nil or s.best < old then
                    save(p, "best_lap", s.best)
                    p.best_lap = fmt(s.best)
                end
            end
        elseif n == 1 and s.done then
            p.wins = (p.wins or 0) + 1
        end
    end
    say("")
    board = find("Results")
    board.text = text
    board.visible = true
    -- Out of the karts, onto the podium (and the rest in front of it).
    for name in racers do
        s = racers[name]
        s.kart.locked = true
        s.p.kart = nil
    end
    wait(0.3)
    n = 0
    for name in order do
        n += 1
        s = racers[name]
        if n <= 3 then
            step = find("Step " + n)
            s.p.position = {x = step.position.x, y = step.position.y + step.size.y / 2 + 3, z = step.position.z}
        else
            s.p.position = {x = 30 + n * 5, y = 3, z = 90}
        end
    end
    for i in 1..8 do
        firework(40 + random(0, 40), 108 + random(0, 10))
        wait(0.6)
    end
    wait(3)
    board.visible = false
    for name in racers do
        s = racers[name]
        if s.bubble != nil then
            destroy(s.bubble)
        end
    end
    racers = {}
    order = []
end

-- Round and round: lobby, flyover, lights, race, results.
fn run_races()
    play_music(find("Lobby Music"))
    while true do
        world.time_of_day = TIMES[race_no % 3 + 1]
        to_lobby()
        -- The lobby lasts its time, and waits for someone who wants to race.
        while true do
            ins = 0
            for p in humans() do
                if racing_next(p) then
                    ins += 1
                end
            end
            if ins == 0 then
                lobby_ends = time() + LOBBY_SECONDS
            end
            if time() >= lobby_ends then
                break
            end
            wait(0.5)
        end
        race_no += 1
        to_grid()
        intro()
        race_end = time() + 300
        while time() < race_end do
            all_done = true
            for name in racers do
                s = racers[name]
                if not s.done and not is_bot(s.p) then
                    all_done = false
                end
            end
            if finished_count > 0 and (all_done or time() - first_finish > 25) then
                break
            end
            humans_racing = 0
            for name in racers do
                if not is_bot(racers[name].p) then
                    humans_racing += 1
                end
            end
            if humans_racing == 0 then
                break
            end
            wait(0.25)
        end
        results()
    end
end

leaderboard("wins", "best_lap")
run_races()
"#;

/// A kart built facing +Z, then turned `yaw` degrees and put at `at`
/// (its chassis).
fn kart_at(b: &mut B, parent: InstanceId, name: &str, at: V, yaw: f32, colour: Rgb, trim: Rgb) -> InstanceId {
    let kart = build_kart(&mut b.dm, parent, name, (0.0, at.1, 0.0), colour, trim);
    for id in b.dm.parts_under(kart) {
        let p = b.dm.part_mut(id).unwrap();
        let turned = rotate((p.position.x, 0.0, p.position.z), (0.0, yaw, 0.0));
        p.position.x = at.0 + turned.0;
        p.position.z = at.2 + turned.2;
        // (Composing a yaw onto YXZ Euler angles is just adding it.)
        p.rotation = Vec3::new(p.rotation.x, p.rotation.y + yaw, p.rotation.z);
    }
    kart
}

/// Turned to look from `from` at `to`: the rotation that points a part's
/// front (+Z) that way.
fn look_rotation(from: V, to: V) -> V {
    let d = sub(to, from);
    let flat = (d.0 * d.0 + d.2 * d.2).sqrt();
    (-(d.1.atan2(flat)).to_degrees(), d.0.atan2(d.2).to_degrees(), 0.0)
}

/// The Drift Park: an open pad of tarmac in the infield, ringed by tyres,
/// with cones to weave through, a circle to drift round and a little jump.
/// Four practice karts, always free, even during a race.
fn build_drift_park(b: &mut B) {
    let root = b.root();
    let park = b.folder(root, "Drift Park");
    let (x0, x1, z0, z1) = DRIFT;
    let (cx, cz) = ((x0 + x1) / 2.0, (z0 + z1) / 2.0);
    let floor = b.part(park, "Drift Park Floor", (cx, LIFT / 2.0 - 0.02, cz), (x1 - x0, LIFT, z1 - z0), (70, 72, 80), Material::Concrete);
    let _ = floor;
    // Painted lines: a drift circle and a start box.
    let circle = (cx + 20.0, cz + 25.0);
    for k in 0..24 {
        let a = k as f32 / 24.0 * std::f32::consts::TAU;
        let at = (circle.0 + a.cos() * 22.0, LIFT + 0.02, circle.1 + a.sin() * 22.0);
        let id = b.part(park, "Circle Line", at, (0.6, 0.04, 5.2), (245, 245, 245), Material::Plastic);
        b.turned(id, (0.0, -a.to_degrees(), 0.0));
        b.ghost(id, 0.0);
    }
    let cone = |b: &mut B, x: f32, z: f32| {
        let c = b.part(park, "Cone", (x, LIFT + 0.9, z), (1.2, 1.8, 1.2), (255, 120, 20), Material::Plastic);
        b.shape(c, Shape::Cylinder);
        b.ghost(c, 0.0);
        let band = b.part(park, "Cone Band", (x, LIFT + 1.1, z), (1.25, 0.3, 1.25), (250, 250, 250), Material::Plastic);
        b.shape(band, Shape::Cylinder);
        b.ghost(band, 0.0);
    };
    // A slalom down the west side, and a ring of cones in the circle.
    for k in 0..7 {
        cone(b, x0 + 30.0 + if k % 2 == 0 { -4.0 } else { 4.0 }, z0 + 20.0 + k as f32 * 13.0);
    }
    for k in 0..8 {
        let a = k as f32 / 8.0 * std::f32::consts::TAU;
        cone(b, circle.0 + a.cos() * 9.0, circle.1 + a.sin() * 9.0);
    }
    // A kicker and a landing, running north.
    let kick = b.part(park, "Park Kicker", (cx + 20.0, LIFT + 1.5, z0 + 22.0), (12.0, 3.0, 10.0), (250, 200, 30), Material::Metal);
    b.shape(kick, Shape::Wedge);
    let land = b.part(park, "Park Landing", (cx + 20.0, LIFT + 1.0, z0 + 52.0), (12.0, 2.0, 12.0), (250, 200, 30), Material::Metal);
    b.shape(land, Shape::Wedge);
    b.turned(land, (0.0, 180.0, 0.0));
    // Tyre walls all round, with a gap on the pits side to walk in.
    let tyre = |b: &mut B, x: f32, z: f32| {
        let t = b.part(park, "Tyres", (x, 1.0, z), (2.6, 2.0, 2.6), (30, 30, 34), Material::Plastic);
        b.shape(t, Shape::Cylinder);
    };
    let mut x = x0;
    while x <= x1 {
        tyre(b, x, z0 - 1.5);
        tyre(b, x, z1 + 1.5);
        x += 2.8;
    }
    let mut z = z0;
    while z <= z1 {
        if !(z > cz - 8.0 && z < cz + 8.0) {
            tyre(b, x0 - 1.5, z);
        }
        tyre(b, x1 + 1.5, z);
        z += 2.8;
    }
    // A sign at the gate.
    let board = b.part(park, "Drift Park Sign", (x0 - 6.0, 5.0, cz - 11.0), (0.6, 4.0, 10.0), NAVY, Material::Plastic);
    b.part(park, "Sign Post", (x0 - 6.0, 1.5, cz - 11.0), (0.5, 3.0, 0.5), (60, 60, 64), Material::Metal);
    let label = b.label(root, "Drift Park Label", "DRIFT PARK · practise any time · F to drive", (0.0, 0.0, 0.22, 0.04), 15.0, GOLD, Some(NAVY));
    b.dm.gui_mut(label).unwrap().attached_to = Some(board);
    let label = b.label(root, "Pits Label", "PITS · walk up to a kart, press F", (0.0, 0.0, 0.18, 0.04), 15.0, GOLD, Some(NAVY));
    let roof = b.dm.find_first("Garage Roof").unwrap();
    b.dm.gui_mut(label).unwrap().attached_to = Some(roof);
}

/// The flyover before a race: shots from a moving camera (each a From
/// and a To, looking at what matters), about ten seconds in all, then
/// behind the grid.
fn build_flyover(b: &mut B) {
    let root = b.root();
    let fly = b.folder(root, "Flyover");
    // (from, looking at), (to, looking at), seconds.
    let shots: [((V, V), (V, V), f32); 6] = [
        (((-95.0, 26.0, 30.0), (-10.0, 8.0, -25.0)), ((55.0, 20.0, 34.0), (10.0, 8.0, -25.0)), 2.2),
        (((305.0, 22.0, 40.0), (262.0, 2.0, 100.0)), ((305.0, 18.0, 175.0), (258.0, 2.0, 150.0)), 2.0),
        (((-30.0, 5.0, 150.0), (-75.0, 12.0, 178.0)), ((-120.0, 6.0, 205.0), (-75.0, 12.0, 175.0)), 2.0),
        (((-226.0, 7.0, 298.0), (-265.0, 6.0, 312.0)), ((-226.0, 12.0, 336.0), (-265.0, 6.0, 320.0)), 1.9),
        (((-200.0, 15.0, 470.0), (-150.0, 3.0, 440.0)), ((-30.0, 15.0, 468.0), (20.0, 3.0, 428.0)), 1.9),
        (((-135.0, 9.0, -3.0), (-40.0, 3.0, 0.0)), ((-105.0, 7.0, -2.0), (-10.0, 6.0, 0.0)), 2.3),
    ];
    for (n, ((from, look_a), (to, look_b), seconds)) in shots.iter().enumerate() {
        let shot = b.dm.create(Class::Model, &format!("Shot {}", n + 1), fly).unwrap();
        b.attr(shot, "seconds", Attribute::Num(*seconds as f64));
        for (name, at, look) in [("From", *from, *look_a), ("To", *to, *look_b)] {
            let id = b.part(shot, name, at, (0.5, 0.5, 1.0), (255, 0, 255), Material::Neon);
            b.turned(id, look_rotation(at, look));
            b.ghost(id, 1.0);
        }
    }
    let cam = b.part(root, "Flyover Camera", (-95.0, 26.0, 30.0), (0.5, 0.5, 1.0), (255, 0, 255), Material::Neon);
    b.ghost(cam, 1.0);
}

/// Templates the race copies: the per-player "race next" button.
fn build_storage(b: &mut B) {
    let root = b.root();
    let storage = b.folder(root, "Storage");
    let id = b.dm.create(Class::TextButton, "Race Toggle Template", storage).unwrap();
    let g = b.dm.gui_mut(id).unwrap();
    g.text = "Next race: I'm IN  (G)".into();
    (g.x, g.y, g.width, g.height) = (0.39, 0.125, 0.22, 0.045);
    g.text_size = 16.0;
    g.text_color = Color::new(255, 255, 255);
    g.background = true;
    g.background_color = Color::new(40, 150, 70);
    g.visible = false;
    b.script(id, "Toggle", RACE_TOGGLE);
}

const RACE_TOGGLE: &str = r#"
-- In or out of the next race.
on clicked(p)
    if p.sit_out == true then
        p.sit_out = nil
        self.text = "Next race: I'm IN  (G)"
        self.background_color = {r = 40, g = 150, b = 70}
    else
        p.sit_out = true
        self.text = "Next race: sitting out  (G)"
        self.background_color = {r = 120, g = 60, b = 60}
    end
end
"#;

/// The music and the sounds of the start, made in code (see synth.rs).
fn build_sounds(b: &mut B) {
    let root = b.root();
    let sounds = b.folder(root, "Music");
    use crate::synth;
    for (name, bytes, volume) in [
        ("Lobby Music", synth::speedway_lobby(), 0.55),
        ("Intro Music", synth::speedway_intro(), 0.7),
        ("Race Music", synth::speedway_race(false), 0.5),
        ("Final Lap Music", synth::speedway_race(true), 0.5),
        ("Light Beep", synth::light_beep(), 0.8),
        ("Go Beep", synth::go_beep(), 0.8),
        ("Final Lap Sting", synth::final_lap_sting(), 0.8),
        ("Victory Fanfare", synth::victory(), 0.8),
    ] {
        let id = b.dm.create(Class::Sound, name, sounds).unwrap();
        *b.dm.sound_mut(id).unwrap() = brixo_core::SoundProps::from_bytes("wav", &bytes);
        b.dm.sound_mut(id).unwrap().volume = volume;
    }
}

pub fn brickport_speedway() -> DataModel {
    let mut b = B { dm: DataModel::new() };
    let root = b.root();
    let mut rng = Rand(0x5eed_b41c);
    let t = track();

    build_land(&mut b, &mut rng);
    build_track(&mut b, &t);
    build_venue(&mut b, &mut rng);
    build_pits(&mut b);
    build_scenery(&mut b, &t, &mut rng);
    build_race_bits(&mut b, &t);
    b.folder(root, "Effects");

    // The race karts, parked in the pits; the practice karts, in the park.
    let karts = b.folder(root, "Race Karts");
    for (k, (name, colour, trim)) in KART_COLORS.iter().enumerate() {
        let at = (-60.0 + k as f32 * 17.0, LIFT + 1.1, 48.0);
        kart_at(&mut b, karts, &format!("{name} Kart"), at, 90.0, *colour, *trim);
    }
    let practice = b.folder(root, "Practice Karts");
    for k in 0..4 {
        let at = (DRIFT.0 + 12.0, LIFT + 1.1, DRIFT.2 + 22.0 + k as f32 * 12.0);
        let kart = kart_at(&mut b, practice, &format!("Practice Kart {}", k + 1), at, 90.0, (235, 235, 240), (30, 30, 36));
        b.attr(kart, "practice", Attribute::Bool(true));
    }
    build_drift_park(&mut b);
    build_flyover(&mut b);
    build_storage(&mut b);
    build_sounds(&mut b);

    // Lighting: it starts at sunset.
    let mut l = brixo_core::Lighting::of(&b.dm);
    l.time_of_day = 18.0;
    l.set(&mut b.dm);

    // On screen: the banner, the standings, and the controls.
    let banner = b.label(root, "Banner", "", (0.3, 0.2, 0.4, 0.08), 32.0, GOLD, Some(NAVY));
    b.dm.gui_mut(banner).unwrap().visible = false;
    let results = b.label(root, "Results", "", (0.7, 0.33, 0.28, 0.38), 19.0, GOLD, Some(NAVY));
    b.dm.gui_mut(results).unwrap().visible = false;
    let standings = b.label(root, "Standings", "", (0.01, 0.25, 0.16, 0.3), 15.0, (255, 255, 255), Some((20, 24, 36)));
    b.dm.gui_mut(standings).unwrap().visible = false;
    b.label(root, "Title", "BRICKPORT SPEEDWAY", (0.39, 0.012, 0.22, 0.05), 24.0, GOLD, Some(NAVY));
    b.label(root, "Help", "F get in / out of a kart · W/S drive · A/D steer · hold Space to drift, let go to boost · E item · R back on track · G sit out", (0.14, 0.94, 0.72, 0.04), 14.0, (255, 255, 255), Some((20, 24, 36)));
    let countdown = b.label(root, "Countdown", "", (0.36, 0.075, 0.28, 0.045), 18.0, (255, 255, 255), Some((20, 24, 36)));
    b.dm.gui_mut(countdown).unwrap().visible = false;

    b.script(root, "Race", RACE);
    b.dm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_track_is_a_closed_loop_from_the_line() {
        let t = track();
        assert!(t.len() > 500);
        assert!(len(t[0].at) < 0.01, "starts at the line: {:?}", t[0].at);
        assert!((t[0].yaw - 90.0).abs() < 1.0, "heading down the straight: {}", t[0].yaw);
        let lap: f32 = (0..t.len()).map(|i| len(sub(t[(i + 1) % t.len()].at, t[i].at))).sum();
        assert!(lap > 2200.0 && lap < 2500.0, "{lap}");
        // No bend so tight the inside wall crowds the road.
        let m = t.len();
        for i in 0..m {
            let turn = wrap_deg(t[(i + 2) % m].yaw - t[(i + m - 2) % m].yaw).to_radians().abs();
            let radius = 4.0 * STEP / turn.max(1e-6);
            assert!(radius > 22.0, "too tight at {:?}: {radius}", t[i].at);
        }
        // Where the track crosses itself, one is well above the other.
        for i in 0..t.len() {
            for j in i + 25..t.len() {
                if t.len() - (j - i) < 25 {
                    continue;
                }
                let d = flat_dist((t[i].at.0, t[i].at.2), (t[j].at.0, t[j].at.2));
                if d < WIDTH + 4.0 {
                    assert!((t[i].at.1 - t[j].at.1).abs() >= 10.0, "{i} and {j} overlap");
                }
            }
        }
    }

    #[test]
    fn rotate_matches_brixo() {
        // Yaw 90 turns +Z to +X; pitch positive tips the nose down; roll
        // positive lifts +X.
        let v = rotate((0.0, 0.0, 1.0), (0.0, 90.0, 0.0));
        assert!((v.0 - 1.0).abs() < 1e-4 && v.2.abs() < 1e-4);
        let v = rotate((0.0, 0.0, 1.0), (30.0, 0.0, 0.0));
        assert!(v.1 < -0.4);
        let v = rotate((1.0, 0.0, 0.0), (0.0, 0.0, 20.0));
        assert!(v.1 > 0.3);
    }
}
