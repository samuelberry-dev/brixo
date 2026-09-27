//! Saved player data: what `save(player, "key", value)` keeps between
//! visits, per game and per player.
//!
//! Each player's data is one small JSON object, loaded when they join (so
//! `load` works straight away, even in `on player_joined`) and written back
//! to the store every so often, when they leave, and when the game ends.
//! Online, the website is the store (one row per game and account); in
//! Studio it's kept in memory until Studio closes; a plain local game keeps
//! it in memory for that game.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use brixo_core::InstanceId;
use rovik::value::Value;
use serde_json::Value as Json;

/// Where saved data lives between games. `key` says whose it is (online,
/// their account; locally, their name).
pub trait SaveStore: Send + Sync {
    fn load(&self, key: &str) -> Option<String>;
    fn save(&self, key: &str, data: &str);
}

/// Keeps saved data in memory (Studio, local games, tests).
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SaveStore for MemoryStore {
    fn load(&self, key: &str) -> Option<String> {
        self.0.lock().unwrap().get(key).cloned()
    }
    fn save(&self, key: &str, data: &str) {
        self.0.lock().unwrap().insert(key.to_string(), data.to_string());
    }
}

/// The most one player can have saved in one game, as JSON.
pub const SAVE_LIMIT: usize = 64 * 1024;
/// The longest name a saved value can have.
pub const KEY_LIMIT: usize = 50;
/// How often (in game seconds) changed data is written to the store.
pub const SAVE_EVERY: f64 = 15.0;

struct Slot {
    key: String,
    data: BTreeMap<String, Json>,
    dirty: bool,
}

/// Everyone's saved data while they're in the game.
pub struct Saves {
    store: Arc<dyn SaveStore>,
    slots: HashMap<InstanceId, Slot>,
}

impl Saves {
    pub fn new(store: Arc<dyn SaveStore>) -> Saves {
        Saves { store, slots: HashMap::new() }
    }

    pub fn set_store(&mut self, store: Arc<dyn SaveStore>) {
        self.store = store;
    }

    /// A player arrives: fetch what they saved last time.
    pub fn open(&mut self, player: InstanceId, key: &str) {
        let data = self
            .store
            .load(key)
            .and_then(|text| serde_json::from_str::<BTreeMap<String, Json>>(&text).ok())
            .unwrap_or_default();
        self.slots.insert(player, Slot { key: key.to_string(), data, dirty: false });
    }

    /// A player leaves: write anything unsaved, and forget them.
    pub fn close(&mut self, player: InstanceId) {
        self.write(player);
        self.slots.remove(&player);
    }

    /// Writes everyone's changed data to the store.
    pub fn flush(&mut self) {
        let ids: Vec<InstanceId> = self.slots.keys().copied().collect();
        for id in ids {
            self.write(id);
        }
    }

    fn write(&mut self, player: InstanceId) {
        let store = self.store.clone();
        let Some(slot) = self.slots.get_mut(&player) else { return };
        if !slot.dirty {
            return;
        }
        if let Ok(text) = serde_json::to_string(&slot.data) {
            store.save(&slot.key, &text);
        }
        slot.dirty = false;
    }

    pub fn load(&self, player: InstanceId, key: &str) -> Result<Value, String> {
        let slot = self.slots.get(&player).ok_or("load needs a player who's in the game")?;
        Ok(slot.data.get(key).map(from_json).unwrap_or(Value::Nil))
    }

    pub fn save(&mut self, player: InstanceId, key: &str, value: &Value) -> Result<(), String> {
        if key.is_empty() || key.chars().count() > KEY_LIMIT {
            return Err(format!("a saved value's name has to be 1 to {KEY_LIMIT} letters long"));
        }
        let slot = self.slots.get_mut(&player).ok_or("save needs a player who's in the game")?;
        let old = slot.data.get(key).cloned();
        match value {
            Value::Nil => {
                slot.data.remove(key);
            }
            v => {
                slot.data.insert(key.to_string(), to_json(v, 0)?);
            }
        }
        // Too much? Put it back the way it was.
        let size = serde_json::to_string(&slot.data).map(|t| t.len()).unwrap_or(usize::MAX);
        if size > SAVE_LIMIT {
            match old {
                Some(o) => slot.data.insert(key.to_string(), o),
                None => slot.data.remove(key),
            };
            return Err(format!("that's too much to save: each player can keep {} KB in a game", SAVE_LIMIT / 1024));
        }
        slot.dirty = true;
        Ok(())
    }
}

fn to_json(v: &Value, depth: usize) -> Result<Json, String> {
    if depth > 20 {
        return Err("that's nested too deep to save".into());
    }
    Ok(match v {
        Value::Nil => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Num(n) => serde_json::Number::from_f64(*n).map(Json::Number).ok_or("can't save a number that isn't finite")?,
        Value::Str(s) => Json::String(s.to_string()),
        Value::List(l) => Json::Array(l.read().unwrap().iter().map(|x| to_json(x, depth + 1)).collect::<Result<_, _>>()?),
        Value::Map(m) => Json::Object(
            m.read().unwrap().iter().map(|(k, x)| Ok((k.clone(), to_json(x, depth + 1)?))).collect::<Result<_, String>>()?,
        ),
        Value::Object(_) => {
            return Err("can't save a part or a player: save numbers, text, true/false, and lists and maps of them (for a part, save its name)".into())
        }
        other => {
            return Err(format!(
                "can't save a {}: save numbers, text, true/false, and lists and maps of them",
                other.type_name()
            ))
        }
    })
}

fn from_json(j: &Json) -> Value {
    match j {
        Json::Null => Value::Nil,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => Value::Num(n.as_f64().unwrap_or(0.0)),
        Json::String(s) => Value::str(s.as_str()),
        Json::Array(a) => Value::list(a.iter().map(from_json).collect()),
        Json::Object(o) => Value::map(o.iter().map(|(k, v)| (k.clone(), from_json(v))).collect()),
    }
}
