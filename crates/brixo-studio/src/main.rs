use std::collections::HashSet;
use std::f32::consts::{PI, TAU};
use std::sync::Arc;
use std::time::Instant;

use brixo_core::{Class, Color, DataModel, InstanceId, PartProps, Vec3 as V};
use brixo_render::{Camera, SceneRenderer};
use brixo_core::CameraMode;
use brixo_runtime::{Game, LogLine, PlayerInput};

/// Closest the third-person camera gets before switching to first person.
const MIN_FOLLOW_DISTANCE: f32 = 6.0;
use glam::{EulerRot, Mat4, Quat, Vec3};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const SCENE_PATH: &str = "scene.brixo";
/// Output lines kept in the Output panel.
const OUTPUT_LIMIT: usize = 1000;

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

// --- editor state ----------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tool {
    Move,
    Rotate,
    Scale,
}

/// An in-progress gizmo drag. Everything is measured from the drag's start,
/// so the part never drifts from rounding.
struct Drag {
    tool: Tool,
    axis: usize,
    world_axis: Vec3,
    start: PartProps,
    start_pointer: egui::Pos2,
    /// Unit direction of the axis on screen (move/scale).
    screen_dir: egui::Vec2,
    /// World units per screen pixel along that axis (move/scale).
    units_per_px: f32,
    /// Gizmo centre on screen (rotate).
    center_screen: egui::Pos2,
    /// +1 or -1 depending on whether the axis points toward the camera (rotate).
    facing: f32,
    last_angle: f32,
    accum: f32,
}

const HISTORY_LIMIT: usize = 200;

/// Whole-scene snapshots. Simple, and it covers every kind of edit at once.
#[derive(Default)]
struct History {
    undo: Vec<DataModel>,
    redo: Vec<DataModel>,
}

impl History {
    /// Call *before* changing the model.
    fn checkpoint(&mut self, model: &DataModel) {
        self.undo.push(model.clone());
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        // A new edit invalidates anything that was undone.
        self.redo.clear();
    }

    fn undo(&mut self, model: &mut DataModel) -> bool {
        match self.undo.pop() {
            Some(previous) => {
                self.redo.push(std::mem::replace(model, previous));
                true
            }
            None => false,
        }
    }

    fn redo(&mut self, model: &mut DataModel) -> bool {
        match self.redo.pop() {
            Some(next) => {
                self.undo.push(std::mem::replace(model, next));
                true
            }
            None => false,
        }
    }
}

struct Editor {
    tool: Tool,
    snap: bool,
    drag: Option<Drag>,
    history: History,
    /// True while a Properties edit is in progress (dragging a value,
    /// typing a name), so the whole edit becomes a single undo step.
    prop_session: bool,
    /// How far the camera sits behind the player during Play.
    follow_distance: f32,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            tool: Tool::Move,
            snap: true,
            drag: None,
            history: History::default(),
            prop_session: false,
            follow_distance: 16.0,
        }
    }
}

// --- app -------------------------------------------------------------------

