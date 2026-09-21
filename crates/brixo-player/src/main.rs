//! Brixo Player: pick a game and play it. No editor, just the game.
//!
//! Usage:
//!   brixo-player                              the games library
//!   brixo-player game.brixo                   play that file (so "Open with" works)
//!   brixo-player --join 127.0.0.1:4570 [--name Ann]   join a server

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use brixo_client::{list_games, movement_input, FollowCamera, GameEntry, Held};
use brixo_core::DataModel;
use brixo_render::{Camera, SceneRenderer};
use brixo_runtime::{Game, LogLine, PlayerInput};
use brixo_server::NetClient;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

/// Most console lines kept.
const CONSOLE_LIMIT: usize = 500;
/// Radians the camera turns per pixel of right-drag.
const LOOK_SPEED: f32 = 0.005;

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
        let surface = instance.create_surface(window.clone()).expect("failed to create surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("no suitable GPU adapter found");
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("brixo player device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ))
        .expect("failed to create device");

        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
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
        let egui_state = egui_winit::State::new(egui_ctx.clone(), egui::ViewportId::ROOT, &window, None, None, None);
        let egui_renderer = egui_wgpu::Renderer::new(&device, format, None, 1, false);

        Self { window, surface, device, queue, config, scene, egui_ctx, egui_state, egui_renderer }
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

// --- screens ---------------------------------------------------------------

/// Where the game runs: here, or on a server you've joined.
enum Backend {
    Local(Game),
    Online(NetClient),
}

/// A game being played.
struct Session {
    name: String,
    backend: Backend,
    /// The server went away.
    lost: bool,
    paused: bool,
    console: bool,
    output: Vec<LogLine>,
    follow: FollowCamera,
}

enum Screen {
    Library { games: Vec<GameEntry>, message: Option<String> },
    Playing(Box<Session>),
}

/// What the UI asked for this frame, applied once it's done.
enum Action {
    Play(GameEntry),
    Join,
    Leave,
    Refresh,
}

struct Player {
    gpu: Option<Gpu>,
    screen: Screen,
    keys: HashSet<KeyCode>,
    camera: Camera,
    last_frame: Instant,
    /// Drawn behind the library: just the sky.
    empty: DataModel,
    /// The "Join a server" boxes.
    join_addr: String,
    join_name: String,
}

impl Player {
    fn new() -> Self {
        Self {
            gpu: None,
            screen: Screen::Library { games: list_games(), message: None },
            keys: HashSet::new(),
            camera: Camera::new(),
            last_frame: Instant::now(),
            empty: DataModel::new(),
            join_addr: format!("127.0.0.1:{}", brixo_server::DEFAULT_PORT),
            join_name: "Player".to_string(),
        }
    }

    fn start_session(&mut self, name: String, backend: Backend, output: Vec<LogLine>) {
        self.camera = Camera::new();
        self.camera.pitch = -0.35;
        self.keys.clear();
        if let Some(gpu) = &self.gpu {
            gpu.window.set_title(&format!("Brixo - {name}"));
        }
        self.screen = Screen::Playing(Box::new(Session {
            name,
            backend,
            lost: false,
            paused: false,
            console: false,
            output,
            follow: FollowCamera::default(),
        }));
    }

    /// Joins a server as `name`.
    fn join(&mut self, addr: String, name: String) {
        match NetClient::connect(&addr, &name) {
            Ok(client) => self.start_session(format!("{addr} (online)"), Backend::Online(client), Vec::new()),
            Err(e) => {
                self.screen = Screen::Library {
                    games: list_games(),
                    message: Some(format!("Couldn't join {addr}: {e}")),
                };
            }
        }
    }

    fn play(&mut self, entry: GameEntry) {
        match DataModel::load_file(&entry.path.to_string_lossy()) {
            Ok(model) => {
                let game = Game::start(model);
                let output = game.take_log();
                self.start_session(entry.name, Backend::Local(game), output);
            }
            Err(e) => {
                self.screen = Screen::Library {
                    games: list_games(),
                    message: Some(format!("Couldn't open {}: {e}", entry.name)),
                };
            }
        }
    }

    fn leave(&mut self) {
        if let Some(gpu) = &self.gpu {
            gpu.window.set_title("Brixo");
        }
        self.keys.clear();
        self.screen = Screen::Library { games: list_games(), message: None };
    }

    /// Esc and F9, on the moment they're pressed.
    fn key_pressed(&mut self, code: KeyCode) {
        if let Screen::Playing(s) = &mut self.screen {
            match code {
                KeyCode::Escape => s.paused = !s.paused,
                KeyCode::F9 => s.console = !s.console,
                _ => {}
            }
        }
    }

    fn frame(&mut self, dt: f32) {
        let held = |k| self.keys.contains(&k);
        let held = Held {
            forward: held(KeyCode::KeyW),
            back: held(KeyCode::KeyS),
            left: held(KeyCode::KeyA),
            right: held(KeyCode::KeyD),
            jump: held(KeyCode::Space),
        };

        // Run the game. It keeps running while paused (in multiplayer the
        // world won't wait for you), but your character stands still.
        let mut hidden = None;
        if let Screen::Playing(s) = &mut self.screen {
            let input = if s.paused || s.lost { PlayerInput::default() } else { movement_input(&self.camera, held) };
            match &mut s.backend {
                Backend::Local(game) => {
                    game.set_input(input);
                    game.step(dt as f64);
                    s.output.extend(game.take_log());
                    hidden = s.follow.update(&mut self.camera, &game.world(), game.player_id());
                }
                Backend::Online(net) => {
                    // The server runs the game; we send keys and draw its world.
                    net.send_input(input);
                    net.poll();
                    s.lost |= !net.connected;
                    hidden = s.follow.update(&mut self.camera, &net.world, net.me);
                }
            }
            let extra = s.output.len().saturating_sub(CONSOLE_LIMIT);
            s.output.drain(..extra);
        }

        let Some(gpu) = self.gpu.as_mut() else { return };
        let raw_input = gpu.egui_state.take_egui_input(&gpu.window);
        let mut action = None;
        let camera = &mut self.camera;
        let (join_addr, join_name) = (&mut self.join_addr, &mut self.join_name);
        let full_output = gpu.egui_ctx.run(raw_input, |ctx| match &mut self.screen {
            Screen::Library { games, message } => {
                library_ui(ctx, games, message.as_deref(), join_addr, join_name, &mut action)
            }
            Screen::Playing(s) => {
                game_ui(ctx, s, &mut action);
                if !s.paused && !ctx.wants_pointer_input() {
                    look_around(ctx, camera, &mut s.follow);
                }
            }
        });
        gpu.egui_state.handle_platform_output(&gpu.window, full_output.platform_output);
        let pixels_per_point = gpu.egui_ctx.pixels_per_point();
        let paint_jobs = gpu.egui_ctx.tessellate(full_output.shapes, pixels_per_point);

        let frame = match gpu.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                return;
            }
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });

        gpu.scene.hidden_player = hidden;
        {
            let world;
            let model = match &self.screen {
                Screen::Playing(s) => match &s.backend {
                    Backend::Local(game) => {
                        world = game.world();
                        &*world
                    }
                    Backend::Online(net) => &net.world,
                },
                Screen::Library { .. } => &self.empty,
            };
            gpu.scene.render(
                &gpu.device,
                &gpu.queue,
                &mut encoder,
                &view,
                model,
                &self.camera,
                &[],
                gpu.config.width,
                gpu.config.height,
            );
        }

        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [gpu.config.width, gpu.config.height],
            pixels_per_point,
        };
        for (id, delta) in &full_output.textures_delta.set {
            gpu.egui_renderer.update_texture(&gpu.device, &gpu.queue, *id, delta);
        }
        gpu.egui_renderer.update_buffers(&gpu.device, &gpu.queue, &mut encoder, &paint_jobs, &screen);
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let mut pass = pass.forget_lifetime();
            gpu.egui_renderer.render(&mut pass, &paint_jobs, &screen);
        }
        gpu.queue.submit(Some(encoder.finish()));
        frame.present();
        for id in &full_output.textures_delta.free {
            gpu.egui_renderer.free_texture(id);
        }

        match action {
            Some(Action::Play(entry)) => self.play(entry),
            Some(Action::Join) => self.join(self.join_addr.clone(), self.join_name.clone()),
            Some(Action::Leave) => self.leave(),
            Some(Action::Refresh) => {
                self.screen = Screen::Library { games: list_games(), message: None };
            }
            None => {}
        }
    }
}

