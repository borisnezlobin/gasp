//! Edit-time tracking in a real workspace: typing counts, the time lands
//! in this device's file under `.gasp/stats/`, other devices' files add
//! to it, Chronotyper's frontmatter is the starting value, and the status
//! bar says it.

use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use gasp_desktop::actions::bind_keys;
use gasp_desktop::edit_time::{STATS_DIR, StatsFile, load};
use gasp_desktop::features;
use gasp_desktop::workspace::{OpenIn, Workspace};
use gpui::{Entity, TestAppContext, VisualTestContext};

const NOTE: &str = "---\ntitle: Waves\nedited_seconds: 600\n---\n# Waves\n\nText.\n";

/// Snapshots from these tests go to a folder of their own, never the
/// real data folder.
fn data_dir() {
    static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        gasp_desktop::recovery::store::use_data_dir(dir.path().to_path_buf());
        dir
    });
}

fn open_workspace<'a>(
    cx: &'a mut TestAppContext,
    vault: &Path,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    data_dir();
    cx.update(|cx| {
        bind_keys(cx);
        features::bind_view_keys(cx);
    });
    let vault = vault.to_path_buf();
    let (workspace, cx) = cx.add_window_view(move |window, cx| {
        let mut workspace = Workspace::new(&vault, window, cx);
        features::install(&mut workspace, window, cx);
        workspace
    });
    cx.run_until_parked();
    (workspace, cx)
}

fn open_note(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            let path = workspace.vault().join("Waves.md");
            workspace
                .open_path(&path, OpenIn::ActiveTab, window, cx)
                .unwrap();
            workspace.focus_active(window, cx);
        })
    });
    cx.run_until_parked();
}

fn status_items(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.read(|cx| workspace.read(cx).status().unwrap().items())
}

#[gpui::test]
fn typing_is_counted_saved_per_device_and_summed(cx: &mut TestAppContext) {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("Waves.md"), NOTE).unwrap();
    let phone = StatsFile {
        device: "Phone".into(),
        edited_seconds: [("Waves.md".to_owned(), 120)].into(),
    };
    let stats = vault.path().join(STATS_DIR);
    std::fs::create_dir_all(&stats).unwrap();
    std::fs::write(
        stats.join("phone-abc123.json"),
        serde_json::to_string(&phone).unwrap(),
    )
    .unwrap();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open_note(&workspace, cx);
    // Chronotyper's ten minutes and the phone's two.
    assert!(
        status_items(&workspace, cx).contains(&"12 min editing".to_owned()),
        "{:?}",
        status_items(&workspace, cx)
    );

    // Two keystrokes 30 seconds apart count 30 seconds.
    let editor = cx.read(|cx| workspace.read(cx).active_editor(cx).unwrap());
    editor.update(cx, |editor, cx| {
        let end = editor.doc().len();
        editor.move_to(end, false, cx)
    });
    cx.simulate_input("a");
    cx.executor().advance_clock(Duration::from_secs(30));
    cx.simulate_input("b");
    // A keystroke after a long break adds nothing.
    cx.executor().advance_clock(Duration::from_secs(600));
    cx.simulate_input("c");
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();

    let id = cx.read(|cx| workspace.read(cx).device_state(cx).device_id);
    assert!(!id.is_empty(), "the device has an id for its file");
    let loaded = load(vault.path(), &id);
    assert_eq!(loaded.own.edited_seconds.get("Waves.md"), Some(&30));
    assert_eq!(loaded.others.get("Waves.md"), Some(&120));
    assert!(
        status_items(&workspace, cx).contains(&"13 min editing".to_owned()),
        "{:?}",
        status_items(&workspace, cx)
    );
    // The frontmatter stays as it was: nothing is written into notes.
    let text = cx.read(|cx| editor.read(cx).text());
    assert!(text.starts_with("---\ntitle: Waves\nedited_seconds: 600\n---\n"));
}

