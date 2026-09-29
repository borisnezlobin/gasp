//! The built-in synthetic equation list converts and renders completely.

use gasp_math::{MathCache, find_math, latex_to_typst};

const EQUATIONS: &str = include_str!("data/equations.md");

#[test]
fn list_is_large_enough() {
    assert!(find_math(EQUATIONS).len() >= 300);
}

#[test]
fn every_equation_converts() {
    let failures: Vec<String> = find_math(EQUATIONS)
        .into_iter()
        .filter_map(|snippet| {
            latex_to_typst(&snippet.source, snippet.display)
                .err()
                .map(|error| format!("{}: {error}", snippet.source))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_equation_renders_with_sane_dimensions() {
    let mut cache = MathCache::new();
    let mut failures = Vec::new();
    for snippet in find_math(EQUATIONS) {
        match cache.render(&snippet.source, snippet.display, 16.0) {
            Ok(rendered) => {
                let sane = rendered.width > 0.0
                    && rendered.height > 0.0
                    && rendered.width < 2000.0
                    && rendered.height < 400.0
                    && (0.0..=rendered.height).contains(&rendered.baseline)
                    && rendered.svg.len() > 100;
                if !sane {
                    failures.push(format!("{}: odd size {rendered:?}", snippet.source));
                }
            }
            Err(error) => failures.push(format!("{}: {error}", snippet.source)),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
