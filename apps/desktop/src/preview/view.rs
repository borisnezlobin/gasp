//! The live-preview commands and actions of the editor view: zoom,
//! readable width, symbol reveal modes, following links, toggling tasks,
//! folding callouts, and running math renders in the background.

use std::ops::Range;
use std::path::{Path, PathBuf};

use gasp_config::settings::SymbolMode;
use gasp_core::transaction::{ChangeSet, Origin, Transaction};
use gpui::{AppContext, Context};

use crate::editor::{EditorEvent, EditorView};
use crate::pending_renders::PendingRender;
use crate::preview::code_highlight::load_syntaxes;
use crate::preview::links::link_target_at;
use crate::preview::reveal::reveal_settings;

/// Each zoom step scales text by this much.
pub const ZOOM_STEP: f32 = 1.1;
pub const MIN_ZOOM: f32 = 0.5;
pub const MAX_ZOOM: f32 = 3.;

/// How long a note's tab stays hidden before it lets go of its pictures.
pub const RELEASE_HIDDEN_AFTER: std::time::Duration = std::time::Duration::from_secs(60);

impl EditorView {
    /// Scales every size in the editor and re-measures the lines.
    pub fn set_zoom(&mut self, zoom: f32, cx: &mut Context<Self>) {
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        self.theme = self.base_theme.scaled(self.zoom);
        self.remeasure();
        cx.notify();
    }

    pub fn zoom_in(&mut self, cx: &mut Context<Self>) {
        self.set_zoom(self.zoom * ZOOM_STEP, cx);
    }

    pub fn zoom_out(&mut self, cx: &mut Context<Self>) {
        self.set_zoom(self.zoom / ZOOM_STEP, cx);
    }

    pub fn reset_zoom(&mut self, cx: &mut Context<Self>) {
        self.set_zoom(1., cx);
    }

    /// Switches between the readable column and the full width.
    pub fn toggle_readable_width(&mut self, cx: &mut Context<Self>) {
        self.readable_width = !self.readable_width;
        self.remeasure();
        cx.notify();
    }

    pub fn symbol_mode(&self) -> SymbolMode {
        self.symbols.mode
    }

    /// Always shown, then revealed around the cursor, then always hidden.
    /// Per-syntax overrides and the reveal scope stay as they are.
    pub fn cycle_symbols(&mut self, cx: &mut Context<Self>) {
        self.set_symbol_mode(self.symbols.mode.next(), cx);
    }

    pub fn set_symbol_mode(&mut self, mode: SymbolMode, cx: &mut Context<Self>) {
        self.symbols.mode = mode;
        self.reveal = reveal_settings(&self.symbols);
        self.remeasure();
        cx.notify();
    }

    /// The target of the link at `offset`, if there is one.
    pub fn link_at(&self, offset: usize) -> Option<String> {
        link_target_at(self.source.tree(), offset)
    }

    /// Asks the container to open the link under the cursor.
    pub fn follow_link(&mut self, cx: &mut Context<Self>) {
        if let Some(target) = self.link_at(self.cursor()) {
            cx.emit(EditorEvent::OpenLink(target));
        }
    }

    /// Checks or unchecks the task whose `[ ]` marker is in `marker`.
    pub fn toggle_task(&mut self, marker: Range<usize>, cx: &mut Context<Self>) {
        let text = self.source.text();
        let Some(open) = text
            .get(marker.clone())
            .and_then(|marker_text| marker_text.find('['))
        else {
            return;
        };
        let at = marker.start + open + 1;
        let Some(current) = text[at..].chars().next() else {
            return;
        };
        let replacement = if current == ' ' { "x" } else { " " };
        let changes = ChangeSet::replace(at..at + current.len_utf8(), replacement);
        let transaction = Transaction::new(changes, Origin::command("task.toggle"), self.now_ms());
        self.apply_transaction(transaction, cx);
    }

    /// Folds or unfolds a callout by its header, without editing the note.
    pub fn toggle_fold(&mut self, header: usize, folded_now: bool, cx: &mut Context<Self>) {
        self.folds.toggle(header, folded_now);
        cx.notify();
    }

    /// Loads the code grammars in the background when a code block
    /// needed them, then redraws with colours.
    pub(crate) fn start_code_loads(&mut self, cx: &mut Context<Self>) {
        self.start_code_jobs(cx);
        if !self.code.take_load_request() {
            return;
        }
        let pending = PendingRender::start();
        let load = cx.background_spawn(async {
            load_syntaxes();
        });
        cx.spawn(async move |this, cx| {
            load.await;
            this.update(cx, |_, cx| cx.notify()).ok();
            drop(pending);
        })
        .detach();
    }