struct Studio {
    gpu: Option<Gpu>,
    model: DataModel,
    camera: Camera,
    selection: Option<InstanceId>,
    status: String,
    editor: Editor,
    /// The running game, while Play is on. It works on a copy of `model`.
    game: Option<Game>,
    output: Vec<LogLine>,
    /// The studio camera, put back when Play stops.
    saved_camera: Option<(Vec3, f32, f32)>,
    keys: HashSet<KeyCode>,
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
            editor: Editor::default(),
            game: None,
            output: Vec::new(),
            saved_camera: None,
            keys: HashSet::new(),
            last_frame: Instant::now(),
        }
    }

    /// WASD relative to where the camera faces, flattened onto the ground.
    fn player_input(&self) -> PlayerInput {
        let forward = Vec3::new(self.camera.yaw.cos(), 0.0, self.camera.yaw.sin());
        let right = Vec3::new(-forward.z, 0.0, forward.x);
        let mut dir = Vec3::ZERO;
        if self.keys.contains(&KeyCode::KeyW) {
            dir += forward;
        }
        if self.keys.contains(&KeyCode::KeyS) {
            dir -= forward;
        }
        if self.keys.contains(&KeyCode::KeyD) {
            dir += right;
        }
        if self.keys.contains(&KeyCode::KeyA) {
            dir -= right;
        }
        let dir = dir.normalize_or_zero();
        PlayerInput {
            move_x: dir.x,
            move_z: dir.z,
            jump: self.keys.contains(&KeyCode::Space),
        }
    }

    fn move_camera(&mut self, dt: f32) {
        // During Play the keys drive the player and the camera follows it.
        if self.game.is_some() {
            return;
        }
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

    fn frame(&mut self, dt: f32) {
        // Advance the running game before drawing it.
        let input = self.player_input();
        let mut first_person_player = None;
        if let Some(game) = self.game.as_mut() {
            game.set_input(input);
            game.step(dt as f64);
            self.output.extend(game.take_log());
            trim_output(&mut self.output);
            // The game picks the camera mode; by default the wheel zooms
            // between third person and (all the way in) first person.
            if let Some(center) = game.player_position() {
                let mode = game
                    .player_id()
                    .and_then(|id| game.world().player(id).map(|p| p.camera_mode))
                    .unwrap_or_default();
                let first_person = match mode {
                    CameraMode::FirstPerson => true,
                    CameraMode::ThirdPerson => false,
                    CameraMode::Default => self.editor.follow_distance == 0.0,
                };
                first_person_player = if first_person { game.player_id() } else { None };
                if first_person {
                    // At eye level. Your own character is hidden, like in
                    // Roblox, so it doesn't block the view.
                    self.camera.position = center + Vec3::Y * 1.9;
                } else {
                    let distance = self.editor.follow_distance.max(MIN_FOLLOW_DISTANCE);
                    self.camera.position = center + Vec3::Y * 1.5 - self.camera.forward() * distance;
                }
            }
        }

        let Studio {
            gpu,
            model,
            camera,
            selection,
            status,
            editor,
            game,
            output,
            saved_camera,
            ..
        } = self;
        let Some(gpu) = gpu.as_mut() else { return };
        let playing = game.is_some();
        let play_time = game.as_ref().map(|g| g.time());
        let mut toggle_play = false;

        {
            // While playing, show and click the live game world, not the scene.
            let mut world_guard = game.as_ref().map(|g| g.world());
            let scene: &mut DataModel = match world_guard.as_mut() {
                Some(world) => world,
                None => model,
            };

            // --- build the UI (this also handles viewport mouse input) ---
            let raw_input = gpu.egui_state.take_egui_input(&gpu.window);
            let full_output = gpu.egui_ctx.run(raw_input, |ctx| {
                toggle_play |= build_ui(
                    ctx, scene, selection, status, camera, editor, output, playing, play_time,
                );
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

            gpu.scene.hidden_player = first_person_player;
            gpu.scene.render(
                &gpu.device,
                &gpu.queue,
                &mut encoder,
                &view,
                scene,
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
        } // the game world is unlocked here

        if toggle_play {
            editor.drag = None;
            editor.prop_session = false;
            if game.take().is_some() {
                // Stop: the copy is thrown away, so the scene is exactly as before.
                if let Some((position, yaw, pitch)) = saved_camera.take() {
                    camera.position = position;
                    camera.yaw = yaw;
                    camera.pitch = pitch;
                }
                output.push(system_line("Stopped"));
                *status = "Stopped".to_string();
                if selection.and_then(|id| model.get(id)).is_none() {
                    *selection = None;
                }
            } else {
                *saved_camera = Some((camera.position, camera.yaw, camera.pitch));
                camera.pitch = -0.35;
                output.push(system_line("Playing"));
                let started = Game::start(model.clone());
                output.extend(started.take_log());
                trim_output(output);
                *game = Some(started);
                *status = "Playing".to_string();
            }
        }
    }
}

fn system_line(text: &str) -> LogLine {
    LogLine {
        source: "Brixo".to_string(),
        text: format!("--- {text} ---"),
        is_error: false,
    }
}

fn trim_output(output: &mut Vec<LogLine>) {
    if output.len() > OUTPUT_LIMIT {
        let extra = output.len() - OUTPUT_LIMIT;
        output.drain(..extra);
    }
}

// --- ui --------------------------------------------------------------------

enum Action {
    AddPart,
    AddSpawn,
    AddScript,
    AddFolder,
    Delete,
    Save,
    Load,
    Undo,
    Redo,
}

/// Returns true if Play or Stop was pressed.
#[allow(clippy::too_many_arguments)]
fn build_ui(
    ctx: &egui::Context,
    model: &mut DataModel,
    selection: &mut Option<InstanceId>,
    status: &mut String,
    camera: &mut Camera,
    editor: &mut Editor,
    output: &mut Vec<LogLine>,
    playing: bool,
    play_time: Option<f64>,
) -> bool {
    let mut action: Option<Action> = None;
    let mut toggle_play = ctx.input(|i| i.key_pressed(egui::Key::F5));

    // Shortcuts, only when no text field has focus.
    if !ctx.wants_keyboard_input() {
        let (k1, k2, k3, del) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Num1),
                i.key_pressed(egui::Key::Num2),
                i.key_pressed(egui::Key::Num3),
                i.key_pressed(egui::Key::Delete),
            )
        });
        if k1 {
            editor.tool = Tool::Move;
        }
        if k2 {
            editor.tool = Tool::Rotate;
        }
        if k3 {
            editor.tool = Tool::Scale;
        }
        if del && !playing {
            action = Some(Action::Delete);
        }

        // Check redo first so Ctrl+Shift+Z isn't also read as Ctrl+Z.
        let (redo, undo) = ctx.input_mut(|i| {
            let redo = i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z)
                || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y);
            let undo = i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z);
            (redo, undo)
        });
        if playing {
            // Scene history is frozen while the game runs.
        } else if redo {
            action = Some(Action::Redo);
        } else if undo {
            action = Some(Action::Undo);
        }
    }

    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            let (label, color) = if playing {
                ("■ Stop (F5)", egui::Color32::from_rgb(200, 70, 70))
            } else {
                ("▶ Play (F5)", egui::Color32::from_rgb(60, 160, 80))
            };
            if ui
                .add(egui::Button::new(egui::RichText::new(label).color(egui::Color32::WHITE)).fill(color))
                .clicked()
            {
                toggle_play = true;
            }
            ui.separator();

            ui.add_enabled_ui(!playing, |ui| {
                ui.selectable_value(&mut editor.tool, Tool::Move, "Move (1)");
                ui.selectable_value(&mut editor.tool, Tool::Rotate, "Rotate (2)");
                ui.selectable_value(&mut editor.tool, Tool::Scale, "Scale (3)");
                ui.checkbox(&mut editor.snap, "Snap");
                ui.separator();
                if ui
                    .add_enabled(!editor.history.undo.is_empty(), egui::Button::new("Undo"))
                    .clicked()
                {
                    action = Some(Action::Undo);
                }
                if ui
                    .add_enabled(!editor.history.redo.is_empty(), egui::Button::new("Redo"))
                    .clicked()
                {
                    action = Some(Action::Redo);
                }
                ui.separator();
                if ui.button("Add Part").clicked() {
                    action = Some(Action::AddPart);
                }
                if ui.button("Add Spawn").clicked() {
                    action = Some(Action::AddSpawn);
                }
                if ui.button("Add Script").clicked() {
                    action = Some(Action::AddScript);
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
            });
            ui.separator();
            match play_time {
                Some(t) => ui.label(format!("Playing  {t:.1}s")),
                None => ui.label(status.as_str()),
            };
        });
    });

    // Bottom panels go before the side panels so they span the full width.
    egui::TopBottomPanel::bottom("output")
        .resizable(true)
        .default_height(140.0)
        .show(ctx, |ui| output_panel(ui, output));

    let selected_script = selection.filter(|id| model.script(*id).is_some());
    if let Some(script_id) = selected_script {
        egui::TopBottomPanel::bottom("script_editor")
            .resizable(true)
            .default_height(280.0)
            .min_height(160.0)
            .show(ctx, |ui| script_panel(ui, model, script_id, editor, playing));
    }

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
            ui.add_enabled_ui(!playing, |ui| properties_panel(ui, model, *selection, editor));
        });

    // The central panel is the 3D viewport. It's transparent, so the scene
    // drawn underneath shows through; egui just handles its input.
    egui::CentralPanel::default()
        .frame(egui::Frame::default())
        .show(ctx, |ui| {
            viewport(ui, model, selection, status, camera, editor, playing);
        });

    // Apply actions after the UI is built, so nothing is borrowed twice.
    match action {
        Some(Action::AddPart) => {
            editor.history.checkpoint(model);
            let parent = container_for(model, *selection);
            if let Some(id) = model.create(Class::Part, "Part", parent) {
                // Drop it in front of the camera instead of at the origin.
                let spot = camera.position + camera.forward() * 12.0;
                if let Some(p) = model.part_mut(id) {
                    p.position = V::new(spot.x.round(), spot.y.round().max(0.5), spot.z.round());
                    p.size = V::new(2.0, 2.0, 2.0);
                    // Like Roblox: new parts are loose and fall when you press Play.
                    p.anchored = false;
                }
                *selection = Some(id);
                *status = "Added a Part".to_string();
            }
        }
        Some(Action::AddSpawn) => {
            editor.history.checkpoint(model);
            let parent = container_for(model, *selection);
            if let Some(id) = model.create(Class::SpawnLocation, "SpawnLocation", parent) {
                let spot = camera.position + camera.forward() * 14.0;
                if let Some(p) = model.part_mut(id) {
                    p.position = V::new(spot.x.round(), 0.5, spot.z.round());
                }
                *selection = Some(id);
                *status = "Added a SpawnLocation. Players appear here when you press Play".to_string();
            }
        }
        Some(Action::AddScript) => {
            editor.history.checkpoint(model);
            let parent = script_parent_for(model, *selection);
            if let Some(id) = model.create(Class::Script, "Script", parent) {
                *selection = Some(id);
                *status = "Added a Script. Press Play to run it".to_string();
            }
        }
        Some(Action::AddFolder) => {
            editor.history.checkpoint(model);
            let parent = container_for(model, *selection);
            if let Some(id) = model.create(Class::Folder, "Folder", parent) {
                *selection = Some(id);
                *status = "Added a Folder".to_string();
            }
        }
        Some(Action::Delete) => {
            if let Some(id) = *selection {
                if id != model.root() && model.get(id).is_some() {
                    editor.history.checkpoint(model);
                }
                if model.remove(id) {
                    *selection = None;
                    editor.drag = None;
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
                editor.history.checkpoint(model);
                *model = loaded;
                *selection = None;
                editor.drag = None;
                *status = format!("Loaded {SCENE_PATH}");
            }
            Err(e) => *status = format!("Load failed: {e}"),
        },
        Some(Action::Undo) => {
            if editor.history.undo(model) {
                after_history_jump(model, selection, editor);
                *status = "Undo".to_string();
            }
        }
        Some(Action::Redo) => {
            if editor.history.redo(model) {
                after_history_jump(model, selection, editor);
                *status = "Redo".to_string();
            }
        }
        None => {}
    }

    toggle_play
}

