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
}

/// The atlas's plain white cell, for everything without a picture.
const WHITE: [f32; 4] = [0.5 / avatar::ATLAS_SLOTS as f32, 0.5, 0.0, 0.0];

fn atlas_rect(slot: u32) -> [f32; 4] {
    let w = 1.0 / avatar::ATLAS_SLOTS as f32;
    [slot as f32 * w, 0.0, w, 1.0]
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
}

/// Toward the sun: high, and a little off to one side.
const SUN_DIR: Vec3 = Vec3::new(0.45, 1.0, 0.3);
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
/// avatars) with their highlight values.
/// Parts (with their shape) and players, with highlight values. Anything
/// in `selected` is highlighted.
#[allow(clippy::type_complexity)]
fn build_instances(
    model: &DataModel,
    selected: &[brixo_core::InstanceId],
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
                uv_rect: WHITE,
            },
        ));
    }
    (out, players)
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

/// One avatar mesh for one player: they all share the player's transform
/// and take their colour from the mesh's slot.
fn avatar_instance(p: &brixo_core::PlayerProps, slot: avatar::Slot, highlight: f32) -> InstanceRaw {
    let c = |c: brixo_core::Color| rgb(c.r, c.g, c.b);
    let color = match slot {
        avatar::Slot::Skin => c(p.skin_color),
        avatar::Slot::Shirt => c(p.shirt_color),
        avatar::Slot::Pants => c(p.pants_color),
        avatar::Slot::Shoes => c(p.shoes_color),
        avatar::Slot::Decal => rgb(255, 255, 255),
    };
    let uv_rect = match slot {
        avatar::Slot::Decal => atlas_rect(avatar::face_slot(p.face)),
        _ => WHITE,
    };
    let m = Mat4::from_translation(to_glam(p.body.position)) * Mat4::from_rotation_y(p.body.rotation.y.to_radians());
    InstanceRaw {
        model: m.to_cols_array_2d(),
        color,
        highlight,
        uv_rect,
    }
}

struct AvatarDraw {
    slot: avatar::Slot,
    vertices: std::ops::Range<u32>,
}

// --- renderer --------------------------------------------------------------

