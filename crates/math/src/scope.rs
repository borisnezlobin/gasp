//! The part of the mitex scope one equation needs.
//!
//! Typst's `eval` memoises on its arguments, so passing the whole scope,
//! hundreds of bindings, means turning it into a Typst scope and hashing
//! it for every equation. An equation can only reach the names it spells
//! out, so a scope of just those names evaluates it the same way.

use std::collections::HashSet;
use std::sync::LazyLock;

use typst::foundations::{Array, Str};
use typst::introspection::{Introspector, MetadataElem};

use crate::world::{SCOPE_MODULE_PATH, compile_document};

/// Every name the mitex scope binds, read from the compiled scope once.
/// Empty if that fails, and then equations get the whole scope.
static SCOPE_NAMES: LazyLock<HashSet<String>> = LazyLock::new(read_scope_names);

fn read_scope_names() -> HashSet<String> {
    let main = format!(
        "#import \"{SCOPE_MODULE_PATH}\": mitex-scope\n\
         #metadata(mitex-scope.keys()) <mitex-names>\n"
    );
    let Ok(document) = compile_document(main) else {
        return HashSet::new();
    };
    document
        .introspector()
        .query_labelled()
        .iter()
        .filter_map(|content| content.to_packed::<MetadataElem>())
        .filter_map(|metadata| metadata.value.clone().cast::<Array>().ok())
        .flatten()
        .filter_map(|name| name.cast::<Str>().ok())
        .map(String::from)
        .collect()
}

/// Every run of word characters in `equation`, and the pieces of each
/// between dashes and underscores: more than the identifiers Typst will
/// find in it, never fewer.
fn candidate_names(equation: &str) -> impl Iterator<Item = &str> {
    equation
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
        .filter(|run| !run.is_empty())
        .flat_map(|run| std::iter::once(run).chain(run.split(['_', '-'])))
}

/// A Typst expression for the scope `equation` needs: a dictionary of the
/// mitex bindings it names, or the whole scope when the names are unknown.
pub(crate) fn scope_for(equation: &str) -> String {
    if SCOPE_NAMES.is_empty() {
        return "mitex-scope".to_owned();
    }
    let mut names: Vec<&str> = candidate_names(equation)
        .filter(|name| SCOPE_NAMES.contains(*name))
        .collect();
    names.sort_unstable();
    names.dedup();
    if names.is_empty() {
        return "(:)".to_owned();
    }
    let mut scope = String::from("(");
    for name in names {
        scope.push_str(&format!("\"{name}\": mitex-scope.at(\"{name}\"), "));
    }
    scope.push(')');
    scope
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scope_names_are_read() {
        assert!(SCOPE_NAMES.len() > 150, "{}", SCOPE_NAMES.len());
        for name in ["frac", "mitexsqrt", "dot", "set"] {
            assert!(SCOPE_NAMES.contains(name), "{name}");
        }
    }

    #[test]
    fn candidates_cover_every_identifier() {
        let names: Vec<&str> = candidate_names("frac(a_1, #mitex-color(x2)) + sym.arrow").collect();
        for name in [
            "frac",
            "a",
            "mitex-color",
            "mitex",
            "color",
            "x2",
            "sym",
            "arrow",
        ] {
            assert!(names.contains(&name), "{name} in {names:?}");
        }
    }

    #[test]
    fn only_named_bindings_are_passed() {
        let scope = scope_for("$frac(a, b) + x$");
        assert!(
            scope.contains("\"frac\": mitex-scope.at(\"frac\")"),
            "{scope}"
        );
        assert!(!scope.contains("mitexsqrt"), "{scope}");
        assert_eq!(scope_for("$1 + 2$"), "(:)");
    }
}
