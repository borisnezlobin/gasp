//! Sync in the app, end to end: a bare repository in a temp dir stands in
//! for GitHub and two clones stand in for two devices. One clone is the
//! vault the window has open; the other is driven straight through the
//! engine. Nothing here touches a real repository or the network.

use gasp_config::CONFIG_DIR;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gasp_config::settings::SyncSettings;
use gasp_desktop::actions::bind_keys;
use gasp_desktop::features;
use gasp_desktop::notices::{self, NoticeKind};
use gasp_desktop::settings_view::{ControlRow, SettingsView};
use gasp_desktop::sync::setup::{SetupField, SetupStop};
use gasp_desktop::sync::start::{Stage, StartAction};
use gasp_desktop::sync::{
    ConflictResolver, SetupPhase, SetupProblem, SyncPhase, SyncService, SyncSetup, SyncStart,
    set_credential_store,
};
use gasp_desktop::workspace::{OpenIn, Workspace};
use gasp_sync::{
    Author, CredentialStore, InMemoryCredentialStore, SyncStep, Token, Vault, VaultConfig,
};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use tempfile::TempDir;

/// A bare remote on `master` plus a folder for each device.
struct World {
    dir: TempDir,
    remote: String,
}

fn author(name: &str) -> Author {
    Author::new(name, format!("{name}@devices.invalid"))
}

impl World {
    fn seeded(files: &[(&str, &str)]) -> World {
        World::seeded_on("master", files)
    }

