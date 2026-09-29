//! Draws a Brixo DataModel: every Part becomes an instanced cube.

mod avatar;

use brixo_core::{DataModel, Vec3 as BVec3};
use glam::{Mat4, Quat, Vec3};
use wgpu::util::DeviceExt;

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

// --- camera ----------------------------------------------------------------

pub struct Camera {
    pub position: Vec3,
    /// Radians.
    pub yaw: f32,
    /// Radians.
    pub pitch: f32,
    pub fov_degrees: f32,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            position: Vec3::new(14.0, 10.0, 14.0),
            yaw: -2.36,
            pitch: -0.5,
            fov_degrees: 70.0,
        }
    }

    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        )
        .normalize()
    }

    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y).normalize()
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = glam::camera::rh::view::look_to_mat4(self.position, self.forward(), Vec3::Y);
        // "directx" means depth runs 0 to 1, which is what wgpu uses.
        let proj = glam::camera::rh::proj::directx::perspective(
            self.fov_degrees.to_radians(),
            aspect.max(0.0001),
            0.1,
            2000.0,
        );
        proj * view
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}

// --- gpu data --------------------------------------------------------------

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    /// Where on a texture this vertex sits (only decals use it).
    uv: [f32; 2],
}

impl Vertex {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceRaw {
    model: [[f32; 4]; 4],
    color: [f32; 3],
    /// 1.0 when the part is selected, 0.0 otherwise.
    highlight: f32,
    /// Which part of the texture atlas to use: offset (xy) and size (zw).
    /// Untextured things point at the atlas's white cell.
    uv_rect: [f32; 4],
    /// x: 1 = tile a material texture over the part; y: 1 = neon (glows);
    /// z: transparency (dithered); w: the material's average shade.
    extra: [f32; 4],
    /// Breaks ties between surfaces in exactly the same place (two parts
    /// overlapping flush): each part sits a hair nearer or farther by this
    /// step, so one of them always wins, the same one every frame, instead
    /// of the two flickering ("z-fighting"). 0 for avatars.
    layer: f32,
}

/// The atlas's plain white cell, for everything without a picture.
const WHITE: [f32; 4] = [0.5 / avatar::COLS as f32, 0.5 / avatar::ROWS as f32, 0.0, 0.0];

fn atlas_rect(slot: u32) -> [f32; 4] {
    let (w, h) = (1.0 / avatar::COLS as f32, 1.0 / avatar::ROWS as f32);
    [(slot % avatar::COLS) as f32 * w, (slot / avatar::COLS) as f32 * h, w, h]
}

impl InstanceRaw {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceRaw>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 64,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 76,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: 80,
                    shader_location: 9,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 96,
                    shader_location: 10,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 112,
                    shader_location: 11,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    /// The sun's view, for shadows.
    light_view_proj: [[f32; 4]; 4],
    /// Direction toward the sun (w unused).
    sun_dir: [f32; 4],
    /// Screen back to world, for working out which way each sky pixel looks.
    inv_view_proj: [[f32; 4]; 4],
    /// The camera's position (xyz) and seconds since start (w), for the
    /// distance haze and drifting clouds.
    eye: [f32; 4],
    /// The lighting (see `Sky`).
    zenith: [f32; 4],
    horizon: [f32; 4],
    light: [f32; 4],
    ambient: [f32; 4],
    disc: [f32; 4],
    fog: [f32; 4],
    fog_color: [f32; 4],
}

/// What a game's Lighting works out to, for the shader: sky colours, the
/// light (sun by day, moon by night), the disc in the sky, stars and fog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sky {
    /// Toward the light that shines on things (and casts shadows).
    pub light_dir: Vec3,
    /// Overhead and horizon colours of the sky.
    pub zenith: Vec3,
    pub horizon: Vec3,
    /// The light's colour times its strength, and the flat ambient light.
    pub light: Vec3,
    pub ambient: Vec3,
    /// The disc in the sky: its direction, and 1 for the sun, 2 the moon, 0 none.
    pub disc_dir: Vec3,
    pub disc: f32,
    /// How many stars show (0 by day, 1 at night).
    pub stars: f32,
    /// Clouds' brightness and tint.
    pub cloud: Vec3,
    /// Fog: start, end (0 for none), and colour.
    pub fog_start: f32,
    pub fog_end: f32,
    pub fog_color: Vec3,
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn color_vec(c: brixo_core::Color) -> Vec3 {
    Vec3::new(c.r as f32, c.g as f32, c.b as f32) / 255.0
}

impl Sky {
    /// The sky and light for some Lighting. The default (2 in the
    /// afternoon) is Brixo's classic look, exactly as it always was.
    pub fn of(l: &brixo_core::Lighting) -> Sky {
        // The sun rises in the east at 6, is highest at 12, sets at 18.
        let a = (l.time_of_day - 6.0) / 12.0 * std::f32::consts::PI;
        let sun = Vec3::new(-a.cos() * 0.9, a.sin() * 1.1547, 0.3).normalize();
        let moon = Vec3::new(-sun.x, -sun.y, sun.z).normalize();
        let h = sun.y;
        let day = smoothstep(-0.12, 0.3, h);
        let dusk = 1.0 - smoothstep(0.0, 0.42, h.abs());

        let day_zenith = l.sky_color.map(color_vec).unwrap_or(Vec3::new(0.08, 0.33, 0.78));
        let day_horizon = l.sky_color.map(|c| color_vec(c).lerp(Vec3::ONE, 0.6)).unwrap_or(Vec3::new(0.66, 0.84, 0.98));
        let night_zenith = Vec3::new(0.01, 0.02, 0.07);
        let night_horizon = Vec3::new(0.06, 0.09, 0.19);
        let zenith = night_zenith.lerp(day_zenith, day).lerp(Vec3::new(0.24, 0.27, 0.55), dusk * 0.55);
        let horizon = night_horizon.lerp(day_horizon, day).lerp(Vec3::new(1.0, 0.56, 0.32), dusk * 0.85);

        // By day the sun lights things; at night a dim, blue moon. Light
        // never comes from right down at the horizon (flat, shadowless).
        let from = if h >= -0.02 { sun } else { moon };
        let light_dir = Vec3::new(from.x, from.y.max(0.25), from.z).normalize();
        let warm = Vec3::ONE.lerp(Vec3::new(1.0, 0.72, 0.5), dusk);
        let moonlight = Vec3::new(0.5, 0.6, 1.0);
        // (The screen is sRGB, so small numbers show brighter than they look.)
        let light = (moonlight * 0.1).lerp(warm * 0.7, day) * l.brightness;
        let ambient = (moonlight * 0.045).lerp(Vec3::splat(0.45), day) * l.brightness;

        let (disc_dir, disc) = if h > -0.06 { (sun, 1.0) } else { (moon, 2.0) };
        let cloud = (Vec3::new(0.06, 0.07, 0.12)).lerp(Vec3::ONE, day).lerp(Vec3::new(1.0, 0.7, 0.55), dusk * 0.7);
        Sky {
            light_dir,
            zenith,
            horizon,
            light,
            ambient,
            disc_dir,
            disc,
            stars: 1.0 - smoothstep(-0.3, -0.02, h),
            cloud,
            fog_start: l.fog_start.min(l.fog_end),
            fog_end: l.fog_end,
            fog_color: color_vec(l.fog_color),
        }
    }
}

/// Shadows cover this far around the camera, in studs.
const SHADOW_RANGE: f32 = 70.0;
const SHADOW_MAP_SIZE: u32 = 2048;

/// Unit cube, four vertices per face, counter-clockwise seen from outside.
fn cube() -> (Vec<Vertex>, Vec<u16>) {
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        (
            [1.0, 0.0, 0.0],
            [
                [0.5, -0.5, 0.5],
                [0.5, -0.5, -0.5],
                [0.5, 0.5, -0.5],
                [0.5, 0.5, 0.5],
            ],
        ),
        (
            [-1.0, 0.0, 0.0],
            [
                [-0.5, -0.5, -0.5],
                [-0.5, -0.5, 0.5],
                [-0.5, 0.5, 0.5],
                [-0.5, 0.5, -0.5],
            ],
        ),
        (
            [0.0, 1.0, 0.0],
            [
                [-0.5, 0.5, 0.5],
                [0.5, 0.5, 0.5],
                [0.5, 0.5, -0.5],
                [-0.5, 0.5, -0.5],
            ],
        ),
        (
            [0.0, -1.0, 0.0],
            [
                [-0.5, -0.5, -0.5],
                [0.5, -0.5, -0.5],
                [0.5, -0.5, 0.5],
                [-0.5, -0.5, 0.5],
            ],
        ),
        (
            [0.0, 0.0, 1.0],
            [
                [-0.5, -0.5, 0.5],
                [0.5, -0.5, 0.5],
                [0.5, 0.5, 0.5],
                [-0.5, 0.5, 0.5],
            ],
        ),
        (
            [0.0, 0.0, -1.0],
            [
                [0.5, -0.5, -0.5],
                [-0.5, -0.5, -0.5],
                [-0.5, 0.5, -0.5],
                [0.5, 0.5, -0.5],
            ],
        ),
    ];

    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (normal, corners) in faces {
        let base = vertices.len() as u16;
        for (c, uv) in corners.into_iter().zip([[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]) {
            vertices.push(Vertex {
                position: c,
                normal,
                uv,
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (vertices, indices)
}

fn to_glam(v: BVec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

/// Instances for every part, plus the players (drawn separately, as
/// avatars) with their highlight values. `editing`: invisible parts stay
/// faintly visible so they can be picked and edited.
/// Parts (with their shape) and players, with highlight values. Anything
/// in `selected` is highlighted.
#[allow(clippy::type_complexity)]
fn build_instances(
    model: &DataModel,
    selected: &[brixo_core::InstanceId],
    editing: bool,
) -> (Vec<(brixo_core::Shape, InstanceRaw)>, Vec<(brixo_core::InstanceId, brixo_core::PlayerProps, f32)>) {
    let mut out = Vec::new();
    let mut players = Vec::new();
    let mut stack = vec![model.root()];
    while let Some(id) = stack.pop() {
        let Some(inst) = model.get(id) else { continue };
        stack.extend(inst.children.iter().copied());

        let highlight = if selected.contains(&id) { 1.0 } else { 0.0 };
        if let Some(player) = model.player(id) {
            players.push((id, *player, highlight));
            continue;
        }
        // Parts and SpawnLocations.
        let Some(p) = model.part(id) else { continue };
        let transparency = if editing { p.transparency.min(0.75) } else { p.transparency };
        if transparency >= 0.99 {
            continue; // invisible: not drawn, no shadow
        }

        let rotation = Quat::from_euler(
            glam::EulerRot::YXZ,
            p.rotation.y.to_radians(),
            p.rotation.x.to_radians(),
            p.rotation.z.to_radians(),
        );
        // Round shapes use their smallest side, exactly like their colliders.
        let size = match p.shape {
            brixo_core::Shape::Ball => Vec3::splat(p.size.x.min(p.size.y).min(p.size.z)),
            brixo_core::Shape::Cylinder => {
                let d = p.size.x.min(p.size.z);
                Vec3::new(d, p.size.y, d)
            }
            _ => to_glam(p.size),
        };
        let m = Mat4::from_scale_rotation_translation(size, rotation, to_glam(p.position));

        out.push((
            p.shape,
            InstanceRaw {
                model: m.to_cols_array_2d(),
                color: rgb(p.color.r, p.color.g, p.color.b),
                highlight,
                uv_rect: avatar::material_slot(p.material).map(atlas_rect).unwrap_or(WHITE),
                extra: [
                    if avatar::material_slot(p.material).is_some() { 1.0 } else { 0.0 },
                    if p.material == brixo_core::Material::Neon { 1.0 } else { 0.0 },
                    transparency.max(0.0),
                    avatar::material_slot(p.material).map(|_| avatar::material_average(p.material)).unwrap_or(1.0),
                ],
                layer: depth_layer(id),
            },
        ));
    }
    (out, players)
}

/// Parts made one after another (a duplicate, the next brick in a wall)
/// get different layers, so the ones most likely to overlap flush don't tie.
fn depth_layer(id: brixo_core::InstanceId) -> f32 {
    (id.raw() % 63 + 1) as f32
}

/// Depth runs backwards on screen: 1 at the near plane, 0 far away. With a
/// floating-point depth buffer that keeps depth precise all the way out,
/// instead of spending it all on the first few studs, so surfaces a hair
/// apart stay apart at any distance.
fn reversed_depth() -> Mat4 {
    Mat4::from_cols(
        glam::Vec4::new(1.0, 0.0, 0.0, 0.0),
        glam::Vec4::new(0.0, 1.0, 0.0, 0.0),
        glam::Vec4::new(0.0, 0.0, -1.0, 0.0),
        glam::Vec4::new(0.0, 0.0, 1.0, 1.0),
    )
}

/// A colour as picked (sRGB, 0-255) in the linear form the GPU blends in.
/// The screen converts back, so colours show up exactly as picked.
fn rgb(r: u8, g: u8, b: u8) -> [f32; 3] {
    let lin = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    [lin(r), lin(g), lin(b)]
}

/// How each limb is turned right now (radians about the side axis:
/// negative swings forward/up): [left arm, right arm, left leg, right leg].
/// Worked out from what the player is doing, so every client animates
/// every player the same way without sending animation data.
fn pose(p: &brixo_core::PlayerProps, time: f32, seed: f32, grip_up: bool) -> [f32; 4] {
    let walk = (p.speed / 16.0).clamp(0.0, 1.0);
    let a = (time * 10.0 + seed).sin() * 0.75 * walk;
    let mut pose = [-a * 0.9, a * 0.9, a, -a];
    if p.airborne {
        // The classic jump: arms straight up, legs apart.
        pose = [-2.8, -2.8, -0.3, 0.3];
    }
    if p.kart.is_some() {
        // Sitting in a kart: legs out in front, hands on the wheel.
        pose = [-1.15, -1.15, -1.5, -1.5];
    }
    if p.equipped.is_some() {
        // The same arm the runtime puts the tool in (see held_arm_angle).
        pose[1] = brixo_core::held_arm_angle(p.swing, grip_up);
    }
    pose
}

/// One avatar mesh for one player: they all share the player's transform
/// (plus a limb's swing) and take their colour from the mesh's slot.
/// Falling apart: each piece is thrown off, tumbles through the air and
/// stops where it lands. Worked out from the seconds since death, so every
/// client draws the same fall. The transform is in the character's space.
fn scatter(limb: avatar::Limb, t: f32, seed: f32) -> Mat4 {
    use avatar::Limb::*;
    // Each piece's middle, how it's thrown, and half its size lying down.
    let (center, throw, rest) = match limb {
        Head => (Vec3::new(0.0, 1.8, 0.0), Vec3::new(0.6, 11.0, 2.0), 0.86),
        Body => (Vec3::new(0.0, 0.125, 0.0), Vec3::new(0.0, 2.5, -2.5), 0.55),
        ArmLeft => (Vec3::new(1.43, 0.0, 0.0), Vec3::new(7.0, 7.0, 1.0), 0.45),
        ArmRight => (Vec3::new(-1.43, 0.0, 0.0), Vec3::new(-7.0, 7.0, -1.0), 0.45),
        LegLeft => (Vec3::new(0.49, -1.6, 0.0), Vec3::new(3.0, 4.5, 2.5), 0.47),
        LegRight => (Vec3::new(-0.49, -1.6, 0.0), Vec3::new(-3.0, 4.5, -2.5), 0.47),
    };
    // A little different for every player, the same on every screen.
    let wobble = |k: f32| ((seed * 12.9898 + k * 78.233).sin() * 43758.5453).fract() - 0.5;
    let v = throw + Vec3::new(wobble(1.0), wobble(2.0).abs(), wobble(3.0)) * 3.0;
    const GRAVITY: f32 = 60.0;
    // Land when the piece's middle reaches the floor (feet level) plus half
    // its size: solve center.y + v.y*t - g*t^2/2 = floor for t.
    let floor = -2.5 + rest;
    let a = GRAVITY / 2.0;
    let land = (v.y + (v.y * v.y + 4.0 * a * (center.y - floor)).max(0.0).sqrt()) / (2.0 * a);
    let tt = t.min(land);
    let offset = Vec3::new(v.x * tt, v.y * tt - a * tt * tt, v.z * tt);
    let axis = Vec3::new(v.z, 0.3, -v.x).normalize_or(Vec3::X);
    let angle = tt * (6.0 + wobble(4.0) * 4.0);
    Mat4::from_translation(center + offset) * Mat4::from_axis_angle(axis, angle) * Mat4::from_translation(-center)
}

fn avatar_instance(
    p: &brixo_core::PlayerProps,
    slot: avatar::Slot,
    tex: avatar::Tex,
    limb: avatar::Limb,
    pose: [f32; 4],
    seed: f32,
    highlight: f32,
) -> InstanceRaw {
    let c = |c: brixo_core::Color| rgb(c.r, c.g, c.b);
    let color = match slot {
        avatar::Slot::Body(part) => c(p.body_colors()[part.index()]),
        avatar::Slot::Shirt => c(p.shirt_color),
        avatar::Slot::Pants => c(p.pants_color),
        avatar::Slot::Decal => rgb(255, 255, 255),
        avatar::Slot::Paint([r, g, b]) => rgb(r, g, b),
    };
    let uv_rect = avatar::tex_slot(tex, p).map(atlas_rect).unwrap_or(WHITE);
    let base = Mat4::from_translation(to_glam(p.body.position)) * Mat4::from_rotation_y(p.body.rotation.y.to_radians());
    let angle = match limb {
        avatar::Limb::Body | avatar::Limb::Head => 0.0,
        avatar::Limb::ArmLeft => pose[0],
        avatar::Limb::ArmRight => pose[1],
        avatar::Limb::LegLeft => pose[2],
        avatar::Limb::LegRight => pose[3],
    };
    let pivot = limb.pivot();
    let m = if p.dead > 0.0 {
        base * scatter(limb, p.dead, seed)
    } else {
        base * Mat4::from_translation(pivot) * Mat4::from_rotation_x(angle) * Mat4::from_translation(-pivot)
    };
    InstanceRaw {
        model: m.to_cols_array_2d(),
        color,
        highlight,
        uv_rect,
        extra: [0.0; 4],
        layer: 0.0,
    }
}

struct AvatarDraw {
    slot: avatar::Slot,
    limb: avatar::Limb,
    show: avatar::Show,
    tex: avatar::Tex,
    vertices: std::ops::Range<u32>,
}

// --- renderer --------------------------------------------------------------

pub struct SceneRenderer {
    pipeline: wgpu::RenderPipeline,
    blend_pipeline: wgpu::RenderPipeline,
    sky_pipeline: wgpu::RenderPipeline,
    /// Every part shape's mesh, one after another, and where each one is.
    shape_buffer: wgpu::Buffer,
    shape_ranges: Vec<std::ops::Range<u32>>,
    /// All the avatar meshes, one after another.
    avatar_buffer: wgpu::Buffer,
    avatar_draws: Vec<AvatarDraw>,
    /// A player not to draw: your own character, in first person.
    pub hidden_player: Option<brixo_core::InstanceId>,
    /// The studio's editor: invisible parts are drawn faintly.
    pub editing: bool,
    /// Seconds since the app started, for animations.
    pub time: f32,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    /// Draws the scene from the sun into the shadow map.
    shadow_pipeline: wgpu::RenderPipeline,
    shadow_bind_group: wgpu::BindGroup,
    shadow_view: wgpu::TextureView,
    atlas: wgpu::Texture,
    /// The atlas is filled on the first render (that's when we have a queue).
    atlas_uploaded: std::cell::Cell<bool>,
    depth_view: wgpu::TextureView,
}

impl SceneRenderer {
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let mut shape_vertices: Vec<Vertex> = Vec::new();
        let mut shape_ranges = Vec::new();
        for mesh in avatar::shape_meshes() {
            let start = shape_vertices.len() as u32;
            shape_vertices.extend(mesh);
            shape_ranges.push(start..shape_vertices.len() as u32);
        }
        let shape_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("part shapes"),
            contents: bytemuck::cast_slice(&shape_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut avatar_vertices: Vec<Vertex> = Vec::new();
        let mut avatar_draws = Vec::new();
        for mesh in avatar::meshes() {
            let start = avatar_vertices.len() as u32;
            avatar_vertices.extend(mesh.vertices);
            avatar_draws.push(AvatarDraw {
                slot: mesh.slot,
                limb: mesh.limb,
                show: mesh.show,
                tex: mesh.tex,
                vertices: start..avatar_vertices.len() as u32,
            });
        }
        let avatar_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("avatar vertices"),
            contents: bytemuck::cast_slice(&avatar_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera uniform"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_entry = |visibility| wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };

        // Face pictures: one small atlas, sampled without smoothing so the
        // pixels stay crisp.
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("face atlas"),
            size: wgpu::Extent3d {
                width: avatar::CELL * avatar::COLS,
                height: avatar::CELL * avatar::ROWS,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let atlas_view = atlas.create_view(&Default::default());
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // The shadow map: depth as seen from the sun.
        let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow map"),
            size: wgpu::Extent3d {
                width: SHADOW_MAP_SIZE,
                height: SHADOW_MAP_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow_texture.create_view(&Default::default());
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow sampler"),
            compare: Some(wgpu::CompareFunction::LessEqual),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let main_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene layout"),
            entries: &[
                uniform_entry(wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene bind group"),
            layout: &main_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: camera_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&shadow_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&shadow_sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&atlas_sampler) },
            ],
        });
        // The shadow pass only needs the matrices (it can't read the map
        // it's drawing into).
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow layout"),
            entries: &[uniform_entry(wgpu::ShaderStages::VERTEX)],
        });
        let shadow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow bind group"),
            layout: &shadow_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: camera_buffer.as_entire_binding() }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        // The scene's depth runs backwards (see reversed_depth): nearer is
        // bigger. The shadow map's runs the usual way.
        let depth_state = |bias| wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Greater,
            stencil: wgpu::StencilState::default(),
            bias,
        };
        let shadow_depth_state = |bias| wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias,
        };
        let primitive = |cull_mode| wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        };
        let scene_pipeline = |label: &str, blend: wgpu::BlendState, depth_write: bool| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("scene pipeline layout"),
                bind_group_layouts: &[&main_layout],
                push_constant_ranges: &[],
            })),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout(), InstanceRaw::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: primitive(Some(wgpu::Face::Back)),
            depth_stencil: Some(wgpu::DepthStencilState {
                depth_write_enabled: depth_write,
                ..depth_state(wgpu::DepthBiasState::default())
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let pipeline = scene_pipeline("scene pipeline", wgpu::BlendState::REPLACE, true);
        // See-through parts: blended over what's already drawn, and not
        // hiding what's behind them from each other.
        let blend_pipeline = scene_pipeline("see-through pipeline", wgpu::BlendState::ALPHA_BLENDING, false);
        // The sky: one triangle covering the screen, drawn first, behind
        // everything (no depth test or write).
        let mut sky_depth = depth_state(wgpu::DepthBiasState::default());
        sky_depth.depth_write_enabled = false;
        sky_depth.depth_compare = wgpu::CompareFunction::Always;
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky pipeline"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("sky pipeline layout"),
                bind_group_layouts: &[&main_layout],
                push_constant_ranges: &[],
            })),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_sky"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_sky"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: primitive(None),
            depth_stencil: Some(sky_depth),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow pipeline"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("shadow pipeline layout"),
                bind_group_layouts: &[&shadow_layout],
                push_constant_ranges: &[],
            })),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_shadow"),
                buffers: &[Vertex::layout(), InstanceRaw::layout()],
                compilation_options: Default::default(),
            },
            fragment: None,
            primitive: primitive(Some(wgpu::Face::Back)),
            // Pushes stored depths back a touch so lit surfaces don't
            // shadow themselves ("shadow acne").
            depth_stencil: Some(shadow_depth_state(wgpu::DepthBiasState {
                constant: 2,
                slope_scale: 2.0,
                clamp: 0.0,
            })),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let depth_view = create_depth_view(device, width, height);

        Self {
            pipeline,
            blend_pipeline,
            sky_pipeline,
            shape_buffer,
            shape_ranges,
            avatar_buffer,
            avatar_draws,
            hidden_player: None,
            editing: false,
            time: 0.0,
            camera_buffer,
            camera_bind_group,
            shadow_pipeline,
            shadow_bind_group,
            shadow_view,
            atlas,
            atlas_uploaded: std::cell::Cell::new(false),
            depth_view,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.depth_view = create_depth_view(device, width, height);
    }

    /// Clears the target and draws every Part in the model.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        color_view: &wgpu::TextureView,
        model: &DataModel,
        camera: &Camera,
        selected: &[brixo_core::InstanceId],
        width: u32,
        height: u32,
    ) {
        let aspect = width as f32 / height.max(1) as f32;
        // The sun's camera: looking down along the sun direction at the
        // area around where the camera is looking.
        let sky = Sky::of(&brixo_core::Lighting::of(model));
        let sun = sky.light_dir;
        let focus = camera.position + camera.forward() * (SHADOW_RANGE * 0.4);
        let light_view = glam::camera::rh::view::look_at_mat4(focus + sun * 200.0, focus, Vec3::Y);
        let light_proj = glam::camera::rh::proj::directx::orthographic(
            -SHADOW_RANGE, SHADOW_RANGE, -SHADOW_RANGE, SHADOW_RANGE, 1.0, 400.0,
        );
        let uniform = CameraUniform {
            view_proj: (reversed_depth() * camera.view_proj(aspect)).to_cols_array_2d(),
            light_view_proj: (light_proj * light_view).to_cols_array_2d(),
            sun_dir: [sun.x, sun.y, sun.z, 0.0],
            inv_view_proj: camera.view_proj(aspect).inverse().to_cols_array_2d(),
            eye: [camera.position.x, camera.position.y, camera.position.z, self.time],
            zenith: [sky.zenith.x, sky.zenith.y, sky.zenith.z, sky.stars],
            horizon: [sky.horizon.x, sky.horizon.y, sky.horizon.z, 0.0],
            light: [sky.light.x, sky.light.y, sky.light.z, 0.0],
            ambient: [sky.ambient.x, sky.ambient.y, sky.ambient.z, 0.0],
            disc: [sky.disc_dir.x, sky.disc_dir.y, sky.disc_dir.z, sky.disc],
            fog: [sky.fog_start, sky.fog_end, sky.cloud.x, sky.cloud.y],
            fog_color: [sky.fog_color.x, sky.fog_color.y, sky.fog_color.z, sky.cloud.z],
        };
        if !self.atlas_uploaded.get() {
            let size = self.atlas.size();
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.atlas,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &avatar::face_atlas(),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size.width * 4),
                    rows_per_image: Some(size.height),
                },
                size,
            );
            self.atlas_uploaded.set(true);
        }
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));

        let (parts, players) = build_instances(model, selected, self.editing);
        // Solid parts first, grouped by shape so each shape is one draw call.
        // (See-through ones are drawn last, one by one, farthest first.)
        let see_through = |i: &InstanceRaw| i.extra[2] > 0.001;
        let mut instances = Vec::with_capacity(parts.len());
        let mut part_batches = Vec::new();
        for (k, shape) in brixo_core::Shape::ALL.iter().enumerate() {
            let start = instances.len() as u32;
            instances.extend(parts.iter().filter(|(s, i)| s == shape && !see_through(i)).map(|(_, i)| *i));
            let end = instances.len() as u32;
            if end > start {
                part_batches.push((k, start..end));
            }
        }
        // Avatar instances go after the parts, grouped by mesh.
        let mut avatar_batches = Vec::new();
        for (i, draw) in self.avatar_draws.iter().enumerate() {
            let start = instances.len() as u32;
            for (id, p, highlight) in &players {
                if self.hidden_player == Some(*id) {
                    continue;
                }
                // Someone right up against the camera (a kart alongside
                // yours) would fill the screen: leave them out.
                let chest = Vec3::new(p.body.position.x, p.body.position.y + 1.5, p.body.position.z);
                if chest.distance(camera.position) < 5.0 {
                    continue;
                }
                if !avatar::shows(draw.show, p) {
                    continue;
                }
                let grip_up = p.equipped.is_some_and(|t| brixo_core::holds_up(model, t));
                let pose = pose(p, self.time, id.raw() as f32 * 1.7, grip_up);
                instances.push(avatar_instance(p, draw.slot, draw.tex, draw.limb, pose, id.raw() as f32, *highlight));
            }
            let end = instances.len() as u32;
            if end > start {
                avatar_batches.push((i, start..end));
            }
        }
        let eye = camera.position;
        let mut glassy: Vec<(usize, InstanceRaw, f32)> = parts
            .iter()
            .filter(|(_, i)| see_through(i))
            .map(|(shape, i)| {
                let k = brixo_core::Shape::ALL.iter().position(|s| s == shape).unwrap_or(0);
                let at = Vec3::new(i.model[3][0], i.model[3][1], i.model[3][2]);
                (k, *i, at.distance_squared(eye))
            })
            .collect();
        glassy.sort_by(|a, b| b.2.total_cmp(&a.2));
        let mut glassy_draws = Vec::with_capacity(glassy.len());
        for (k, i, _) in glassy {
            glassy_draws.push((k, instances.len() as u32));
            instances.push(i);
        }
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("part instances"),
            contents: bytemuck::cast_slice(&instances),
            usage: wgpu::BufferUsages::VERTEX,
        });

        {
            let mut shadow = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if !instances.is_empty() {
                shadow.set_pipeline(&self.shadow_pipeline);
                shadow.set_bind_group(0, &self.shadow_bind_group, &[]);
                shadow.set_vertex_buffer(0, self.shape_buffer.slice(..));
                shadow.set_vertex_buffer(1, instance_buffer.slice(..));
                for (k, batch) in &part_batches {
                    shadow.draw(self.shape_ranges[*k].clone(), batch.clone());
                }
                shadow.set_vertex_buffer(0, self.avatar_buffer.slice(..));
                for (i, batch) in &avatar_batches {
                    // A printed picture casts no shadow of its own.
                    if self.avatar_draws[*i].slot != avatar::Slot::Decal {
                        shadow.draw(self.avatar_draws[*i].vertices.clone(), batch.clone());
                    }
                }
            }
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    // Sky blue (#3f86c9, in linear form).
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.052,
                        g: 0.238,
                        b: 0.584,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.sky_pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.draw(0..3, 0..1);

        if !instances.is_empty() {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, self.shape_buffer.slice(..));
            pass.set_vertex_buffer(1, instance_buffer.slice(..));
            for (k, batch) in &part_batches {
                pass.draw(self.shape_ranges[*k].clone(), batch.clone());
            }
            pass.set_vertex_buffer(0, self.avatar_buffer.slice(..));
            for (i, batch) in avatar_batches {
                pass.draw(self.avatar_draws[i].vertices.clone(), batch);
            }
            if !glassy_draws.is_empty() {
                pass.set_pipeline(&self.blend_pipeline);
                pass.set_vertex_buffer(0, self.shape_buffer.slice(..));
                for (k, at) in glassy_draws {
                    pass.draw(self.shape_ranges[k].clone(), at..at + 1);
                }
            }
        }
    }
}

