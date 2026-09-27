//! Hot reload through the file watcher.

use std::fs;
use std::time::{Duration, Instant};

use editor_config::settings::SidebarMode;
use editor_config::{ConfigFile, ConfigLoader, ConfigUpdate, ConfigWatcher};
use tempfile::TempDir;

const WAIT: Duration = Duration::from_secs(10);

/// Waits for an update that satisfies `done`, skipping partial writes.
fn wait_for(watcher: &ConfigWatcher, done: impl Fn(&ConfigUpdate) -> bool) -> ConfigUpdate {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        let update = watcher.recv_timeout(Duration::from_millis(200));
        if let Some(update) = update.filter(|u| done(u)) {
            return update;
        }
    }
    panic!("no matching config update arrived");
}

#[test]
fn reloads_on_change_and_reports_errors() {
    let vault = TempDir::new().unwrap();
    let loader = ConfigLoader::for_vault(vault.path());
    let dir = loader.dir().to_path_buf();
    let (watcher, initial) = ConfigWatcher::start(loader).unwrap();
    assert!(initial.diagnostics.is_empty());
    assert!(dir.is_dir());

    fs::write(
        dir.join("settings.toml"),
        "[sidebar.files]\nmode = \"push\"\n",
    )
    .unwrap();
    let update = wait_for(&watcher, |u| {
        u.config.settings.sidebar.files.mode == SidebarMode::Push
    });
    assert!(update.files.contains(&ConfigFile::Settings));
    assert!(update.diagnostics.is_empty());

    fs::write(
        dir.join("settings.toml"),
        "[sidebar.files\nmode = \"overlay\"\n",
    )
    .unwrap();
    let broken = wait_for(&watcher, |u| !u.diagnostics.is_empty());
    assert_eq!(broken.diagnostics[0].file, "settings.toml");
    assert_eq!(broken.diagnostics[0].line, 1);
    assert_eq!(broken.config.settings.sidebar.files.mode, SidebarMode::Push);
}

#[test]
fn ignores_files_that_are_not_config() {
    let vault = TempDir::new().unwrap();
    let loader = ConfigLoader::for_vault(vault.path());
    let dir = loader.dir().to_path_buf();
    let (watcher, _) = ConfigWatcher::start(loader).unwrap();
    fs::write(dir.join("notes.txt"), "hello").unwrap();
    fs::write(dir.join("theme.toml"), "[color]\nblack = \"#444444\"\n").unwrap();
    let update = wait_for(&watcher, |u| {
        u.config.theme.text("color.accent") == Some("#444444")
    });
    assert_eq!(update.files, [ConfigFile::Theme]);
}
