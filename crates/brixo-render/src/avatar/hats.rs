//! Hats: each one built from a few simple shapes (domes, cylinders, cones,
//! rings, boxes) in fixed colours, sitting on the round head. They belong to
//! the head, so they turn with it and fly off with it when a player falls
//! apart.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use brixo_core::Hat;
use glam::{Quat, Vec2, Vec3};

use super::{Mesh, HEAD_CENTER};

/// The pieces of one hat: a colour and the triangles drawn in it.
pub(crate) fn hat_pieces(hat: Hat) -> Vec<([u8; 3], Mesh)> {
    let mut b = Builder::default();
    let c = HEAD_CENTER;
    let at = |x: f32, y: f32, z: f32| c + Vec3::new(x, y, z);
    let none = Quat::IDENTITY;
    match hat {
        Hat::Cap => {
            const RED: [u8; 3] = [196, 40, 28];
            b.add(RED, |m| m.dome(at(0.0, 0.1, 0.0), none, Vec3::new(0.9, 0.84, 0.9), 0.0, TAU));
            // The brim sticks out the front, tipped down a little.
            b.add(RED, |m| m.cuboid_at(at(0.0, 0.14, 0.78), Quat::from_rotation_x(0.14), Vec3::new(0.98, 0.07, 0.72)));
            b.add(RED, |m| m.ellipsoid(at(0.0, 0.95, 0.0), Vec3::splat(0.08)));
        }
        Hat::Beanie => {
            const NAVY: [u8; 3] = [35, 62, 120];
            b.add(NAVY, |m| m.dome(at(0.0, 0.08, 0.0), none, Vec3::new(0.91, 0.9, 0.91), 0.0, TAU));
            b.add([27, 48, 94], |m| m.frustum(at(0.0, 0.2, 0.0), none, 0.93, 0.92, 0.3));
            b.add([242, 243, 243], |m| m.ellipsoid(at(0.0, 1.05, 0.0), Vec3::splat(0.2)));
        }
        Hat::TopHat => {
            const BLACK: [u8; 3] = [30, 30, 32];
            b.add(BLACK, |m| m.frustum(at(0.0, 0.7, 0.0), none, 1.05, 1.05, 0.06));
            b.add(BLACK, |m| m.frustum(at(0.0, 1.22, 0.0), none, 0.6, 0.64, 1.0));
            b.add([170, 32, 32], |m| m.frustum(at(0.0, 0.84, 0.0), none, 0.655, 0.655, 0.2));
        }
        Hat::CowboyHat => {
            const BROWN: [u8; 3] = [132, 88, 52];
            // A wide round brim.
            b.add(BROWN, |m| m.frustum(at(0.0, 0.62, 0.0), none, 1.35, 1.35, 0.06));
            b.add(BROWN, |m| m.frustum(at(0.0, 0.95, 0.0), none, 0.58, 0.5, 0.62));
            b.add(BROWN, |m| m.dome(at(0.0, 1.26, 0.0), none, Vec3::new(0.5, 0.14, 0.5), 0.0, TAU));
            b.add([74, 46, 26], |m| m.frustum(at(0.0, 0.72, 0.0), none, 0.6, 0.59, 0.14));
        }
        Hat::Crown => {
            const GOLD: [u8; 3] = [245, 192, 40];
            b.add(GOLD, |m| m.tube(at(0.0, 0.76, 0.0), 0.62, 0.55, 0.32));
            for i in 0..5 {
                let a = i as f32 / 5.0 * TAU + FRAC_PI_2;
                let p = at(a.cos() * 0.585, 1.06, a.sin() * 0.585);
                b.add(GOLD, |m| m.frustum(p, none, 0.11, 0.0, 0.3));
                b.add(GOLD, |m| m.ellipsoid(p + Vec3::Y * 0.17, Vec3::splat(0.05)));
            }
            b.add([200, 30, 45], |m| m.ellipsoid(at(0.0, 0.76, 0.62), Vec3::new(0.08, 0.08, 0.04)));
            for side in [-1.0f32, 1.0] {
                b.add([40, 110, 210], |m| {
                    m.ellipsoid(at(side * 0.44, 0.76, 0.44), Vec3::new(0.06, 0.06, 0.03))
                });
            }
        }
        Hat::Headphones => {
            // The band arcs over the top from ear to ear.
            b.add([35, 35, 38], |m| m.torus(c, Quat::from_rotation_x(-FRAC_PI_2), 0.95, 0.07, 0.0, PI));
            for side in [-1.0f32, 1.0] {
                let turn = Quat::from_rotation_z(FRAC_PI_2);
                b.add([55, 55, 60], |m| m.frustum(at(side * 0.9, 0.0, 0.0), turn, 0.34, 0.34, 0.24));
                b.add([196, 40, 28], |m| m.frustum(at(side * 1.035, 0.0, 0.0), turn, 0.22, 0.22, 0.04));
            }
        }
        Hat::PartyHat => {
            // A cone in three bands, with a pom-pom on the point.
            let tilt = Quat::from_rotation_z(-0.18) * Quat::from_rotation_x(-0.1);
            let base = at(0.08, 0.68, -0.05);
            let (r, h) = (0.5, 1.1);
            let up = tilt * Vec3::Y;
            let band = |k: f32| r * (1.0 - k);
            let colors = [[120, 55, 180], [255, 205, 40], [120, 55, 180]];
            for (i, color) in colors.into_iter().enumerate() {
                let (k0, k1) = (i as f32 / 3.0, (i + 1) as f32 / 3.0);
                let mid = base + up * (h * (k0 + k1) / 2.0);
                b.add(color, |m| m.frustum(mid, tilt, band(k0), band(k1), h / 3.0));
            }
            b.add([255, 205, 40], |m| m.ellipsoid(base + up * h, Vec3::splat(0.12)));
        }
        Hat::ChefHat => {
            const WHITE: [u8; 3] = [245, 245, 242];
            b.add(WHITE, |m| m.frustum(at(0.0, 0.55, 0.0), none, 0.72, 0.72, 0.4));
            b.add(WHITE, |m| m.ellipsoid(at(0.0, 0.98, 0.0), Vec3::new(0.92, 0.5, 0.92)));
            b.add(WHITE, |m| m.ellipsoid(at(0.0, 1.28, 0.0), Vec3::new(0.55, 0.32, 0.55)));
        }
        Hat::VikingHelmet => {
            b.add([150, 152, 160], |m| m.dome(at(0.0, 0.06, 0.0), none, Vec3::new(0.92, 0.88, 0.92), 0.0, TAU));
            b.add([112, 76, 44], |m| m.frustum(at(0.0, 0.14, 0.0), none, 0.94, 0.94, 0.16));
            // Horns: two segments each, curving out and up.
            for side in [-1.0f32, 1.0] {
                let low = Quat::from_rotation_z(-side * 1.15);
                let high = Quat::from_rotation_z(-side * 0.45);
                let root = at(side * 0.78, 0.42, 0.0);
                let mid = root + low * Vec3::Y * 0.36;
                b.add([236, 226, 200], |m| m.frustum(root + low * Vec3::Y * 0.18, low, 0.15, 0.11, 0.36));
                b.add([236, 226, 200], |m| m.frustum(mid + high * Vec3::Y * 0.2, high, 0.11, 0.0, 0.4));
            }
        }
        Hat::HardHat => {
            const YELLOW: [u8; 3] = [248, 205, 45];
            b.add(YELLOW, |m| m.dome(at(0.0, 0.14, 0.0), none, Vec3::new(0.9, 0.8, 0.92), 0.0, TAU));
            b.add(YELLOW, |m| m.frustum(at(0.0, 0.15, 0.06), none, 1.06, 1.06, 0.05));
        }
        Hat::PropellerCap => {
            // Four coloured panels, like a beach ball.
            let panels = [[196, 40, 28], [248, 205, 45], [13, 105, 172], [40, 150, 70]];
            for (i, color) in panels.into_iter().enumerate() {
                let a0 = i as f32 * FRAC_PI_2;
                b.add(color, |m| m.dome(at(0.0, 0.1, 0.0), none, Vec3::new(0.9, 0.84, 0.9), a0, a0 + FRAC_PI_2));
            }
            b.add([196, 40, 28], |m| m.cuboid_at(at(0.0, 0.14, 0.8), Quat::from_rotation_x(0.12), Vec3::new(0.8, 0.06, 0.5)));
            b.add([60, 60, 66], |m| m.frustum(at(0.0, 1.0, 0.0), none, 0.04, 0.04, 0.22));
            let spin = Quat::from_rotation_y(0.5);
            b.add([196, 40, 28], |m| m.cuboid_at(at(0.0, 1.12, 0.0) + spin * Vec3::X * 0.3, spin, Vec3::new(0.6, 0.03, 0.14)));
            b.add([13, 105, 172], |m| m.cuboid_at(at(0.0, 1.12, 0.0) - spin * Vec3::X * 0.3, spin, Vec3::new(0.6, 0.03, 0.14)));
        }
        Hat::Halo => {
            b.add([255, 220, 90], |m| m.torus(at(0.0, 1.15, 0.0), none, 0.55, 0.07, 0.0, TAU));
        }
        Hat::TrafficCone => {
            const ORANGE: [u8; 3] = [240, 108, 22];
            b.add(ORANGE, |m| m.cuboid_at(at(0.0, 0.66, 0.0), none, Vec3::new(1.2, 0.08, 1.2)));
            let (r0, r1, h, y0) = (0.52, 0.09, 1.15, 0.7);
            let r = |k: f32| r0 + (r1 - r0) * k;
            let bands = [(0.0, 0.35, ORANGE), (0.35, 0.55, [245, 245, 242]), (0.55, 1.0, ORANGE)];
            for (k0, k1, color) in bands {
                b.add(color, |m| m.frustum(at(0.0, y0 + h * (k0 + k1) / 2.0, 0.0), none, r(k0), r(k1), h * (k1 - k0)));
            }
        }
    }
    b.done()
}

