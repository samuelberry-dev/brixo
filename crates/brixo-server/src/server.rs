//! The game server: runs one game and streams it to everyone connected.

use std::collections::HashMap;
use std::io::{self, BufReader};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use parking_lot::Mutex as WorldMutex;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use brixo_core::{DataModel, InstanceId};
use brixo_runtime::{Game, LogLine, PlayerInput};

use crate::protocol::{client_view, read_msg, write_msg, ToClient, ToServer};

/// Server ticks per second: physics, scripts, and one State to each player.
pub const TICK_RATE: f64 = 60.0;

/// A running server. Dropping it (or calling stop) shuts it down and
/// disconnects everyone.
pub struct ServerHandle {
    port: u16,
    stop: Arc<AtomicBool>,
    log: Arc<Mutex<Vec<LogLine>>>,
    world: Arc<WorldMutex<DataModel>>,
    players: Arc<AtomicUsize>,
    thread: Option<JoinHandle<()>>,
}

impl ServerHandle {
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The address to connect to from this computer.
    pub fn local_addr(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    /// Script output and join/leave messages since the last call.
    pub fn take_log(&self) -> Vec<LogLine> {
        std::mem::take(&mut *self.log.lock().unwrap())
    }

    /// The live game world, for watching (the studio draws it).
    pub fn world(&self) -> parking_lot::MutexGuard<'_, DataModel> {
        self.world.lock()
    }

    pub fn player_count(&self) -> usize {
        self.players.load(Ordering::Relaxed)
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Who a ticket belongs to: their username and saved look.
#[derive(Debug, Clone)]
pub struct Identity {
    pub name: String,
    pub look: brixo_runtime::Look,
}

/// Checks a join ticket (and uses it up). None means "not allowed in".
pub type TicketCheck = std::sync::Arc<dyn Fn(&str) -> Option<Identity> + Send + Sync>;

/// Starts serving `model` on `port` (0 picks any free port). Anyone can
/// join, under the name they ask for: the local/LAN test server.
pub fn start(model: DataModel, port: u16) -> io::Result<ServerHandle> {
    start_inner(model, port, None)
}

/// A server that only lets in players with a valid ticket, who then join
/// as their account (name and avatar). The website starts these.
pub fn start_with_tickets(model: DataModel, port: u16, check: TicketCheck) -> io::Result<ServerHandle> {
    start_inner(model, port, Some(check))
}

fn start_inner(model: DataModel, port: u16, tickets: Option<TicketCheck>) -> io::Result<ServerHandle> {
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();

    let stop = Arc::new(AtomicBool::new(false));
    let log = Arc::new(Mutex::new(Vec::new()));
    let players = Arc::new(AtomicUsize::new(0));
    let (world_tx, world_rx) = mpsc::channel();

    let thread = {
        let (stop, log, players) = (stop.clone(), log.clone(), players.clone());
        std::thread::Builder::new()
            .name("brixo server".into())
            .spawn(move || {
                // Scripts start here, on the server's own thread.
                let game = Game::start_server(model);
                let _ = world_tx.send(game.shared_world());
                Server::new(game, listener, log, players, tickets).run(&stop);
            })?
    };
    let world = world_rx.recv().map_err(|_| io::Error::other("the server failed to start"))?;
    Ok(ServerHandle { port, stop, log, world, players, thread: Some(thread) })
}

enum Event {
    Message(ToServer),
    Closed,
}

struct Connection {
    writer: TcpStream,
    player: Option<InstanceId>,
    /// Which Sounds' audio this player already has.
    assets_sent: std::collections::HashSet<u64>,
}

impl Drop for Connection {
    fn drop(&mut self) {
        // The reader thread holds its own handle to this socket, so just
        // dropping ours wouldn't close it. Shutting it down does, for
        // everyone: the player sees the disconnect, the reader thread exits.
        let _ = self.writer.shutdown(std::net::Shutdown::Both);
    }
}

struct Server {
    game: Game,
    listener: TcpListener,
    log: Arc<Mutex<Vec<LogLine>>>,
    player_count: Arc<AtomicUsize>,
    connections: HashMap<u64, Connection>,
    next_connection: u64,
    events_tx: Sender<(u64, Event)>,
    events: Receiver<(u64, Event)>,
    /// The music everyone should hear, for players who join later.
    music: Option<String>,
    /// What players were last sent, to send only what changed.
    sent: Sent,
    /// When set, players need a ticket to join (website servers).
    tickets: Option<TicketCheck>,
}

impl Server {
    fn new(
        game: Game,
        listener: TcpListener,
        log: Arc<Mutex<Vec<LogLine>>>,
        player_count: Arc<AtomicUsize>,
        tickets: Option<TicketCheck>,
    ) -> Self {
        let (events_tx, events) = mpsc::channel();
        Server {
            game,
            listener,
            log,
            player_count,
            connections: HashMap::new(),
            next_connection: 0,
            events_tx,
            events,
            music: None,
            sent: Sent::default(),
            tickets,
        }
    }

    fn run(&mut self, stop: &AtomicBool) {
        // Filming hook: BRIXO_TIME_SCALE=0.25 runs the game at quarter
        // speed (the same steps, just further apart), so a slow computer can
        // render every moment and the footage is sped back up afterwards.
        let scale = std::env::var("BRIXO_TIME_SCALE").ok().and_then(|s| s.parse::<f64>().ok()).filter(|s| *s > 0.01 && *s <= 1.0).unwrap_or(1.0);
        let tick = Duration::from_secs_f64(1.0 / TICK_RATE / scale);
        let mut next = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            self.accept();
            self.handle_events();
            self.game.step(1.0 / TICK_RATE);
            self.log.lock().unwrap().extend(self.game.take_log());
            self.broadcast();
            self.send_sounds();
            self.player_count.store(self.game.players().len(), Ordering::Relaxed);

            next += tick;
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            } else {
                next = now; // running behind: don't try to catch up in a burst
            }
        }
        // Dropping the connections closes every socket: players see the
        // server go away.
    }

