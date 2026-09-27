//! Drives the vault search panel over a temporary vault: live results,
//! ranking and grouping, keyboard navigation and replace across notes.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use editor_desktop::actions::bind_keys;
use editor_desktop::find;
use editor_desktop::vault_search::{self, VaultSearch, VaultSearchEvent};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn vault() -> tempfile::TempDir {
    let vault = tempfile::tempdir().unwrap();
    let root = vault.path();
    write(
        root,
        "Daily/Monday.md",
        "Walked past the prism shop.\nNothing else.",
    );
    write(
        root,
        "Optics.md",
        "# Prism\nLight bends in a prism.\n## Lenses",
    );
    write(root, "Prism Notes.md", "A list of things.");
    write(root, "Unrelated.md", "Nothing to see.");
    vault
}

struct Panel<'a> {
    panel: Entity<VaultSearch>,
    events: Rc<RefCell<Vec<VaultSearchEvent>>>,
    cx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext, root: PathBuf) -> Panel<'_> {
    cx.update(|cx| {
        bind_keys(cx);
        find::bind_keys(cx);
        vault_search::bind_keys(cx);
    });
    let (panel, cx) = cx.add_window_view(move |window, cx| VaultSearch::new(root, window, cx));
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&panel, move |_, event: &VaultSearchEvent, _| {
            sink.borrow_mut().push(event.clone())
        })
        .detach();
    });
    cx.run_until_parked();
    Panel { panel, events, cx }
}

impl Panel<'_> {
    fn type_text(&mut self, text: &str) {
        self.cx.simulate_input(text);
        self.cx.run_until_parked();
    }

    fn keys(&mut self, keys: &str) {
        self.cx.simulate_keystrokes(keys);
        self.cx.run_until_parked();
    }

    fn order(&mut self) -> Vec<String> {
        self.panel.read_with(self.cx, |panel, _| {
            panel
                .results()
                .iter()
                .map(|result| result.path.to_string_lossy().replace('\\', "/"))
                .collect()
        })
    }
}

#[gpui::test]
fn results_rank_file_names_then_headings_then_body(cx: &mut TestAppContext) {
    let vault = vault();
    let mut p = open(cx, vault.path().to_path_buf());
    p.type_text("PRÍSM");
    assert_eq!(
        p.order(),
        vec!["Prism Notes.md", "Optics.md", "Daily/Monday.md"]
    );
    p.panel.read_with(p.cx, |panel, _| {
        let optics = &panel.results()[1];
        assert_eq!(optics.match_count, 2);
        assert_eq!(optics.hits[1].excerpt, "Light bends in a prism.");
        assert_eq!(optics.hits[1].ranges, vec![17..22]);
        assert_eq!(panel.rows().len(), 3 + 2 + 1);
    });
}

#[gpui::test]
fn results_update_on_every_keystroke(cx: &mut TestAppContext) {
    let vault = vault();
    let mut p = open(cx, vault.path().to_path_buf());
    p.type_text("noth");
    assert_eq!(p.order(), vec!["Daily/Monday.md", "Unrelated.md"]);
    p.type_text("ing t");
    assert_eq!(p.order(), vec!["Unrelated.md"]);
    p.type_text("zz");
    assert!(p.order().is_empty());
}

#[gpui::test]
fn arrows_move_and_enter_opens_at_the_match(cx: &mut TestAppContext) {
    let vault = vault();
    let root = vault.path().to_path_buf();
    let mut p = open(cx, root.clone());
    p.type_text("prism");
    p.keys("down down down");
    assert_eq!(
        p.panel.read_with(p.cx, |panel, _| panel.selected_index()),
        3
    );
    p.keys("enter");
    p.keys("up up up up");
    p.keys("enter");
    assert_eq!(
        *p.events.borrow(),
        vec![
            VaultSearchEvent::Open {
                path: root.join("Optics.md"),
                offset: 25,
            },
            VaultSearchEvent::Open {
                path: root.join("Daily/Monday.md"),
                offset: 16,
            },
        ]
    );
    p.keys("escape");
    assert_eq!(p.events.borrow().last(), Some(&VaultSearchEvent::Dismissed));
}

#[gpui::test]
fn replace_all_asks_then_rewrites_notes(cx: &mut TestAppContext) {
    let vault = vault();
    let root = vault.path().to_path_buf();
    let mut p = open(cx, root.clone());
    p.type_text("prism");
    p.keys("tab");
    p.type_text("lens");
    p.keys("enter");
    let prompt = p.panel.read_with(p.cx, |panel, _| {
        panel.pending_replace().map(vault_search::replace_prompt)
    });
    assert_eq!(prompt.as_deref(), Some("Replace 3 matches in 2 notes?"));
    p.keys("escape");
    assert!(
        p.panel
            .read_with(p.cx, |panel, _| panel.pending_replace().is_none())
    );
    assert!(p.events.borrow().is_empty());

    p.keys("enter enter");
    let status = p
        .panel
        .read_with(p.cx, |panel, _| panel.status().map(str::to_owned));
    assert_eq!(status.as_deref(), Some("Replaced 3 matches in 2 notes"));
    let optics = std::fs::read_to_string(root.join("Optics.md")).unwrap();
    assert_eq!(optics, "# lens\nLight bends in a lens.\n## Lenses");
    let monday = std::fs::read_to_string(root.join("Daily/Monday.md")).unwrap();
    assert!(monday.starts_with("Walked past the lens shop."));
    let notes = std::fs::read_to_string(root.join("Prism Notes.md")).unwrap();
    assert_eq!(notes, "A list of things.");
    let replaced = match p.events.borrow().last() {
        Some(VaultSearchEvent::Replaced { paths }) => paths.len(),
        other => panic!("expected a replace event, got {other:?}"),
    };
    assert_eq!(replaced, 2);
    // The panel searched again: only the file name still matches.
    assert_eq!(p.order(), vec!["Prism Notes.md"]);
}