// --- ui --------------------------------------------------------------------

fn library_ui(
    ctx: &egui::Context,
    games: &[GameEntry],
    message: Option<&str>,
    join_addr: &mut String,
    join_name: &mut String,
    action: &mut Option<Action>,
) {
    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.12);
            ui.label(egui::RichText::new("BRIXO").size(64.0).strong().color(egui::Color32::WHITE));
            ui.label(egui::RichText::new("Pick a game to play").size(20.0).color(egui::Color32::WHITE));
            ui.add_space(24.0);

            if let Some(m) = message {
                ui.colored_label(egui::Color32::from_rgb(255, 120, 110), m);
                ui.add_space(8.0);
            }
            if games.is_empty() {
                ui.label(
                    egui::RichText::new("No games yet. In Brixo Studio, name your game and press Publish.")
                        .size(16.0)
                        .color(egui::Color32::WHITE),
                );
                ui.label(
                    egui::RichText::new(brixo_client::games_dir().display().to_string())
                        .size(13.0)
                        .color(egui::Color32::from_gray(220)),
                );
            }
            egui::ScrollArea::vertical().max_height(ui.available_height() - 80.0).show(ui, |ui| {
                for g in games {
                    let button = egui::Button::new(egui::RichText::new(&g.name).size(22.0))
                        .min_size(egui::vec2(360.0, 52.0));
                    if ui.add(button).clicked() {
                        *action = Some(Action::Play(g.clone()));
                    }
                    ui.add_space(6.0);
                }
            });
            ui.add_space(12.0);
            if ui.button("Refresh").clicked() {
                *action = Some(Action::Refresh);
            }

            ui.add_space(28.0);
            ui.label(egui::RichText::new("Join a server").size(18.0).color(egui::Color32::WHITE));
            ui.horizontal(|ui| {
                // Centre the row by hand: address, name, button.
                ui.add_space((ui.available_width() - 470.0).max(0.0) / 2.0);
                ui.add(egui::TextEdit::singleline(join_addr).desired_width(180.0).hint_text("address:port"));
                ui.add(egui::TextEdit::singleline(join_name).desired_width(140.0).hint_text("your name"));
                if ui.button(egui::RichText::new("Join").size(16.0)).clicked() {
                    *action = Some(Action::Join);
                }
            });
        });
    });
}

