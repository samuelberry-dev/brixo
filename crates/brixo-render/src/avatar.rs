//! The Brixo avatar: a blocky body with one smooth, round head, and a
//! pixel-art face printed on it like a decal.
//!
//! Every mesh is in the character's own space: the origin is the centre of
//! its 5-stud-tall body (feet at y = -2.5), and it faces +Z. Pieces that
//! share a colour are merged into one mesh, so a player is drawn with a
//! handful of instances, all with the same transform.

use std::f32::consts::{PI, TAU};

use brixo_core::{Face, Hat};
use glam::{Vec2, Vec3};

mod hats;

use crate::Vertex;

/// Which of the player's colours a mesh is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    Skin,
    Shirt,
    Pants,
    /// The face decal: white, textured with the face's picture.
    Decal,
    /// A fixed colour (hats).
    Paint([u8; 3]),
}

/// Which body part a mesh belongs to: arms and legs swing about a pivot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Limb {
    Body,
    /// The head and face: part of the body, except when falling apart.
    Head,
    ArmLeft,
    ArmRight,
    LegLeft,
    LegRight,
}

impl Limb {
    /// The shoulder or hip each limb turns around (character space).
    pub fn pivot(self) -> Vec3 {
        match self {
            Limb::Body => Vec3::ZERO,
            Limb::Head => HEAD_CENTER,
            Limb::ArmLeft => Vec3::new(1.43, 0.97, 0.0),
            Limb::ArmRight => Vec3::new(-1.43, 0.97, 0.0),
            Limb::LegLeft => Vec3::new(0.49, -0.8, 0.0),
            Limb::LegRight => Vec3::new(-0.49, -0.8, 0.0),
        }
    }
}

pub(crate) struct AvatarMesh {
    pub slot: Slot,
    pub limb: Limb,
    /// Only drawn on players wearing this hat.
    pub hat: Option<Hat>,
    pub vertices: Vec<Vertex>,
}

pub(crate) const HEAD_CENTER: Vec3 = Vec3::new(0.0, 1.8, 0.0);
pub(crate) const HEAD_RADIUS: f32 = 0.86;
/// How big the face picture is, wrapped onto the front of the head.
const DECAL_SIZE: f32 = 1.25;

pub(crate) fn meshes() -> Vec<AvatarMesh> {
    let mut out = Vec::new();
    let mut piece = |slot, limb, build: &dyn Fn(&mut Mesh)| {
        let mut m = Mesh::default();
        build(&mut m);
        out.push(AvatarMesh { slot, limb, hat: None, vertices: m.vertices });
    };
    // The character faces +Z, so its right side is -X.
    for (side, arm, leg) in [(1.0f32, Limb::ArmLeft, Limb::LegLeft), (-1.0, Limb::ArmRight, Limb::LegRight)] {
        // No shoes: the pants go all the way to the ground.
        piece(Slot::Pants, leg, &|m| m.cuboid(Vec3::new(side * 0.49, -1.65, 0.0), Vec3::new(0.88, 1.7, 0.94)));
        piece(Slot::Shirt, arm, &|m| m.cuboid(Vec3::new(side * 1.43, 0.295, 0.0), Vec3::new(0.8, 1.35, 0.9)));
        piece(Slot::Skin, arm, &|m| m.cuboid(Vec3::new(side * 1.43, -0.63, 0.0), Vec3::new(0.72, 0.5, 0.82)));
    }
    piece(Slot::Shirt, Limb::Body, &|m| m.cuboid(Vec3::new(0.0, 0.125, 0.0), Vec3::new(2.0, 1.85, 1.02)));
    piece(Slot::Skin, Limb::Head, &|m| m.sphere(HEAD_CENTER, HEAD_RADIUS));
    piece(Slot::Decal, Limb::Head, &|m| m.face_patch());
    for hat in Hat::ALL {
        for (color, m) in hats::hat_pieces(hat) {
            out.push(AvatarMesh { slot: Slot::Paint(color), limb: Limb::Head, hat: Some(hat), vertices: m.vertices });
        }
    }
    out
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
/// Materials with a texture (Plastic and Neon are plain).
pub(crate) const TEXTURED: [brixo_core::Material; 5] = [
    brixo_core::Material::Wood,
    brixo_core::Material::Brick,
    brixo_core::Material::Metal,
    brixo_core::Material::Grass,
    brixo_core::Material::Concrete,
];
pub(crate) const ATLAS_SLOTS: u32 = 1 + Face::ALL.len() as u32 + TEXTURED.len() as u32;

/// Which atlas slot a material's texture lives in, if it has one.
pub(crate) fn material_slot(m: brixo_core::Material) -> Option<u32> {
    TEXTURED.iter().position(|t| *t == m).map(|i| 1 + Face::ALL.len() as u32 + i as u32)
}

/// A small hash for texture noise: the same pixel always gets the same value.
fn noise(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263) ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}

