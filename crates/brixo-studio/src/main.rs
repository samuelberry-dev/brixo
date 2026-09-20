use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use brixo_core::{Class, Color, DataModel, InstanceId, Vec3 as V};
use brixo_render::{Camera, SceneRenderer};
use glam::Vec3;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const SCENE_PATH: &str = "scene.brixo";

// --- gpu + egui ------------------------------------------------------------

struct Gpu {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    scene: SceneRenderer,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
}

impl Gpu {
    fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(window.clone())
            .expect("failed to create surface");

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("no suitable GPU adapter found");

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("brixo device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ))
        .expect("failed to create device");

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let scene = SceneRenderer::new(&device, format, config.width, config.height);

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            None,
            None,
            None,
        );
        let egui_renderer = egui_wgpu::Renderer::new(&device, format, None, 1, false);

        Self {
            window,
            surface,
            device,
            queue,
            config,
            scene,
            egui_ctx,
            egui_state,
            egui_renderer,
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.scene.resize(&self.device, width, height);
    }
}

// --- app -------------------------------------------------------------------

struct Studio {
    gpu: Option<Gpu>,
    model: DataModel,
    camera: Camera,
    selection: Option<InstanceId>,
    status: String,
    keys: HashSet<KeyCode>,
    looking: bool,
    last_frame: Instant,
}

impl Studio {
    fn new() -> Self {
        Self {
            gpu: None,
            model: demo_scene(),
            camera: Camera::new(),
            selection: None,
            status: "Ready".to_string(),
            keys: HashSet::new(),
            looking: false,
            last_frame: Instant::now(),
        }
    }

    fn move_camera(&mut self, dt: f32) {
        let speed = if self.keys.contains(&KeyCode::ControlLeft) {
            40.0
        } else {
            12.0
        } * dt;

        let forward = self.camera.forward();
        let right = self.camera.right();
        let mut delta = Vec3::ZERO;

        if self.keys.contains(&KeyCode::KeyW) {
            delta += forward;
        }
        if self.keys.contains(&KeyCode::KeyS) {
            delta -= forward;
        }
        if self.keys.contains(&KeyCode::KeyD) {
            delta += right;
        }
        if self.keys.contains(&KeyCode::KeyA) {
            delta -= right;
        }
        if self.keys.contains(&KeyCode::Space) {
            delta += Vec3::Y;
        }
        if self.keys.contains(&KeyCode::ShiftLeft) {
            delta -= Vec3::Y;
        }
        if delta.length_squared() > 0.0 {
            self.camera.position += delta.normalize() * speed;
        }
    }

    fn frame(&mut self) {
        let Studio {
            gpu,
            model,
            camera,
            selection,
            status,
            ..
        } = self;
        let Some(gpu) = gpu.as_mut() else { return };

        // --- build the UI ---
        let raw_input = gpu.egui_state.take_egui_input(&gpu.window);
        let full_output = gpu.egui_ctx.run(raw_input, |ctx| {
            build_ui(ctx, model, selection, status);
        });
        gpu.egui_state
            .handle_platform_output(&gpu.window, full_output.platform_output);

        let pixels_per_point = gpu.egui_ctx.pixels_per_point();
        let paint_jobs = gpu
            .egui_ctx
            .tessellate(full_output.shapes, pixels_per_point);

        // --- draw ---
        let frame = match gpu.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame encoder"),
            });

        gpu.scene.render(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &view,
            model,
            camera,
            *selection,
            gpu.config.width,
            gpu.config.height,
        );

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [gpu.config.width, gpu.config.height],
            pixels_per_point,
        };
        for (id, delta) in &full_output.textures_delta.set {
            gpu.egui_renderer
                .update_texture(&gpu.device, &gpu.queue, *id, delta);
        }
        gpu.egui_renderer.update_buffers(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let mut pass = pass.forget_lifetime();
            gpu.egui_renderer
                .render(&mut pass, &paint_jobs, &screen_descriptor);
        }

        gpu.queue.submit(Some(encoder.finish()));
        frame.present();

        for id in &full_output.textures_delta.free {
            gpu.egui_renderer.free_texture(id);
        }
    }
}

