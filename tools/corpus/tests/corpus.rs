use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use editor_corpus::manifest::{manifest_json, notes_with, scan_vault, totals};
use editor_corpus::scan::NoteScan;
use editor_corpus::targets::scaled_targets;
use editor_corpus::{DEFAULT_SEED, FootnoteProblem, GITATTRIBUTES, Options, Vault, generate};

static VAULT: LazyLock<Vault> = LazyLock::new(|| generate(DEFAULT_SEED, &Options::default()));
static SCANS: LazyLock<Vec<(String, NoteScan)>> = LazyLock::new(|| scan_vault(&VAULT));
static TOTALS: LazyLock<BTreeMap<String, usize>> = LazyLock::new(|| totals(&SCANS));
static NOTES_WITH: LazyLock<BTreeMap<String, usize>> = LazyLock::new(|| notes_with(&SCANS));

fn total(key: &str) -> usize {
    TOTALS.get(key).copied().unwrap_or(0)
}

fn files_under(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let bytes = fs::read(&path).unwrap();
                files.insert(path.strip_prefix(dir).unwrap().to_path_buf(), bytes);
            }
        }
    }
    files
}

#[test]
fn generation_is_deterministic() {
    let again = generate(DEFAULT_SEED, &Options::default());
    assert_eq!(*VAULT, again);
    let other = generate(DEFAULT_SEED + 1, &Options::default());
    assert_ne!(VAULT.notes, other.notes);
}

#[test]
fn writing_twice_gives_identical_bytes() {
    // A fresh folder each run: CI restores target/ from a cache, so a shared
    // folder there can hold a half-written corpus from an earlier run.
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path();
    let options = Options { notes: 40 };
    let first = base.join("a");
    let second = base.join("b");
    generate(7, &options).write_to(&first).unwrap();
    generate(7, &options).write_to(&second).unwrap();
    generate(7, &options).write_to(&second).unwrap();
    assert_eq!(files_under(&first), files_under(&second));
}

#[test]
fn refuses_to_replace_a_folder_without_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("corpus-refuse");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("keep.txt"), "mine").unwrap();
    assert!(generate(1, &Options { notes: 3 }).write_to(&dir).is_err());
    assert!(dir.join("keep.txt").exists());
}

#[test]
fn every_feature_is_present() {
    let keys = [
        "inline_math",
        "block_math_delimiters",
        "html_br",
        "html_hr",
        "html_other",
        "embeds",
        "embeds_with_size",
        "task_items",
        "checked_task_items",
        "nested_task_items",
        "table_rows",
        "table_alignment_rows",
        "escaped_pipes",
        "markdown_links",
        "code_fence_lines",
        "code_fence_titles",
        "tilde_fences",
        "callouts",
        "foldable_callouts",
        "callout_titles",
        "nested_callouts",
        "frontmatter",
        "frontmatter_chronotyper_only",
        "frontmatter_with_other_keys",
        "footnote_refs",
        "footnote_defs",
        "footnote_typos",
        "comments",
        "inline_comments",
        "comment_blocks",
        "tags",
        "highlights",
        "wikilinks",
        "heading_h1",
        "heading_h2",
        "heading_h3",
        "heading_h4",
        "heading_h5",
        "heading_h6",
        "list_items",
        "nested_list_items",
        "ordered_list_items",
        "blockquote_lines",
        "bold",
        "italic",
        "strikethrough",
        "inline_code",
        "em_dashes",
        "ellipses",
        "abbreviations",
        "initials",
        "decimals",
        "times",
        "urls",
        "escaped_dollars",
    ];
    for key in keys {
        assert!(total(key) > 0, "{key} missing from the corpus");
    }
}

