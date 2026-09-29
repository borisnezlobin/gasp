//! A kept plan that follows the cursor through every corpus note gives
//! the same lines as planning the whole note afresh at each stop.

use gasp_bench::corpus::corpus_notes;
use gasp_config::settings::SymbolSettings;
use gasp_core::render::{
    KeptPlan, RenderInput, RevealMode, RevealScope, RevealSettings, plan, reveal_settings,
};
use gasp_core::syntax::parse;

const CURSOR_STEP: usize = 211;

#[test]
fn a_kept_plan_follows_the_cursor_through_the_corpus() {
    let scopes = [RevealScope::Element, RevealScope::Line, RevealScope::Block];
    let mut every_settings: Vec<RevealSettings> = scopes
        .iter()
        .map(|&scope| RevealSettings::new(RevealMode::AroundCursor { scope }))
        .collect();
    every_settings.push(reveal_settings(&SymbolSettings::default()));
    for (path, text) in corpus_notes() {
        let tree = parse(&text);
        for settings in &every_settings {
            let mut kept = KeptPlan::default();
            let stops = (0..=text.len())
                .step_by(CURSOR_STEP)
                .filter(|&at| text.is_char_boundary(at));
            for at in stops {
                let width = (at / CURSOR_STEP % 2) * 40;
                let selection = at..(at + width).min(text.len());
                let selection = match text.is_char_boundary(selection.end) {
                    true => selection,
                    false => at..at,
                };
                let selections = [selection];
                let input = RenderInput {
                    text: &text,
                    tree: &tree,
                    selections: &selections,
                    settings,
                };
                let fresh = plan(&input).lines;
                assert!(
                    kept.plan(&input) == &fresh[..],
                    "{path} at {at}, {settings:?}"
                );
            }
        }
    }
}
