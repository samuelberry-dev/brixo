//! Brixo multiplayer: a server that runs the game and streams it to
//! players, and the client side players use to connect.
//!
//! The server owns the world: physics, scripts and rules all run there.
//! Players send what they're pressing and draw what the server sends back.

pub mod client;
pub mod protocol;
pub mod server;

pub use client::NetClient;
pub use protocol::DEFAULT_PORT;
pub use server::{start, start_with_tickets, Identity, TicketCheck, ServerHandle};
