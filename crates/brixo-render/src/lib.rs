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
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

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
        for c in corners {
            vertices.push(Vertex {
                position: c,
                normal,
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
fn build_instances(
    model: &DataModel,
    selected: Option<brixo_core::InstanceId>,
) -> (Vec<InstanceRaw>, Vec<(brixo_core::InstanceId, brixo_core::PlayerProps, f32)>) {
    let mut out = Vec::new();
    let mut players = Vec::new();
    let mut stack = vec![model.root()];
    while let Some(id) = stack.pop() {
        let Some(inst) = model.get(id) else { continue };
        stack.extend(inst.children.iter().copied());

        let highlight = if selected == Some(id) { 1.0 } else { 0.0 };
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
        let m =
            Mat4::from_scale_rotation_translation(to_glam(p.size), rotation, to_glam(p.position));

        out.push(InstanceRaw {
            model: m.to_cols_array_2d(),
            color: [
                p.color.r as f32 / 255.0,
                p.color.g as f32 / 255.0,
                p.color.b as f32 / 255.0,
            ],
            highlight,
        });
    }
    (out, players)
}

fn rgb(r: u8, g: u8, b: u8) -> [f32; 3] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
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
        avatar::Slot::Ink => rgb(35, 35, 43),
        avatar::Slot::Shine => rgb(255, 255, 255),
    };
    let m = Mat4::from_translation(to_glam(p.body.position)) * Mat4::from_rotation_y(p.body.rotation.y.to_radians());
    InstanceRaw {
        model: m.to_cols_array_2d(),
        color,
        highlight,
    }
}

struct AvatarDraw {
    slot: avatar::Slot,
    face: Option<brixo_core::Face>,
    vertices: std::ops::Range<u32>,
}

// --- renderer --------------------------------------------------------------

pub struct SceneRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    /// All the avatar meshes, one after another.
    avatar_buffer: wgpu::Buffer,
    avatar_draws: Vec<AvatarDraw>,
    /// A player not to draw: your own character, in first person.
    pub hidden_player: Option<brixo_core::InstanceId>,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    depth_view: wgpu::TextureView,
}

impl SceneRenderer {
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let (vertices, indices) = cube();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let mut avatar_vertices: Vec<Vertex> = Vec::new();
        let mut avatar_draws = Vec::new();
        for mesh in avatar::meshes() {
            let start = avatar_vertices.len() as u32;
            avatar_vertices.extend(mesh.vertices);
            avatar_draws.push(AvatarDraw {
                slot: mesh.slot,
                face: mesh.face,
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
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera bind group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("part shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("part pipeline layout"),
            bind_group_layouts: &[&camera_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("part pipeline"),
            layout: Some(&pipeline_layout),
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
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let depth_view = create_depth_view(device, width, height);

        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
            avatar_buffer,
            avatar_draws,
            hidden_player: None,
            camera_buffer,
            camera_bind_group,
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
        selected: Option<brixo_core::InstanceId>,
        width: u32,
        height: u32,
    ) {
        let aspect = width as f32 / height.max(1) as f32;
        let uniform = CameraUniform {
            view_proj: camera.view_proj(aspect).to_cols_array_2d(),
        };
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));

        let (mut instances, players) = build_instances(model, selected);
        let part_count = instances.len() as u32;
        // Avatar instances go after the parts, grouped by mesh.
        let mut avatar_batches = Vec::new();
        for (i, draw) in self.avatar_draws.iter().enumerate() {
            let start = instances.len() as u32;
            for (id, p, highlight) in &players {
                if draw.face.is_some_and(|f| f != p.face) || self.hidden_player == Some(*id) {
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

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.05,
                        g: 0.07,
                        b: 0.10,
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
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_vertex_buffer(1, instance_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            if part_count > 0 {
                pass.draw_indexed(0..self.index_count, 0, 0..part_count);
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
};
@group(0) @binding(0) var<uniform> camera: Camera;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct InstanceInput {
    @location(2) m0: vec4<f32>,
    @location(3) m1: vec4<f32>,
    @location(4) m2: vec4<f32>,
    @location(5) m3: vec4<f32>,
    @location(6) color: vec3<f32>,
    @location(7) highlight: f32,
};

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) highlight: f32,
};

@vertex
fn vs_main(v: VertexInput, i: InstanceInput) -> VsOut {
    let model = mat4x4<f32>(i.m0, i.m1, i.m2, i.m3);
    var out: VsOut;
    out.clip_position = camera.view_proj * model * vec4<f32>(v.position, 1.0);
    out.color = i.color;
    out.normal = (model * vec4<f32>(v.normal, 0.0)).xyz;
    out.highlight = i.highlight;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let light_dir = normalize(vec3<f32>(0.4, 1.0, 0.3));
    let n = normalize(in.normal);
    let diffuse = max(dot(n, light_dir), 0.0);
    let shade = 0.35 + 0.65 * diffuse;
    var rgb = in.color * shade;
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
