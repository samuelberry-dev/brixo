//! What every Brixo program that plays games shares: the studio's Play
//! mode and the standalone player both use this, so games feel the same.

pub mod library;
pub mod play;

pub use library::{games_dir, list_games, list_games_in, publish, publish_to, GameEntry};
pub use play::{movement_input, FollowCamera, Held, MIN_FOLLOW_DISTANCE};
