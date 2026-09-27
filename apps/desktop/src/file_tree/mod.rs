//! The file explorer: a keyboard-first tree over the vault's folders,
//! notes, images and PDFs.
//!
//! [`FileTree`] is a GPUI view. It reports what the user did through
//! [`FileTreeEvent`] and never touches tabs or editors itself.

mod entries;
mod keys;
mod menu;
mod model;
pub mod ops;
mod render;
mod view;
mod watch;

use std::path::PathBuf;

pub use entries::{Entry, EntryKind, SortOrder, display_name, natural_cmp};
pub use keys::KEY_HINTS;
pub use menu::MenuItem;
pub use model::{Row, TreeModel};
pub use view::{FileTree, FileTreeOptions};

/// What the tree tells the workspace. Paths are absolute.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileTreeEvent {
    /// Open a file, in a new tab when `new_tab` is set.
    Open { path: PathBuf, new_tab: bool },
    /// A file or folder was renamed or moved. For a folder, every open
    /// path under `from` moved to the same place under `to`.
    Renamed { from: PathBuf, to: PathBuf },
    /// Notes whose links were rewritten after a rename. Open copies of
    /// them should reload.
    LinksUpdated { paths: Vec<PathBuf> },
    /// A note or folder was created.
    Created { path: PathBuf },
    /// A file or folder went to the trash.
    Trashed { path: PathBuf },
    /// Folders were expanded or collapsed. Read them with
    /// [`FileTree::expanded_folders`] to save them.
    ExpansionChanged,
    /// Escape was pressed with nothing to cancel: focus the editor.
    Dismissed,
    /// An operation failed. The message is short and ready to show.
    Failed { message: String },
}