fn create_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth texture"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

const SHADER: &str = r#"
struct Camera {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    sun_dir: vec4<f32>,
    inv_view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    zenith: vec4<f32>,
    horizon: vec4<f32>,
    light: vec4<f32>,
    ambient: vec4<f32>,
    disc: vec4<f32>,
    fog: vec4<f32>,
    fog_color: vec4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var shadow_map: texture_depth_2d;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;
@group(0) @binding(3) var atlas: texture_2d<f32>;
@group(0) @binding(4) var atlas_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(8) uv: vec2<f32>,
};

struct InstanceInput {
    @location(2) m0: vec4<f32>,
    @location(3) m1: vec4<f32>,
    @location(4) m2: vec4<f32>,
    @location(5) m3: vec4<f32>,
    @location(6) color: vec3<f32>,
    @location(7) highlight: f32,
    @location(9) uv_rect: vec4<f32>,
    @location(10) extra: vec4<f32>,
    @location(11) layer: f32,
};

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) highlight: f32,
    @location(3) uv: vec2<f32>,
    @location(4) world: vec3<f32>,
    // The vertex in the part's own space, in studs, and its own normal:
    // material textures tile along these, so they stick to the part.
    @location(5) local: vec3<f32>,
    @location(6) local_normal: vec3<f32>,
    @location(7) uv_rect: vec4<f32>,
    @location(8) extra: vec4<f32>,
};