pub struct SceneRenderer {
    pipeline: wgpu::RenderPipeline,
    /// Every part shape's mesh, one after another, and where each one is.
    shape_buffer: wgpu::Buffer,
    shape_ranges: Vec<std::ops::Range<u32>>,
    /// All the avatar meshes, one after another.
    avatar_buffer: wgpu::Buffer,
    avatar_draws: Vec<AvatarDraw>,
    /// A player not to draw: your own character, in first person.
    pub hidden_player: Option<brixo_core::InstanceId>,
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
                width: avatar::CELL * avatar::ATLAS_SLOTS,
                height: avatar::CELL,
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
        let depth_state = |bias| wgpu::DepthStencilState {
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
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene pipeline"),
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
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: primitive(Some(wgpu::Face::Back)),
            depth_stencil: Some(depth_state(wgpu::DepthBiasState::default())),
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
            depth_stencil: Some(depth_state(wgpu::DepthBiasState {
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
            shape_buffer,
            shape_ranges,
            avatar_buffer,
            avatar_draws,
            hidden_player: None,
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
        let sun = SUN_DIR.normalize();
        let focus = camera.position + camera.forward() * (SHADOW_RANGE * 0.4);
        let light_view = glam::camera::rh::view::look_at_mat4(focus + sun * 200.0, focus, Vec3::Y);
        let light_proj = glam::camera::rh::proj::directx::orthographic(
            -SHADOW_RANGE, SHADOW_RANGE, -SHADOW_RANGE, SHADOW_RANGE, 1.0, 400.0,
        );
        let uniform = CameraUniform {
            view_proj: camera.view_proj(aspect).to_cols_array_2d(),
            light_view_proj: (light_proj * light_view).to_cols_array_2d(),
            sun_dir: [sun.x, sun.y, sun.z, 0.0],
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

        let (parts, players) = build_instances(model, selected);
        // Parts first, grouped by shape so each shape is one draw call.
        let mut instances = Vec::with_capacity(parts.len());
        let mut part_batches = Vec::new();
        for (k, shape) in brixo_core::Shape::ALL.iter().enumerate() {
            let start = instances.len() as u32;
            instances.extend(parts.iter().filter(|(s, _)| s == shape).map(|(_, i)| *i));
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
                instances.push(avatar_instance(p, draw.slot, *highlight));
            }
            let end = instances.len() as u32;
            if end > start {
                avatar_batches.push((i, start..end));
            }
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
                    // A printed face casts no shadow of its own.
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
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

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
};

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) highlight: f32,
    @location(3) uv: vec2<f32>,
    @location(4) world: vec3<f32>,
};

@vertex
fn vs_main(v: VertexInput, i: InstanceInput) -> VsOut {
    let model = mat4x4<f32>(i.m0, i.m1, i.m2, i.m3);
    let world = model * vec4<f32>(v.position, 1.0);
    var out: VsOut;
    out.clip_position = camera.view_proj * world;
    out.world = world.xyz;
    out.color = i.color;
    out.normal = (model * vec4<f32>(v.normal, 0.0)).xyz;
    out.highlight = i.highlight;
    out.uv = i.uv_rect.xy + v.uv * i.uv_rect.zw;
    return out;
}

@vertex
fn vs_shadow(v: VertexInput, i: InstanceInput) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(i.m0, i.m1, i.m2, i.m3);
    return camera.light_view_proj * model * vec4<f32>(v.position, 1.0);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let tex = textureSample(atlas, atlas_sampler, in.uv);

    // Is this point in the sun, or in something's shadow?
    let lp = camera.light_view_proj * vec4<f32>(in.world, 1.0);
    let ndc = lp.xyz / lp.w;
    let suv = vec2<f32>(ndc.x * 0.5 + 0.5, -ndc.y * 0.5 + 0.5);
    let inside = all(suv >= vec2<f32>(0.0)) && all(suv <= vec2<f32>(1.0)) && ndc.z <= 1.0;
    let sampled = textureSampleCompareLevel(shadow_map, shadow_sampler, clamp(suv, vec2<f32>(0.0), vec2<f32>(1.0)), ndc.z - 0.0015);
    let lit = select(1.0, sampled, inside);

    // Transparent parts of a decal aren't there at all.
    if tex.a < 0.5 {
        discard;
    }

    // Old-school lighting: flat ambient plus one hard sun.
    let n = normalize(in.normal);
    let lambert = max(dot(n, normalize(camera.sun_dir.xyz)), 0.0);
    let shade = 0.45 + 0.7 * lambert * lit;
    var rgb = tex.rgb * in.color * shade;
    // Selected parts get tinted toward orange.
    rgb = mix(rgb, vec3<f32>(1.0, 0.55, 0.1), in.highlight * 0.45);
    return vec4<f32>(rgb, 1.0);
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
    // Unproject two points to get a world-space ray through the cursor.
    let inv = camera.view_proj(aspect).inverse();
    let near = inv * glam::Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
    let far = inv * glam::Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
    let near = near.truncate() / near.w;
    let far = far.truncate() / far.w;
    let dir = (far - near).normalize();

    let mut best: Option<(f32, brixo_core::InstanceId)> = None;
    let mut stack = vec![model.root()];
    while let Some(id) = stack.pop() {
        let Some(inst) = model.get(id) else { continue };
        stack.extend(inst.children.iter().copied());

        // Parts and SpawnLocations can be picked.
        let Some(p) = model.part(id) else { continue };

        let rotation = Quat::from_euler(
            glam::EulerRot::YXZ,
            p.rotation.y.to_radians(),
            p.rotation.x.to_radians(),
            p.rotation.z.to_radians(),
        );
        let m =
            Mat4::from_scale_rotation_translation(to_glam(p.size), rotation, to_glam(p.position));
        let inv_m = m.inverse();

        // Test in the part's own space, where every box is a unit cube.
        let local_origin = inv_m.transform_point3(near);
        let local_dir = inv_m.transform_vector3(dir);

        if let Some(t) = ray_unit_cube(local_origin, local_dir) {
            if best.map_or(true, |(bt, _)| t < bt) {
                best = Some((t, id));
            }
        }
    }
    best.map(|(_, id)| id)
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