    fn seeded_on(branch: &str, files: &[(&str, &str)]) -> World {
        let dir = tempfile::tempdir().unwrap();
        let bare = dir.path().join("remote.git");
        let mut options = git2::RepositoryInitOptions::new();
        options.bare(true).initial_head(branch);
        git2::Repository::init_opts(&bare, &options).unwrap();
        let remote = bare.to_str().unwrap().to_owned();
        let world = World { dir, remote };
        let config = VaultConfig {
            branch: branch.to_owned(),
            legacy_branch: None,
            ..VaultConfig::default()
        };
        let seed = Vault::init(world.path("seed"), &world.remote, config).unwrap();
        for (name, text) in files {
            write(seed.root(), name, text);
        }
        seed.commit_all(&author("seed"), "Seed").unwrap();
        seed.push().unwrap();
        world
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// A device's clone, synced through the engine directly.
    fn device(&self, name: &str) -> Vault {
        Vault::clone_remote(&self.remote, self.path(name), VaultConfig::default(), None).unwrap()
    }

    fn remote_file(&self, path: &str) -> Option<String> {
        let repo = git2::Repository::open_bare(&self.remote).unwrap();
        let tree = repo
            .find_reference("refs/heads/master")
            .ok()?
            .peel_to_tree()
            .ok()?;
        let blob = repo
            .find_blob(tree.get_path(Path::new(path)).ok()?.id())
            .ok()?;
        Some(String::from_utf8_lossy(blob.content()).into_owned())
    }

    /// The message of the newest commit on the remote's `master`.
    fn remote_message(&self) -> String {
        let repo = git2::Repository::open_bare(&self.remote).unwrap();
        let commit = repo
            .find_reference("refs/heads/master")
            .and_then(|reference| reference.peel_to_commit())
            .unwrap();
        commit.message().unwrap_or_default().to_owned()
    }
}

fn write(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// Commits, merges and pushes on another device.
fn sync_device(vault: &Vault, name: &str) {
    vault.commit_all(&author(name), "Edit").unwrap();
    vault.fetch().unwrap();
    vault.merge(&author(name)).unwrap();
    vault.push().unwrap();
}

fn open_workspace<'a>(
    cx: &'a mut TestAppContext,
    vault: &Path,
    store: Arc<InMemoryCredentialStore>,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    // Stand-ins for iCloud Drive and GitHub beside the vault, so nothing
    // here reaches the real ones.
    let beside = vault
        .ancestors()
        .skip(1)
        .find(|folder| !folder.iter().any(|part| part == "stand-in-icloud"))
        .unwrap_or(vault)
        .to_path_buf();
    cx.update(|cx| {
        bind_keys(cx);
        features::bind_view_keys(cx);
        set_credential_store(store, cx);
        gasp_desktop::sync::icloud::use_drive(beside.join("stand-in-icloud"), cx);
        gasp_desktop::sync::github_client::use_stand_in(beside.join("stand-in-github"), cx);
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

fn service(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<SyncService> {
    cx.read(|cx| workspace.read(cx).sync().unwrap().clone())
}

fn phase(service: &Entity<SyncService>, cx: &mut VisualTestContext) -> SyncPhase {
    cx.read(|cx| service.read(cx).phase())
}

fn run(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, command: &str) {
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.run_command(command, window, cx)
        });
    });
    cx.run_until_parked();
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} isn't drawn"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

fn is_drawn(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.run_until_parked();
    cx.debug_bounds(selector).is_some()
}

/// Tells sync a file changed, as the vault watcher does.
fn edited(service: &Entity<SyncService>, cx: &mut VisualTestContext, path: PathBuf) {
    service.update(cx, |service, cx| service.files_changed(&[path], cx));
    cx.run_until_parked();
}

#[gpui::test]
fn a_vault_that_isnt_a_clone_shows_no_sync(cx: &mut TestAppContext) {
    let vault = tempfile::tempdir().unwrap();
    write(vault.path(), "a.md", "A\n");
    let (workspace, cx) = open_workspace(cx, vault.path(), Arc::default());
    let service = service(&workspace, cx);
    assert_eq!(phase(&service, cx), SyncPhase::Hidden);
    assert!(!is_drawn(cx, "sync-indicator-button"));
    // Sync now saves instead, says so, and offers setting sync up once,
    // without opening anything unasked.
    run(&workspace, cx, "sync.now");
    assert!(cx.read(|cx| workspace.read(cx).active_modal::<SettingsView>().is_none()));
    let shown = cx.update(|window, cx| notices::shown_in(window.window_handle(), cx));
    let (_, notice) = shown.last().expect("a notice says what happened");
    assert_eq!(notice.kind, NoticeKind::Done);
    assert_eq!(notice.message.as_ref(), "Everything’s saved.");
    let action = notice
        .action
        .clone()
        .expect("the first one offers setting up");
    assert_eq!(action.command.as_ref(), "sync.set-up");
    run(&workspace, cx, "sync.now");
    let shown = cx.update(|window, cx| notices::shown_in(window.window_handle(), cx));
    assert!(
        shown.iter().all(|(_, notice)| notice.action.is_none()),
        "the offer isn't repeated"
    );
    run(&workspace, cx, &action.command);
    assert!(cx.read(|cx| workspace.read(cx).active_modal::<SyncStart>().is_some()));
}

#[gpui::test]
fn opening_the_vault_syncs_and_edits_follow_a_minute_later(cx: &mut TestAppContext) {
    let world = World::seeded(&[("note.md", "first\n")]);
    let laptop = world.path("laptop");
    drop(world.device("laptop"));
    let (workspace, cx) = open_workspace(cx, &laptop, Arc::default());
    let service = service(&workspace, cx);
    assert_eq!(phase(&service, cx), SyncPhase::Synced);
    assert!(is_drawn(cx, "sync-indicator-button"));

    write(&laptop, "note.md", "first\nwritten on the laptop\n");
    edited(&service, cx, laptop.join("note.md"));
    assert_eq!(phase(&service, cx), SyncPhase::Synced);
    assert_eq!(world.remote_file("note.md").unwrap(), "first\n");

    // Nothing goes out until edits have stopped for a minute.
    cx.executor().advance_clock(Duration::from_secs(59));
    cx.run_until_parked();
    assert_eq!(world.remote_file("note.md").unwrap(), "first\n");
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(
        world.remote_file("note.md").unwrap(),
        "first\nwritten on the laptop\n"
    );
    let sent = cx.read(|cx| service.read(cx).last_run().unwrap().sent.clone());
    assert_eq!(sent, [PathBuf::from("note.md")]);
}

#[gpui::test]
fn sync_now_brings_in_another_devices_changes(cx: &mut TestAppContext) {
    let world = World::seeded(&[("note.md", "first\n")]);
    let laptop = world.path("laptop");
    drop(world.device("laptop"));
    let phone = world.device("phone");
    let (workspace, cx) = open_workspace(cx, &laptop, Arc::default());
    let service = service(&workspace, cx);

    write(phone.root(), "from phone.md", "hello\n");
    sync_device(&phone, "phone");
    run(&workspace, cx, "sync.now");
    assert_eq!(
        std::fs::read_to_string(laptop.join("from phone.md")).unwrap(),
        "hello\n"
    );
    let received = cx.read(|cx| {
        service
            .read(cx)
            .recent_runs()
            .next()
            .unwrap()
            .received
            .clone()
    });
    assert_eq!(received, [PathBuf::from("from phone.md")]);

    // The popover says when and what.
    click(cx, "sync-indicator-button");
    assert!(is_drawn(cx, "sync-popover"));
    cx.simulate_keystrokes("escape");
    assert!(!is_drawn(cx, "sync-popover"));
}

#[gpui::test]
fn an_unreachable_remote_shows_offline_and_recovers(cx: &mut TestAppContext) {
    let world = World::seeded(&[("note.md", "first\n")]);
    let laptop = world.path("laptop");
    let clone = world.device("laptop");
    clone
        .set_remote_url(&world.path("missing.git").to_string_lossy())
        .unwrap();
    drop(clone);
    write(&laptop, "note.md", "first\noffline edit\n");
    let (workspace, cx) = open_workspace(cx, &laptop, Arc::default());
    let service = service(&workspace, cx);
    assert_eq!(phase(&service, cx), SyncPhase::Offline { waiting: 1 });

    let clone = Vault::open(&laptop, VaultConfig::default()).unwrap();
    clone.set_remote_url(&world.remote).unwrap();
    // It retries on its own a minute later.
    cx.executor().advance_clock(Duration::from_secs(61));
    cx.run_until_parked();
    assert_eq!(phase(&service, cx), SyncPhase::Synced);
    assert_eq!(
        world.remote_file("note.md").unwrap(),
        "first\noffline edit\n"
    );
}

#[gpui::test]
fn a_conflict_shows_a_banner_and_the_resolver_finishes_the_merge(cx: &mut TestAppContext) {
    let world = World::seeded(&[("note.md", "title\nshared line\nend\n")]);
    let laptop = world.path("laptop");
    drop(world.device("laptop"));
    let phone = world.device("phone");
    write(phone.root(), "note.md", "title\nphone's line\nend\n");
    sync_device(&phone, "phone");
    write(&laptop, "note.md", "title\nlaptop's line\nend\n");

    let (workspace, cx) = open_workspace(cx, &laptop, Arc::default());
    let service = service(&workspace, cx);
    assert_eq!(phase(&service, cx), SyncPhase::Conflict { files: 1 });
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace
                .open_path(Path::new("note.md"), OpenIn::ActiveTab, window, cx)
                .unwrap()
        })
    });
    assert!(is_drawn(cx, "sync-conflict-banner"));

    click(cx, "sync-banner-resolve");
    let resolver = cx.read(|cx| workspace.read(cx).active_modal::<ConflictResolver>());
    assert!(resolver.is_some(), "the resolver opens");
    click(cx, "resolve-0-both");
    click(cx, "resolve-0-mine");
    click(cx, "resolver-finish");
    assert!(cx.read(|cx| {
        workspace
            .read(cx)
            .active_modal::<ConflictResolver>()
            .is_none()
    }));
    assert_eq!(phase(&service, cx), SyncPhase::Synced);
    assert_eq!(
        world.remote_file("note.md").unwrap(),
        "title\nlaptop's line\nend\n"
    );
    // Drawn bounds outlive the element in tests, so ask the pane.
    let banner = cx.read(|cx| {
        let workspace = workspace.read(cx);
        workspace.active_pane().read(cx).shows_sync_conflict(cx)
    });
    assert!(!banner);
}

