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
mod editor;
mod input;
mod pan;
mod paste;
mod save;
mod siblings;
pub mod startup;
pub mod tools;
mod ui;
pub mod view;

pub use app::{Launch, ZimgedApp};
pub use commands::Command;
