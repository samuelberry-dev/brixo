//! Sample games, built in code so they're reproducible and tested. Open
//! them in Brixo Studio to see how they work, and change anything.

mod battle;
pub mod flagbots;
pub mod flagfall;
pub mod gears;
pub mod lab;
pub mod speedway;
pub mod synth;
mod tycoon;

pub use battle::spire_wars;
pub use tycoon::coin_tycoon;
pub use flagfall::flagfall;
