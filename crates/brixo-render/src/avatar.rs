//! The Brixo avatar: a low-poly figure with a gem-cut head, built in code.
//!
//! Every mesh is in the character's own space: the origin is the centre of
//! its 5-stud-tall body (feet at y = -2.5), and it faces +Z. Pieces that
//! share a colour are merged into one mesh, so a player is drawn with a
//! handful of instances, all with the same transform.

use std::f32::consts::TAU;

use brixo_core::Face;
use glam::{Mat3, Vec2, Vec3};

use crate::Vertex;

/// Which of the player's colours a mesh is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    Skin,
    Shirt,
    Pants,
    Shoes,
    /// Eyes, brows and mouths.
    Ink,
    /// The little highlights in the eyes.
    Shine,
}

pub(crate) struct AvatarMesh {
    pub slot: Slot,
    /// Face meshes only draw for players wearing that face.
    pub face: Option<Face>,
    pub vertices: Vec<Vertex>,
}

/// Where the face panel is: the flat front of the eight-sided head.
const PANEL_Z: f32 = 0.85; // 1.0 * cos(22.5°) * 0.92
const INK_Z: f32 = PANEL_Z + 0.012;
const SHINE_Z: f32 = PANEL_Z + 0.02;
const EYE_Y: f32 = 1.92;
const MOUTH_Y: f32 = 1.63;

pub(crate) fn meshes() -> Vec<AvatarMesh> {
    let mut out = Vec::new();
    let body = |slot, build: &dyn Fn(&mut Mesh)| {
        let mut m = Mesh::default();
        build(&mut m);
        AvatarMesh { slot, face: None, vertices: m.vertices }
    };

    let r8 = 22.5f32.to_radians(); // puts a flat side of an octagon at the front
    out.push(body(Slot::Skin, &|m| {
        // Neck, then the head: chamfered bottom, straight band, chamfered top.
        m.prism(6, 0.3, 0.3, 0.97, 1.19, 0.0, 1.0, Vec3::ZERO, 0.0);
        m.prism(8, 0.72, 1.0, 1.19, 1.41, r8, 0.92, Vec3::ZERO, 0.0);
        m.prism(8, 1.0, 1.0, 1.41, 2.21, r8, 0.92, Vec3::ZERO, 0.0);
        m.prism(8, 1.0, 0.7, 2.21, 2.49, r8, 0.92, Vec3::ZERO, 0.0);
        // Gem hands.
        for side in [-1.0, 1.0] {
            m.gem(Vec3::new(side * 1.41, -0.58, 0.0), 0.3);
        }
    }));
    out.push(body(Slot::Shirt, &|m| {
        // A shield-shaped torso, narrow at the waist.
        m.prism(8, 0.58, 1.1, -0.64, 0.98, r8, 0.6, Vec3::ZERO, 0.0);
        // Arms, splayed slightly outward at the bottom.
        for side in [-1.0f32, 1.0] {
            m.prism(6, 0.21, 0.3, -0.395, 0.955, 0.0, 1.0, Vec3::new(side * 1.3, 0.0, 0.0), side * 9f32.to_radians());
        }
    }));
    out.push(body(Slot::Pants, &|m| {
        m.prism(8, 0.64, 0.52, -0.82, -0.62, r8, 0.62, Vec3::ZERO, 0.0);
        for side in [-1.0, 1.0] {
            m.prism(6, 0.33, 0.42, -2.12, -0.8, 30f32.to_radians(), 1.0, Vec3::new(side * 0.46, 0.0, 0.0), 0.0);
        }
    }));
    out.push(body(Slot::Shoes, &|m| {
        for side in [-1.0, 1.0] {
            m.wedge_foot(side * 0.46);
        }
    }));

    for face in Face::ALL {
        let mut ink = Mesh::default();
        let mut shine = Mesh::default();
        draw_face(face, &mut ink, &mut shine);
        out.push(AvatarMesh { slot: Slot::Ink, face: Some(face), vertices: ink.vertices });
        if !shine.vertices.is_empty() {
            out.push(AvatarMesh { slot: Slot::Shine, face: Some(face), vertices: shine.vertices });
        }
    }
    out
}

