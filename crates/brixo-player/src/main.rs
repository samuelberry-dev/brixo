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

use brixo_client::{draw_beacons, keyboard_look, projector, trackpad_look, Audio, CameraKeys, ChatLog, Smoother, draw_gui, draw_hotbar, hotbar_key, movement_input, FollowCamera, GameEntry, GuiEvents, Held};
use brixo_client::install::{self, App};

mod film;
mod mac_links;
mod menus;
mod protocol;
mod settings;
use settings::Settings;
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
/// Turning with the mouse locked (first person, shift lock), per pixel.
const LOCKED_LOOK_SPEED: f32 = 0.003;

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
            // Filming reads each frame back (film.rs).
            usage: if std::env::var_os("BRIXO_FILM").is_some() {
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC
            } else {
                wgpu::TextureUsages::RENDER_ATTACHMENT
            },
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
        brixo_client::theme::apply_site(&egui_ctx);
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
    /// The leaderboard is unfolded (Tab folds it).
    board_open: bool,
    output: Vec<LogLine>,
    follow: FollowCamera,
    /// Shift lock is on (Shift toggles it).
    shift_lock: bool,
    /// Online: your own character, moved at once instead of waiting for
    /// the server (see brixo_client::Predictor).
    predictor: brixo_client::Predictor,
}

enum Screen {
    /// Not in a game: games open from the website. Shows a message.
    Home { message: Option<String> },
    /// Checking for (and getting) a newer Brixo Player before anything
    /// else: see brixo_client::update. Play links wait until it's done.
    Updating,
    Playing(Box<Session>),
}

