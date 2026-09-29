//! The Brixo avatar: a blocky body with one smooth, round head, and a
//! pixel-art face printed on it like a decal.
//!
//! Every mesh is in the character's own space: the origin is the centre of
//! its 5-stud-tall body (feet at y = -2.5), and it faces +Z. Pieces that
//! share a colour are merged into one mesh, so a player is drawn with a
//! handful of instances, all with the same transform.

use std::f32::consts::{PI, TAU};

use brixo_core::{BodyPart, Face, Hat, Pants, PlayerProps, Shirt, Sleeves, TShirt};
use glam::{Vec2, Vec3};

pub(crate) mod art;
mod hats;

use crate::Vertex;

/// Which colour a mesh is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    /// One of the player's six body colours.
    Body(BodyPart),
    /// The colour their shirt is worn in.
    Shirt,
    /// The colour their pants are worn in.
    Pants,
    /// White, so a full-colour picture shows as it is (the face, a
    /// t-shirt picture).
    Decal,
    /// A fixed colour (accessories).
    Paint([u8; 3]),
}

/// Which picture (atlas cell) a mesh is printed with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tex {
    None,
    Face,
    ShirtAround,
    ShirtFront,
    Pants,
    TShirt,
}

/// Which players a mesh is drawn for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Show {
    Always,
    /// Wearing this accessory.
    Hat(Hat),
    /// Wearing a shirt (any shirt).
    Shirt,
    /// Wearing a shirt with sleeves this long.
    Sleeves(Sleeves),
    /// Wearing long pants.
    PantsLong,
    /// Wearing shorts.
    PantsShort,
    /// Wearing a t-shirt picture.
    TShirt,
}

/// Whether a mesh shows on this player.
pub(crate) fn shows(show: Show, p: &PlayerProps) -> bool {
    match show {
        Show::Always => true,
        Show::Hat(h) => p.hats.contains(&Some(h)),
        Show::Shirt => p.shirt != Shirt::None,
        Show::Sleeves(s) => p.shirt != Shirt::None && p.shirt.sleeves() == s,
        Show::PantsLong => p.pants != Pants::None && !p.pants.short(),
        Show::PantsShort => p.pants.short(),
        Show::TShirt => p.tshirt.is_some(),
    }
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
    pub show: Show,
    pub tex: Tex,
    pub vertices: Vec<Vertex>,
}

pub(crate) const HEAD_CENTER: Vec3 = Vec3::new(0.0, 1.8, 0.0);
pub(crate) const HEAD_RADIUS: f32 = 0.86;
/// How big the face picture is, wrapped onto the front of the head.
const DECAL_SIZE: f32 = 1.25;
/// How big the t-shirt picture is on the front of the torso.
const TSHIRT_SIZE: f32 = 1.24;