#[derive(Default)]
struct Builder {
    pieces: Vec<([u8; 3], Mesh)>,
}

impl Builder {
    fn add(&mut self, color: [u8; 3], build: impl FnOnce(&mut Mesh)) {
        let i = match self.pieces.iter().position(|(c, _)| *c == color) {
            Some(i) => i,
            None => {
                self.pieces.push((color, Mesh::default()));
                self.pieces.len() - 1
            }
        };
        build(&mut self.pieces[i].1);
    }

    fn done(self) -> Vec<([u8; 3], Mesh)> {
        self.pieces
    }
}

const SEGMENTS: usize = 24;

impl Mesh {
    /// A triangle wound so its front faces the way its normals point
    /// (triangles facing away from the camera aren't drawn).
    fn tri(&mut self, v: [(Vec3, Vec3); 3]) {
        let [a, b, c] = v;
        let facing = (b.0 - a.0).cross(c.0 - a.0).dot(a.1 + b.1 + c.1);
        let order = if facing < 0.0 { [a, c, b] } else { [a, b, c] };
        for (p, n) in order {
            self.vertex(p, n.normalize_or_zero(), Vec2::ZERO);
        }
    }

    fn quad(&mut self, a: (Vec3, Vec3), b: (Vec3, Vec3), c: (Vec3, Vec3), d: (Vec3, Vec3)) {
        self.tri([a, b, c]);
        self.tri([a, c, d]);
    }

