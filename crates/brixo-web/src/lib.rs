//! The Brixo website: accounts, avatars, the game catalog, and the game
//! servers that Play starts. Clients never download games: they join a
//! server the website runs, the way Roblox works.

pub mod api;
pub mod db;
pub mod docs;
pub mod limits;
pub mod servers;
pub mod shop;
