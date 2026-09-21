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
    last_input: Option<PlayerInput>,
}

impl NetClient {
    /// Connects to `addr` ("127.0.0.1:4570", "192.168.1.20:4570"...) and asks
    /// to join as `name`. The server may add a number if the name's taken.
    pub fn connect(addr: &str, name: &str) -> io::Result<NetClient> {
        let target = addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no such address"))?;
        let mut writer = TcpStream::connect_timeout(&target, Duration::from_secs(3))?;
        writer.set_nodelay(true)?;
        write_msg(&mut writer, &ToServer::Hello { name: name.to_string() })?;

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

        Ok(NetClient { writer, incoming, world: DataModel::new(), me: None, connected: true, last_input: None })
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
            ToClient::State { parts, players } => apply_state(&mut self.world, &parts, &players),
        }
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
