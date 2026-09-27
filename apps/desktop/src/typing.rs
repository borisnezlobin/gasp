//! Typing through the input pipeline, so auto-pairing, smart quotes and
//! list continuation apply to every keystroke, and curling the quotes in
//! pasted text.

use editor_config::settings::EditorSettings;
use editor_core::pipeline::{CURLS_IN, EditRequest, curl_quotes, step_names};
use gpui::Context;

use crate::editor::EditorView;

impl EditorView {
    /// Turns smart quotes and auto-pairing on or off, for typing and for
    /// pasting.
    pub(crate) fn apply_typing_settings(&mut self, settings: &EditorSettings) {
        self.pipeline
            .set_enabled(step_names::SMART_QUOTES, settings.smart_quotes)
            .expect("the built-in pipeline has a smart quotes step");
        self.pipeline
            .set_enabled(step_names::AUTO_PAIR, settings.auto_pair)
            .expect("the built-in pipeline has an auto-pair step");
        self.curl_pasted_quotes = settings.smart_quotes && settings.curl_pasted_quotes;
    }

    /// Text typed at the keyboard, as opposed to an input method's
    /// composition or a paste.
    pub fn type_text(&mut self, text: &str, cx: &mut Context<Self>) {
        // Input-to-paint counts from the keystroke, pipeline included.
        self.timings
            .input_started
            .get_or_insert_with(std::time::Instant::now);
        self.run_pipeline(EditRequest::InsertText(text.to_owned()), cx);
    }

    /// Backspace, which also deletes both halves of an empty pair.
    pub fn delete_backward(&mut self, cx: &mut Context<Self>) {
        self.run_pipeline(EditRequest::DeleteBackward, cx);
    }

    /// `text` as it should be pasted at `at`: with curly quotes when the
    /// setting is on and `at` is somewhere quotes curl.
    pub(crate) fn curl_pasted(&self, text: &str, at: usize) -> String {
        let curls_here = CURLS_IN.contains(&self.source.tree().context_at(at));
        if !self.curl_pasted_quotes || !curls_here {
            return text.to_owned();
        }
        curl_quotes(text, self.doc().char_before(at))
    }
}