/// What the UI asked for this frame, applied once it's done.
enum Action {
    Leave,
    /// Fullscreen on or off.
    Fullscreen,
    /// Knock yourself out, to come back at a spawn (when stuck).
    Reset,
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
    /// Updating on launch (None once done, or for builds that don't).
    updater: Option<brixo_client::update::Updater>,
    /// A Play link (macOS) that arrived while updating: joined after, or
    /// handed to the new version.
    pending_link: Option<String>,
    /// The Play link we joined with, to use again after an update.
    last_link: Option<String>,
    /// The game's server said this Player is too old.
    need_update: bool,
    /// Updating because a server asked (not on launch): tried once only.
    server_update: bool,
    /// Your settings, and what's in the file (saved when they differ and
    /// the menu's closed).
    settings: Settings,
    saved_settings: Settings,
    pause_tab: menus::PauseTab,
    /// The volumes last given to the speakers.
    levels: Option<(f32, f32)>,
    /// Filming a trailer shot (BRIXO_FILM).
    film: Option<film::Film>,
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
            updater: brixo_client::update::Updater::start(App::Player),
            pending_link: None,
            last_link: None,
            need_update: false,
            server_update: false,
            locked: false,
            confined: false,
            started: Instant::now(),
            settings: Settings::load(),
            saved_settings: Settings::load(),
            pause_tab: menus::PauseTab::default(),
            levels: None,
            film: film::load().inspect(|f| FILM_NO_HUD.store(!f.hud(), std::sync::atomic::Ordering::Relaxed)),
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
            board_open: true,
            output,
            follow: FollowCamera::default(),
            shift_lock: false,
            predictor: brixo_client::Predictor::default(),
        }));
    }

    /// What we were opened to do: a Play link, a developer option, a file.
    fn start_from_args(&mut self) {
        // Opened by Play on the website: `brixo://play?server=...&ticket=...`.
        if let Some(raw) = std::env::args().nth(1).filter(|a| protocol::parse(a).is_some()) {
            self.last_link = Some(raw.clone());
        }
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
        if let Some(path) = std::env::args().nth(1).filter(|a| !a.starts_with('-')) {
            let path = Path::new(&path).to_path_buf();
            let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            self.play(GameEntry { name, path });
        }
    }

    /// While updating on launch: carry on once it's done or has failed, or
    /// hand over to the new version.
    fn poll_update(&mut self) {
        // The game's server needs a newer Player: get it, then come back
        // into the same game.
        if self.need_update && self.updater.is_none() {
            self.need_update = false;
            let tried = std::mem::replace(&mut self.server_update, true);
            match brixo_client::update::Updater::start(App::Player).filter(|_| !tried) {
                Some(u) => {
                    self.updater = Some(u);
                    self.pending_link = self.last_link.clone();
                    self.started = Instant::now();
                    self.screen = Screen::Updating;
                }
                None => {
                    self.screen = Screen::Home { message: Some("This game needs a newer Brixo Player than this one. Try again in a minute.".into()) };
                }
            }
            return;
        }
        let Some(updater) = &self.updater else { return };
        let state = updater.state();
        // A check that hangs mustn't keep anyone out of their game.
        let stuck = state == brixo_client::update::State::Checking && self.started.elapsed().as_secs() > 8;
        if state.busy() && !stuck {
            return;
        }
        if let brixo_client::update::State::Ready { .. } = state {
            match updater.relaunch(self.pending_link.as_deref()) {
                Ok(()) => {
                    self.quit = true;
                    return;
                }
                Err(e) => eprintln!("update: {e}"),
            }
        }
        self.updater = None;
        if self.server_update {
            // Couldn't get the newer Player the game needs: don't loop.
            let why = match state {
                brixo_client::update::State::Failed(e) => format!(" ({e})"),
                _ => String::new(),
            };
            self.pending_link = None;
            self.screen = Screen::Home { message: Some(format!("This game needs a newer Brixo Player, and updating didn't work{why}. Download it again from the website.")) };
            return;
        }
        self.screen = Screen::Home { message: None };
        self.start_from_args();
        if let Some(link) = self.pending_link.take().as_deref().and_then(protocol::parse) {
            self.join_with_ticket(link);
        }
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

    /// Fullscreen on or off (the toolbar's button, and F11).
    fn toggle_fullscreen(&mut self) {
        if let Some(gpu) = &self.gpu {
            let full = gpu.window.fullscreen().is_some();
            gpu.window.set_fullscreen(if full { None } else { Some(winit::window::Fullscreen::Borderless(None)) });
        }
    }

    /// Esc and F9, on the moment they're pressed.
    fn key_pressed(&mut self, code: KeyCode) {
        if code == KeyCode::F11 {
            self.toggle_fullscreen();
            return;
        }
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
                KeyCode::Escape => {
                    s.paused = !s.paused;
                    self.pause_tab = menus::PauseTab::Game;
                }
                // Shift lock, like Roblox: Shift switches it on and off.
                KeyCode::ShiftLeft | KeyCode::ShiftRight if self.settings.shift_lock && !s.paused && !s.lost => {
                    s.shift_lock = !s.shift_lock;
                    self.audio.sound("click");
                }
                KeyCode::F9 => s.console = !s.console,
                KeyCode::Tab if !s.lost => s.board_open = !s.board_open,
                // Keys games hear (`on key(player, key)`): E, Q, F, R...
                other if !s.paused && !s.lost => {
                    if let Some(key) = script_key(other) {
                        match &mut s.backend {
                            Backend::Local(game) => {
                                if let Some(me) = game.player_id() {
                                    game.key(me, key);
                                }
                            }
                            Backend::Online(net) => net.key(key),
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn frame(&mut self, dt: f32) {
        let chatting = self.chat_open;
        let held = |k| !chatting && self.keys.contains(&k);
        let cam_keys = CameraKeys {
            turn_left: held(KeyCode::ArrowLeft),
            turn_right: held(KeyCode::ArrowRight),
            tilt_up: held(KeyCode::PageUp),
            tilt_down: held(KeyCode::PageDown),
            zoom_in: held(KeyCode::KeyI),
            zoom_out: held(KeyCode::KeyO),
        };
        let held = Held {
            // Up/Down walk too, like Roblox (Left/Right turn the camera).
            forward: held(KeyCode::KeyW) || held(KeyCode::ArrowUp),
            back: held(KeyCode::KeyS) || held(KeyCode::ArrowDown),
            left: held(KeyCode::KeyA),
            right: held(KeyCode::KeyD),
            jump: held(KeyCode::Space),
        };
        // Driving a kart? (Arrow keys steer then, and the camera chases.)
        let driving = match &self.screen {
            Screen::Playing(s) => {
                let me = match &s.backend {
                    Backend::Local(game) => game.player_id(),
                    Backend::Online(net) => net.me,
                };
                me.and_then(|m| s.view.player(m)).and_then(|p| p.kart).is_some()
            }
            _ => false,
        };
        let kart_held = Held {
            left: held.left || (!chatting && self.keys.contains(&KeyCode::ArrowLeft)),
            right: held.right || (!chatting && self.keys.contains(&KeyCode::ArrowRight)),
            ..held
        };
        // The camera keys, for trackpads (see brixo_client::CameraKeys).
        if let Screen::Playing(s) = &mut self.screen {
            if !s.paused && !s.lost && !driving {
                keyboard_look(&mut self.camera, &mut s.follow, cam_keys, dt);
            }
        }

        // Run the game. It keeps running while paused (in multiplayer the
        // world won't wait for you), but your character stands still.
        let mut hidden = None;
        let mut first_person = false;
        if let Screen::Playing(s) = &mut self.screen {
            let mut input = if s.paused || s.lost {
                PlayerInput::default()
            } else if driving {
                brixo_client::kart_input(kart_held)
            } else {
                movement_input(&self.camera, held)
            };
            // Test hook for filming: queued actions (see pending_actions).
            let (mut queued_equip, mut queued_use) = (None, false);
            let mut queued_say = Vec::new();
            let scripted = self.film.as_mut().map(|f| f.actions()).unwrap_or_default();
            for line in pending_actions().into_iter().chain(scripted) {
                // "say ..." chats; "menu", "help" and "settings" open the menu.
                if let Some(text) = line.strip_prefix("say ") {
                    queued_say.push(text.to_string());
                    continue;
                }
                let page = match line.as_str() {
                    "menu" => Some(menus::PauseTab::Game),
                    "help" => Some(menus::PauseTab::Controls),
                    "settings" => Some(menus::PauseTab::Settings),
                    "leave" => Some(menus::PauseTab::Leave),
                    _ => None,
                };
                if let Some(page) = page {
                    s.paused = true;
                    self.pause_tab = page;
                    continue;
                }
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
            if let Some(goto) = goto_target().or(self.film.as_ref().and_then(|f| f.walk())) {
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
            // Shift lock: face where the camera looks, and look from over
            // the shoulder.
            let shift = s.shift_lock && !s.paused && !s.lost && !driving;
            s.follow.shoulder = shift;
            let facing = shift.then(|| brixo_client::shift_lock_yaw(&self.camera));
            match &mut s.backend {
                Backend::Local(game) => {
                    game.set_facing(facing);
                    if let Some(me) = game.player_id() {
                        // Filming: BRIXO_AUTOPILOT drives your kart for you.
                        if std::env::var_os("BRIXO_AUTOPILOT").is_some() {
                            game.set_autopilot(me, true);
                        }
                        if let Some(slot) = queued_equip {
                            game.equip(me, Some(slot));
                        }
                        if queued_use {
                            game.activate(me);
                        }
                        for text in &queued_say {
                            game.chat(me, text);
                        }
                    }
                    game.set_input(input);
                    game.step(dt as f64);
                    let log = game.take_log();
                    if let Some(f) = &mut self.film {
                        f.log(&log);
                    }
                    s.output.extend(log);
                    for (from, name, text) in game.take_chat() {
                        self.chat.push(from, name, text);
                    }
                    let me = game.player_id();
                    for cue in game.take_sounds().into_iter().filter_map(|e| e.for_player(me)) {
                        let world = game.world();
                        if let Some(f) = &mut self.film {
                            f.sound(&cue, &world);
                        }
                        self.audio.cue_with(&cue, &|id| brixo_client::world_sound(&world, id));
                    }
                    s.view = self.smoother.view(&game.world());
                    brixo_client::smooth::hold_tools(&mut s.view, &game.world());
                    brixo_runtime::kart::pose_all(&mut s.view);
                    hidden = s.follow.update(&mut self.camera, &s.view, game.player_id());
                    write_position(&s.view, game.player_id());
                    first_person = s.follow.first_person(&s.view, game.player_id());
                }
                Backend::Online(net) => {
                    net.send_facing(facing);
                    if let Some(slot) = queued_equip {
                        net.equip(slot);
                    }
                    if queued_use {
                        net.activate();
                    }
                    for text in &queued_say {
                        net.chat(text);
                    }
                    // The server runs the game; we send keys and draw its world.
                    // (A server that takes numbered steps gets them below.)
                    if !net.server_steps {
                        net.send_input(input);
                    }
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
                    if net.update_required {
                        self.need_update = true;
                    }
                    // Your own character: predicted, so it moves the moment
                    // you press a key (BRIXO_NO_PREDICT turns this off, to
                    // compare).
                    if net.server_steps {
                        // Server Authority: our keys for each physics step,
                        // numbered; the server says where each left us, and
                        // the predictor rewinds and replays when it differs.
                        let (first, keys) = s.predictor.step_steps(&net.world, net.me, net.you, net.queued, input, facing, dt);
                        net.send_steps(first, &keys);
                    } else if std::env::var_os("BRIXO_NO_PREDICT").is_none() {
                        s.predictor.step(&net.world, net.me, input, facing, dt);
                    }
                    s.view = self.smoother.view(&net.world);
                    s.predictor.apply(&mut s.view);
                    // Your tool in your (predicted) hand.
                    brixo_client::smooth::hold_tools(&mut s.view, &net.world);
                    brixo_runtime::kart::pose_all(&mut s.view);
                    hidden = s.follow.update(&mut self.camera, &s.view, net.me);
                    write_position(&s.view, net.me);
                    write_watch(&s.view);
                    first_person = s.follow.first_person(&s.view, net.me);
                }
            }
            // Your kart's engine, humming with its speed.
            let me = match &s.backend {
                Backend::Local(game) => game.player_id(),
                Backend::Online(net) => net.me,
            };
            self.audio.engine(if s.lost { None } else { brixo_client::kart_fx::engine_pitch(&s.view, me) });
            // Sounds in the world: heard from the camera.
            self.audio.listen(self.camera.position, self.camera.right(), &s.view);
            let extra = s.output.len().saturating_sub(CONSOLE_LIMIT);
            s.output.drain(..extra);
        }

        // Filming: place the camera before the interface is drawn too, so
        // markers over the world (beacons) line up with what's filmed.
        if let Some((at, look)) = camera_shot() {
            aim(&mut self.camera, at, look);
        }
        self.film_camera();
        // Filming: nothing's drawn while the game warms up (it's faster).
        if self.film.as_ref().is_some_and(|f| f.warming()) && matches!(self.screen, Screen::Playing(_)) {
            return;
        }
                let Some(gpu) = self.gpu.as_mut() else { return };
        let mut raw_input = gpu.egui_state.take_egui_input(&gpu.window);
        // Filming: effects that animate by the clock follow the game's time.
        if let Some(f) = &self.film {
            raw_input.time = Some(f.sim as f64);
        }
        let mut action = None;
        let camera = &mut self.camera;
        let (chat, chat_open, chat_text) = (&self.chat, &mut self.chat_open, &mut self.chat_text);
        let mut said: Option<String> = None;
        let mut opened_chat = false;
        // Clicks on the game's GUI and hotbar, and whether a click swung a
        // tool (and where in the world the mouse pointed).
        let mut gui_input: Option<(GuiEvents, Option<brixo_core::Vec3>)> = None;
        let locked = self.locked;
        let update = self.update.lock().unwrap().clone();
        let update_hidden = &mut self.update_hidden;
        let settings = &mut self.settings;
        let pause_tab = &mut self.pause_tab;
        let full_output = gpu.egui_ctx.run(raw_input, |ctx| { match &mut self.screen {
            Screen::Home { message } => menus::home_ui(ctx, message.as_deref()),
            Screen::Updating => {
                let state = self.updater.as_ref().map(|u| u.state()).unwrap_or(brixo_client::update::State::Checking);
                menus::updating_ui(ctx, &state.label(), state.fraction());
            }
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
                        brixo_client::kart_fx::draw_effects(ctx, world, camera.position, &project, ctx.input(|i| i.time) as f32);
                        brixo_client::kart_fx::draw_hud(ctx, area, world, me);
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
                    let shift_locked = s.shift_lock;
                    menus::game_ui(ctx, menus::Hud { session: s, action: &mut action, settings, tab: pause_tab, shift_locked });
                }
                // The chat bar under the toolbar: click it (or press /) to talk.
                if !cinematic() && !s.lost {
                    let was_open = *chat_open;
                    said = brixo_client::classic::chat_bar(ctx, area.left_top() + egui::vec2(5.0, brixo_client::classic::TOP_BAR), 330.0, chat_open, chat_text);
                    opened_chat = *chat_open && !was_open;
                }
                let swing = live
                    && !events.pointer_on_gui
                    && !ctx.is_pointer_over_area()
                    && ctx.input(|i| i.pointer.primary_clicked());
                // Tools aim where you click (the middle of the screen when
                // the mouse is locked, in first person).
                let aim = swing.then(|| {
                    let ndc = match ctx.input(|i| i.pointer.interact_pos()) {
                        Some(p) if !locked => (
                            (p.x - area.left()) / area.width() * 2.0 - 1.0,
                            1.0 - (p.y - area.top()) / area.height() * 2.0,
                        ),
                        _ => (0.0, 0.0),
                    };
                    let me = match &s.backend {
                        Backend::Local(game) => game.player_id(),
                        Backend::Online(net) => net.me,
                    };
                    brixo_client::aim_point(&s.view, camera, area.width() / area.height(), me, ndc)
                });
                gui_input = live.then_some((events, aim));
                if live && !ctx.wants_pointer_input() {
                    look_around(ctx, camera, &mut s.follow, settings.camera_speed);
                }
            }
        }
        menus::update_notice(ctx, update.as_deref(), update_hidden);
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
        // Your settings: the speakers' volumes follow them, and they're
        // saved once the menu's closed.
        let levels = (self.settings.sounds, self.settings.music);
        if self.levels != Some(levels) {
            self.audio.set_levels(levels.0, levels.1);
            self.levels = Some(levels);
        }
        let menu_open = matches!(&self.screen, Screen::Playing(s) if s.paused);
        if self.settings != self.saved_settings && !menu_open {
            self.settings.save();
            self.saved_settings = self.settings;
        }
        if !self.settings.shift_lock {
            if let Screen::Playing(s) = &mut self.screen {
                s.shift_lock = false;
            }
        }
        // First person and shift lock lock the mouse to the window, and the
        // mouse turns the camera; anything else (pause, chat) frees it.
        let shift_locked = matches!(&self.screen, Screen::Playing(s) if s.shift_lock);
        let want_lock = (first_person || shift_locked)
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
                        if let Some(aim) = swing {
                            game.activate_at(me, Some(aim));
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
                    if let Some(aim) = swing {
                        net.activate_at(aim);
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
        if self.film_camera() {
            hidden = None;
        }
        let Some(gpu) = self.gpu.as_mut() else { return };
        gpu.scene.hidden_player = hidden;
        gpu.scene.time = match &self.film {
            Some(f) => f.sim,
            None => self.started.elapsed().as_secs_f32(),
        };
        {
            let model = match &self.screen {
                Screen::Playing(s) => &s.view,
                Screen::Home { .. } | Screen::Updating => &self.empty,
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
        if let Some(f) = self.film.as_mut().filter(|f| !f.warming() && matches!(self.screen, Screen::Playing(_))) {
            let pixels = film::read_back(&gpu.device, &gpu.queue, &frame.texture);
            let bgra = matches!(gpu.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
            f.save(&pixels, gpu.config.width, gpu.config.height, bgra);
            if f.done() {
                self.quit = true;
            }
        }
        frame.present();
        for id in &full_output.textures_delta.free {
            gpu.egui_renderer.free_texture(id);
        }

        if opened_chat {
            // Clicked the chat bar: what's held down stops (you're typing).
            self.keys.clear();
        }
        match action {
            Some(Action::Leave) => self.leave(),
            Some(Action::Fullscreen) => self.toggle_fullscreen(),
            Some(Action::Reset) => {
                if let Screen::Playing(s) = &mut self.screen {
                    match &mut s.backend {
                        Backend::Local(game) => {
                            if let Some(me) = game.player_id() {
                                if let Some(p) = game.world().player_mut(me) {
                                    p.health = 0.0;
                                }
                            }
                        }
                        Backend::Online(net) => net.reset(),
                    }
                }
            }
            None => {}
        }
    }
}

/// The point in BRIXO_GOTO_FILE, if that's set and the file says "x z".
/// Test hook for filming: hide the interface (BRIXO_CINEMATIC set).
fn cinematic() -> bool {
    std::env::var_os("BRIXO_CINEMATIC").is_some() || FILM_NO_HUD.load(std::sync::atomic::Ordering::Relaxed)
}

/// Filming a shot without the interface (see film.rs).
static FILM_NO_HUD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl Player {
    /// Filming: puts the camera where the shot says. True if it did.
    fn film_camera(&mut self) -> bool {
        let (Some(f), Screen::Playing(s)) = (&mut self.film, &self.screen) else { return false };
        f.track(&s.view);
        let Some((at, look, fov)) = f.camera(&s.view) else { return false };
        aim(&mut self.camera, at, look);
        if let Some(fov) = fov {
            self.camera.fov_degrees = fov;
        }
        true
    }
}

/// Puts the camera at `at`, looking toward `look`.
fn aim(camera: &mut Camera, at: glam::Vec3, look: glam::Vec3) {
    let d = (look - at).normalize_or(glam::Vec3::X);
    camera.position = at;
    camera.yaw = d.z.atan2(d.x);
    camera.pitch = d.y.clamp(-1.0, 1.0).asin();
}

/// Test hook for filming: the camera's position and the point it looks at,
/// The letter a key is, if it's one games can hear (brixo_runtime::SCRIPT_KEYS).
fn script_key(code: KeyCode) -> Option<&'static str> {
    Some(match code {
        KeyCode::KeyE => "e",
        KeyCode::KeyQ => "q",
        KeyCode::KeyF => "f",
        KeyCode::KeyR => "r",
        KeyCode::KeyG => "g",
        KeyCode::KeyZ => "z",
        KeyCode::KeyX => "x",
        KeyCode::KeyC => "c",
        KeyCode::KeyV => "v",
        KeyCode::KeyB => "b",
        _ => return None,
    })
}

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

/// Right-drag turns the camera; the wheel zooms. (Keys and trackpads: see
/// brixo_client::keyboard_look and trackpad_look.)
fn look_around(ctx: &egui::Context, camera: &mut Camera, follow: &mut FollowCamera, speed: f32) {
    let (dragging, delta, scroll, pinch) =
        ctx.input(|i| (i.pointer.secondary_down(), i.pointer.delta(), i.smooth_scroll_delta, i.zoom_delta()));
    if dragging {
        camera.yaw += delta.x * LOOK_SPEED * speed;
        camera.pitch = (camera.pitch - delta.y * LOOK_SPEED * speed).clamp(-1.5, 1.5);
    }
    follow.zoom(scroll.y);
    // Trackpads: pinch zooms, a two-finger swipe sideways turns.
    trackpad_look(camera, follow, scroll, pinch);
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
        // Bring this copy up to date first; the game opens after (or in the
        // new version).
        if self.updater.is_some() {
            self.screen = Screen::Updating;
            return;
        }
        self.start_from_args();
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: winit::event::DeviceId, event: winit::event::DeviceEvent) {
        // Raw mouse movement turns the camera while the mouse is locked.
        if let winit::event::DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if self.locked && !self.confined {
                let k = LOCKED_LOOK_SPEED * self.settings.camera_speed;
                self.camera.yaw += dx as f32 * k;
                self.camera.pitch = (self.camera.pitch - dy as f32 * k).clamp(-1.5, 1.5);
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
                        let k = LOCKED_LOOK_SPEED * self.settings.camera_speed;
                        self.camera.yaw += dx as f32 * k;
                        self.camera.pitch = (self.camera.pitch - dy as f32 * k).clamp(-1.5, 1.5);
                        let _ = gpu.window.set_cursor_position(winit::dpi::PhysicalPosition::new(cx, cy));
                    }
                }
            }
            WindowEvent::Focused(false) => self.keys.clear(),
            WindowEvent::RedrawRequested => {
                // macOS: Play on the website (see mac_links); joining
                // replaces whatever game is open.
                if let Some(link) = mac_links::take() {
                    if matches!(self.screen, Screen::Updating) {
                        self.pending_link = Some(link);
                    } else if let Some(parsed) = protocol::parse(&link) {
                        self.last_link = Some(link.clone());
                        self.join_with_ticket(parsed);
                    }
                }
                self.poll_update();
                let now = Instant::now();
                let mut dt = (now - self.last_frame).as_secs_f32().min(0.1);
                self.last_frame = now;
                // Filming: the game moves a fixed step per frame.
                if let Some(f) = &self.film {
                    dt = f.dt();
                }
                self.frame(dt);
                if let Some(f) = &mut self.film {
                    if matches!(self.screen, Screen::Playing(_)) {
                        f.advance(dt);
                    }
                }
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
    brixo_client::update::tidy(App::Player);
    let startup = install::on_startup(App::Player, |exe| if cfg!(windows) { protocol::register_exe(exe).map(|_| ()) } else { Ok(()) });
    if startup == install::Startup::Exit {
        return;
    }
    // macOS delivers Play links as Apple Events: start catching them now,
    // so one that launched us isn't missed.
    mac_links::listen();
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
