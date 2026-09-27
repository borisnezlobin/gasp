//! The snippets and replacements a vault types with: `snippets.txt` and
//! `replacements.toml` in the config folder, or the built-in ones when a
//! vault has neither.
//!
//! Compiling a few hundred snippets takes milliseconds, so a table compiles
//! on first use rather than on load, and tables read from the same text are
//! shared: reloading the config after an unrelated setting changes doesn't
//! compile them again.

use std::fmt;
use std::sync::{Arc, LazyLock, Mutex};

use editor_snippets::{ParseError, Replacements, SnippetEngine, SnippetFile};

use crate::diagnostics::{Diagnostic, Severity};
use crate::loader::Built;

/// A snippets file and its engine, which compiles when first used.
pub struct SnippetTable {
    pub file: SnippetFile,
    /// Whether the file came from the vault rather than the built-in list.
    pub from_vault: bool,
    pub engine: Arc<SnippetEngine>,
}

impl SnippetTable {
    fn new(file: SnippetFile, from_vault: bool) -> SnippetTable {
        let engine = Arc::new(SnippetEngine::lazy(file.snippets().cloned().collect()));
        SnippetTable {
            file,
            from_vault,
            engine,
        }
    }
}

impl fmt::Debug for SnippetTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SnippetTable")
            .field("snippets", &self.engine.len())
            .field("from_vault", &self.from_vault)
            .finish()
    }
}

impl PartialEq for SnippetTable {
    fn eq(&self, other: &SnippetTable) -> bool {
        self.from_vault == other.from_vault && self.file == other.file
    }
}

/// The replacements table and where it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct ReplacementTable {
    pub table: Arc<Replacements>,
    /// Whether the table came from the vault rather than the built-in one.
    pub from_vault: bool,
}

/// Everything typing expands with.
#[derive(Clone, Debug, PartialEq)]
pub struct TypingTables {
    pub snippets: Arc<SnippetTable>,
    pub replacements: Arc<ReplacementTable>,
}

impl Default for TypingTables {
    fn default() -> Self {
        TypingTables {
            snippets: BUILTIN_SNIPPETS.clone(),
            replacements: BUILTIN_REPLACEMENTS.clone(),
        }
    }
}

static BUILTIN_SNIPPETS: LazyLock<Arc<SnippetTable>> =
    LazyLock::new(|| Arc::new(SnippetTable::new(SnippetFile::builtin(), false)));

static BUILTIN_REPLACEMENTS: LazyLock<Arc<ReplacementTable>> = LazyLock::new(|| {
    Arc::new(ReplacementTable {
        table: Arc::new(Replacements::builtin()),
        from_vault: false,
    })
});

/// Snippet tables read from recent file texts, oldest first.
static RECENT: Mutex<Vec<(String, Arc<SnippetTable>)>> = Mutex::new(Vec::new());

/// How many snippet files [`RECENT`] remembers: one per open vault is
/// plenty.
const RECENT_LIMIT: usize = 4;

/// The snippets in `user`, the text of the vault's `snippets.txt`, or the
/// built-in ones when there is none.
pub fn build_snippets(file: &str, user: Option<&str>) -> Built<Arc<SnippetTable>> {
    let Some(text) = user else {
        return Ok((BUILTIN_SNIPPETS.clone(), Vec::new()));
    };
    let mut recent = RECENT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((_, table)) = recent.iter().find(|(seen, _)| seen == text) {
        return Ok((table.clone(), Vec::new()));
    }
    let parsed = SnippetFile::parse(text).map_err(|errors| {
        errors
            .iter()
            .map(|error| snippet_diagnostic(file, error))
            .collect::<Vec<_>>()
    })?;
    let table = Arc::new(SnippetTable::new(parsed, true));
    recent.push((text.to_string(), table.clone()));
    if recent.len() > RECENT_LIMIT {
        recent.remove(0);
    }
    Ok((table, Vec::new()))
}

/// The replacements in `user`, the text of the vault's
/// `replacements.toml`, or the built-in ones when there is none.
pub fn build_replacements(file: &str, user: Option<&str>) -> Built<Arc<ReplacementTable>> {
    let Some(text) = user else {
        return Ok((BUILTIN_REPLACEMENTS.clone(), Vec::new()));
    };
    Replacements::from_toml(text)
        .map(|table| {
            let table = ReplacementTable {
                table: Arc::new(table),
                from_vault: true,
            };
            (Arc::new(table), Vec::new())
        })
        .map_err(|error| vec![Diagnostic::error(file, text, None, error.message)])
}

fn snippet_diagnostic(file: &str, error: &ParseError) -> Diagnostic {
    Diagnostic {
        file: file.to_string(),
        line: error.line,
        column: error.column,
        message: error.message.clone(),
        severity: Severity::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_files_the_built_in_tables_are_used() {
        let (snippets, _) = build_snippets("snippets.txt", None).unwrap();
        assert!(!snippets.from_vault);
        assert!(snippets.engine.len() > 200);
        let (replacements, _) = build_replacements("replacements.toml", None).unwrap();
        assert!(!replacements.from_vault);
    }

    #[test]
    fn the_same_text_shares_one_compiled_table() {
        let text = "mk → $●$  text, instant\n";
        let (first, _) = build_snippets("snippets.txt", Some(text)).unwrap();
        let (second, _) = build_snippets("snippets.txt", Some(text)).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert!(first.from_vault);
    }

    #[test]
    fn a_bad_line_is_reported_where_it_is() {
        let text = "mk → $●$  text, instant\nab → c  sometimes\n";
        let errors = build_snippets("snippets.txt", Some(text)).unwrap_err();
        assert_eq!(errors[0].line, 2);
        assert!(errors[0].message.contains("sometimes"));
    }
}
