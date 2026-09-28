//! Runs a game: every script, its waits, its timers and its events.
//!
//! Each script task (a script's main body, or one run of an event handler)
//! gets its own thread, but only one task ever runs at a time. The game
//! resumes a task and blocks until that task calls wait() or finishes. That
//! gives scripts true pausing anywhere, even deep inside loops and
//! functions, without anything actually running in parallel.
//!
//! Stopping the game drops the channels, so every paused wait() returns an
//! error and its thread unwinds on its own.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};
pub use parking_lot::MutexGuard as WorldMutexGuard;
use parking_lot::Mutex as WorldMutex;
use std::sync::{Arc, Mutex};
use std::thread;

use brixo_core::{Class, DataModel, InstanceId, Vec3 as BVec3};
use rovik::ast::Stmt;
use rovik::{Interpreter, RovikError, Trigger, Value};

use crate::host::{object, WorldHost};
use crate::physics::{Physics, PlayerInput};

/// Statements a script may run between waits before it's stopped.
/// Lower than the command line's limit so a runaway loop can't freeze a
/// frame for long.
pub const GAME_STEP_LIMIT: u64 = 1_000_000;
/// Stack for each task thread. Reserved address space, not memory used.
const TASK_STACK_SIZE: usize = 16 * 1024 * 1024;
/// The error a paused wait() gets when the game stops. Never shown.
const STOPPED: &str = "the game was stopped";

/// A player's saved look: their account's colours and face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub skin: brixo_core::Color,
    pub shirt: brixo_core::Color,
    pub pants: brixo_core::Color,
    pub shoes: brixo_core::Color,
    pub face: brixo_core::Face,
    pub hats: [Option<brixo_core::Hat>; brixo_core::MAX_HATS],
}

impl Look {
    fn apply(self, p: &mut brixo_core::PlayerProps) {
        p.skin_color = self.skin;
        p.shirt_color = self.shirt;
        p.pants_color = self.pants;
        p.shoes_color = self.shoes;
        p.face = self.face;
        p.hats = self.hats;
        p.body.color = self.shirt;
    }
}

// --- random avatar colours ---

/// Colours picked for each slot so any combination looks good together:
/// a range of skin tones, bright shirts, darker pants, neutral shoes.
pub const SKIN_TONES: [(u8, u8, u8); 6] = [
    (227, 185, 138), (160, 99, 62), (242, 211, 176), (204, 142, 105), (124, 78, 50), (234, 196, 160),
];
pub const SHIRT_COLORS: [(u8, u8, u8); 8] = [
    (196, 40, 28), (13, 105, 172), (75, 151, 75), (218, 133, 65),
    (107, 50, 124), (245, 205, 48), (27, 42, 53), (0, 143, 156),
];
pub const PANTS_COLORS: [(u8, u8, u8); 6] = [
    (27, 42, 53), (39, 70, 45), (99, 95, 98), (105, 64, 40), (52, 43, 117), (13, 105, 172),
];
pub const SHOES_COLORS: [(u8, u8, u8); 4] = [
    (27, 42, 53), (27, 27, 27), (99, 95, 98), (105, 64, 40),
];

/// A tiny xorshift generator; the avatar doesn't need anything fancier.
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn seeded() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        Rng(nanos | 1)
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        items[(self.0 % items.len() as u64) as usize]
    }
}

pub(crate) fn random_colors(p: &mut brixo_core::PlayerProps, rng: &mut Rng) {
    let c = |(r, g, b): (u8, u8, u8)| brixo_core::Color::new(r, g, b);
    p.skin_color = c(rng.pick(&SKIN_TONES));
    p.shirt_color = c(rng.pick(&SHIRT_COLORS));
    p.pants_color = c(rng.pick(&PANTS_COLORS));
    p.shoes_color = c(rng.pick(&SHOES_COLORS));
    p.body.color = p.shirt_color;
}

/// The Tools directly inside a player, in order.
pub fn backpack(world: &DataModel, player: InstanceId) -> Vec<InstanceId> {
    world
        .get(player)
        .map(|p| p.children.iter().copied().filter(|c| world.get(*c).is_some_and(|i| i.class == Class::Tool)).collect())
        .unwrap_or_default()
}

/// Whether a GUI element is showing: it and every GUI element around it
/// are visible.
pub fn gui_shown(world: &DataModel, id: InstanceId) -> bool {
    let mut at = Some(id);
    while let Some(n) = at {
        if world.gui(n).is_some_and(|g| !g.visible) {
            return false;
        }
        at = world.get(n).and_then(|i| i.parent);
    }
    world.gui(id).is_some()
}

/// Longest chat message, in characters.
pub const CHAT_LIMIT: usize = 120;
/// Shortest gap between one player's messages, in seconds.
pub const CHAT_COOLDOWN: f64 = 0.4;

pub use crate::chat_filter::filter_chat;

/// How long a player stays fallen apart before respawning, in seconds.
pub const RESPAWN_TIME: f32 = 4.0;

/// How long a tool swing animation lasts, in seconds.
pub const SWING_TIME: f32 = brixo_core::SWING_TIME;

/// A part's own turn, in the same order the renderer uses.
fn part_turn(p: &brixo_core::PartProps) -> glam::Quat {
    glam::Quat::from_euler(glam::EulerRot::YXZ, p.rotation.y.to_radians(), p.rotation.x.to_radians(), p.rotation.z.to_radians())
}

/// Players below this height have fallen off the world and respawn.
pub const FALL_LIMIT: f32 = -60.0;

/// Events scripts can use with `on`.
pub const EVENTS: &[&str] = &["touched", "player_joined", "player_left", "clicked", "activated", "died", "respawned", "key"];

/// The keys scripts can hear with `on key(player, key)`: letters the game
/// doesn't use for moving (WASD), the camera (I, O) or the tools (1-9).
pub const SCRIPT_KEYS: &[&str] = &["e", "q", "f", "r", "g", "z", "x", "c", "v", "b"];

#[derive(Debug, Clone, PartialEq)]
pub struct LogLine {
    /// Which script, like "Coin/Spin".
    pub source: String,
    pub text: String,
    pub is_error: bool,
}

enum TaskMsg {
    Wait(f64),
    Done(Result<(), RovikError>),
}

enum Job {
    Run(Arc<Vec<Stmt>>),
    Call(Value, Vec<Value>),
}

struct Task {
    id: u64,
    script: usize,
    wake_at: f64,
    resume: Sender<()>,
    messages: Receiver<TaskMsg>,
}

struct Timer {
    function: Value,
    interval: f64,
    next: f64,
    /// The task from this timer's last run, if it's still going. A timer
    /// won't start again until its previous run has finished.
    running: Option<u64>,
}

