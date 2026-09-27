//! The cutover from vault-sync and GitSync, which push to `main`, to the
//! app, which syncs `master` and pulls `main` in one way until it retires.

mod common;

use common::{World, author, read, sync, write};
use editor_sync::{MergeOutcome, Vault, VaultConfig};

const NOTE: &str = "notes/plan.md";

/// How vault-sync behaves: it syncs `main` and knows nothing else.
fn old_tool_config() -> VaultConfig {
    VaultConfig {
        branch: "main".to_owned(),
        legacy_branch: None,
        ..VaultConfig::default()
    }
}

fn world_on_main() -> World {
    World::seeded_on(old_tool_config(), &[(NOTE, b"first line\n")])
}

#[test]
fn the_first_clone_starts_master_from_the_tip_of_main() {
    let world = world_on_main();
    let app = world.device("laptop");
    assert_eq!(read(&app, NOTE), "first line\n");
    assert!(world.remote_file("master", NOTE).is_none());

    write(&app, NOTE, b"first line\nfrom the app\n");
    sync(&app, "laptop");
    assert_eq!(
        world.remote_file("master", NOTE).unwrap(),
        b"first line\nfrom the app\n"
    );
    assert_eq!(world.remote_file("main", NOTE).unwrap(), b"first line\n");
}

#[test]
fn pushes_to_main_are_merged_into_master() {
    let world = world_on_main();
    let app = world.device("laptop");
    write(&app, NOTE, b"first line\nfrom the app\n");
    sync(&app, "laptop");

    let old_tool = world.device_with("phone", old_tool_config());
    write(&old_tool, "notes/from-phone.md", b"written by GitSync\n");
    sync(&old_tool, "phone");

    app.fetch().unwrap();
    let outcome = app.merge(&author("laptop")).unwrap();
    assert!(
        matches!(
            outcome,
            MergeOutcome::Merged { .. } | MergeOutcome::FastForward
        ),
        "{outcome:?}"
    );
    assert_eq!(read(&app, "notes/from-phone.md"), "written by GitSync\n");
    app.push().unwrap();
    assert_eq!(
        world.remote_file("master", "notes/from-phone.md").unwrap(),
        b"written by GitSync\n"
    );
    assert!(world.remote_file("main", NOTE).unwrap() == b"first line\n");
}

#[test]
fn a_second_app_device_clones_master_and_still_follows_main() {
    let world = world_on_main();
    let laptop = world.device("laptop");
    write(&laptop, NOTE, b"first line\nfrom the laptop\n");
    sync(&laptop, "laptop");

    let desktop = world.device("desktop");
    assert_eq!(read(&desktop, NOTE), "first line\nfrom the laptop\n");

    let old_tool = world.device_with("phone", old_tool_config());
    write(&old_tool, "later.md", b"late push\n");
    sync(&old_tool, "phone");
    desktop.fetch().unwrap();
    desktop.merge(&author("desktop")).unwrap();
    assert_eq!(read(&desktop, "later.md"), "late push\n");
}

#[test]
fn without_a_legacy_branch_a_missing_branch_is_an_error() {
    let world = world_on_main();
    let config = VaultConfig {
        legacy_branch: None,
        ..VaultConfig::default()
    };
    let cloned = Vault::clone_remote(&world.remote_url, world.path("strict"), config, None);
    assert!(cloned.is_err());
}