fn draw_face(face: Face, ink: &mut Mesh, shine: &mut Mesh) {
    let eye = |ink: &mut Mesh, shine: &mut Mesh, x: f32, y: f32, rx: f32, ry: f32| {
        ink.ellipse(Vec2::new(x, y), rx, ry, INK_Z);
        shine.ellipse(Vec2::new(x + 0.03, y + 0.055), 0.035, 0.035, SHINE_Z);
    };
    match face {
        Face::Smile => {
            eye(ink, shine, -0.24, EYE_Y, 0.1, 0.16);
            eye(ink, shine, 0.24, EYE_Y, 0.1, 0.16);
            ink.arc(
                Vec2::new(-0.13, MOUTH_Y + 0.05),
                Vec2::new(0.0, MOUTH_Y - 0.02),
                Vec2::new(0.13, MOUTH_Y + 0.05),
                0.03,
            );
        }
        Face::Happy => {
            // ^ ^ eyes and an open grin.
            for cx in [-0.24, 0.24] {
                ink.bar(Vec2::new(cx - 0.07, EYE_Y), 0.2, 0.06, 50f32.to_radians());
                ink.bar(Vec2::new(cx + 0.07, EYE_Y), 0.2, 0.06, -50f32.to_radians());
            }
            ink.ellipse(Vec2::new(0.0, MOUTH_Y), 0.15, 0.09, INK_Z);
        }
        Face::Surprised => {
            for cx in [-0.24, 0.24] {
                ink.ring(Vec2::new(cx, EYE_Y), 0.11, 0.064);
            }
            ink.ellipse(Vec2::new(0.0, MOUTH_Y - 0.02), 0.06, 0.085, INK_Z);
        }
        Face::Determined => {
            eye(ink, shine, -0.24, EYE_Y - 0.03, 0.09, 0.12);
            eye(ink, shine, 0.24, EYE_Y - 0.03, 0.09, 0.12);
            // Brows lower on the inside.
            ink.bar(Vec2::new(-0.26, EYE_Y + 0.17), 0.24, 0.06, 18f32.to_radians());
            ink.bar(Vec2::new(0.26, EYE_Y + 0.17), 0.24, 0.06, -18f32.to_radians());
            ink.bar(Vec2::new(0.0, MOUTH_Y), 0.24, 0.05, 0.0);
        }
    }
}

/// Flat-shaded triangles: every triangle gets its own three vertices and
/// its face normal, which is what gives the low-poly look.
#[derive(Default)]
struct Mesh {
    vertices: Vec<Vertex>,
}

impl Mesh {
    /// Adds a triangle facing away from `inside` (for closed solids).
    fn solid_tri(&mut self, a: Vec3, b: Vec3, c: Vec3, inside: Vec3) {
        let n = (b - a).cross(c - a);
        let centroid = (a + b + c) / 3.0;
        if n.dot(centroid - inside) < 0.0 {
            self.push(a, c, b, -n);
        } else {
            self.push(a, b, c, n);
        }
    }

    /// Adds a triangle on the face panel, facing +Z (out of the face).
    fn flat_tri(&mut self, a: Vec3, b: Vec3, c: Vec3) {
        let n = (b - a).cross(c - a);
        if n.z < 0.0 {
            self.push(a, c, b, Vec3::Z);
        } else {
            self.push(a, b, c, Vec3::Z);
        }
    }

    fn push(&mut self, a: Vec3, b: Vec3, c: Vec3, normal: Vec3) {
        let normal = normal.normalize_or_zero().to_array();
        for p in [a, b, c] {
            self.vertices.push(Vertex { position: p.to_array(), normal });
        }
    }