struct ScriptInfo {
    id: InstanceId,
    label: String,
    /// The part (or other instance) the script sits inside.
    parent: Option<InstanceId>,
    /// Never runs itself; each task runs on a fork of it, so they all share
    /// the script's variables.
    base: Interpreter,
    handlers_seen: usize,
    touch_handlers: Vec<Value>,
    joined_handlers: Vec<Value>,
    left_handlers: Vec<Value>,
    died_handlers: Vec<Value>,
    /// `on respawned(player)`: back from a knockout, at their spawn.
    respawned_handlers: Vec<Value>,
    clicked_handlers: Vec<Value>,
    activated_handlers: Vec<Value>,
    /// `on key(player, key)`.
    key_handlers: Vec<Value>,
    timers: Vec<Timer>,
}

pub struct Game {
    world: Arc<WorldMutex<DataModel>>,
    clock: Arc<Mutex<f64>>,
    log: Arc<Mutex<Vec<LogLine>>>,
    sounds: Arc<Mutex<Vec<crate::host::SoundEvent>>>,
    blasts: Arc<Mutex<Vec<crate::host::Blast>>>,
    /// Saved player data (see saves.rs), and when it was last written out.
    saves: Arc<Mutex<crate::saves::Saves>>,
    last_save: f64,
    hinge_angles: Arc<Mutex<HashMap<InstanceId, f32>>>,
    /// What scripts asked of karts (boost, spin out, place), and bots they
    /// added, waiting for the next step.
    kart_commands: Arc<Mutex<Vec<crate::physics::KartCommand>>>,
    new_bots: Arc<Mutex<Vec<InstanceId>>>,
    /// Bots, and how far round the racing line each has got.
    bots: HashMap<InstanceId, crate::kart::BotDriver>,
    /// Players whose kart drives itself like a bot's (for filming).
    autopilot: HashMap<InstanceId, crate::kart::BotDriver>,
    /// Explosion fireballs on screen: the part, when it started, its radius.
    fireballs: Vec<(InstanceId, f64, f32)>,
    scripts: Vec<ScriptInfo>,
    known_scripts: HashSet<InstanceId>,
    tasks: Vec<Task>,
    next_task: u64,
    physics: Physics,
    /// What each player is pressing.
    inputs: HashMap<InstanceId, PlayerInput>,
    /// The player on this machine, in single-player games.
    local_player: Option<InstanceId>,
    /// Everyone playing, in the order they joined.
    players: Vec<InstanceId>,
    /// Chat messages since the last take_chat(): (player, name, text).
    chat: Vec<(InstanceId, String, String)>,
    /// When each player last chatted, for the spam limit.
    last_chat: HashMap<InstanceId, f64>,
    /// How each held tool's parts sit around its first part (the handle),
    /// as it was built: offset and extra turn, in the holder's frame.
    grips: HashMap<InstanceId, Vec<(InstanceId, glam::Vec3, glam::Quat)>>,
    /// Where each team's SpawnLocation was last seen standing: a team whose
    /// pad got knocked off the world still comes back home.
    team_spawns: HashMap<String, BVec3>,
    /// When loose parts that fell off the world were last cleared away.
    last_sweep: f64,
    /// Players whose `player_joined` hasn't fired yet (scripts weren't
    /// running when they arrived).
    unannounced: Vec<InstanceId>,
    /// Where the player appears, on top of the first SpawnLocation.
    spawn_point: BVec3,
    time: f64,
}

impl Game {
    /// Starts a game on a copy of the scene. Every enabled script runs its
    /// body until it first waits or finishes.
    /// Starts a single-player game: you're the one player, named "Player".
    pub fn start(model: DataModel) -> Game {
        Game::start_with_store(model, Arc::new(crate::saves::MemoryStore::default()))
    }

    /// A single-player game whose saved data (save/load) goes in `store`
    /// (Studio keeps one for as long as it's open).
    pub fn start_with_store(model: DataModel, store: Arc<dyn crate::saves::SaveStore>) -> Game {
        let mut game = Game::new(model);
        game.saves.lock().unwrap().set_store(store);
        // The player exists before scripts start, so they can find it.
        let id = game.spawn_player("Player", None);
        game.saves.lock().unwrap().open(id, "Player");
        game.local_player = Some(id);
        game.discover_scripts();
        game.announce_players();
        game
    }

    /// Starts a game for a server: no players until they join.
    pub fn start_server(model: DataModel) -> Game {
        let mut game = Game::new(model);
        game.discover_scripts();
        game
    }

    fn new(model: DataModel) -> Game {
        let game = Game {
            world: Arc::new(WorldMutex::new(model)),
            clock: Arc::new(Mutex::new(0.0)),
            sounds: Arc::new(Mutex::new(Vec::new())),
            blasts: Arc::new(Mutex::new(Vec::new())),
            saves: Arc::new(Mutex::new(crate::saves::Saves::new(Arc::new(crate::saves::MemoryStore::default())))),
            last_save: 0.0,
            hinge_angles: Arc::new(Mutex::new(HashMap::new())),
            kart_commands: Arc::new(Mutex::new(Vec::new())),
            new_bots: Arc::new(Mutex::new(Vec::new())),
            bots: HashMap::new(),
            autopilot: HashMap::new(),
            fireballs: Vec::new(),
            log: Arc::new(Mutex::new(Vec::new())),
            scripts: Vec::new(),
            known_scripts: HashSet::new(),
            tasks: Vec::new(),
            next_task: 0,
            physics: Physics::new(),
            inputs: HashMap::new(),
            local_player: None,
            players: Vec::new(),
            grips: HashMap::new(),
            team_spawns: HashMap::new(),
            last_sweep: 0.0,
            chat: Vec::new(),
            last_chat: HashMap::new(),
            unannounced: Vec::new(),
            spawn_point: BVec3::new(0.0, 10.0, 0.0),
            time: 0.0,
        };
        game
    }

    /// What the local player is pressing. Call before each step().
    pub fn set_input(&mut self, input: PlayerInput) {
        if let Some(id) = self.local_player {
            self.inputs.insert(id, input);
        }
    }

    /// What one player is pressing (servers: one per connected player).
    pub fn set_input_for(&mut self, player: InstanceId, input: PlayerInput) {
        self.inputs.insert(player, input);
    }

    /// Shift lock for the local player: face `yaw` (radians, 0 = +Z, the
    /// camera's way) whichever way they walk, or None to face where they
    /// walk again.
    pub fn set_facing(&mut self, yaw: Option<f32>) {
        if let Some(id) = self.local_player {
            self.physics.hold_facing(id, yaw);
        }
    }

    /// Shift lock for one player (servers: from that player's client).
    pub fn set_facing_for(&mut self, player: InstanceId, yaw: Option<f32>) {
        self.physics.hold_facing(player, yaw);
    }

    /// The local player, in single-player games.
    pub fn player_id(&self) -> Option<InstanceId> {
        self.local_player
    }

    /// Everyone playing, in the order they joined.
    pub fn players(&self) -> &[InstanceId] {
        &self.players
    }

    /// The local player's character position, for the camera to follow.
    pub fn player_position(&self) -> Option<glam::Vec3> {
        self.physics.character_position(self.local_player?)
    }

