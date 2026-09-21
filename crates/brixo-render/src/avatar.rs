//! The Brixo avatar: a blocky body with one smooth, round head, and a
//! pixel-art face printed on it like a decal.
//!
//! Every mesh is in the character's own space: the origin is the centre of
//! its 5-stud-tall body (feet at y = -2.5), and it faces +Z. Pieces that
//! share a colour are merged into one mesh, so a player is drawn with a
//! handful of instances, all with the same transform.

use std::f32::consts::{PI, TAU};

use brixo_core::Face;
use glam::{Vec2, Vec3};

use crate::Vertex;

/// Which of the player's colours a mesh is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    Skin,
    Shirt,
    Pants,
    Shoes,
    /// The face decal: white, textured with the face's picture.
    Decal,
}

pub(crate) struct AvatarMesh {
    pub slot: Slot,
    pub vertices: Vec<Vertex>,
}

pub(crate) const HEAD_CENTER: Vec3 = Vec3::new(0.0, 1.8, 0.0);
pub(crate) const HEAD_RADIUS: f32 = 0.86;
/// How big the face picture is, wrapped onto the front of the head.
const DECAL_SIZE: f32 = 1.25;

pub(crate) fn meshes() -> Vec<AvatarMesh> {
    let mut skin = Mesh::default();
    let mut shirt = Mesh::default();
    let mut pants = Mesh::default();
    let mut shoes = Mesh::default();
    for side in [-1.0f32, 1.0] {
        shoes.cuboid(Vec3::new(side * 0.49, -2.325, 0.04), Vec3::new(0.9, 0.35, 1.0));
        pants.cuboid(Vec3::new(side * 0.49, -1.475, 0.0), Vec3::new(0.88, 1.35, 0.94));
        shirt.cuboid(Vec3::new(side * 1.43, 0.295, 0.0), Vec3::new(0.8, 1.35, 0.9));
        skin.cuboid(Vec3::new(side * 1.43, -0.63, 0.0), Vec3::new(0.72, 0.5, 0.82));
    }
    shirt.cuboid(Vec3::new(0.0, 0.125, 0.0), Vec3::new(2.0, 1.85, 1.02));
    skin.sphere(HEAD_CENTER, HEAD_RADIUS);

    let mut decal = Mesh::default();
    decal.face_patch();

    vec![
        AvatarMesh { slot: Slot::Skin, vertices: skin.vertices },
        AvatarMesh { slot: Slot::Shirt, vertices: shirt.vertices },
        AvatarMesh { slot: Slot::Pants, vertices: pants.vertices },
        AvatarMesh { slot: Slot::Shoes, vertices: shoes.vertices },
        AvatarMesh { slot: Slot::Decal, vertices: decal.vertices },
    ]
}

#[derive(Default)]
pub(crate) struct Mesh {
    pub vertices: Vec<Vertex>,
}

/// Unit-sized meshes (filling -0.5..0.5) for each part shape, in
/// `Shape::ALL` order. Instances scale them to the part's size.
pub(crate) fn shape_meshes() -> Vec<Vec<Vertex>> {
    brixo_core::Shape::ALL
        .iter()
        .map(|shape| {
            let mut m = Mesh::default();
            match shape {
                brixo_core::Shape::Block => m.cuboid(Vec3::ZERO, Vec3::ONE),
                brixo_core::Shape::Wedge => m.wedge(),
                brixo_core::Shape::Cylinder => m.cylinder(),
                brixo_core::Shape::Ball => m.sphere(Vec3::ZERO, 0.5),
            }
            m.vertices
        })
        .collect()
}

impl Mesh {
    fn vertex(&mut self, p: Vec3, n: Vec3, uv: Vec2) {
        self.vertices.push(Vertex { position: p.to_array(), normal: n.to_array(), uv: uv.to_array() });
    }

    /// A box with flat faces, counter-clockwise seen from outside.
    fn cuboid(&mut self, center: Vec3, size: Vec3) {
        let (cube, indices) = crate::cube();
        for i in indices {
            let v = cube[i as usize];
            let p = center + Vec3::from(v.position) * size;
            self.vertex(p, Vec3::from(v.normal), Vec2::from(v.uv));
        }
    }

