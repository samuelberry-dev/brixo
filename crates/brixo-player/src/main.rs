#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
//! Brixo Player: pick a game and play it. No editor, just the game.
//!
//! Usage:
//!   brixo-player                              the home screen (games open from the website)
//!   brixo-player brixo://play?...             what Play on the website opens
//!   brixo-player --uninstall                  what Windows' "Uninstall" runs
//!   brixo-player game.brixo                   play that file (so "Open with" works)
//!   brixo-player --join 127.0.0.1:4570 [--name Ann]   join a server

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use brixo_client::{draw_beacons, projector, Audio, ChatLog, Smoother, draw_gui, draw_hotbar, hotbar_key, movement_input, FollowCamera, GameEntry, GuiEvents, Held};
use brixo_client::install::{self, App};

mod protocol;
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
        brixo_client::theme::apply(&egui_ctx);
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
    /// The world as drawn this frame: smoothed between updates (see
    /// brixo_client::Smoother). Everything visual reads this.
    view: DataModel,
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
    /// Not in a game: games open from the website. Shows a message.
    Home { message: Option<String> },
    Playing(Box<Session>),
}

/// What the UI asked for this frame, applied once it's done.
enum Action {
    Leave,
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
    /// Set when the player leaves: the app closes, like Roblox.
    quit: bool,
    smoother: Smoother,
    audio: Audio,
    chat: ChatLog,
    /// The chat box is open for typing.
    chat_open: bool,
    chat_text: String,
    /// The cursor is locked to the window (first person).
    locked: bool,
    /// Where the OS couldn't truly lock the cursor (X11), it's confined
    /// instead: then we turn by how far it moved and put it back.
    confined: bool,
    started: Instant,
    /// A newer Brixo Player on the website, once the check finds one.
    update: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    update_hidden: bool,
}

impl Player {
    fn new() -> Self {
        Self {
            gpu: None,
            screen: Screen::Home { message: None },
            keys: HashSet::new(),
            camera: Camera::new(),
            last_frame: Instant::now(),
            empty: DataModel::new(),
            quit: false,
            smoother: Smoother::default(),
            audio: Audio::new(),
            chat: ChatLog::default(),
            chat_open: false,
            chat_text: String::new(),
            update: install::check_for_update(App::Player),
            update_hidden: false,
            locked: false,
            confined: false,
            started: Instant::now(),
        }
    }

