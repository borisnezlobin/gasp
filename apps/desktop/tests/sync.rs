//! Sync in the app, end to end: a bare repository in a temp dir stands in
//! for GitHub and two clones stand in for two devices. One clone is the
//! vault the window has open; the other is driven straight through the
//! engine. Nothing here touches a real repository or the network.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use editor_config::settings::SyncSettings;
use editor_desktop::actions::bind_keys;
use editor_desktop::features;
use editor_desktop::settings_view::{ControlRow, SettingsView};
use editor_desktop::sync::{
    ConflictResolver, SetupProblem, SyncPhase, SyncService, set_credential_store,
};
use editor_desktop::workspace::{OpenIn, Workspace};
use editor_sync::{
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
    cx.update(|cx| {
        bind_keys(cx);
        features::bind_view_keys(cx);
        set_credential_store(store, cx);
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
    // Sync now does nothing rather than open something unasked.
    run(&workspace, cx, "sync.now");
    assert!(cx.read(|cx| workspace.read(cx).active_modal::<SettingsView>().is_none()));
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
    let text = std::fs::read_to_string(laptop.join(".editor/settings.toml")).unwrap();
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
    assert_eq!(now.len(), 3);
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