    pub fn player_grounded(&self) -> bool {
        self.local_player.is_some_and(|id| self.physics.character_grounded(id))
    }

    /// Where a player's character is.
    pub fn position_of(&self, player: InstanceId) -> Option<glam::Vec3> {
        self.physics.character_position(player)
    }

    /// The world, shared: servers hand this to whatever wants to watch.
    pub fn shared_world(&self) -> Arc<WorldMutex<DataModel>> {
        self.world.clone()
    }

    /// A new player joins: they appear at the spawn and scripts hear
    /// `on player_joined(player)`.
    pub fn add_player(&mut self, name: &str) -> InstanceId {
        self.add_player_as(name, None)
    }

    /// A player joins wearing a specific look (their account's avatar)
    /// instead of a random one.
    pub fn add_player_as(&mut self, name: &str, look: Option<Look>) -> InstanceId {
        self.add_player_saved(name, look, None)
    }

    /// A player joins, with their saved data kept under `save_key` (their
    /// account, online) or, if None, under their name.
    pub fn add_player_saved(&mut self, name: &str, look: Option<Look>, save_key: Option<&str>) -> InstanceId {
        let id = self.spawn_player(name, look);
        // Loaded before player_joined fires, so scripts can load() there.
        let key = save_key.map(str::to_string).unwrap_or_else(|| self.world.lock().get(id).map(|i| i.name.clone()).unwrap_or_default());
        self.saves.lock().unwrap().open(id, &key);
        self.announce_players();
        id
    }

    /// Where saved player data (save/load) is kept. Set before anyone joins.
    pub fn set_save_store(&mut self, store: Arc<dyn crate::saves::SaveStore>) {
        self.saves.lock().unwrap().set_store(store);
    }

    /// Writes all changed saved data to the store now (a server shutting
    /// down; it also happens every few seconds and when players leave).
    pub fn flush_saves(&mut self) {
        self.saves.lock().unwrap().flush();
    }

    /// A player leaves. Scripts hear `on player_left(player)` while the
    /// player still exists, then they're removed from the world.
    pub fn remove_player(&mut self, player: InstanceId) {
        if !self.players.contains(&player) {
            return;
        }
        self.fire_everywhere(|s| &s.left_handlers, player);
        // After player_left, so what it saves is kept.
        self.saves.lock().unwrap().close(player);
        self.players.retain(|p| *p != player);
        self.unannounced.retain(|p| *p != player);
        self.inputs.remove(&player);
        if self.local_player == Some(player) {
            self.local_player = None;
        }
        self.world.lock().remove(player);
    }

    // --- chat ---

    /// A player says something. It's trimmed, capped, filtered and
    /// rate-limited; returns what everyone will see, or None if dropped.
    pub fn chat(&mut self, player: InstanceId, text: &str) -> Option<String> {
        if !self.players.contains(&player) {
            return None;
        }
        let text: String = text.trim().chars().filter(|c| !c.is_control()).take(CHAT_LIMIT).collect();
        if text.is_empty() {
            return None;
        }
        let now = self.time;
        if self.last_chat.get(&player).is_some_and(|t| now - t < CHAT_COOLDOWN) {
            return None;
        }
        self.last_chat.insert(player, now);
        let clean = filter_chat(&text);
        let name = self.world.lock().get(player).map(|i| i.name.clone()).unwrap_or_default();
        self.chat.push((player, name, clean.clone()));
        Some(clean)
    }

    /// Chat messages since the last call: (player, name, text).
    pub fn take_chat(&mut self) -> Vec<(InstanceId, String, String)> {
        std::mem::take(&mut self.chat)
    }

    // --- GUI and tools ---

    /// A player clicked a TextButton: scripts inside it hear
    /// `on clicked(player)`. Buttons inside another player's GUI (or
    /// hidden ones) can't be clicked, whatever a client claims.
    pub fn click(&mut self, player: InstanceId, button: InstanceId) -> bool {
        let ok = {
            let world = self.world.lock();
            let is_button = world.get(button).is_some_and(|i| i.class == Class::TextButton);
            let owner_ok = world.player_of(button).is_none_or(|p| p == player);
            is_button && owner_ok && self.players.contains(&player) && gui_shown(&world, button)
        };
        if ok {
            self.fire_where(|s| &s.clicked_handlers, |_, parent| parent == Some(button), player);
        }
        ok
    }

    /// A player pressed a key: scripts hear `on key(player, key)`. Only the
    /// keys in SCRIPT_KEYS count (whatever a client claims).
    pub fn key(&mut self, player: InstanceId, key: &str) -> bool {
        let key = key.to_lowercase();
        if !SCRIPT_KEYS.contains(&key.as_str()) || !self.players.contains(&player) {
            return false;
        }
        let mut calls = Vec::new();
        for (s, script) in self.scripts.iter().enumerate() {
            for handler in &script.key_handlers {
                calls.push((s, handler.clone()));
            }
        }
        for (s, handler) in calls {
            let wanted = handler.param_count().unwrap_or(2);
            let args: Vec<Value> = [object(player), Value::str(key.as_str())].into_iter().take(wanted).collect();
            self.spawn(s, Job::Call(handler, args));
        }
        true
    }

    /// Lets a player's kart drive itself along the racing line, as a
    /// bot's does (filming demos), or gives them the wheel back.
    pub fn set_autopilot(&mut self, player: InstanceId, on: bool) {
        if on {
            self.autopilot.entry(player).or_default();
        } else {
            self.autopilot.remove(&player);
        }
    }

    /// Whether a player is a bot (made by a script's `add_bot`).
    pub fn is_bot(&self, player: InstanceId) -> bool {
        self.bots.contains_key(&player)
    }

    /// The Tools in a player's backpack, in order (keys 1-9).
    pub fn backpack(&self, player: InstanceId) -> Vec<InstanceId> {
        backpack(&self.world.lock(), player)
    }

    /// Holds the tool in backpack slot `slot` (0-based), or puts it away if
    /// it's already held. None puts away whatever's held.
    pub fn equip(&mut self, player: InstanceId, slot: Option<usize>) {
        let mut world = self.world.lock();
        let tools = backpack(&world, player);
        let Some(p) = world.player(player) else { return };
        let current = p.equipped;
        let next = slot.and_then(|i| tools.get(i).copied()).filter(|t| Some(*t) != current);
        if let Some(tool) = next {
            // Remember how the tool is built, once: each part's offset from
            // the handle (its first part) and its own turn, as authored,
            // with the tool pointing along +Z.
            if !self.grips.contains_key(&tool) {
                let parts = world.parts_under(tool);
                if let Some(&handle) = parts.first() {
                    let h = world.part(handle).unwrap().position;
                    let grip = parts
                        .iter()
                        .map(|id| {
                            let q = world.part(*id).unwrap();
                            let off = glam::Vec3::new(q.position.x - h.x, q.position.y - h.y, q.position.z - h.z);
                            (*id, off, part_turn(q))
                        })
                        .collect();
                    self.grips.insert(tool, grip);
                }
            }
            for id in world.parts_under(tool) {
                let q = world.part_mut(id).unwrap();
                q.anchored = true;
                q.can_collide = false;
            }
        }
        world.player_mut(player).unwrap().equipped = next;
    }