fn output_panel(ui: &mut egui::Ui, output: &mut Vec<LogLine>) {
    ui.horizontal(|ui| {
        ui.strong("Output");
        if ui.small_button("Clear").clicked() {
            output.clear();
        }
    });
    ui.separator();
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for line in output.iter() {
                let text = egui::RichText::new(format!("[{}] {}", line.source, line.text)).monospace();
                if line.is_error {
                    ui.label(text.color(egui::Color32::from_rgb(240, 95, 95)));
                } else {
                    ui.label(text);
                }
            }
        });
}

/// The code editor for the selected script, with a live syntax check.
fn script_panel(
    ui: &mut egui::Ui,
    model: &mut DataModel,
    id: InstanceId,
    editor: &mut Editor,
    playing: bool,
) {
    let Some(inst) = model.get(id) else { return };
    let name = inst.name.clone();
    let Some(script) = model.script(id) else { return };
    let mut source = script.source.clone();
    let mut enabled = script.enabled;
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.strong(format!("Script: {name}"));
        ui.separator();
        changed |= ui
            .add_enabled(!playing, egui::Checkbox::new(&mut enabled, "Enabled"))
            .changed();
        if playing {
            ui.weak("read-only while playing");
        }
    });
    ui.separator();

    let editor_height = (ui.available_height() - 24.0).max(60.0);
    egui::ScrollArea::vertical()
        .max_height(editor_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let edit = egui::TextEdit::multiline(&mut source)
                .code_editor()
                .desired_width(f32::INFINITY)
                .desired_rows(12)
                .interactive(!playing);
            changed |= ui.add(edit).changed();
        });

    // Parsing is fast, so check on every frame for instant feedback.
    match rovik::lexer::lex(&source).and_then(rovik::parser::parse) {
        Ok(_) => {
            ui.weak("No syntax errors");
        }
        Err(e) => {
            ui.colored_label(egui::Color32::from_rgb(240, 95, 95), format!("⚠ {e}"));
        }
    }

    if changed && !playing {
        // One undo step per typing session, like the Properties panel.
        if !editor.prop_session {
            editor.history.checkpoint(model);
            editor.prop_session = true;
        }
        if let Some(script) = model.script_mut(id) {
            script.source = source;
            script.enabled = enabled;
        }
    }
}