@vertex
fn vs_main(v: VertexInput, i: InstanceInput) -> VsOut {
    let model = mat4x4<f32>(i.m0, i.m1, i.m2, i.m3);
    let world = model * vec4<f32>(v.position, 1.0);
    var out: VsOut;
    out.clip_position = camera.view_proj * world;
    // Each part's tie-breaking nudge toward the camera (see InstanceRaw::layer).
    out.clip_position.z = out.clip_position.z * (1.0 + i.layer * 1.0e-6);
    out.world = world.xyz;
    out.color = i.color;
    out.normal = (model * vec4<f32>(v.normal, 0.0)).xyz;
    out.highlight = i.highlight;
    out.uv = i.uv_rect.xy + v.uv * i.uv_rect.zw;
    let scale = vec3<f32>(length(i.m0.xyz), length(i.m1.xyz), length(i.m2.xyz));
    out.local = v.position * scale;
    out.local_normal = v.normal;
    out.uv_rect = i.uv_rect;
    out.extra = i.extra;
    return out;
}

@vertex
fn vs_shadow(v: VertexInput, i: InstanceInput) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(i.m0, i.m1, i.m2, i.m3);
    return camera.light_view_proj * model * vec4<f32>(v.position, 1.0);
}

// --- the sky -------------------------------------------------------------
// Early-2000s daytime: saturated blue overhead, a pale horizon, a crisp sun
// and flat two-tone clouds with hard edges. Worked out per pixel from the
// view direction, so it's sharp at any resolution.

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