    /// The player clicked with a tool in hand: scripts in the tool hear
    /// `on activated(player)`. (Without a mouse point: they aim straight
    /// ahead. See `activate_at`.)
    pub fn activate(&mut self, player: InstanceId) -> bool {
        self.activate_at(player, None)
    }

    /// Clicked with a tool in hand, the mouse pointing at `aim` in the
    /// world: scripts read it as `player.mouse`, and the character turns to
    /// face it, the way a gear aims where you click.
    pub fn activate_at(&mut self, player: InstanceId, aim: Option<BVec3>) -> bool {
        let tool = {
            let world = self.world.lock();
            world.player(player).and_then(|p| p.equipped).filter(|t| world.player_of(*t) == Some(player))
        };
        let Some(tool) = tool else { return false };
        if self.world.lock().player(player).is_some_and(|p| p.dead > 0.0) {
            return false; // the dead don't swing
        }
        let mut turn = None;
        if let Some(p) = self.world.lock().player_mut(player) {
            p.swing = SWING_TIME;
            let at = p.body.position;
            let aim = aim.filter(|a| a.x.is_finite() && a.y.is_finite() && a.z.is_finite());
            match aim {
                Some(a) => {
                    p.mouse = a;
                    let (dx, dz) = (a.x - at.x, a.z - at.z);
                    if dx * dx + dz * dz > 0.25 {
                        let yaw = dx.atan2(dz);
                        p.body.rotation.y = yaw.to_degrees();
                        turn = Some(yaw);
                    }
                }
                None => {
                    // No mouse: a point far straight ahead, at chest height.
                    let yaw = p.body.rotation.y.to_radians();
                    p.mouse = BVec3::new(at.x + yaw.sin() * 100.0, at.y + 1.0, at.z + yaw.cos() * 100.0);
                }
            }
        }
        // (Physics owns which way a character faces: tell it too, or the
        // next step would turn them back.)
        if let Some(yaw) = turn {
            self.physics.face(player, yaw);
        }
        let world = self.world.clone();
        self.fire_where(
            |s| &s.activated_handlers,
            move |_, parent| parent.is_some_and(|p| world.lock().tool_of(p) == Some(tool)),
            player,
        );
        true
    }

    /// Puts held tools in their holders' hands, and backpack tools away.
    fn place_tools(&mut self, dt: f32) {
        let mut world = self.world.lock();
        for &player in &self.players {
            if let Some(p) = world.player_mut(player) {
                p.swing = (p.swing - dt).max(0.0);
            }
        }
        for &player in &self.players {
            let Some(p) = world.player(player).copied() else { continue };
            let yaw = p.body.rotation.y.to_radians();
            let body_turn = glam::Quat::from_rotation_y(yaw);
            let c = glam::Vec3::new(p.body.position.x, p.body.position.y, p.body.position.z);
            for tool in backpack(&world, player) {
                let held = p.equipped == Some(tool);
                // The arm the renderer draws: the tool sits in its hand and
                // runs along it (a tool is authored pointing along +Z, which
                // is where the arm points when held forward).
                let theta = brixo_core::held_arm_angle(p.swing, brixo_core::holds_up(&world, tool));
                let s = brixo_core::RIGHT_SHOULDER;
                let hand_local = glam::Vec3::new(s.x, s.y, s.z) + glam::Quat::from_rotation_x(theta) * glam::Vec3::new(0.0, -brixo_core::ARM_REACH, 0.0);
                let hand = c + body_turn * hand_local;
                let tool_turn = body_turn * glam::Quat::from_rotation_x(theta + std::f32::consts::FRAC_PI_2);
                let grip = self.grips.get(&tool).cloned().unwrap_or_default();
                for (id, off, local) in grip {
                    let Some(q) = world.part_mut(id) else { continue };
                    if held {
                        let at = hand + tool_turn * off;
                        q.position = BVec3::new(at.x, at.y, at.z);
                        let (y, x, z) = (tool_turn * local).to_euler(glam::EulerRot::YXZ);
                        q.rotation = BVec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
                    } else {
                        // In the backpack: out of sight, out of the way.
                        q.position = BVec3::new(c.x, -500.0, c.z);
                    }
                }
            }
            // A held tool that's gone (destroyed, or given away) isn't held.
            if p.equipped.is_some_and(|t| world.player_of(t) != Some(player)) {
                world.player_mut(player).unwrap().equipped = None;
            }
        }
    }

    /// Runs a player event in scripts chosen by `wants(script, parent)`.
    fn fire_where(
        &mut self,
        handlers: fn(&ScriptInfo) -> &Vec<Value>,
        wants: impl Fn(usize, Option<InstanceId>) -> bool,
        player: InstanceId,
    ) {
        let mut calls = Vec::new();
        for (s, script) in self.scripts.iter().enumerate() {
            if !wants(s, script.parent) {
                continue;
            }
            for handler in handlers(script) {
                calls.push((s, handler.clone()));
            }
        }
        for (s, handler) in calls {
            let wanted = handler.param_count().unwrap_or(1);
            let args = if wanted >= 1 { vec![object(player)] } else { Vec::new() };
            self.spawn(s, Job::Call(handler, args));
        }
    }

    fn announce_players(&mut self) {
        for player in std::mem::take(&mut self.unannounced) {
            self.fire_everywhere(|s| &s.joined_handlers, player);
        }
    }

    /// Runs a player event in every script that listens for it.
    fn fire_everywhere(&mut self, handlers: fn(&ScriptInfo) -> &Vec<Value>, player: InstanceId) {
        let mut calls = Vec::new();
        for (s, script) in self.scripts.iter().enumerate() {
            for handler in handlers(script) {
                calls.push((s, handler.clone()));
            }
        }
        for (s, handler) in calls {
            let wanted = handler.param_count().unwrap_or(1);
            let args = if wanted >= 1 { vec![object(player)] } else { Vec::new() };
            self.spawn(s, Job::Call(handler, args));
        }
    }

    fn spawn_player(&mut self, name: &str, look: Option<Look>) -> InstanceId {
        let mut world = self.world.lock();
        let spawn = world
            .walk()
            .into_iter()
            .find(|id| world.get(*id).map(|i| i.class) == Some(Class::SpawnLocation));
        if let Some(pad) = spawn.and_then(|id| world.part(id)) {
            // Standing on top of the pad: its top, plus half the character's height.
            self.spawn_point = BVec3::new(
                pad.position.x,
                pad.position.y + pad.size.y / 2.0 + 2.55,
                pad.position.z,
            );
        }
        let root = world.root();
        let id = world.create(Class::Player, name, root).expect("the workspace holds players");
        let spot = self.spawn_spot(self.players.len());
        if let Some(p) = world.player_mut(id) {
            p.body.position = spot;
            match look {
                Some(l) => l.apply(p),
                None => random_colors(p, &mut Rng::seeded()),
            }
        }
        self.players.push(id);
        self.unannounced.push(id);
        id
    }