    fn start_session(&mut self, name: String, backend: Backend, output: Vec<LogLine>) {
        self.camera = Camera::new();
        self.camera.pitch = -0.35;
        self.keys.clear();
        if let Some(gpu) = &self.gpu {
            if brixo_client::filming::window_title().is_none() {
                gpu.window.set_title(&format!("Brixo - {name}"));
            }
        }
        self.smoother.reset();
        self.screen = Screen::Playing(Box::new(Session {
            view: DataModel::new(),
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
    /// Opened from Play on the website: join that game's server with the
    /// one-time ticket. Who we are comes from the ticket, not from us.
    fn join_with_ticket(&mut self, link: protocol::PlayLink) {
        match NetClient::connect_with_ticket(&link.server, &link.ticket) {
            Ok(client) => {
                let name = link.game.clone().unwrap_or_else(|| "Brixo".to_string());
                self.start_session(name, Backend::Online(client), Vec::new())
            }
            Err(e) => {
                self.screen = Screen::Home { message: Some(format!("Couldn't reach the game server: {e}")) };
            }
        }
    }

    /// Developer option (`--join ADDR`): the studio's multiplayer test
    /// windows use this.
    fn join(&mut self, addr: String, name: String) {
        match NetClient::connect(&addr, &name) {
            Ok(client) => self.start_session(format!("{addr} (online)"), Backend::Online(client), Vec::new()),
            Err(e) => {
                self.screen = Screen::Home { message: Some(format!("Couldn't join {addr}: {e}")) };
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
                self.screen = Screen::Home { message: Some(format!("Couldn't open {}: {e}", entry.name)) };
            }
        }
    }

    fn leave(&mut self) {
        self.audio.stop_music();
        if let Some(gpu) = &self.gpu {
            gpu.window.set_title("Brixo");
        }
        self.keys.clear();
        // Leaving a game closes Brixo Player, like Roblox.
        self.quit = true;
    }

    /// Esc and F9, on the moment they're pressed.
    fn key_pressed(&mut self, code: KeyCode) {
        if let Screen::Playing(s) = &mut self.screen {
            if self.chat_open {
                if code == KeyCode::Escape {
                    self.chat_open = false;
                    self.chat_text.clear();
                }
                return;
            }
            match code {
                KeyCode::Slash | KeyCode::Enter if !s.paused && !s.lost => {
                    self.chat_open = true;
                    self.keys.clear();
                }
                KeyCode::Escape => s.paused = !s.paused,
                KeyCode::F9 => s.console = !s.console,
                _ => {}
            }
        }
    }

    fn frame(&mut self, dt: f32) {
        let chatting = self.chat_open;
        let held = |k| !chatting && self.keys.contains(&k);
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
        let mut first_person = false;
        if let Screen::Playing(s) = &mut self.screen {
            let mut input = if s.paused || s.lost { PlayerInput::default() } else { movement_input(&self.camera, held) };
            // Test hook for filming: queued actions (see pending_actions).
            let (mut queued_equip, mut queued_use) = (None, false);
            for line in pending_actions() {
                let mut words = line.split_whitespace();
                match (words.next(), words.next().and_then(|n| n.parse::<usize>().ok())) {
                    // "equip N" is the hotbar key N (1-9).
                    (Some("equip"), Some(n)) if n >= 1 => queued_equip = Some(n - 1),
                    (Some("use"), _) => queued_use = true,
                    (Some("jump"), _) => input.jump = true,
                    _ => {}
                }
            }
            // Test hook for recording demos: walk toward the point in
            // BRIXO_GOTO_FILE ("x z"), exactly as if WASD were held.
            if let Some(goto) = goto_target() {
                let here = match &s.backend {
                    Backend::Local(game) => game.player_position(),
                    Backend::Online(net) => net.me.and_then(|m| net.world.player(m)).map(|p| glam::Vec3::new(p.body.position.x, 0.0, p.body.position.z)),
                };
                if let Some(here) = here {
                    let d = glam::Vec2::new(goto.x - here.x, goto.y - here.z);
                    // Stop a little short: at low frame rates one frame moves
                    // most of a stud, and overshooting would turn it around.
                    input = if d.length() > 1.2 {
                        let d = d.normalize();
                        PlayerInput { move_x: d.x, move_z: d.y, jump: input.jump }
                    } else {
                        PlayerInput { jump: input.jump, ..Default::default() }
                    };
                }
            }
            match &mut s.backend {
                Backend::Local(game) => {
                    if let Some(me) = game.player_id() {
                        if let Some(slot) = queued_equip {
                            game.equip(me, Some(slot));
                        }
                        if queued_use {
                            game.activate(me);
                        }
                    }
                    game.set_input(input);
                    game.step(dt as f64);
                    s.output.extend(game.take_log());
                    for (from, name, text) in game.take_chat() {
                        self.chat.push(from, name, text);
                    }
                    let me = game.player_id();
                    for cue in game.take_sounds().into_iter().filter_map(|e| e.for_player(me)) {
                        let world = game.world();
                        self.audio.cue_with(&cue, &|id| brixo_client::world_sound(&world, id));
                    }
                    s.view = self.smoother.view(&game.world());
                    hidden = s.follow.update(&mut self.camera, &s.view, game.player_id());
                    write_position(&s.view, game.player_id());
                    first_person = s.follow.first_person(&s.view, game.player_id());
                }
                Backend::Online(net) => {
                    if let Some(slot) = queued_equip {
                        net.equip(slot);
                    }
                    if queued_use {
                        net.activate();
                    }
                    // The server runs the game; we send keys and draw its world.
                    net.send_input(input);
                    net.poll();
                    let assets = net.assets.clone();
                    let lookup = |id: u64| {
                        let id = brixo_core::InstanceId::from_raw(id);
                        let volume = net.world.sound(id).map(|s| s.volume).unwrap_or(1.0);
                        assets.get(&id).map(|b| (b.clone(), volume))
                    };
                    for cue in net.cues.clone() {
                        self.audio.cue_with(&cue, &lookup);
                    }
                    self.audio.retry_music(&lookup);
                    net.cues.clear();
                    for (from, name, text) in net.chat.drain(..) {
                        self.chat.push(from, name, text);
                    }
                    s.lost |= !net.connected;
                    s.view = self.smoother.view(&net.world);
                    hidden = s.follow.update(&mut self.camera, &s.view, net.me);
                    write_position(&s.view, net.me);
                    write_watch(&s.view);
                    first_person = s.follow.first_person(&s.view, net.me);
                }
            }
            let extra = s.output.len().saturating_sub(CONSOLE_LIMIT);
            s.output.drain(..extra);
        }

        // Filming: place the camera before the interface is drawn too, so
        // markers over the world (beacons) line up with what's filmed.
        if let Some((at, look)) = camera_shot() {
            aim(&mut self.camera, at, look);
        }
                let Some(gpu) = self.gpu.as_mut() else { return };
        let raw_input = gpu.egui_state.take_egui_input(&gpu.window);
        let mut action = None;
        let camera = &mut self.camera;
        let (chat, chat_open, chat_text) = (&self.chat, &mut self.chat_open, &mut self.chat_text);
        let mut said: Option<String> = None;
        // Clicks on the game's GUI and hotbar, and whether a click swung a tool.
        let mut gui_input: Option<(GuiEvents, bool)> = None;
        let update = self.update.lock().unwrap().clone();
        let update_hidden = &mut self.update_hidden;
        let full_output = gpu.egui_ctx.run(raw_input, |ctx| { match &mut self.screen {
            Screen::Home { message } => home_ui(ctx, message.as_deref()),
            Screen::Playing(s) => {
                // The game's own GUI and the hotbar, under our menus. The
                // world is read in its own scope: game_ui reads it again.
                let area = ctx.screen_rect();
                let mut events = {
                    let me = match &s.backend {
                        Backend::Local(game) => game.player_id(),
                        Backend::Online(net) => net.me,
                    };
                    let world = &s.view;
                    let project = projector(camera, area);
                    // Filming (BRIXO_CINEMATIC): no interface at all, just the world.
                    if cinematic() {
                        GuiEvents::default()
                    } else {
                        draw_beacons(ctx, area, world, me, camera);
                        let mut events = draw_gui(ctx, area, world, me, !s.paused && !s.lost, &project);
                        if let Some(me) = me {
                            draw_hotbar(ctx, area, world, me, &mut events);
                        }
                        chat.draw(ctx, area, world, &project);
                        events
                    }
                };
                let live = !s.paused && !s.lost;
                if live {
                    events.hotbar = events.hotbar.or_else(|| hotbar_key(ctx));
                }
                if !cinematic() {
                    game_ui(ctx, s, &mut action);
                }
                if *chat_open {
                    egui::Area::new(egui::Id::new("chat input"))
                        .anchor(egui::Align2::LEFT_BOTTOM, [12.0, -34.0])
                        .show(ctx, |ui| {
                            let edit = ui.add(
                                egui::TextEdit::singleline(chat_text)
                                    .desired_width(420.0)
                                    .hint_text("Say something (Enter to send, Esc to cancel)"),
                            );
                            // Check for Enter *before* keeping the focus:
                            // losing focus is how a text box reports Enter.
                            if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                said = Some(std::mem::take(chat_text));
                                *chat_open = false;
                            } else {
                                edit.request_focus();
                            }
                        });
                }
                let swing = live
                    && !events.pointer_on_gui
                    && !ctx.is_pointer_over_area()
                    && ctx.input(|i| i.pointer.primary_clicked());
                gui_input = live.then_some((events, swing));
                if live && !ctx.wants_pointer_input() {
                    look_around(ctx, camera, &mut s.follow);
                }
            }
        }
        update_notice(ctx, update.as_deref(), update_hidden);
        });
        gpu.egui_state.handle_platform_output(&gpu.window, full_output.platform_output);

        // Send what the player did with the GUI and tools to the game.
        // Send what the player typed in chat.
        if let (Some(text), Screen::Playing(s)) = (said, &mut self.screen) {
            match &mut s.backend {
                Backend::Local(game) => {
                    if let Some(me) = game.player_id() {
                        game.chat(me, &text);
                    }
                }
                Backend::Online(net) => net.chat(&text),
            }
        }
        // First person locks the mouse to the window, and the mouse turns
        // the camera; anything else (pause, chat, zooming out) frees it.
        let want_lock = first_person
            && !self.chat_open
            && matches!(&self.screen, Screen::Playing(s) if !s.paused && !s.lost);
        if want_lock != self.locked {
            {
                use winit::window::CursorGrabMode;
                let grab = if want_lock {
                    // A true lock (Windows, macOS) gives raw mouse motion;
                    // otherwise confine the cursor and recentre it.
                    match gpu.window.set_cursor_grab(CursorGrabMode::Locked) {
                        Ok(()) => {
                            self.confined = false;
                            Ok(())
                        }
                        Err(_) => {
                            self.confined = true;
                            gpu.window.set_cursor_grab(CursorGrabMode::Confined)
                        }
                    }
                } else {
                    self.confined = false;
                    gpu.window.set_cursor_grab(CursorGrabMode::None)
                };
                if grab.is_ok() || !want_lock {
                    gpu.window.set_cursor_visible(!want_lock);
                    self.locked = want_lock;
                    if want_lock && self.confined {
                        let size = gpu.window.inner_size();
                        let _ = gpu.window.set_cursor_position(winit::dpi::PhysicalPosition::new(size.width / 2, size.height / 2));
                    }
                }
            }
        }
        if let Some((events, _)) = &gui_input {
            if events.clicked.is_some() || events.hotbar.is_some() {
                self.audio.sound("click");
            }
        }
        if let (Some((events, swing)), Screen::Playing(s)) = (gui_input, &mut self.screen) {
            match &mut s.backend {
                Backend::Local(game) => {
                    if let Some(me) = game.player_id() {
                        if let Some(b) = events.clicked {
                            game.click(me, b);
                        }
                        if let Some(slot) = events.hotbar {
                            game.equip(me, Some(slot));
                        }
                        if swing {
                            game.activate(me);
                        }
                    }
                }
                Backend::Online(net) => {
                    if let Some(b) = events.clicked {
                        net.click(b);
                    }
                    if let Some(slot) = events.hotbar {
                        net.equip(slot);
                    }
                    if swing {
                        net.activate();
                    }
                }
            }
        }
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

        // Filming (BRIXO_CAMERA_FILE): a director script places the camera.
        // Mouse-look turned the camera after the world update placed it, in
        // the interface pass above (and raw mouse motion can arrive any time).
        // Put it back on its orbit for the angle it has *now*: otherwise it
        // sits where the old angle put it, and while turning the character
        // slides back and forth on screen: the jitter.
        if let Screen::Playing(s) = &mut self.screen {
            let me = match &s.backend {
                Backend::Local(game) => game.player_id(),
                Backend::Online(net) => net.me,
            };
            hidden = s.follow.update(&mut self.camera, &s.view, me);
        }
                if let Some((at, look)) = camera_shot() {
            aim(&mut self.camera, at, look);
            hidden = None;
        }
        gpu.scene.hidden_player = hidden;
        gpu.scene.time = self.started.elapsed().as_secs_f32();
        {
            let model = match &self.screen {
                Screen::Playing(s) => &s.view,
                Screen::Home { .. } => &self.empty,
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
            Some(Action::Leave) => self.leave(),
            None => {}
        }
    }
}

/// The point in BRIXO_GOTO_FILE, if that's set and the file says "x z".
/// Test hook for filming: hide the interface (BRIXO_CINEMATIC set).
fn cinematic() -> bool {
    std::env::var_os("BRIXO_CINEMATIC").is_some()
}

/// Puts the camera at `at`, looking toward `look`.
fn aim(camera: &mut Camera, at: glam::Vec3, look: glam::Vec3) {
    let d = (look - at).normalize_or(glam::Vec3::X);
    camera.position = at;
    camera.yaw = d.z.atan2(d.x);
    camera.pitch = d.y.clamp(-1.0, 1.0).asin();
}

/// Test hook for filming: the camera's position and the point it looks at,
/// from BRIXO_CAMERA_FILE ("x y z lx ly lz"), if that's set and present.
fn camera_shot() -> Option<(glam::Vec3, glam::Vec3)> {
    let text = std::fs::read_to_string(std::env::var_os("BRIXO_CAMERA_FILE")?).ok()?;
    let v: Vec<f32> = text.split_whitespace().filter_map(|n| n.parse().ok()).collect();
    (v.len() == 6).then(|| (glam::Vec3::new(v[0], v[1], v[2]), glam::Vec3::new(v[3], v[4], v[5])))
}

/// Test hook for filming: lines another program appends to
/// BRIXO_ACTION_FILE ("equip 2", "use", "jump"), read and cleared each frame.
fn pending_actions() -> Vec<String> {
    let Some(path) = std::env::var_os("BRIXO_ACTION_FILE") else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(&path) else { return Vec::new() };
    if text.is_empty() {
        return Vec::new();
    }
    let _ = std::fs::write(&path, "");
    text.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect()
}

/// Test hook for filming: where the player is ("x y z"), written to
/// BRIXO_POSITION_FILE every frame, so a director knows when it's arrived.
/// Filming hook: BRIXO_WATCH_FILE gets every player ("player name x y z
/// yaw dead team carrying") and every beacon part ("part name x y z") each
/// frame, so a director script can follow the action.
fn write_watch(world: &DataModel) {
    let Some(path) = std::env::var_os("BRIXO_WATCH_FILE") else { return };
    let attr = |id, key: &str| match world.get(id).and_then(|i| i.attributes.get(key)) {
        Some(brixo_core::Attribute::Str(s)) => s.replace(' ', "_"),
        _ => "-".to_string(),
    };
    let mut out = String::new();
    for id in world.walk() {
        let Some(inst) = world.get(id) else { continue };
        if let Some(p) = world.player(id) {
            let b = p.body.position;
            out += &format!("player {} {} {} {} {} {} {} {}\n", inst.name.replace(' ', "_"), b.x, b.y, b.z, p.body.rotation.y, (p.dead > 0.0) as u8, attr(id, "team"), attr(id, "carrying"));
        } else if let Some(p) = world.part(id) {
            if matches!(inst.attributes.get("beacon"), Some(brixo_core::Attribute::Bool(true))) {
                out += &format!("part {} {} {} {}\n", inst.name.replace(' ', "_"), p.position.x, p.position.y, p.position.z);
            }
        }
    }
    let path = std::path::PathBuf::from(path);
    let mut tmp = path.clone().into_os_string();
    tmp.push(".tmp");
    if std::fs::write(&tmp, out).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

fn write_position(world: &DataModel, me: Option<brixo_core::InstanceId>) {
    let Some(path) = std::env::var_os("BRIXO_POSITION_FILE") else { return };
    let Some(p) = me.and_then(|id| world.player(id)) else { return };
    let path = std::path::PathBuf::from(path);
    let mut tmp = path.clone().into_os_string();
    tmp.push(".tmp");
    let at = p.body.position;
    if std::fs::write(&tmp, format!("{} {} {}", at.x, at.y, at.z)).is_ok() {
        // Windows can briefly deny the rename if the reader has the target
        // open at that instant (POSIX always allows it); a quiet retry is
        // fine here since a stale position for one frame doesn't matter.
        for _ in 0..5 {
            if std::fs::rename(&tmp, &path).is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
}

fn goto_target() -> Option<glam::Vec2> {
    let path = std::env::var_os("BRIXO_GOTO_FILE")?;
    let text = std::fs::read_to_string(path).ok()?;
    let mut it = text.split_whitespace().map(|n| n.parse::<f32>());
    Some(glam::Vec2::new(it.next()?.ok()?, it.next()?.ok()?))
}

// --- ui --------------------------------------------------------------------

/// Shown when Brixo Player isn't in a game: games are opened from the
/// website's Play buttons, so there's nothing to pick here.
fn home_ui(ctx: &egui::Context, message: Option<&str>) {
    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.3);
            ui.label(egui::RichText::new("BRIXO").size(64.0).strong().color(egui::Color32::WHITE));
            ui.add_space(8.0);
            let text = message.unwrap_or("Games open from the Brixo website: find one you like and press Play.");
            // Long messages wrap instead of running off the window.
            ui.set_max_width((ui.available_width() - 40.0).min(760.0));
            ui.add(egui::Label::new(egui::RichText::new(text).size(20.0).color(egui::Color32::WHITE)).wrap());
            ui.add_space(16.0);
            let site = install::site();
            let label = format!("Open {}", site.trim_start_matches("https://").trim_start_matches("http://"));
            if ui.add(egui::Button::new(egui::RichText::new(label).size(18.0)).min_size(egui::vec2(220.0, 40.0))).clicked() {
                install::open_url(&site);
            }
        });
    });
}

/// "A new Brixo Player is out" along the top, until dismissed.
fn update_notice(ctx: &egui::Context, latest: Option<&str>, hidden: &mut bool) {
    let Some(latest) = latest else { return };
    if *hidden {
        return;
    }
    egui::Area::new(egui::Id::new("update notice"))
        .anchor(egui::Align2::CENTER_TOP, [0.0, 10.0])
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("A new {} is out ({latest}).", App::Player.title()));
                    if ui.button("Get it").clicked() {
                        install::open_url(&format!("{}/download", install::site()));
                    }
                    if ui.small_button("x").clicked() {
                        *hidden = true;
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

    if !s.lost && matches!(&s.backend, Backend::Online(n) if n.me.is_none()) {
        egui::Area::new(egui::Id::new("joining"))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(egui::RichText::new("Joining game...").size(22.0).strong());
                });
            });
    }

    if s.lost {
        egui::Window::new("Disconnected")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let never_joined = matches!(&s.backend, Backend::Online(n) if n.me.is_none());
                ui.label(if never_joined {
                    "Couldn't join: that Play link expired or was already used. Press Play on the website again."
                } else {
                    "The server closed, or the connection was lost."
                });
                ui.add_space(6.0);
                if ui.button(egui::RichText::new("Close").size(16.0)).clicked() {
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
                ui.label("1-9 hold a tool, click to use it");
                ui.label("/ or Enter to chat");
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
        let attrs = brixo_client::filming::window_attributes("Brixo", 1280.0, 800.0);
        let window = Arc::new(event_loop.create_window(attrs).expect("failed to create window"));
        self.gpu = Some(Gpu::new(window));

        // Opened by Play on the website: `brixo://play?server=...&ticket=...`.
        if let Some(link) = std::env::args().nth(1).as_deref().and_then(protocol::parse) {
            self.join_with_ticket(link);
            return;
        }
        // Developer options below: `--join ADDR [--name NAME]` (the studio's
        // test windows) and `brixo-player game.brixo`.
        let args: Vec<String> = std::env::args().collect();
        let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
        if let Some(addr) = flag("--join") {
            let name = flag("--name").unwrap_or_else(|| "Player".to_string());
            self.join(addr, name);
            return;
        }
        // Just installed from the website's download.
        if args.iter().any(|a| a == "--installed") {
            let site = install::site();
            let site = site.trim_start_matches("https://").trim_start_matches("http://");
            self.screen = Screen::Home { message: Some(format!("{} is installed!\nPick a game on {site} and press Play.", App::Player.title())) };
            return;
        }
        // `brixo-player path/to/game.brixo` plays that game straight away.
        if let Some(path) = std::env::args().nth(1).filter(|a| !a.starts_with("--")) {
            let path = Path::new(&path).to_path_buf();
            let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            self.play(GameEntry { name, path });
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: winit::event::DeviceId, event: winit::event::DeviceEvent) {
        // Raw mouse movement turns the camera while the mouse is locked.
        if let winit::event::DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if self.locked && !self.confined {
                self.camera.yaw += dx as f32 * 0.003;
                self.camera.pitch = (self.camera.pitch - dy as f32 * 0.003).clamp(-1.5, 1.5);
            }
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
            WindowEvent::CursorMoved { position, .. } if self.locked && self.confined => {
                // Confined mouse look: turn by the distance from the centre,
                // then warp back to the centre.
                if let Some(gpu) = &self.gpu {
                    let size = gpu.window.inner_size();
                    let (cx, cy) = ((size.width / 2) as f64, (size.height / 2) as f64);
                    let (dx, dy) = (position.x - cx, position.y - cy);
                    if dx.abs() > 0.5 || dy.abs() > 0.5 {
                        self.camera.yaw += dx as f32 * 0.003;
                        self.camera.pitch = (self.camera.pitch - dy as f32 * 0.003).clamp(-1.5, 1.5);
                        let _ = gpu.window.set_cursor_position(winit::dpi::PhysicalPosition::new(cx, cy));
                    }
                }
            }
            WindowEvent::Focused(false) => self.keys.clear(),
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last_frame).as_secs_f32().min(0.1);
                self.last_frame = now;
                self.frame(dt);
                if self.quit {
                    event_loop.exit();
                    return;
                }
                if let Some(gpu) = self.gpu.as_ref() {
                    gpu.window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() {
    // The downloaded BrixoPlayer.exe installs itself, then hands over to the
    // installed copy; `--uninstall` is what Windows' Uninstall runs.
    // (Only the Windows download is offered; elsewhere there's nothing to set up.)
    let startup = install::on_startup(App::Player, |exe| if cfg!(windows) { protocol::register_exe(exe).map(|_| ()) } else { Ok(()) });
    if startup == install::Startup::Exit {
        return;
    }
    // `brixo-player --register-protocol`: make Play on the website open us.
    if std::env::args().nth(1).as_deref() == Some("--register-protocol") {
        match protocol::register() {
            Ok(msg) => println!("{msg}"),
            Err(e) => {
                eprintln!("couldn't register brixo:// links: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut player = Player::new();
    event_loop.run_app(&mut player).expect("event loop failed");
}