/// Clears state that may point at things the undo/redo just removed.
fn after_history_jump(model: &DataModel, selection: &mut Option<InstanceId>, editor: &mut Editor) {
    editor.drag = None;
    editor.prop_session = false;
    if selection.and_then(|id| model.get(id)).is_none() {
        *selection = None;
    }
}

fn properties_panel(
    ui: &mut egui::Ui,
    model: &mut DataModel,
    selection: Option<InstanceId>,
    editor: &mut Editor,
) {
    ui.heading("Properties");
    ui.separator();

    let Some(id) = selection else {
        ui.label("Nothing selected.");
        editor.prop_session = false;
        return;
    };
    let Some(inst) = model.get(id) else {
        ui.label("Nothing selected.");
        editor.prop_session = false;
        return;
    };

    // Edit copies, so the model is still untouched when we detect a change.
    // That lets us checkpoint the true "before" state.
    let class = inst.class;
    let mut name = inst.name.clone();
    let mut props = model.part(id).copied();
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.label("Name");
        changed |= ui.text_edit_singleline(&mut name).changed();
    });
    ui.label(format!("Class: {class:?}"));
    ui.separator();

    match props.as_mut() {
        Some(p) => {
            changed |= vec3_row(ui, "Position", &mut p.position, 0.1);
            changed |= vec3_row(ui, "Size", &mut p.size, 0.1);
            changed |= vec3_row(ui, "Rotation", &mut p.rotation, 1.0);

            ui.horizontal(|ui| {
                ui.label("Color");
                let mut rgb = [p.color.r, p.color.g, p.color.b];
                if ui.color_edit_button_srgb(&mut rgb).changed() {
                    p.color = Color::new(rgb[0], rgb[1], rgb[2]);
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                changed |= ui
                    .checkbox(&mut p.anchored, "Anchored")
                    .on_hover_text("Anchored parts stay put when you press Play. Unanchored parts fall.")
                    .changed();
                changed |= ui
                    .checkbox(&mut p.can_collide, "Can collide")
                    .on_hover_text("Off: things pass through it, but it still fires 'on touched'.")
                    .changed();
            });

            p.size.x = p.size.x.max(0.01);
            p.size.y = p.size.y.max(0.01);
            p.size.z = p.size.z.max(0.01);
        }
        None => {
            ui.label("No editable properties.");
        }
    }

    if changed {
        // First change of a new edit: snapshot before applying it.
        if !editor.prop_session {
            editor.history.checkpoint(model);
            editor.prop_session = true;
        }
        if let Some(i) = model.get_mut(id) {
            i.name = name;
        }
        if let (Some(new), Some(p)) = (props, model.part_mut(id)) {
            *p = new;
        }
    } else {
        // The edit is over once nothing is held and no text field has focus.
        let busy = ui.input(|i| i.pointer.any_down()) || ui.ctx().wants_keyboard_input();
        if !busy {
            editor.prop_session = false;
        }
    }
}

