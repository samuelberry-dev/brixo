//! Plays cues on the speakers, and (when BRIXO_SOUND_LOG is set) logs
//! them with timestamps, which is how demo videos get their soundtrack.

use std::io::Write;
use std::time::Instant;

use brixo_runtime::Cue;

/// Finds a Sound's audio and volume by id (from the local game's world, or
/// what the server sent).
pub type SoundLookup<'a> = &'a dyn Fn(u64) -> Option<(std::sync::Arc<Vec<u8>>, f32)>;

pub use brixo_audio::decodes;

pub struct Audio {
    speaker: Option<brixo_audio::Speaker>,
    /// A game song asked for before its audio arrived (online, the "play
    /// this" message can beat the file): started once the file is here.
    pending_music: Option<String>,
    log: Option<std::fs::File>,
    start: Instant,
}

impl Audio {
    pub fn new() -> Audio {
        let mut log = std::env::var_os("BRIXO_SOUND_LOG").and_then(|p| std::fs::File::create(p).ok());
        // The log's times count from here; note when "here" is in real time
        // (Unix seconds), so they can be lined up with a recording.
        if let Some(f) = &mut log {
            let epoch = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
            let _ = writeln!(f, "# epoch {epoch:.3}");
        }
        Audio { speaker: brixo_audio::Speaker::new(), pending_music: None, log, start: Instant::now() }
    }

    /// The player's volume settings (0 to 1) for sound effects and music.
    pub fn set_levels(&mut self, sounds: f32, music: f32) {
        if let Some(s) = &mut self.speaker {
            s.set_levels(sounds, music);
        }
    }

    /// Plays a cue that may name one of the game's own Sounds ("#id").
    pub fn cue_with(&mut self, cue: &Cue, lookup: SoundLookup) {
        let custom = |name: &str| name.strip_prefix('#').and_then(|id| id.parse::<u64>().ok());
        match cue {
            Cue::Sound(name) if custom(name).is_some() => {
                if let (Some(s), Some((bytes, volume))) = (&mut self.speaker, lookup(custom(name).unwrap())) {
                    s.play_file(bytes, volume);
                }
                self.log_line("sound", name);
            }
            Cue::Music(Some(name)) if custom(name).is_some() => {
                match lookup(custom(name).unwrap()) {
                    Some((bytes, volume)) => {
                        self.pending_music = None;
                        if let Some(s) = &mut self.speaker {
                            s.music_file(name, bytes, volume);
                        }
                        self.log_line("music", name);
                    }
                    // Not here yet: remember it (see retry_music).
                    None => self.pending_music = Some(name.clone()),
                }
            }
            Cue::Music(_) => {
                self.pending_music = None;
                self.cue(cue);
            }
            other => self.cue(other),
        }
    }

    /// Starts a song that was waiting for its audio, if it's arrived now.
    /// Call every frame.
    pub fn retry_music(&mut self, lookup: SoundLookup) {
        if let Some(name) = self.pending_music.take() {
            self.cue_with(&Cue::Music(Some(name)), lookup);
        }
    }

    /// The song waiting for its audio, if any.
    pub fn pending_music(&self) -> Option<&str> {
        self.pending_music.as_deref()
    }

    fn log_line(&mut self, kind: &str, name: &str) {
        let t = self.start.elapsed().as_secs_f32();
        if let Some(f) = &mut self.log {
            let _ = writeln!(f, "{t:.3} {kind} {name}");
        }
    }

    pub fn cue(&mut self, cue: &Cue) {
        let t = self.start.elapsed().as_secs_f32();
        match cue {
            Cue::Sound(name) => {
                if let Some(s) = &mut self.speaker {
                    s.play(name);
                }
                if let Some(f) = &mut self.log {
                    let _ = writeln!(f, "{t:.3} sound {name}");
                }
            }
            Cue::Music(name) => {
                if let Some(s) = &mut self.speaker {
                    s.music(name.as_deref());
                }
                if let Some(f) = &mut self.log {
                    let _ = writeln!(f, "{t:.3} music {}", name.as_deref().unwrap_or("-"));
                }
            }
        }
    }

    /// Plays an audio file once (the studio's Preview button).
    pub fn play_bytes(&mut self, bytes: std::sync::Arc<Vec<u8>>, volume: f32) {
        if let Some(s) = &mut self.speaker {
            s.play_file(bytes, volume);
        }
    }

    pub fn sound(&mut self, name: &str) {
        self.cue(&Cue::Sound(name.to_string()));
    }

    pub fn stop_music(&mut self) {
        self.cue(&Cue::Music(None));
    }
}

impl Default for Audio {
    fn default() -> Self {
        Self::new()
    }
}

/// Looks Sounds up in a local world (Studio Play, single player).
pub fn world_sound(world: &brixo_core::DataModel, id: u64) -> Option<(std::sync::Arc<Vec<u8>>, f32)> {
    let s = world.sound(brixo_core::InstanceId::from_raw(id))?;
    Some((std::sync::Arc::new(s.bytes()?), s.volume))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_song_that_arrives_late_still_starts() {
        let mut audio = Audio::new();
        let song = std::sync::Arc::new(vec![1u8, 2, 3]);
        // "Play song #7" arrives before the song does.
        audio.cue_with(&Cue::Music(Some("#7".into())), &|_| None);
        assert_eq!(audio.pending_music(), Some("#7"), "remembered");
        audio.retry_music(&|_| None);
        assert_eq!(audio.pending_music(), Some("#7"), "still waiting");
        // Now it's here.
        audio.retry_music(&|id| (id == 7).then(|| (song.clone(), 1.0)));
        assert_eq!(audio.pending_music(), None, "started");
        // Stopping the music forgets any waiting song.
        audio.cue_with(&Cue::Music(Some("#9".into())), &|_| None);
        audio.cue_with(&Cue::Music(None), &|_| None);
        assert_eq!(audio.pending_music(), None);
    }
}