#[gpui::test]
fn reloading_a_note_from_disk_is_not_editing(cx: &mut TestAppContext) {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("Waves.md"), "# Waves\n").unwrap();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open_note(&workspace, cx);
    // Focus is elsewhere, as when sync rewrites the note in the background.
    cx.update(|window, _| window.blur());
    let editor = cx.read(|cx| workspace.read(cx).active_editor(cx).unwrap());
    for _ in 0..3 {
        editor.update(cx, |editor, cx| editor.replace(0..0, "x", cx));
        cx.executor().advance_clock(Duration::from_secs(5));
    }
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    assert!(
        !vault.path().join(STATS_DIR).exists(),
        "nothing was counted"
    );
    let items = status_items(&workspace, cx);
    assert!(
        !items.iter().any(|item| item.ends_with("editing")),
        "{items:?}"
    );
}

#[gpui::test]
fn a_moved_note_keeps_its_edit_time_and_snapshots_unopened(cx: &mut TestAppContext) {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("Waves.md"), "# Waves\n").unwrap();
    let phone = StatsFile {
        device: "Phone".into(),
        edited_seconds: [("Waves.md".to_owned(), 120)].into(),
    };
    let stats = vault.path().join(STATS_DIR);
    std::fs::create_dir_all(&stats).unwrap();
    std::fs::write(
        stats.join("phone-abc123.json"),
        serde_json::to_string(&phone).unwrap(),
    )
    .unwrap();
    let (workspace, cx) = open_workspace(cx, vault.path());
    let from = vault.path().join("Waves.md");
    let to = vault.path().join("Physics/Light waves.md");
    cx.update(|_, cx| gasp_desktop::recovery::keep_version(&from, "an older draft", cx));
    cx.run_until_parked();

    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::rename(&from, &to).unwrap();
    workspace.update(cx, |workspace, cx| workspace.entry_moved(&from, &to, cx));
    cx.executor().advance_clock(Duration::from_secs(60));
    cx.run_until_parked();

    let id = cx.read(|cx| workspace.read(cx).device_state(cx).device_id);
    let loaded = load(vault.path(), &id);
    assert_eq!(
        loaded.own.edited_seconds.get("Physics/Light waves.md"),
        Some(&120),
        "the time spent on it moved with it"
    );
    let (store, relative) = cx
        .read(|cx| gasp_desktop::recovery::store_for(&to, cx))
        .expect("the vault keeps snapshots");
    let kept = store.list(&relative);
    assert_eq!(kept.len(), 1, "its snapshot moved with it");
    assert_eq!(store.read(&kept[0]).unwrap(), "an older draft");
}

/// macOS's temporary folder is reached through a symlink (`/var` is
/// `/private/var`), and the workspace keeps the vault's real path: a
/// note named through the link is still the vault's.
#[cfg(unix)]
#[gpui::test]
fn a_note_named_through_a_link_to_the_vault_is_still_the_vaults(cx: &mut TestAppContext) {
    let real = tempfile::tempdir().unwrap();
    std::fs::write(real.path().join("Waves.md"), "# Waves\n").unwrap();
    let phone = StatsFile {
        device: "Phone".into(),
        edited_seconds: [("Waves.md".to_owned(), 120)].into(),
    };
    let stats = real.path().join(STATS_DIR);
    std::fs::create_dir_all(&stats).unwrap();
    std::fs::write(
        stats.join("phone-abc123.json"),
        serde_json::to_string(&phone).unwrap(),
    )
    .unwrap();
    let links = tempfile::tempdir().unwrap();
    let vault = links.path().join("vault");
    std::os::unix::fs::symlink(real.path(), &vault).unwrap();
    let (workspace, cx) = open_workspace(cx, &vault);

    let from = vault.join("Waves.md");
    let to = vault.join("Physics/Light waves.md");
    cx.update(|_, cx| gasp_desktop::recovery::keep_version(&from, "an older draft", cx));
    cx.run_until_parked();
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::rename(&from, &to).unwrap();
    workspace.update(cx, |workspace, cx| workspace.entry_moved(&from, &to, cx));
    cx.executor().advance_clock(Duration::from_secs(60));
    cx.run_until_parked();

    let id = cx.read(|cx| workspace.read(cx).device_state(cx).device_id);
    let loaded = load(real.path(), &id);
    assert_eq!(
        loaded.own.edited_seconds.get("Physics/Light waves.md"),
        Some(&120),
        "the time spent on it moved with it"
    );
    let (store, relative) = cx
        .read(|cx| gasp_desktop::recovery::store_for(&to, cx))
        .expect("the vault keeps snapshots");
    let kept = store.list(&relative);
    assert_eq!(kept.len(), 1, "its snapshot moved with it");
}