fn viewport(
    ui: &mut egui::Ui,
    model: &mut DataModel,
    selection: &mut Option<InstanceId>,
    status: &mut String,
    camera: &mut Camera,
    editor: &mut Editor,
    playing: bool,
) {
    let (rect, response) =
        ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());

    // The 3D scene fills the whole window, so projection uses the screen rect.
    let screen = ui.ctx().screen_rect();
    let aspect = screen.width() / screen.height().max(1.0);

    // Scroll wheel: zoom the follow camera during Play. Zooming in past
    // the closest distance switches to first person; zooming out leaves it.
    if playing && response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let d = editor.follow_distance;
            editor.follow_distance = if d == 0.0 {
                if scroll < 0.0 { MIN_FOLLOW_DISTANCE } else { 0.0 }
            } else {
                let next = d - scroll * 0.05;
                if next < MIN_FOLLOW_DISTANCE - 1.0 { 0.0 } else { next.clamp(MIN_FOLLOW_DISTANCE, 60.0) }
            };
        }
    }

    // Right-drag: look around (during Play, orbit the player).
    if response.dragged_by(egui::PointerButton::Secondary) {
        let d = response.drag_delta();
        camera.yaw += d.x * 0.005;
        camera.pitch = (camera.pitch - d.y * 0.005).clamp(-1.55, 1.55);
    }

    let vp = camera.view_proj(aspect);
    let tool = editor.tool;
    let snap = editor.snap;

    // No gizmos while playing: the game world isn't editable.
    let gizmo = if playing {
        None
    } else {
        selection.and_then(|id| model.part(id).map(|p| Gizmo::new(p, camera, tool)))
    };

    // Left-drag starting on a handle: begin a gizmo drag.
    if response.drag_started_by(egui::PointerButton::Primary) {
        let origin = ui.input(|i| i.pointer.press_origin());
        if let (Some(g), Some(o), Some(id)) = (gizmo.as_ref(), origin, *selection) {
            if let Some(axis) = g.hit_test(&vp, screen, o) {
                let start = model.part(id).copied();
                if let Some(start) = start {
                    if let Some(drag) = Drag::begin(g, &vp, screen, axis, o, start, camera) {
                        // One undo step per drag, taken before anything moves.
                        editor.history.checkpoint(model);
                        editor.drag = Some(drag);
                    }
                }
            }
        }
    }

    // Continue an active drag.
    if response.dragged_by(egui::PointerButton::Primary) {
        if let (Some(drag), Some(id)) = (editor.drag.as_mut(), *selection) {
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some(p) = model.part_mut(id) {
                    drag.apply(pos, snap, p);
                }
            }
        }
    }

    if response.drag_stopped() {
        editor.drag = None;
    }

    // Plain left-click (no drag): select whatever is under the cursor,
    // unless the click landed on a gizmo handle.
    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let on_handle = gizmo
                .as_ref()
                .and_then(|g| g.hit_test(&vp, screen, pos))
                .is_some();
            if !on_handle {
                let ndc_x = (pos.x - screen.left()) / screen.width() * 2.0 - 1.0;
                let ndc_y = 1.0 - (pos.y - screen.top()) / screen.height() * 2.0;
                let hit = brixo_render::pick(model, camera, aspect, ndc_x, ndc_y);
                *status = match hit.and_then(|id| model.get(id)) {
                    Some(inst) => format!("Selected {}", inst.name),
                    None => "Nothing there".to_string(),
                };
                *selection = hit;
            }
        }
    }

    // Draw the gizmo for the (possibly just-moved) selection.
    let visible = if playing { None } else { *selection };
    if let Some(g) = visible.and_then(|id| model.part(id).map(|p| Gizmo::new(p, camera, tool))) {
        let active = match editor.drag.as_ref() {
            Some(d) => Some(d.axis),
            None => response
                .hover_pos()
                .and_then(|pos| g.hit_test(&vp, screen, pos)),
        };
        g.draw(&ui.painter_at(rect), &vp, screen, active);
    }
}

