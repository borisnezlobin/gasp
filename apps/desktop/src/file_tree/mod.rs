//! The file explorer: a keyboard-first tree over the vault's folders,
//! notes, images and PDFs.
//!
//! [`FileTree`] is a GPUI view. It reports what the user did through
//! [`FileTreeEvent`] and never touches tabs or editors itself.

mod autoscroll;
mod keys;
mod menu;
mod model;
mod render;
mod view;
mod watch;

use std::path::PathBuf;

use gasp_vault::entries;
pub use gasp_vault::ops;

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
    /// path under `from` moved to the same place under `to`. Links to it
    /// are the workspace's to rewrite, in open editors where it can.
    Renamed { from: PathBuf, to: PathBuf },
    /// A note or folder was created.
    Created { path: PathBuf },
    /// A file or folder went to the trash.
    /// `text` is what a note read as it went; a folder has none.
    Trashed {
        path: PathBuf,
        text: Option<String>,
        /// Where in the trash it went, when that's known.
        trashed_to: Option<PathBuf>,
    },
    /// Folders were expanded or collapsed. Read them with
    /// [`FileTree::expanded_folders`] to save them.
    ExpansionChanged,
    /// Escape was pressed with nothing to cancel: focus the editor.
    Dismissed,
    /// An operation failed. The message is short and ready to show.
    Failed { message: String },
}