#[test]
fn all_callout_types_code_languages_and_html_tags_appear() {
    let types: BTreeSet<&str> = SCANS
        .iter()
        .flat_map(|(_, s)| &s.callout_types)
        .map(String::as_str)
        .collect();
    for kind in [
        "note",
        "abstract",
        "summary",
        "tldr",
        "info",
        "todo",
        "tip",
        "hint",
        "important",
        "success",
        "check",
        "done",
        "question",
        "help",
        "faq",
        "warning",
        "caution",
        "attention",
        "failure",
        "fail",
        "missing",
        "danger",
        "error",
        "bug",
        "example",
        "quote",
        "cite",
    ] {
        assert!(types.contains(kind), "callout type {kind} missing");
    }
    let languages: BTreeSet<&str> = SCANS
        .iter()
        .flat_map(|(_, s)| &s.code_languages)
        .map(String::as_str)
        .collect();
    assert!(languages.len() >= 6, "{languages:?}");
    let tags: BTreeSet<&str> = SCANS
        .iter()
        .flat_map(|(_, s)| &s.html_tags)
        .map(String::as_str)
        .collect();
    for tag in ["br", "hr", "div", "img", "u"] {
        assert!(tags.contains(tag), "html tag {tag} missing");
    }
}

#[test]
fn proportions_match_the_vault_table() {
    for target in scaled_targets(VAULT.notes.len()) {
        let uses = total(target.key) as f64;
        let notes = NOTES_WITH.get(target.key).copied().unwrap_or(0) as f64;
        let (want_uses, want_notes) = (target.uses as f64, target.notes as f64);
        assert!(
            (0.85 * want_uses..=1.2 * want_uses).contains(&uses),
            "{}: {uses} uses, table says {want_uses}",
            target.key
        );
        assert!(
            (0.85 * want_notes..=1.2 * want_notes).contains(&notes),
            "{}: {notes} notes, table says {want_notes}",
            target.key
        );
    }
    assert_eq!(total("html_br"), 326);
    assert_eq!(total("html_hr"), 312);
}

#[test]
fn math_notes_are_dense() {
    assert!(
        total("equations") >= 4000,
        "only {} equations",
        total("equations")
    );
    let heaviest = SCANS
        .iter()
        .map(|(_, s)| s.count("inline_math"))
        .max()
        .unwrap();
    assert!(
        heaviest >= 150,
        "densest note has only {heaviest} inline equations"
    );
}

#[test]
fn smaller_vaults_scale_down() {
    let vault = generate(3, &Options { notes: 51 });
    assert_eq!(vault.notes.len(), 51);
    let scans = scan_vault(&vault);
    let sums = totals(&scans);
    assert_eq!(sums["inline_math"], 915);
    assert!(sums["callouts"] > 0 && sums["footnote_refs"] > 0);
    assert!(generate(3, &Options { notes: 0 }).notes.is_empty());
    assert_eq!(generate(3, &Options { notes: 1 }).notes.len(), 1);
}

#[test]
fn every_embed_and_image_resolves() {
    let attachments: BTreeSet<&str> = VAULT.attachments.iter().map(|a| a.path.as_str()).collect();
    let titles: BTreeSet<String> = VAULT
        .notes
        .iter()
        .map(|n| {
            n.path
                .rsplit('/')
                .next()
                .unwrap()
                .trim_end_matches(".md")
                .to_string()
        })
        .collect();
    let mut referenced = BTreeSet::new();
    for (path, scan) in SCANS.iter() {
        let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
        let images = if folder.is_empty() {
            "images".to_string()
        } else {
            format!("{folder}/images")
        };
        for target in &scan.embeds {
            if target.ends_with(".png") {
                let attachment = format!("{images}/{target}");
                assert!(
                    attachments.contains(attachment.as_str()),
                    "{path}: embed {target} has no file"
                );
                referenced.insert(attachment);
            } else {
                assert!(
                    titles.contains(target),
                    "{path}: embed {target} names no note"
                );
            }
        }
        for source in &scan.img_sources {
            let attachment = format!("{}/{source}", folder)
                .trim_start_matches('/')
                .to_string();
            assert!(
                attachments.contains(attachment.as_str()),
                "{path}: <img> {source} has no file"
            );
            referenced.insert(attachment);
        }
        for target in &scan.wikilinks {
            assert!(
                titles.contains(target),
                "{path}: wikilink {target} names no note"
            );
        }
    }
    assert_eq!(
        referenced.len(),
        attachments.len(),
        "some attachments are never used"
    );
}

#[test]
fn attachments_are_tiny_pngs() {
    for attachment in &VAULT.attachments {
        assert!(
            attachment.bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "{}",
            attachment.path
        );
        assert!(attachment.bytes.len() < 200, "{}", attachment.path);
    }
}