// --- gizmo -----------------------------------------------------------------

struct Gizmo {
    tool: Tool,
    center: Vec3,
    axes: [Vec3; 3],
    /// Handle length in world units, scaled with distance so the gizmo
    /// stays roughly the same size on screen.
    length: f32,
}

impl Gizmo {
    fn new(p: &PartProps, camera: &Camera, tool: Tool) -> Self {
        let center = to_glam(p.position);
        let length = (camera.position - center).length().max(1.0) * 0.18;
        let axes = match tool {
            // Scale works along the part's own axes.
            Tool::Scale => {
                let q = part_quat(p);
                [q * Vec3::X, q * Vec3::Y, q * Vec3::Z]
            }
            Tool::Move | Tool::Rotate => [Vec3::X, Vec3::Y, Vec3::Z],
        };
        Self {
            tool,
            center,
            axes,
            length,
        }
    }

    fn handle_end(&self, i: usize) -> Vec3 {
        self.center + self.axes[i] * self.length
    }

    fn ring_points(&self, i: usize, vp: &Mat4, screen: egui::Rect) -> Vec<egui::Pos2> {
        let u = self.axes[(i + 1) % 3];
        let v = self.axes[(i + 2) % 3];
        (0..=64)
            .filter_map(|k| {
                let t = k as f32 / 64.0 * TAU;
                project(vp, screen, self.center + (u * t.cos() + v * t.sin()) * self.length)
            })
            .collect()
    }

    /// Which handle (0 = X, 1 = Y, 2 = Z) is under `pos`, if any.
    fn hit_test(&self, vp: &Mat4, screen: egui::Rect, pos: egui::Pos2) -> Option<usize> {
        const THRESHOLD: f32 = 10.0;
        let mut best: Option<(f32, usize)> = None;

        for i in 0..3 {
            let dist = match self.tool {
                Tool::Rotate => polyline_distance(&self.ring_points(i, vp, screen), pos),
                Tool::Move | Tool::Scale => {
                    let (Some(a), Some(b)) = (
                        project(vp, screen, self.center),
                        project(vp, screen, self.handle_end(i)),
                    ) else {
                        continue;
                    };
                    segment_distance(pos, a, b)
                }
            };
            if dist < THRESHOLD && best.map_or(true, |(bd, _)| dist < bd) {
                best = Some((dist, i));
            }
        }
        best.map(|(_, i)| i)
    }

    fn draw(&self, painter: &egui::Painter, vp: &Mat4, screen: egui::Rect, active: Option<usize>) {
        let colors = [
            egui::Color32::from_rgb(230, 70, 70),
            egui::Color32::from_rgb(80, 200, 90),
            egui::Color32::from_rgb(70, 130, 240),
        ];
        let highlight = egui::Color32::from_rgb(255, 210, 60);

        let Some(c) = project(vp, screen, self.center) else {
            return;
        };

        for i in 0..3 {
            let is_active = active == Some(i);
            let color = if is_active { highlight } else { colors[i] };
            let stroke = egui::Stroke::new(if is_active { 4.0 } else { 2.5 }, color);

            match self.tool {
                Tool::Rotate => {
                    let points = self.ring_points(i, vp, screen);
                    if points.len() > 1 {
                        painter.add(egui::Shape::line(points, stroke));
                    }
                }
                Tool::Move | Tool::Scale => {
                    let Some(e) = project(vp, screen, self.handle_end(i)) else {
                        continue;
                    };
                    painter.line_segment([c, e], stroke);
                    if self.tool == Tool::Move {
                        painter.circle_filled(e, 6.0, color);
                    } else {
                        painter.rect_filled(
                            egui::Rect::from_center_size(e, egui::vec2(11.0, 11.0)),
                            0.0,
                            color,
                        );
                    }
                }
            }
        }
        painter.circle_filled(c, 3.0, egui::Color32::WHITE);
    }
}

