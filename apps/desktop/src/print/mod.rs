//! Print. `app.print` opens a dialog with a live preview of the pages and
//! the settings PDF Export Plus had (paper, margins, page numbers, the
//! note's title and the drop cap). The note is laid out by Typst off the
//! main thread; every change lays it out again after a short pause, and
//! the old pages stay until the new ones are drawn.
//!
//! Print… hands the PDF to the system: the print panel on macOS, the PDF
//! viewer elsewhere. Save as PDF… asks where to save and writes the same
//! PDF. Enter prints, Mod-S saves, Escape closes; Up and Down move
//! between settings and Left, Right or Space change one.
//!
//! Wiring: on `app.print` the workspace builds a [`PrintDialog`] in its
//! modal slot, which it closes on the dialog's `DismissEvent`.

#[cfg(target_os = "macos")]
mod macos;
mod native;
pub mod preview;
pub mod settings;
mod view;

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    App, Context, DismissEvent, EventEmitter, FocusHandle, Focusable, Global, KeyBinding,
    ScrollHandle, Task, Window, actions,
};

use crate::export_ui;
use crate::theme::{SettingsTheme, UiTheme};
use crate::ui::{HasMenuSlot, MenuAnchor, MenuItem, MenuSlot};

pub use preview::{LayoutFailure, Preview, PreviewJob};
pub use settings::{Control, MarginSize, Paper, PrintSettings};

/// The key context the dialog sets.
pub const PRINT_CONTEXT: &str = "PrintDialog";

actions!(
    print_dialog,
    [
        NextSetting,
        PreviousSetting,
        ChangeNext,
        ChangePrevious,
        ToggleSetting,
        Print,
        SaveAsPdf,
        Cancel
    ]
);

/// The dialog's keys.
pub fn bind_keys(cx: &mut App) {
    let context = Some(PRINT_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("down", NextSetting, context),
        KeyBinding::new("up", PreviousSetting, context),
        KeyBinding::new("right", ChangeNext, context),
        KeyBinding::new("left", ChangePrevious, context),
        KeyBinding::new("space", ToggleSetting, context),
        KeyBinding::new("enter", Print, context),
        KeyBinding::new("secondary-s", SaveAsPdf, context),
        KeyBinding::new("escape", Cancel, context),
    ]);
}

/// The settings last used, so the next print starts from them.
struct LastSettings(PrintSettings);

impl Global for LastSettings {}

fn last_settings(cx: &App) -> PrintSettings {
    cx.try_global::<LastSettings>()
        .map(|last| last.0)
        .unwrap_or_default()
}

/// Where Print… sends the PDF in place of the system, when set: tests
/// set it, since the test platform has no printer or viewer.
pub struct PrintSink(pub Rc<dyn Fn(Arc<Vec<u8>>)>);

impl Global for PrintSink {}

/// What Print… or Save as PDF… waits for while the pages are laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pending {
    Print,
    Save,
}

/// The line beside the buttons: what the last action did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Saved(PathBuf),
    Failed(String),
}

/// The print dialog.
pub struct PrintDialog {
    text: Arc<str>,
    note_path: Option<PathBuf>,
    vault_root: Option<PathBuf>,
    settings: PrintSettings,
    preview: Option<Preview>,
    failure: Option<LayoutFailure>,
    laying_out: bool,
    /// Counts layouts asked for; only the latest one's pages are kept.
    generation: u64,
    layout_task: Option<Task<()>>,
    pending: Option<Pending>,
    save_task: Option<Task<()>>,
    status: Option<Status>,
    /// The setting the keyboard is on.
    focused: usize,
    menu: MenuSlot,
    scroll: ScrollHandle,
    scale_factor: f32,
    focus_handle: FocusHandle,
    theme: UiTheme,
    style: SettingsTheme,
}

impl EventEmitter<DismissEvent> for PrintDialog {}

impl Focusable for PrintDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl HasMenuSlot for PrintDialog {
    fn menu_slot(&mut self) -> &mut MenuSlot {
        &mut self.menu
    }
}

impl PrintDialog {
    /// A dialog for the note's text that starts laying it out at once.
    /// Images are looked for beside the note, then from `vault_root`.
    pub fn new(
        text: String,
        note_path: Option<PathBuf>,
        vault_root: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut dialog = PrintDialog {
            text: text.into(),
            note_path,
            vault_root,
            settings: last_settings(cx),
            preview: None,
            failure: None,
            laying_out: false,
            generation: 0,
            layout_task: None,
            pending: None,
            save_task: None,
            status: None,
            focused: 0,
            menu: MenuSlot::default(),
            scroll: ScrollHandle::new(),
            scale_factor: window.scale_factor(),
            focus_handle: cx.focus_handle(),
            theme: crate::ui::ui_theme(cx),
            style: crate::ui::settings_theme(cx),
        };
        dialog.lay_out(false, window, cx);
        cx.on_release(|dialog: &mut PrintDialog, cx| {
            if let Some(preview) = dialog.preview.take() {
                release_pages(preview, cx);
            }
        })
        .detach();
        dialog
    }

