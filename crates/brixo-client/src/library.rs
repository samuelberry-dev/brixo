//! The games library: a folder of published `.brixo` games that the player
//! lists and the studio publishes into.

use std::path::{Path, PathBuf};

use brixo_core::DataModel;

pub const EXTENSION: &str = "brixo";

/// A game in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameEntry {
    pub name: String,
    pub path: PathBuf,
}

/// Where published games live: `BRIXO_GAMES` if set, otherwise a `Brixo/games`
/// folder in your home folder (`C:\Users\<you>\Brixo\games` on Windows).
pub fn games_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("BRIXO_GAMES") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("Brixo").join("games")
}

pub fn list_games() -> Vec<GameEntry> {
    list_games_in(&games_dir())
}

/// Every `.brixo` file in `dir`, sorted by name. A missing folder is just
/// an empty library.
pub fn list_games_in(dir: &Path) -> Vec<GameEntry> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut games: Vec<GameEntry> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == EXTENSION))
        .filter_map(|path| {
            let name = path.file_stem()?.to_string_lossy().to_string();
            Some(GameEntry { name, path })
        })
        .collect();
    games.sort_by_key(|g| g.name.to_lowercase());
    games
}

pub fn publish(model: &DataModel, name: &str) -> Result<PathBuf, String> {
    publish_to(&games_dir(), model, name)
}

/// Saves the game into the library as `<name>.brixo`, replacing any earlier
/// version with the same name.
pub fn publish_to(dir: &Path, model: &DataModel, name: &str) -> Result<PathBuf, String> {
    let file = file_name(name).ok_or("give your game a name first")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("couldn't create {}: {e}", dir.display()))?;
    let path = dir.join(format!("{file}.{EXTENSION}"));
    let json = model.to_json().map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
    Ok(path)
}

/// A name that's safe as a file name on every system: letters, digits,
/// spaces, dashes and underscores, trimmed. None if nothing's left.
fn file_name(name: &str) -> Option<String> {
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .collect();
    let cleaned = cleaned.trim().to_string();
    (!cleaned.is_empty()).then_some(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("brixo-library-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn publishing_puts_a_game_in_the_library() {
        let dir = temp_dir("publish");
        assert!(list_games_in(&dir).is_empty(), "missing folder is an empty library");
        let model = DataModel::new();
        publish_to(&dir, &model, "Zombie Tag").unwrap();
        publish_to(&dir, &model, "apple run").unwrap();
        publish_to(&dir, &model, "Zombie Tag").unwrap(); // replaces, no duplicate
        std::fs::write(dir.join("notes.txt"), "not a game").unwrap();
        let names: Vec<_> = list_games_in(&dir).into_iter().map(|g| g.name).collect();
        assert_eq!(names, ["apple run", "Zombie Tag"]);
        let loaded = DataModel::load_file(dir.join("Zombie Tag.brixo").to_str().unwrap());
        assert!(loaded.is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_are_made_safe_for_files() {
        assert_eq!(file_name("  Obby: Level 1/2?  ").as_deref(), Some("Obby Level 12"));
        assert_eq!(file_name("../../etc"), Some("etc".to_string()));
        assert_eq!(file_name("???"), None);
        let dir = temp_dir("empty-name");
        assert!(publish_to(&dir, &DataModel::new(), "  ").is_err());
    }
}