    fn accept(&mut self) {
        while let Ok((stream, _)) = self.listener.accept() {
            if self.add_connection(stream).is_err() {
                continue;
            }
        }
    }

    fn add_connection(&mut self, stream: TcpStream) -> io::Result<()> {
        stream.set_nonblocking(false)?;
        stream.set_nodelay(true)?;
        // A player that stops reading can't freeze the server for long.
        stream.set_write_timeout(Some(Duration::from_secs(1)))?;
        let id = self.next_connection;
        self.next_connection += 1;

        let mut reader = BufReader::new(stream.try_clone()?);
        let tx = self.events_tx.clone();
        std::thread::Builder::new().name(format!("brixo connection {id}")).spawn(move || {
            loop {
                match read_msg::<ToServer>(&mut reader) {
                    Ok(Some(msg)) => {
                        if tx.send((id, Event::Message(msg))).is_err() {
                            return;
                        }
                    }
                    // Closed, broken, or sent garbage: either way, they're gone.
                    Ok(None) | Err(_) => {
                        let _ = tx.send((id, Event::Closed));
                        return;
                    }
                }
            }
        })?;
        self.connections.insert(id, Connection { writer: stream, player: None, assets_sent: Default::default() });
        Ok(())
    }

    fn handle_events(&mut self) {
        while let Ok((conn, event)) = self.events.try_recv() {
            match event {
                Event::Message(ToServer::Hello { name, ticket }) => self.hello(conn, &name, ticket.as_deref()),
                Event::Message(ToServer::Input { move_x, move_z, jump }) => {
                    if let Some(player) = self.connections.get(&conn).and_then(|c| c.player) {
                        let len = (move_x * move_x + move_z * move_z).sqrt();
                        // Never trust a client: no moving faster than walking.
                        let (move_x, move_z) = if len > 1.0 { (move_x / len, move_z / len) } else { (move_x, move_z) };
                        self.game.set_input_for(player, PlayerInput { move_x, move_z, jump });
                    }
                }
                Event::Message(ToServer::Chat { text }) => {
                    if let Some(player) = self.connections.get(&conn).and_then(|c| c.player) {
                        self.game.chat(player, &text);
                    }
                }
                Event::Message(msg @ (ToServer::Click { .. } | ToServer::Equip { .. } | ToServer::Activate)) => {
                    // The game checks each request against this player: a
                    // client can't press someone else's button or tool.
                    let Some(player) = self.connections.get(&conn).and_then(|c| c.player) else { continue };
                    match msg {
                        ToServer::Click { button } => {
                            self.game.click(player, InstanceId::from_raw(button));
                        }
                        ToServer::Equip { slot } => self.game.equip(player, Some(slot as usize)),
                        _ => {
                            self.game.activate(player);
                        }
                    }
                }
                Event::Closed => self.disconnect(conn),
            }
        }
    }