    /// Where a player respawns: on the SpawnLocation whose `team` matches
    /// theirs if there is one, otherwise the usual spawn.
    fn respawn_spot(&self, world: &DataModel, player: InstanceId, n: usize) -> BVec3 {
        let team = |id: InstanceId| match world.get(id).and_then(|i| i.attributes.get("team")) {
            Some(brixo_core::Attribute::Str(t)) => Some(t.clone()),
            _ => None,
        };
        let Some(mine) = team(player) else { return self.spawn_spot(n) };
        let pad = world
            .walk()
            .into_iter()
            .find(|id| world.get(*id).is_some_and(|i| i.class == Class::SpawnLocation) && team(*id).as_ref() == Some(&mine));
        const OFFSETS: [(f32, f32); 5] = [(0.0, 0.0), (2.0, 0.0), (-2.0, 0.0), (0.0, 2.0), (0.0, -2.0)];
        let (dx, dz) = OFFSETS[n % OFFSETS.len()];
        // A pad that fell off the world doesn't count: respawning on it
        // would only fall again, forever.
        match pad.and_then(|id| world.part(id)).filter(|p| p.position.y > FALL_LIMIT + 10.0) {
            Some(p) => BVec3::new(p.position.x + dx, p.position.y + p.size.y / 2.0 + 2.55, p.position.z + dz),
            None => match self.team_spawns.get(&mine) {
                Some(home) => BVec3::new(home.x + dx, home.y, home.z + dz),
                None => self.spawn_spot(n),
            },
        }
    }

    /// Where the n-th player to join stands: spread over the spawn pad in a
    /// 3x3 pattern (centre first), so players don't start inside each other.
    fn spawn_spot(&self, n: usize) -> BVec3 {
        const OFFSETS: [(f32, f32); 9] =
            [(0.0, 0.0), (2.5, 0.0), (-2.5, 0.0), (0.0, 2.5), (0.0, -2.5), (2.5, 2.5), (-2.5, -2.5), (2.5, -2.5), (-2.5, 2.5)];
        let (dx, dz) = OFFSETS[n % OFFSETS.len()];
        BVec3::new(self.spawn_point.x + dx, self.spawn_point.y, self.spawn_point.z + dz)
    }

    /// Out of health (or off the world): the player falls apart, everyone
    /// hears it and scripts hear `on died(player)`. A few seconds later they
    /// respawn at the spawn point with full health.
    fn check_respawn(&mut self, dt: f32) {
        self.remember_team_spawns();
        let mut died = Vec::new();
        let mut respawned = Vec::new();
        {
            let mut world = self.world.lock();
            let spots: HashMap<InstanceId, BVec3> =
                self.players.iter().enumerate().map(|(i, id)| (*id, self.respawn_spot(&world, *id, i))).collect();
            for &id in &self.players {
                let name = world.get(id).map(|i| i.name.clone()).unwrap_or_default();
                let Some(p) = world.player_mut(id) else { continue };
                if p.dead > 0.0 {
                    p.dead += dt;
                    if p.dead >= RESPAWN_TIME {
                        p.dead = 0.0;
                        p.health = p.max_health;
                        p.body.position = spots[&id];
                        respawned.push((id, name));
                    }
                } else if p.health <= 0.0 || p.body.position.y < FALL_LIMIT {
                    let fell = p.health > 0.0;
                    p.health = 0.0;
                    p.dead = 0.0001;
                    p.equipped = None;
                    p.kart = None;
                    died.push((id, name, fell));
                }
            }
        }
        for (id, name, fell) in died {
            self.push_log("Brixo", format!("{name} {}", if fell { "fell off the world" } else { "died" }), false);
            // Heard from where they fell apart.
            let position = brixo_core::object_position(&self.world.lock(), id).unwrap_or_default();
            let at = Some(brixo_core::SoundAt { object: Some(id), position });
            self.sounds.lock().unwrap().push(crate::host::SoundEvent::Play { name: "death".into(), player: None, at });
            self.fire_everywhere(|s| &s.died_handlers, id);
        }
        for (id, name) in respawned {
            self.push_log("Brixo", format!("{name} respawned"), false);
            self.fire_everywhere(|s| &s.respawned_handlers, id);
        }
    }

    /// Before physics: bots scripts added join the game; kart commands from
    /// scripts are carried out; each kart gets its driver's keys (or a
    /// bot's driving).
    fn drive_karts(&mut self, dt: f32) {
        for bot in std::mem::take(&mut *self.new_bots.lock().unwrap()) {
            if !self.players.contains(&bot) {
                self.players.push(bot);
            }
            self.bots.insert(bot, crate::kart::BotDriver::default());
        }
        for c in std::mem::take(&mut *self.kart_commands.lock().unwrap()) {
            // A kart the engine hasn't picked up yet (placed as the game
            // starts): move its chassis, which it'll start from.
            if let crate::physics::KartCommand::Place(k, at, yaw) = c {
                if self.physics.kart_state(k).is_none() {
                    let mut world = self.world.lock();
                    // (Where its parts sit on it, first, from where it was built.)
                    crate::kart::record_offsets(&mut world, k);
                    if let Some(p) = brixo_core::kart_chassis(&world, k).and_then(|ch| world.part_mut(ch)) {
                        p.position = BVec3::new(at.x, at.y, at.z);
                        p.rotation = BVec3::new(0.0, yaw, 0.0);
                    }
                    crate::kart::pose_parts(&mut world, k);
                    continue;
                }
            }
            self.physics.kart_command(c);
        }
        let world = self.world.lock();
        let line = crate::kart::racing_line(&world);
        let mut inputs = HashMap::new();
        let all_karts: Vec<(InstanceId, glam::Vec3)> = crate::kart::karts(&world)
            .into_iter()
            .filter_map(|k| Some((k, self.physics.kart_state(k)?.0)))
            .collect();
        for &player in &self.players {
            let Some(p) = world.player(player) else { continue };
            let Some(kart) = p.kart.filter(|k| brixo_core::is_kart(&world, *k)) else { continue };
            let driver = match self.bots.get_mut(&player) {
                Some(d) => Some(d),
                None => self.autopilot.get_mut(&player),
            };
            let input = if let Some(driver) = driver {
                let Some((at, yaw, speed)) = self.physics.kart_state(kart) else { continue };
                let lane = match world.get(player).and_then(|i| i.attributes.get("lane")) {
                    Some(brixo_core::Attribute::Num(n)) => *n as f32,
                    _ => 0.0,
                };
                let others: Vec<glam::Vec3> = all_karts.iter().filter(|(k, _)| *k != kart).map(|(_, p)| *p).collect();
                driver.drive(&line, at, yaw, speed, lane, &others, dt)
            } else {
                crate::kart::KartInput::from_player(self.inputs.get(&player).copied().unwrap_or_default())
            };
            inputs.insert(kart, input);
        }
        drop(world);
        self.physics.set_kart_inputs(inputs);
    }

