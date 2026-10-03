//! The player's side of a connection.

use std::io::{self, BufReader};
use std::net::{TcpStream, ToSocketAddrs};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::Instant;
use std::time::Duration;

use brixo_core::{DataModel, InstanceId};
use brixo_runtime::PlayerInput;

use crate::lag::{Delayer, Lag};
use crate::protocol::{apply_state, read_msg, write_msg, ToClient, ToServer, PROTOCOL};

/// A connection to a Brixo server, with this player's copy of the world.
pub struct NetClient {
    writer: TcpStream,
    /// What the server sent, and when it's to arrive (later than it really
    /// did only with a pretend bad connection: see crate::lag).
    incoming: Receiver<(Instant, ToClient)>,
    /// Arrived from the socket, still "on its way" (pretend lag).
    held: VecDeque<(Instant, ToClient)>,
    /// Pretend lag on what we send: a thread sends each when it's due.
    outgoing: Option<(Sender<(Instant, ToServer)>, Arc<AtomicBool>)>,
    /// The world as of the server's last message.
    pub world: DataModel,
    /// Your character, once the server has let you in.
    pub me: Option<InstanceId>,
    pub connected: bool,
    /// Sounds and music the server sent, for the app to play.
    pub cues: Vec<brixo_runtime::Cue>,
    /// Chat messages: (who, their name, what they said).
    pub chat: Vec<(InstanceId, String, String)>,
    /// The game's Sounds' audio files, as the server sends them.
    pub assets: std::collections::HashMap<InstanceId, std::sync::Arc<Vec<u8>>>,
    last_input: Option<PlayerInput>,
    /// The shift-lock facing last sent.
    last_facing: Option<f32>,
    /// The server said this Player is too old: update, then join again.
    pub update_required: bool,
    /// The server takes numbered input steps and says where you are after
    /// each (`You`): prediction can rewind and replay (brixo_client).
    pub server_steps: bool,
    /// The newest `You`: your character after the server applied your step
    /// `.0`. Taken by the predictor.
    pub you: Option<(u64, brixo_core::CharacterState)>,
    /// How many of our steps were waiting on the server, as of `you`.
    pub queued: u32,
}

impl NetClient {
    /// Connects to `addr` ("127.0.0.1:4570", "192.168.1.20:4570"...) and asks
    /// to join as `name`. The server may add a number if the name's taken.
    /// (BRIXO_LAG pretends the connection is a bad one: see crate::lag.)
    pub fn connect(addr: &str, name: &str) -> io::Result<NetClient> {
        Self::connect_with(addr, name, None, Lag::from_env())
    }

    /// Joins a website-started server with the one-time ticket from Play.
    pub fn connect_with_ticket(addr: &str, ticket: &str) -> io::Result<NetClient> {
        Self::connect_with(addr, "", Some(ticket.to_string()), Lag::from_env())
    }

    /// Joins with a pretend bad connection (for tests and measuring).
    pub fn connect_lagged(addr: &str, name: &str, lag: Option<Lag>) -> io::Result<NetClient> {
        Self::connect_with(addr, name, None, lag)
    }

