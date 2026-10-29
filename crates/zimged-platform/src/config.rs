//! `config.toml` (§8.3): where it is stored, and the settings read from it.
//!
//! Parsing, `version = 1`, the corrupt-file rename and debounced writes are the Config file ticket
//! (#24). Until then [`Config::load`] reads the file (so the startup path pays the real I/O cost)
//! and returns the defaults.

use std::io;
use std::path::PathBuf;
use std::sync::Mutex;

/// Reads and writes the raw text of `config.toml`.
pub trait ConfigStore: Send + Sync {
    /// The file's text, or `None` when there is no file yet.
    fn read(&self) -> io::Result<Option<String>>;
    fn write(&self, text: &str) -> io::Result<()>;
}

/// `config.toml` in the OS's user-config directory, resolved with `directories`.
pub struct OsConfigStore {
    path: Option<PathBuf>,
}

impl OsConfigStore {
    pub fn new() -> Self {
        let path = directories::ProjectDirs::from("", "", "zimged")
            .map(|dirs| dirs.config_dir().join("config.toml"));
        Self { path }
    }
}

impl Default for OsConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigStore for OsConfigStore {
    fn read(&self) -> io::Result<Option<String>> {
        let Some(path) = &self.path else {
            return Ok(None);
        };
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn write(&self, text: &str) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Err(io::Error::other("no config directory"));
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, text)
    }
}

/// An in-memory `config.toml`, for tests.
#[derive(Default)]
pub struct MemoryConfigStore {
    text: Mutex<Option<String>>,
}

impl MemoryConfigStore {
    pub fn with_text(text: impl Into<String>) -> Self {
        Self {
            text: Mutex::new(Some(text.into())),
        }
    }

    /// What was last written (or the initial text).
    pub fn text(&self) -> Option<String> {
        self.text.lock().unwrap().clone()
    }
}

impl ConfigStore for MemoryConfigStore {
    fn read(&self) -> io::Result<Option<String>> {
        Ok(self.text())
    }

    fn write(&self, text: &str) -> io::Result<()> {
        *self.text.lock().unwrap() = Some(text.to_owned());
        Ok(())
    }
}

/// The remembered window geometry, in logical points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowConfig {
    pub width: f32,
    pub height: f32,
    pub maximised: bool,
}

/// Settings from `config.toml`, with the §8.3 defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub allow_multiple_instances: bool,
    pub window: WindowConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            allow_multiple_instances: true,
            window: WindowConfig {
                width: 1600.0,
                height: 1000.0,
                maximised: false,
            },
        }
    }
}

impl Config {
    /// Read the config. Never fails: a missing or unreadable file gives the defaults.
    pub fn load(store: &dyn ConfigStore) -> Self {
        // #24 parses the text here.
        let _text = store.read().ok().flatten();
        Self::default()
    }
}
