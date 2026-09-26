//! Runs Brixo games: connects Rovik scripts to the world.

pub mod game;
pub mod host;
pub mod physics;

pub use host::{Cue, SoundEvent};
pub use game::{filter_chat, backpack, gui_shown, Game, Look, LogLine, WorldMutexGuard, EVENTS, FALL_LIMIT, RESPAWN_TIME, GAME_STEP_LIMIT, PANTS_COLORS, SHIRT_COLORS, SHOES_COLORS, SKIN_TONES};
pub use physics::{PlayerInput, COYOTE_TIME, GRAVITY, JUMP_BUFFER, JUMP_HOLD_GRAVITY, MAX_FALL_SPEED, PHYSICS_DT};