#[gpui::test]
fn the_resolver_works_from_the_keyboard(cx: &mut TestAppContext) {
    let base = "one\nshared a\ntwo\nthree\nfour\nshared b\nfive\n";
    let world = World::seeded(&[("note.md", base)]);
    let laptop = world.path("laptop");
    drop(world.device("laptop"));
    let phone = world.device("phone");
    let phone_text = base
        .replace("shared a", "phone a")
        .replace("shared b", "phone b");
    write(phone.root(), "note.md", &phone_text);
    sync_device(&phone, "phone");
    let laptop_text = base
        .replace("shared a", "laptop a")
        .replace("shared b", "laptop b");
    write(&laptop, "note.md", &laptop_text);

    let (workspace, cx) = open_workspace(cx, &laptop, Arc::default());
    run(&workspace, cx, "sync.resolve-conflicts");
    let resolver = cx
        .read(|cx| workspace.read(cx).active_modal::<ConflictResolver>())
        .expect("the resolver opens");
    let current = |cx: &mut VisualTestContext| cx.read(|cx| resolver.read(cx).current());
    assert_eq!(current(cx), (0, 0));
    // Enter with places open goes to one instead of finishing.
    cx.simulate_keystrokes("down");
    assert_eq!(current(cx), (0, 1));
    cx.simulate_keystrokes("2");
    assert_eq!(current(cx), (0, 0), "on to the place still open");
    cx.simulate_keystrokes("enter");
    assert!(cx.read(|cx| !resolver.read(cx).is_complete()));
    cx.simulate_keystrokes("3");
    assert!(cx.read(|cx| resolver.read(cx).is_complete()));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(cx.read(|cx| {
        workspace
            .read(cx)
            .active_modal::<ConflictResolver>()
            .is_none()
    }));
    let merged = world.remote_file("note.md").unwrap();
    assert!(merged.contains("laptop a\nphone a\n"), "{merged}");
    assert!(
        merged.contains("phone b") && !merged.contains("laptop b"),
        "{merged}"
    );
}

/// The laptop's window opens on a note the phone changed on the same line,
/// with a third device, the desktop, synced straight through the engine.
fn laptop_window_with_a_conflict(
    cx: &mut TestAppContext,
) -> (World, Vault, Entity<Workspace>, &mut VisualTestContext) {
    let world = World::seeded(&[
        ("note.md", "title\nshared line\nend\n"),
        ("other.md", "other\n"),
    ]);
    let laptop = world.path("laptop");
    drop(world.device("laptop"));
    let phone = world.device("phone");
    write(phone.root(), "note.md", "title\nphone's line\nend\n");
    sync_device(&phone, "phone");
    write(&laptop, "note.md", "title\nlaptop's line\nend\n");
    let desktop = world.device("desktop");
    let (workspace, cx) = open_workspace(cx, &laptop, Arc::default());
    (world, desktop, workspace, cx)
}

#[gpui::test]
fn a_conflict_doesnt_stop_other_notes_from_syncing(cx: &mut TestAppContext) {
    let (world, desktop, workspace, cx) = laptop_window_with_a_conflict(cx);
    let laptop = world.path("laptop");
    let service = service(&workspace, cx);
    assert_eq!(phase(&service, cx), SyncPhase::Conflict { files: 1 });
    assert!(!laptop.join(".git/MERGE_HEAD").exists());
    assert_eq!(
        world.remote_file("note.md").unwrap(),
        "title\nphone's line\nend\n",
        "the phone's text stays on the remote"
    );

    // An edit to another note goes out a minute later, as usual.
    write(&laptop, "other.md", "other\nwritten on the laptop\n");
    edited(&service, cx, laptop.join("other.md"));
    cx.executor().advance_clock(Duration::from_secs(61));
    cx.run_until_parked();
    assert_eq!(
        world.remote_file("other.md").unwrap(),
        "other\nwritten on the laptop\n"
    );
    assert_eq!(
        world.remote_message(),
        format!("{}: other.md", gasp_desktop::edit_time::device_name())
    );
    assert_eq!(phase(&service, cx), SyncPhase::Conflict { files: 1 });
    sync_device(&desktop, "desktop");
    assert_eq!(
        std::fs::read_to_string(desktop.root().join("other.md")).unwrap(),
        "other\nwritten on the laptop\n"
    );

    // Sync now still brings in other devices' changes.
    write(desktop.root(), "from desktop.md", "hello\n");
    sync_device(&desktop, "desktop");
    run(&workspace, cx, "sync.now");
    assert_eq!(
        std::fs::read_to_string(laptop.join("from desktop.md")).unwrap(),
        "hello\n"
    );
    assert_eq!(phase(&service, cx), SyncPhase::Conflict { files: 1 });
}

