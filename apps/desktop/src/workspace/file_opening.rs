//! What opening a file does, by its kind: a note or an image gets a tab,
//! and anything else, such as a PDF, opens in the system's default app.
//! Image tabs follow their file on disk as the watcher reports it.

use std::io;
use std::path::Path;

use gasp_vault::entries::EntryKind;
use gpui::{AppContext, Context, Entity, Window};

use super::Workspace;
use super::image_tab::ImageView;
use super::pane::{Pane, Tab, TabContent};
use super::watcher::DiskChange;

/// Extensions besides `.md` that open as notes.
const OTHER_NOTE_EXTENSIONS: [&str; 2] = ["markdown", "txt"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileOpening {
    Note,
    Image,
    SystemApp,
}

/// How the file at `path` opens, by its extension. A file without one is
/// taken for a note.
pub(crate) fn opening_for(path: &Path) -> FileOpening {
    let name = path.file_name().map(|name| name.to_string_lossy());
    match name.as_deref().and_then(EntryKind::of_file) {
        Some(EntryKind::Note) => FileOpening::Note,
        Some(EntryKind::Image) => FileOpening::Image,
        Some(EntryKind::Pdf | EntryKind::Folder) => FileOpening::SystemApp,
        None => opening_by_other_extension(path),
    }
}

fn opening_by_other_extension(path: &Path) -> FileOpening {
    let Some(extension) = path.extension() else {
        return FileOpening::Note;
    };
    let extension = extension.to_string_lossy().to_ascii_lowercase();
    if OTHER_NOTE_EXTENSIONS.contains(&extension.as_str()) {
        FileOpening::Note
    } else {
        FileOpening::SystemApp
    }
}

impl Workspace {
    /// A new tab on the file at `path`, or `None` when the file went to
    /// the system's default app instead, as a PDF or a note that isn't
    /// text does.
    pub(crate) fn tab_for_path(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> io::Result<Option<Tab>> {
        match opening_for(path) {
            FileOpening::Note => self.note_tab_unless_binary(path, window, cx),
            FileOpening::Image => Ok(Some(self.image_tab(path, window, cx))),
            FileOpening::SystemApp => {
                crate::sandbox::open_with_system(path, cx);
                Ok(None)
            }
        }
    }

    fn note_tab_unless_binary(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> io::Result<Option<Tab>> {
        match self.note_tab(path, window, cx) {
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                crate::sandbox::open_with_system(path, cx);
                Ok(None)
            }
            opened => opened.map(Some),
        }
    }

    /// A new tab on the image at `path`.
    fn image_tab(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) -> Tab {
        let image = cx.new(|cx| ImageView::new(path, window, cx));
        let subscriptions = vec![cx.observe(&image, |_, _, cx| cx.notify())];
        Tab::new(TabContent::Image(image), subscriptions)
    }

    /// Every image open in a tab.
    pub(crate) fn image_views(&self, cx: &gpui::App) -> Vec<Entity<ImageView>> {
        self.panes
            .panes()
            .iter()
            .flat_map(|pane| {
                pane.read(cx)
                    .tabs()
                    .iter()
                    .filter_map(|tab| tab.image().cloned())
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Closes every tab on the image at `path`, which is going away.
    pub(crate) fn close_image_tabs(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for pane in self.panes.panes() {
            for index in image_tabs_on(&pane, path, cx) {
                if self.panes.contains(&pane) {
                    self.close_tab_now(&pane, index, false, window, cx);
                }
            }
        }
        self.closed_tabs.retain(|closed| closed != path);
    }

    /// Image tabs follow their files through what the watcher saw: moved
    /// ones take the new path, and changed or removed ones read their
    /// file again.
    pub(crate) fn follow_image_changes(&mut self, changes: &[DiskChange], cx: &mut Context<Self>) {
        for image in self.image_views(cx) {
            let path = image.read(cx).path().to_path_buf();
            if let Some(moved) = changes.iter().find_map(|change| moved_path(change, &path)) {
                image.update(cx, |image, cx| image.set_path(&moved, cx));
            } else if changes.iter().any(|change| touches(change, &path)) {
                image.update(cx, |image, cx| image.refresh(cx));
            }
        }
    }
}

/// The indexes of `pane`'s tabs on the image at `path`, last first so
/// they can be closed in order.
fn image_tabs_on(pane: &Entity<Pane>, path: &Path, cx: &gpui::App) -> Vec<usize> {
    let pane = pane.read(cx);
    let shows_path = |tab: &Tab| {
        tab.image()
            .is_some_and(|image| image.read(cx).path() == path)
    };
    (0..pane.len())
        .rev()
        .filter(|&index| shows_path(&pane.tabs()[index]))
        .collect()
}

/// Where `path` went, when `change` moved it or a folder holding it.
fn moved_path(change: &DiskChange, path: &Path) -> Option<std::path::PathBuf> {
    let DiskChange::Renamed { from, to } = change else {
        return None;
    };
    let rest = path.strip_prefix(from).ok()?;
    Some(if rest.as_os_str().is_empty() {
        to.clone()
    } else {
        to.join(rest)
    })
}

fn touches(change: &DiskChange, path: &Path) -> bool {
    change
        .paths()
        .iter()
        .any(|changed| path.starts_with(changed))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn files_open_by_their_kind() {
        assert_eq!(opening_for(Path::new("a/Note.md")), FileOpening::Note);
        assert_eq!(opening_for(Path::new("Plain")), FileOpening::Note);
        assert_eq!(opening_for(Path::new("list.TXT")), FileOpening::Note);
        assert_eq!(opening_for(Path::new("pic.PNG")), FileOpening::Image);
        assert_eq!(opening_for(Path::new("paper.pdf")), FileOpening::SystemApp);
        assert_eq!(opening_for(Path::new("song.mp3")), FileOpening::SystemApp);
    }

    #[test]
    fn a_moved_folder_takes_its_images_along() {
        let change = DiskChange::Renamed {
            from: PathBuf::from("/v/old"),
            to: PathBuf::from("/v/new"),
        };
        assert_eq!(
            moved_path(&change, Path::new("/v/old/pic.png")),
            Some(PathBuf::from("/v/new/pic.png"))
        );
        assert_eq!(moved_path(&change, Path::new("/v/other.png")), None);
    }
}