pub(crate) fn meshes() -> Vec<AvatarMesh> {
    let mut out = Vec::new();
    let mut piece = |slot, limb, show, tex, build: &dyn Fn(&mut Mesh)| {
        let mut m = Mesh::default();
        build(&mut m);
        out.push(AvatarMesh { slot, limb, show, tex, vertices: m.vertices });
    };
    let all = |_: Vec3| true;
    // The character faces +Z, so its right side is -X. Body parts first;
    // clothes are shells a little bigger than what they cover.
    for (side, arm, leg, arm_part, leg_part) in [
        (1.0f32, Limb::ArmLeft, Limb::LegLeft, BodyPart::LeftArm, BodyPart::LeftLeg),
        (-1.0, Limb::ArmRight, Limb::LegRight, BodyPart::RightArm, BodyPart::RightLeg),
    ] {
        piece(Slot::Body(leg_part), leg, Show::Always, Tex::None, &|m| m.cuboid(Vec3::new(side * 0.49, -1.645, 0.0), Vec3::new(0.84, 1.69, 0.9), &all));
        piece(Slot::Pants, leg, Show::PantsLong, Tex::Pants, &|m| m.cuboid(Vec3::new(side * 0.49, -1.65, 0.0), Vec3::new(0.88, 1.7, 0.94), &all));
        piece(Slot::Pants, leg, Show::PantsShort, Tex::Pants, &|m| m.cuboid(Vec3::new(side * 0.49, -1.21, 0.0), Vec3::new(0.88, 0.82, 0.94), &all));
        piece(Slot::Body(arm_part), arm, Show::Always, Tex::None, &|m| m.cuboid(Vec3::new(side * 1.43, 0.04, 0.0), Vec3::new(0.72, 1.84, 0.82), &all));
        piece(Slot::Shirt, arm, Show::Sleeves(Sleeves::Long), Tex::ShirtAround, &|m| {
            m.cuboid(Vec3::new(side * 1.43, 0.295, 0.0), Vec3::new(0.8, 1.35, 0.9), &all)
        });
        piece(Slot::Shirt, arm, Show::Sleeves(Sleeves::Short), Tex::ShirtAround, &|m| {
            m.cuboid(Vec3::new(side * 1.43, 0.635, 0.0), Vec3::new(0.8, 0.67, 0.9), &all)
        });
    }
    let torso = Vec3::new(0.0, 0.125, 0.0);
    piece(Slot::Body(BodyPart::Torso), Limb::Body, Show::Always, Tex::None, &|m| m.cuboid(torso, Vec3::new(2.0, 1.85, 1.02), &all));
    let shell = Vec3::new(2.04, 1.87, 1.06);
    piece(Slot::Shirt, Limb::Body, Show::Shirt, Tex::ShirtFront, &|m| m.cuboid(torso, shell, &|n: Vec3| n.z > 0.5));
    piece(Slot::Shirt, Limb::Body, Show::Shirt, Tex::ShirtAround, &|m| m.cuboid(torso, shell, &|n: Vec3| n.z <= 0.5));
    piece(Slot::Decal, Limb::Body, Show::TShirt, Tex::TShirt, &|m| m.tshirt_patch(Vec3::new(0.0, 0.2, shell.z / 2.0 + 0.006)));
    piece(Slot::Body(BodyPart::Head), Limb::Head, Show::Always, Tex::None, &|m| m.sphere(HEAD_CENTER, HEAD_RADIUS));
    piece(Slot::Decal, Limb::Head, Show::Always, Tex::Face, &|m| m.face_patch());
    for &hat in Hat::ALL {
        let limb = if hat.slot().on_head() { Limb::Head } else { Limb::Body };
        for (color, m) in hats::hat_pieces(hat) {
            out.push(AvatarMesh { slot: Slot::Paint(color), limb, show: Show::Hat(hat), tex: Tex::None, vertices: m.vertices });
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
                brixo_core::Shape::Block => m.cuboid(Vec3::ZERO, Vec3::ONE, &|_| true),
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

    /// A box with flat faces, counter-clockwise seen from outside: the
    /// sides whose outward normal passes `keep`. Each side shows a whole
    /// picture, the right way up and round seen from outside.
    fn cuboid(&mut self, center: Vec3, size: Vec3, keep: &dyn Fn(Vec3) -> bool) {
        let (cube, indices) = crate::cube();
        for tri in indices.chunks(3) {
            if !keep(Vec3::from(cube[tri[0] as usize].normal)) {
                continue;
            }
            for &i in tri {
                let v = cube[i as usize];
                let p = center + Vec3::from(v.position) * size;
                self.vertex(p, Vec3::from(v.normal), Vec2::from(v.uv));
            }
        }
    }

    /// A flat square picture facing +Z (the front of the torso), centred
    /// on `center`, reading left to right seen from the front.
    fn tshirt_patch(&mut self, center: Vec3) {
        let h = TSHIRT_SIZE / 2.0;
        let corner = |u: f32, v: f32| (center + Vec3::new((u - 0.5) * 2.0 * h, (0.5 - v) * 2.0 * h, 0.0), Vec2::new(u, v));
        let (a, b, c, d) = (corner(0.0, 1.0), corner(1.0, 1.0), corner(1.0, 0.0), corner(0.0, 0.0));
        for (p, uv) in [a, b, c, a, c, d] {
            self.vertex(p, Vec3::Z, uv);
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
            // Seen from the front, the character's left (+X) is on the
            // viewer's right, so u runs from -X to +X for the picture to
            // read the right way round (as on the torso's front).
            let x = (uv.x - 0.5) * DECAL_SIZE;
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
                // Counter-clockwise seen from the front (+Z): u runs toward
                // +X and v runs down, so a -> d -> c turns that way.
                for v in [a, c, b, a, d, c] {
                    self.vertex(v.0, v.1, v.2);
                }
            }
        }
    }
}

// --- the atlas ---------------------------------------------------------------

/// Every picture lives in one texture, a grid of 64x64 cells: cell 0 is
/// plain white (for everything without a picture), then the faces, the
/// material textures, shirts (the back and sides, then the front), pants
/// and t-shirt pictures.
pub(crate) const CELL: u32 = 64;
pub(crate) const COLS: u32 = 16;
/// Materials with a texture (Plastic and Neon are plain).
pub(crate) const TEXTURED: [brixo_core::Material; 5] = [
    brixo_core::Material::Wood,
    brixo_core::Material::Brick,
    brixo_core::Material::Metal,
    brixo_core::Material::Grass,
    brixo_core::Material::Concrete,
];
const FACES: u32 = Face::ALL.len() as u32;
/// Shirts and pants with a picture (all but "none", which is first).
const SHIRTS: u32 = Shirt::ALL.len() as u32 - 1;
const PANTS: u32 = Pants::ALL.len() as u32 - 1;
const TSHIRTS: u32 = TShirt::ALL.len() as u32;
const FIRST_MATERIAL: u32 = 1 + FACES;
const FIRST_SHIRT: u32 = FIRST_MATERIAL + TEXTURED.len() as u32;
const FIRST_SHIRT_FRONT: u32 = FIRST_SHIRT + SHIRTS;
const FIRST_PANTS: u32 = FIRST_SHIRT_FRONT + SHIRTS;
const FIRST_TSHIRT: u32 = FIRST_PANTS + PANTS;
pub(crate) const ATLAS_CELLS: u32 = FIRST_TSHIRT + TSHIRTS;
pub(crate) const ROWS: u32 = ATLAS_CELLS.div_ceil(COLS);

fn position<T: PartialEq>(all: &[T], x: &T) -> u32 {
    all.iter().position(|y| y == x).unwrap() as u32
}

/// Which atlas cell a material's texture lives in, if it has one.
pub(crate) fn material_slot(m: brixo_core::Material) -> Option<u32> {
    TEXTURED.iter().position(|t| *t == m).map(|i| FIRST_MATERIAL + i as u32)
}

/// Which atlas cell a face lives in.
pub(crate) fn face_slot(face: Face) -> u32 {
    1 + position(Face::ALL, &face)
}

/// Which cell a mesh's picture is in, for this player (None: plain white).
pub(crate) fn tex_slot(tex: Tex, p: &PlayerProps) -> Option<u32> {
    match tex {
        Tex::None => None,
        Tex::Face => Some(face_slot(p.face)),
        Tex::ShirtAround | Tex::ShirtFront if p.shirt == Shirt::None => None,
        Tex::ShirtAround => Some(FIRST_SHIRT + position(Shirt::ALL, &p.shirt) - 1),
        Tex::ShirtFront => Some(FIRST_SHIRT_FRONT + position(Shirt::ALL, &p.shirt) - 1),
        Tex::Pants if p.pants == Pants::None => None,
        Tex::Pants => Some(FIRST_PANTS + position(Pants::ALL, &p.pants) - 1),
        Tex::TShirt => p.tshirt.map(|t| FIRST_TSHIRT + position(TShirt::ALL, &t)),
    }
}

/// One cell's pixels (64x64 RGBA).
fn cell_pixel(slot: u32, x: u32, y: u32) -> [u8; 4] {
    // Clothes and t-shirts are drawn at 32x32 and shown in 2x2 blocks.
    let (hx, hy) = ((x / 2) as i32, (y / 2) as i32);
    let grey = |v: f32| {
        let v = (v.clamp(0.0, 1.0) * 255.0) as u8;
        [v, v, v, 255]
    };
    if slot == 0 {
        [255, 255, 255, 255]
    } else if slot < FIRST_MATERIAL {
        match art::face_pixel(Face::ALL[slot as usize - 1], x as f32 + 0.5, y as f32 + 0.5) {
            Some([r, g, b]) => [r, g, b, 255],
            None => [0, 0, 0, 0],
        }
    } else if slot < FIRST_SHIRT {
        // Brightness above 1 can't be stored, so textures are kept at 0..1
        // and the shader scales them back up by 1.25.
        grey(material_pixel(TEXTURED[(slot - FIRST_MATERIAL) as usize], x, y) / 1.25)
    } else if slot < FIRST_SHIRT_FRONT {
        grey(art::shirt_pixel(Shirt::ALL[(slot - FIRST_SHIRT + 1) as usize], art::Side::Around, hx, hy))
    } else if slot < FIRST_PANTS {
        grey(art::shirt_pixel(Shirt::ALL[(slot - FIRST_SHIRT_FRONT + 1) as usize], art::Side::Front, hx, hy))
    } else if slot < FIRST_TSHIRT {
        grey(art::pants_pixel(Pants::ALL[(slot - FIRST_PANTS + 1) as usize], hx, hy))
    } else if slot < ATLAS_CELLS {
        art::tshirt_pixel(TShirt::ALL[(slot - FIRST_TSHIRT) as usize], hx, hy)
    } else {
        [0, 0, 0, 0]
    }
}

/// The atlas as RGBA8 pixels, COLS cells wide and ROWS cells tall.
pub(crate) fn face_atlas() -> Vec<u8> {
    let (width, height) = (CELL * COLS, CELL * ROWS);
    let mut px = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let slot = (y / CELL) * COLS + x / CELL;
            let i = ((y * width + x) * 4) as usize;
            px[i..i + 4].copy_from_slice(&cell_pixel(slot, x % CELL, y % CELL));
        }
    }
    px
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_avatar_is_about_five_studs_tall() {
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for m in meshes().into_iter().filter(|m| !matches!(m.show, Show::Hat(_))) {
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
        let decal = ms.iter().find(|m| m.tex == Tex::Face).unwrap();
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
        let decal = ms.iter().find(|m| m.tex == Tex::Face).unwrap();
        for tri in decal.vertices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(tri[k].position));
            let facing = (b - a).cross(c - a).dot((a + b + c) / 3.0 - HEAD_CENTER);
            assert!(facing > 0.0, "decal triangle wound clockwise");
        }
    }

    #[test]
    fn head_triangles_face_outward() {
        let ms = meshes();
        let skin = ms.iter().find(|m| m.slot == Slot::Body(BodyPart::Head)).unwrap();
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
    fn the_atlas_has_a_white_cell_solid_materials_and_every_picture() {
        let px = face_atlas();
        let width = CELL * COLS;
        let count = |slot: u32| {
            (0..CELL * CELL)
                .filter(|k| {
                    let (x, y) = ((slot % COLS) * CELL + k % CELL, (slot / COLS) * CELL + k / CELL);
                    px[((y * width + x) * 4 + 3) as usize] == 255
                })
                .count()
        };
        let solid = (CELL * CELL) as usize;
        assert_eq!(count(0), solid);
        for m in TEXTURED {
            assert_eq!(count(material_slot(m).unwrap()), solid);
        }
        let mut p = PlayerProps::default();
        for &f in Face::ALL {
            p.face = f;
            let n = count(tex_slot(Tex::Face, &p).unwrap());
            assert!(n > 60 && n < 900, "{f:?} has {n} pixels");
        }
        for &s in &Shirt::ALL[1..] {
            p.shirt = s;
            assert_eq!(count(tex_slot(Tex::ShirtAround, &p).unwrap()), solid, "{s:?}");
            assert_eq!(count(tex_slot(Tex::ShirtFront, &p).unwrap()), solid, "{s:?}");
        }
        for &t in TShirt::ALL {
            p.tshirt = Some(t);
            let n = count(tex_slot(Tex::TShirt, &p).unwrap());
            assert!(n > 300 && n < solid, "{t:?} has {n} pixels");
        }
        p.shirt = Shirt::None;
        assert_eq!(tex_slot(Tex::ShirtFront, &p), None);
        assert!(ATLAS_CELLS <= COLS * ROWS);
    }

    #[test]
    fn clothes_show_by_what_is_worn() {
        let ms = meshes();
        let mut p = PlayerProps::default();
        let showing = |p: &PlayerProps, slot: Slot| ms.iter().filter(|m| m.slot == slot && shows(m.show, p)).count();
        p.shirt = Shirt::Tank;
        assert_eq!(showing(&p, Slot::Shirt), 2, "a tank top is just the torso (front and around)");
        p.shirt = Shirt::Hoodie;
        assert_eq!(showing(&p, Slot::Shirt), 4, "and long sleeves");
        p.shirt = Shirt::None;
        assert_eq!(showing(&p, Slot::Shirt), 0);
        p.pants = Pants::Shorts;
        assert_eq!(showing(&p, Slot::Pants), 2);
        p.pants = Pants::None;
        assert_eq!(showing(&p, Slot::Pants), 0);
        assert_eq!(showing(&p, Slot::Decal), 1, "just the face");
        p.tshirt = Some(TShirt::Star);
        assert_eq!(showing(&p, Slot::Decal), 2);
    }

    #[test]
    fn the_tshirt_picture_faces_front_and_is_not_culled() {
        let ms = meshes();
        let t = ms.iter().find(|m| m.tex == Tex::TShirt).unwrap();
        for tri in t.vertices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(tri[k].position));
            assert!((b - a).cross(c - a).z > 0.0);
        }
    }
}

