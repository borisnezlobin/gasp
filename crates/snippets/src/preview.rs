//! What typing some text gives with a set of snippets, for the snippet
//! editor's test box.

use crate::context::{InputContext, TriggerKey};
use crate::engine::{Request, SnippetEngine};

/// The text a test box shows, with the caret where typing left it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    pub text: String,
    /// Byte offset of the caret in `text`.
    pub caret: usize,
    /// How many times a snippet fired.
    pub expansions: usize,
    /// Whether Tab was pressed at the end to fire a snippet that waits for it.
    pub tab: bool,
}

/// Types `input` one character at a time in `context`, expanding snippets
/// as the editor would. A snippet that waits for Tab gets it at the end.
/// The caret jumps to the first tab stop of each expansion, so what's
/// typed next fills it in.
pub fn preview(
    engine: &SnippetEngine,
    input: &str,
    context: InputContext,
    block_math: bool,
) -> Preview {
    let mut state = Preview {
        text: String::new(),
        caret: 0,
        expansions: 0,
        tab: false,
    };
    for typed in input.chars() {
        state.text.insert(state.caret, typed);
        state.caret += typed.len_utf8();
        state.expand(engine, context, block_math, TriggerKey::Char(typed));
    }
    if !input.is_empty() {
        state.tab = state.expand(engine, context, block_math, TriggerKey::Tab);
    }
    state
}

impl Preview {
    fn expand(
        &mut self,
        engine: &SnippetEngine,
        context: InputContext,
        block_math: bool,
        key: TriggerKey,
    ) -> bool {
        let request = Request {
            before: &self.text[..self.caret],
            selection: "",
            after: &self.text[self.caret..],
            context,
            block_math,
            key,
        };
        let Some(edit) = engine.expand(&request) else {
            return false;
        };
        let start = edit.replace.start;
        self.text.replace_range(edit.replace.clone(), &edit.text);
        self.caret = start + edit.first_selection().end;
        self.expansions += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::SnippetFile;

    fn engine(text: &str) -> SnippetEngine {
        let file = SnippetFile::parse(text).unwrap();
        SnippetEngine::new(file.snippets().cloned().collect()).unwrap()
    }

    #[test]
    fn typing_fills_the_first_stop() {
        let engine = engine("// → \\frac{●}{●}●  math, instant\n");
        let shown = preview(&engine, "a//b", InputContext::Math, false);
        assert_eq!(shown.text, "a\\frac{b}{}");
        assert_eq!(shown.caret, "a\\frac{b".len());
        assert_eq!(shown.expansions, 1);
    }

    #[test]
    fn tab_snippets_fire_at_the_end() {
        let engine = engine("sum → \\sum  math\n");
        let shown = preview(&engine, "sum", InputContext::Math, false);
        assert_eq!(shown.text, "\\sum");
        assert!(shown.tab);
    }

    #[test]
    fn nothing_fires_outside_the_snippets_context() {
        let engine = engine("reals → \\mathbb{R}  math, instant\n");
        let shown = preview(&engine, "reals", InputContext::Text, false);
        assert_eq!(shown.text, "reals");
        assert_eq!(shown.expansions, 0);
    }

    #[test]
    fn switched_off_snippets_never_fire() {
        let engine = engine("mk → $●$  text, instant, off\n");
        assert_eq!(
            preview(&engine, "mk", InputContext::Text, false).expansions,
            0
        );
    }

    #[test]
    fn lazy_engines_compile_on_first_use() {
        let file = SnippetFile::parse("mk → $●$  text, instant\n").unwrap();
        let lazy = SnippetEngine::lazy(file.snippets().cloned().collect());
        assert_eq!(lazy.len(), 1);
        assert_eq!(preview(&lazy, "mk", InputContext::Text, false).text, "$$");
    }
}