    /// A flat triangle facing away from `inside` (for closed solids).
    fn facet(&mut self, a: Vec3, b: Vec3, c: Vec3, inside: Vec3) {
        let n = (b - a).cross(c - a);
        let (b, c, n) = if n.dot((a + b + c) / 3.0 - inside) < 0.0 { (c, b, -n) } else { (b, c, n) };
        let n = n.normalize_or_zero();
        for p in [a, b, c] {
            self.vertex(p, n, Vec2::ZERO);
        }
    }

    /// A ramp filling the unit box, rising toward +Z (the physics collider
    /// is built from the same six corners).
    fn wedge(&mut self) {
        let h = 0.5;
        let v = [
            Vec3::new(-h, -h, -h), Vec3::new(h, -h, -h), Vec3::new(h, -h, h), Vec3::new(-h, -h, h), // bottom
            Vec3::new(-h, h, h), Vec3::new(h, h, h),                                                 // top edge, at the back
        ];
        let inside = Vec3::new(0.0, -0.2, 0.2);
        self.facet(v[0], v[1], v[2], inside); // bottom
        self.facet(v[0], v[2], v[3], inside);
        self.facet(v[3], v[2], v[5], inside); // back
        self.facet(v[3], v[5], v[4], inside);
        self.facet(v[0], v[4], v[5], inside); // slope
        self.facet(v[0], v[5], v[1], inside);
        self.facet(v[0], v[3], v[4], inside); // left side
        self.facet(v[1], v[5], v[2], inside); // right side
    }

    /// An upright cylinder filling the unit box: smooth round side, flat caps.
    fn cylinder(&mut self) {
        let (n, r, h) = (32, 0.5, 0.5);
        let at = |i: usize| {
            let a = i as f32 / n as f32 * TAU;
            Vec3::new(a.cos(), 0.0, a.sin())
        };
        for i in 0..n {
            let (d0, d1) = (at(i), at(i + 1));
            let (b0, b1) = (d0 * r - Vec3::Y * h, d1 * r - Vec3::Y * h);
            let (t0, t1) = (d0 * r + Vec3::Y * h, d1 * r + Vec3::Y * h);
            // Side: outward normals per vertex so it shades round.
            for (p, nn) in [(b0, d0), (t1, d1), (b1, d1), (b0, d0), (t0, d0), (t1, d1)] {
                self.vertex(p, nn, Vec2::ZERO);
            }
            self.facet(Vec3::Y * h, t0, t1, Vec3::ZERO);
            self.facet(-Vec3::Y * h, b0, b1, Vec3::ZERO);
        }
    }

    /// A smooth sphere: each vertex's normal points straight out, so the
    /// lighting rounds it off instead of showing facets.
    fn sphere(&mut self, center: Vec3, r: f32) {
        let (rings, segments) = (16, 32);
        let at = |ring: usize, seg: usize| {
            let theta = ring as f32 / rings as f32 * PI; // 0 at the top
            let phi = seg as f32 / segments as f32 * TAU;
            Vec3::new(theta.sin() * phi.cos(), theta.cos(), theta.sin() * phi.sin())
        };
        for ring in 0..rings {
            for seg in 0..segments {
                let (a, b) = (at(ring, seg), at(ring + 1, seg));
                let (c, d) = (at(ring + 1, seg + 1), at(ring, seg + 1));
                for n in [a, c, b, a, d, c] {
                    self.vertex(center + n * r, n, Vec2::ZERO);
                }
            }
        }
    }