    /// A box turned by `rot`.
    fn cuboid_at(&mut self, center: Vec3, rot: Quat, size: Vec3) {
        let (cube, indices) = crate::cube();
        for i in indices {
            let v = cube[i as usize];
            let p = center + rot * (Vec3::from(v.position) * size);
            self.vertex(p, rot * Vec3::from(v.normal), Vec2::from(v.uv));
        }
    }

    /// A cylinder (or cone, when one end is 0) along `rot`'s up, centred on
    /// `center`, with flat caps.
    fn frustum(&mut self, center: Vec3, rot: Quat, r_bottom: f32, r_top: f32, height: f32) {
        let h = height / 2.0;
        let ring = |i: usize| {
            let a = i as f32 / SEGMENTS as f32 * TAU;
            Vec3::new(a.cos(), 0.0, a.sin())
        };
        let put = |p: Vec3| center + rot * p;
        let bottom_c = put(-Vec3::Y * h);
        let top_c = put(Vec3::Y * h);
        for i in 0..SEGMENTS {
            let (d0, d1) = (ring(i), ring(i + 1));
            // Sloped sides lean their normals up (or down) with the slope.
            let slope = |d: Vec3| rot * (d * height + Vec3::Y * (r_bottom - r_top)).normalize();
            let b0 = (put(d0 * r_bottom - Vec3::Y * h), slope(d0));
            let b1 = (put(d1 * r_bottom - Vec3::Y * h), slope(d1));
            let t0 = (put(d0 * r_top + Vec3::Y * h), slope(d0));
            let t1 = (put(d1 * r_top + Vec3::Y * h), slope(d1));
            if r_top > 1e-4 {
                self.quad(b0, b1, t1, t0);
                let up = rot * Vec3::Y;
                self.tri([(top_c, up), (t0.0, up), (t1.0, up)]);
            } else {
                self.tri([b0, b1, t0]);
            }
            if r_bottom > 1e-4 {
                let down = rot * -Vec3::Y;
                self.tri([(bottom_c, down), (b0.0, down), (b1.0, down)]);
            }
        }
    }