impl Drag {
    fn begin(
        g: &Gizmo,
        vp: &Mat4,
        screen: egui::Rect,
        axis: usize,
        origin: egui::Pos2,
        start: PartProps,
        camera: &Camera,
    ) -> Option<Self> {
        let c = project(vp, screen, g.center)?;
        let e = project(vp, screen, g.handle_end(axis))?;
        let along = e - c;
        let len = along.length();

        // An axis pointing straight at the camera can't be dragged along.
        if g.tool != Tool::Rotate && len < 2.0 {
            return None;
        }

        let facing = if g.axes[axis].dot(camera.position - g.center) > 0.0 {
            -1.0
        } else {
            1.0
        };

        Some(Self {
            tool: g.tool,
            axis,
            world_axis: g.axes[axis],
            start,
            start_pointer: origin,
            screen_dir: if len > 0.0 { along / len } else { egui::Vec2::X },
            units_per_px: g.length / len.max(1e-3),
            center_screen: c,
            facing,
            last_angle: (origin - c).angle(),
            accum: 0.0,
        })
    }

    fn apply(&mut self, pos: egui::Pos2, snap: bool, p: &mut PartProps) {
        match self.tool {
            Tool::Move => {
                let mut amount = (pos - self.start_pointer).dot(self.screen_dir) * self.units_per_px;
                if snap {
                    amount = amount.round();
                }
                p.position = from_glam(to_glam(self.start.position) + self.world_axis * amount);
            }

            Tool::Scale => {
                let mut amount = (pos - self.start_pointer).dot(self.screen_dir) * self.units_per_px;
                if snap {
                    amount = amount.round();
                }
                let start_size = component(self.start.size, self.axis);
                let new_size = (start_size + amount).max(0.1);
                let grown = new_size - start_size;
                set_component(&mut p.size, self.axis, new_size);
                // Grow toward the handle: the opposite face stays put.
                p.position =
                    from_glam(to_glam(self.start.position) + self.world_axis * (grown * 0.5));
            }

            Tool::Rotate => {
                let angle = (pos - self.center_screen).angle();
                let mut delta = angle - self.last_angle;
                if delta > PI {
                    delta -= TAU;
                } else if delta < -PI {
                    delta += TAU;
                }
                self.accum += delta;
                self.last_angle = angle;

                let mut degrees = (self.accum * self.facing).to_degrees();
                if snap {
                    degrees = (degrees / 15.0).round() * 15.0;
                }

                // Rotate about the world axis as a quaternion, then store as Euler.
                let q = Quat::from_axis_angle(self.world_axis, degrees.to_radians())
                    * part_quat(&self.start);
                let (y, x, z) = q.to_euler(EulerRot::YXZ);
                p.rotation = V::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
            }
        }
    }
}

// --- math helpers ----------------------------------------------------------

fn to_glam(v: V) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

fn from_glam(v: Vec3) -> V {
    V::new(v.x, v.y, v.z)
}

fn component(v: V, i: usize) -> f32 {
    match i {
        0 => v.x,
        1 => v.y,
        _ => v.z,
    }
}

fn set_component(v: &mut V, i: usize, value: f32) {
    match i {
        0 => v.x = value,
        1 => v.y = value,
        _ => v.z = value,
    }
}

/// Must match the rotation order the renderer uses.
fn part_quat(p: &PartProps) -> Quat {
    Quat::from_euler(
        EulerRot::YXZ,
        p.rotation.y.to_radians(),
        p.rotation.x.to_radians(),
        p.rotation.z.to_radians(),
    )
}

/// World point to screen position (in egui points). None if behind the camera.
fn project(vp: &Mat4, screen: egui::Rect, p: Vec3) -> Option<egui::Pos2> {
    let clip = *vp * p.extend(1.0);
    if clip.w <= 1e-4 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    Some(egui::pos2(
        screen.left() + (ndc.x + 1.0) * 0.5 * screen.width(),
        screen.top() + (1.0 - ndc.y) * 0.5 * screen.height(),
    ))
}

