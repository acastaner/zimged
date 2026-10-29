//! OS integration behind adapter traits (spec §7.1, §8, §9).
//!
//! The app only ever talks to the OS through a [`Platform`]: [`Platform::os`] in the real binary,
//! [`Platform::in_memory`] in the headless App tests. Each adapter is a trait with a real
//! implementation and an in-memory fake next to it. Clipboard, trash and the single instance socket join [`Platform`] as fields when their tickets land.

pub mod config;
pub mod dialogs;

use std::sync::Arc;

pub use config::{Config, ConfigStore, MemoryConfigStore, OsConfigStore};
pub use dialogs::{DialogReply, FileDialogs, MemoryFileDialogs, OsFileDialogs};

/// Every OS adapter the app uses. Cheap to clone.
#[derive(Clone)]
pub struct Platform {
    /// Where `config.toml` lives (§8.3).
    pub config: Arc<dyn ConfigStore>,
    /// Native file dialogs (§8.2).
    pub dialogs: Arc<dyn FileDialogs>,
}

impl Platform {
    /// The real adapters.
    pub fn os() -> Self {
        Self {
            config: Arc::new(OsConfigStore::new()),
            dialogs: Arc::new(OsFileDialogs),
        }
    }

    /// In-memory fakes for tests: no file system, clipboard, trash or dialogs are touched.
    pub fn in_memory() -> Self {
        Self {
            config: Arc::new(MemoryConfigStore::default()),
            dialogs: Arc::new(MemoryFileDialogs::default()),
        }
    }

    /// The same adapters, with this config store.
    pub fn with_config(mut self, store: Arc<dyn ConfigStore>) -> Self {
        self.config = store;
        self
    }
}
