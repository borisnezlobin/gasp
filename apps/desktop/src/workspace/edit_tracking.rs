//! Counting the time spent editing each note, for the status bar, and
//! keeping it in this device's file under `.editor/stats/` (see
//! [`crate::edit_time`]). Reading and writing happen off the main thread.

use std::path::Path;

use gpui::{Context, Entity, Focusable, Window};

use super::Workspace;
use crate::edit_time::{self, EditTime, SAVE_DELAY};
use crate::editor::EditorView;

impl Workspace {
    /// Starts counting, with this device's id (made now the first time
    /// the vault opens here), and reads every device's file.
    pub(super) fn start_edit_time(&mut self, cx: &mut Context<Self>) {
        let mut id = self.config.device.device_id.clone();
        if id.is_empty() {
            id = edit_time::new_device_id(&edit_time::device_name());
        }
        self.edit_time = EditTime::new(&id, "");
        let vault = self.vault.clone();
        let task = cx.spawn(async move |workspace, cx| {
            let (loaded, name) = cx
                .background_executor()
                .spawn(async move { (edit_time::load(&vault, &id), edit_time::device_name()) })
                .await;
            workspace
                .update(cx, |workspace, cx| {
                    workspace.edit_time.set_device_name(&name);
                    workspace.edit_time.merge_loaded(loaded);
                    workspace.refresh_status(cx);
                })
                .ok();
        });
        self.tasks.push(task);
    }

    /// The id that names this device's file, for `device.toml`.
    pub(super) fn edit_time_device_id(&self) -> String {
        self.edit_time.device_id().to_owned()
    }

    /// Counts an edit typed into `editor`: only while it has the
    /// keyboard, so a reload from disk or sync isn't editing.
    pub(super) fn count_edit(
        &mut self,
        editor: &Entity<EditorView>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !editor.focus_handle(cx).is_focused(window) {
            return;
        }
        if self.active_editor(cx).as_ref() != Some(editor) {
            return;
        }
        let Some(path) = self.active_path(cx) else {
            return;
        };
        let note = self.relative_name(&path);
        let now = cx.background_executor().now();
        if self.edit_time.record_edit(&note, now) {
            self.schedule_edit_time_save(cx);
        }
    }

    /// Writes this device's file a moment after typing, once per pause.
    fn schedule_edit_time_save(&mut self, cx: &mut Context<Self>) {
        if self.edit_time_save_pending {
            return;
        }
        self.edit_time_save_pending = true;
        cx.spawn(async move |workspace, cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            let Ok((path, file)) = workspace.update(cx, |workspace, _| {
                workspace.edit_time_save_pending = false;
                let path = workspace.edit_time.own_file(&workspace.vault);
                (path, workspace.edit_time.take_file())
            }) else {
                return;
            };
            let saved = cx
                .background_executor()
                .spawn(async move { edit_time::save(&path, &file) })
                .await;
            if let Err(error) = saved {
                eprintln!("could not save edit times: {error}");
            }
        })
        .detach();
    }

    /// Writes this device's file now, as the window closes.
    pub(super) fn save_edit_time_now(&mut self) {
        if !self.edit_time.is_dirty() {
            return;
        }
        let path = self.edit_time.own_file(&self.vault);
        if let Err(error) = edit_time::save(&path, &self.edit_time.take_file()) {
            eprintln!("could not save edit times: {error}");
        }
    }

    /// Carries the time spent on a note to its new path.
    pub(super) fn edit_time_moved(&mut self, from: &Path, to: &Path, cx: &mut Context<Self>) {
        let (from, to) = (self.relative_name(from), self.relative_name(to));
        self.edit_time.rename(&from, &to);
        if self.edit_time.is_dirty() {
            self.schedule_edit_time_save(cx);
        }
    }

    /// Seconds spent editing the note at `path` shown in `editor`: every
    /// device's count, on top of what Chronotyper left in its frontmatter.
    pub(crate) fn edit_seconds(&self, path: &Path, editor: &EditorView) -> u64 {
        let counted = self.edit_time.seconds(&self.relative_name(path));
        let doc = editor.doc();
        let end = doc.line_start(doc.line_of_offset(doc.len().min(FRONTMATTER_SCAN)));
        let head = doc.rope().byte_slice(0..end);
        let imported = edit_time::frontmatter_seconds(&head.to_string()).unwrap_or(0);
        counted + imported
    }
}

/// How much of a note's start is searched for Chronotyper's key.
const FRONTMATTER_SCAN: usize = 4096;