/// Smooth, tileable blobs: noise on a coarse grid of `cell`-pixel squares
/// (wrapping every 64 pixels), blended between grid points. Gives patches
/// and mottling instead of per-pixel static.
fn blobs(x: u32, y: u32, cell: u32, seed: u32) -> f32 {
    let n = 64 / cell;
    let (gx, gy) = (x / cell, y / cell);
    let (fx, fy) = ((x % cell) as f32 / cell as f32, (y % cell) as f32 / cell as f32);
    let ease = |t: f32| t * t * (3.0 - 2.0 * t);
    let at = |i: u32, j: u32| noise(i % n, j % n, seed);
    let top = at(gx, gy) + (at(gx + 1, gy) - at(gx, gy)) * ease(fx);
    let bottom = at(gx, gy + 1) + (at(gx + 1, gy + 1) - at(gx, gy + 1)) * ease(fx);
    top + (bottom - top) * ease(fy)
}

/// Each textured material's average shade (as stored in the atlas), for
/// fading detail out in the distance.
pub(crate) fn material_average(m: brixo_core::Material) -> f32 {
    static AVERAGES: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    let all = AVERAGES.get_or_init(|| {
        brixo_core::Material::ALL
            .iter()
            .map(|&m| {
                let mut sum = 0.0;
                for y in 0..CELL {
                    for x in 0..CELL {
                        sum += (material_pixel(m, x, y) / 1.25).clamp(0.0, 1.0);
                    }
                }
                sum / (CELL * CELL) as f32
            })
            .collect()
    });
    all[brixo_core::Material::ALL.iter().position(|x| *x == m).unwrap()]
}

/// One pixel of a material texture (64x64, tiles seamlessly), as a
/// brightness around 1.0 that multiplies the part's colour.
fn material_pixel(m: brixo_core::Material, x: u32, y: u32) -> f32 {
    use brixo_core::Material::*;
    let n = noise(x, y, m as u32);
    match m {
        Wood => {
            // Four planks, with grain streaks and dark seams.
            let seam = y % 16 == 15;
            let plank = y / 16;
            let grain = noise(x / 6 + plank * 17, y, 3);
            let end = (x + plank * 23) % 64 == 0;
            if seam || end { 0.55 } else { 0.82 + grain * 0.12 + n * 0.06 }
        }
        Brick => {
            // Staggered bricks and light mortar.
            let row = y / 8;
            let shift = if row % 2 == 0 { 0 } else { 8 };
            let mortar = y % 8 == 7 || (x + shift) % 16 == 15;
            if mortar { 1.05 } else { 0.72 + noise((x + shift) / 16, row, 9) * 0.14 + n * 0.04 }
        }
        Metal => {
            // Diamond plate: raised bumps on brushed steel.
            let bx = (x % 8) as i32 - 4;
            let by = (y % 8) as i32 - 4;
            let bump = bx.abs() + by.abs() <= 2 && ((x / 8 + y / 8) % 2 == 0);
            let brushed = 0.86 + noise(x, y / 4, 5) * 0.08;
            if bump { brushed + 0.14 } else { brushed }
        }
        Grass => {
            // Soft light and dark patches, with short blades on top.
            let patch = blobs(x, y, 16, 7) * 0.6 + blobs(x, y, 8, 8) * 0.4;
            let mut v = 0.8 + (patch - 0.5) * 0.24;
            // One blade per 8x8 cell: a 1x3 stroke at a hashed spot, some
            // lighter (catching the sun), some darker.
            let (cx, cy) = (x / 8, y / 8);
            let h = noise(cx, cy, 21);
            let bx = cx * 8 + (h * 7.0) as u32;
            let by = cy * 8 + (noise(cx, cy, 22) * 5.0) as u32;
            if x == bx && (by..by + 3).contains(&y) {
                v += if h > 0.5 { 0.14 } else { -0.12 };
            }
            v
        }
        Concrete => {
            // Two slabs per tile each way: a dark seam with a lit edge, and
            // gentle mottling rather than speckle.
            let mut v = 0.9 + (blobs(x, y, 16, 11) - 0.5) * 0.1 + (blobs(x, y, 4, 12) - 0.5) * 0.03;
            if x % 32 == 31 || y % 32 == 31 {
                v = 0.7;
            } else if x % 32 == 0 || y % 32 == 0 {
                v += 0.05;
            }
            // A few small worn pits.
            if noise(x / 2, y / 2, 13) > 0.985 {
                v -= 0.1;
            }
            v
        }
        Plastic | Neon => 1.0,
    }
}

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
            let faces = Face::ALL.len() as u32;
            let rgba = if slot == 0 {
                [255, 255, 255, 255]
            } else if slot <= faces {
                let ink = face_ink(Face::ALL[slot as usize - 1], (x % CELL) as f32 + 0.5, y as f32 + 0.5);
                if ink { [20, 20, 20, 255] } else { [0, 0, 0, 0] }
            } else {
                // Brightness above 1 can't be stored, so textures are kept
                // at 0..1 and the shader scales them back up by 1.25.
                let b = (material_pixel(TEXTURED[(slot - faces - 1) as usize], x % CELL, y) / 1.25).clamp(0.0, 1.0);
                let v = (b * 255.0) as u8;
                [v, v, v, 255]
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
        for m in meshes().into_iter().filter(|m| m.hat.is_none()) {
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
            if slot == 0 || slot > Face::ALL.len() as u32 {
                assert_eq!(inked, (CELL * CELL) as usize, "white and material cells are solid");
            } else {
                assert!(inked > 60 && inked < 800, "slot {slot} has {inked} ink pixels");
            }
        }
    }
}