// Smooth value noise.
fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn cloud_density(p: vec2<f32>) -> f32 {
    return vnoise(p) * 0.6 + vnoise(p * 2.3 + vec2<f32>(17.0, 5.0)) * 0.3 + vnoise(p * 5.1) * 0.1;
}

fn sky_color(dir: vec3<f32>) -> vec3<f32> {
    let up = dir.y;
    let horizon = camera.horizon.xyz;
    var col = mix(horizon, camera.zenith.xyz, pow(clamp(up, 0.0, 1.0), 0.5));
    // Below the horizon: a soft haze, never black.
    col = select(col, mix(horizon, horizon * vec3<f32>(0.79, 0.81, 0.86), clamp(-up * 3.0, 0.0, 1.0)), up < 0.0);

    // Stars at night: tiny hard points, fixed in the sky.
    if (camera.zenith.w > 0.0 && up > 0.0) {
        let cell = floor(dir.xz / (up + 0.35) * 90.0 + dir.y * 7.0);
        let star = step(0.9965, hash2(cell));
        col = col + vec3<f32>(star * camera.zenith.w * smoothstep(0.0, 0.25, up) * 0.85);
    }

    // The sun (a hard-edged disc and a tight, bright halo), or the moon.
    let s = dot(dir, normalize(camera.disc.xyz));
    if (camera.disc.w > 1.5) {
        col = select(col, vec3<f32>(0.9, 0.92, 0.98), s > 0.9993);
    } else if (camera.disc.w > 0.5) {
        let tint = mix(vec3<f32>(1.0, 0.95, 0.75), vec3<f32>(1.0, 0.7, 0.4), clamp(1.0 - camera.disc.y * 3.0, 0.0, 1.0));
        col = col + tint * smoothstep(0.985, 0.9985, s) * 0.35;
        col = select(col, vec3<f32>(1.0, 0.98, 0.88), s > 0.9988);
    }

    // Clouds: on a flat layer above, drifting slowly. Crisp edges, a white
    // top and a cooler shaded underside, like a painted skybox.
    if (up > 0.015) {
        let p = dir.xz / up * 0.9 + vec2<f32>(camera.eye.w * 0.012, camera.eye.w * 0.004);
        let n = cloud_density(p);
        let cover = smoothstep(0.585, 0.605, n);
        let shade = smoothstep(0.585, 0.70, cloud_density(p - normalize(camera.disc.xz + vec2<f32>(0.001)) * 0.08));
        let tint = vec3<f32>(camera.fog.z, camera.fog.w, camera.fog_color.w);
        let cloud = mix(vec3<f32>(0.80, 0.86, 0.95), vec3<f32>(1.0, 1.0, 1.0), shade) * tint;
        col = mix(col, cloud, cover * smoothstep(0.015, 0.12, up));
    }

    // Thick fog hides the horizon too.
    if (camera.fog.y > 0.0) {
        let thick = 1.0 - smoothstep(150.0, 1500.0, camera.fog.y);
        col = mix(col, camera.fog_color.xyz, thick * (1.0 - smoothstep(-0.1, 0.45, up)));
    }
    return col;
}

