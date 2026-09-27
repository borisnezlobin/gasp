//! Every command in mitex's bundled spec should convert and render against
//! the vendored scope plus `assets/compat.typ`.

use editor_math::render_latex;
use mitex_spec::{ArgPattern, ArgShape, CommandSpecItem};

/// Commands that cannot render on their own, and why.
const KNOWN_UNSUPPORTED: [&str; 7] = [
    "cite",           // needs a bibliography
    "ref",            // needs a label in the document
    "eqref",          // needs a label in the document
    "item",           // only valid inside itemize or enumerate
    "dashleftarrow",  // upstream maps it to a modifier Typst no longer has
    "dashrightarrow", // same as above
    "raisebox",       // expects a TeX length as its first argument
];

fn sample_source(name: &str, args: &ArgShape) -> Option<String> {
    match args {
        ArgShape::Right {
            pattern: ArgPattern::None,
        } => Some(format!(r"a \{name} b")),
        ArgShape::Right {
            pattern: ArgPattern::FixedLenTerm { len },
        } => Some(format!(r"\{name}{}", "{1em}".repeat(usize::from(*len)))),
        _ => None,
    }
}

#[test]
fn every_spec_symbol_resolves() {
    let mut failures = Vec::new();
    let mut tested = 0;
    for (name, item) in mitex_spec_gen::DEFAULT_SPEC.items() {
        let CommandSpecItem::Cmd(command) = item else {
            continue;
        };
        if KNOWN_UNSUPPORTED.contains(&name) {
            continue;
        }
        let Some(source) = sample_source(name, &command.args) else {
            continue;
        };
        tested += 1;
        if let Err(error) = render_latex(&source, false, 16.0) {
            failures.push(format!("{source}: {error}"));
        }
    }
    assert!(tested > 800, "only {tested} commands tested");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