/// The avatar and accessory meshes, for the website's 3D preview
/// (crates/brixo-web/src/web/avatar-model.json; its pictures are
/// avatar-atlas.png next to it), so the site draws exactly what the game
/// draws. Positions are stored as whole thousandths of a stud (i16),
/// normals as i8 (x127), UVs as u16 (x65535), all base64.
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
        match (m.slot, m.show) {
            (Slot::Paint([r, g, b]), Show::Hat(hat)) => {
                let entry = hats.iter_mut().find(|(h, _)| *h == hat).unwrap();
                entry.1.push(format!("{{\"color\":[{r},{g},{b}],{}}}", mesh(&m.vertices, false)));
            }
            (slot, show) => {
                let slot = match slot {
                    Slot::Body(part) => part.name().to_string(),
                    Slot::Shirt => "shirt".into(),
                    Slot::Pants => "pants".into(),
                    Slot::Decal => "decal".into(),
                    Slot::Paint(_) => unreachable!("only accessories are painted"),
                };
                let show = match show {
                    Show::Always => "always",
                    Show::Shirt => "shirt",
                    Show::Sleeves(Sleeves::Long) => "long_sleeves",
                    Show::Sleeves(Sleeves::Short) => "short_sleeves",
                    Show::Sleeves(Sleeves::None) => "no_sleeves",
                    Show::PantsLong => "pants_long",
                    Show::PantsShort => "pants_short",
                    Show::TShirt => "tshirt",
                    Show::Hat(_) => unreachable!(),
                };
                let tex = match m.tex {
                    Tex::None => "none",
                    Tex::Face => "face",
                    Tex::ShirtAround => "shirt_around",
                    Tex::ShirtFront => "shirt_front",
                    Tex::Pants => "pants",
                    Tex::TShirt => "tshirt",
                };
                body.push(format!(
                    "{{\"slot\":\"{slot}\",\"limb\":\"{}\",\"show\":\"{show}\",\"tex\":\"{tex}\",{}}}",
                    limb(m.limb),
                    mesh(&m.vertices, m.tex != Tex::None)
                ));
            }
        }
    }
    let hats: Vec<String> = hats
        .iter()
        .map(|(h, pieces)| {
            let on = if h.slot().on_head() { "head" } else { "body" };
            format!("{{\"name\":\"{}\",\"title\":\"{}\",\"slot\":\"{}\",\"limb\":\"{on}\",\"pieces\":[{}]}}", h.name(), h.title(), h.slot().name(), pieces.join(","))
        })
        .collect();
    // Where each picture is in the atlas.
    let names = |list: Vec<(&str, u32)>| list.iter().map(|(n, i)| format!("\"{n}\":{i}")).collect::<Vec<_>>().join(",");
    let mut p = PlayerProps::default();
    let faces = names(Face::ALL.iter().map(|f| (f.name(), face_slot(*f))).collect());
    let mut around = Vec::new();
    let mut front = Vec::new();
    for &s in &Shirt::ALL[1..] {
        p.shirt = s;
        around.push((s.name(), tex_slot(Tex::ShirtAround, &p).unwrap()));
        front.push((s.name(), tex_slot(Tex::ShirtFront, &p).unwrap()));
    }
    let mut pants = Vec::new();
    for &x in &Pants::ALL[1..] {
        p.pants = x;
        pants.push((x.name(), tex_slot(Tex::Pants, &p).unwrap()));
    }
    let mut tshirts = Vec::new();
    for &t in TShirt::ALL {
        p.tshirt = Some(t);
        tshirts.push((t.name(), tex_slot(Tex::TShirt, &p).unwrap()));
    }
    let short: Vec<String> = Shirt::ALL.iter().map(|s| format!("\"{}\":\"{}\"", s.name(), match s.sleeves() {
        Sleeves::None => "no_sleeves",
        Sleeves::Short => "short_sleeves",
        Sleeves::Long => "long_sleeves",
    })).collect();
    let shorts: Vec<String> = Pants::ALL.iter().filter(|p| p.short()).map(|p| format!("\"{}\"", p.name())).collect();
    format!(
        "{{\"cell\":{CELL},\"cols\":{COLS},\"rows\":{ROWS},\"max_hats\":{},\n\"atlas\":{{\"face\":{{{faces}}},\"shirt_around\":{{{}}},\"shirt_front\":{{{}}},\"pants\":{{{}}},\"tshirt\":{{{}}}}},\n\"sleeves\":{{{}}},\"shorts\":[{}],\n\"body\":[\n{}\n],\"hats\":[\n{}\n]}}\n",
        brixo_core::MAX_HATS,
        names(around),
        names(front),
        names(pants),
        names(tshirts),
        short.join(","),
        shorts.join(","),
        body.join(",\n"),
        hats.join(",\n"),
    )
}

