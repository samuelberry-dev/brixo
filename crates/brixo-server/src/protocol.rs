//! What goes over the wire: one JSON message per line.

use std::io::{self, BufRead, Write};

use brixo_core::{Class, DataModel, GuiProps, InstanceId, PartProps, PlayerProps};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// The port servers listen on unless told otherwise.
pub const DEFAULT_PORT: u16 = 4570;

/// Player -> server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToServer {
    /// First message: who's joining. Servers started by the website need a
    /// `ticket` (from pressing Play there); local test servers don't.
    Hello {
        name: String,
        #[serde(default)]
        ticket: Option<String>,
    },
    /// What the player is pressing (camera-relative, already turned into
    /// a world direction).
    Input { move_x: f32, move_z: f32, jump: bool },
    /// Clicked a TextButton.
    Click { button: u64 },
    /// Pressed a hotbar key: hold that backpack slot (again to put it away).
    Equip { slot: u32 },
    /// Clicked with a tool in hand.
    Activate,
    /// Said something in chat.
    Chat { text: String },
}

/// Server -> player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToClient {
    /// You're in: this is your character's id and the world.
    Welcome { you: u64, world: String },
    /// Things were added, removed, renamed or moved in the tree.
    World { world: String },
    /// Where everything is now, and how it looks.
    /// What changed since the last update: only parts, players, GUI and
    /// custom fields that are different.
    State {
        parts: Vec<(u64, PartProps)>,
        players: Vec<(u64, PlayerProps)>,
        guis: Vec<(u64, GuiProps)>,
        #[serde(default)]
        attrs: Vec<(u64, std::collections::BTreeMap<String, brixo_core::Attribute>)>,
    },
    /// Things added to or removed from the world, or renamed or moved in
    /// the tree (small, instead of resending the whole world).
    Changes {
        /// New instances, parents first (as JSON).
        added: Vec<String>,
        removed: Vec<u64>,
        /// (id, new parent, new name).
        moved: Vec<(u64, Option<u64>, String)>,
    },
    /// Play a sound effect.
    Sound { name: String },
    /// Loop this music (None: stop the music).
    Music { name: Option<String> },
    /// A Sound's audio file (sent once per player; World messages leave
    /// the audio out, since they're re-sent whenever the world changes).
    Asset { id: u64, format: String, data: String },
    /// Someone said something (already filtered).
    Chat { from: u64, name: String, text: String },
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
/// Everything under a Folder named "Storage" stays on the server (like
/// Roblox's ServerStorage): scripts can find and clone from it, but players
/// never receive it.
pub fn server_only(world: &DataModel) -> std::collections::HashSet<InstanceId> {
    let mut out = std::collections::HashSet::new();
    for id in world.walk() {
        let Some(inst) = world.get(id) else { continue };
        let in_storage = inst.class == Class::Folder && inst.name == "Storage";
        if in_storage || inst.parent.is_some_and(|p| out.contains(&p)) {
            out.insert(id);
        }
    }
    out
}

/// An instance as players see it: no script source, no audio (that's sent
/// separately, once).
pub fn public_instance(world: &DataModel, id: InstanceId) -> Option<brixo_core::Instance> {
    let mut inst = world.get(id)?.clone();
    match &mut inst.props {
        brixo_core::Props::Script(s) => s.source.clear(),
        brixo_core::Props::Sound(s) => s.data.clear(),
        _ => {}
    }
    Some(inst)
}

pub fn client_view(world: &DataModel) -> String {
    let mut copy = world.clone();
    for id in server_only(&copy) {
        copy.remove(id);
    }
    let scripts: Vec<_> = copy.walk().into_iter().filter(|id| copy.script(*id).is_some()).collect();
    for id in scripts {
        copy.script_mut(id).unwrap().source.clear();
    }
    let sounds: Vec<_> = copy.walk().into_iter().filter(|id| copy.sound(*id).is_some()).collect();
    for id in sounds {
        copy.sound_mut(id).unwrap().data.clear();
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
    let mut guis = Vec::new();
    for id in world.walk() {
        if let Some(p) = world.part(id) {
            parts.push((id.raw(), *p));
        } else if let Some(p) = world.player(id) {
            players.push((id.raw(), *p));
        } else if let Some(g) = world.gui(id) {
            guis.push((id.raw(), g.clone()));
        }
    }
    ToClient::State { parts, players, guis, attrs: Vec::new() }
}

/// Copies a State into a player's copy of the world.
pub fn apply_state(
    world: &mut DataModel,
    parts: &[(u64, PartProps)],
    players: &[(u64, PlayerProps)],
    guis: &[(u64, GuiProps)],
    attrs: &[(u64, std::collections::BTreeMap<String, brixo_core::Attribute>)],
) {
    for (id, map) in attrs {
        if let Some(i) = world.get_mut(InstanceId::from_raw(*id)) {
            i.attributes = map.clone();
        }
    }
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
    for (id, props) in guis {
        if let Some(g) = world.gui_mut(InstanceId::from_raw(*id)) {
            *g = props.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip_one_per_line() {
        let mut buf = Vec::new();
        write_msg(&mut buf, &ToServer::Hello { name: "Ann".into(), ticket: None }).unwrap();
        write_msg(&mut buf, &ToServer::Input { move_x: 0.5, move_z: -1.0, jump: true }).unwrap();
        let mut r = io::BufReader::new(&buf[..]);
        assert_eq!(read_msg::<ToServer>(&mut r).unwrap(), Some(ToServer::Hello { name: "Ann".into(), ticket: None }));
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

/// Applies added / removed / moved instances to a client's world.
pub fn apply_changes(world: &mut DataModel, added: &[String], removed: &[u64], moved: &[(u64, Option<u64>, String)]) {
    for id in removed {
        world.remove(InstanceId::from_raw(*id));
    }
    for json in added {
        if let Ok(inst) = serde_json::from_str::<brixo_core::Instance>(json) {
            world.insert_instance(inst);
        }
    }
    for (id, parent, name) in moved {
        let id = InstanceId::from_raw(*id);
        if let Some(p) = parent {
            world.reparent(id, InstanceId::from_raw(*p));
        }
        if let Some(i) = world.get_mut(id) {
            i.name = name.clone();
        }
    }
}
