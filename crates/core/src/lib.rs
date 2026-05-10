//! Open-Lid core: platform-agnostic types, state machine, IPC schemas.
//!
//! This crate must compile on any target — no Apple frameworks, no IOKit,
//! no AppKit. Anything platform-specific lives in `crates/app` or
//! `crates/helper` under a `platform/<os>/` subdirectory.

pub mod config;
pub mod ipc;
pub mod mode;
pub mod platform;
pub mod state;
