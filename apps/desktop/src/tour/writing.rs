//! The writing step: a real note to type in, with a heading, bold and
//! italics, tasks, a link and some math. The caret starts inside the bold
//! word, so its stars show from the start; they fade as it leaves, which
//! is the one idea the step has to get across.

use gpui::{AnyElement, App, AppContext, Context, Entity, div, prelude::*, px};

use super::{Step, Tour, explanation, heading, sea};
use crate::editor::EditorView;
use crate::ui::ui_theme;

pub const PRACTICE_NOTE: &str = "\
# Things to try

Two stars make **bold** and one makes *italics*. Click another line and the stars fade away.

- [ ] Tick this box
- [ ] Link a note with [[double brackets]]

Math goes between dollar signs, like $e^{i\\pi} + 1 = 0$.
";

/// Where the caret starts: inside the bold word.
const CARET_AFTER: &str = "**bo";

/// The widest the practice note gets.
const NOTE_WIDTH: f32 = 760.;

pub fn new_playground(cx: &mut Context<Tour>) -> Entity<EditorView> {
    let playground = cx.new(|cx| EditorView::new(PRACTICE_NOTE, Vec::new(), cx));
    let caret = PRACTICE_NOTE
        .find(CARET_AFTER)
        .map_or(0, |at| at + CARET_AFTER.len());
    playground.update(cx, |editor, cx| editor.select(caret, caret, cx));
    playground
}

pub fn render(playground: Entity<EditorView>, cx: &mut App) -> AnyElement {
    let ui = ui_theme(cx);
    let bottom = sea::band_height(&ui) + ui.button_height + ui.space_xl * 2.;
    let note = div()
        .id("tour-practice-note")
        .flex_1()
        .min_h_0()
        .w_full()
        .overflow_hidden()
        .rounded(ui.surface_radius)
        .bg(ui.note_background)
        .shadow(ui.surface_shadows())
        .child(playground);
    div()
        .absolute()
        .top(ui.tab_height + ui.space_xl * 2.)
        .bottom(bottom)
        .left_0()
        .right_0()
        .flex()
        .justify_center()
        .px(sea::margin(&ui))
        .child(
            div()
                .flex()
                .flex_col()
                .w_full()
                .max_w(px(NOTE_WIDTH))
                .gap(ui.space_md)
                .child(heading(Step::Writing, &ui))
                .child(explanation(
                    "Gasp formats Markdown as you type. The symbols show while your cursor is next to them.",
                    &ui,
                ))
                .child(div().h(ui.space_lg))
                .child(note),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_caret_starts_inside_the_bold_word() {
        let at = PRACTICE_NOTE.find(CARET_AFTER).unwrap() + CARET_AFTER.len();
        assert!(PRACTICE_NOTE[at..].starts_with("ld**"));
    }
}