    /// A tapered prism with `sides` sides, from height y0 (radius r0) to y1
    /// (radius r1), squashed front-to-back by `depth`, centred on `at`
    /// (x/z) and tilted about its own centre around the Z axis.
    #[allow(clippy::too_many_arguments)]
    fn prism(&mut self, sides: usize, r0: f32, r1: f32, y0: f32, y1: f32, turn: f32, depth: f32, at: Vec3, tilt: f32) {
        let mid = (y0 + y1) / 2.0;
        let tilt = Mat3::from_rotation_z(tilt);
        let place = |p: Vec3| Vec3::new(at.x, mid, at.z) + tilt * (p - Vec3::new(0.0, mid, 0.0));
        let ring = |r: f32, y: f32| -> Vec<Vec3> {
            (0..sides)
                .map(|i| {
                    let a = turn + i as f32 * TAU / sides as f32;
                    place(Vec3::new(a.cos() * r, y, a.sin() * r * depth))
                })
                .collect()
        };
        let bottom = ring(r0, y0);
        let top = ring(r1, y1);
        let inside = place(Vec3::new(0.0, mid, 0.0));
        let (bc, tc) = (place(Vec3::new(0.0, y0, 0.0)), place(Vec3::new(0.0, y1, 0.0)));
        for i in 0..sides {
            let j = (i + 1) % sides;
            self.solid_tri(bottom[i], bottom[j], top[j], inside);
            self.solid_tri(bottom[i], top[j], top[i], inside);
            self.solid_tri(bc, bottom[j], bottom[i], inside);
            self.solid_tri(tc, top[i], top[j], inside);
        }
    }