    /// Carries out explosions scripts set off: throws loose parts and shows
    /// a fireball (players in range were already knocked out by `explode`).
    fn set_off_blasts(&mut self) {
        let blasts: Vec<crate::host::Blast> = std::mem::take(&mut *self.blasts.lock().unwrap());
        for b in blasts {
            let center = glam::Vec3::new(b.center.x, b.center.y, b.center.z);
            self.physics.blast(center, b.radius, b.power);
            let mut world = self.world.lock();
            let root = world.root();
            if let Some(id) = world.create(Class::Part, "Explosion", root) {
                let p = world.part_mut(id).unwrap();
                p.position = b.center;
                p.size = BVec3::new(1.0, 1.0, 1.0);
                p.shape = brixo_core::Shape::Ball;
                p.material = brixo_core::Material::Neon;
                p.color = brixo_core::Color::new(255, 150, 30);
                p.anchored = true;
                p.can_collide = false;
                self.fireballs.push((id, self.time, b.radius));
            }
        }
        // Fireballs swell and fade over half a second, then vanish.
        let now = self.time;
        let mut world = self.world.lock();
        self.fireballs.retain(|(id, start, radius)| {
            let t = ((now - start) / 0.5) as f32;
            if t >= 1.0 {
                world.remove(*id);
                return false;
            }
            if let Some(p) = world.part_mut(*id) {
                let d = radius * 2.0 * (0.25 + 0.75 * t.sqrt());
                p.size = BVec3::new(d, d, d);
                p.transparency = t.powf(1.5);
                p.color = brixo_core::Color::new(255, (150.0 + 100.0 * t) as u8, (30.0 + 120.0 * t) as u8);
            }
            true
        });
    }

    /// Advances the game by `dt` seconds.
    pub fn step(&mut self, dt: f64) {
        self.time += dt.max(0.0);
        *self.clock.lock().unwrap() = self.time;

        self.resume_due_tasks();
        self.stop_removed_scripts();
        self.discover_scripts();
        self.run_timers();
        self.set_off_blasts();
        self.run_physics(dt);
        self.place_tools(dt as f32);
        self.carry_parts();
        if self.time - self.last_save >= crate::saves::SAVE_EVERY {
            self.last_save = self.time;
            self.flush_saves();
        }
    }

    /// Parts with `carried_by = "<player name>"` ride along with that
    /// player: a flag on their back, a hat, a backpack. `carry_x`,
    /// `carry_y` and `carry_z` place it in the player's own frame (+Z
    /// ahead, +Y up), and it turns as they turn. While the player is
    /// knocked out, it stays where they fell; the game decides what then.
    fn carry_parts(&mut self) {
        let mut world = self.world.lock();
        let carried: Vec<(InstanceId, String)> = world
            .walk()
            .into_iter()
            .filter(|id| world.part(*id).is_some())
            .filter_map(|id| match world.get(id)?.attributes.get("carried_by") {
                Some(brixo_core::Attribute::Str(name)) => Some((id, name.clone())),
                _ => None,
            })
            .collect();
        if carried.is_empty() {
            return;
        }
        let num = |world: &DataModel, id: InstanceId, key: &str| match world.get(id).and_then(|i| i.attributes.get(key)) {
            Some(brixo_core::Attribute::Num(n)) => *n as f32,
            _ => 0.0,
        };
        for (id, name) in carried {
            let Some(player) = self.players.iter().copied().find(|p| world.get(*p).is_some_and(|i| i.name == name)) else { continue };
            let Some(p) = world.player(player).copied() else { continue };
            if p.dead > 0.0 {
                continue;
            }
            let yaw = p.body.rotation.y;
            let turn = glam::Quat::from_rotation_y(yaw.to_radians());
            let offset = glam::Vec3::new(num(&world, id, "carry_x"), num(&world, id, "carry_y"), num(&world, id, "carry_z"));
            let at = glam::Vec3::new(p.body.position.x, p.body.position.y, p.body.position.z) + turn * offset;
            if let Some(q) = world.part_mut(id) {
                q.position = BVec3::new(at.x, at.y, at.z);
                q.rotation.y = yaw;
            }
        }
    }

    /// Sounds and music scripts played since the last call.
    pub fn take_sounds(&self) -> Vec<crate::host::SoundEvent> {
        std::mem::take(&mut *self.sounds.lock().unwrap())
    }

