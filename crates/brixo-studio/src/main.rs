#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
use std::collections::HashSet;
use std::f32::consts::{PI, TAU};
use std::sync::Arc;
use std::time::Instant;

mod editing;

use brixo_core::{Class, Color, DataModel, InstanceId, PartProps, Shape, Vec3 as V};
use brixo_render::{Camera, SceneRenderer};
use brixo_client::install::{self, App};
use brixo_client::{keyboard_look, trackpad_look, CameraKeys};
use brixo_client::{draw_beacons, projector, Audio, ChatLog, Smoother, draw_gui, draw_hotbar, hotbar_key, movement_input, FollowCamera, GuiEvents, Held};
use brixo_runtime::{Game, LogLine, PlayerInput};
use glam::{EulerRot, Mat4, Quat, Vec3};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

/// Turning with the mouse locked (Play: first person, shift lock), per pixel.
const LOCKED_LOOK_SPEED: f32 = 0.003;
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
        brixo_client::theme::apply_site(&egui_ctx);
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
    /// Goes up with every edit, undo and redo: the game has changed since
    /// it was saved if this has.
    edits: u64,
}

impl History {
    /// Call *before* changing the model.
    fn checkpoint(&mut self, model: &DataModel) {
        self.edits += 1;
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
                self.edits += 1;
                self.redo.push(std::mem::replace(model, previous));
                true
            }
            None => false,
        }
    }

    fn redo(&mut self, model: &mut DataModel) -> bool {
        match self.redo.pop() {
            Some(next) => {
                self.edits += 1;
                self.undo.push(std::mem::replace(model, next));
                true
            }
            None => false,
        }
    }
}

struct Editor {
    /// A newer Brixo Studio on the website, once the check finds one.
    update: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    update_hidden: bool,
    tool: Tool,
    snap: bool,
    /// How far a move or resize snaps (studs), and a turn (degrees).
    snap_step: f32,
    turn_step: f32,
    /// A box select in progress: where the drag started.
    box_select: Option<egui::Pos2>,
    /// The axis the Line up buttons work along (0 = X, 1 = Y, 2 = Z).
    line_axis: usize,
    drag: Option<Drag>,
    history: History,
    /// True while a Properties edit is in progress (dragging a value,
    /// typing a name), so the whole edit becomes a single undo step.
    prop_session: bool,
    /// The camera that follows the player during Play.
    follow: FollowCamera,
    /// What the game is called when published to the player.
    publish_name: String,
    /// Publishing to the Brixo website: the login form, and the logged-in
    /// connection once you've signed in. (Which website: install::site(),
    /// so BRIXO_SITE points a test build at a local one.)
    web_user: String,
    web_pass: String,
    web_error: String,
    show_web_login: bool,
    web: Option<ureq::Agent>,
    /// How many players Play starts. More than one hosts a local server
    /// and opens a Brixo Player window for each.
    players: u32,
    /// Selected alongside the main selection (Ctrl-click).
    also: Vec<InstanceId>,
    /// What Ctrl+C copied.
    clipboard: Vec<InstanceId>,
    /// What was last copied, as clipboard text (so Paste works in another
    /// game opened in this studio, not just this one).
    clip_text: Option<String>,
    /// The Explorer row being renamed, and the name so far.
    renaming: Option<(InstanceId, String)>,
    /// What the Explorer asked for this frame (applied after it's drawn).
    tree_requests: Vec<TreeRequest>,
    /// Parts being dragged over surfaces in the 3D view.
    surface_drag: Option<Vec<InstanceId>>,
    /// Script autocomplete: where the word starts, the suggestions, which
    /// one is picked, and whether Escape hid them.
    completion: Option<(usize, Vec<&'static str>)>,
    completion_pick: usize,
    completion_hidden: Option<usize>,
    /// While dragging a group: the group's box and every part's start.
    group_drag: Option<(PartProps, Vec<(InstanceId, PartProps)>)>,
    /// During Play: GUI clicks, hotbar picks and tool swings from the
    /// viewport, handed to the game after the UI is built.
    /// Play mode's clicks: GUI and hotbar, and a tool swing aimed at a
    /// spot in the world.
    play_input: Option<(GuiEvents, Option<brixo_core::Vec3>)>,
    /// A Sound to play once (its Preview button).
    preview: Option<InstanceId>,
    /// An action for build_ui to take this frame (from the Explorer's
    /// right-click menu).
    queued_action: Option<Action>,
    /// The file this game was opened from or saved to (None: never saved),
    /// and the history's edit count when it was, to tell if it's changed.
    file: Option<std::path::PathBuf>,
    saved_edits: u64,
    /// Waiting on "save your changes first?" before doing this.
    pending: Option<Pending>,
    /// Studio should close (after that question, if it was asked).
    quit: bool,
    /// Scripts open as tabs beside the World tab, and which tab is showing
    /// (None: the World).
    tabs: Vec<InstanceId>,
    tab: Option<InstanceId>,
    /// Put the cursor in the code (a tab was just opened).
    focus_code: bool,
    /// The local game's player (None when hosting a server: the studio
    /// only watches then, so it doesn't show anyone's personal GUI).
    local_player: Option<InstanceId>,
    chat: ChatLog,
    started: std::time::Instant,
    /// Play, as in Brixo Player: shift lock (Shift), the chat box (/ or
    /// Enter), and Esc freeing the mouse from its lock until you click the
    /// 3D view again (so Stop and the panels stay reachable).
    shift_lock: bool,
    chat_open: bool,
    /// The leaderboard is unfolded during Play (Tab folds it).
    board_open: bool,
    chat_text: String,
    mouse_freed: bool,
    /// The mouse is locked right now (tools then aim at the middle).
    mouse_locked: bool,
    /// What was typed in chat, to send once the UI's done.
    said: Option<String>,
    /// Saved player data (save/load) while testing: kept from one Play to
    /// the next until Studio closes or another game is opened.
    saves: Arc<brixo_runtime::MemoryStore>,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            tool: Tool::Move,
            snap: true,
            snap_step: 1.0,
            turn_step: 15.0,
            box_select: None,
            line_axis: 0,
            drag: None,
            history: History::default(),
            prop_session: false,
            follow: FollowCamera::default(),
            publish_name: "My Game".to_string(),
            update: install::check_for_update(App::Studio),
            update_hidden: false,
            web_user: String::new(),
            web_pass: String::new(),
            web_error: String::new(),
            show_web_login: false,
            web: None,
            players: 1,
            also: Vec::new(),
            clipboard: Vec::new(),
            clip_text: None,
            renaming: None,
            tree_requests: Vec::new(),
            surface_drag: None,
            completion: None,
            completion_pick: 0,
            completion_hidden: None,
            group_drag: None,
            play_input: None,
            preview: None,
            queued_action: None,
            file: None,
            saved_edits: 0,
            pending: None,
            quit: false,
            tabs: Vec::new(),
            tab: None,
            focus_code: false,
            local_player: None,
            chat: ChatLog::default(),
            started: std::time::Instant::now(),
            shift_lock: false,
            chat_open: false,
            board_open: true,
            chat_text: String::new(),
            mouse_freed: false,
            mouse_locked: false,
            said: None,
            saves: Arc::new(brixo_runtime::MemoryStore::default()),
        }
    }
}

// --- app -------------------------------------------------------------------

/// A multiplayer test running from the studio.
struct Hosted {
    server: brixo_server::ServerHandle,
    windows: Vec<std::process::Child>,
}

impl Drop for Hosted {
    fn drop(&mut self) {
        for w in &mut self.windows {
            let _ = w.kill();
        }
    }
}

/// "A new Brixo Studio is out" along the top, until dismissed.
fn update_notice(ctx: &egui::Context, editor: &mut Editor) {
    let latest = editor.update.lock().unwrap().clone();
    let Some(latest) = latest else { return };
    if editor.update_hidden {
        return;
    }
    egui::Area::new(egui::Id::new("update notice"))
        .anchor(egui::Align2::CENTER_TOP, [0.0, 40.0])
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("A new {} is out ({latest}).", App::Studio.title()));
                    if ui.button("Get it").clicked() {
                        install::open_url(&format!("{}/download", install::site()));
                    }
                    if ui.small_button("x").clicked() {
                        editor.update_hidden = true;
                    }
                });
            });
        });
}

/// The Brixo Player for test windows: your own build next to Studio, or
/// the installed one.
fn find_player() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let built = exe.with_file_name(format!("brixo-player{}", std::env::consts::EXE_SUFFIX));
    if built.exists() {
        return Ok(built);
    }
    install::installed_places(App::Player)
        .into_iter()
        .find(|p| p.exists())
        .ok_or_else(|| format!("Testing with players needs Brixo Player: get it at {}/download", install::site()))
}

/// Starts a server on the scene and opens `players` player windows on it.
fn host(model: &DataModel, players: u32) -> Result<Hosted, String> {
    let player = find_player()?;
    let server = brixo_server::start(model.clone(), 0).map_err(|e| format!("couldn't start a server: {e}"))?;
    let mut hosted = Hosted { server, windows: Vec::new() };
    for _ in 0..players {
        let child = std::process::Command::new(&player)
            .args(["--join", &hosted.server.local_addr(), "--name", "Player"])
            .spawn()
            .map_err(|e| format!("couldn't open a player window: {e}"))?;
        hosted.windows.push(child);
    }
    Ok(hosted)
}

struct Studio {
    gpu: Option<Gpu>,
    model: DataModel,
    camera: Camera,
    selection: Option<InstanceId>,
    status: String,
    editor: Editor,
    /// The running game, while Play is on. It works on a copy of `model`.
    game: Option<Game>,
    /// A local multiplayer test: a server plus a player window per player.
    hosted: Option<Hosted>,
    audio: Audio,
    /// Set when Play stops, so the music stops too.
    audio_stop: bool,
    /// During local Play: the world as drawn, smoothed between physics
    /// steps. The live world underneath is what the panels edit.
    smoother: Smoother,
    play_view: Option<DataModel>,
    output: Vec<LogLine>,
    /// The studio camera, put back when Play stops.
    saved_camera: Option<(Vec3, f32, f32)>,
    keys: HashSet<KeyCode>,
    last_frame: Instant,
    /// Where the OS couldn't truly lock the mouse (X11) it's confined
    /// instead, and put back in the middle after each move.
    confined: bool,
}

impl Studio {
    fn new() -> Self {
        // `brixo-studio game.brixo` (or double-clicking one) opens that
        // game; otherwise a new one.
        let opened = std::env::args().nth(1).filter(|a| !a.starts_with("--")).map(|path| (DataModel::load_file(&path), path));
        let mut file = None;
        let (model, opened_status) = match opened {
            Some((Ok(model), path)) => {
                file = Some(std::path::PathBuf::from(&path));
                (model, Some(format!("Opened {}", name_of(std::path::Path::new(&path)))))
            }
            Some((Err(e), path)) => (start_scene(), Some(format!("Couldn't open {path}: {e}"))),
            None => (start_scene(), None),
        };
        let mut studio = Self {
            gpu: None,
            model,
            camera: start_camera(),
            selection: None,
            status: "Ready".to_string(),
            editor: Editor::default(),
            game: None,
            hosted: None,
            audio: Audio::new(),
            audio_stop: false,
            smoother: Smoother::default(),
            play_view: None,
            output: Vec::new(),
            saved_camera: None,
            keys: HashSet::new(),
            last_frame: Instant::now(),
            confined: false,
        };
        if let Some(status) = opened_status {
            studio.status = status;
        }
        if let Some(path) = file {
            studio.editor.publish_name = name_of(&path);
            studio.editor.file = Some(path);
        }
        // For screenshots (the website guide's): BRIXO_STUDIO_CAMERA="x y z
        // lx ly lz" starts the camera at x,y,z looking at lx,ly,lz.
        if let Ok(spec) = std::env::var("BRIXO_STUDIO_CAMERA") {
            let n: Vec<f32> = spec.split_whitespace().filter_map(|s| s.parse().ok()).collect();
            if let [x, y, z, lx, ly, lz] = n[..] {
                look_from(&mut studio.camera, Vec3::new(x, y, z), Vec3::new(lx, ly, lz));
            }
        }
        studio
    }

    /// Whether you're driving a kart in Play.
    fn driving(&self) -> bool {
        self.game.as_ref().is_some_and(|g| g.player_id().and_then(|me| g.world().player(me).and_then(|p| p.kart)).is_some())
    }

    fn player_input(&self) -> PlayerInput {
        let chatting = self.editor.chat_open;
        let held = |k| !chatting && self.keys.contains(&k);
        if self.driving() {
            return brixo_client::kart_input(Held {
                forward: held(KeyCode::KeyW) || held(KeyCode::ArrowUp),
                back: held(KeyCode::KeyS) || held(KeyCode::ArrowDown),
                left: held(KeyCode::KeyA) || held(KeyCode::ArrowLeft),
                right: held(KeyCode::KeyD) || held(KeyCode::ArrowRight),
                jump: held(KeyCode::Space),
            });
        }
        movement_input(
            &self.camera,
            Held {
                // Up/Down walk too, like Roblox (Left/Right turn the camera).
                forward: held(KeyCode::KeyW) || held(KeyCode::ArrowUp),
                back: held(KeyCode::KeyS) || held(KeyCode::ArrowDown),
                left: held(KeyCode::KeyA),
                right: held(KeyCode::KeyD),
                jump: held(KeyCode::Space),
            },
        )
    }