    /// An icosahedron: the faceted "gem" hands.
    fn gem(&mut self, at: Vec3, r: f32) {
        let t = (1.0 + 5f32.sqrt()) / 2.0;
        let v: Vec<Vec3> = [
            (-1.0, t, 0.0), (1.0, t, 0.0), (-1.0, -t, 0.0), (1.0, -t, 0.0),
            (0.0, -1.0, t), (0.0, 1.0, t), (0.0, -1.0, -t), (0.0, 1.0, -t),
            (t, 0.0, -1.0), (t, 0.0, 1.0), (-t, 0.0, -1.0), (-t, 0.0, 1.0),
        ]
        .iter()
        .map(|&(x, y, z)| at + Vec3::new(x, y, z).normalize() * r)
        .collect();
        const FACES: [[usize; 3]; 20] = [
            [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
            [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
            [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
            [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
        ];
        for [a, b, c] in FACES {
            self.solid_tri(v[a], v[b], v[c], at);
        }
    }

    /// A shoe: flat sole, sloped toe pointing forward (+Z).
    fn wedge_foot(&mut self, x: f32) {
        let y0 = -2.5;
        let y1 = -2.12;
        let p = |px: f32, py: f32, pz: f32| Vec3::new(x + px, py, pz);
        let v = [
            p(-0.42, y0, 0.8), p(0.42, y0, 0.8), p(0.42, y0, -0.45), p(-0.42, y0, -0.45),
            p(-0.38, y1, 0.3), p(0.38, y1, 0.3), p(0.38, y1, -0.42), p(-0.38, y1, -0.42),
        ];
        let inside = v.iter().copied().sum::<Vec3>() / 8.0;
        for [a, b, c, d] in [[0, 1, 2, 3], [4, 7, 6, 5], [0, 4, 5, 1], [1, 5, 6, 2], [2, 6, 7, 3], [3, 7, 4, 0]] {
            self.solid_tri(v[a], v[b], v[c], inside);
            self.solid_tri(v[a], v[c], v[d], inside);
        }
    }

    // --- flat shapes drawn on the face panel ---

    fn ellipse(&mut self, c: Vec2, rx: f32, ry: f32, z: f32) {
        let n = 20;
        let centre = Vec3::new(c.x, c.y, z);
        for i in 0..n {
            let a0 = i as f32 * TAU / n as f32;
            let a1 = (i + 1) as f32 * TAU / n as f32;
            let p = |a: f32| Vec3::new(c.x + a.cos() * rx, c.y + a.sin() * ry, z);
            self.flat_tri(centre, p(a0), p(a1));
        }
    }

    fn ring(&mut self, c: Vec2, r: f32, width: f32) {
        let n = 24;
        let (ri, ro) = (r - width / 2.0, r + width / 2.0);
        for i in 0..n {
            let a0 = i as f32 * TAU / n as f32;
            let a1 = (i + 1) as f32 * TAU / n as f32;
            let p = |a: f32, rr: f32| Vec3::new(c.x + a.cos() * rr, c.y + a.sin() * rr, INK_Z);
            self.flat_tri(p(a0, ri), p(a0, ro), p(a1, ro));
            self.flat_tri(p(a0, ri), p(a1, ro), p(a1, ri));
        }
    }

    /// A rounded-off rectangle stroke, rotated by `angle`.
    fn bar(&mut self, c: Vec2, length: f32, thick: f32, angle: f32) {
        let dir = Vec2::new(angle.cos(), angle.sin());
        self.stroke(c - dir * length / 2.0, c + dir * length / 2.0, thick);
    }

    /// A smooth curve starting at a, passing through b, ending at c.
    fn arc(&mut self, a: Vec2, b: Vec2, c: Vec2, thick: f32) {
        let control = b * 2.0 - (a + c) / 2.0;
        let at = |t: f32| a * (1.0 - t) * (1.0 - t) + control * 2.0 * t * (1.0 - t) + c * t * t;
        let steps = 12;
        for i in 0..steps {
            self.stroke(at(i as f32 / steps as f32), at((i + 1) as f32 / steps as f32), thick * 2.0);
        }
    }

    /// A line segment with round caps.
    fn stroke(&mut self, a: Vec2, b: Vec2, width: f32) {
        let side = (b - a).perp().normalize_or_zero() * width / 2.0;
        let q = |p: Vec2| Vec3::new(p.x, p.y, INK_Z);
        self.flat_tri(q(a - side), q(b - side), q(b + side));
        self.flat_tri(q(a - side), q(b + side), q(a + side));
        self.ellipse(a, width / 2.0, width / 2.0, INK_Z);
        self.ellipse(b, width / 2.0, width / 2.0, INK_Z);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_avatar_fits_in_a_five_stud_box() {
        for m in meshes() {
            for v in &m.vertices {
                let [x, y, z] = v.position;
                assert!((-2.5..=2.5).contains(&y), "y {y} in {:?}", m.slot);
                assert!(x.abs() <= 1.8 && z.abs() <= 1.0, "({x}, {z}) in {:?}", m.slot);
            }
        }
    }

    #[test]
    fn the_face_panel_is_flat_and_points_forward() {
        let skin = meshes().into_iter().find(|m| m.slot == Slot::Skin).unwrap();
        let panel: Vec<_> = skin
            .vertices
            .chunks(3)
            .filter(|t| t.iter().all(|v| (v.position[2] - PANEL_Z).abs() < 0.01))
            .collect();
        assert!(panel.len() >= 2, "the flat face panel exists");
        for tri in panel {
            assert!(tri[0].normal[2] > 0.99, "face panel points forward");
        }
    }

    #[test]
    fn body_triangles_point_outward() {
        // The chest's front faces point forward, its back faces backward.
        let shirt = meshes().into_iter().find(|m| m.slot == Slot::Shirt).unwrap();
        for tri in shirt.vertices.chunks(3) {
            let centroid_z = tri.iter().map(|v| v.position[2]).sum::<f32>() / 3.0;
            let side_facet = tri[0].normal[1].abs() < 0.9; // not a top or bottom cap
            if side_facet && centroid_z.abs() > 0.3 && tri.iter().all(|v| v.position[0].abs() < 0.8) {
                assert!(tri[0].normal[2] * centroid_z > 0.0, "chest facet points inward");
            }
        }
    }

    #[test]
    fn every_face_has_ink_in_front_of_the_panel() {
        let ms = meshes();
        for face in Face::ALL {
            let ink = ms.iter().find(|m| m.face == Some(face) && m.slot == Slot::Ink).unwrap();
            assert!(!ink.vertices.is_empty());
            assert!(ink.vertices.iter().all(|v| v.position[2] > PANEL_Z && v.normal == [0.0, 0.0, 1.0]));
        }
    }
}