fn segment_distance(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    if len_sq < 1e-6 {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

fn polyline_distance(points: &[egui::Pos2], p: egui::Pos2) -> f32 {
    points
        .windows(2)
        .map(|w| segment_distance(p, w[0], w[1]))
        .fold(f32::INFINITY, f32::min)
}

// --- explorer --------------------------------------------------------------

/// New parts and folders go inside a selected folder, otherwise beside
/// the selection.
fn container_for(model: &DataModel, selection: Option<InstanceId>) -> InstanceId {
    match selection.and_then(|id| model.get(id)) {
        Some(inst) if matches!(inst.class, Class::Folder | Class::Workspace) => inst.id,
        Some(inst) => inst.parent.unwrap_or_else(|| model.root()),
        None => model.root(),
    }
}

/// New scripts go inside the selection (usually a part), or beside a
/// selected script since scripts can't hold children.
fn script_parent_for(model: &DataModel, selection: Option<InstanceId>) -> InstanceId {
    match selection.and_then(|id| model.get(id)) {
        Some(inst) if inst.class.can_hold_children() => inst.id,
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

/// Returns true if any of the three values changed.
fn vec3_row(ui: &mut egui::Ui, label: &str, v: &mut V, speed: f32) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        let x = ui.add(egui::DragValue::new(&mut v.x).speed(speed).prefix("x "));
        let y = ui.add(egui::DragValue::new(&mut v.y).speed(speed).prefix("y "));
        let z = ui.add(egui::DragValue::new(&mut v.z).speed(speed).prefix("z "));
        x.changed() || y.changed() || z.changed()
    })
    .inner
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
        // egui sees every event first. Mouse input is handled inside the UI
        // pass now; winit only tracks movement keys for the camera.
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
                if let PhysicalKey::Code(code) = event.physical_key {
                    match event.state {
                        // Always honour releases so keys can't get stuck
                        // when focus moves into a text field.
                        ElementState::Released => {
                            self.keys.remove(&code);
                        }
                        ElementState::Pressed if !consumed => {
                            self.keys.insert(code);
                        }
                        ElementState::Pressed => {}
                    }
                }
            }

            WindowEvent::Focused(false) => self.keys.clear(),

            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last_frame).as_secs_f32().min(0.1);
                self.last_frame = now;

                self.move_camera(dt);
                self.frame(dt);

                if let Some(gpu) = self.gpu.as_ref() {
                    gpu.window.request_redraw();
                }
            }

            _ => {}
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
        p.color = Color::new(99, 95, 98);
    }

    let red = dm.create(Class::Part, "RedBlock", root).unwrap();
    {
        let p = dm.part_mut(red).unwrap();
        p.size = V::new(4.0, 4.0, 4.0);
        p.position = V::new(0.0, 2.0, 0.0);
        p.color = Color::new(200, 60, 60);
    }
    let spin = dm.create(Class::Script, "Spin", red).unwrap();
    dm.script_mut(spin).unwrap().source = "-- Spins this block. Press Play!\nevery 0.05 seconds\n    self.rotation.y += 3\nend\n".to_string();

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
        p.anchored = false;
    }

    // --- a little course for the player ---
    let spawn = dm.create(Class::SpawnLocation, "SpawnLocation", root).unwrap();
    dm.part_mut(spawn).unwrap().position = V::new(-12.0, 0.5, 8.0);

    let coin = dm.create(Class::Part, "Coin", root).unwrap();
    {
        let p = dm.part_mut(coin).unwrap();
        p.size = V::new(1.6, 1.6, 0.4);
        p.position = V::new(-12.0, 2.5, 0.0);
        p.color = Color::new(255, 200, 40);
        p.can_collide = false;
    }
    let s = dm.create(Class::Script, "Collect", coin).unwrap();
    dm.script_mut(s).unwrap().source = "-- Walk into me! Can collide is off, so you pass through.\non touched(other)\n    if other.class == \"player\" then\n        print(other.name + \" grabbed a coin!\")\n        other.face = \"happy\"\n        destroy(self)\n    end\nend\n\nevery 0.03 seconds\n    self.rotation.y += 5\nend\n".to_string();

    let lava = dm.create(Class::Part, "Lava", root).unwrap();
    {
        let p = dm.part_mut(lava).unwrap();
        p.size = V::new(8.0, 0.2, 4.0);
        p.position = V::new(-12.0, 0.1, -8.0);
        p.color = Color::new(255, 80, 20);
    }
    let s = dm.create(Class::Script, "Burn", lava).unwrap();
    dm.script_mut(s).unwrap().source = "-- Touching lava sends you back to the spawn.\non touched(other)\n    if other.class == \"player\" then\n        other.health = 0\n    end\nend\n".to_string();

    let platform = dm.create(Class::Part, "Platform", root).unwrap();
    {
        let p = dm.part_mut(platform).unwrap();
        p.size = V::new(6.0, 2.0, 6.0);
        p.position = V::new(-3.0, 1.0, 8.0);
        p.color = Color::new(120, 120, 150);
    }

    // Drops onto the edge of the tower and knocks it over when you press Play.
    let wrecker = dm.create(Class::Part, "Wrecker", root).unwrap();
    {
        let p = dm.part_mut(wrecker).unwrap();
        p.size = V::new(4.0, 4.0, 4.0);
        p.position = V::new(10.0, 22.0, -5.0);
        p.color = Color::new(70, 110, 200);
        p.anchored = false;
    }

    dm
}

fn main() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut studio = Studio::new();
    event_loop.run_app(&mut studio).expect("event loop failed");
}