    fn connect_with(addr: &str, name: &str, ticket: Option<String>, lag: Option<Lag>) -> io::Result<NetClient> {
        let mut writer = connect_any(addr.to_socket_addrs()?)?;
        writer.set_nodelay(true)?;
        write_msg(&mut writer, &ToServer::Hello { name: name.to_string(), ticket, protocol: PROTOCOL })?;

        let mut reader = BufReader::new(writer.try_clone()?);
        let (tx, incoming) = mpsc::channel();
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
        let mut delay_in = lag.map(|l| Delayer::new(l, seed));
        std::thread::Builder::new().name("brixo client".into()).spawn(move || {
            loop {
                match read_msg::<ToClient>(&mut reader) {
                    Ok(Some(msg)) => {
                        let now = Instant::now();
                        let at = delay_in.as_mut().map_or(now, |d| d.arrival(now));
                        if tx.send((at, msg)).is_err() {
                            return;
                        }
                    }
                    // A message this Player doesn't understand (from a
                    // newer server): skip it, don't drop the game.
                    Err(e) if e.kind() == io::ErrorKind::InvalidData => continue,
                    _ => break,
                }
            }
            // Dropping tx tells poll() the server has gone.
        })?;

        // Pretend lag going out: a thread that sends each message when due.
        let outgoing = match lag {
            None => None,
            Some(l) => {
                let (out_tx, out_rx) = mpsc::channel::<(Instant, ToServer)>();
                let failed = Arc::new(AtomicBool::new(false));
                let flag = failed.clone();
                let mut w = writer.try_clone()?;
                let mut delay_out = Delayer::new(l, seed.rotate_left(17));
                std::thread::Builder::new().name("brixo client lag".into()).spawn(move || {
                    while let Ok((sent, msg)) = out_rx.recv() {
                        let at = delay_out.arrival(sent);
                        let now = Instant::now();
                        if at > now {
                            std::thread::sleep(at - now);
                        }
                        if write_msg(&mut w, &msg).is_err() {
                            flag.store(true, Ordering::Relaxed);
                            return;
                        }
                    }
                })?;
                Some((out_tx, failed))
            }
        };
        Ok(NetClient { writer, incoming, held: VecDeque::new(), outgoing, world: DataModel::new(), me: None, connected: true, cues: Vec::new(), chat: Vec::new(), assets: Default::default(), last_input: None, last_facing: None, update_required: false, server_steps: false, you: None, queued: 0 })
    }