    /// A square picture projected straight back onto the front of the
    /// head and lifted a hair off it, like a sticker.
    fn face_patch(&mut self) {
        let steps = 16;
        let r = HEAD_RADIUS * 1.006;
        let point = |i: usize, j: usize| -> Option<(Vec3, Vec3, Vec2)> {
            let uv = Vec2::new(i as f32 / steps as f32, j as f32 / steps as f32);
            // Seen from the front, +X is on the viewer's right... of the
            // character, so u runs from +X to -X for the picture to read
            // the right way round.
            let x = (0.5 - uv.x) * DECAL_SIZE;
            let y = (0.5 - uv.y) * DECAL_SIZE - 0.05;
            let z2 = r * r - x * x - y * y;
            if z2 < (r * 0.25).powi(2) {
                return None; // off the edge of the head
            }
            let n = Vec3::new(x, y, z2.sqrt()) / r;
            Some((HEAD_CENTER + n * r, n, uv))
        };
        for i in 0..steps {
            for j in 0..steps {
                let quad = [point(i, j), point(i + 1, j), point(i + 1, j + 1), point(i, j + 1)];
                let [Some(a), Some(b), Some(c), Some(d)] = quad else { continue };
                // Counter-clockwise seen from the front (+Z). (u runs toward
                // -X and v runs down, so a -> b -> c already turns that way.)
                for v in [a, b, c, a, c, d] {
                    self.vertex(v.0, v.1, v.2);
                }
            }
        }
    }
}

// --- face pictures ---------------------------------------------------------

/// Each face is a 64x64 pixel-art picture; slot 0 of the atlas is plain
/// white, for everything that isn't textured.
pub(crate) const CELL: u32 = 64;
pub(crate) const ATLAS_SLOTS: u32 = 1 + Face::ALL.len() as u32;

/// Which atlas slot a face lives in.
pub(crate) fn face_slot(face: Face) -> u32 {
    1 + Face::ALL.iter().position(|f| *f == face).unwrap() as u32
}

/// The atlas as RGBA8 pixels, ATLAS_SLOTS cells wide and one cell tall.
pub(crate) fn face_atlas() -> Vec<u8> {
    let width = CELL * ATLAS_SLOTS;
    let mut px = vec![0u8; (width * CELL * 4) as usize];
    for y in 0..CELL {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            let slot = x / CELL;
            let ink = slot > 0 && face_ink(Face::ALL[slot as usize - 1], (x % CELL) as f32 + 0.5, y as f32 + 0.5);
            let rgba = if slot == 0 {
                [255, 255, 255, 255]
            } else if ink {
                [20, 20, 20, 255]
            } else {
                [0, 0, 0, 0]
            };
            px[i..i + 4].copy_from_slice(&rgba);
        }
    }
    px
}

