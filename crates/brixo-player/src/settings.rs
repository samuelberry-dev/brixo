//! The player's own settings (the pause menu's Settings tab), kept in
//! settings.json in the Brixo folder so they stick between games.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Shift turns shift lock on and off.
    pub shift_lock: bool,
    /// How fast the mouse turns the camera (1 is normal).
    pub camera_speed: f32,
    /// Volumes, 0 to 1 (1 is Brixo's normal mix).
    pub music: f32,
    pub sounds: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { shift_lock: true, camera_speed: 1.0, music: 1.0, sounds: 1.0 }
    }
}

fn path() -> Option<std::path::PathBuf> {
    Some(brixo_client::games_dir().parent()?.join("settings.json"))
}

impl Settings {
    pub fn load() -> Settings {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
            .map(Settings::sane)
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(p) = path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, text);
        }
    }

    /// In range, whatever the file said.
    fn sane(self) -> Settings {
        let unit = |v: f32, d: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { d };
        Settings {
            shift_lock: self.shift_lock,
            camera_speed: if self.camera_speed.is_finite() { self.camera_speed.clamp(0.25, 2.5) } else { 1.0 },
            music: unit(self.music, 1.0),
            sounds: unit(self.sounds, 1.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_odd_file_gives_sensible_settings() {
        let s: Settings = serde_json::from_str("{\"music\": 0.2}").unwrap();
        assert_eq!(s.music, 0.2);
        assert!(s.shift_lock && s.camera_speed == 1.0 && s.sounds == 1.0, "the rest default");
        let wild = Settings { shift_lock: false, camera_speed: 90.0, music: -3.0, sounds: f32::NAN }.sane();
        assert_eq!((wild.camera_speed, wild.music, wild.sounds), (2.5, 0.0, 1.0));
    }
}
