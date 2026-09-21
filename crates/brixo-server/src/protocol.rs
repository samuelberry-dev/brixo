//! What goes over the wire: one JSON message per line.

use std::io::{self, BufRead, Write};

use brixo_core::{Class, DataModel, PartProps, PlayerProps};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// The port servers listen on unless told otherwise.
pub const DEFAULT_PORT: u16 = 4570;

/// Player -> server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToServer {
    /// First message: who's joining.
    Hello { name: String },
    /// What the player is pressing (camera-relative, already turned into
    /// a world direction).
    Input { move_x: f32, move_z: f32, jump: bool },
}

/// Server -> player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToClient {
    /// You're in: this is your character's id and the world.
    Welcome { you: u64, world: String },
    /// Things were added, removed, renamed or moved in the tree.
    World { world: String },
    /// Where everything is now, and how it looks.
    State { parts: Vec<(u64, PartProps)>, players: Vec<(u64, PlayerProps)> },
}

pub fn write_msg<T: Serialize>(w: &mut impl Write, msg: &T) -> io::Result<()> {
    let mut line = serde_json::to_string(msg).map_err(io::Error::other)?;
    line.push('\n');
    w.write_all(line.as_bytes())
}

/// The next message, or None when the other side has closed.
pub fn read_msg<T: DeserializeOwned>(r: &mut impl BufRead) -> io::Result<Option<T>> {
    let mut line = String::new();
    if r.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    serde_json::from_str(&line)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// The world as players get it: script source code stays on the server,
/// so nobody can read (or copy) a game's logic.
pub fn client_view(world: &DataModel) -> String {
    let mut copy = world.clone();
    let scripts: Vec<_> = copy.walk().into_iter().filter(|id| copy.script(*id).is_some()).collect();
    for id in scripts {
        copy.script_mut(id).unwrap().source.clear();
    }
    copy.to_json().expect("worlds always serialize")
}

/// The shape of the tree: when this changes, players need a new World.
pub fn structure(world: &DataModel) -> Vec<(u64, Option<u64>, String, Class)> {
    world
        .walk()
        .into_iter()
        .filter_map(|id| world.get(id))
        .map(|i| (i.id.raw(), i.parent.map(|p| p.raw()), i.name.clone(), i.class))
        .collect()
}

/// Every part's and player's current properties.
pub fn state_of(world: &DataModel) -> ToClient {
    let mut parts = Vec::new();
    let mut players = Vec::new();
    for id in world.walk() {
        if let Some(p) = world.part(id) {
            parts.push((id.raw(), *p));
        } else if let Some(p) = world.player(id) {
            players.push((id.raw(), *p));
        }
    }
    ToClient::State { parts, players }
}

/// Copies a State into a player's copy of the world.
pub fn apply_state(world: &mut DataModel, parts: &[(u64, PartProps)], players: &[(u64, PlayerProps)]) {
    use brixo_core::InstanceId;
    for (id, props) in parts {
        if let Some(p) = world.part_mut(InstanceId::from_raw(*id)) {
            *p = *props;
        }
    }
    for (id, props) in players {
        if let Some(p) = world.player_mut(InstanceId::from_raw(*id)) {
            *p = *props;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip_one_per_line() {
        let mut buf = Vec::new();
        write_msg(&mut buf, &ToServer::Hello { name: "Ann".into() }).unwrap();
        write_msg(&mut buf, &ToServer::Input { move_x: 0.5, move_z: -1.0, jump: true }).unwrap();
        let mut r = io::BufReader::new(&buf[..]);
        assert_eq!(read_msg::<ToServer>(&mut r).unwrap(), Some(ToServer::Hello { name: "Ann".into() }));
        assert_eq!(
            read_msg::<ToServer>(&mut r).unwrap(),
            Some(ToServer::Input { move_x: 0.5, move_z: -1.0, jump: true })
        );
        assert_eq!(read_msg::<ToServer>(&mut r).unwrap(), None, "end of stream");
    }

    #[test]
    fn players_never_see_script_source() {
        let mut dm = DataModel::new();
        let root = dm.root();
        let s = dm.create(Class::Script, "Secret", root).unwrap();
        dm.script_mut(s).unwrap().source = "print(\"the answer is 42\")".into();
        let view = client_view(&dm);
        assert!(!view.contains("answer"));
        let seen = DataModel::from_json(&view).unwrap();
        assert_eq!(seen.script(s).unwrap().source, "", "the script is there, its code isn't");
        assert_eq!(dm.script(s).unwrap().source, "print(\"the answer is 42\")", "the server's copy is untouched");
    }
}