/// Is the pixel at (x, y) (0..64, y down) part of this face's drawing?
fn face_ink(face: Face, x: f32, y: f32) -> bool {
    let p = Vec2::new(x, y);
    let ellipse = |cx: f32, cy: f32, rx: f32, ry: f32| ((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2) <= 1.0;
    let ring = |cx: f32, cy: f32, r: f32, w: f32| (p.distance(Vec2::new(cx, cy)) - r).abs() <= w / 2.0;
    let line = |a: (f32, f32), b: (f32, f32), w: f32| segment_distance(p, a.into(), b.into()) <= w / 2.0;
    let curve = |a: (f32, f32), c: (f32, f32), b: (f32, f32), w: f32| {
        let (a, c, b) = (Vec2::from(a), Vec2::from(c), Vec2::from(b));
        let at = |t: f32| a * (1.0 - t) * (1.0 - t) + c * 2.0 * t * (1.0 - t) + b * t * t;
        (0..16).any(|i| segment_distance(p, at(i as f32 / 16.0), at((i + 1) as f32 / 16.0)) <= w / 2.0)
    };
    match face {
        Face::Smile => {
            ellipse(21.0, 25.0, 4.0, 7.0)
                || ellipse(43.0, 25.0, 4.0, 7.0)
                || curve((18.0, 41.0), (32.0, 52.0), (46.0, 41.0), 3.2)
        }
        Face::Happy => {
            line((15.0, 28.0), (21.0, 21.0), 3.2)
                || line((21.0, 21.0), (27.0, 28.0), 3.2)
                || line((37.0, 28.0), (43.0, 21.0), 3.2)
                || line((43.0, 21.0), (49.0, 28.0), 3.2)
                || (ellipse(32.0, 40.0, 11.0, 9.0) && y >= 40.0)
        }
        Face::Surprised => {
            ring(21.0, 25.0, 5.0, 2.6) || ring(43.0, 25.0, 5.0, 2.6) || ellipse(32.0, 45.0, 3.5, 5.0)
        }
        Face::Determined => {
            ellipse(21.0, 27.0, 3.5, 5.0)
                || ellipse(43.0, 27.0, 3.5, 5.0)
                || line((14.0, 16.0), (27.0, 20.0), 3.2)
                || line((50.0, 16.0), (37.0, 20.0), 3.2)
                || line((24.0, 43.0), (40.0, 43.0), 3.2)
        }
    }
}

fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_avatar_is_about_five_studs_tall() {
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for m in meshes() {
            for v in &m.vertices {
                lo = lo.min(v.position[1]);
                hi = hi.max(v.position[1]);
                assert!(v.position[0].abs() <= 1.85 && v.position[2].abs() <= 0.9);
            }
        }
        assert!((lo + 2.5).abs() < 0.01, "feet at {lo}");
        assert!((hi - 2.66).abs() < 0.05, "top of head at {hi}");
    }

    #[test]
    fn the_head_is_smooth_and_the_decal_sits_just_outside_it() {
        let ms = meshes();
        let decal = ms.iter().find(|m| m.slot == Slot::Decal).unwrap();
        assert!(!decal.vertices.is_empty());
        for v in &decal.vertices {
            let d = Vec3::from(v.position).distance(HEAD_CENTER);
            assert!(d > HEAD_RADIUS && d < HEAD_RADIUS * 1.01, "decal at {d}");
            assert!(v.position[2] > 0.0, "decal on the front");
            assert!((0.0..=1.0).contains(&v.uv[0]) && (0.0..=1.0).contains(&v.uv[1]));
        }
    }

    #[test]
    fn decal_triangles_face_outward_so_they_are_not_culled() {
        let ms = meshes();
        let decal = ms.iter().find(|m| m.slot == Slot::Decal).unwrap();
        for tri in decal.vertices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(tri[k].position));
            let facing = (b - a).cross(c - a).dot((a + b + c) / 3.0 - HEAD_CENTER);
            assert!(facing > 0.0, "decal triangle wound clockwise");
        }
    }

    #[test]
    fn head_triangles_face_outward() {
        let ms = meshes();
        let skin = ms.iter().find(|m| m.slot == Slot::Skin).unwrap();
        for tri in skin.vertices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(tri[k].position));
            let mid = (a + b + c) / 3.0;
            let area = (b - a).cross(c - a);
            if mid.distance(HEAD_CENTER) < HEAD_RADIUS * 1.01 && area.length() > 1e-6 {
                assert!(area.dot(mid - HEAD_CENTER) > 0.0, "head triangle wound inward");
            }
        }
    }

    #[test]
    fn shape_meshes_fill_the_unit_box_and_face_outward() {
        for (shape, verts) in brixo_core::Shape::ALL.iter().zip(shape_meshes()) {
            assert!(!verts.is_empty(), "{shape:?}");
            for v in &verts {
                assert!(v.position.iter().all(|c| c.abs() <= 0.5001), "{shape:?} pokes out");
            }
            for tri in verts.chunks(3) {
                let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(tri[k].position));
                let area = (b - a).cross(c - a);
                if area.length() > 1e-6 {
                    let mid = (a + b + c) / 3.0;
                    // The wedge's centre of mass sits low and back.
                    let inside = if *shape == brixo_core::Shape::Wedge { Vec3::new(0.0, -0.2, 0.2) } else { Vec3::ZERO };
                    assert!(area.dot(mid - inside) > 0.0, "{shape:?} triangle wound inward");
                }
            }
        }
    }

    #[test]
    fn every_face_draws_something_and_the_white_slot_is_solid() {
        let px = face_atlas();
        let width = CELL * ATLAS_SLOTS;
        for slot in 0..ATLAS_SLOTS {
            let inked = (0..CELL * CELL)
                .filter(|k| {
                    let (x, y) = (slot * CELL + k % CELL, k / CELL);
                    px[((y * width + x) * 4 + 3) as usize] == 255
                })
                .count();
            if slot == 0 {
                assert_eq!(inked, (CELL * CELL) as usize);
            } else {
                assert!(inked > 60 && inked < 800, "slot {slot} has {inked} ink pixels");
            }
        }
    }
}
