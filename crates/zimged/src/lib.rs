//! The zimged app: the startup path (§1.3), the eframe app, the egui chrome and input → commands.
//!
//! The binary (`main.rs`) only calls [`startup::run`]. Everything else lives in this library so the
//! headless App tests (spec §9) drive exactly the code the binary runs.

mod animation;
pub mod app;
pub mod bench;
pub mod commands;
mod config_save;
pub mod decode;
mod input;
mod pan;
pub mod startup;
mod ui;
pub mod view;

pub use app::{Launch, ZimgedApp};
pub use commands::Command;