    /// A new connection says hello: on a ticketed server, the ticket decides
    /// who they are; anywhere else, they pick a name.
    fn hello(&mut self, conn: u64, requested: &str, ticket: Option<&str>) {
        match &self.tickets {
            None => self.join(conn, requested, None),
            Some(check) => match ticket.and_then(|t| check(t)) {
                Some(who) => self.join(conn, &who.name, Some(who.look)),
                None => {
                    self.log.lock().unwrap().push(LogLine {
                        source: "Server".into(),
                        text: "Someone tried to join without a valid ticket".into(),
                        is_error: false,
                    });
                    self.connections.remove(&conn); // drops and shuts the socket
                }
            },
        }
    }

    fn join(&mut self, conn: u64, requested: &str, look: Option<brixo_runtime::Look>) {
        let Some(c) = self.connections.get(&conn) else { return };
        if c.player.is_some() {
            return; // already joined
        }
        let name = self.unique_name(requested);
        let player = self.game.add_player_as(&name, look);
        let welcome = ToClient::Welcome { you: player.raw(), world: client_view(&self.game.world()) };
        let c = self.connections.get_mut(&conn).unwrap();
        c.player = Some(player);
        let music = ToClient::Music { name: self.music.clone() };
        if write_msg(&mut c.writer, &welcome).and_then(|_| write_msg(&mut c.writer, &music)).is_err() {
            self.disconnect(conn);
            return;
        }
        self.log.lock().unwrap().push(LogLine { source: "Server".into(), text: format!("{name} joined"), is_error: false });
    }

    fn disconnect(&mut self, conn: u64) {
        if let Some(c) = self.connections.remove(&conn) {
            if let Some(player) = c.player {
                let name = self.game.world().get(player).map(|i| i.name.clone()).unwrap_or_default();
                self.game.remove_player(player);
                self.log.lock().unwrap().push(LogLine { source: "Server".into(), text: format!("{name} left"), is_error: false });
            }
        }
    }

    /// "Player", "Player2", "Player3"... so everyone's name is different.
    fn unique_name(&self, requested: &str) -> String {
        let base: String = requested.chars().filter(|c| c.is_alphanumeric() || *c == '_').take(20).collect();
        let base = if base.is_empty() { "Player".to_string() } else { base };
        let world = self.game.world();
        let taken = |n: &str| self.game.players().iter().any(|p| world.get(*p).is_some_and(|i| i.name == n));
        if !taken(&base) {
            return base;
        }
        (2..).map(|i| format!("{base}{i}")).find(|n| !taken(n)).unwrap()
    }

    /// Sounds scripts played this tick, to everyone or to one player.
    fn send_sounds(&mut self) {
        let mut dead = Vec::new();
        for (from, name, text) in self.game.take_chat() {
            self.log.lock().unwrap().push(LogLine { source: "Chat".into(), text: format!("{name}: {text}"), is_error: false });
            let msg = ToClient::Chat { from: from.raw(), name, text };
            for (id, c) in &mut self.connections {
                if c.player.is_some() && write_msg(&mut c.writer, &msg).is_err() {
                    dead.push(*id);
                }
            }
        }
        for event in self.game.take_sounds() {
            let (msg, target) = match event {
                brixo_runtime::SoundEvent::Play { name, player } => (ToClient::Sound { name }, player),
                brixo_runtime::SoundEvent::Music { name, player } => {
                    if player.is_none() {
                        self.music = name.clone();
                    }
                    (ToClient::Music { name }, player)
                }
            };
            for (id, c) in &mut self.connections {
                let wanted = c.player.is_some() && (target.is_none() || target == c.player);
                if wanted && write_msg(&mut c.writer, &msg).is_err() {
                    dead.push(*id);
                }
            }
        }
        for id in dead {
            self.disconnect(id);
        }
    }