    /// Play's keys, as in Brixo Player: Shift for shift lock, / or Enter
    /// to chat, Esc to close the chat or free the mouse.
    fn play_key(&mut self, code: KeyCode, consumed: bool) {
        let e = &mut self.editor;
        if e.chat_open {
            if code == KeyCode::Escape {
                e.chat_open = false;
                e.chat_text.clear();
            }
            return;
        }
        if consumed {
            return;
        }
        match code {
            KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                e.shift_lock = !e.shift_lock;
                e.mouse_freed = false;
                self.audio.sound("click");
            }
            KeyCode::Slash | KeyCode::Enter => {
                e.chat_open = true;
                self.keys.clear();
            }
            KeyCode::Escape => e.mouse_freed = true,
            KeyCode::Tab => e.board_open = !e.board_open,
            other => {
                // Keys games hear (`on key(player, key)`).
                if let (Some(key), Some(game)) = (script_key(other), self.game.as_mut()) {
                    if let Some(me) = game.player_id() {
                        game.key(me, key);
                    }
                }
            }
        }
    }

    /// First person and shift lock lock the mouse to the window during
    /// Play, and it turns the camera. Chatting, Esc, a script tab or a
    /// question box free it.
    fn update_mouse_lock(&mut self) {
        let first_person = match (self.game.as_ref(), self.play_view.as_ref()) {
            (Some(g), Some(v)) => self.editor.follow.first_person(v, g.player_id()),
            _ => false,
        };
        let e = &self.editor;
        let want = self.game.is_some()
            && (first_person || e.shift_lock)
            && !e.chat_open
            && !e.mouse_freed
            && e.tab.is_none()
            && e.pending.is_none()
            && !e.show_web_login;
        if want == self.editor.mouse_locked {
            return;
        }
        let Some(gpu) = self.gpu.as_ref() else { return };
        use winit::window::CursorGrabMode;
        let grab = if want {
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
        if grab.is_ok() || !want {
            gpu.window.set_cursor_visible(!want);
            self.editor.mouse_locked = want;
            if want && self.confined {
                let size = gpu.window.inner_size();
                let _ = gpu.window.set_cursor_position(winit::dpi::PhysicalPosition::new(size.width / 2, size.height / 2));
            }
        }
    }

    fn move_camera(&mut self, dt: f32) {
        let held = |k| self.keys.contains(&k);
        let camera_keys = CameraKeys {
            turn_left: held(KeyCode::ArrowLeft),
            turn_right: held(KeyCode::ArrowRight),
            tilt_up: held(KeyCode::PageUp),
            tilt_down: held(KeyCode::PageDown),
            zoom_in: held(KeyCode::KeyI),
            zoom_out: held(KeyCode::KeyO),
        };
        // During Play the keys drive the player and the camera follows it;
        // the camera keys turn and zoom it, as in Brixo Player.
        if self.game.is_some() {
            if !self.driving() {
                keyboard_look(&mut self.camera, &mut self.editor.follow, camera_keys, dt);
            }
            return;
        }
        // Building: Left/Right turn and Page Up/Down tilt here too, for
        // trackpads; Up/Down fly forward and back like W/S.
        let axis = |plus: bool, minus: bool| (plus as i8 - minus as i8) as f32;
        self.camera.yaw += axis(camera_keys.turn_right, camera_keys.turn_left) * brixo_client::play::KEY_TURN_SPEED * dt;
        self.camera.pitch = (self.camera.pitch + axis(camera_keys.tilt_up, camera_keys.tilt_down) * brixo_client::play::KEY_TILT_SPEED * dt)
            .clamp(-1.55, 1.55);
        let speed = if self.keys.contains(&KeyCode::ControlLeft) {
            40.0
        } else {
            12.0
        } * dt;

        let forward = self.camera.forward();
        let right = self.camera.right();
        let mut delta = Vec3::ZERO;

        if self.keys.contains(&KeyCode::KeyW) || self.keys.contains(&KeyCode::ArrowUp) {
            delta += forward;
        }
        if self.keys.contains(&KeyCode::KeyS) || self.keys.contains(&KeyCode::ArrowDown) {
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

    /// An audio file dropped on the window: checked, then added as a Sound.
    /// A .brixo game file dropped on the window (like a sample game from
    /// the website's guide): opens it in place of what's there. Undo brings
    /// the old one back.
    fn open_dropped_game(&mut self, path: &std::path::Path) {
        if self.game.is_some() || self.hosted.is_some() {
            self.status = "Stop the game first, then drop the file in again".into();
            return;
        }
        let what = Pending::OpenFile(path.to_path_buf());
        if self.editor.unsaved() {
            self.editor.pending = Some(what);
        } else {
            carry_out(what, &mut self.model, &mut self.selection, &mut self.camera, &mut self.editor, &mut self.status);
        }
    }

    /// The window's title: the game's name, and * while it has unsaved
    /// changes.
    fn update_title(&mut self) {
        if brixo_client::filming::window_title().is_some() {
            return;
        }
        let title = format!("{}{} - Brixo Studio", self.editor.doc_name(), if self.editor.unsaved() { " *" } else { "" });
        if let Some(gpu) = self.gpu.as_ref() {
            if gpu.window.title() != title {
                gpu.window.set_title(&title);
            }
        }
    }

    fn import_audio(&mut self, path: &std::path::Path) {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if !["mp3", "wav", "ogg"].contains(&ext.as_str()) {
            self.status = format!("Brixo can add mp3, wav and ogg files as sounds, not .{ext}");
            return;
        }
        if self.game.is_some() || self.hosted.is_some() {
            self.status = "Stop the game first, then drop the sound in again".into();
            return;
        }
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                self.status = format!("Couldn't read that file: {e}");
                return;
            }
        };
        if bytes.len() > 10_000_000 {
            self.status = "That file's over 10 MB: try a shorter clip".into();
            return;
        }
        if !brixo_client::sound::decodes(&bytes) {
            self.status = format!("That file doesn't play. Is it really a .{ext}?");
            return;
        }
        self.editor.history.checkpoint(&self.model);
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Sound").to_string();
        let root = self.model.root();
        if let Some(id) = self.model.create(Class::Sound, &name, root) {
            *self.model.sound_mut(id).unwrap() = brixo_core::SoundProps::from_bytes(&ext, &bytes);
            self.selection = Some(id);
            self.status = format!("Added the sound \"{name}\". Play it from a script: play_sound(find(\"{name}\"))");
        }
    }

    fn frame(&mut self, dt: f32) {
        if let Some(id) = self.editor.preview.take() {
            if let Some((bytes, volume)) = brixo_client::world_sound(&self.model, id.raw()) {
                self.audio.play_bytes(bytes, volume);
            }
        }
        if std::mem::take(&mut self.audio_stop) {
            self.audio.stop_music();
        }
        // Advance the running game before drawing it.
        let input = self.player_input();
        self.editor.local_player = self.game.as_ref().and_then(|g| g.player_id());
        let mut first_person_player = None;
        if let Some(game) = self.game.as_mut() {
            // Shift lock: face where the camera looks, from over the shoulder.
            let driving = game.player_id().and_then(|me| game.world().player(me).and_then(|p| p.kart)).is_some();
            let shift = self.editor.shift_lock && !driving;
            self.editor.follow.shoulder = shift;
            game.set_facing(shift.then(|| brixo_client::shift_lock_yaw(&self.camera)));
            game.set_input(input);
            game.step(dt as f64);
            self.output.extend(game.take_log());
            let me = game.player_id();
            for cue in game.take_sounds().into_iter().filter_map(|e| e.for_player(me)) {
                let world = game.world();
                self.audio.cue_with(&cue, &|id| brixo_client::world_sound(&world, id));
            }
            for (from, name, text) in game.take_chat() {
                self.editor.chat.push(from, name, text);
            }
            trim_output(&mut self.output);
            let mut view = self.smoother.view(&game.world());
            brixo_client::smooth::hold_tools(&mut view, &game.world());
            self.audio.engine(brixo_client::kart_fx::engine_pitch(&view, game.player_id()));
            first_person_player = self.editor.follow.update(&mut self.camera, &view, game.player_id());
            self.audio.listen(self.camera.position, self.camera.right(), &view);
            self.play_view = Some(view);
        } else if self.play_view.take().is_some() {
            self.smoother.reset();
            self.audio.engine(None);
        }

        let Studio {
            gpu,
            model,
            camera,
            selection,
            status,
            editor,
            game,
            hosted,
            audio_stop,
            play_view,
            output,
            saved_camera,
            ..
        } = self;
        let Some(gpu) = gpu.as_mut() else { return };
        let playing = game.is_some() || hosted.is_some();
        let play_time = game.as_ref().map(|g| g.time());
        if let Some(h) = hosted.as_ref() {
            output.extend(h.server.take_log());
            trim_output(output);
            *status = format!("Testing with {} of {} player windows connected", h.server.player_count(), h.windows.len());
        }
        let mut toggle_play = false;

        {
            // While playing, show and click the live game world, not the scene.
            // With a local server, that's the server's world: fly around and
            // watch everyone. We draw a copy, taken in one quick lock: holding
            // the server's world for a whole frame would stall its ticks.
            let mut server_view = hosted.as_ref().map(|h| h.server.world().clone());
            let mut world_guard = game.as_ref().map(|g| g.world());
            let scene: &mut DataModel = match (world_guard.as_mut(), server_view.as_mut()) {
                (Some(world), _) => world,
                (None, Some(view)) => view,
                (None, None) => model,
            };

            // --- build the UI (this also handles viewport mouse input) ---
            let raw_input = gpu.egui_state.take_egui_input(&gpu.window);
            let full_output = gpu.egui_ctx.run(raw_input, |ctx| {
                toggle_play |= build_ui(
                    ctx, scene, selection, status, camera, editor, output, playing, play_time,
                );
                update_notice(ctx, editor);
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

            // Right-drag turned the camera in the interface pass, after the
            // follow camera was placed: put it back on its orbit for the new
            // angle, or the player slides on screen as you turn (jitter).
            if let (Some(view), Some(g)) = (play_view.as_ref(), game.as_ref()) {
                first_person_player = editor.follow.update(camera, view, g.player_id());
            }
            gpu.scene.hidden_player = first_person_player;
            gpu.scene.editing = !playing;
            gpu.scene.time = editor.started.elapsed().as_secs_f32();
            // Highlight everything selected, and every part inside it.
            let highlight: Vec<InstanceId> = if playing {
                Vec::new()
            } else {
                let items = selected_items(scene, *selection, &editor.also);
                let mut h = moving_parts(scene, &items);
                h.extend(items);
                h
            };
            gpu.scene.render(
                &gpu.device,
                &gpu.queue,
                &mut encoder,
                &view,
                play_view.as_ref().unwrap_or(&*scene),
                camera,
                &highlight,
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

        if let (Some(text), Some(g)) = (editor.said.take(), game.as_mut()) {
            if let Some(me) = g.player_id() {
                g.chat(me, &text);
            }
        }
        if let (Some((events, swing)), Some(g)) = (editor.play_input.take(), game.as_mut()) {
            if let Some(me) = g.player_id() {
                if let Some(b) = events.clicked {
                    g.click(me, b);
                }
                if let Some(slot) = events.hotbar {
                    g.equip(me, Some(slot));
                }
                if let Some(aim) = swing {
                    g.activate_at(me, Some(aim));
                }
            }
        }

        if toggle_play {
            // Play and Stop show the World.
            editor.tab = None;
            editor.shift_lock = false;
            editor.chat_open = false;
            editor.chat_text.clear();
            editor.mouse_freed = false;
            editor.drag = None;
            editor.prop_session = false;
            if let Some(h) = hosted.take() {
                drop(h); // closes the player windows, then stops the server
                output.push(system_line("Stopped the server"));
                *status = "Stopped".to_string();
                if selection.and_then(|id| model.get(id)).is_none() {
                    *selection = None;
                }
            } else if editor.players > 1 {
                match host(model, editor.players) {
                    Ok(h) => {
                        output.push(system_line(&format!(
                            "Hosting on port {} with {} player windows",
                            h.server.port(),
                            editor.players
                        )));
                        *hosted = Some(h);
                    }
                    Err(e) => *status = e,
                }
            } else if game.take().is_some() {
                *audio_stop = true;
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
                let started = Game::start_with_store(model.clone(), editor.saves.clone());
                output.extend(started.take_log());
                trim_output(output);
                *game = Some(started);
                *status = "Playing".to_string();
            }
        }
    }
}

/// With several things selected: line them up along an axis, or space
/// them out evenly.
fn line_up_ui(ui: &mut egui::Ui, model: &mut DataModel, selection: Option<InstanceId>, editor: &mut Editor) {
    use editing::Align;
    let items = selected_items(model, selection, &editor.also);
    let boxes: Vec<(InstanceId, Vec3, Vec3)> = items
        .iter()
        .filter_map(|id| bounds(model, &model.parts_under(*id)).map(|b| (*id, to_glam(b.position), to_glam(b.size) / 2.0)))
        .collect();
    if boxes.len() < 2 {
        return;
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Line up").strong().color(brixo_client::theme::site::NAVY));
    let mut todo: Option<Vec<f32>> = None;
    let only: Vec<(Vec3, Vec3)> = boxes.iter().map(|(_, c, h)| (*c, *h)).collect();
    let axis = editor.line_axis;
    ui.horizontal(|ui| {
        ui.label("Along");
        for (i, name) in ["X", "Y", "Z"].into_iter().enumerate() {
            ui.selectable_value(&mut editor.line_axis, i, name);
        }
    });
    ui.horizontal(|ui| {
        let low = ["Left", "Bottom", "Back"][axis];
        let high = ["Right", "Top", "Front"][axis];
        for (label, how, tip) in [(low, Align::Min, "Line up their low sides"), ("Middle", Align::Middle, "Line up their middles"), (high, Align::Max, "Line up their high sides")] {
            if ui.button(label).on_hover_text(tip).clicked() {
                todo = Some(editing::align(&only, axis, how));
            }
        }
        if ui
            .add_enabled(boxes.len() >= 3, egui::Button::new("Space evenly"))
            .on_hover_text("Same gap between each, ends staying put (3 or more)")
            .clicked()
        {
            todo = Some(editing::space_evenly(&only, axis));
        }
    });
    if let Some(moves) = todo {
        editor.history.checkpoint(model);
        for ((id, _, _), d) in boxes.iter().zip(moves) {
            if d.abs() > 1e-5 {
                let mut by = Vec3::ZERO;
                by[axis] = d;
                shift(model, &[*id], by);
            }
        }
        editor.prop_session = false;
    }
}

/// The Workspace's Lighting section. True if anything changed.
fn lighting_ui(ui: &mut egui::Ui, l: &mut brixo_core::Lighting) -> bool {
    use brixo_client::theme::site;
    let mut changed = false;
    ui.label(egui::RichText::new("Lighting").strong().color(site::NAVY));
    ui.horizontal(|ui| {
        ui.label("Time of day").on_hover_text("0 to 24 hours: 6 is sunrise, 12 noon, 18 sunset, night after. Moves the sun and colours the sky.");
        changed |= ui.add(egui::Slider::new(&mut l.time_of_day, 0.0..=24.0).custom_formatter(|v, _| {
            let h = v.floor() as u32 % 24;
            let m = ((v - v.floor()) * 60.0).round() as u32 % 60;
            let (hh, ampm) = match h { 0 => (12, "am"), 1..=11 => (h, "am"), 12 => (12, "pm"), _ => (h - 12, "pm") };
            format!("{hh}:{m:02} {ampm}")
        })).changed();
    });
    ui.horizontal(|ui| {
        ui.label("Brightness").on_hover_text("1 is normal: lower is gloomier, higher is dazzling");
        changed |= ui.add(egui::Slider::new(&mut l.brightness, 0.0..=3.0).max_decimals(2)).changed();
    });
    ui.horizontal(|ui| {
        let mut custom = l.sky_color.is_some();
        if ui.checkbox(&mut custom, "Sky color").on_hover_text("The sky overhead by day, instead of blue").changed() {
            l.sky_color = custom.then(|| Color::new(20, 85, 200));
            changed = true;
        }
        if let Some(c) = l.sky_color.as_mut() {
            let mut rgb = [c.r, c.g, c.b];
            if ui.color_edit_button_srgb(&mut rgb).changed() {
                *c = Color::new(rgb[0], rgb[1], rgb[2]);
                changed = true;
            }
        }
    });
    ui.separator();
    ui.horizontal(|ui| {
        let mut on = l.fog_end > 0.0;
        if ui.checkbox(&mut on, "Fog").on_hover_text("Things fade into the fog colour, and are gone by the end distance").changed() {
            (l.fog_start, l.fog_end) = if on { (30.0, 200.0) } else { (0.0, 0.0) };
            changed = true;
        }
        if on {
            let mut rgb = [l.fog_color.r, l.fog_color.g, l.fog_color.b];
            if ui.color_edit_button_srgb(&mut rgb).changed() {
                l.fog_color = Color::new(rgb[0], rgb[1], rgb[2]);
                changed = true;
            }
        }
    });
    if l.fog_end > 0.0 {
        ui.horizontal(|ui| {
            ui.label("Starts");
            changed |= ui.add(egui::DragValue::new(&mut l.fog_start).speed(1.0).range(0.0..=5000.0).suffix(" studs")).changed();
            ui.label("gone by");
            changed |= ui.add(egui::DragValue::new(&mut l.fog_end).speed(1.0).range(1.0..=5000.0).suffix(" studs")).changed();
        });
        l.fog_start = l.fog_start.min(l.fog_end);
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Scripts change these too: find(\"Workspace\").time_of_day = 19").small().color(site::DIM));
    changed
}

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

fn stud_label(step: f32) -> String {
    match step {
        s if s == 0.25 => "¼ stud".into(),
        s if s == 0.5 => "½ stud".into(),
        s if s == 1.0 => "1 stud".into(),
        s => format!("{s} studs"),
    }
}

fn side_label(side: brixo_core::Side) -> &'static str {
    use brixo_core::Side::*;
    match side {
        Middle => "Middle",
        Left => "Left side (-X)",
        Right => "Right side (+X)",
        Top => "Top (+Y)",
        Bottom => "Bottom (-Y)",
        Front => "Front (+Z)",
        Back => "Back (-Z)",
    }
}

fn shape_label(shape: Shape) -> &'static str {
    match shape {
        Shape::Block => "Part",
        Shape::Wedge => "Wedge",
        Shape::Cylinder => "Cylinder",
        Shape::Ball => "Ball",
    }
}

fn plural(n: usize, word: &str) -> String {
    match (n, word) {
        (1, _) => format!("1 {word}"),
        (_, "copy") => format!("{n} copies"),
        _ => format!("{n} {word}s"),
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

/// Something the Explorer asked for.
enum TreeRequest {
    Rename(InstanceId, String),
    /// Move the first into the second.
    Move(InstanceId, InstanceId),
    /// Run a command on this (the right-click menu).
    Menu(InstanceId, Action),
}

enum Action {
    Publish,
    WebLogin,
    AddGui(Class),
    AddTool,
    AddShape(Shape),
    Copy,
    Paste,
    Duplicate,
    Group,
    Ungroup,
    Focus,
    AddSpawn,
    AddScript,
    AddFolder,
    Delete,
    /// File menu. The ones that replace the game ask about unsaved changes
    /// first (see Pending).
    New,
    Open,
    OpenSample(usize),
    Save,
    SaveAs,
    Undo,
    Redo,
}

/// Something that replaces (or closes) the game, waiting on "save your
/// changes first?".
#[derive(Clone, Debug)]
enum Pending {
    New,
    Open,
    OpenSample(usize),
    OpenFile(std::path::PathBuf),
    Quit,
}

/// The sample games, for File > Open a sample.
const SAMPLES: [(&str, fn() -> DataModel); 5] = [
    ("Coin Tycoon", brixo_samples::coin_tycoon),
    ("Flagfall", brixo_samples::flagfall),
    ("Spire Wars", brixo_samples::spire_wars),
    ("Gear Range", brixo_samples::gears::gear_range),
    ("Brickport Speedway", brixo_samples::speedway::brickport_speedway),
];

/// A new game: a big grey baseplate and a spawn pad, ready to build on.
fn start_scene() -> DataModel {
    let mut dm = DataModel::new();
    let root = dm.root();
    let baseplate = dm.create(Class::Part, "Baseplate", root).unwrap();
    {
        let p = dm.part_mut(baseplate).unwrap();
        p.size = V::new(256.0, 1.0, 256.0);
        p.position = V::new(0.0, -0.5, 0.0);
        p.color = Color::new(163, 162, 165);
    }
    let spawn = dm.create(Class::SpawnLocation, "SpawnLocation", root).unwrap();
    dm.part_mut(spawn).unwrap().position = V::new(0.0, 0.5, 0.0);
    dm
}

/// Points `camera` from `from` at `at`.
fn look_from(camera: &mut Camera, from: Vec3, at: Vec3) {
    let d = (at - from).normalize_or_zero();
    camera.position = from;
    camera.yaw = d.z.atan2(d.x);
    camera.pitch = d.y.clamp(-1.0, 1.0).asin();
}

/// Where the camera starts: back from the spawn, looking down at it.
fn start_camera() -> Camera {
    let mut camera = Camera::new();
    look_from(&mut camera, Vec3::new(-14.0, 14.0, -22.0), Vec3::new(0.0, 1.0, 0.0));
    camera
}

/// The game's name from its file: "Lava Escape.brixo" is "Lava Escape".
fn name_of(path: &std::path::Path) -> String {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or("Game").to_string()
}

/// Where the Open and Save As boxes start: "My Games" in the Brixo folder
/// in your home folder (made if it isn't there). Not Brixo/games: that's
/// Brixo Player's own library.
fn games_folder() -> Option<std::path::PathBuf> {
    let dir = brixo_client::games_dir().parent()?.join("My Games");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

impl Editor {
    fn unsaved(&self) -> bool {
        self.history.edits != self.saved_edits
    }

    /// The game's name for the title bar and questions.
    fn doc_name(&self) -> String {
        self.file.as_deref().map(name_of).unwrap_or_else(|| self.publish_name.clone())
    }
}

/// Swaps in another game: a fresh history, no tabs, no selection.
fn replace_game(
    model: &mut DataModel,
    selection: &mut Option<InstanceId>,
    camera: &mut Camera,
    editor: &mut Editor,
    new: DataModel,
    file: Option<std::path::PathBuf>,
    name: String,
) {
    *model = new;
    *selection = None;
    *camera = start_camera();
    editor.history = History::default();
    editor.saved_edits = 0;
    editor.also.clear();
    editor.drag = None;
    editor.tabs.clear();
    editor.tab = None;
    editor.renaming = None;
    editor.file = file;
    editor.publish_name = name;
    // Another game: its test save data starts empty.
    editor.saves = Arc::new(brixo_runtime::MemoryStore::default());
}

/// Saves to `path` (adding .brixo if it's missing). Gives back the status.
fn save_to(model: &DataModel, editor: &mut Editor, mut path: std::path::PathBuf) -> Result<String, String> {
    if path.extension().is_none_or(|e| !e.eq_ignore_ascii_case("brixo")) {
        let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
        name.push(".brixo");
        path.set_file_name(name);
    }
    model.save_file(&path.to_string_lossy()).map_err(|e| format!("Couldn't save: {e}"))?;
    editor.saved_edits = editor.history.edits;
    editor.publish_name = name_of(&path);
    let status = format!("Saved {}", path.display());
    editor.file = Some(path);
    Ok(status)
}

/// Save: to the game's file, or ask where (Save As) the first time. False
/// if it wasn't saved (cancelled, or it failed).
fn save(model: &DataModel, editor: &mut Editor, status: &mut String, ask: bool) -> bool {
    let path = match (&editor.file, ask) {
        (Some(p), false) => Some(p.clone()),
        _ => {
            let mut dialog = rfd::FileDialog::new()
                .set_title("Save your Brixo game")
                .add_filter("Brixo game", &["brixo"])
                .set_file_name(format!("{}.brixo", editor.doc_name()));
            if let Some(dir) = editor.file.as_ref().and_then(|f| f.parent().map(|p| p.to_path_buf())).or_else(games_folder) {
                dialog = dialog.set_directory(dir);
            }
            dialog.save_file()
        }
    };
    let Some(path) = path else { return false };
    match save_to(model, editor, path) {
        Ok(s) => {
            *status = s;
            true
        }
        Err(e) => {
            *status = e;
            false
        }
    }
}

/// Does what was waiting (the changes were saved, or you said not to).
fn carry_out(
    what: Pending,
    model: &mut DataModel,
    selection: &mut Option<InstanceId>,
    camera: &mut Camera,
    editor: &mut Editor,
    status: &mut String,
) {
    match what {
        Pending::New => {
            replace_game(model, selection, camera, editor, start_scene(), None, "My Game".into());
            *status = "New game".into();
        }
        Pending::Open => {
            let mut dialog = rfd::FileDialog::new().set_title("Open a Brixo game").add_filter("Brixo game", &["brixo"]);
            if let Some(dir) = games_folder() {
                dialog = dialog.set_directory(dir);
            }
            if let Some(path) = dialog.pick_file() {
                carry_out(Pending::OpenFile(path), model, selection, camera, editor, status);
            }
        }
        Pending::OpenFile(path) => match DataModel::load_file(&path.to_string_lossy()) {
            Ok(loaded) => {
                let name = name_of(&path);
                *status = format!("Opened {name}");
                replace_game(model, selection, camera, editor, loaded, Some(path), name);
            }
            Err(e) => *status = format!("Couldn't open that game: {e}"),
        },
        Pending::OpenSample(i) => {
            let (name, make) = SAMPLES[i];
            // Not saved anywhere yet: Save asks where.
            replace_game(model, selection, camera, editor, make(), None, name.into());
            *status = format!("Opened the {name} sample. Save it to keep your changes");
        }
        Pending::Quit => editor.quit = true,
    }
}

/// "Save your changes first?" while something is waiting on it.
fn unsaved_question(
    ctx: &egui::Context,
    model: &mut DataModel,
    selection: &mut Option<InstanceId>,
    camera: &mut Camera,
    editor: &mut Editor,
    status: &mut String,
) {
    let Some(what) = editor.pending.clone() else { return };
    let mut choice = None;
    egui::Modal::new(egui::Id::new("unsaved")).show(ctx, |ui| {
        ui.set_width(340.0);
        brixo_client::theme::title_bar(ui, "Unsaved changes", |_| {});
        egui::Frame::NONE.inner_margin(12).show(ui, |ui| {
            ui.label(format!("Save your changes to \"{}\" first?", editor.doc_name()));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                use brixo_client::theme::{gloss_button, Gloss};
                if gloss_button(ui, "Save", Gloss::Green).clicked() {
                    choice = Some(0);
                }
                if gloss_button(ui, "Don't save", Gloss::Gray).clicked() {
                    choice = Some(1);
                }
                if gloss_button(ui, "Cancel", Gloss::Gray).clicked() {
                    choice = Some(2);
                }
            });
        });
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        choice = Some(2);
    }
    match choice {
        Some(0) => {
            editor.pending = None;
            if save(model, editor, status, false) {
                carry_out(what, model, selection, camera, editor, status);
            }
        }
        Some(1) => {
            editor.pending = None;
            carry_out(what, model, selection, camera, editor, status);
        }
        Some(_) => editor.pending = None,
        None => {}
    }
}

/// Returns true if Play or Stop was pressed.
#[allow(clippy::too_many_arguments)]
/// Uploads the game to the website you're logged in to.
fn publish_to_web(editor: &mut Editor, model: &DataModel) -> String {
    let Some(agent) = editor.web.clone() else { return "Log in to publish".to_string() };
    let data = match model.to_json() {
        Ok(d) => d,
        Err(e) => return format!("Publish failed: {e}"),
    };
    let url = format!("{}/api/games", install::site().trim_end_matches('/'));
    match agent.post(&url).send_json(serde_json::json!({ "name": editor.publish_name, "data": data })) {
        Ok(_) => format!("Published \"{}\" to the Brixo website", editor.publish_name),
        Err(ureq::Error::Status(401, _)) => {
            editor.web = None; // logged out on the website: log in again
            editor.show_web_login = true;
            "Please log in again to publish".to_string()
        }
        Err(e) => format!("Publish failed: {}", web_error(e)),
    }
}

/// The website's own error message if it gave one, otherwise what went wrong.
fn web_error(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(_, r) => r
            .into_json::<serde_json::Value>()
            .ok()
            .and_then(|v| v["error"].as_str().map(str::to_string))
            .unwrap_or_else(|| "the website said no".to_string()),
        ureq::Error::Transport(t) => format!("couldn't reach the website ({t})"),
    }
}

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
    let mut action: Option<Action> = editor.queued_action.take();
    let mut toggle_play = ctx.input(|i| i.key_pressed(egui::Key::F5));

    unsaved_question(ctx, model, selection, camera, editor, status);

    // Logging in to the Brixo website, the first time you Publish.
    if editor.show_web_login {
        let mut open = true;
        egui::Window::new("Publish to the Brixo website")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                egui::Grid::new("web login").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                    ui.label("Username");
                    ui.text_edit_singleline(&mut editor.web_user);
                    ui.end_row();
                    ui.label("Password");
                    let pw = ui.add(egui::TextEdit::singleline(&mut editor.web_pass).password(true));
                    ui.end_row();
                    if pw.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        action = Some(Action::WebLogin);
                    }
                });
                if !editor.web_error.is_empty() {
                    ui.colored_label(brixo_client::theme::site::RED, &editor.web_error);
                }
                ui.add_space(4.0);
                if brixo_client::theme::gloss_button(ui, &format!("Log in and publish \"{}\"", editor.publish_name), brixo_client::theme::Gloss::Green).clicked() {
                    action = Some(Action::WebLogin);
                }
                ui.small("No account yet? Sign up on the website first.");
            });
        if !open {
            editor.show_web_login = false;
            editor.web_error.clear();
        }
    }

    // Keep the extra selection valid after undo, deletes and Stop.
    editor.also.retain(|id| model.get(*id).is_some() && Some(*id) != *selection);
    if selection.is_none() && !editor.also.is_empty() {
        *selection = Some(editor.also.remove(0));
    }

    // File shortcuts work while typing in a script too.
    if !playing {
        let (save_as, save_now, open, new) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::S),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::S),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::O),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::N),
            )
        });
        for (pressed, a) in [(save_as, Action::SaveAs), (save_now, Action::Save), (open, Action::Open), (new, Action::New)] {
            if pressed {
                action = Some(a);
            }
        }
    }
    // Ctrl+W closes the script tab that's showing.
    if let Some(t) = editor.tab.filter(|_| ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::W))) {
        let at = editor.tabs.iter().position(|x| *x == t).unwrap_or(0);
        editor.tabs.retain(|x| *x != t);
        editor.tab = at.checked_sub(1).and_then(|k| editor.tabs.get(k).copied());
    }

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
        // (During Play, 1-9 pick tools from the hotbar instead.)
        if k1 && !playing {
            editor.tool = Tool::Move;
        }
        if k2 && !playing {
            editor.tool = Tool::Rotate;
        }
        if k3 && !playing {
            editor.tool = Tool::Scale;
        }
        if del && !playing {
            action = Some(Action::Delete);
        }
        if !playing {
            // Ctrl+C / Ctrl+X / Ctrl+V arrive as clipboard events, not keys.
            let (copied, cut, pasted) = ctx.input(|i| {
                let mut out = (false, false, None);
                for e in &i.events {
                    match e {
                        egui::Event::Copy => out.0 = true,
                        egui::Event::Cut => out.1 = true,
                        egui::Event::Paste(text) => out.2 = Some(text.clone()),
                        _ => {}
                    }
                }
                out
            });
            if copied || cut {
                action = Some(Action::Copy);
                if cut {
                    editor.queued_action = Some(Action::Delete);
                }
            }
            if let Some(text) = pasted {
                // Anything copied in Brixo, from any game.
                let parent = container_for(model, *selection);
                let before = model.clone();
                if let Some(items) = model.paste_clipboard(&text, parent) {
                    editor.history.checkpoint(&before);
                    stack_if_overlapping(model, &items);
                    *status = format!("Pasted {}", plural(items.len(), "thing"));
                    select(items, selection, editor);
                } else if !text.trim().is_empty() {
                    *status = "That isn't something copied in Brixo Studio".to_string();
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F2)) {
                if let Some(id) = selection.filter(|id| *id != model.root()) {
                    editor.renaming = model.get(id).map(|i| (id, i.name.clone()));
                }
            }
            let (copy, paste, dup, group, ungroup, focus) = ctx.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::C),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::V),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::D),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::G),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::U),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::F),
                )
            });
            for (pressed, a) in [
                (copy, Action::Copy),
                (paste, Action::Paste),
                (dup, Action::Duplicate),
                (group, Action::Group),
                (ungroup, Action::Ungroup),
                (focus, Action::Focus),
            ] {
                if pressed {
                    action = Some(a);
                }
            }
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

    use brixo_client::theme::{self, site, Gloss};
    // The banner, like the website's: navy with studs, the brick logo, Play,
    // and publishing.
    egui::TopBottomPanel::top("banner").exact_height(54.0).frame(egui::Frame::NONE).show(ctx, |ui| {
        let rect = ui.max_rect();
        theme::paint_banner(ui.painter(), rect);
        let logo = theme::paint_small_logo(ui.painter(), rect.left_center() + egui::vec2(12.0, 3.0), 28.0);
        theme::paint_label(
            ui.painter(),
            rect.left_center() + egui::vec2(24.0 + logo, 3.0),
            egui::Align2::LEFT_CENTER,
            "S T U D I O",
            egui::FontId::proportional(12.0),
            site::TAGLINE,
        );
        let inner = egui::Rect::from_min_max(rect.min + egui::vec2(logo + 100.0, 0.0), rect.max - egui::vec2(12.0, 0.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            theme::on_blue(ui);
            let (label, gloss) = if playing { ("■  Stop (F5)", Gloss::Red) } else { ("▶  Play (F5)", Gloss::Green) };
            if theme::gloss_button(ui, label, gloss).clicked() {
                toggle_play = true;
            }
            ui.add_enabled(!playing, egui::DragValue::new(&mut editor.players).range(1..=8).prefix("Players: "))
                .on_hover_text("2 or more: test multiplayer with a local server and a window per player");
            ui.add_space(10.0);
            ui.label(egui::RichText::new(match play_time {
                Some(t) => format!("Playing  {t:.1}s"),
                None => status.clone(),
            }).color(site::TAGLINE));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if theme::gloss_button(ui, "Publish", Gloss::Gold).on_hover_text("Put this game on the Brixo website").clicked() {
                    action = Some(Action::Publish);
                }
                let v = ui.visuals_mut();
                v.override_text_color = Some(site::INK);
                v.extreme_bg_color = site::FIELD;
                v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(10, 31, 56));
                ui.add(egui::TextEdit::singleline(&mut editor.publish_name).desired_width(130.0).hint_text("Game name"))
                    .on_hover_text("The name players see on the Brixo website");
            });
        });
    });

    // The tab bar under it: the building tools.
    egui::TopBottomPanel::top("ribbon").exact_height(32.0).frame(egui::Frame::NONE).show(ctx, |ui| {
        let rect = ui.max_rect();
        theme::paint_gradient(ui.painter(), rect, site::BLUE2, site::BLUE);
        ui.painter().line_segment([rect.left_bottom() - egui::vec2(0.0, 1.0), rect.right_bottom() - egui::vec2(0.0, 1.0)], egui::Stroke::new(2.0, egui::Color32::WHITE));
        let inner = rect.shrink2(egui::vec2(10.0, 3.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            theme::on_blue(ui);
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add_enabled_ui(!playing, |ui| {
                ui.menu_button("File", |ui| {
                    theme::menu_items(ui);
                    for (label, keys, a) in [
                        ("New", "Ctrl+N", Action::New),
                        ("Open...", "Ctrl+O", Action::Open),
                        ("Save", "Ctrl+S", Action::Save),
                        ("Save As...", "Ctrl+Shift+S", Action::SaveAs),
                    ] {
                        if ui.add(egui::Button::new(label).shortcut_text(keys)).clicked() {
                            action = Some(a);
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    ui.menu_button("Open a sample game", |ui| {
                        theme::menu_items(ui);
                        for (i, (name, _)) in SAMPLES.iter().enumerate() {
                            if ui.button(*name).clicked() {
                                action = Some(Action::OpenSample(i));
                                ui.close_menu();
                            }
                        }
                    });
                });
                ui.separator();
                ui.selectable_value(&mut editor.tool, Tool::Move, "Move (1)");
                ui.selectable_value(&mut editor.tool, Tool::Rotate, "Rotate (2)");
                ui.selectable_value(&mut editor.tool, Tool::Scale, "Scale (3)");
                ui.checkbox(&mut editor.snap, "Snap");
                ui.add_enabled_ui(editor.snap, |ui| {
                    egui::ComboBox::from_id_salt("snap step")
                        .width(64.0)
                        .selected_text(stud_label(editor.snap_step))
                        .show_ui(ui, |ui| {
                            for step in [0.25, 0.5, 1.0, 2.0, 4.0] {
                                ui.selectable_value(&mut editor.snap_step, step, stud_label(step));
                            }
                        })
                        .response
                        .on_hover_text("How far moving and resizing snap");
                    egui::ComboBox::from_id_salt("turn step")
                        .width(48.0)
                        .selected_text(format!("{}°", editor.turn_step))
                        .show_ui(ui, |ui| {
                            for step in [5.0, 15.0, 45.0, 90.0] {
                                ui.selectable_value(&mut editor.turn_step, step, format!("{step}°"));
                            }
                        })
                        .response
                        .on_hover_text("How far turning snaps");
                });
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
                // The menus themselves are white boxes, like the rest.
                let menu_style = theme::menu_items;
                ui.menu_button("Add Part", |ui| {
                    menu_style(ui);
                    for shape in Shape::ALL {
                        if ui.button(shape_label(shape)).clicked() {
                            action = Some(Action::AddShape(shape));
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    if ui.button("Tool").on_hover_text("Something players hold. Put it inside a player to give it to them").clicked() {
                        action = Some(Action::AddTool);
                        ui.close_menu();
                    }
                });
                ui.menu_button("Add GUI", |ui| {
                    menu_style(ui);
                    for (label, class) in [("TextLabel", Class::TextLabel), ("TextButton", Class::TextButton), ("Frame", Class::Frame)] {
                        if ui.button(label).clicked() {
                            action = Some(Action::AddGui(class));
                            ui.close_menu();
                        }
                    }
                });
                ui.menu_button("Edit", |ui| {
                    menu_style(ui);
                    for (label, keys, a) in [
                        ("Copy", "Ctrl+C", Action::Copy),
                        ("Paste (on top)", "Ctrl+V", Action::Paste),
                        ("Duplicate (beside)", "Ctrl+D", Action::Duplicate),
                        ("Group into Model", "Ctrl+G", Action::Group),
                        ("Ungroup", "Ctrl+U", Action::Ungroup),
                        ("Focus camera", "F", Action::Focus),
                    ] {
                        if ui.add(egui::Button::new(label).shortcut_text(keys)).clicked() {
                            action = Some(a);
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    ui.label("Ctrl-click to select more.\nAlt-click picks a part inside a Model.");
                });
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
            });
        });
    });

    // Bottom panels go before the side panels so they span the full width.
    egui::TopBottomPanel::bottom("output")
        .resizable(true)
        .default_height(140.0)
        .frame(box_frame())
        .show(ctx, |ui| output_panel(ui, output));

    // Tabs for scripts that were deleted (or undone away) close.
    if !playing {
        editor.tabs.retain(|id| model.script(*id).is_some());
        if editor.tab.is_some_and(|t| !editor.tabs.contains(&t)) {
            editor.tab = None;
        }
    }

    egui::SidePanel::left("explorer")
        .default_width(220.0)
        .frame(box_frame())
        .show(ctx, |ui| {
            theme::title_bar(ui, "Explorer", |_| {});
            box_body(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    tree_node(ui, model, model.root(), selection, editor);
                    ui.add_space(24.0);
                    ui.weak("Drag to move things. Double-click or F2 to rename. Right-click for more.");
                });
            });
        });

    // What the Explorer asked for, applied now it's drawn.
    for request in std::mem::take(&mut editor.tree_requests) {
        match request {
            TreeRequest::Rename(id, name) => {
                let name = name.trim().to_string();
                if !name.is_empty() && model.get(id).is_some_and(|i| i.name != name) {
                    editor.history.checkpoint(model);
                    model.get_mut(id).unwrap().name = name;
                }
            }
            TreeRequest::Move(id, into) => {
                let (what, place) = (
                    model.get(id).map(|i| i.name.clone()).unwrap_or_default(),
                    model.get(into).map(|i| i.name.clone()).unwrap_or_default(),
                );
                let before = model.clone();
                if model.reparent(id, into) {
                    editor.history.checkpoint(&before);
                    *status = format!("Moved {what} into {place}");
                } else {
                    *status = format!("{what} can't go inside {place}");
                }
            }
            TreeRequest::Menu(id, a) => {
                if !selected_items(model, *selection, &editor.also).contains(&id) {
                    *selection = Some(id);
                    editor.also.clear();
                }
                editor.queued_action = Some(a);
            }
        }
    }

    egui::SidePanel::right("properties")
        .default_width(260.0)
        .frame(box_frame())
        .show(ctx, |ui| {
            theme::title_bar(ui, "Properties", |_| {});
            box_body(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_enabled_ui(!playing, |ui| properties_panel(ui, model, *selection, editor));
                });
            });
        });

    // The central panel is the 3D viewport. It's transparent, so the scene
    // drawn underneath shows through; egui just handles its input.
    // The middle: the World (the 3D view) and a tab for each open script.
    egui::CentralPanel::default()
        .frame(egui::Frame::default())
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            tab_strip(ui, model, editor);
            match editor.tab.filter(|id| model.script(*id).is_some()) {
                Some(id) => {
                    // Opaque, over the 3D scene drawn underneath.
                    ui.painter().rect_filled(ui.available_rect_before_wrap(), 0.0, egui::Color32::WHITE);
                    script_panel(ui, model, id, editor, playing);
                }
                None => viewport(ui, model, selection, status, camera, editor, playing),
            }
        });

    // Apply actions after the UI is built, so nothing is borrowed twice.
    match action {
        Some(Action::AddShape(shape)) => {
            editor.history.checkpoint(model);
            let parent = container_for(model, *selection);
            if let Some(id) = model.create(Class::Part, shape_label(shape), parent) {
                // Drop it in front of the camera instead of at the origin.
                let spot = camera.position + camera.forward() * 12.0;
                if let Some(p) = model.part_mut(id) {
                    p.position = V::new(spot.x.round(), spot.y.round().max(0.5), spot.z.round());
                    p.size = V::new(2.0, 2.0, 2.0);
                    p.shape = shape;
                    // Like Roblox: new parts are loose and fall when you press Play.
                    p.anchored = false;
                }
                select(vec![id], selection, editor);
                *status = format!("Added a {}", shape_label(shape));
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
                open_tab(editor, id);
                *status = "Added a Script. Write it here, then press Play to run it".to_string();
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
            let items = selected_items(model, *selection, &editor.also);
            if items.is_empty() {
                *status = "Nothing to delete".to_string();
            } else {
                editor.history.checkpoint(model);
                for id in &items {
                    model.remove(*id);
                }
                select(Vec::new(), selection, editor);
                editor.drag = None;
                *status = format!("Deleted {}", plural(items.len(), "thing"));
            }
        }
        Some(Action::AddGui(class)) => {
            editor.history.checkpoint(model);
            let parent = container_for(model, *selection);
            let name = match class {
                Class::TextLabel => "TextLabel",
                Class::TextButton => "TextButton",
                _ => "Frame",
            };
            if let Some(id) = model.create(class, name, parent) {
                select(vec![id], selection, editor);
                *status = format!("Added a {name}. Everyone sees it; put it inside a player (from a script) for one player's eyes only");
            }
        }
        Some(Action::AddTool) => {
            editor.history.checkpoint(model);
            let parent = container_for(model, *selection);
            if let Some(tool) = model.create(Class::Tool, "Tool", parent) {
                let spot = camera.position + camera.forward() * 10.0;
                if let Some(handle) = model.create(Class::Part, "Handle", tool) {
                    let p = model.part_mut(handle).unwrap();
                    p.position = V::new(spot.x.round(), spot.y.round().max(1.0), spot.z.round());
                    p.size = V::new(0.5, 0.5, 2.0);
                    p.color = Color::new(105, 64, 40);
                }
                select(vec![tool], selection, editor);
                *status = "Added a Tool. Its first part is the handle; build it pointing along +Z (blue arrow)".to_string();
            }
        }
        Some(Action::Copy) => {
            editor.clipboard = selected_items(model, *selection, &editor.also);
            // Also as text on the system clipboard: pastes into any game.
            let text = model.to_clipboard(&editor.clipboard);
            ctx.copy_text(text.clone());
            editor.clip_text = Some(text);
            *status = format!("Copied {}", plural(editor.clipboard.len(), "thing"));
        }
        // Pasting what was copied in another game (opened since).
        Some(Action::Paste) if editor.clipboard.iter().all(|id| model.get(*id).is_none()) && editor.clip_text.is_some() => {
            let text = editor.clip_text.clone().unwrap();
            let parent = container_for(model, *selection);
            let before = model.clone();
            match model.paste_clipboard(&text, parent) {
                Some(items) => {
                    editor.history.checkpoint(&before);
                    stack_if_overlapping(model, &items);
                    *status = format!("Pasted {}", plural(items.len(), "thing"));
                    select(items, selection, editor);
                }
                None => *status = "Nothing to paste".to_string(),
            }
        }
        Some(a @ (Action::Paste | Action::Duplicate)) => {
            // Paste stacks copies on top; Duplicate puts them beside.
            let source = if matches!(a, Action::Paste) {
                editor.clipboard.clone()
            } else {
                selected_items(model, *selection, &editor.also)
            };
            let source: Vec<InstanceId> = source.into_iter().filter(|id| model.get(*id).is_some()).collect();
            if source.is_empty() {
                *status = "Nothing to paste".to_string();
            } else {
                editor.history.checkpoint(model);
                let size = bounds(model, &moving_parts(model, &source)).map(|b| to_glam(b.size)).unwrap_or(Vec3::ONE);
                let copies: Vec<InstanceId> = source.iter().filter_map(|id| model.clone_subtree(*id)).collect();
                let by = if matches!(a, Action::Paste) { Vec3::Y * size.y } else { Vec3::X * size.x };
                shift(model, &copies, by);
                *status = format!("Made {}", plural(copies.len(), "copy"));
                select(copies, selection, editor);
            }
        }
        Some(Action::Group) => {
            let items = selected_items(model, *selection, &editor.also);
            if items.is_empty() {
                *status = "Select some parts to group".to_string();
            } else {
                editor.history.checkpoint(model);
                let parent = model.get(items[0]).and_then(|i| i.parent).unwrap_or(model.root());
                if let Some(group) = model.create(Class::Model, "Model", parent) {
                    for id in &items {
                        model.reparent(*id, group);
                    }
                    select(vec![group], selection, editor);
                    *status = format!("Grouped {} into a Model. Its parts are welded together in play", plural(items.len(), "thing"));
                }
            }
        }
        Some(Action::Ungroup) => {
            let models: Vec<InstanceId> = selected_items(model, *selection, &editor.also)
                .into_iter()
                .filter(|id| model.get(*id).is_some_and(|i| i.class == Class::Model))
                .collect();
            if models.is_empty() {
                *status = "Select a Model to ungroup".to_string();
            } else {
                editor.history.checkpoint(model);
                let mut freed = Vec::new();
                for m in models {
                    let Some(inst) = model.get(m) else { continue };
                    let (parent, children) = (inst.parent.unwrap_or(model.root()), inst.children.clone());
                    for c in children {
                        if model.reparent(c, parent) {
                            freed.push(c);
                        }
                    }
                    model.remove(m);
                }
                *status = format!("Ungrouped {}", plural(freed.len(), "thing"));
                select(freed, selection, editor);
            }
        }
        Some(Action::Focus) => {
            let items = selected_items(model, *selection, &editor.also);
            if let Some(b) = bounds(model, &moving_parts(model, &items)) {
                let reach = b.size.x.max(b.size.y).max(b.size.z);
                camera.position = to_glam(b.position) - camera.forward() * (reach * 1.5 + 6.0);
                *status = "Focused".to_string();
            }
        }
        Some(Action::WebLogin) => {
            // Log in to the website, then publish right away.
            let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(10)).build();
            let login = agent
                .post(&format!("{}/api/login", install::site().trim_end_matches('/')))
                .send_json(serde_json::json!({ "username": editor.web_user, "password": editor.web_pass }));
            match login {
                Ok(_) => {
                    editor.web = Some(agent);
                    editor.web_pass.clear();
                    editor.web_error.clear();
                    editor.show_web_login = false;
                    *status = publish_to_web(editor, model);
                }
                Err(e) => editor.web_error = web_error(e),
            }
        }
        Some(Action::Publish) => {
            if editor.web.is_some() {
                *status = publish_to_web(editor, model);
            } else {
                editor.show_web_login = true;
            }
        }
        Some(Action::Save) => {
            save(model, editor, status, false);
        }
        Some(Action::SaveAs) => {
            save(model, editor, status, true);
        }
        Some(a @ (Action::New | Action::Open | Action::OpenSample(_))) => {
            let what = match a {
                Action::New => Pending::New,
                Action::Open => Pending::Open,
                Action::OpenSample(i) => Pending::OpenSample(i),
                _ => unreachable!(),
            };
            if editor.unsaved() {
                editor.pending = Some(what);
            } else {
                carry_out(what, model, selection, camera, editor, status);
            }
        }
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

/// Play's extras over the 3D view, as in Brixo Player: shift lock's
/// crosshair and note, the chat box, and a hint when Esc freed the mouse.
fn play_overlay(ctx: &egui::Context, rect: egui::Rect, editor: &mut Editor) {
    let pill = |ui: &mut egui::Ui, text: &str| {
        egui::Frame::NONE
            .fill(brixo_client::classic::PANEL)
            .inner_margin(egui::Margin::symmetric(8, 3))
            .show(ui, |ui| ui.label(egui::RichText::new(text).font(brixo_client::classic::bold(13.0)).color(brixo_client::classic::YELLOW)));
    };
    if editor.shift_lock {
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("shift lock")));
        let c = ctx.screen_rect().center();
        p.circle_stroke(c, 7.0, egui::Stroke::new(3.0, egui::Color32::from_black_alpha(110)));
        p.circle_stroke(c, 7.0, egui::Stroke::new(1.6, egui::Color32::WHITE));
        p.circle_filled(c, 1.6, egui::Color32::WHITE);
        egui::Area::new(egui::Id::new("shift lock note"))
            .fixed_pos(rect.right_bottom() + egui::vec2(-170.0, -34.0))
            .show(ctx, |ui| pill(ui, "Shift lock on (Shift)"));
    }
    if editor.mouse_freed && editor.shift_lock {
        egui::Area::new(egui::Id::new("mouse freed"))
            .fixed_pos(rect.center_top() + egui::vec2(-130.0, 10.0))
            .show(ctx, |ui| pill(ui, "Mouse free: click the game to look again"));
    }
    // The chat bar where Brixo Player has it (under its toolbar): click it
    // or press / to talk.
    if let Some(text) = brixo_client::classic::chat_bar(ctx, rect.left_top() + egui::vec2(5.0, brixo_client::classic::TOP_BAR), 330.0, &mut editor.chat_open, &mut editor.chat_text) {
        editor.said = Some(text);
    }
}