struct SkyOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

// One triangle that covers the whole screen.
@vertex
fn vs_sky(@builtin(vertex_index) i: u32) -> SkyOut {
    let xy = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    var out: SkyOut;
    out.clip_position = vec4<f32>(xy, 0.0, 1.0);
    out.ndc = xy;
    return out;
}

@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    let far = camera.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let dir = normalize(far.xyz / far.w - camera.eye.xyz);
    return vec4<f32>(sky_color(dir), 1.0);
}

// A 4x4 ordered-dither pattern, for retro see-through parts.

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Materials tile one texture per 4 studs, on the face's own plane.
    let an = abs(in.local_normal);
    var plane = in.local.xy;
    if (an.y >= an.x && an.y >= an.z) {
        plane = in.local.xz;
    } else if (an.x >= an.z) {
        plane = in.local.zy;
    }
    let t = fract(plane / 4.0);
    let tiled = in.uv_rect.xy + vec2<f32>(t.x, 1.0 - t.y) * in.uv_rect.zw;
    let tiling = in.extra.x > 0.5;
    let tex = textureSample(atlas, atlas_sampler, select(in.uv, tiled, tiling));

    // Is this point in the sun, or in something's shadow?
    let lp = camera.light_view_proj * vec4<f32>(in.world, 1.0);
    let ndc = lp.xyz / lp.w;
    let suv = vec2<f32>(ndc.x * 0.5 + 0.5, -ndc.y * 0.5 + 0.5);
    let inside = all(suv >= vec2<f32>(0.0)) && all(suv <= vec2<f32>(1.0)) && ndc.z <= 1.0;
    let sampled = textureSampleCompareLevel(shadow_map, shadow_sampler, clamp(suv, vec2<f32>(0.0), vec2<f32>(1.0)), ndc.z - 0.0015);
    let lit = select(1.0, sampled, inside);

    // Transparent parts of a decal aren't there at all.
    if (tex.a < 0.5) {
        discard;
    }

    // Old-school lighting: flat ambient plus one hard sun. Neon ignores it.
    let n = normalize(in.normal);
    let lambert = max(dot(n, normalize(camera.sun_dir.xyz)), 0.0);
    let shade = select(camera.ambient.xyz + camera.light.xyz * lambert * lit, vec3<f32>(1.15), in.extra.y > 0.5);
    // Far away, a texture's pixels are smaller than the screen's and would
    // shimmer like static: fade the detail to the material's average shade.
    let texels_per_pixel = length(fwidth(plane)) * 16.0;
    // (The textures are built from soft shapes a few pixels wide, so they
    // hold up until their pixels get a few times smaller than the screen's.)
    let fade = smoothstep(1.2, 3.5, texels_per_pixel);
    let far_faded = mix(tex.rgb, vec3<f32>(in.extra.w), fade);
    let texel = select(tex.rgb, far_faded * 1.25, tiling);
    var rgb = texel * in.color * shade;
    // Distance haze: far things fade into the sky's colour that way, so
    // the world's edge melts into the horizon instead of ending abruptly.
    let to_eye = in.world - camera.eye.xyz;
    let haze = smoothstep(160.0, 520.0, length(to_eye)) * 0.6;
    rgb = mix(rgb, sky_color(normalize(to_eye)), haze);
    // Fog: things fade into it, and are gone by its end.
    if (camera.fog.y > 0.0) {
        rgb = mix(rgb, camera.fog_color.xyz, smoothstep(camera.fog.x, camera.fog.y, length(to_eye)));
    }
    // Selected parts get tinted toward orange.
    rgb = mix(rgb, vec3<f32>(1.0, 0.55, 0.1), in.highlight * 0.45);
    // See-through parts are blended over what's behind them (they're drawn
    // last, farthest first); solid ones ignore this.
    return vec4<f32>(rgb, 1.0 - in.extra.z);
}
"#;