    /// Applies everything the server has sent since the last call.
    pub fn poll(&mut self) {
        let mut gone = false;
        loop {
            match self.incoming.try_recv() {
                Ok(m) => self.held.push_back(m),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    gone = true;
                    break;
                }
            }
        }
        let now = Instant::now();
        while self.held.front().is_some_and(|(at, _)| *at <= now) {
            let (_, msg) = self.held.pop_front().unwrap();
            self.apply(msg);
        }
        // Gone once everything it sent has "arrived".
        if gone && self.held.is_empty() {
            self.connected = false;
        }
        if self.outgoing.as_ref().is_some_and(|(_, failed)| failed.load(Ordering::Relaxed)) {
            self.connected = false;
        }
    }

    fn apply(&mut self, msg: ToClient) {
        match msg {
            ToClient::Welcome { you, world, steps } => {
                self.me = Some(InstanceId::from_raw(you));
                self.server_steps = steps;
                if let Ok(w) = DataModel::from_json(&world) {
                    self.world = w;
                }
            }
            ToClient::World { world } => {
                if let Ok(w) = DataModel::from_json(&world) {
                    self.world = w;
                }
            }
            ToClient::State { parts, players, guis, attrs } => {
                apply_state(&mut self.world, &parts, &players, &guis, &attrs);
                // Karts' parts aren't sent: pose them from their chassis.
                brixo_runtime::kart::pose_all(&mut self.world);
            }
            ToClient::Changes { added, removed, moved } => crate::protocol::apply_changes(&mut self.world, &added, &removed, &moved),
            ToClient::Sound { name, at } => self.cues.push(match at {
                Some(at) => brixo_runtime::Cue::SoundAt(name, at),
                None => brixo_runtime::Cue::Sound(name),
            }),
            ToClient::Music { name } => self.cues.push(brixo_runtime::Cue::Music(name)),
            ToClient::Chat { from, name, text } => self.chat.push((InstanceId::from_raw(from), name, text)),
            ToClient::UpdateRequired { .. } => self.update_required = true,
            ToClient::You { ack, state, queued } => {
                if self.you.is_none_or(|(old, _)| ack >= old) {
                    self.you = Some((ack, state));
                    self.queued = queued;
                }
            }
            ToClient::Asset { id, format, data } => {
                let props = brixo_core::SoundProps { format, data: data.into(), volume: 1.0 };
                if let Some(bytes) = props.bytes() {
                    self.assets.insert(InstanceId::from_raw(id), std::sync::Arc::new(bytes));
                }
            }
        }
    }

    fn send(&mut self, msg: ToServer) {
        if !self.connected {
            return;
        }
        let ok = match &self.outgoing {
            Some((tx, _)) => tx.send((Instant::now(), msg)).is_ok(),
            None => write_msg(&mut self.writer, &msg).is_ok(),
        };
        if !ok {
            self.connected = false;
        }
    }

    /// Says something in chat (the server filters it).
    pub fn chat(&mut self, text: &str) {
        self.send(ToServer::Chat { text: text.to_string() });
    }

    /// You clicked a TextButton.
    pub fn click(&mut self, button: InstanceId) {
        self.send(ToServer::Click { button: button.raw() });
    }

    /// You pressed hotbar key `slot` + 1.
    pub fn equip(&mut self, slot: usize) {
        self.send(ToServer::Equip { slot: slot as u32 });
    }

    /// You clicked with a tool in hand.
    pub fn activate(&mut self) {
        self.send(ToServer::Activate);
    }

    /// You clicked with a tool in hand, the mouse pointing at `aim`.
    pub fn activate_at(&mut self, aim: brixo_core::Vec3) {
        self.send(ToServer::ActivateAt { x: aim.x, y: aim.y, z: aim.z });
    }

    /// You pressed a key scripts can hear (`on key`).
    pub fn key(&mut self, key: &str) {
        self.send(ToServer::Key { key: key.to_string() });
    }

    /// Reset character: knocks you out, so you come back at a spawn.
    pub fn reset(&mut self) {
        self.send(ToServer::Reset);
    }

    /// Shift lock: face `yaw` whichever way you walk (None: off). Only
    /// sent when it changes by more than a hair, not every frame.
    pub fn send_facing(&mut self, yaw: Option<f32>) {
        let same = match (self.last_facing, yaw) {
            (None, None) => true,
            (Some(a), Some(b)) => (a - b).abs() < 0.01,
            _ => false,
        };
        if same || !self.connected {
            return;
        }
        self.last_facing = yaw;
        self.send(ToServer::Face { yaw });
    }

    /// Your keys on each of your physics steps, numbered from `first` (to a
    /// server with `server_steps`). Repeats are fine: the server skips steps
    /// it already has.
    pub fn send_steps(&mut self, first: u64, inputs: &[PlayerInput]) {
        if inputs.is_empty() {
            return;
        }
        let inputs = inputs.iter().map(|i| (i.move_x, i.move_z, i.jump)).collect();
        self.send(ToServer::Steps { first, inputs });
    }

    /// Tells the server what you're pressing (only when it changes).
    pub fn send_input(&mut self, input: PlayerInput) {
        if !self.connected || self.last_input == Some(input) {
            return;
        }
        self.send(ToServer::Input { move_x: input.move_x, move_z: input.move_z, jump: input.jump });
        self.last_input = Some(input);
    }
}

impl Drop for NetClient {
    fn drop(&mut self) {
        let _ = self.writer.shutdown(std::net::Shutdown::Both);
    }
}

/// A name can give several addresses (IPv6 then IPv4 for "localhost" on
/// Windows, or a domain with both): try each until one answers.
pub fn connect_any(targets: impl IntoIterator<Item = std::net::SocketAddr>) -> io::Result<TcpStream> {
    let mut last_err = io::Error::new(io::ErrorKind::InvalidInput, "no such address");
    for target in targets {
        match TcpStream::connect_timeout(&target, Duration::from_secs(3)) {
            // With no server on that port, TCP can connect a socket to itself
            // when the OS happens to pick the same port for our end (Windows
            // hands local ports out in order). That's nobody: try the next.
            Ok(s) if s.local_addr().ok() == Some(target) => {
                last_err = io::Error::new(io::ErrorKind::ConnectionRefused, format!("nothing is listening at {target}"));
            }
            Ok(s) => return Ok(s),
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}