/// The avatar and hat meshes plus the face pictures, for the website's 3D
/// preview (crates/brixo-web/src/web/avatar-model.json), so the site draws
/// exactly what the game draws. Positions are stored as whole thousandths of
/// a stud (i16), normals as i8 (x127), UVs as u16 (x65535), all base64.
#[cfg(test)]
pub(crate) fn web_model_json() -> String {
    fn b64(bytes: &[u8]) -> String {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for c in bytes.chunks(3) {
            let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
            for k in 0..4 {
                out.push(if k <= c.len() { T[(n >> (18 - 6 * k) & 63) as usize] as char } else { '=' });
            }
        }
        out
    }
    let mesh = |vs: &[Vertex], uv: bool| {
        let mut p = Vec::new();
        let mut n = Vec::new();
        let mut t = Vec::new();
        for v in vs {
            for x in v.position {
                p.extend(((x * 1000.0).round() as i16).to_le_bytes());
            }
            for x in v.normal {
                n.push((x * 127.0).round() as i8 as u8);
            }
            for x in v.uv {
                t.extend(((x.clamp(0.0, 1.0) * 65535.0).round() as u16).to_le_bytes());
            }
        }
        let uv = if uv { format!(",\"uv\":\"{}\"", b64(&t)) } else { String::new() };
        format!("\"p\":\"{}\",\"n\":\"{}\"{uv}", b64(&p), b64(&n))
    };
    let limb = |l: Limb| match l {
        Limb::Body => "body",
        Limb::Head => "head",
        Limb::ArmLeft => "arm_left",
        Limb::ArmRight => "arm_right",
        Limb::LegLeft => "leg_left",
        Limb::LegRight => "leg_right",
    };
    let mut body = Vec::new();
    let mut hats: Vec<(Hat, Vec<String>)> = Hat::ALL.iter().map(|h| (*h, Vec::new())).collect();
    for m in meshes() {
        match (m.slot, m.hat) {
            (Slot::Paint([r, g, b]), Some(hat)) => {
                let entry = hats.iter_mut().find(|(h, _)| *h == hat).unwrap();
                entry.1.push(format!("{{\"color\":[{r},{g},{b}],{}}}", mesh(&m.vertices, false)));
            }
            (slot, _) => {
                let slot = match slot {
                    Slot::Skin => "skin",
                    Slot::Shirt => "shirt",
                    Slot::Pants => "pants",
                    Slot::Decal => "decal",
                    Slot::Paint(_) => unreachable!("only hats are painted"),
                };
                body.push(format!(
                    "{{\"slot\":\"{slot}\",\"limb\":\"{}\",{}}}",
                    limb(m.limb),
                    mesh(&m.vertices, slot == "decal")
                ));
            }
        }
    }
    let hats: Vec<String> = hats
        .iter()
        .map(|(h, pieces)| format!("{{\"name\":\"{}\",\"title\":\"{}\",\"pieces\":[{}]}}", h.name(), h.title(), pieces.join(",")))
        .collect();
    let faces: Vec<String> = Face::ALL
        .iter()
        .map(|f| {
            let mut bits = vec![0u8; (CELL * CELL / 8) as usize];
            for y in 0..CELL {
                for x in 0..CELL {
                    if face_ink(*f, x as f32 + 0.5, y as f32 + 0.5) {
                        let i = (y * CELL + x) as usize;
                        bits[i / 8] |= 1 << (i % 8);
                    }
                }
            }
            format!("\"{}\":\"{}\"", f.name(), b64(&bits))
        })
        .collect();
    format!(
        "{{\"cell\":{CELL},\"max_hats\":{},\"body\":[\n{}\n],\"hats\":[\n{}\n],\"faces\":{{{}}}}}\n",
        brixo_core::MAX_HATS,
        body.join(",\n"),
        hats.join(",\n"),
        faces.join(",")
    )
}

#[cfg(test)]
mod web_model {
    #[test]
    fn the_websites_copy_of_the_avatar_model_is_up_to_date() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../brixo-web/src/web/avatar-model.json");
        let fresh = super::web_model_json();
        if std::env::var_os("BRIXO_WRITE_MODEL").is_some() {
            std::fs::write(path, &fresh).unwrap();
            return;
        }
        let saved = std::fs::read_to_string(path).unwrap_or_default().replace("\r\n", "\n");
        assert!(
            saved == fresh,
            "the avatar changed: run `BRIXO_WRITE_MODEL=1 cargo test -p brixo-render web_model` \
             (PowerShell: $env:BRIXO_WRITE_MODEL=1; cargo test -p brixo-render web_model; Remove-Item Env:BRIXO_WRITE_MODEL) \
             to update crates/brixo-web/src/web/avatar-model.json"
        );
    }
}
