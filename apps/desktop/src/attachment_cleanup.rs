//! Offers to delete an image pasted into a note once its embed is gone
//! again, so pastes that didn't work out don't pile up beside the note.
//! Only images pasted while the note is open count, and nothing goes
//! without a click: the offer is a notice whose button deletes the file
//! the way `files.trash` says. Undoing the removal withdraws the offer.

use std::path::{Path, PathBuf};

use gpui::{App, Context, Global, Window};

use crate::editor::EditorView;
use crate::notices::{self, Notice};
use crate::workspace::Workspace;

/// The command the offer's button runs.
pub const DELETE_COMMAND: &str = "attachment.delete-removed-image";

/// An image pasted into a note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PastedImage {
    pub file: PathBuf,
    /// The note it was pasted into, which doesn't count as still using it.
    pub note: Option<PathBuf>,
    offered: bool,
}

impl PastedImage {
    pub fn new(file: PathBuf, note: Option<PathBuf>) -> Self {
        PastedImage {
            file,
            note,
            offered: false,
        }
    }

    fn name(&self) -> String {
        self.file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// The deletion on offer. One at a time: a newer offer replaces it.
struct Offer {
    image: PastedImage,
    notice: u64,
}

#[derive(Default)]
struct Offered(Option<Offer>);

impl Global for Offered {}

impl EditorView {
    /// Offers to delete each pasted image whose embed is no longer in the
    /// note, and withdraws the offer when the embed comes back.
    pub(crate) fn follow_pasted_images(&mut self, cx: &mut Context<Self>) {
        if self.pasted_images.is_empty() {
            return;
        }
        let text = self.source.text();
        for image in &mut self.pasted_images {
            let embedded = text.contains(image.name().as_str());
            match (embedded, image.offered) {
                (false, false) => {
                    image.offered = true;
                    offer(image.clone(), cx);
                }
                (true, true) => {
                    image.offered = false;
                    withdraw(&image.file, cx);
                }
                _ => {}
            }
        }
    }
}

fn offer(image: PastedImage, cx: &mut App) {
    let message = format!("“{}” isn’t in the note anymore.", image.name());
    let notice = Notice::offer(message).with_action("Delete the image", DELETE_COMMAND);
    let replaced = cx
        .default_global::<Offered>()
        .0
        .take()
        .map(|offer| offer.notice);
    let id = notices::replace(replaced, notice, cx);
    cx.global_mut::<Offered>().0 = Some(Offer { image, notice: id });
}

fn withdraw(file: &Path, cx: &mut App) {
    let offered = cx.default_global::<Offered>();
    if offered
        .0
        .as_ref()
        .is_none_or(|offer| offer.image.file != file)
    {
        return;
    }
    if let Some(offer) = offered.0.take() {
        notices::dismiss(offer.notice, cx);
    }
}

/// The offer's button: deletes the image unless another note shows it.
pub fn delete_offered_image(
    workspace: &mut Workspace,
    _: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some(offer) = cx.default_global::<Offered>().0.take() else {
        return;
    };
    notices::dismiss(offer.notice, cx);
    let name = offer.image.name();
    let vault = workspace.vault().to_path_buf();
    let Ok(relative) = offer.image.file.strip_prefix(&vault) else {
        return;
    };
    if let Some(other) = other_note_showing(workspace, &offer.image, cx) {
        let message = format!("“{other}” still shows “{name}”, so it stays.");
        notices::show(Notice::problem(message), cx);
        return;
    }
    let mode = workspace.config().settings.files.trash;
    match crate::trashing::move_to_trash(&vault, relative, mode) {
        Ok(_) => notices::show(Notice::done(format!("Deleted “{name}”.")), cx),
        Err(error) => notices::problem(format!("Couldn’t delete “{name}”: {error}"), cx),
    };
}

/// The title of a note other than the one it was pasted into that links
/// to `image`, if any.
fn other_note_showing(workspace: &Workspace, image: &PastedImage, cx: &App) -> Option<String> {
    let vault = workspace.vault();
    let target = crate::vault_index::vault_relative(vault, &image.file)?;
    let pasted_into = image
        .note
        .as_deref()
        .and_then(|note| crate::vault_index::vault_relative(vault, note));
    workspace
        .vault_index()
        .read(cx)
        .links()
        .linking_to(&target)
        .into_iter()
        .find(|note| Some(note) != pasted_into.as_ref())
        .map(|note| crate::workspace::files::note_title(Path::new(&note)))
}