// --- picking ---------------------------------------------------------------

/// Returns the nearest Part under the cursor.
/// `ndc_x`/`ndc_y` are normalised device coordinates: -1..1, y pointing up.
pub fn pick(
    model: &DataModel,
    camera: &Camera,
    aspect: f32,
    ndc_x: f32,
    ndc_y: f32,
) -> Option<brixo_core::InstanceId> {
    let (near, dir) = cursor_ray(camera, aspect, ndc_x, ndc_y);
    cast(model, near, dir, false, &|_| false).map(|(_, id)| id)
}

/// Where the mouse points in the world: the spot on the nearest part or
/// character under the cursor, or a point far along the ray when there's
/// nothing but sky. `skip` leaves things out (your own character and the
/// tool in your hand, so you don't aim at yourself).
pub fn pick_point(
    model: &DataModel,
    camera: &Camera,
    aspect: f32,
    ndc_x: f32,
    ndc_y: f32,
    skip: &dyn Fn(brixo_core::InstanceId) -> bool,
) -> brixo_core::Vec3 {
    let (near, dir) = cursor_ray(camera, aspect, ndc_x, ndc_y);
    let t = cast(model, near, dir, true, skip).map_or(400.0, |(t, _)| t.min(400.0));
    let at = near + dir * t;
    brixo_core::Vec3::new(at.x, at.y, at.z)
}

