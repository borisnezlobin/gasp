//! Hot reload: watch the config folder and emit the new config after each change.

use std::collections::BTreeSet;
use std::fs;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::diagnostics::Diagnostic;
use crate::loader::{Config, ConfigFile, ConfigLoader};

/// Changes that arrive this close together are handled as one reload.
const DEBOUNCE: Duration = Duration::from_millis(40);

/// The result of one reload. `config` is always usable: parts with errors keep
/// their last good version and the errors are in `diagnostics`.
#[derive(Clone, Debug)]
pub struct ConfigUpdate {
    pub config: Arc<Config>,
    pub diagnostics: Vec<Diagnostic>,
    pub files: Vec<ConfigFile>,
}

/// Watches a config folder until dropped.
pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
    updates: Receiver<ConfigUpdate>,
}

impl ConfigWatcher {
    /// Creates the folder if needed, loads it once and starts watching.
    /// Returns the watcher and the initial load.
    pub fn start(mut loader: ConfigLoader) -> notify::Result<(ConfigWatcher, ConfigUpdate)> {
        fs::create_dir_all(loader.dir()).map_err(notify::Error::io)?;
        let diagnostics = loader.load_all();
        let initial = ConfigUpdate {
            config: Arc::new(loader.config().clone()),
            diagnostics,
            files: ConfigFile::ALL.to_vec(),
        };
        let (event_tx, event_rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(event_tx)?;
        watcher.watch(loader.dir(), RecursiveMode::NonRecursive)?;
        let (update_tx, updates) = mpsc::channel();
        thread::spawn(move || reload_loop(loader, event_rx, update_tx));
        Ok((
            ConfigWatcher {
                _watcher: watcher,
                updates,
            },
            initial,
        ))
    }

    /// The stream of updates, for hosts with their own event loop.
    pub fn updates(&self) -> &Receiver<ConfigUpdate> {
        &self.updates
    }

    /// Waits up to `timeout` for the next update.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<ConfigUpdate> {
        self.updates.recv_timeout(timeout).ok()
    }
}

type NotifyEvents = Receiver<notify::Result<notify::Event>>;

fn reload_loop(mut loader: ConfigLoader, events: NotifyEvents, updates: Sender<ConfigUpdate>) {
    while let Ok(first) = events.recv() {
        let mut changed = changed_files(first);
        if !collect_burst(&events, &mut changed) {
            return;
        }
        if changed.is_empty() {
            continue;
        }
        let update = reload(&mut loader, changed);
        if updates.send(update).is_err() {
            return;
        }
    }
}

/// Gathers events until the folder is quiet. Returns false once the watcher is gone.
fn collect_burst(events: &NotifyEvents, changed: &mut BTreeSet<ConfigFile>) -> bool {
    loop {
        match events.recv_timeout(DEBOUNCE) {
            Ok(event) => changed.extend(changed_files(event)),
            Err(RecvTimeoutError::Timeout) => return true,
            Err(RecvTimeoutError::Disconnected) => return false,
        }
    }
}

fn changed_files(event: notify::Result<notify::Event>) -> BTreeSet<ConfigFile> {
    let Ok(event) = event else {
        return BTreeSet::new();
    };
    if event.kind.is_access() {
        return BTreeSet::new();
    }
    event
        .paths
        .iter()
        .filter_map(|path| ConfigFile::from_path(path))
        .collect()
}

fn reload(loader: &mut ConfigLoader, changed: BTreeSet<ConfigFile>) -> ConfigUpdate {
    let files: Vec<ConfigFile> = changed.into_iter().collect();
    let diagnostics = files.iter().flat_map(|file| loader.reload(*file)).collect();
    ConfigUpdate {
        config: Arc::new(loader.config().clone()),
        diagnostics,
        files,
    }
}