    /// A ring-shaped wall (a crown's band): outside, inside and a top rim.
    fn tube(&mut self, center: Vec3, r_out: f32, r_in: f32, height: f32) {
        let h = height / 2.0;
        let ring = |i: usize| {
            let a = i as f32 / SEGMENTS as f32 * TAU;
            Vec3::new(a.cos(), 0.0, a.sin())
        };
        for i in 0..SEGMENTS {
            let (d0, d1) = (ring(i), ring(i + 1));
            let p = |d: Vec3, r: f32, y: f32| center + d * r + Vec3::Y * y;
            self.quad((p(d0, r_out, -h), d0), (p(d1, r_out, -h), d1), (p(d1, r_out, h), d1), (p(d0, r_out, h), d0));
            self.quad((p(d0, r_in, -h), -d0), (p(d1, r_in, -h), -d1), (p(d1, r_in, h), -d1), (p(d0, r_in, h), -d0));
            let up = Vec3::Y;
            self.quad((p(d0, r_in, h), up), (p(d1, r_in, h), up), (p(d1, r_out, h), up), (p(d0, r_out, h), up));
        }
    }

    /// The top half of an egg shape (radii per axis), from angle `a0` to
    /// `a1` around, closed underneath.
    fn dome(&mut self, center: Vec3, rot: Quat, radii: Vec3, a0: f32, a1: f32) {
        let rings = 8;
        let segs = ((a1 - a0) / TAU * SEGMENTS as f32).ceil().max(2.0) as usize;
        let point = |ring: usize, seg: usize| {
            let theta = ring as f32 / rings as f32 * FRAC_PI_2; // 0 at the top
            let phi = a0 + (a1 - a0) * seg as f32 / segs as f32;
            let unit = Vec3::new(theta.sin() * phi.cos(), theta.cos(), theta.sin() * phi.sin());
            (center + rot * (unit * radii), rot * (unit / radii).normalize())
        };
        for ring in 0..rings {
            for seg in 0..segs {
                self.quad(point(ring, seg), point(ring + 1, seg), point(ring + 1, seg + 1), point(ring, seg + 1));
            }
        }
        let down = rot * -Vec3::Y;
        for seg in 0..segs {
            self.tri([(center, down), (point(rings, seg).0, down), (point(rings, seg + 1).0, down)]);
        }
    }