#[cfg(test)]
mod web_model {
    #[test]
    fn the_websites_copy_of_the_avatar_model_is_up_to_date() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../brixo-web/src/web/avatar-model.json");
        let atlas_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../brixo-web/src/web/avatar-atlas.png");
        let fresh = super::web_model_json();
        let (w, h) = (super::CELL * super::COLS, super::CELL * super::ROWS);
        let pixels = super::face_atlas();
        if std::env::var_os("BRIXO_WRITE_MODEL").is_some() {
            std::fs::write(path, &fresh).unwrap();
            image::RgbaImage::from_raw(w, h, pixels).unwrap().save(atlas_path).unwrap();
            return;
        }
        let saved_atlas = image::open(atlas_path).map(|i| i.to_rgba8().into_raw()).unwrap_or_default();
        assert!(
            saved_atlas == pixels,
            "the avatar's pictures changed: run `BRIXO_WRITE_MODEL=1 cargo test -p brixo-render web_model` \
             (PowerShell: $env:BRIXO_WRITE_MODEL=1; cargo test -p brixo-render web_model; Remove-Item Env:BRIXO_WRITE_MODEL) \
             to update crates/brixo-web/src/web/avatar-atlas.png"
        );
        let saved = std::fs::read_to_string(path).unwrap_or_default().replace("\r\n", "\n");
        assert!(
            same_model(&saved, &fresh),
            "the avatar changed: run `BRIXO_WRITE_MODEL=1 cargo test -p brixo-render web_model` \
             (PowerShell: $env:BRIXO_WRITE_MODEL=1; cargo test -p brixo-render web_model; Remove-Item Env:BRIXO_WRITE_MODEL) \
             to update crates/brixo-web/src/web/avatar-model.json"
        );
    }

    /// The same model, allowing the last rounded step in a number: Windows,
    /// Mac and Linux work out sin and cos very slightly differently, so a
    /// value right on a rounding edge can land one step either side. Any
    /// real change to the avatar moves things far more than that.
    fn same_model(saved: &str, fresh: &str) -> bool {
        if saved == fresh {
            return true;
        }
        let (a, b): (Vec<&str>, Vec<&str>) = (saved.split('"').collect(), fresh.split('"').collect());
        if a.len() != b.len() {
            return false;
        }
        (0..a.len()).all(|i| {
            if a[i] == b[i] {
                return true;
            }
            // Only the packed numbers may differ: "p", "n" or "uv" came just before.
            let kind = if i >= 2 { a[i - 2] } else { "" };
            let (Some(x), Some(y)) = (unb64(a[i]), unb64(b[i])) else { return false };
            if x.len() != y.len() {
                return false;
            }
            let near = |p: i32, q: i32| (p - q).abs() <= 1;
            match kind {
                "p" => x.chunks(2).zip(y.chunks(2)).all(|(p, q)| near(i16::from_le_bytes([p[0], p[1]]) as i32, i16::from_le_bytes([q[0], q[1]]) as i32)),
                "n" => x.iter().zip(&y).all(|(p, q)| near(*p as i8 as i32, *q as i8 as i32)),
                "uv" => x.chunks(2).zip(y.chunks(2)).all(|(p, q)| near(u16::from_le_bytes([p[0], p[1]]) as i32, u16::from_le_bytes([q[0], q[1]]) as i32)),
                _ => false,
            }
        })
    }

    fn unb64(s: &str) -> Option<Vec<u8>> {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = Vec::with_capacity(s.len() / 4 * 3);
        for c in s.as_bytes().chunks(4) {
            if c.len() != 4 {
                return None;
            }
            let mut n = 0u32;
            let mut pad = 0;
            for &ch in c {
                n <<= 6;
                if ch == b'=' {
                    pad += 1;
                } else {
                    n |= T.iter().position(|&t| t == ch)? as u32;
                }
            }
            out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8][..3 - pad]);
        }
        Some(out)
    }

    #[test]
    fn the_up_to_date_check_forgives_rounding_but_not_real_changes() {
        let fresh = super::web_model_json();
        // Nudge the first position by one thousandth of a stud: forgiven.
        let at = fresh.find("\"p\":\"").unwrap() + 5;
        let end = at + fresh[at..].find('"').unwrap();
        let mut bytes = unb64(&fresh[at..end]).unwrap();
        let v = i16::from_le_bytes([bytes[0], bytes[1]]) + 1;
        bytes[..2].copy_from_slice(&v.to_le_bytes());
        let enc = |b: &[u8]| {
            const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut out = String::new();
            for c in b.chunks(3) {
                let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
                for k in 0..4 {
                    out.push(if k <= c.len() { T[(n >> (18 - 6 * k) & 63) as usize] as char } else { '=' });
                }
            }
            out
        };
        let nudged = format!("{}{}{}", &fresh[..at], enc(&bytes), &fresh[end..]);
        assert!(nudged != fresh && same_model(&nudged, &fresh));
        // Moved by a tenth of a stud: that's a real change.
        let v = v + 100;
        bytes[..2].copy_from_slice(&v.to_le_bytes());
        let moved = format!("{}{}{}", &fresh[..at], enc(&bytes), &fresh[end..]);
        assert!(!same_model(&moved, &fresh));
        // So is a hat's name.
        assert!(!same_model(&fresh.replace("top_hat", "tip_hat"), &fresh));
    }
}
