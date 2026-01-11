//! Language Server Protocol implementation for CantaLoop.
//!
//! This is a thin protocol adapter over the compiler session.
//! It never re-implements language logic - it only queries compiler state.

pub mod server;
pub mod handlers;
pub mod mapping;

pub use server::CantaLoopServer;