    /// A whole egg shape (or a ball, with equal radii).
    fn ellipsoid(&mut self, center: Vec3, radii: Vec3) {
        let (rings, segs) = (10, 16);
        let point = |ring: usize, seg: usize| {
            let theta = ring as f32 / rings as f32 * PI;
            let phi = seg as f32 / segs as f32 * TAU;
            let unit = Vec3::new(theta.sin() * phi.cos(), theta.cos(), theta.sin() * phi.sin());
            (center + unit * radii, (unit / radii).normalize())
        };
        for ring in 0..rings {
            for seg in 0..segs {
                let (a, b, c, d) = (point(ring, seg), point(ring + 1, seg), point(ring + 1, seg + 1), point(ring, seg + 1));
                // The rows at the poles have one corner squashed to a point.
                if ring != 0 {
                    self.tri([a, b, c]);
                }
                if ring != rings - 1 {
                    self.tri([a, c, d]);
                }
            }
        }
    }

    /// A doughnut lying flat (turned by `rot`), from angle `a0` to `a1`
    /// around: a halo, or half of one for a headphone band.
    fn torus(&mut self, center: Vec3, rot: Quat, big: f32, small: f32, a0: f32, a1: f32) {
        let (around, tube) = (32, 8);
        let point = |i: usize, j: usize| {
            let a = a0 + (a1 - a0) * i as f32 / around as f32;
            let b = j as f32 / tube as f32 * TAU;
            let out = Vec3::new(a.cos(), 0.0, a.sin());
            let n = out * b.cos() + Vec3::Y * b.sin();
            (center + rot * (out * big + n * small), rot * n)
        };
        for i in 0..around {
            for j in 0..tube {
                self.quad(point(i, j), point(i + 1, j), point(i + 1, j + 1), point(i, j + 1));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hat_has_triangles_facing_outward() {
        for hat in Hat::ALL {
            let pieces = hat_pieces(hat);
            assert!(!pieces.is_empty(), "{hat:?} has no pieces");
            for (_, m) in &pieces {
                assert!(!m.vertices.is_empty() && m.vertices.len() % 3 == 0);
                for tri in m.vertices.chunks(3) {
                    let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(tri[k].position));
                    let n: Vec3 = [0, 1, 2].map(|k| Vec3::from(tri[k].normal)).into_iter().sum();
                    let geo = (b - a).cross(c - a);
                    if geo.length() > 1e-6 {
                        assert!(geo.dot(n) >= 0.0, "{hat:?} has a triangle facing inward");
                    }
                }
            }
        }
    }

    #[test]
    fn caps_and_helmets_cover_the_top_of_the_head() {
        // Straight down onto the head's top: something of the hat is above it.
        let top = HEAD_CENTER.y + super::super::HEAD_RADIUS;
        for hat in [Hat::Cap, Hat::Beanie, Hat::VikingHelmet, Hat::HardHat, Hat::PropellerCap, Hat::ChefHat] {
            let above = hat_pieces(hat).iter().flat_map(|(_, m)| m.vertices.iter()).any(|v| {
                let p = Vec3::from(v.position);
                p.x.abs() < 0.05 && p.z.abs() < 0.05 && p.y > top + 0.02
            });
            assert!(above, "{hat:?} lets the head poke through its top");
        }
    }

    #[test]
    fn hats_sit_on_the_head_not_the_body() {
        for hat in Hat::ALL {
            for (_, m) in hat_pieces(hat) {
                for v in &m.vertices {
                    let p = Vec3::from(v.position);
                    assert!(p.y > HEAD_CENTER.y - 0.5, "{hat:?} hangs down to y={}", p.y);
                    assert!(p.y < HEAD_CENTER.y + 2.2 && p.x.abs() < 1.6 && p.z.abs() < 1.6, "{hat:?} is too big");
                }
            }
        }
    }
}