/// Opens a script in a tab (or shows its tab if it's open).
fn open_tab(editor: &mut Editor, id: InstanceId) {
    if !editor.tabs.contains(&id) {
        editor.tabs.push(id);
    }
    editor.tab = Some(id);
    editor.focus_code = true;
}

/// The tabs along the top of the middle: the World, then open scripts,
/// each with a close button. Middle-click closes one too.
fn tab_strip(ui: &mut egui::Ui, model: &DataModel, editor: &mut Editor) {
    use brixo_client::theme::{self, site};
    let (strip, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::hover());
    let p = ui.painter().clone();
    theme::paint_gradient(&p, strip, egui::Color32::from_rgb(233, 241, 250), egui::Color32::from_rgb(211, 226, 242));
    p.line_segment([strip.left_bottom(), strip.right_bottom()], egui::Stroke::new(1.0, site::LINE));

    // A script's label: its name, and where it is when two open ones share a name.
    let label = |id: InstanceId| -> String {
        let Some(inst) = model.get(id) else { return "?".into() };
        let twin = editor.tabs.iter().any(|t| *t != id && model.get(*t).is_some_and(|o| o.name == inst.name));
        match inst.parent.and_then(|p| model.get(p)) {
            Some(parent) if twin => format!("{} › {}", parent.name, inst.name),
            _ => inst.name.clone(),
        }
    };
    let font = egui::FontId::proportional(13.0);
    let mut x = strip.left() + 6.0;
    let (mut pick, mut close) = (None, None);
    let entries: Vec<Option<InstanceId>> = std::iter::once(None).chain(editor.tabs.iter().copied().map(Some)).collect();
    for entry in entries {
        let text = match entry {
            None => "World".to_string(),
            Some(id) => label(id),
        };
        let galley = p.layout_no_wrap(text.clone(), font.clone(), site::INK);
        let width = galley.size().x + 36.0 + if entry.is_some() { 20.0 } else { 0.0 };
        let r = egui::Rect::from_min_size(egui::pos2(x, strip.top() + 4.0), egui::vec2(width, strip.height() - 4.0));
        x += width + 3.0;
        if r.left() > strip.right() {
            break;
        }
        let response = ui.interact(r, ui.id().with(("tab", entry)), egui::Sense::click());
        let open = editor.tab == entry;
        let fill = if open {
            egui::Color32::WHITE
        } else if response.hovered() {
            egui::Color32::from_rgb(245, 249, 253)
        } else {
            egui::Color32::from_rgb(222, 233, 245)
        };
        let corners = egui::CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 };
        p.rect_filled(r, corners, fill);
        p.rect_stroke(r, corners, egui::Stroke::new(1.0, site::LINE), egui::StrokeKind::Inside);
        if open {
            // Joined to the page below, like the website's open tab.
            p.line_segment([r.left_bottom() + egui::vec2(1.0, 0.0), r.right_bottom() - egui::vec2(1.0, 0.0)], egui::Stroke::new(2.0, egui::Color32::WHITE));
            p.line_segment([r.left_top() + egui::vec2(1.0, 1.0), r.right_top() + egui::vec2(-1.0, 1.0)], egui::Stroke::new(2.0, site::GOLD2));
        }
        let class = if entry.is_some() { Class::Script } else { Class::Workspace };
        let (glyph, color) = theme::badge(class);
        let badge = egui::Rect::from_center_size(egui::pos2(r.left() + 16.0, r.center().y), egui::vec2(16.0, 16.0));
        p.rect_filled(badge, 3.0, color);
        p.text(badge.center(), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(9.5), egui::Color32::WHITE);
        let text_color = if open { site::BLUE } else { site::INK };
        theme::paint_label(&p, egui::pos2(r.left() + 29.0, r.center().y), egui::Align2::LEFT_CENTER, &text, font.clone(), text_color);
        if let Some(id) = entry {
            let x_rect = egui::Rect::from_center_size(egui::pos2(r.right() - 13.0, r.center().y), egui::vec2(16.0, 16.0));
            let x_hover = ui.rect_contains_pointer(x_rect);
            if x_hover {
                p.rect_filled(x_rect, 3.0, egui::Color32::from_rgb(214, 226, 240));
            }
            p.text(x_rect.center(), egui::Align2::CENTER_CENTER, "×", egui::FontId::proportional(15.0), if x_hover { site::RED } else { site::DIM });
            let path = {
                let mut names = vec![];
                let mut at = Some(id);
                while let Some(i) = at.and_then(|a| model.get(a)) {
                    names.push(i.name.clone());
                    at = i.parent;
                }
                names.reverse();
                names.join(" › ")
            };
            let response = response.on_hover_text(path);
            if response.middle_clicked() || (response.clicked() && x_hover) {
                close = Some(id);
                continue;
            }
            if response.clicked() {
                pick = Some(entry);
            }
        } else if response.clicked() {
            pick = Some(None);
        }
    }
    if let Some(t) = pick {
        editor.tab = t;
    }
    if let Some(id) = close {
        let at = editor.tabs.iter().position(|t| *t == id).unwrap_or(0);
        editor.tabs.retain(|t| *t != id);
        if editor.tab == Some(id) {
            // Show the tab to its left (or the World).
            editor.tab = at.checked_sub(1).and_then(|k| editor.tabs.get(k).copied());
        }
    }
}

