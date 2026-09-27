//! Typing through the input pipeline, so snippets, replacements, the math
//! helpers, auto-pairing, smart quotes and list continuation apply to
//! every keystroke; the tab stops a snippet leaves; and curling the quotes
//! in pasted text.

use std::ops::Range;
use std::sync::Arc;

use editor_config::Config;
use editor_config::typing::TypingTables;
use editor_core::commands::{indent, outdent};
use editor_core::pipeline::{
    CURLS_IN, ContextFilter, EditRequest, MathOptions, MathStep, PipelineOutput, TabStops,
    curl_quotes, follow_stops, step_names,
};
use editor_core::steps::{ReplacementStep, SnippetStep, install_snippets};
use editor_core::transaction::{Origin, Transaction};
use gpui::Context;

use crate::editor::EditorView;

impl EditorView {
    /// Puts the vault's snippets and replacements in the pipeline and turns
    /// each typing helper on or off, for typing and for pasting.
    pub(crate) fn apply_typing_settings(&mut self, config: &Config) {
        let editor = &config.settings.editor;
        let math = &config.settings.math;
        self.install_tables(&config.typing, math.enlarge_brackets);
        let switches = [
            (step_names::SMART_QUOTES, editor.smart_quotes),
            (step_names::AUTO_PAIR, editor.auto_pair),
            (step_names::SNIPPETS, editor.snippets),
            (step_names::REPLACEMENTS, editor.replacements),
        ];
        for (step, on) in switches {
            self.pipeline
                .set_enabled(step, on)
                .expect("the built-in pipeline has every typing step");
        }
        let options = MathOptions {
            auto_fraction: math.auto_fraction,
            matrix_shortcuts: math.matrix_shortcuts,
            tab_out: math.tab_out,
        };
        self.pipeline
            .replace(step_names::MATH, Box::new(MathStep::new(options)))
            .expect("the built-in pipeline has a math step");
        self.reveal.bracket_colours = math.bracket_colours;
        self.curl_pasted_quotes = editor.smart_quotes && editor.curl_pasted_quotes;
        self.set_footnote_renumbering(editor.renumber_footnotes);
    }

    /// Fills the snippet and replacement slots. The tables are shared by
    /// every editor on the vault and compiled once, so this is cheap.
    fn install_tables(&mut self, tables: &TypingTables, enlarge_brackets: bool) {
        let unchanged = self.typing.as_ref().is_some_and(|(installed, enlarge)| {
            Arc::ptr_eq(&installed.snippets, &tables.snippets)
                && Arc::ptr_eq(&installed.replacements, &tables.replacements)
                && *enlarge == enlarge_brackets
        });
        if unchanged {
            return;
        }
        let snippets = SnippetStep::new(tables.snippets.engine.clone())
            .with_enlarged_brackets(enlarge_brackets);
        install_snippets(&mut self.pipeline, snippets)
            .expect("the built-in pipeline has a snippets slot");
        let replacements = ReplacementStep::new(tables.replacements.table.clone());
        self.pipeline
            .replace(step_names::REPLACEMENTS, Box::new(replacements))
            .and_then(|()| {
                // Each entry says where it fires, so the slot is open.
                self.pipeline
                    .set_contexts(step_names::REPLACEMENTS, ContextFilter::any())
            })
            .expect("the built-in pipeline has a replacements slot");
        self.typing = Some((tables.clone(), enlarge_brackets));
        self.tab_stops = None;
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

    /// Tab expands a snippet, moves to the next tab stop, adds a column in
    /// a matrix or leaves a bracket in math; anywhere else it indents.
    pub fn tab(&mut self, cx: &mut Context<Self>) {
        let output = self.pipeline_output(EditRequest::Tab);
        if output.step.as_deref() == Some(step_names::APPLY) {
            self.run_edit(indent, cx);
            return;
        }
        self.apply_output(output, cx);
    }

    /// Shift+Tab goes back to the previous tab stop while a snippet is
    /// being filled in, and outdents otherwise.
    pub fn back_tab(&mut self, cx: &mut Context<Self>) {
        let previous = self.tab_stops.as_mut().and_then(TabStops::back);
        match previous {
            Some(selection) => {
                let select = Transaction::select(selection, Origin::Input, self.now_ms());
                self.apply_transaction(select, cx);
            }
            None => self.run_edit(outdent, cx),
        }
    }

    /// Runs a request through the input pipeline and applies what it
    /// makes of it.
    pub(crate) fn run_pipeline(&mut self, request: EditRequest, cx: &mut Context<Self>) {
        let output = self.pipeline_output(request);
        self.apply_output(output, cx);
    }

    fn pipeline_output(&mut self, request: EditRequest) -> PipelineOutput {
        let started = std::time::Instant::now();
        let phase = crate::keytrace::span("pipeline");
        let output = self.pipeline.run_input(
            request,
            self.state.doc(),
            self.state.selection(),
            self.source.tree(),
            self.now_ms(),
            self.tab_stops.as_ref(),
        );
        drop(phase);
        self.timings.pipeline.push(started.elapsed());
        output
    }

    fn apply_output(&mut self, output: PipelineOutput, cx: &mut Context<Self>) {
        let stops = self.tab_stops.take();
        self.apply_transactions(output.transactions.clone(), cx);
        self.tab_stops = follow_stops(stops, &output);
        cx.notify();
    }

    /// Ends the snippet being filled in once the cursor leaves its current
    /// stop, so its marks don't linger.
    pub(crate) fn drop_stale_tab_stops(&mut self) {
        let head = self.cursor();
        if self
            .tab_stops
            .as_ref()
            .is_some_and(|stops| !stops.holds(head))
        {
            self.tab_stops = None;
        }
    }

    /// The tab stops still to be visited, which the editor marks.
    pub fn pending_tab_stops(&self) -> Vec<Range<usize>> {
        self.tab_stops
            .as_ref()
            .map(|stops| stops.pending().cloned().collect())
            .unwrap_or_default()
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