// --- ui --------------------------------------------------------------------

enum Action {
    AddPart,
    AddFolder,
    Delete,
    Save,
    Load,
}

fn build_ui(
    ctx: &egui::Context,
    model: &mut DataModel,
    selection: &mut Option<InstanceId>,
    status: &mut String,
) {
    let mut action: Option<Action> = None;

    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.button("Add Part").clicked() {
                action = Some(Action::AddPart);
            }
            if ui.button("Add Folder").clicked() {
                action = Some(Action::AddFolder);
            }
            if ui.button("Delete").clicked() {
                action = Some(Action::Delete);
            }
            ui.separator();
            if ui.button("Save").clicked() {
                action = Some(Action::Save);
            }
            if ui.button("Load").clicked() {
                action = Some(Action::Load);
            }
            ui.separator();
            ui.label(status.as_str());
        });
    });

    egui::SidePanel::left("explorer")
        .default_width(220.0)
        .show(ctx, |ui| {
            ui.heading("Explorer");
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                tree_node(ui, model, model.root(), selection);
            });
        });

    egui::SidePanel::right("properties")
        .default_width(260.0)
        .show(ctx, |ui| {
            ui.heading("Properties");
            ui.separator();

            let Some(id) = *selection else {
                ui.label("Nothing selected.");
                return;
            };
            let Some(inst) = model.get(id) else {
                ui.label("Nothing selected.");
                return;
            };

            let class = inst.class;
            let mut name = inst.name.clone();
            ui.horizontal(|ui| {
                ui.label("Name");
                if ui.text_edit_singleline(&mut name).changed() {
                    if let Some(i) = model.get_mut(id) {
                        i.name = name.clone();
                    }
                }
            });
            ui.label(format!("Class: {class:?}"));
            ui.separator();

            let Some(p) = model.part_mut(id) else {
                ui.label("No editable properties.");
                return;
            };

            vec3_row(ui, "Position", &mut p.position, 0.1);
            vec3_row(ui, "Size", &mut p.size, 0.1);
            vec3_row(ui, "Rotation", &mut p.rotation, 1.0);

            ui.horizontal(|ui| {
                ui.label("Color");
                let mut rgb = [p.color.r, p.color.g, p.color.b];
                if ui.color_edit_button_srgb(&mut rgb).changed() {
                    p.color = Color::new(rgb[0], rgb[1], rgb[2]);
                }
            });

            // Sizes below zero flip the cube inside out.
            p.size.x = p.size.x.max(0.01);
            p.size.y = p.size.y.max(0.01);
            p.size.z = p.size.z.max(0.01);
        });

    // Apply actions after the UI is built, so nothing is borrowed twice.
    match action {
        Some(Action::AddPart) => {
            let parent = container_for(model, *selection);
            if let Some(id) = model.create(Class::Part, "Part", parent) {
                *selection = Some(id);
                *status = "Added a Part".to_string();
            }
        }
        Some(Action::AddFolder) => {
            let parent = container_for(model, *selection);
            if let Some(id) = model.create(Class::Folder, "Folder", parent) {
                *selection = Some(id);
                *status = "Added a Folder".to_string();
            }
        }
        Some(Action::Delete) => {
            if let Some(id) = *selection {
                if model.remove(id) {
                    *selection = None;
                    *status = "Deleted".to_string();
                } else {
                    *status = "Can't delete that".to_string();
                }
            }
        }
        Some(Action::Save) => match model.save_file(SCENE_PATH) {
            Ok(()) => *status = format!("Saved to {SCENE_PATH}"),
            Err(e) => *status = format!("Save failed: {e}"),
        },
        Some(Action::Load) => match DataModel::load_file(SCENE_PATH) {
            Ok(loaded) => {
                *model = loaded;
                *selection = None;
                *status = format!("Loaded {SCENE_PATH}");
            }
            Err(e) => *status = format!("Load failed: {e}"),
        },
        None => {}
    }
}

/// New instances go inside the selection when it can hold children,
/// otherwise beside it.
fn container_for(model: &DataModel, selection: Option<InstanceId>) -> InstanceId {
    match selection.and_then(|id| model.get(id)) {
        Some(inst) if inst.class != Class::Part => inst.id,
        Some(inst) => inst.parent.unwrap_or_else(|| model.root()),
        None => model.root(),
    }
}