fn output_panel(ui: &mut egui::Ui, output: &mut Vec<LogLine>) {
    use brixo_client::theme::{self, site};
    let mut clear = false;
    theme::title_bar(ui, "Output", |ui| {
        clear = ui.small_button("Clear").clicked();
    });
    if clear {
        output.clear();
    }
    box_body(ui, |ui| {
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for line in output.iter() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        ui.label(egui::RichText::new(format!("[{}]", line.source)).monospace().color(site::DIM));
                        let text = egui::RichText::new(&line.text).monospace();
                        ui.label(if line.is_error { text.color(site::RED) } else { text });
                    });
                }
            });
    });
}

/// A panel dressed as one of the website's boxes: white, with a blue edge.
/// The title bar goes right at its top, so it has no margin of its own.
fn box_frame() -> egui::Frame {
    use brixo_client::theme::site;
    egui::Frame::NONE.fill(egui::Color32::WHITE).stroke(egui::Stroke::new(1.0, site::LINE))
}

/// The inside of a box, under its title bar.
fn box_body<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8, 6)).show(ui, add).inner
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

    // Where it is ("Workspace › Lava › Script"), and whether it runs.
    let mut path = vec![name.clone()];
    let mut up = inst.parent;
    while let Some(p) = up.and_then(|p| model.get(p)) {
        path.push(p.name.clone());
        up = p.parent;
    }
    path.reverse();
    let path = path.join("  ›  ");
    box_body(ui, |ui| {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(path).color(brixo_client::theme::site::DIM));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            changed |= ui
                .add_enabled(!playing, egui::Checkbox::new(&mut enabled, "Enabled"))
                .on_hover_text("Off: the script doesn't run when you press Play")
                .changed();
            if playing {
                ui.label(egui::RichText::new("Read-only while playing").color(brixo_client::theme::site::DIM));
            }
        });
    });

    // Checked on every frame (parsing is fast): the error's line is shaded.
    let parsed = rovik::lexer::lex(&source).and_then(rovik::parser::parse);
    let error_line = parsed.as_ref().err().map(|e| e.line);
    let edit_id = egui::Id::new(("script-editor", id));
    let font = egui::TextStyle::Monospace.resolve(ui.style());

    // Autocomplete keys, taken before the editor sees them (while the list
    // from last frame is showing): arrows choose, Tab completes, Escape hides.
    let mut accept = None;
    if let Some((start, list)) = editor.completion.clone() {
        let (tab, up, down, escape) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::Tab),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
            )
        });
        if up {
            editor.completion_pick = editor.completion_pick.saturating_sub(1);
        }
        if down {
            editor.completion_pick = (editor.completion_pick + 1).min(list.len() - 1);
        }
        if escape {
            editor.completion_hidden = Some(start);
        }
        if tab {
            accept = Some((start, list[editor.completion_pick.min(list.len() - 1)]));
        }
    }
    if let (Some((start, word)), false) = (accept, playing) {
        // Swap the partly typed word for the whole one; cursor after it.
        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), edit_id) {
            let cursor = state.cursor.char_range().map(|r| r.primary.index).unwrap_or(start);
            let byte = |c: usize| source.char_indices().nth(c).map(|(b, _)| b).unwrap_or(source.len());
            let (from, to) = (byte(start), byte(cursor));
            source.replace_range(from..to, word);
            let after = egui::text::CCursor::new(start + word.chars().count());
            state.cursor.set_char_range(Some(egui::text::CCursorRange::one(after)));
            state.store(ui.ctx(), edit_id);
            changed = true;
        }
        // (The suggestion list is cleared just below, and rebuilt.)
        editor.completion_pick = 0;
    }

    let lines = source.split('\n').count();
    let editor_height = (ui.available_height() - 30.0).max(60.0);
    let mut output = None;
    // The code box looks like the guide's: dark navy with a gold edge.
    let code_box = egui::Frame::NONE
        .fill(brixo_client::theme::site::CODE)
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(13, 27, 43)))
        .inner_margin(egui::Margin { left: 10, right: 4, top: 4, bottom: 4 });
    let box_response = code_box.show(ui, |ui| {
    {
        let v = ui.visuals_mut();
        v.override_text_color = Some(brixo_client::theme::site::CODE_TEXT);
        v.extreme_bg_color = brixo_client::theme::site::CODE;
        v.selection.bg_fill = egui::Color32::from_rgb(45, 84, 130);
        v.text_cursor.stroke = egui::Stroke::new(2.0, brixo_client::theme::site::GOLD);
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
            w.bg_stroke = egui::Stroke::NONE;
        }
    }
    egui::ScrollArea::vertical()
        .max_height(editor_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                // Line numbers.
                ui.vertical(|ui| {
                    ui.add_space(2.0);
                    let numbers: String = (1..=lines).map(|n| format!("{n:>3}\n")).collect();
                    ui.add(
                        egui::Label::new(egui::RichText::new(numbers.trim_end()).font(font.clone()).color(egui::Color32::from_rgb(110, 132, 158)))
                            .selectable(false),
                    );
                });
                let mut layouter = |ui: &egui::Ui, text: &str, _wrap: f32| {
                    let job = editing::highlight(text, error_line, font.clone());
                    ui.fonts(|f| f.layout_job(job))
                };
                if std::mem::take(&mut editor.focus_code) && !playing {
                    ui.memory_mut(|m| m.request_focus(edit_id));
                }
                let out = egui::TextEdit::multiline(&mut source)
                    .id(edit_id)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(12)
                    .interactive(!playing)
                    .layouter(&mut layouter)
                    .show(ui);
                changed |= out.response.changed();
                output = Some(out);
            });
        });
    });
    // The gold edge down its left side.
    let r = box_response.response.rect;
    ui.painter().rect_filled(
        egui::Rect::from_min_size(r.min, egui::vec2(4.0, r.height())),
        0.0,
        brixo_client::theme::site::GOLD2,
    );

    // Suggestions for the word being typed, under the cursor.
    editor.completion = None;
    if let Some(out) = output.filter(|o| o.response.has_focus() && !playing) {
        if let Some(range) = out.cursor_range.filter(|r| r.is_empty()) {
            if let Some((start, list)) = editing::completions(&source, range.primary.ccursor.index) {
                if editor.completion_hidden != Some(start) {
                    editor.completion_hidden = None;
                    if editor.completion_pick >= list.len() {
                        editor.completion_pick = 0;
                    }
                    let at = out.galley.pos_from_cursor(&range.primary).translate(out.galley_pos.to_vec2());
                    egui::Area::new(edit_id.with("complete"))
                        .order(egui::Order::Foreground)
                        .fixed_pos(at.left_bottom() + egui::vec2(0.0, 2.0))
                        .show(ui.ctx(), |ui| {
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                for (k, name) in list.iter().enumerate() {
                                    let _ = ui.selectable_label(
                                        k == editor.completion_pick,
                                        egui::RichText::new(*name).font(font.clone()),
                                    );
                                }
                                ui.weak("Tab to complete");
                            });
                        });
                    editor.completion = Some((start, list));
                }
            } else {
                editor.completion_hidden = None;
            }
        }
    }

    match &parsed {
        Ok(_) => {
            ui.weak("No syntax errors");
        }
        Err(e) => {
            ui.colored_label(brixo_client::theme::site::RED, format!("⚠ {e}"));
        }
    }
    });

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
    if !editor.also.is_empty() {
        ui.label(format!("{} selected; showing the first", editor.also.len() + 1));
        line_up_ui(ui, model, selection, editor);
        ui.separator();
    }

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
    let mut gui = model.gui(id).cloned();
    let mut lighting = (class == Class::Workspace).then(|| brixo_core::Lighting::of(model));
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
                ui.label("Material");
                egui::ComboBox::from_id_salt("material")
                    .selected_text(p.material.name())
                    .show_ui(ui, |ui| {
                        for m in brixo_core::Material::ALL {
                            changed |= ui.selectable_value(&mut p.material, m, m.name()).changed();
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Transparency");
                changed |= ui.add(egui::Slider::new(&mut p.transparency, 0.0..=1.0)).changed();
            });
            changed |= vec3_row(ui, "Velocity", &mut p.velocity, 0.1)
                ;
            ui.small("Anchored + velocity = a conveyor belt");
            ui.horizontal(|ui| {
                ui.label("Shape");
                egui::ComboBox::from_id_salt("shape")
                    .selected_text(shape_label(p.shape))
                    .show_ui(ui, |ui| {
                        for shape in Shape::ALL {
                            changed |= ui.selectable_value(&mut p.shape, shape, shape_label(shape)).changed();
                        }
                    });
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
            ui.horizontal(|ui| {
                changed |= ui.checkbox(&mut p.floating, "Floating").on_hover_text("Loose, but no gravity: flies straight").changed();
                ui.label("Bounce");
                changed |= ui.add(egui::Slider::new(&mut p.bounce, 0.0..=1.0)).changed();
            });

            // Hinge and motor.
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Hinge")
                    .on_hover_text("Turns the part around one of its own lines, held to whatever it touches nearest the hinge (or to where it is). Doors, wheels, drawbridges, spinners.");
                let before = p.hinge;
                egui::ComboBox::from_id_salt("hinge")
                    .selected_text(p.hinge.title())
                    .show_ui(ui, |ui| {
                        for h in brixo_core::Hinge::ALL {
                            changed |= ui.selectable_value(&mut p.hinge, h, h.title()).changed();
                        }
                    });
                // A hinged part has to be loose to turn.
                if before == brixo_core::Hinge::Off && p.hinge != brixo_core::Hinge::Off {
                    p.anchored = false;
                }
            });
            if p.hinge != brixo_core::Hinge::Off {
                ui.horizontal(|ui| {
                    ui.label("Hinge at");
                    egui::ComboBox::from_id_salt("hinge_at")
                        .selected_text(side_label(p.hinge_at))
                        .show_ui(ui, |ui| {
                            for side in brixo_core::Side::ALL {
                                changed |= ui.selectable_value(&mut p.hinge_at, side, side_label(side)).changed();
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Motor speed").on_hover_text("Degrees a second it keeps turning (a wheel, a spinner). 0: no motor, it swings freely.");
                    changed |= ui.add(egui::DragValue::new(&mut p.motor_speed).speed(5.0).range(-3600.0..=3600.0).suffix("°/s")).changed();
                });
                ui.horizontal(|ui| {
                    let mut on = p.swing_to.is_some();
                    if ui.checkbox(&mut on, "Swing to").on_hover_text("Turns to this angle (from where it starts) and holds it: an open door, a lowered drawbridge.").changed() {
                        p.swing_to = on.then_some(90.0);
                        changed = true;
                    }
                    if let Some(a) = p.swing_to.as_mut() {
                        changed |= ui.add(egui::DragValue::new(a).speed(1.0).range(-180.0..=180.0).suffix("°")).changed();
                    }
                });
                if p.anchored {
                    ui.colored_label(egui::Color32::from_rgb(200, 90, 20), "Anchored parts don't turn: untick Anchored.");
                }
            }

            p.size.x = p.size.x.max(0.01);
            p.size.y = p.size.y.max(0.01);
            p.size.z = p.size.z.max(0.01);
        }
        None if model.sound(id).is_some() => {
            let s = model.sound_mut(id).unwrap();
            let size = s.data.len() * 3 / 4;
            ui.label(format!("{} file, {:.1} MB", s.format.to_uppercase(), size as f64 / 1_000_000.0));
            ui.horizontal(|ui| {
                ui.label("Volume");
                changed |= ui.add(egui::Slider::new(&mut s.volume, 0.0..=1.0)).changed();
            });
            if ui.button("▶ Preview").clicked() {
                editor.preview = Some(id);
            }
            ui.small("Play it from a script: play_sound(find(\"Name\"))");
        }
        None => match gui.as_mut() {
            Some(g) => {
                ui.horizontal(|ui| {
                    ui.label("Text");
                    changed |= ui.text_edit_singleline(&mut g.text).changed();
                });
                ui.label("Position and size (fractions of the screen)");
                for (label, a, b) in [("Position", 0usize, 1usize), ("Size", 2, 3)] {
                    ui.horizontal(|ui| {
                        ui.label(label);
                        let [x, y, w, h] = [&mut g.x, &mut g.y, &mut g.width, &mut g.height];
                        let mut pair = [x, y, w, h].into_iter().skip(a).step_by(b - a).take(2);
                        let first = pair.next().unwrap();
                        changed |= ui.add(egui::DragValue::new(first).speed(0.005).range(0.0..=1.0).max_decimals(3)).changed();
                        let second = pair.next().unwrap();
                        changed |= ui.add(egui::DragValue::new(second).speed(0.005).range(0.0..=1.0).max_decimals(3)).changed();
                    });
                }
                ui.horizontal(|ui| {
                    ui.label("Text size");
                    changed |= ui.add(egui::DragValue::new(&mut g.text_size).speed(0.5).range(4.0..=200.0)).changed();
                });
                for (label, c) in [("Text color", &mut g.text_color), ("Background", &mut g.background_color)] {
                    ui.horizontal(|ui| {
                        ui.label(label);
                        let mut rgb = [c.r, c.g, c.b];
                        if ui.color_edit_button_srgb(&mut rgb).changed() {
                            *c = Color::new(rgb[0], rgb[1], rgb[2]);
                            changed = true;
                        }
                    });
                }
                ui.horizontal(|ui| {
                    changed |= ui.checkbox(&mut g.background, "Show background").changed();
                    changed |= ui.checkbox(&mut g.visible, "Visible").changed();
                });
            }
            None if class == Class::Script => {
                if brixo_client::theme::gloss_button(ui, "Edit script", brixo_client::theme::Gloss::Blue).clicked() {
                    open_tab(editor, id);
                }
                ui.label(egui::RichText::new("Or double-click it in the Explorer.").small().color(brixo_client::theme::site::DIM));
            }
            None if lighting.is_some() => {
                let l = lighting.as_mut().unwrap();
                changed |= lighting_ui(ui, l);
            }
            None => {
                ui.label("No editable properties.");
            }
        },
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
        if let (Some(new), Some(g)) = (gui, model.gui_mut(id)) {
            *g = new;
        }
        if let Some(l) = lighting {
            l.set(model);
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
    // Trackpads: pinch zooms, a two-finger swipe sideways turns. While
    // building, scrolling and pinching fly the camera forward and back.
    if response.hovered() {
        let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
        if playing {
            editor.follow.zoom(scroll.y);
            trackpad_look(camera, &mut editor.follow, scroll, pinch);
        } else {
            let fly = scroll.y * 0.05 + (pinch - 1.0) * 20.0;
            camera.position += camera.forward() * fly;
            camera.yaw -= scroll.x * 0.004;
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
    // Snap steps (moves in studs, turns in degrees), or None.
    let snap = editor.snap.then_some((editor.snap_step, editor.turn_step));

    // What's selected, and what the gizmo sits on: a single part, or (for
    // a Model or several things) the box around all their parts.
    let items = selected_items(model, *selection, &editor.also);
    let group = is_group(model, &items);
    let target = |model: &DataModel| -> Option<PartProps> {
        if group {
            bounds(model, &moving_parts(model, &items))
        } else {
            items.first().and_then(|id| model.part(*id).copied())
        }
    };

    // No gizmos while playing: the game world isn't editable.
    let gizmo = if playing { None } else { target(model).map(|p| Gizmo::new(&p, camera, tool)) };

    // Left-drag starting on a handle: begin a gizmo drag.
    if response.drag_started_by(egui::PointerButton::Primary) {
        let origin = ui.input(|i| i.pointer.press_origin());
        if let (Some(g), Some(o), Some(start)) = (gizmo.as_ref(), origin, target(model)) {
            if let Some(axis) = g.hit_test(&vp, screen, o) {
                if group && tool == Tool::Scale {
                    *status = "Scale works on one part at a time: select a single part".to_string();
                } else if let Some(drag) = Drag::begin(g, &vp, screen, axis, o, start, camera) {
                    // One undo step per drag, taken before anything moves.
                    editor.history.checkpoint(model);
                    editor.group_drag = group.then(|| {
                        let parts = moving_parts(model, &items);
                        (start, parts.iter().filter_map(|id| model.part(*id).map(|p| (*id, *p))).collect())
                    });
                    editor.drag = Some(drag);
                }
            }
        }
    }

    // Left-drag starting on a selected part (not a handle): slide it over
    // whatever's under the mouse, resting on it.
    if response.drag_started_by(egui::PointerButton::Primary) && editor.drag.is_none() && !playing {
        if let Some(o) = ui.input(|i| i.pointer.press_origin()) {
            let (nx, ny) = ndc(screen, o);
            if let Some(hit) = brixo_render::pick(model, camera, aspect, nx, ny) {
                if items.iter().any(|it| *it == hit || is_inside(model, hit, *it)) {
                    editor.history.checkpoint(model);
                    editor.surface_drag = Some(moving_parts(model, &items));
                }
            }
        }
    }
    // Left-drag on anything else: a box select.
    if response.drag_started_by(egui::PointerButton::Primary) && editor.drag.is_none() && editor.surface_drag.is_none() && !playing {
        editor.box_select = ui.input(|i| i.pointer.press_origin());
    }
    if let (Some(parts), true) = (editor.surface_drag.clone(), response.dragged_by(egui::PointerButton::Primary)) {
        if let Some(pos) = response.interact_pointer_pos() {
            let (nx, ny) = ndc(screen, pos);
            let inv = vp.inverse();
            let near = inv.project_point3(Vec3::new(nx, ny, 0.0));
            let far = inv.project_point3(Vec3::new(nx, ny, 1.0));
            let skip: std::collections::HashSet<InstanceId> = parts.iter().copied().collect();
            let props: Vec<PartProps> = parts.iter().filter_map(|id| model.part(*id).copied()).collect();
            if let (Some((point, normal)), Some((center, half))) =
                (editing::surface_hit(model, near, (far - near).normalize(), &skip), editing::world_box(&props))
            {
                let by = editing::rest_on(point, normal, half, snap.map(|(m, _)| m)) - center;
                if by.length() > 1e-4 {
                    shift(model, &parts, by);
                }
            }
        }
    }

    // Continue an active drag: move the part, or the whole group.
    if response.dragged_by(egui::PointerButton::Primary) {
        if let (Some(drag), Some(pos)) = (editor.drag.as_mut(), response.interact_pointer_pos()) {
            match &editor.group_drag {
                Some((start, parts)) => {
                    let mut now = *start;
                    drag.apply(pos, snap, &mut now);
                    apply_group(model, start, &now, parts);
                }
                None => {
                    if let Some(p) = selection.and_then(|id| model.part_mut(id)) {
                        drag.apply(pos, snap, p);
                    }
                }
            }
        }
    }

    if response.drag_stopped() {
        editor.drag = None;
        editor.group_drag = None;
        editor.surface_drag = None;
    }

    // The game's GUI over the viewport. While editing it shows shared GUI
    // so you can see your layout; during Play it's live, with the hotbar.
    let me = if playing { editor.local_player } else { None };
    let project = projector(camera, screen);
    let mut events = draw_gui(ui.ctx(), rect, model, me, playing, &project);
    if playing {
        draw_beacons(ui.ctx(), screen, model, me, camera);
        brixo_client::kart_fx::draw_effects(ui.ctx(), model, camera.position, &project, ui.ctx().input(|i| i.time) as f32);
        brixo_client::kart_fx::draw_hud(ui.ctx(), rect, model, me);
        editor.chat.draw(ui.ctx(), rect, model, &project);
    }
    if let Some(me) = me {
        draw_hotbar(ui.ctx(), rect, model, me, &mut events);
        events.hotbar = events.hotbar.or_else(|| hotbar_key(ui.ctx()));
        // A click swings the tool in hand, aimed where the mouse points.
        let locked = editor.mouse_locked;
        let swing = (response.clicked() && !events.pointer_on_gui && !editor.mouse_freed)
            .then(|| if locked { Some(screen.center()) } else { response.interact_pointer_pos() })
            .flatten()
            .map(|pos| brixo_client::aim_point(model, camera, screen.width() / screen.height(), Some(me), ndc(screen, pos)));
        editor.play_input = Some((events, swing));
        // Esc freed the mouse: a click on the game takes it back.
        if editor.mouse_freed && response.clicked() {
            editor.mouse_freed = false;
        }
        brixo_client::leaderboard::draw(ui.ctx(), rect, model, editor.local_player, &mut editor.board_open);
        // The classic health bar, like Brixo Player's.
        if let Some(p) = model.player(me) {
            brixo_client::classic::health_bar(ui.ctx(), rect, p.health, p.max_health);
        }
        play_overlay(ui.ctx(), rect, editor);
    }

    // Plain left-click (no drag): select whatever is under the cursor,
    // unless the click landed on a gizmo handle. Clicking a part inside a
    // Model selects the Model (Alt-click picks the part itself); Ctrl-click
    // adds to or removes from the selection.
    if response.clicked() && !playing {
        if let Some(pos) = response.interact_pointer_pos() {
            let on_handle = gizmo.as_ref().and_then(|g| g.hit_test(&vp, screen, pos)).is_some();
            if !on_handle {
                let ndc_x = (pos.x - screen.left()) / screen.width() * 2.0 - 1.0;
                let ndc_y = 1.0 - (pos.y - screen.top()) / screen.height() * 2.0;
                let (ctrl, alt) = ui.input(|i| (i.modifiers.command, i.modifiers.alt));
                let hit = brixo_render::pick(model, camera, aspect, ndc_x, ndc_y)
                    .map(|id| if alt || playing { id } else { model.top_model(id) });
                match hit {
                    Some(id) if ctrl && selection.is_some() => {
                        if *selection == Some(id) {
                            *selection = if editor.also.is_empty() { None } else { Some(editor.also.remove(0)) };
                        } else if let Some(k) = editor.also.iter().position(|x| *x == id) {
                            editor.also.remove(k);
                        } else {
                            editor.also.push(id);
                        }
                        let n = selected_items(model, *selection, &editor.also).len();
                        *status = format!("{} selected", plural(n, "thing"));
                    }
                    _ => {
                        *status = match hit.and_then(|id| model.get(id)) {
                            Some(inst) => format!("Selected {}", inst.name),
                            None => "Nothing there".to_string(),
                        };
                        *selection = hit;
                        editor.also.clear();
                    }
                }
            }
        }
    }

    // The box select: drawn while dragging; on letting go, everything
    // fully inside it is selected (Ctrl adds to what's selected already).
    if let Some(start) = editor.box_select {
        let now = response.interact_pointer_pos().or_else(|| ui.input(|i| i.pointer.hover_pos())).unwrap_or(start);
        let r = egui::Rect::from_two_pos(start, now);
        let painter = ui.painter_at(rect);
        painter.rect_filled(r, 0.0, egui::Color32::from_rgba_unmultiplied(77, 143, 214, 45));
        painter.rect_stroke(r, 0.0, egui::Stroke::new(1.5, egui::Color32::from_rgb(37, 102, 176)), egui::StrokeKind::Inside);
        if response.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
            editor.box_select = None;
            if r.width() > 4.0 && r.height() > 4.0 {
                let alt = ui.input(|i| i.modifiers.alt);
                let mut picked: Vec<InstanceId> = Vec::new();
                for id in model.walk() {
                    if model.part(id).is_none() {
                        continue;
                    }
                    let item = if alt { id } else { model.top_model(id) };
                    if picked.contains(&item) {
                        continue;
                    }
                    let Some(b) = bounds(model, &model.parts_under(item)) else { continue };
                    let (c, h) = (to_glam(b.position), to_glam(b.size) / 2.0);
                    let corners = [-1.0, 1.0].into_iter().flat_map(|x| [-1.0, 1.0].into_iter().flat_map(move |y| [-1.0, 1.0].into_iter().map(move |z| Vec3::new(x, y, z))));
                    if editing::fully_inside(r.min, r.max, corners.map(|k| project(c + k * h))) {
                        picked.push(item);
                    }
                }
                let adding = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                let mut all: Vec<InstanceId> = if adding { selected_items(model, *selection, &editor.also) } else { Vec::new() };
                for id in picked {
                    if !all.contains(&id) {
                        all.push(id);
                    }
                }
                *selection = all.first().copied();
                editor.also = all.iter().skip(1).copied().collect();
                *status = match all.len() {
                    0 => "Nothing fully inside the box".to_string(),
                    n => format!("{} selected", plural(n, "thing")),
                };
            }
        }
    }

    // Hinged parts in the selection: an orange line along each hinge.
    if !playing {
        let painter = ui.painter_at(rect);
        for id in moving_parts(model, &items) {
            let Some(p) = model.part(id) else { continue };
            let Some(axis) = p.hinge.axis() else { continue };
            let q = part_quat(p);
            let pivot = to_glam(p.position) + q * to_glam(p.hinge_at.point(p.size));
            let dir = q * to_glam(axis);
            let reach = (to_glam(p.size) * to_glam(axis)).length() / 2.0 + 0.8;
            let (Some(a), Some(b)) = (project(pivot - dir * reach), project(pivot + dir * reach)) else { continue };
            let orange = egui::Color32::from_rgb(255, 150, 30);
            painter.line_segment([a, b], egui::Stroke::new(5.0, egui::Color32::from_rgba_unmultiplied(40, 20, 0, 160)));
            painter.line_segment([a, b], egui::Stroke::new(3.0, orange));
            for end in [a, b] {
                painter.circle_filled(end, 4.5, orange);
            }
            if let Some(c) = project(pivot) {
                painter.circle_stroke(c, 6.0, egui::Stroke::new(2.0, egui::Color32::WHITE));
            }
        }
    }

    // Draw the gizmo for the (possibly just-moved) selection.
    let visible = if playing { None } else { target(model) };
    if let Some(p) = visible {
        let g = Gizmo::new(&p, camera, tool);
        let active = match editor.drag.as_ref() {
            Some(d) => Some(d.axis),
            None => response.hover_pos().and_then(|pos| g.hit_test(&vp, screen, pos)),
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

    fn apply(&mut self, pos: egui::Pos2, snap: Option<(f32, f32)>, p: &mut PartProps) {
        match self.tool {
            Tool::Move => {
                let mut amount = (pos - self.start_pointer).dot(self.screen_dir) * self.units_per_px;
                if let Some((step, _)) = snap {
                    amount = (amount / step).round() * step;
                }
                p.position = from_glam(to_glam(self.start.position) + self.world_axis * amount);
            }

            Tool::Scale => {
                let mut amount = (pos - self.start_pointer).dot(self.screen_dir) * self.units_per_px;
                if let Some((step, _)) = snap {
                    amount = (amount / step).round() * step;
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
                if let Some((_, turn)) = snap {
                    degrees = (degrees / turn).round() * turn;
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

/// A screen point as normalised device coordinates (-1 to 1, y up).
fn ndc(screen: egui::Rect, p: egui::Pos2) -> (f32, f32) {
    (
        (p.x - screen.min.x) / screen.width() * 2.0 - 1.0,
        1.0 - (p.y - screen.min.y) / screen.height() * 2.0,
    )
}

/// True if `node` is `ancestor` or somewhere inside it.
fn is_inside(model: &DataModel, node: InstanceId, ancestor: InstanceId) -> bool {
    let mut current = Some(node);
    while let Some(id) = current {
        if id == ancestor {
            return true;
        }
        current = model.get(id).and_then(|i| i.parent);
    }
    false
}

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

// --- selection -------------------------------------------------------------

/// Everything selected: the main selection, then any Ctrl-clicked extras.
fn selected_items(model: &DataModel, selection: Option<InstanceId>, also: &[InstanceId]) -> Vec<InstanceId> {
    let mut items: Vec<InstanceId> = selection.into_iter().chain(also.iter().copied()).collect();
    let mut seen = HashSet::new();
    items.retain(|id| model.get(*id).is_some() && *id != model.root() && seen.insert(*id));
    items
}

/// The parts that move when the selection moves.
fn moving_parts(model: &DataModel, items: &[InstanceId]) -> Vec<InstanceId> {
    let mut seen = HashSet::new();
    items.iter().flat_map(|id| model.parts_under(*id)).filter(|id| seen.insert(*id)).collect()
}

/// More than one single part: moved and rotated as one, around its box.
fn is_group(model: &DataModel, items: &[InstanceId]) -> bool {
    items.len() > 1 || items.first().is_some_and(|id| model.part(*id).is_none())
}

/// The box around some parts (rotations included), as a part-shaped box:
/// the group gizmo sits on it and the camera focuses on it.
fn bounds(model: &DataModel, parts: &[InstanceId]) -> Option<PartProps> {
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for p in parts.iter().filter_map(|id| model.part(*id)) {
        let (q, c, h) = (part_quat(p), to_glam(p.position), to_glam(p.size) / 2.0);
        for sx in [-1.0, 1.0] {
            for sy in [-1.0, 1.0] {
                for sz in [-1.0, 1.0] {
                    let corner = c + q * Vec3::new(sx * h.x, sy * h.y, sz * h.z);
                    lo = lo.min(corner);
                    hi = hi.max(corner);
                }
            }
        }
    }
    (lo.x <= hi.x).then(|| PartProps {
        position: from_glam((lo + hi) / 2.0),
        size: from_glam(hi - lo),
        ..PartProps::default()
    })
}

/// Moves (and turns) a group's parts to follow its box, from where each
/// part started. `q` turns around the box's starting centre.
fn apply_group(model: &mut DataModel, start: &PartProps, now: &PartProps, parts: &[(InstanceId, PartProps)]) {
    let q = part_quat(now) * part_quat(start).inverse();
    let (c0, c1) = (to_glam(start.position), to_glam(now.position));
    for (id, s) in parts {
        let Some(p) = model.part_mut(*id) else { continue };
        p.position = from_glam(c1 + q * (to_glam(s.position) - c0));
        let (y, x, z) = (q * part_quat(s)).to_euler(EulerRot::YXZ);
        p.rotation = V::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
    }
}

/// Shifts every part under `items` by `by`.
fn shift(model: &mut DataModel, items: &[InstanceId], by: Vec3) {
    for id in moving_parts(model, items) {
        if let Some(p) = model.part_mut(id) {
            p.position = from_glam(to_glam(p.position) + by);
        }
    }
}

/// Makes `items` the selection (the first is the main one).
fn select(items: Vec<InstanceId>, selection: &mut Option<InstanceId>, editor: &mut Editor) {
    let mut it = items.into_iter();
    *selection = it.next();
    editor.also = it.collect();
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
    editor: &mut Editor,
) {
    let Some(inst) = model.get(id) else { return };
    let children = inst.children.clone();
    let is_root = id == model.root();

    let selected = *selection == Some(id) || editor.also.contains(&id);
    // A badge for what it is, its name (or a box to rename it), and its
    // class, dimmed.
    let row = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            brixo_client::theme::paint_badge(ui, inst.class);
            let renaming = editor.renaming.as_ref().is_some_and(|(r, _)| *r == id);
            let row = if renaming {
                let (_, name) = editor.renaming.as_mut().unwrap();
                let edit = ui.add(egui::TextEdit::singleline(name).desired_width(140.0));
                if !edit.has_focus() && !edit.lost_focus() {
                    edit.request_focus();
                }
                let (enter, escape) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
                if escape {
                    editor.renaming = None;
                } else if enter || edit.lost_focus() {
                    let (_, name) = editor.renaming.take().unwrap();
                    editor.tree_requests.push(TreeRequest::Rename(id, name));
                }
                edit
            } else {
                ui.selectable_label(selected, &inst.name).interact(egui::Sense::click_and_drag())
            };
            ui.label(egui::RichText::new(format!("{:?}", inst.class)).small().color(brixo_client::theme::site::DIM));
            row
        })
        .inner;

    // Drag and drop: carry this row onto another to move it inside.
    if !is_root {
        row.dnd_set_drag_payload(id);
    }
    if row.dnd_hover_payload::<InstanceId>().is_some_and(|p| *p != id) {
        ui.painter().rect_stroke(
            row.rect.expand(2.0),
            4.0,
            egui::Stroke::new(1.5, brixo_client::theme::site::GOLD2),
            egui::StrokeKind::Outside,
        );
    }
    if let Some(dragged) = row.dnd_release_payload::<InstanceId>() {
        if *dragged != id {
            editor.tree_requests.push(TreeRequest::Move(*dragged, id));
        }
    }

    if row.double_clicked() && inst.class == Class::Script {
        // Scripts open in a tab (F2 renames them).
        *selection = Some(id);
        open_tab(editor, id);
    } else if row.double_clicked() && !is_root {
        editor.renaming = Some((id, inst.name.clone()));
    } else if row.clicked() {
        if ui.input(|i| i.modifiers.command) && selection.is_some() {
            // Ctrl-click: add to or remove from the selection.
            if *selection == Some(id) {
                *selection = if editor.also.is_empty() { None } else { Some(editor.also.remove(0)) };
            } else if let Some(k) = editor.also.iter().position(|x| *x == id) {
                editor.also.remove(k);
            } else {
                editor.also.push(id);
            }
        } else {
            *selection = Some(id);
            editor.also.clear();
        }
    }

    // Right-click: the things you can do to it.
    if !is_root {
        row.context_menu(|ui| {
            let mut pick = |ui: &mut egui::Ui, label: &str, a: Action| {
                if ui.button(label).clicked() {
                    editor.tree_requests.push(TreeRequest::Menu(id, a));
                    ui.close_menu();
                }
            };
            if ui.button("Rename  (F2)").clicked() {
                editor.renaming = Some((id, inst.name.clone()));
                ui.close_menu();
            }
            pick(ui, "Duplicate  (Ctrl+D)", Action::Duplicate);
            pick(ui, "Copy  (Ctrl+C)", Action::Copy);
            pick(ui, "Paste into  (Ctrl+V)", Action::Paste);
            pick(ui, "Group into a Model  (Ctrl+G)", Action::Group);
            if inst.class == Class::Model {
                pick(ui, "Ungroup  (Ctrl+U)", Action::Ungroup);
            }
            ui.separator();
            pick(ui, "Delete  (Del)", Action::Delete);
        });
    }

    if !children.is_empty() {
        ui.indent(id, |ui| {
            for child in children {
                tree_node(ui, model, child, selection, editor);
            }
        });
    }
}

/// Things pasted exactly where others already are (pasting in the same
/// game) go on top of them, like Paste always has.
fn stack_if_overlapping(model: &mut DataModel, items: &[InstanceId]) {
    let parts = moving_parts(model, items);
    let pasted: std::collections::HashSet<InstanceId> = parts.iter().copied().collect();
    let overlaps = parts.iter().filter_map(|id| model.part(*id).map(|p| p.position)).any(|pos| {
        model.walk().into_iter().any(|other| !pasted.contains(&other) && model.part(other).is_some_and(|q| q.position == pos))
    });
    if overlaps {
        if let Some(b) = bounds(model, &parts) {
            shift(model, items, Vec3::Y * b.size.y);
        }
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
    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: winit::event::DeviceId, event: winit::event::DeviceEvent) {
        // Raw mouse movement turns the camera while Play has the mouse locked.
        if let winit::event::DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if self.editor.mouse_locked && !self.confined {
                self.camera.yaw += dx as f32 * LOCKED_LOOK_SPEED;
                self.camera.pitch = (self.camera.pitch - dy as f32 * LOCKED_LOOK_SPEED).clamp(-1.5, 1.5);
            }
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let attrs = brixo_client::filming::window_attributes("Brixo Studio", 1400.0, 860.0);
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
            // Closing with unsaved changes asks first.
            WindowEvent::CloseRequested => {
                if self.editor.unsaved() && self.game.is_none() && self.hosted.is_none() {
                    self.editor.pending = Some(Pending::Quit);
                } else {
                    event_loop.exit();
                }
            }
            // An audio file dropped on the window becomes a Sound in the game.
            WindowEvent::DroppedFile(path) => {
                if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("brixo")) {
                    self.open_dropped_game(&path);
                } else {
                    self.import_audio(&path);
                }
            }

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
                        ElementState::Pressed => {
                            if !event.repeat && self.game.is_some() {
                                self.play_key(code, consumed);
                            }
                            if !consumed && !self.editor.chat_open {
                                self.keys.insert(code);
                            }
                        }
                    }
                }
            }

            // Where the mouse could only be confined (X11): turn by how far
            // it moved from the middle, then put it back.
            WindowEvent::CursorMoved { position, .. } if self.editor.mouse_locked && self.confined => {
                if let Some(gpu) = &self.gpu {
                    let size = gpu.window.inner_size();
                    let (cx, cy) = ((size.width / 2) as f64, (size.height / 2) as f64);
                    let (dx, dy) = (position.x - cx, position.y - cy);
                    if dx.abs() > 0.5 || dy.abs() > 0.5 {
                        self.camera.yaw += dx as f32 * LOCKED_LOOK_SPEED;
                        self.camera.pitch = (self.camera.pitch - dy as f32 * LOCKED_LOOK_SPEED).clamp(-1.5, 1.5);
                        let _ = gpu.window.set_cursor_position(winit::dpi::PhysicalPosition::new(cx, cy));
                    }
                }
            }

            WindowEvent::Focused(false) => {
                self.keys.clear();
                // Switching to another window frees the mouse.
                self.editor.mouse_freed = true;
            }

            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last_frame).as_secs_f32().min(0.1);
                self.last_frame = now;

                self.move_camera(dt);
                self.frame(dt);
                self.update_mouse_lock();
                if self.editor.quit {
                    event_loop.exit();
                    return;
                }
                self.update_title();

                if let Some(gpu) = self.gpu.as_ref() {
                    gpu.window.request_redraw();
                }
            }

            _ => {}
        }
    }
}

fn main() {
    // The downloaded BrixoStudio.exe installs itself, then hands over to the
    // installed copy; `--uninstall` is what Windows' Uninstall runs.
    if install::on_startup(App::Studio, |_| Ok(())) == install::Startup::Exit {
        return;
    }
    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut studio = Studio::new();
    event_loop.run_app(&mut studio).expect("event loop failed");
}
