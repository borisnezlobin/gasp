//! Watching the vault folder so the tree refreshes when files change
//! outside it, such as after a sync.

use std::path::{Component, Path, PathBuf};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use notify::event::ModifyKind;
use notify::{Event, EventKind};

use super::entries::is_hidden;
use crate::vault_watch::WatchHandle;

/// Keeps the OS watch alive until dropped.
pub struct VaultWatcher {
    _watch: WatchHandle,
}

/// Starts watching `root`. The receiver gets one message per relevant
/// change; the tree debounces them.
pub fn watch(root: &Path) -> notify::Result<(VaultWatcher, UnboundedReceiver<()>)> {
    let (sender, receiver) = unbounded();
    // macOS reports events under the resolved path (`/private/var/…` for a
    // vault opened as `/var/…`), so both spellings count as the vault.
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let roots = [root.to_path_buf(), canonical];
    let watch = crate::vault_watch::watch(root, move |event| forward(&roots, event, &sender))?;
    Ok((VaultWatcher { _watch: watch }, receiver))
}

fn forward(roots: &[PathBuf], event: &Event, sender: &UnboundedSender<()>) {
    if roots.iter().any(|root| is_relevant(root, event)) {
        let _ = sender.unbounded_send(());
    }
}

/// Changes to the tree's shape count; reads, and anything inside a hidden
/// folder such as `.git`, don't.
pub fn is_relevant(root: &Path, event: &Event) -> bool {
    let shape_change = matches!(
        event.kind,
        EventKind::Create(_)
            | EventKind::Remove(_)
            | EventKind::Modify(ModifyKind::Name(_) | ModifyKind::Any)
            | EventKind::Any
    );
    shape_change && event.paths.iter().any(|path| is_visible(root, path))
}

fn is_visible(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    !relative.components().any(|component| match component {
        Component::Normal(name) => name.to_str().is_some_and(is_hidden),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use notify::event::{AccessKind, CreateKind};

    use super::*;

    fn event(kind: EventKind, path: &str) -> Event {
        Event::new(kind).add_path(Path::new("/vault").join(path))
    }

    #[test]
    fn hidden_folders_and_reads_are_ignored() {
        let root = Path::new("/vault");
        let create = EventKind::Create(CreateKind::File);
        assert!(is_relevant(root, &event(create, "Notes/a.md")));
        assert!(!is_relevant(root, &event(create, ".git/objects/ab")));
        assert!(!is_relevant(root, &event(create, "Notes/.DS_Store")));
        let read = EventKind::Access(AccessKind::Read);
        assert!(!is_relevant(root, &event(read, "Notes/a.md")));
        let write = EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Content));
        assert!(!is_relevant(root, &event(write, "Notes/a.md")));
    }
}
