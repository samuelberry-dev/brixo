//! The game server: runs one game and streams it to everyone connected.

use std::collections::HashMap;
use std::io::{self, BufReader};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use brixo_core::{DataModel, InstanceId};
use brixo_runtime::{Game, LogLine, PlayerInput};

use crate::protocol::{client_view, read_msg, state_of, structure, write_msg, ToClient, ToServer};

/// Server ticks per second: physics, scripts, and one State to each player.
pub const TICK_RATE: f64 = 60.0;

/// A running server. Dropping it (or calling stop) shuts it down and
/// disconnects everyone.
pub struct ServerHandle {
    port: u16,
    stop: Arc<AtomicBool>,
    log: Arc<Mutex<Vec<LogLine>>>,
    world: Arc<Mutex<DataModel>>,
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
    pub fn world(&self) -> std::sync::MutexGuard<'_, DataModel> {
        self.world.lock().unwrap()
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

/// Starts serving `model` on `port` (0 picks any free port). Players on
/// other computers can join via this computer's address.
pub fn start(model: DataModel, port: u16) -> io::Result<ServerHandle> {
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
                Server::new(game, listener, log, players).run(&stop);
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
    last_structure: Vec<(u64, Option<u64>, String, brixo_core::Class)>,
}

impl Server {
    fn new(game: Game, listener: TcpListener, log: Arc<Mutex<Vec<LogLine>>>, player_count: Arc<AtomicUsize>) -> Self {
        let (events_tx, events) = mpsc::channel();
        let last_structure = structure(&game.world());
        Server {
            game,
            listener,
            log,
            player_count,
            connections: HashMap::new(),
            next_connection: 0,
            events_tx,
            events,
            last_structure,
        }
    }

    fn run(&mut self, stop: &AtomicBool) {
        let tick = Duration::from_secs_f64(1.0 / TICK_RATE);
        let mut next = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            self.accept();
            self.handle_events();
            self.game.step(1.0 / TICK_RATE);
            self.log.lock().unwrap().extend(self.game.take_log());
            self.broadcast();
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
        self.connections.insert(id, Connection { writer: stream, player: None });
        Ok(())
    }

    fn handle_events(&mut self) {
        while let Ok((conn, event)) = self.events.try_recv() {
            match event {
                Event::Message(ToServer::Hello { name }) => self.join(conn, &name),
                Event::Message(ToServer::Input { move_x, move_z, jump }) => {
                    if let Some(player) = self.connections.get(&conn).and_then(|c| c.player) {
                        let len = (move_x * move_x + move_z * move_z).sqrt();
                        // Never trust a client: no moving faster than walking.
                        let (move_x, move_z) = if len > 1.0 { (move_x / len, move_z / len) } else { (move_x, move_z) };
                        self.game.set_input_for(player, PlayerInput { move_x, move_z, jump });
                    }
                }
                Event::Closed => self.disconnect(conn),
            }
        }
    }

    fn join(&mut self, conn: u64, requested: &str) {
        let Some(c) = self.connections.get(&conn) else { return };
        if c.player.is_some() {
            return; // already joined
        }
        let name = self.unique_name(requested);
        let player = self.game.add_player(&name);
        let welcome = ToClient::Welcome { you: player.raw(), world: client_view(&self.game.world()) };
        let c = self.connections.get_mut(&conn).unwrap();
        c.player = Some(player);
        if write_msg(&mut c.writer, &welcome).is_err() {
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

    fn broadcast(&mut self) {
        let (world_msg, state) = {
            let world = self.game.world();
            let shape = structure(&world);
            let world_msg = if shape != self.last_structure {
                self.last_structure = shape;
                Some(ToClient::World { world: client_view(&world) })
            } else {
                None
            };
            (world_msg, state_of(&world))
        };
        let mut dead = Vec::new();
        for (id, c) in &mut self.connections {
            if c.player.is_none() {
                continue; // hasn't said hello yet
            }
            let sent = match &world_msg {
                Some(w) => write_msg(&mut c.writer, w).and_then(|_| write_msg(&mut c.writer, &state)),
                None => write_msg(&mut c.writer, &state),
            };
            if sent.is_err() {
                dead.push(*id);
            }
        }
        for id in dead {
            self.disconnect(id);
        }
    }
}
