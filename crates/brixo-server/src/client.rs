//! The player's side of a connection.

use std::io::{self, BufReader};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use brixo_core::{DataModel, InstanceId};
use brixo_runtime::PlayerInput;

use crate::protocol::{apply_state, read_msg, write_msg, ToClient, ToServer};

/// A connection to a Brixo server, with this player's copy of the world.
pub struct NetClient {
    writer: TcpStream,
    incoming: Receiver<ToClient>,
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
}

impl NetClient {
    /// Connects to `addr` ("127.0.0.1:4570", "192.168.1.20:4570"...) and asks
    /// to join as `name`. The server may add a number if the name's taken.
    pub fn connect(addr: &str, name: &str) -> io::Result<NetClient> {
        Self::connect_with(addr, name, None)
    }

    /// Joins a website-started server with the one-time ticket from Play.
    pub fn connect_with_ticket(addr: &str, ticket: &str) -> io::Result<NetClient> {
        Self::connect_with(addr, "", Some(ticket.to_string()))
    }

    fn connect_with(addr: &str, name: &str, ticket: Option<String>) -> io::Result<NetClient> {
        let mut writer = connect_any(addr.to_socket_addrs()?)?;
        writer.set_nodelay(true)?;
        write_msg(&mut writer, &ToServer::Hello { name: name.to_string(), ticket })?;

        let mut reader = BufReader::new(writer.try_clone()?);
        let (tx, incoming) = mpsc::channel();
        std::thread::Builder::new().name("brixo client".into()).spawn(move || {
            while let Ok(Some(msg)) = read_msg::<ToClient>(&mut reader) {
                if tx.send(msg).is_err() {
                    return;
                }
            }
            // Dropping tx tells poll() the server has gone.
        })?;

        Ok(NetClient { writer, incoming, world: DataModel::new(), me: None, connected: true, cues: Vec::new(), chat: Vec::new(), assets: Default::default(), last_input: None, last_facing: None })
    }

    /// Applies everything the server has sent since the last call.
    pub fn poll(&mut self) {
        loop {
            match self.incoming.try_recv() {
                Ok(msg) => self.apply(msg),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.connected = false;
                    break;
                }
            }
        }
    }

    fn apply(&mut self, msg: ToClient) {
        match msg {
            ToClient::Welcome { you, world } => {
                self.me = Some(InstanceId::from_raw(you));
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
            ToClient::Sound { name } => self.cues.push(brixo_runtime::Cue::Sound(name)),
            ToClient::Music { name } => self.cues.push(brixo_runtime::Cue::Music(name)),
            ToClient::Chat { from, name, text } => self.chat.push((InstanceId::from_raw(from), name, text)),
            ToClient::Asset { id, format, data } => {
                let props = brixo_core::SoundProps { format, data: data.into(), volume: 1.0 };
                if let Some(bytes) = props.bytes() {
                    self.assets.insert(InstanceId::from_raw(id), std::sync::Arc::new(bytes));
                }
            }
        }
    }

    fn send(&mut self, msg: ToServer) {
        if self.connected && write_msg(&mut self.writer, &msg).is_err() {
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

    /// Tells the server what you're pressing (only when it changes).
    pub fn send_input(&mut self, input: PlayerInput) {
        if !self.connected || self.last_input == Some(input) {
            return;
        }
        let msg = ToServer::Input { move_x: input.move_x, move_z: input.move_z, jump: input.jump };
        if write_msg(&mut self.writer, &msg).is_err() {
            self.connected = false;
        }
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