    pub fn settings(&self) -> PrintSettings {
        self.settings
    }

    /// The pages on show, which may be from the settings before the last
    /// change while it's laid out.
    pub fn preview(&self) -> Option<&Preview> {
        self.preview.as_ref()
    }

    pub fn failure(&self) -> Option<&LayoutFailure> {
        self.failure.as_ref()
    }

    /// Whether a layout is on its way.
    pub fn is_laying_out(&self) -> bool {
        self.laying_out
    }

    pub fn status(&self) -> Option<&Status> {
        self.status.as_ref()
    }

    pub fn pending(&self) -> Option<Pending> {
        self.pending
    }

    pub fn focused_control(&self) -> Control {
        Control::ALL[self.focused]
    }

    fn file_name(&self) -> String {
        export_ui::suggested_file_name(self.note_path.as_deref())
    }

    fn job(&self) -> PreviewJob {
        let width = f32::from(self.theme.print.page_width) * self.scale_factor;
        PreviewJob {
            text: self.text.clone(),
            note_path: self.note_path.clone(),
            vault_root: self.vault_root.clone(),
            settings: self.settings,
            page_width_px: width.round().max(1.) as u32,
        }
    }

    /// Lays the note out with the current settings, after the settle
    /// time when `debounce` holds. A newer layout supersedes this one.
    fn lay_out(&mut self, debounce: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.generation += 1;
        self.laying_out = true;
        let generation = self.generation;
        let job = self.job();
        let delay = debounce.then_some(self.theme.print.debounce);
        let executor = cx.background_executor().clone();
        self.layout_task = Some(cx.spawn_in(window, async move |this, cx| {
            if let Some(delay) = delay {
                executor.timer(delay).await;
            }
            let result = executor
                .spawn(async move { preview::build_preview(job) })
                .await;
            this.update_in(cx, |dialog, window, cx| {
                dialog.finish_layout(generation, result, window, cx)
            })
            .ok();
        }));
        cx.notify();
    }

    fn finish_layout(
        &mut self,
        generation: u64,
        result: Result<Preview, LayoutFailure>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if generation != self.generation {
            if let Ok(stale) = result {
                release_pages(stale, cx);
            }
            return;
        }
        self.laying_out = false;
        match result {
            Ok(preview) => {
                self.failure = None;
                if let Some(old) = self.preview.replace(preview) {
                    release_pages(old, cx);
                }
            }
            Err(failure) => {
                eprintln!("print preview failed: {}", failure.messages.join("; "));
                // The error shows in place of the pages, which were for
                // other settings.
                if let Some(old) = self.preview.take() {
                    release_pages(old, cx);
                }
                self.failure = Some(failure);
            }
        }
        cx.notify();
        self.run_pending(window, cx);
    }

    /// Changes the settings and lays the note out again.
    pub fn set_settings(
        &mut self,
        settings: PrintSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if settings == self.settings {
            return;
        }
        self.settings = settings;
        cx.set_global(LastSettings(settings));
        self.status = None;
        self.lay_out(true, window, cx);
    }

    /// Moves `control` on one choice, or flips its switch.
    pub fn change(
        &mut self,
        control: Control,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let settings = self.settings.stepped(control, forward);
        self.set_settings(settings, window, cx);
    }

    fn move_focus(&mut self, forward: bool, cx: &mut Context<Self>) {
        let count = Control::ALL.len();
        self.focused = if forward {
            (self.focused + 1) % count
        } else {
            (self.focused + count - 1) % count
        };
        cx.notify();
    }