fn game_ui(ctx: &egui::Context, s: &mut Session, action: &mut Option<Action>) {
    // Game name, top left.
    egui::Area::new(egui::Id::new("game name"))
        .anchor(egui::Align2::LEFT_TOP, [12.0, 10.0])
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(egui::RichText::new(&s.name).size(16.0).strong());
            });
        });

    // Health, bottom left.
    let health = match &s.backend {
        Backend::Local(game) => game.player_id().and_then(|id| game.world().player(id).map(|p| (p.health, p.max_health))),
        Backend::Online(net) => net.me.and_then(|id| net.world.player(id).map(|p| (p.health, p.max_health))),
    };
    if let Some((hp, max)) = health {
        egui::Area::new(egui::Id::new("health"))
            .anchor(egui::Align2::LEFT_BOTTOM, [12.0, -12.0])
            .show(ctx, |ui| {
                let fraction = if max > 0.0 { (hp / max).clamp(0.0, 1.0) } else { 0.0 };
                let bar = egui::ProgressBar::new(fraction)
                    .desired_width(220.0)
                    .fill(egui::Color32::from_rgb(70, 180, 70))
                    .text(format!("Health {} / {}", hp.round(), max.round()));
                ui.add(bar);
            });
    }

    if s.console {
        egui::Window::new("Console (F9)")
            .default_size([520.0, 260.0])
            .anchor(egui::Align2::RIGHT_BOTTOM, [-12.0, -12.0])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                    for line in &s.output {
                        let text = format!("[{}] {}", line.source, line.text);
                        if line.is_error {
                            ui.colored_label(egui::Color32::from_rgb(255, 110, 100), text);
                        } else {
                            ui.label(text);
                        }
                    }
                });
            });
    }

    if s.lost {
        egui::Window::new("Disconnected")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("The server closed, or the connection was lost.");
                ui.add_space(6.0);
                if ui.button(egui::RichText::new("Back to games").size(16.0)).clicked() {
                    *action = Some(Action::Leave);
                }
            });
        return;
    }

    if s.paused {
        egui::Area::new(egui::Id::new("dim"))
            .fixed_pos(egui::Pos2::ZERO)
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                ui.painter().rect_filled(ctx.screen_rect(), 0.0, egui::Color32::from_black_alpha(120));
            });
        egui::Window::new("Paused")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(260.0);
                ui.vertical_centered_justified(|ui| {
                    if ui.button(egui::RichText::new("Resume").size(18.0)).clicked() {
                        s.paused = false;
                    }
                    ui.add_space(4.0);
                    if ui.button(egui::RichText::new("Leave game").size(18.0)).clicked() {
                        *action = Some(Action::Leave);
                    }
                });
                ui.add_space(8.0);
                ui.label("WASD move, Space jump");
                ui.label("Right-drag to look, scroll to zoom");
                ui.label("F9 console, Esc to close this menu");
            });
    }
}