/// A world-space ray from the camera through the cursor: where it starts
/// (on the near plane) and which way it goes.
fn cursor_ray(camera: &Camera, aspect: f32, ndc_x: f32, ndc_y: f32) -> (Vec3, Vec3) {
    // Unproject two points to get a world-space ray through the cursor.
    let inv = camera.view_proj(aspect).inverse();
    let near = inv * glam::Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
    let far = inv * glam::Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
    let near = near.truncate() / near.w;
    let far = far.truncate() / far.w;
    (near, (far - near).normalize())
}

/// The nearest thing the ray hits: how far along it (in studs, since `dir`
/// is a unit vector) and what. Characters count too when `players` is set.
fn cast(
    model: &DataModel,
    near: Vec3,
    dir: Vec3,
    players: bool,
    skip: &dyn Fn(brixo_core::InstanceId) -> bool,
) -> Option<(f32, brixo_core::InstanceId)> {
    let mut best: Option<(f32, brixo_core::InstanceId)> = None;
    let mut consider = |id, m: Mat4| {
        let inv_m = m.inverse();
        // Test in the thing's own space, where every box is a unit cube.
        let local_origin = inv_m.transform_point3(near);
        let local_dir = inv_m.transform_vector3(dir);
        if let Some(t) = ray_unit_cube(local_origin, local_dir) {
            if t >= 0.0 && best.map_or(true, |(bt, _)| t < bt) {
                best = Some((t, id));
            }
        }
    };
    let mut stack = vec![model.root()];
    while let Some(id) = stack.pop() {
        let Some(inst) = model.get(id) else { continue };
        stack.extend(inst.children.iter().copied());
        if skip(id) {
            continue;
        }
        if let Some(p) = model.part(id) {
            // Parts and SpawnLocations.
            let rotation = Quat::from_euler(
                glam::EulerRot::YXZ,
                p.rotation.y.to_radians(),
                p.rotation.x.to_radians(),
                p.rotation.z.to_radians(),
            );
            consider(id, Mat4::from_scale_rotation_translation(to_glam(p.size), rotation, to_glam(p.position)));
        } else if let (true, Some(p)) = (players, model.player(id)) {
            // A character: roughly its box, arms included.
            if p.dead > 0.0 {
                continue;
            }
            let rotation = Quat::from_rotation_y(p.body.rotation.y.to_radians());
            consider(id, Mat4::from_scale_rotation_translation(Vec3::new(3.7, 5.3, 1.1), rotation, to_glam(p.body.position)));
        }
    }
    best
}