#[gpui::test]
fn each_resolution_in_the_resolver_syncs_the_note(cx: &mut TestAppContext) {
    let cases = [
        ("resolve-0-mine", "title\nlaptop's line\nend\n"),
        ("resolve-0-theirs", "title\nphone's line\nend\n"),
        (
            "resolve-0-both",
            "title\nlaptop's line\nphone's line\nend\n",
        ),
    ];
    for (button, expected) in cases {
        let (world, desktop, workspace, cx) = laptop_window_with_a_conflict(cx);
        run(&workspace, cx, "sync.resolve-conflicts");
        click(cx, button);
        click(cx, "resolver-finish");
        let service = service(&workspace, cx);
        assert_eq!(phase(&service, cx), SyncPhase::Synced, "{button}");
        assert_eq!(world.remote_file("note.md").unwrap(), expected, "{button}");
        sync_device(&desktop, "desktop");
        assert_eq!(
            std::fs::read_to_string(desktop.root().join("note.md")).unwrap(),
            expected
        );
    }
}

#[gpui::test]
fn a_conflict_from_before_a_restart_still_shows_and_resolves(cx: &mut TestAppContext) {
    let world = World::seeded(&[("note.md", "title\nshared line\nend\n")]);
    let phone = world.device("phone");
    let laptop = world.device("laptop");
    write(phone.root(), "note.md", "title\nphone's line\nend\n");
    sync_device(&phone, "phone");
    write(laptop.root(), "note.md", "title\nlaptop's line\nend\n");
    sync_device(&laptop, "laptop");
    assert_eq!(laptop.conflicts().unwrap().len(), 1);
    let root = laptop.root().to_owned();
    drop(laptop);

    let (workspace, cx) = open_workspace(cx, &root, Arc::default());
    let service = service(&workspace, cx);
    assert_eq!(phase(&service, cx), SyncPhase::Conflict { files: 1 });
    run(&workspace, cx, "sync.resolve-conflicts");
    click(cx, "resolve-0-both");
    click(cx, "resolver-finish");
    assert_eq!(phase(&service, cx), SyncPhase::Synced);
    assert_eq!(
        world.remote_file("note.md").unwrap(),
        "title\nlaptop's line\nphone's line\nend\n"
    );
}

#[gpui::test]
fn appends_on_two_devices_merge_without_a_conflict(cx: &mut TestAppContext) {
    let world = World::seeded(&[("Lemma.md", "# Lemma\nproof")]);
    let laptop = world.path("laptop");
    drop(world.device("laptop"));
    let phone = world.device("phone");
    write(phone.root(), "Lemma.md", "# Lemma\nproof\nfrom the phone");
    sync_device(&phone, "phone");
    write(&laptop, "Lemma.md", "# Lemma\nproof\nfrom the laptop");

    let (workspace, cx) = open_workspace(cx, &laptop, Arc::default());
    let service = service(&workspace, cx);
    assert_eq!(phase(&service, cx), SyncPhase::Synced);
    assert_eq!(
        world.remote_file("Lemma.md").unwrap(),
        "# Lemma\nproof\nfrom the laptop\nfrom the phone"
    );
}

#[gpui::test]
fn a_vault_on_another_branch_is_left_alone(cx: &mut TestAppContext) {
    let world = World::seeded_on("main", &[("note.md", "on main\n")]);
    let old_tool = world.path("old-tool");
    let config = VaultConfig {
        branch: "main".into(),
        legacy_branch: None,
        ..VaultConfig::default()
    };
    drop(Vault::clone_remote(&world.remote, &old_tool, config, None).unwrap());
    let exclude = old_tool.join(".git/info/exclude");
    std::fs::write(&exclude, "# untouched\n").unwrap();

    let (workspace, cx) = open_workspace(cx, &old_tool, Arc::default());
    let service = service(&workspace, cx);
    let expected = SyncPhase::Setup(SetupProblem::WrongBranch {
        expected: "master".into(),
        actual: Some("main".into()),
    });
    assert_eq!(phase(&service, cx), expected);
    assert_eq!(std::fs::read_to_string(&exclude).unwrap(), "# untouched\n");
    // Sync now explains instead of syncing.
    run(&workspace, cx, "sync.now");
    assert!(is_drawn(cx, "sync-popover"));
}