/// Right-drag turns the camera; the wheel zooms.
fn look_around(ctx: &egui::Context, camera: &mut Camera, follow: &mut FollowCamera) {
    let (dragging, delta, scroll) =
        ctx.input(|i| (i.pointer.secondary_down(), i.pointer.delta(), i.smooth_scroll_delta.y));
    if dragging {
        camera.yaw += delta.x * LOOK_SPEED;
        camera.pitch = (camera.pitch - delta.y * LOOK_SPEED).clamp(-1.5, 1.5);
    }
    follow.zoom(scroll);
}

// --- window ----------------------------------------------------------------

impl ApplicationHandler for Player {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Brixo")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("failed to create window"));
        self.gpu = Some(Gpu::new(window));

        // `--join ADDR [--name NAME]` joins a server straight away.
        let args: Vec<String> = std::env::args().collect();
        let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
        if let Some(addr) = flag("--join") {
            let name = flag("--name").unwrap_or_else(|| "Player".to_string());
            self.join(addr, name);
            return;
        }
        // `brixo-player path/to/game.brixo` plays that game straight away.
        if let Some(path) = std::env::args().nth(1) {
            let path = Path::new(&path).to_path_buf();
            let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            self.play(GameEntry { name, path });
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let consumed = match self.gpu.as_mut() {
            Some(gpu) => {
                let window = gpu.window.clone();
                gpu.egui_state.on_window_event(&window, &event).consumed
            }
            None => false,
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
                        ElementState::Released => {
                            self.keys.remove(&code);
                        }
                        ElementState::Pressed => {
                            if !event.repeat {
                                self.key_pressed(code);
                            }
                            if !consumed {
                                self.keys.insert(code);
                            }
                        }
                    }
                }
            }
            WindowEvent::Focused(false) => self.keys.clear(),
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last_frame).as_secs_f32().min(0.1);
                self.last_frame = now;
                self.frame(dt);
                if let Some(gpu) = self.gpu.as_ref() {
                    gpu.window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut player = Player::new();
    event_loop.run_app(&mut player).expect("event loop failed");
}
