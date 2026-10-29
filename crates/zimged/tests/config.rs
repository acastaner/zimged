//! `config.toml` (§8.3): defaults, the corrupt-file rename, debounced writes, the remembered window
//! and the *Allow multiple instances* toggle.

mod support;

use std::sync::Arc;

use support::TestApp;
use zimged::Launch;
use zimged_platform::{Config, MemoryConfigStore, OsConfigStore, Platform};

fn platform_with(store: &Arc<MemoryConfigStore>) -> Platform {
    Platform::in_memory().with_config(store.clone())
}

#[test]
fn keys_in_the_file_override_the_defaults_and_unknown_keys_are_ignored() {
    let store = Arc::new(MemoryConfigStore::with_text(
        "version = 1\n\
         jpeg_quality = 80\n\
         from_the_future = \"ignored\"\n\
         [window]\n\
         width = 1200.0\n",
    ));
    let app = TestApp::with_platform(platform_with(&store));

    let config = app.harness.state().config().clone();
    assert_eq!(config.jpeg_quality, 80);
    assert_eq!(config.window.width, 1200.0);
    assert_eq!(config.window.height, 1000.0);
    assert!(!config.window.maximised);
    assert!(config.allow_multiple_instances);
    assert_eq!(config.magnify_factor, 2.0);
}

#[test]
fn a_corrupt_file_is_set_aside_and_the_defaults_are_used() {
    let corrupt = "version = 1\njpeg_quality = = 80\n";
    let store = Arc::new(MemoryConfigStore::with_text(corrupt));
    let app = TestApp::with_platform(platform_with(&store));

    assert_eq!(app.harness.state().config(), &Config::default());
    assert_eq!(store.bad_text().as_deref(), Some(corrupt));
    assert_eq!(store.text(), None);
}

#[test]
fn a_known_key_with_the_wrong_type_makes_the_file_corrupt() {
    let store = Arc::new(MemoryConfigStore::with_text(
        "version = 1\njpeg_quality = \"high\"\n",
    ));
    let app = TestApp::with_platform(platform_with(&store));

    assert_eq!(app.harness.state().config(), &Config::default());
    assert!(store.bad_text().is_some());
}

#[test]
fn on_disk_a_corrupt_config_toml_is_renamed_to_config_toml_bad() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "not toml at all [").unwrap();
    let platform = Platform::in_memory().with_config(Arc::new(OsConfigStore::at(&path)));
    let app = TestApp::with_platform(platform);

    assert_eq!(app.harness.state().config(), &Config::default());
    assert!(!path.exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("config.toml.bad")).unwrap(),
        "not toml at all ["
    );
}

#[test]
fn the_window_size_is_remembered_for_the_next_run() {
    let store = Arc::new(MemoryConfigStore::default());
    let mut app = TestApp::with_platform(platform_with(&store));

    app.resize_window(egui::vec2(1280.0, 800.0), false);
    app.settle();

    let text = store.text().expect("config.toml written");
    assert!(text.starts_with("version = 1\n"), "{text}");
    let next = Launch::new(platform_with(&store));
    assert_eq!(
        (next.config.window.width, next.config.window.height),
        (1280.0, 800.0)
    );
    assert!(!next.config.window.maximised);
}

#[test]
fn the_maximised_state_is_remembered_with_the_restored_size() {
    let store = Arc::new(MemoryConfigStore::default());
    let mut app = TestApp::with_platform(platform_with(&store));

    app.resize_window(egui::vec2(1280.0, 800.0), false);
    app.resize_window(egui::vec2(1920.0, 1050.0), true);
    app.settle();

    let next = Launch::new(platform_with(&store));
    assert!(next.config.window.maximised);
    assert_eq!(
        (next.config.window.width, next.config.window.height),
        (1280.0, 800.0)
    );
}

#[test]
fn on_disk_the_config_is_written_whole_with_no_temporary_file_left() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("zimged").join("config.toml");
    let platform = Platform::in_memory().with_config(Arc::new(OsConfigStore::at(&path)));
    let mut app = TestApp::with_platform(platform);

    app.resize_window(egui::vec2(1280.0, 800.0), false);
    app.settle();

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("version = 1\n"), "{text}");
    let written = Config::parse(&text).unwrap();
    assert_eq!(written.window.width, 1280.0);
    let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["config.toml"]);
}

#[test]
fn the_allow_multiple_instances_toggle_in_the_view_menu_persists() {
    use egui_kittest::kittest::Queryable;

    let store = Arc::new(MemoryConfigStore::default());
    let mut app = TestApp::with_platform(platform_with(&store));

    app.harness.get_by_label("View").click();
    app.harness.run();
    app.harness.get_by_label("Allow multiple instances").click();
    app.settle();

    assert!(!app.harness.state().config().allow_multiple_instances);
    let next = Launch::new(platform_with(&store));
    assert!(!next.config.allow_multiple_instances);
}

#[test]
fn writes_are_debounced() {
    let store = Arc::new(MemoryConfigStore::default());
    let mut app = TestApp::with_platform(platform_with(&store));

    for width in [1100.0, 1150.0, 1200.0, 1250.0] {
        app.resize_window(egui::vec2(width, 800.0), false);
    }
    assert_eq!(store.write_count(), 0, "written before the debounce delay");
    app.settle();

    assert_eq!(store.write_count(), 1);
    assert_eq!(app.harness.state().config().window.width, 1250.0);
}

#[test]
fn no_file_gives_the_defaults() {
    let app = TestApp::new();

    let config = app.harness.state().config().clone();
    assert_eq!(config, Config::default());
    assert!(config.allow_multiple_instances);
    assert_eq!(config.jpeg_quality, 92);
    assert_eq!(
        (config.window.width, config.window.height),
        (1600.0, 1000.0)
    );
    assert!(!config.window.maximised);
    assert_eq!(config.magnify_factor, 2.0);
}