/// Slab test against the cube spanning -0.5..0.5 on each axis.
fn ray_unit_cube(origin: Vec3, dir: Vec3) -> Option<f32> {
    let o = origin.to_array();
    let d = dir.to_array();
    let mut tmin = f32::NEG_INFINITY;
    let mut tmax = f32::INFINITY;

    for i in 0..3 {
        if d[i].abs() < 1e-8 {
            // Ray runs parallel to this pair of faces.
            if o[i] < -0.5 || o[i] > 0.5 {
                return None;
            }
        } else {
            let mut t1 = (-0.5 - o[i]) / d[i];
            let mut t2 = (0.5 - o[i]) / d[i];
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        }
    }

    if tmax < 0.0 {
        return None; // box is behind the camera
    }
    Some(if tmin >= 0.0 { tmin } else { tmax })
}

#[cfg(test)]
mod shader_tests {
    use super::SHADER;

    /// The GPU only checks the shader when an app starts, so a mistake
    /// there passes every other test and crashes both apps on launch. This
    /// runs the same checker (naga) at test time.
    #[test]
    fn the_shader_compiles() {
        let module = naga::front::wgsl::parse_str(SHADER).unwrap_or_else(|e| panic!("{}", e.emit_to_string(SHADER)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
            .validate(&module)
            .unwrap_or_else(|e| panic!("the shader doesn't validate: {e:?}"));
    }
}

#[cfg(test)]
mod sky_tests {
    use super::Sky;
    use brixo_core::{Color, Lighting};
    use glam::Vec3;

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-4
    }

    #[test]
    fn the_default_is_the_classic_afternoon() {
        let s = Sky::of(&Lighting::default());
        assert!(near(s.light_dir, Vec3::new(0.45, 1.0, 0.3).normalize()), "{:?}", s.light_dir);
        assert!(near(s.zenith, Vec3::new(0.08, 0.33, 0.78)) && near(s.horizon, Vec3::new(0.66, 0.84, 0.98)));
        assert!(near(s.light, Vec3::splat(0.7)) && near(s.ambient, Vec3::splat(0.45)));
        assert!(near(s.cloud, Vec3::ONE));
        assert_eq!((s.disc, s.stars, s.fog_end), (1.0, 0.0, 0.0));
    }

    #[test]
    fn the_day_goes_round() {
        let at = |t: f32| Sky::of(&Lighting { time_of_day: t, ..Lighting::default() });
        let (noon, sunset, night) = (at(12.0), at(18.2), at(0.0));
        assert!(noon.light_dir.y > 0.9, "noon sun overhead");
        assert!(sunset.horizon.x > sunset.horizon.z, "sunset horizon is orange: {:?}", sunset.horizon);
        assert!(night.zenith.length() < 0.1 && night.stars > 0.99 && night.disc == 2.0, "night: dark, stars, moon");
        assert!(night.light.length() < noon.light.length() * 0.5 && night.light.z > night.light.x, "moonlight is dim and blue");
        let morning = at(8.0);
        assert!(morning.light_dir.x < 0.0 && at(16.0).light_dir.x > 0.0, "the sun crosses the sky");
    }

    #[test]
    fn brightness_sky_colour_and_fog() {
        let s = Sky::of(&Lighting { brightness: 2.0, sky_color: Some(Color::new(255, 0, 0)), fog_start: 20.0, fog_end: 80.0, ..Lighting::default() });
        assert!(near(s.light, Vec3::splat(1.4)));
        assert!(near(s.zenith, Vec3::new(1.0, 0.0, 0.0)));
        assert_eq!((s.fog_start, s.fog_end), (20.0, 80.0));
        // Written into the Workspace and read back.
        let mut dm = brixo_core::DataModel::new();
        let l = Lighting { time_of_day: 19.5, fog_end: 120.0, fog_color: Color::new(10, 20, 30), sky_color: Some(Color::new(1, 2, 3)), ..Lighting::default() };
        l.set(&mut dm);
        assert_eq!(Lighting::of(&dm), l);
        Lighting::default().set(&mut dm);
        assert!(dm.get(dm.root()).unwrap().attributes.is_empty(), "the default leaves no fields behind");
    }
}
