//! Runs Brixo games: connects Rovik scripts to the world.

pub mod game;
pub mod host;
pub mod physics;

pub use game::{Game, LogLine, EVENTS, FALL_LIMIT, GAME_STEP_LIMIT};
pub use physics::{PlayerInput, GRAVITY, PHYSICS_DT};
