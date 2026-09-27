//! One recursive watch per folder, shared by everything that follows it.
//!
//! Watching a folder recursively visits every folder and file in it
//! (inotify has to add a watch to each folder), which for a vault with a
//! large `.git` takes a noticeable while and uses one kernel watch per
//! folder. The workspace and the file tree both follow the vault, so they
//! share one watch: the first to ask starts it, and it stops when the last
//! one lets go.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, Weak};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

type Listener = Box<dyn Fn(&Event) + Send>;
type Listeners = Arc<Mutex<Vec<(usize, Listener)>>>;

/// A running watch and who hears its events.
struct Shared {
    listeners: Listeners,
    _watcher: RecommendedWatcher,
}

/// Running watches by folder.
static WATCHES: LazyLock<Mutex<HashMap<PathBuf, Weak<Shared>>>> = LazyLock::new(Mutex::default);

/// Hears a folder's changes until dropped.
pub struct WatchHandle {
    shared: Arc<Shared>,
    id: usize,
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        let mut listeners = lock(&self.shared.listeners);
        listeners.retain(|(id, _)| *id != self.id);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Calls `listener` with every change under `root`, on the watcher's
/// thread, until the handle is dropped. Starting a new watch blocks while
/// it visits the folder, so call this off the main thread.
pub fn watch(
    root: &Path,
    listener: impl Fn(&Event) + Send + 'static,
) -> notify::Result<WatchHandle> {
    let mut watches = lock(&WATCHES);
    watches.retain(|_, shared| shared.strong_count() > 0);
    let shared = match watches.get(root).and_then(Weak::upgrade) {
        Some(shared) => shared,
        None => {
            let shared = Arc::new(start(root)?);
            watches.insert(root.to_path_buf(), Arc::downgrade(&shared));
            shared
        }
    };
    drop(watches);
    let id = next_id();
    lock(&shared.listeners).push((id, Box::new(listener)));
    Ok(WatchHandle { shared, id })
}

fn start(root: &Path) -> notify::Result<Shared> {
    let listeners: Listeners = Arc::default();
    let heard = listeners.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        let Ok(event) = event else {
            return;
        };
        for (_, listener) in lock(&heard).iter() {
            listener(&event);
        }
    })?;
    watcher.watch(root, RecursiveMode::Recursive)?;
    Ok(Shared {
        listeners,
        _watcher: watcher,
    })
}

fn next_id() -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    #[test]
    fn two_listeners_share_one_watch() {
        let dir = tempfile::tempdir().unwrap();
        let (first_tx, first) = mpsc::channel();
        let (second_tx, second) = mpsc::channel();
        let one = watch(dir.path(), move |_| {
            first_tx.send(()).ok();
        })
        .unwrap();
        let two = watch(dir.path(), move |_| {
            second_tx.send(()).ok();
        })
        .unwrap();
        assert!(Arc::ptr_eq(&one.shared, &two.shared));
        std::fs::write(dir.path().join("a.md"), "A").unwrap();
        let wait = Duration::from_secs(5);
        assert!(first.recv_timeout(wait).is_ok());
        assert!(second.recv_timeout(wait).is_ok());
        drop(one);
        assert_eq!(lock(&two.shared.listeners).len(), 1);
        let shared = Arc::downgrade(&two.shared);
        drop(two);
        assert!(shared.upgrade().is_none());
    }
}