fn tree_node(
    ui: &mut egui::Ui,
    model: &DataModel,
    id: InstanceId,
    selection: &mut Option<InstanceId>,
) {
    let Some(inst) = model.get(id) else { return };
    let label = format!("{}  ({:?})", inst.name, inst.class);
    let children = inst.children.clone();

    if ui.selectable_label(*selection == Some(id), label).clicked() {
        *selection = Some(id);
    }

    if !children.is_empty() {
        ui.indent(id, |ui| {
            for child in children {
                tree_node(ui, model, child, selection);
            }
        });
    }
}

fn vec3_row(ui: &mut egui::Ui, label: &str, v: &mut V, speed: f32) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(&mut v.x).speed(speed).prefix("x "));
        ui.add(egui::DragValue::new(&mut v.y).speed(speed).prefix("y "));
        ui.add(egui::DragValue::new(&mut v.z).speed(speed).prefix("z "));
    });
}

// --- winit plumbing --------------------------------------------------------

impl ApplicationHandler for Studio {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Brixo Studio")
            .with_inner_size(winit::dpi::LogicalSize::new(1400.0, 860.0));
        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("failed to create window"),
        );
        self.gpu = Some(Gpu::new(window));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // Let egui see the event first; it tells us whether it used it.
        let consumed = if let Some(gpu) = self.gpu.as_mut() {
            let window = gpu.window.clone();
            gpu.egui_state.on_window_event(&window, &event).consumed
        } else {
            false
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.resize(size.width, size.height);
                }
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if consumed {
                    return;
                }
                if let PhysicalKey::Code(code) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => {
                            self.keys.insert(code);
                        }
                        ElementState::Released => {
                            self.keys.remove(&code);
                        }
                    }
                }
            }

            WindowEvent::MouseInput { button, state, .. } => {
                if button == MouseButton::Right && !consumed {
                    self.looking = state == ElementState::Pressed;
                }
                if state == ElementState::Released && button == MouseButton::Right {
                    self.looking = false;
                }
            }

            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last_frame).as_secs_f32().min(0.1);
                self.last_frame = now;

                self.move_camera(dt);
                self.frame();

                if let Some(gpu) = self.gpu.as_ref() {
                    gpu.window.request_redraw();
                }
            }

            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if self.looking {
                let sensitivity = 0.003;
                self.camera.yaw += delta.0 as f32 * sensitivity;
                self.camera.pitch -= delta.1 as f32 * sensitivity;
                self.camera.pitch = self.camera.pitch.clamp(-1.55, 1.55);
            }
        }
    }
}

fn demo_scene() -> DataModel {
    let mut dm = DataModel::new();
    let root = dm.root();

    let baseplate = dm.create(Class::Part, "Baseplate", root).unwrap();
    {
        let p = dm.part_mut(baseplate).unwrap();
        p.size = V::new(60.0, 1.0, 60.0);
        p.position = V::new(0.0, -0.5, 0.0);
        p.color = Color::new(90, 110, 90);
    }

    let red = dm.create(Class::Part, "RedBlock", root).unwrap();
    {
        let p = dm.part_mut(red).unwrap();
        p.size = V::new(4.0, 4.0, 4.0);
        p.position = V::new(0.0, 2.0, 0.0);
        p.color = Color::new(200, 60, 60);
    }

    let tower = dm.create(Class::Folder, "Tower", root).unwrap();
    for i in 0..5 {
        let id = dm
            .create(Class::Part, &format!("TowerBlock{i}"), tower)
            .unwrap();
        let p = dm.part_mut(id).unwrap();
        p.size = V::new(2.0, 2.0, 2.0);
        p.position = V::new(8.0, 1.0 + i as f32 * 2.0, -5.0);
        p.rotation = V::new(0.0, i as f32 * 15.0, 0.0);
        p.color = Color::new(220, 190, 80);
    }

    dm
}

fn main() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut studio = Studio::new();
    event_loop.run_app(&mut studio).expect("event loop failed");
}