#[test]
fn only_deliberate_footnotes_are_broken() {
    let mut expected = BTreeSet::new();
    for broken in &VAULT.broken_footnotes {
        expected.insert((broken.note.clone(), broken.label.clone(), broken.problem));
        if broken.problem == FootnoteProblem::Typo {
            expected.insert((
                broken.note.clone(),
                broken.label.clone(),
                FootnoteProblem::Unused,
            ));
        }
    }
    let mut found = BTreeSet::new();
    for (path, scan) in SCANS.iter() {
        for (label, problem) in scan.footnote_problems() {
            found.insert((path.clone(), label, problem));
        }
    }
    assert_eq!(found, expected);
    for problem in [
        FootnoteProblem::Missing,
        FootnoteProblem::Unused,
        FootnoteProblem::Duplicate,
        FootnoteProblem::Empty,
        FootnoteProblem::Typo,
    ] {
        assert!(
            expected.iter().any(|(_, _, p)| *p == problem),
            "no deliberate {problem:?} footnote"
        );
    }
}

#[test]
fn every_reference_has_a_definition_unless_deliberately_missing() {
    let missing: BTreeSet<(String, String)> = VAULT
        .broken_footnotes
        .iter()
        .filter(|b| b.problem == FootnoteProblem::Missing)
        .map(|b| (b.note.clone(), b.label.clone()))
        .collect();
    for (path, scan) in SCANS.iter() {
        let defined: BTreeSet<&String> =
            scan.footnote_defs.iter().map(|(label, _)| label).collect();
        for label in &scan.footnote_refs {
            let deliberate = missing.contains(&(path.clone(), label.clone()));
            assert!(
                defined.contains(label) || deliberate,
                "{path}: [^{label}] has no definition"
            );
        }
    }
}

#[test]
fn corpus_stays_small() {
    let notes: usize = VAULT.notes.iter().map(|n| n.text.len()).sum();
    let attachments: usize = VAULT.attachments.iter().map(|a| a.bytes.len()).sum();
    let manifest = manifest_json(&VAULT).len();
    assert!(notes + attachments + manifest < 3_000_000);
}

#[test]
fn paths_are_portable() {
    for path in VAULT
        .notes
        .iter()
        .map(|n| &n.path)
        .chain(VAULT.attachments.iter().map(|a| &a.path))
    {
        assert!(
            !path.contains(['\\', ':', '*', '?', '"', '<', '>', '|', '#', '^', '[', ']']),
            "{path}"
        );
        assert!(!path.starts_with('/'), "{path}");
    }
    assert!(VAULT.notes.iter().any(|n| n.path.contains(' ')));
    let unique: BTreeSet<String> = VAULT.notes.iter().map(|n| n.path.to_lowercase()).collect();
    assert_eq!(unique.len(), VAULT.notes.len());
}

#[test]
fn committed_fixture_is_up_to_date() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/corpus");
    let files = files_under(&dir);
    let mut expected: BTreeMap<PathBuf, Vec<u8>> = BTreeMap::new();
    for note in &VAULT.notes {
        expected.insert(
            note.path.split('/').collect(),
            note.text.clone().into_bytes(),
        );
    }
    for attachment in &VAULT.attachments {
        expected.insert(
            attachment.path.split('/').collect(),
            attachment.bytes.clone(),
        );
    }
    expected.insert(
        PathBuf::from("manifest.json"),
        manifest_json(&VAULT).into_bytes(),
    );
    expected.insert(
        PathBuf::from(GITATTRIBUTES.0),
        GITATTRIBUTES.1.as_bytes().to_vec(),
    );
    let stale: Vec<&PathBuf> = expected
        .keys()
        .filter(|p| files.get(*p) != expected.get(*p))
        .collect();
    let extra: Vec<&PathBuf> = files
        .keys()
        .filter(|p| !expected.contains_key(*p))
        .collect();
    assert!(
        stale.is_empty() && extra.is_empty(),
        "fixtures/corpus is stale; run `cargo run -p editor-corpus -- --out fixtures/corpus`. stale: {stale:?}, extra: {extra:?}"
    );
}