    /// Space: flips a switch, or opens a list's menu.
    fn toggle_focused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let control = self.focused_control();
        if control.is_switch() {
            self.change(control, true, window, cx);
        } else {
            self.open_menu(control, window, cx);
        }
    }

    /// The menu of `control`'s choices, under its button.
    pub fn open_menu(&mut self, control: Control, window: &mut Window, cx: &mut Context<Self>) {
        self.focused = Control::ALL
            .iter()
            .position(|item| *item == control)
            .unwrap_or(0);
        let items = self.menu_items(control, cx);
        let anchor = MenuAnchor::Below {
            key: control.key().into(),
            align_right: true,
        };
        let mut menu = std::mem::take(&mut self.menu);
        menu.open(items, anchor, window, cx);
        self.menu = menu;
    }

    fn menu_items(&self, control: Control, cx: &mut Context<Self>) -> Vec<MenuItem> {
        let current = self.settings;
        let choices: Vec<(String, PrintSettings)> = match control {
            Control::Paper => Paper::ALL
                .into_iter()
                .map(|paper| {
                    let label = format!("{} ({})", paper.label(), paper.detail());
                    (label, PrintSettings { paper, ..current })
                })
                .collect(),
            Control::Margins => MarginSize::ALL
                .into_iter()
                .map(|margins| {
                    let label = format!("{} ({})", margins.label(), margins.detail());
                    (label, PrintSettings { margins, ..current })
                })
                .collect(),
            _ => Vec::new(),
        };
        let this = cx.entity().downgrade();
        choices
            .into_iter()
            .map(|(label, settings)| {
                let this = this.clone();
                MenuItem::action(label, move |window, cx| {
                    this.update(cx, |dialog, cx| dialog.set_settings(settings, window, cx))
                        .ok();
                })
                .checked(settings == current)
            })
            .collect()
    }

    /// Print…: hands the PDF to the system and closes. While the pages
    /// are still being laid out it waits for them.
    pub fn print(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.laying_out {
            self.pending = Some(Pending::Print);
            cx.notify();
            return;
        }
        let Some(preview) = &self.preview else {
            return;
        };
        let pdf = preview.pdf.clone();
        if let Some(sink) = cx.try_global::<PrintSink>() {
            (sink.0)(pdf);
        } else {
            let title = self.file_name();
            let path = export_ui::print_path(self.note_path.as_deref());
            native::send_to_printer(pdf, title, path, window, cx).detach_and_log_err(cx);
        }
        cx.emit(DismissEvent);
    }

    /// Save as PDF…: asks where, then writes the PDF there. While the
    /// pages are still being laid out it waits for them.
    pub fn save_as_pdf(&mut self, cx: &mut Context<Self>) {
        if self.laying_out {
            self.pending = Some(Pending::Save);
            cx.notify();
            return;
        }
        let Some(preview) = &self.preview else {
            return;
        };
        let pdf = preview.pdf.clone();
        let folder = export_ui::save_folder(self.note_path.as_deref());
        let saving = export_ui::save_with(
            &folder,
            &self.file_name(),
            move |destination| export_ui::write_bytes(destination, &pdf),
            cx,
        );
        self.save_task = Some(cx.spawn(async move |this, cx| {
            let result = saving.await;
            this.update(cx, |dialog, cx| dialog.finish_save(result, cx))
                .ok();
        }));
    }

    fn finish_save(
        &mut self,
        result: anyhow::Result<Option<(PathBuf, ())>>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(Some((path, ()))) => self.status = Some(Status::Saved(path)),
            Ok(None) => {}
            Err(error) => {
                eprintln!("could not save the PDF: {error:#}");
                self.status = Some(Status::Failed(error.to_string()));
            }
        }
        cx.notify();
    }

    /// Runs the Print or Save that waited for the layout just finished.
    fn run_pending(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        if self.failure.is_some() {
            return;
        }
        match pending {
            Pending::Save => self.save_as_pdf(cx),
            Pending::Print => self.print(window, cx),
        }
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

/// Frees the pages' images from the windows' texture atlases, once the
/// frame that drew them is done.
fn release_pages(preview: Preview, cx: &mut App) {
    cx.defer(move |cx| {
        for page in preview.pages {
            cx.drop_image(page.image, None);
        }
    });
}

/// "Saved Lemma.pdf in Notes."
pub fn saved_message(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    match path.parent().and_then(Path::file_name) {
        Some(folder) => format!("Saved {name} in {}.", folder.to_string_lossy()),
        None => format!("Saved {name}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_files_are_named_with_their_folder() {
        assert_eq!(
            saved_message(Path::new("/home/me/Notes/Lemma.pdf")),
            "Saved Lemma.pdf in Notes."
        );
        assert_eq!(saved_message(Path::new("Lemma.pdf")), "Saved Lemma.pdf.");
    }

    #[gpui::test]
    fn a_layout_error_shows_in_place_of_the_pages(cx: &mut gpui::TestAppContext) {
        cx.update(bind_keys);
        let (dialog, cx) = cx.add_window_view(|window, cx| {
            let dialog = PrintDialog::new("Some text.".to_owned(), None, None, window, cx);
            window.focus(&dialog.focus_handle);
            dialog
        });
        assert!(cx.debug_bounds("print-page-1").is_some());
        let dismissed = std::rc::Rc::new(std::cell::Cell::new(false));
        let seen = dismissed.clone();
        cx.update(|_, cx| {
            cx.subscribe(&dialog, move |_, _: &DismissEvent, _| seen.set(true))
                .detach()
        });
        // A change whose layout fails.
        cx.simulate_keystrokes("right");
        dialog.update_in(cx, |dialog, window, cx| {
            let failure = LayoutFailure {
                messages: vec![preview::sentence("unknown variable: nosuch")],
            };
            let generation = dialog.generation;
            dialog.finish_layout(generation, Err(failure), window, cx);
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("print-failure").is_some());
        dialog.read_with(cx, |dialog, _| {
            assert!(dialog.preview().is_none());
            assert_eq!(
                dialog.failure().map(LayoutFailure::summary).as_deref(),
                Some("Unknown variable: nosuch.")
            );
        });
        // There's nothing to print, so Enter does nothing.
        cx.simulate_keystrokes("enter");
        assert!(!dismissed.get());
    }
}