    /// Sends each player the audio of any Sound they don't have yet.
    fn send_assets(&mut self) {
        let sounds: Vec<(u64, String, String)> = {
            let w = self.game.world();
            w.walk().into_iter().filter_map(|id| w.sound(id).map(|s| (id.raw(), s.format.clone(), s.data.clone()))).collect()
        };
        let mut dead = Vec::new();
        for (cid, c) in &mut self.connections {
            if c.player.is_none() {
                continue;
            }
            for (id, format, data) in &sounds {
                if c.assets_sent.insert(*id) {
                    let msg = ToClient::Asset { id: *id, format: format.clone(), data: data.clone() };
                    if write_msg(&mut c.writer, &msg).is_err() {
                        dead.push(*cid);
                        break;
                    }
                }
            }
        }
        for id in dead {
            self.disconnect(id);
        }
    }

    fn broadcast(&mut self) {
        self.send_assets();
        let msgs = {
            let world = self.game.world();
            self.sent.changes(&world)
        };
        if msgs.is_empty() {
            return;
        }
        let mut dead = Vec::new();
        for (id, c) in &mut self.connections {
            if c.player.is_none() {
                continue; // hasn't said hello yet
            }
            if msgs.iter().any(|m| write_msg(&mut c.writer, m).is_err()) {
                dead.push(*id);
            }
        }
        for id in dead {
            self.disconnect(id);
        }
    }
}

/// What players were last told about the world, so each update carries only
/// what's different: a few moving parts, not all of them, every tick.
#[derive(Default)]
struct Sent {
    tree: HashMap<u64, (Option<u64>, String)>,
    parts: HashMap<u64, brixo_core::PartProps>,
    players: HashMap<u64, brixo_core::PlayerProps>,
    guis: HashMap<u64, brixo_core::GuiProps>,
    attrs: HashMap<u64, std::collections::BTreeMap<String, brixo_core::Attribute>>,
}

impl Sent {
    fn changes(&mut self, world: &brixo_core::DataModel) -> Vec<ToClient> {
        let hidden = crate::protocol::server_only(world);
        let order: Vec<InstanceId> = world.walk().into_iter().filter(|id| !hidden.contains(id)).collect();

        // The tree: added, removed, moved or renamed.
        let mut tree = HashMap::with_capacity(order.len());
        let (mut added, mut moved) = (Vec::new(), Vec::new());
        for id in &order {
            let inst = world.get(*id).unwrap();
            let now = (inst.parent.map(|p| p.raw()), inst.name.clone());
            match self.tree.get(&id.raw()) {
                None => {
                    if let Some(public) = crate::protocol::public_instance(world, *id) {
                        added.push(serde_json::to_string(&public).unwrap());
                    }
                }
                Some(before) if *before != now => moved.push((id.raw(), now.0, now.1.clone())),
                _ => {}
            }
            tree.insert(id.raw(), now);
        }
        let removed: Vec<u64> = self.tree.keys().filter(|id| !tree.contains_key(id)).copied().collect();
        self.tree = tree;

        // The state: only what's different from last time.
        let (mut parts, mut players, mut guis, mut attrs) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for id in &order {
            let raw = id.raw();
            if let Some(p) = world.part(*id) {
                if self.parts.get(&raw) != Some(p) {
                    self.parts.insert(raw, *p);
                    parts.push((raw, *p));
                }
            } else if let Some(p) = world.player(*id) {
                if self.players.get(&raw) != Some(p) {
                    self.players.insert(raw, *p);
                    players.push((raw, *p));
                }
            } else if let Some(g) = world.gui(*id) {
                if self.guis.get(&raw) != Some(g) {
                    self.guis.insert(raw, g.clone());
                    guis.push((raw, g.clone()));
                }
            }
            let a = &world.get(*id).unwrap().attributes;
            if self.attrs.get(&raw).map_or(!a.is_empty(), |before| before != a) {
                self.attrs.insert(raw, a.clone());
                attrs.push((raw, a.clone()));
            }
        }
        for id in &removed {
            self.parts.remove(id);
            self.players.remove(id);
            self.guis.remove(id);
            self.attrs.remove(id);
        }

        let mut out = Vec::new();
        if !added.is_empty() || !removed.is_empty() || !moved.is_empty() {
            out.push(ToClient::Changes { added, removed, moved });
        }
        if !parts.is_empty() || !players.is_empty() || !guis.is_empty() || !attrs.is_empty() {
            out.push(ToClient::State { parts, players, guis, attrs });
        }
        out
    }
}
