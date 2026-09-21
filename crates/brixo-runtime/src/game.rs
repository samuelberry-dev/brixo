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
use std::sync::{Arc, Mutex, MutexGuard};
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
struct Rng(u64);

impl Rng {
    fn seeded() -> Self {
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

fn random_colors(p: &mut brixo_core::PlayerProps, rng: &mut Rng) {
    let c = |(r, g, b): (u8, u8, u8)| brixo_core::Color::new(r, g, b);
    p.skin_color = c(rng.pick(&SKIN_TONES));
    p.shirt_color = c(rng.pick(&SHIRT_COLORS));
    p.pants_color = c(rng.pick(&PANTS_COLORS));
    p.shoes_color = c(rng.pick(&SHOES_COLORS));
    p.body.color = p.shirt_color;
}

/// Players below this height have fallen off the world and respawn.
pub const FALL_LIMIT: f32 = -60.0;

/// Events scripts can use with `on`.
pub const EVENTS: &[&str] = &["touched", "player_joined", "player_left"];

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
    timers: Vec<Timer>,
}

pub struct Game {
    world: Arc<Mutex<DataModel>>,
    clock: Arc<Mutex<f64>>,
    log: Arc<Mutex<Vec<LogLine>>>,
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
        let mut game = Game::new(model);
        // The player exists before scripts start, so they can find it.
        let id = game.spawn_player("Player");
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
            world: Arc::new(Mutex::new(model)),
            clock: Arc::new(Mutex::new(0.0)),
            log: Arc::new(Mutex::new(Vec::new())),
            scripts: Vec::new(),
            known_scripts: HashSet::new(),
            tasks: Vec::new(),
            next_task: 0,
            physics: Physics::new(),
            inputs: HashMap::new(),
            local_player: None,
            players: Vec::new(),
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
    pub fn shared_world(&self) -> Arc<Mutex<DataModel>> {
        self.world.clone()
    }

    /// A new player joins: they appear at the spawn and scripts hear
    /// `on player_joined(player)`.
    pub fn add_player(&mut self, name: &str) -> InstanceId {
        let id = self.spawn_player(name);
        self.announce_players();
        id
    }

    /// A player leaves. Scripts hear `on player_left(player)` while the
    /// player still exists, then they're removed from the world.
    pub fn remove_player(&mut self, player: InstanceId) {
        if !self.players.contains(&player) {
            return;
        }
        self.fire_everywhere(|s| &s.left_handlers, player);
        self.players.retain(|p| *p != player);
        self.unannounced.retain(|p| *p != player);
        self.inputs.remove(&player);
        if self.local_player == Some(player) {
            self.local_player = None;
        }
        self.world.lock().unwrap().remove(player);
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

    fn spawn_player(&mut self, name: &str) -> InstanceId {
        let mut world = self.world.lock().unwrap();
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
            random_colors(p, &mut Rng::seeded());
        }
        self.players.push(id);
        self.unannounced.push(id);
        id
    }

    /// Where the n-th player to join stands: spread over the spawn pad in a
    /// 3x3 pattern (centre first), so players don't start inside each other.
    fn spawn_spot(&self, n: usize) -> BVec3 {
        const OFFSETS: [(f32, f32); 9] =
            [(0.0, 0.0), (2.5, 0.0), (-2.5, 0.0), (0.0, 2.5), (0.0, -2.5), (2.5, 2.5), (-2.5, -2.5), (2.5, -2.5), (-2.5, 2.5)];
        let (dx, dz) = OFFSETS[n % OFFSETS.len()];
        BVec3::new(self.spawn_point.x + dx, self.spawn_point.y, self.spawn_point.z + dz)
    }

    /// Dead or fallen players go back to the spawn point with full health.
    fn check_respawn(&mut self) {
        let mut respawned = Vec::new();
        let spots: HashMap<InstanceId, BVec3> =
            self.players.iter().enumerate().map(|(i, id)| (*id, self.spawn_spot(i))).collect();
        {
            let mut world = self.world.lock().unwrap();
            for &id in &self.players {
                let Some(p) = world.player_mut(id) else { continue };
                let reason = if p.health <= 0.0 {
                    "died"
                } else if p.body.position.y < FALL_LIMIT {
                    "fell off the world"
                } else {
                    continue;
                };
                p.health = p.max_health;
                p.body.position = spots[&id];
                respawned.push((world.get(id).map(|i| i.name.clone()).unwrap_or_default(), reason));
            }
        }
        for (name, reason) in respawned {
            self.push_log("Brixo", format!("{name} {reason} and respawned"), false);
        }
    }

    /// Advances the game by `dt` seconds.
    pub fn step(&mut self, dt: f64) {
        self.time += dt.max(0.0);
        *self.clock.lock().unwrap() = self.time;

        self.resume_due_tasks();
        self.stop_removed_scripts();
        self.discover_scripts();
        self.run_timers();
        self.run_physics(dt);
    }

    /// The live world, for drawing. Don't hold this across step().
    pub fn world(&self) -> MutexGuard<'_, DataModel> {
        self.world.lock().unwrap()
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
            let world = self.world.lock().unwrap();
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
            timers: Vec::new(),
        });
        let index = self.scripts.len() - 1;
        self.spawn(index, Job::Run(Arc::new(program)));
    }

    /// If a script is destroyed, its paused tasks stop too.
    fn stop_removed_scripts(&mut self) {
        let alive: Vec<bool> = {
            let world = self.world.lock().unwrap();
            self.scripts.iter().map(|s| world.get(s.id).is_some()).collect()
        };
        // Dropping a task drops its channel, which ends its wait() with an error.
        self.tasks.retain(|t| alive[t.script]);
        for (script, is_alive) in self.scripts.iter_mut().zip(&alive) {
            if !is_alive {
                script.touch_handlers.clear();
                script.joined_handlers.clear();
                script.left_handlers.clear();
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

    /// Simulates physics (which also finds touches), then runs `on touched`.
    fn run_physics(&mut self, dt: f64) {
        let listeners: HashSet<InstanceId> = self
            .scripts
            .iter()
            .filter(|s| !s.touch_handlers.is_empty())
            .filter_map(|s| s.parent)
            .collect();
        let touches = {
            let mut world = self.world.lock().unwrap();
            self.physics.step(&mut world, dt as f32, &listeners, &self.inputs)
        };
        for (a, b) in touches {
            self.fire_touched(a, b);
            self.fire_touched(b, a);
        }
        self.check_respawn();
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
        // Dropping the channels makes every paused wait() fail, so each
        // task thread unwinds and exits on its own.
        self.tasks.clear();
    }
}