    /// Highlights the blocks layout left for the background, redrawing as
    /// each finishes.
    fn start_code_jobs(&mut self, cx: &mut Context<Self>) {
        for job in self.code.take_jobs() {
            let run = cx.background_spawn(async move { job.run() });
            let pending = PendingRender::start();
            cx.spawn(async move |this, cx| {
                let done = run.await;
                this.update(cx, |view, cx| {
                    view.code.finish_job(done);
                    cx.notify();
                })
                .ok();
                drop(pending);
            })
            .detach();
        }
    }

    /// Downloads the web images the last layout asked for, such as link
    /// card previews, redrawing as each arrives.
    pub(crate) fn start_remote_images(&mut self, cx: &mut Context<Self>) {
        for request in self.images.take_remote_requests() {
            let download = cx.background_spawn(async move {
                let image = crate::link_cards::images::load(&request);
                (request.url, image)
            });
            let pending = PendingRender::start();
            cx.spawn(async move |this, cx| {
                let (url, image) = download.await;
                this.update(cx, |view, cx| {
                    view.images.finish_remote(url, image);
                    cx.notify();
                })
                .ok();
                drop(pending);
            })
            .detach();
        }
    }

    /// Follows whether the view's tab is the one its pane shows. A note
    /// hidden for [`RELEASE_HIDDEN_AFTER`] lets go of its decoded images
    /// and equations, which it draws again once it's shown.
    pub fn set_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        if !hidden {
            self.release_when_hidden = None;
            return;
        }
        if self.release_when_hidden.is_some() {
            return;
        }
        self.release_when_hidden = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(RELEASE_HIDDEN_AFTER).await;
            this.update(cx, |view, _| view.release_pictures()).ok();
        }));
    }

    /// Lets go of the decoded images and rendered equations, here and in
    /// the last frame's lines.
    pub fn release_pictures(&mut self) {
        self.images.release_pixels();
        self.math.release_renders();
        let stand_in = self.images.blank();
        if let Some(frame) = self.frame.as_mut() {
            frame.forget_pictures(&stand_in);
        }
    }

    /// Starts everything the last layout left for the background:
    /// equations, code colours, images, and looking for missing images.
    pub(crate) fn start_loads(&mut self, cx: &mut Context<Self>) {
        self.start_math_renders(cx);
        self.start_code_loads(cx);
        self.start_remote_images(cx);
        self.start_image_decodes(cx);
        self.find_vault_images(cx);
    }

    /// Decodes the note's images the last layout drew, each on a
    /// background thread at the size it's drawn, redrawing as each is done.
    pub(crate) fn start_image_decodes(&mut self, cx: &mut Context<Self>) {
        for decode in self.images.take_decodes() {
            let pending = PendingRender::start();
            cx.spawn(async move |this, cx| {
                let decoding = decode.clone();
                let image = cx
                    .background_executor()
                    .spawn(async move { crate::images::decode_file(&decoding) })
                    .await;
                this.update(cx, |view, cx| {
                    view.images.finish_decode(&decode, image);
                    cx.notify();
                })
                .ok();
                drop(pending);
            })
            .detach();
        }
    }

    /// Looks up the images the last layout couldn't find near the note in
    /// the vault index, which finds a bare `![[name.png]]` anywhere in the
    /// vault, and redraws with the ones it found. Before the index is
    /// ready they stay missing; its change retries them.
    pub(crate) fn find_vault_images(&mut self, cx: &mut Context<Self>) {
        let lookups = self.images.take_vault_lookups();
        let Some(index) = self.suggest.index.clone() else {
            return;
        };
        let note_dir = self.images.note_dir().map(Path::to_path_buf);
        let paths: Vec<(String, PathBuf)> = {
            let index = index.read(cx);
            let note_dir = note_dir.as_deref().unwrap_or(index.root());
            lookups
                .into_iter()
                .filter_map(|target| {
                    let path = index.find_file(note_dir, &target)?;
                    Some((target, path))
                })
                .collect()
        };
        let mut found = false;
        for (target, path) in paths {
            found |= self.images.found_in_vault(&target, &path);
        }
        if found {
            cx.notify();
        }
    }

    /// Looks again for the images that were missing, after the vault
    /// changed.
    pub(crate) fn retry_missing_images(&mut self, cx: &mut Context<Self>) {
        if self.images.retry_missing() {
            cx.notify();
        }
    }

    /// Whether every equation asked for has been rendered.
    pub fn is_math_idle(&self) -> bool {
        self.math.is_idle()
    }

    /// Starts the math renders the last layout asked for, each on a
    /// background thread, redrawing when one finishes.
    pub(crate) fn start_math_renders(&mut self, cx: &mut Context<Self>) {
        for request in self.math.take_requests() {
            let pending = PendingRender::start();
            cx.spawn(async move |this, cx| {
                let key = request.key.clone();
                let render = cx.background_executor().spawn(async move { request.run() });
                let state = render.await;
                this.update(cx, |view, cx| {
                    view.math.finish(key, state);
                    cx.notify();
                })
                .ok();
                drop(pending);
            })
            .detach();
        }
    }
}