    /// The live world, for drawing. Don't hold this across step().
    pub fn world(&self) -> WorldMutexGuard<'_, DataModel> {
        self.world.lock()
    }

    /// Like `world()`, but never hangs indefinitely: panics with a clear
    /// message if the lock isn't free within `timeout`. Every real caller
    /// (the player, studio, and server) calls `world()` from the same
    /// thread that drives `step()`, so contention should be brief; this
    /// exists so a genuine stall shows up as a loud, diagnosable failure
    /// instead of a silent hang, and so tests and tools can call it safely
    /// from a different thread than the one driving the game.
    #[track_caller]
    pub fn world_within(&self, timeout: std::time::Duration) -> WorldMutexGuard<'_, DataModel> {
        match self.world.try_lock_for(timeout) {
            Some(guard) => guard,
            None => panic!(
                "Game::world() couldn't get the lock within {timeout:?}. Usually this is the \
                 same thread still holding it: a guard kept alive by an `if let`, `match` or \
                 one long statement that calls world() twice. Bind the first result to a \
                 variable first."
            ),
        }
    }

    /// Output since the last call.
    pub fn take_log(&self) -> Vec<LogLine> {
        std::mem::take(&mut *self.log.lock().unwrap())
    }

    pub fn time(&self) -> f64 {
        self.time
    }

    /// How many tasks are paused in wait() right now.
    pub fn waiting_tasks(&self) -> usize {
        self.tasks.len()
    }

    // --- scripts ---

    /// Starts any enabled script not seen yet (at the start of the game, or
    /// one a script created with clone()).
    fn discover_scripts(&mut self) {
        let found: Vec<(InstanceId, String, Option<InstanceId>, String)> = {
            let world = self.world.lock();
            world
                .walk()
                .into_iter()
                .filter(|id| !self.known_scripts.contains(id))
                .filter_map(|id| {
                    let inst = world.get(id)?;
                    let script = world.script(id)?;
                    if !script.enabled {
                        return None;
                    }
                    let label = match inst.parent.and_then(|p| world.get(p)) {
                        Some(parent) if parent.class != Class::Workspace => {
                            format!("{}/{}", parent.name, inst.name)
                        }
                        _ => inst.name.clone(),
                    };
                    Some((id, label, inst.parent, script.source.clone()))
                })
                .collect()
        };

        for (id, label, parent, source) in found {
            self.known_scripts.insert(id);
            self.start_script(id, label, parent, &source);
        }
    }

    fn start_script(&mut self, id: InstanceId, label: String, parent: Option<InstanceId>, source: &str) {
        let program = match rovik::lexer::lex(source).and_then(rovik::parser::parse) {
            Ok(program) => program,
            Err(e) => {
                self.push_log(&label, e.to_string(), true);
                return;
            }
        };

        let mut base = Interpreter::new();
        base.step_limit = GAME_STEP_LIMIT;
        base.host = Some(Arc::new(WorldHost {
            world: self.world.clone(),
            clock: self.clock.clone(),
            sounds: self.sounds.clone(),
            blasts: self.blasts.clone(),
            saves: self.saves.clone(),
            hinge_angles: self.hinge_angles.clone(),
            kart_commands: self.kart_commands.clone(),
            new_bots: self.new_bots.clone(),
        }));
        let log = self.log.clone();
        let source_label = label.clone();
        base.on_print = Some(Arc::new(move |text: &str| {
            log.lock().unwrap().push(LogLine {
                source: source_label.clone(),
                text: text.to_string(),
                is_error: false,
            });
        }));
        if let Some(p) = parent {
            base.define_global("self", object(p));
        }
        if let Some(player) = self.local_player {
            base.define_global("camera", crate::host::facet_object(player, crate::host::FACET_CAMERA));
        }

        self.scripts.push(ScriptInfo {
            id,
            label,
            parent,
            base,
            handlers_seen: 0,
            touch_handlers: Vec::new(),
            joined_handlers: Vec::new(),
            left_handlers: Vec::new(),
            died_handlers: Vec::new(),
            respawned_handlers: Vec::new(),
            clicked_handlers: Vec::new(),
            activated_handlers: Vec::new(),
            key_handlers: Vec::new(),
            timers: Vec::new(),
        });
        let index = self.scripts.len() - 1;
        self.spawn(index, Job::Run(Arc::new(program)));
    }

    /// If a script is destroyed, its paused tasks stop too.
    fn stop_removed_scripts(&mut self) {
        let alive: Vec<bool> = {
            let world = self.world.lock();
            self.scripts.iter().map(|s| world.get(s.id).is_some()).collect()
        };
        // Dropping a task drops its channel, which ends its wait() with an error.
        self.tasks.retain(|t| alive[t.script]);
        for (script, is_alive) in self.scripts.iter_mut().zip(&alive) {
            if !is_alive {
                script.touch_handlers.clear();
                script.joined_handlers.clear();
                script.left_handlers.clear();
                script.died_handlers.clear();
                script.respawned_handlers.clear();
                script.clicked_handlers.clear();
                script.activated_handlers.clear();
                script.key_handlers.clear();
                script.timers.clear();
            }
        }
    }

    /// Picks up `on` and `every` blocks a script has registered.
    fn collect_handlers(&mut self, index: usize) {
        let handlers = self.scripts[index].base.handlers();
        let seen = self.scripts[index].handlers_seen;
        let label = self.scripts[index].label.clone();
        for handler in &handlers[seen..] {
            match &handler.trigger {
                Trigger::Event(name) if name == "touched" => {
                    self.scripts[index].touch_handlers.push(handler.function.clone());
                }
                Trigger::Event(name) if name == "player_joined" => {
                    self.scripts[index].joined_handlers.push(handler.function.clone());
                }
                Trigger::Event(name) if name == "player_left" => {
                    self.scripts[index].left_handlers.push(handler.function.clone());
                }
                Trigger::Event(name) if name == "died" => {
                    self.scripts[index].died_handlers.push(handler.function.clone());
                }
                Trigger::Event(name) if name == "respawned" => {
                    self.scripts[index].respawned_handlers.push(handler.function.clone());
                }
                Trigger::Event(name) if name == "clicked" => {
                    self.scripts[index].clicked_handlers.push(handler.function.clone());
                }
                Trigger::Event(name) if name == "activated" => {
                    self.scripts[index].activated_handlers.push(handler.function.clone());
                }
                Trigger::Event(name) if name == "key" => {
                    self.scripts[index].key_handlers.push(handler.function.clone());
                }
                Trigger::Event(name) => {
                    self.push_log(
                        &label,
                        format!(
                            "'on {name}' isn't an event Brixo has yet. Events you can use: {}",
                            EVENTS.join(", ")
                        ),
                        true,
                    );
                }
                Trigger::Every(seconds) => {
                    self.scripts[index].timers.push(Timer {
                        function: handler.function.clone(),
                        interval: *seconds,
                        next: self.time + seconds,
                        running: None,
                    });
                }
            }
        }
        self.scripts[index].handlers_seen = handlers.len();
    }

    // --- tasks ---

    /// Starts a task and runs it until it first waits or finishes.
    fn spawn(&mut self, script: usize, job: Job) -> Option<u64> {
        let (resume_tx, resume_rx) = mpsc::channel::<()>();
        let (msg_tx, msg_rx) = mpsc::channel::<TaskMsg>();

        let mut interp = self.scripts[script].base.fork();
        let wait_tx = msg_tx.clone();
        let resume_rx = Mutex::new(resume_rx);
        interp.on_wait = Some(Arc::new(move |seconds: f64| {
            // Tell the game we're pausing, then sleep until it resumes us.
            wait_tx
                .send(TaskMsg::Wait(seconds))
                .map_err(|_| STOPPED.to_string())?;
            resume_rx
                .lock()
                .unwrap()
                .recv()
                .map_err(|_| STOPPED.to_string())
        }));

        let spawned = thread::Builder::new()
            .name(format!("script {}", self.scripts[script].label))
            .stack_size(TASK_STACK_SIZE)
            .spawn(move || {
                let result = match job {
                    Job::Run(program) => interp.run(&program),
                    Job::Call(function, args) => interp.call(function, args, 0).map(|_| ()),
                };
                let _ = msg_tx.send(TaskMsg::Done(result));
            });

        if let Err(e) = spawned {
            let label = self.scripts[script].label.clone();
            self.push_log(&label, format!("couldn't start: {e}"), true);
            return None;
        }

        let id = self.next_task;
        self.next_task += 1;
        self.settle(Task {
            id,
            script,
            wake_at: 0.0,
            resume: resume_tx,
            messages: msg_rx,
        });
        Some(id)
    }

    /// Blocks until the task pauses or finishes, then files it accordingly.
    fn settle(&mut self, mut task: Task) {
        let script = task.script;
        match task.messages.recv() {
            Ok(TaskMsg::Wait(seconds)) => {
                task.wake_at = self.time + seconds.max(0.0);
                self.tasks.push(task);
            }
            Ok(TaskMsg::Done(Ok(()))) => {}
            Ok(TaskMsg::Done(Err(e))) => {
                if e.message != STOPPED {
                    let label = self.scripts[script].label.clone();
                    self.push_log(&label, e.to_string(), true);
                }
            }
            Err(_) => {
                let label = self.scripts[script].label.clone();
                self.push_log(&label, "the script crashed".to_string(), true);
            }
        }
        self.collect_handlers(script);
    }

    /// Wakes every task whose wait is over. Each task runs at most once per
    /// step, so wait(0) in a loop can't spin forever inside one frame.
    fn resume_due_tasks(&mut self) {
        let time = self.time;
        let (mut due, waiting): (Vec<Task>, Vec<Task>) =
            std::mem::take(&mut self.tasks).into_iter().partition(|t| t.wake_at <= time);
        self.tasks = waiting;
        due.sort_by(|a, b| a.wake_at.total_cmp(&b.wake_at).then(a.id.cmp(&b.id)));

        for task in due {
            if task.resume.send(()).is_ok() {
                self.settle(task);
            }
        }
    }

    fn task_running(&self, id: u64) -> bool {
        self.tasks.iter().any(|t| t.id == id)
    }

    // --- timers and events ---

    fn run_timers(&mut self) {
        let mut to_fire: Vec<(usize, usize, Value)> = Vec::new();
        for (s, script) in self.scripts.iter_mut().enumerate() {
            for (t, timer) in script.timers.iter_mut().enumerate() {
                if self.time < timer.next {
                    continue;
                }
                timer.next += timer.interval;
                if timer.next <= self.time {
                    // Fell far behind (a long frame): don't fire a burst.
                    timer.next = self.time + timer.interval;
                }
                to_fire.push((s, t, timer.function.clone()));
            }
        }

        for (s, t, function) in to_fire {
            let busy = self.scripts[s].timers[t]
                .running
                .map(|id| self.task_running(id))
                .unwrap_or(false);
            if busy {
                continue;
            }
            let task = self.spawn(s, Job::Call(function, Vec::new()));
            let still_going = task.filter(|id| self.task_running(*id));
            if let Some(timer) = self.scripts[s].timers.get_mut(t) {
                timer.running = still_going;
            }
        }
    }

    /// Notes where each team's SpawnLocation stands while it's still on the
    /// world (see `team_spawns`).
    fn remember_team_spawns(&mut self) {
        let world = self.world.lock();
        for id in world.walk() {
            let Some(inst) = world.get(id) else { continue };
            if inst.class != Class::SpawnLocation {
                continue;
            }
            let Some(brixo_core::Attribute::Str(team)) = inst.attributes.get("team") else { continue };
            if let Some(p) = world.part(id).filter(|p| p.position.y > FALL_LIMIT + 10.0) {
                let spot = BVec3::new(p.position.x, p.position.y + p.size.y / 2.0 + 2.55, p.position.z);
                self.team_spawns.insert(team.clone(), spot);
            }
        }
    }

    /// Loose parts that fall off the world are gone for good, like in any
    /// Roblox-style game: otherwise they'd fall forever out of sight, still
    /// being simulated, and a spawn pad down there would keep sending
    /// people to their doom. (Anchored parts stay: games keep templates
    /// hidden far below the map.)
    fn clear_fallen_parts(&mut self) {
        if self.time - self.last_sweep < 0.25 {
            return;
        }
        self.last_sweep = self.time;
        let mut world = self.world.lock();
        let fallen: Vec<InstanceId> = world
            .walk()
            .into_iter()
            .filter(|id| world.part(*id).is_some_and(|p| !p.anchored && p.position.y < FALL_LIMIT))
            .filter(|id| world.player_of(*id).is_none())
            .collect();
        for id in fallen {
            world.remove(id);
        }
    }

    /// Simulates physics (which also finds touches), then runs `on touched`.
    fn run_physics(&mut self, dt: f64) {
        // A script set a player's velocity: throw them (jump pads, launchers).
        {
            let mut world = self.world.lock();
            for &id in &self.players {
                let Some(p) = world.player_mut(id) else { continue };
                let v = p.body.velocity;
                if v != BVec3::new(0.0, 0.0, 0.0) {
                    p.body.velocity = BVec3::new(0.0, 0.0, 0.0);
                    if p.dead == 0.0 {
                        self.physics.launch(id, glam::Vec3::new(v.x, v.y, v.z));
                    }
                }
            }
        }
        let listeners: HashSet<InstanceId> = self
            .scripts
            .iter()
            .filter(|s| !s.touch_handlers.is_empty())
            .filter_map(|s| s.parent)
            .collect();
        self.drive_karts(dt as f32);
        let touches = {
            let mut world = self.world.lock();
            self.physics.step(&mut world, dt as f32, &listeners, &self.inputs)
        };
        *self.hinge_angles.lock().unwrap() = self.physics.hinge_angles();
        // What a kart touches, its driver touches too (so a jump pad or a
        // coin that checks for players works for someone driving).
        let touches: Vec<(InstanceId, InstanceId)> = {
            let world = self.world.lock();
            let driver = |part: InstanceId| {
                let kart = brixo_core::kart_of(&world, part)?;
                world.walk().into_iter().find(|p| world.player(*p).is_some_and(|pp| pp.kart == Some(kart)))
            };
            let mut all = touches.clone();
            for (a, b) in &touches {
                if let Some(d) = driver(*a) {
                    if brixo_core::kart_of(&world, *b).is_none() {
                        all.push((*b, d));
                    }
                }
                if let Some(d) = driver(*b) {
                    if brixo_core::kart_of(&world, *a).is_none() {
                        all.push((*a, d));
                    }
                }
            }
            all
        };
        for (a, b) in touches {
            self.fire_touched(a, b);
            self.fire_touched(b, a);
        }
        self.check_respawn(dt as f32);
        self.clear_fallen_parts();
    }

    /// Runs `on touched` for every script inside `part`, passing `other`.
    fn fire_touched(&mut self, part: InstanceId, other: InstanceId) {
        let mut calls = Vec::new();
        for (s, script) in self.scripts.iter().enumerate() {
            if script.parent != Some(part) {
                continue;
            }
            for handler in &script.touch_handlers {
                calls.push((s, handler.clone()));
            }
        }
        for (s, handler) in calls {
            // Pass the other part only if the handler asked for it:
            // `on touched()` and `on touched(other)` both work.
            let wanted = handler.param_count().unwrap_or(1);
            let args = if wanted >= 1 { vec![object(other)] } else { Vec::new() };
            self.spawn(s, Job::Call(handler, args));
        }
    }

    fn push_log(&self, source: &str, text: String, is_error: bool) {
        self.log.lock().unwrap().push(LogLine {
            source: source.to_string(),
            text,
            is_error,
        });
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        // A game that ends (Stop, a server shutting down) writes out
        // saved data.
        if let Ok(mut saves) = self.saves.lock() {
            saves.flush();
        }
        // Dropping the channels makes every paused wait() fail, so each
        // task thread unwinds and exits on its own.
        self.tasks.clear();
    }
}
