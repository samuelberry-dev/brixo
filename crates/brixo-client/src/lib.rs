//! What every Brixo program that plays games shares: the studio's Play
//! mode and the standalone player both use this, so games feel the same.

pub mod filming;
pub mod gui;
pub mod install;
pub mod installer;
pub mod library;
pub mod play;
pub mod smooth;
pub mod theme;
pub mod sound;

pub use gui::{ChatLog, draw_beacons, draw_gui, draw_hotbar, hotbar_key, visible_gui, GuiEvents, Project};
pub use library::{games_dir, list_games, list_games_in, publish, publish_to, GameEntry};
pub use smooth::Smoother;
pub use sound::{world_sound, Audio};
pub use play::{first_hit, keyboard_look, projector, movement_input, trackpad_look, CameraKeys, FollowCamera, Held, MIN_FOLLOW_DISTANCE};
