use super::*;

fn with_toolbars(text: &str) -> Config {
    let mut config = Config::defaults();
    config.toolbars = gasp_config::toolbars::build_toolbars("toolbars.toml", Some(text), &[])
        .unwrap()
        .0;
    config
}

fn command_ids(toolbar: PhoneToolbar) -> Vec<String> {
    toolbar
        .entries
        .into_iter()
        .filter_map(|entry| match entry {
            ToolbarEntry::Command { command } => Some(command.id),
            _ => None,
        })
        .collect()
}

#[test]
fn the_keyboard_bar_follows_toolbars_toml() {
    let text = "[toolbar.keyboard]\nstyle = \"labels\"\nitems = [\"keyboard.hide\", \"format.bold\", \"separator\", \"menu:insert\", \"word-count\", \"pane.close\"]\n";
    let bar = keyboard_toolbar(&with_toolbars(text));
    assert_eq!(bar.labels, ToolbarLabels::Labels);
    assert_eq!(
        bar.entries.len(),
        3,
        "no hide button, widget or pane command"
    );
    assert_eq!(bar.entries[1], ToolbarEntry::Separator);
    assert!(
        matches!(&bar.entries[2], ToolbarEntry::Menu { title, commands } if title == "Insert" && !commands.is_empty())
    );
    let off = with_toolbars("[toolbar.keyboard]\nenabled = false\n");
    assert!(!keyboard_toolbar(&off).enabled);
}

#[test]
fn the_keyboard_bar_starts_like_the_obsidian_one_without_undo() {
    let ids = command_ids(keyboard_toolbar(&Config::defaults()));
    assert_eq!(
        ids[..3],
        ["note.import-image", "edit.indent", "edit.outdent"]
    );
    assert!(!ids.iter().any(|id| id == "edit.undo" || id == "edit.redo"));
    assert_eq!(ids.last().map(String::as_str), Some("palette.open"));
}

#[test]
fn the_bottom_bar_is_the_sidebar_the_title_and_the_tabs() {
    let bar = browser_bar(&Config::defaults());
    assert!(bar.enabled);
    assert_eq!(bar.entries.len(), 3);
    assert!(
        matches!(&bar.entries[0], ToolbarEntry::Command { command } if command.id == "sidebar.files.toggle")
    );
    assert_eq!(bar.entries[1], ToolbarEntry::Spacer);
    assert!(
        matches!(&bar.entries[2], ToolbarEntry::Command { command } if command.id == "tab.overview")
    );
}

#[test]
fn the_bottom_bar_keeps_its_title_and_shows_sync() {
    let mine = with_toolbars(
        "[toolbar.browser-bar]\nenabled = false\nitems = [\"sync\", \"word-count\", \"tab.new\"]\n",
    );
    let bar = browser_bar(&mine);
    assert!(bar.enabled, "tabs are reached through it");
    assert!(matches!(&bar.entries[0], ToolbarEntry::Widget { name, .. } if name == "sync"));
    assert_eq!(
        bar.entries.len(),
        3,
        "no word count, and the title at the end"
    );
    assert_eq!(bar.entries.last(), Some(&ToolbarEntry::Spacer));
}

fn empty_vault() -> (tempfile::TempDir, std::sync::Arc<VaultFolder>) {
    let dir = tempfile::tempdir().unwrap();
    let vault = VaultFolder::open(dir.path().to_string_lossy().into_owned()).unwrap();
    (dir, vault)
}

#[test]
fn settings_list_both_bars_and_what_they_can_add() {
    let (_dir, vault) = empty_vault();
    let bars = vault.phone_toolbars();
    let ids: Vec<&str> = bars.iter().map(|bar| bar.id.as_str()).collect();
    assert_eq!(ids, ["keyboard", "browser-bar"]);
    assert!(bars.iter().all(|bar| !bar.changed));
    assert_eq!(bars[1].items[1].title, "Note title");

    let choices = vault.toolbar_choices("keyboard".into());
    assert!(choices.iter().any(|choice| choice.item == "format.bold"));
    assert!(!choices.iter().any(|choice| choice.item == "keyboard.hide"));
    assert!(choices.iter().any(|choice| choice.item == "spacer"));
    let bottom = vault.toolbar_choices("browser-bar".into());
    assert!(bottom.iter().any(|choice| choice.item == "sync"));
    assert!(!bottom.iter().any(|choice| choice.item == "spacer"));
}

#[test]
fn settings_write_the_keyboard_bar_and_reset_it() {
    let (dir, vault) = empty_vault();
    let items = ["edit.indent", "format.italic"].map(String::from).to_vec();
    vault.set_toolbar_items("keyboard".into(), items).unwrap();
    vault
        .set_toolbar_labels("keyboard".into(), ToolbarLabels::IconsAndLabels)
        .unwrap();
    let text = std::fs::read_to_string(toolbar_files::toolbars_path(dir.path())).unwrap();
    assert!(
        text.contains("\"edit.indent\", \"format.italic\""),
        "{text}"
    );
    assert!(text.contains("icons-and-labels"), "{text}");
    let bar = vault.keyboard_toolbar();
    assert_eq!(command_ids(bar.clone()), ["edit.indent", "format.italic"]);
    assert_eq!(bar.labels, ToolbarLabels::IconsAndLabels);
    assert!(vault.phone_toolbars()[0].changed);

    vault.reset_toolbar("keyboard".into()).unwrap();
    assert_eq!(
        vault.keyboard_toolbar(),
        keyboard_toolbar(&Config::defaults())
    );
    let refused = vault.set_toolbar_labels("no such bar!".into(), ToolbarLabels::Icons);
    assert!(refused.is_err());
}