#[gpui::test]
fn signing_in_keeps_the_token_in_the_store_not_the_vault(cx: &mut TestAppContext) {
    let world = World::seeded(&[("note.md", "first\n")]);
    let laptop = world.path("laptop");
    drop(world.device("laptop"));
    let store = Arc::new(InMemoryCredentialStore::default());
    let (workspace, cx) = open_workspace(cx, &laptop, store.clone());
    let service = service(&workspace, cx);
    assert!(!cx.read(|cx| service.read(cx).is_signed_in()));
    service.update(cx, |service, cx| {
        service.sign_in(Token::new("synthetic-token"), cx).unwrap()
    });
    cx.run_until_parked();
    assert_eq!(
        store.load(&world.remote).unwrap(),
        Some(Token::new("synthetic-token"))
    );
    let leaked = walk(&laptop).into_iter().any(|path| {
        std::fs::read_to_string(path).is_ok_and(|text| text.contains("synthetic-token"))
    });
    assert!(!leaked, "the token was written into the vault");
    service.update(cx, |service, cx| service.sign_out(cx).unwrap());
    assert_eq!(store.load(&world.remote).unwrap(), None);
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(folder).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}

#[gpui::test]
fn the_sync_page_edits_device_only_files(cx: &mut TestAppContext) {
    let world = World::seeded(&[("note.md", "first\n")]);
    let laptop = world.path("laptop");
    drop(world.device("laptop"));
    let (workspace, cx) = open_workspace(cx, &laptop, Arc::default());
    run(&workspace, cx, "settings.open");
    let settings = cx
        .read(|cx| workspace.read(cx).active_modal::<SettingsView>())
        .unwrap();
    settings.update(cx, |settings, cx| settings.show_section("sync", cx));
    cx.run_until_parked();
    let rows = settings.read_with(cx, |settings, _| settings.rows());
    assert_eq!(rows[0], ControlRow::SyncRemote);
    // A remote in a folder needs no token, so there's no sign-in row.
    assert!(!rows.contains(&ControlRow::SyncAccount));
    let patterns: Vec<String> = rows
        .iter()
        .filter_map(|row| match row {
            ControlRow::ListEntry { value, .. } => Some(value.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(patterns, SyncSettings::default().device_only);

    settings.update(cx, |settings, cx| {
        let rows = settings.rows();
        let Some(ControlRow::ListEntry { list, .. }) = rows.last() else {
            panic!("the last row is a pattern");
        };
        settings.remove_list_entry(list, ".trash/**", cx);
    });
    cx.run_until_parked();
    let text = std::fs::read_to_string(laptop.join(CONFIG_DIR).join("settings.toml")).unwrap();
    assert!(text.contains("device-only"), "{text}");
    assert!(!text.contains(".trash"), "{text}");
    let now = cx.read(|cx| {
        workspace
            .read(cx)
            .config()
            .settings
            .sync
            .device_only
            .clone()
    });
    assert_eq!(now.len(), SyncSettings::default().device_only.len() - 1);
}

/// Sync step timings on a synthetic vault of 200 notes, printed for the
/// record. Run with `--nocapture` to see them.
#[gpui::test]
fn step_timings_on_a_synthetic_vault(cx: &mut TestAppContext) {
    let notes: Vec<(String, String)> = (0..200)
        .map(|index| {
            let body: String = (0..40)
                .map(|line| format!("Line {line} of note {index}, with a few more words.\n"))
                .collect();
            (format!("notes/note {index:03}.md"), body)
        })
        .collect();
    let files: Vec<(&str, &str)> = notes
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    let world = World::seeded(&files);
    let laptop = world.path("laptop");
    let started = Instant::now();
    drop(world.device("laptop"));
    let clone = started.elapsed();
    let phone = world.device("phone");
    for index in 0..20 {
        write(
            phone.root(),
            &format!("notes/note {index:03}.md"),
            "phone edit\n",
        );
    }
    sync_device(&phone, "phone");
    for index in 100..120 {
        write(
            &laptop,
            &format!("notes/note {index:03}.md"),
            "laptop edit\n",
        );
    }
    let (workspace, cx) = open_workspace(cx, &laptop, Arc::default());
    let service = service(&workspace, cx);
    assert_eq!(phase(&service, cx), SyncPhase::Synced);
    let run = cx.read(|cx| service.read(cx).last_run().unwrap().clone());
    assert_eq!(run.received.len(), 20);
    // Sent is what the remote didn't have: the laptop's own edits.
    assert_eq!(run.sent.len(), 20);
    eprintln!("clone of 200 notes: {clone:?}");
    for (step, took) in &run.steps {
        eprintln!("{step:?}: {took:?}");
    }
    eprintln!("whole sync: {:?}", run.total());
    let steps: Vec<SyncStep> = run.steps.iter().map(|(step, _)| *step).collect();
    assert_eq!(
        steps,
        [
            SyncStep::Commit,
            SyncStep::Fetch,
            SyncStep::Merge,
            SyncStep::Push
        ]
    );
}

// ---- Setting up sync for a vault that isn't a clone ----

/// A vault folder with `files` and no git, beside an empty bare remote.
fn unsynced_vault(files: &[(&str, &str)]) -> (TempDir, PathBuf, String) {
    let dir = tempfile::tempdir().unwrap();
    let bare = dir.path().join("remote.git");
    let mut options = git2::RepositoryInitOptions::new();
    options.bare(true).initial_head("master");
    git2::Repository::init_opts(&bare, &options).unwrap();
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    for (name, text) in files {
        write(&vault, name, text);
    }
    let remote = bare.to_str().unwrap().to_owned();
    (dir, vault, remote)
}

/// "Set up sync", then the quiet link to the form for an address and a token.
fn open_setup_form(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<SyncSetup> {
    run(workspace, cx, "sync.set-up");
    click(cx, "sync-start-advanced");
    setup_dialog(workspace, cx)
}

fn setup_dialog(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<SyncSetup> {
    cx.read(|cx| workspace.read(cx).active_modal::<SyncSetup>())
        .expect("the setup dialog is open")
}

fn setup_phase(setup: &Entity<SyncSetup>, cx: &mut VisualTestContext) -> SetupPhase {
    setup.read_with(cx, |setup, _| setup.phase().clone())
}

fn fill_and_submit(
    setup: &Entity<SyncSetup>,
    cx: &mut VisualTestContext,
    repository: &str,
    token: &str,
) {
    cx.update(|window, cx| {
        setup.update(cx, |setup, cx| {
            setup.fill(repository, "master", token, cx);
            setup.submit(window, cx);
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn setting_up_sync_makes_the_vault_a_clone_and_starts_syncing(cx: &mut TestAppContext) {
    let (_dir, vault, remote) = unsynced_vault(&[("Plan.md", "# Plan\n")]);
    let store = Arc::new(InMemoryCredentialStore::default());
    let (workspace, cx) = open_workspace(cx, &vault, store.clone());
    let service = service(&workspace, cx);
    assert_eq!(phase(&service, cx), SyncPhase::Hidden);

    let setup = open_setup_form(&workspace, cx);
    fill_and_submit(&setup, cx, &remote, "synthetic-token");
    let SetupPhase::Done(done) = setup_phase(&setup, cx) else {
        panic!("setting up finished: {:?}", setup_phase(&setup, cx));
    };
    assert!(done.report.remote_was_empty);
    assert_eq!(done.report.sent, [PathBuf::from("Plan.md")]);
    assert!(vault.join(".git").is_dir());

    // The window's sync picked up the new clone without a relaunch.
    assert_eq!(phase(&service, cx), SyncPhase::Synced);
    assert!(is_drawn(cx, "sync-indicator-button"));
    assert_eq!(
        store.load(&remote).unwrap(),
        Some(Token::new("synthetic-token"))
    );
    click(cx, "sync-setup-done");
    assert!(cx.read(|cx| workspace.read(cx).active_modal::<SyncSetup>().is_none()));
}

#[gpui::test]
fn setting_up_brings_in_the_repositorys_notes_and_parks_clashes(cx: &mut TestAppContext) {
    let world = World::seeded(&[("Remote.md", "from GitHub\n"), ("Plan.md", "new goal\n")]);
    let vault = world.path("laptop");
    write(&vault, "Plan.md", "old goal\n");
    let (workspace, cx) = open_workspace(cx, &vault, Arc::default());
    let setup = open_setup_form(&workspace, cx);
    fill_and_submit(&setup, cx, &world.remote, "");
    let SetupPhase::Done(done) = setup_phase(&setup, cx) else {
        panic!("setting up finished: {:?}", setup_phase(&setup, cx));
    };
    assert_eq!(done.report.waiting, [PathBuf::from("Plan.md")]);
    assert_eq!(
        std::fs::read_to_string(vault.join("Remote.md")).unwrap(),
        "from GitHub\n"
    );
    let service = service(&workspace, cx);
    assert_eq!(cx.read(|cx| service.read(cx).conflicts().len()), 1);

    // The finished screen offers the resolver, which shows the clash.
    click(cx, "sync-setup-resolve");
    let resolver = cx
        .read(|cx| workspace.read(cx).active_modal::<ConflictResolver>())
        .expect("the resolver opens");
    assert_eq!(
        resolver.read_with(cx, |resolver, _| resolver.files().len()),
        1
    );
}

#[gpui::test]
fn setup_says_what_is_wrong_and_leaves_the_vault_alone(cx: &mut TestAppContext) {
    let (dir, vault, _remote) = unsynced_vault(&[("Plan.md", "# Plan\n")]);
    let (workspace, cx) = open_workspace(cx, &vault, Arc::default());
    let setup = open_setup_form(&workspace, cx);

    fill_and_submit(&setup, cx, "notes", "");
    let SetupPhase::Refused { field, .. } = setup_phase(&setup, cx) else {
        panic!("a bare name isn't a repository");
    };
    assert_eq!(field, Some(SetupField::Repository));

    fill_and_submit(&setup, cx, "you/notes", "");
    let SetupPhase::Refused { field, message } = setup_phase(&setup, cx) else {
        panic!("GitHub needs a token");
    };
    assert_eq!(field, Some(SetupField::Token));
    assert!(message.contains("token"), "{message}");

    let missing = dir.path().join("nowhere.git");
    fill_and_submit(&setup, cx, missing.to_str().unwrap(), "");
    let SetupPhase::Refused { field, message } = setup_phase(&setup, cx) else {
        panic!("a missing repository can't be reached");
    };
    assert_eq!(field, Some(SetupField::Repository));
    assert!(message.contains("Check the address"), "{message}");
    assert!(!vault.join(".git").exists());
    let service = service(&workspace, cx);
    assert_eq!(phase(&service, cx), SyncPhase::Hidden);
}

#[gpui::test]
fn the_setup_dialog_works_from_the_keyboard(cx: &mut TestAppContext) {
    let (_dir, vault, remote) = unsynced_vault(&[("Plan.md", "# Plan\n")]);
    let (workspace, cx) = open_workspace(cx, &vault, Arc::default());
    let setup = open_setup_form(&workspace, cx);
    let stop = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| setup.read(cx).current_stop(window, cx))
    };
    assert_eq!(stop(cx), SetupStop::Repository);
    cx.simulate_input(&remote);
    cx.simulate_keystrokes("tab");
    assert_eq!(stop(cx), SetupStop::Branch);
    cx.simulate_keystrokes("tab tab");
    assert_eq!(stop(cx), SetupStop::CreateToken);
    cx.simulate_keystrokes("tab tab");
    assert_eq!(stop(cx), SetupStop::Submit);
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(stop(cx), SetupStop::Cancel);
    cx.simulate_keystrokes("shift-tab shift-tab shift-tab");
    assert_eq!(stop(cx), SetupStop::Branch);
    // Enter in a field sets up, and Enter again closes the finished screen.
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(matches!(setup_phase(&setup, cx), SetupPhase::Done(_)));
    cx.simulate_keystrokes("enter");
    assert!(cx.read(|cx| workspace.read(cx).active_modal::<SyncSetup>().is_none()));

    // Once the vault syncs, setting up again opens its settings instead.
    run(&workspace, cx, "sync.set-up");
    let settings = cx.read(|cx| workspace.read(cx).active_modal::<SettingsView>());
    assert!(settings.is_some(), "a vault that syncs gets its settings");
}

#[gpui::test]
fn escape_closes_the_setup_dialog(cx: &mut TestAppContext) {
    let (_dir, vault, _remote) = unsynced_vault(&[("Plan.md", "# Plan\n")]);
    let (workspace, cx) = open_workspace(cx, &vault, Arc::default());
    open_setup_form(&workspace, cx);
    cx.simulate_keystrokes("escape");
    assert!(cx.read(|cx| workspace.read(cx).active_modal::<SyncSetup>().is_none()));
}

#[gpui::test]
fn the_sync_page_of_a_vault_that_doesnt_sync_offers_setting_up(cx: &mut TestAppContext) {
    let (_dir, vault, _remote) = unsynced_vault(&[("Plan.md", "# Plan\n")]);
    let (workspace, cx) = open_workspace(cx, &vault, Arc::default());
    run(&workspace, cx, "settings.open");
    let settings = cx
        .read(|cx| workspace.read(cx).active_modal::<SettingsView>())
        .unwrap();
    settings.update(cx, |settings, cx| settings.show_section("sync", cx));
    cx.run_until_parked();
    click(cx, "set-up-sync");
    assert!(cx.read(|cx| workspace.read(cx).active_modal::<SyncStart>().is_some()));
}

fn start_dialog(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<SyncStart> {
    cx.read(|cx| workspace.read(cx).active_modal::<SyncStart>())
        .expect("set up sync is open")
}

/// Gasp's iCloud folder and the one from before it, inside the stand-in
/// for `~/Library/Mobile Documents`.
const ICLOUD_VAULT: &str = "stand-in-icloud/iCloud~com~borisnezlobin~gasp/Documents";
const OLD_ICLOUD_VAULT: &str = "stand-in-icloud/com~apple~CloudDocs/Gasp";

fn start_stage(dialog: &Entity<SyncStart>, cx: &mut VisualTestContext) -> Stage {
    dialog.read_with(cx, |dialog, _| dialog.stage().clone())
}

#[gpui::test]
fn setting_up_offers_icloud_first_then_github_then_the_form(cx: &mut TestAppContext) {
    let (_dir, vault, _remote) = unsynced_vault(&[("Plan.md", "# Plan\n")]);
    let (workspace, cx) = open_workspace(cx, &vault, Arc::default());
    run(&workspace, cx, "sync.set-up");
    let dialog = start_dialog(&workspace, cx);
    let actions = dialog.read_with(cx, |dialog, _| dialog.actions());
    assert_eq!(
        actions,
        [
            StartAction::ICloud,
            StartAction::GitHub,
            StartAction::SignUp,
            StartAction::Advanced,
            StartAction::Cancel,
        ]
    );
    assert!(is_drawn(cx, "sync-start-icloud"));
    cx.simulate_keystrokes("escape");
    assert!(cx.read(|cx| workspace.read(cx).active_modal::<SyncStart>().is_none()));
}

#[gpui::test]
fn syncing_with_icloud_brings_the_old_gasp_folders_notes_along(cx: &mut TestAppContext) {
    let (dir, vault, _remote) = unsynced_vault(&[("Plan.md", "# Plan\n")]);
    let old = dir.path().join(OLD_ICLOUD_VAULT);
    write(&old, "Old.md", "From before\n");
    let (workspace, cx) = open_workspace(cx, &vault, Arc::default());
    run(&workspace, cx, "sync.set-up");
    let dialog = start_dialog(&workspace, cx);
    click(cx, "sync-start-icloud");
    assert!(matches!(
        start_stage(&dialog, cx),
        Stage::ICloudConfirm { notes_there: 1, .. }
    ));
    click(cx, "sync-start-move");
    cx.run_until_parked();

    let icloud = dir.path().join(ICLOUD_VAULT);
    assert_eq!(
        std::fs::read_to_string(icloud.join("Old.md")).unwrap(),
        "From before\n"
    );
    assert_eq!(
        std::fs::read_to_string(icloud.join("Plan.md")).unwrap(),
        "# Plan\n"
    );
    assert!(
        old.join("Old.md").exists(),
        "the old folder stays as it was"
    );
}

#[gpui::test]
fn a_vault_in_the_old_gasp_folder_still_syncs_with_icloud(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join(OLD_ICLOUD_VAULT);
    write(&vault, "Plan.md", "# Plan\n");
    let (_workspace, cx) = open_workspace(cx, &vault, Arc::default());
    assert!(is_drawn(cx, "icloud-status-button"));
}

#[gpui::test]
fn syncing_with_icloud_copies_the_vault_checks_it_and_opens_it_there(cx: &mut TestAppContext) {
    let (dir, vault, _remote) =
        unsynced_vault(&[("Plan.md", "# Plan\n"), ("Daily/Monday.md", "Ran\n")]);
    let (workspace, cx) = open_workspace(cx, &vault, Arc::default());
    run(&workspace, cx, "sync.set-up");
    let dialog = start_dialog(&workspace, cx);
    click(cx, "sync-start-icloud");
    let icloud = dir.path().join(ICLOUD_VAULT);
    assert!(
        matches!(start_stage(&dialog, cx), Stage::ICloudConfirm { ref folder, notes_there: 0 } if *folder == icloud),
        "a vault with notes shows where they're going first"
    );
    click(cx, "sync-start-move");
    cx.run_until_parked();

    assert_eq!(
        std::fs::read_to_string(icloud.join("Daily/Monday.md")).unwrap(),
        "Ran\n"
    );
    assert_eq!(
        std::fs::read_to_string(vault.join("Plan.md")).unwrap(),
        "# Plan\n",
        "the original stays where it was"
    );
    let shown = cx.update(|window, cx| notices::shown_in(window.window_handle(), cx));
    let (_, notice) = shown.last().expect("a notice says where the notes went");
    assert!(notice.message.contains("The old folder is still at"));
    assert!(
        is_drawn(cx, "icloud-status-button"),
        "the window opened the vault from iCloud"
    );
}

#[gpui::test]
fn a_vault_already_in_icloud_gets_its_settings_rather_than_set_up(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join(ICLOUD_VAULT);
    std::fs::create_dir_all(&vault).unwrap();
    std::fs::write(vault.join("Plan.md"), "# Plan\n").unwrap();
    std::fs::write(vault.join("Plan 2.md"), "# Plan\nfrom the phone\n").unwrap();
    let (workspace, cx) = open_workspace(cx, &vault, Arc::default());
    assert!(is_drawn(cx, "icloud-status-button"));
    run(&workspace, cx, "sync.set-up");
    assert!(cx.read(|cx| workspace.read(cx).active_modal::<SettingsView>().is_some()));
    assert!(is_drawn(cx, "show-icloud-folder"));
    assert!(!is_drawn(cx, "set-up-sync"));
}

#[gpui::test]
fn icloud_copies_open_beside_their_notes_to_compare(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join(ICLOUD_VAULT);
    std::fs::create_dir_all(&vault).unwrap();
    std::fs::write(vault.join("Plan.md"), "# Plan\nBuy milk\n").unwrap();
    std::fs::write(vault.join("Plan 2.md"), "# Plan\nBuy milk\nCall Sam\n").unwrap();
    let (workspace, cx) = open_workspace(cx, &vault, Arc::default());
    click(cx, "icloud-status-button");
    assert!(is_drawn(cx, "icloud-popover"));
    click(cx, "icloud-compare-0");
    let titles: Vec<Vec<String>> = cx.read(|cx| {
        workspace
            .read(cx)
            .panes()
            .iter()
            .map(|pane| {
                pane.read(cx)
                    .tabs()
                    .iter()
                    .map(|tab| tab.title(cx))
                    .collect()
            })
            .collect()
    });
    assert_eq!(titles, [vec!["Plan".to_owned()], vec!["Plan 2".to_owned()]]);
}

#[gpui::test]
fn signing_in_with_github_makes_a_private_repository_and_syncs_with_it(cx: &mut TestAppContext) {
    let (dir, vault, _remote) = unsynced_vault(&[("Plan.md", "# Plan\n")]);
    let store = Arc::new(InMemoryCredentialStore::default());
    let (workspace, cx) = open_workspace(cx, &vault, store.clone());
    run(&workspace, cx, "sync.set-up");
    let dialog = start_dialog(&workspace, cx);
    click(cx, "sync-start-github");
    assert!(
        matches!(start_stage(&dialog, cx), Stage::Code { ref code, .. } if code == "WDJB-MJHT"),
        "{:?}",
        start_stage(&dialog, cx)
    );
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_secs(5));
        cx.run_until_parked();
    }
    let Stage::Picking(picking) = start_stage(&dialog, cx) else {
        panic!("signed in: {:?}", start_stage(&dialog, cx));
    };
    assert_eq!(picking.login, "you");
    assert_eq!(
        picking.new_name, "gasp-notes",
        "the stand-in has a notes already"
    );

    click(cx, "sync-start-make-repository");
    cx.run_until_parked();
    assert!(
        matches!(start_stage(&dialog, cx), Stage::Done(_)),
        "{:?}",
        start_stage(&dialog, cx)
    );
    assert!(vault.join(".git").is_dir());
    let bare = dir.path().join("stand-in-github/gasp-notes.git");
    let remote = format!("file://{}", bare.display());
    assert_eq!(
        store.load(&remote).unwrap(),
        Some(Token::new("stand-in-token"))
    );
    let pushed = git2::Repository::open_bare(&bare).unwrap();
    let tree = pushed
        .find_reference("refs/heads/master")
        .unwrap()
        .peel_to_tree()
        .unwrap();
    assert!(tree.get_path(Path::new("Plan.md")).is_ok());
}
