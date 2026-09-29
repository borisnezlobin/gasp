//! Loading a vault's `.gasp/` folder over the built-in defaults.

use std::fs;
use std::path::Path;

use editor_config::layout::SlotContent;
use editor_config::settings::SidebarMode;
use editor_config::{CONFIG_DIR, ConfigLoader, KeyChord, Platform, Severity};
use tempfile::TempDir;

fn write(dir: &Path, name: &str, text: &str) {
    fs::write(dir.join(name), text).unwrap();
}

fn vault() -> (TempDir, ConfigLoader) {
    let vault = TempDir::new().unwrap();
    fs::create_dir(vault.path().join(CONFIG_DIR)).unwrap();
    let loader = ConfigLoader::for_vault(vault.path());
    (vault, loader)
}

#[test]
fn empty_folder_loads_defaults() {
    let (_vault, mut loader) = vault();
    assert!(loader.load_all().is_empty());
    assert_eq!(loader.config(), &editor_config::Config::defaults());
}

#[test]
fn user_files_layer_over_defaults() {
    let (_vault, mut loader) = vault();
    let dir = loader.dir().to_path_buf();
    write(&dir, "settings.toml", "[sidebar.files]\nmode = \"push\"\n");
    write(&dir, "theme.toml", "[color]\nblack = \"#111111\"\n");
    write(
        &dir,
        "layout.toml",
        "[slot.right-sidebar]\ncomponent = \"backlinks\"\nvisible = true\n",
    );
    assert!(loader.load_all().is_empty());
    let config = loader.config();
    assert_eq!(config.settings.sidebar.files.mode, SidebarMode::Push);
    assert_eq!(config.settings.files.attachments_folder, "./images");
    assert_eq!(config.theme.text("color.accent"), Some("#111111"));
    assert_eq!(config.theme.text("font.text"), Some("Charter"));
    let right = config.layout.find("right-sidebar").unwrap();
    assert_eq!(right.content, SlotContent::Component("backlinks".into()));
    assert!(right.visible);
}

#[test]
fn user_rules_add_override_and_delete() {
    let (_vault, mut loader) = vault();
    let rules = r#"
[[rule]]
id   = "key.format.bold"
on   = "key"
keys = "Mod+Shift+B"
do   = "format.bold"

[[rule]]
id     = "key.settings.open.alt"
delete = true

[[rule]]
on   = "key"
keys = "Mod+Shift+L"
do   = "settings.open"
"#;
    write(loader.dir(), "rules.toml", rules);
    assert!(loader.load_all().is_empty());
    let rules = &loader.config().rules;
    let bold = rules.keys_for("format.bold", Platform::Linux);
    assert_eq!(
        bold,
        [KeyChord::parse_for("Ctrl+Shift+B", Platform::Linux).unwrap()]
    );
    let settings: Vec<String> = rules
        .keys_for("settings.open", Platform::Linux)
        .iter()
        .map(|chord| chord.display_for(Platform::Linux))
        .collect();
    assert_eq!(settings, ["Ctrl+,", "Ctrl+Shift+L"]);
}

#[test]
fn deleting_an_unknown_rule_warns() {
    let (_vault, mut loader) = vault();
    write(
        loader.dir(),
        "rules.toml",
        "[[rule]]\nid = \"nope\"\ndelete = true\n",
    );
    let diagnostics = loader.load_all();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].severity, Severity::Warning);
    assert_eq!(
        (diagnostics[0].file.as_str(), diagnostics[0].line),
        ("rules.toml", 2)
    );
}

#[test]
fn a_bad_file_keeps_its_last_good_version_and_spares_the_rest() {
    let (_vault, mut loader) = vault();
    let dir = loader.dir().to_path_buf();
    write(&dir, "settings.toml", "[sidebar.files]\nmode = \"push\"\n");
    write(&dir, "theme.toml", "[color]\nblack = \"#222222\"\n");
    assert!(loader.load_all().is_empty());

    write(
        &dir,
        "settings.toml",
        "[sidebar.files]\nmode = \"push\"\nreveal = \n",
    );
    write(&dir, "theme.toml", "[color]\nblack = \"#333333\"\n");
    let diagnostics = loader.load_all();

    assert_eq!(diagnostics.len(), 1);
    let error = &diagnostics[0];
    assert_eq!(error.file, "settings.toml");
    assert_eq!(error.line, 3);
    assert!(error.column > 1);
    assert_eq!(error.severity, Severity::Error);
    assert_eq!(
        loader.config().settings.sidebar.files.mode,
        SidebarMode::Push
    );
    assert_eq!(loader.config().theme.text("color.accent"), Some("#333333"));
}

#[test]
fn a_theme_cycle_falls_back() {
    let (_vault, mut loader) = vault();
    write(
        loader.dir(),
        "theme.toml",
        "[color]\nblack = \"{color.accent}\"\n",
    );
    let diagnostics = loader.load_all();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 2);
    assert_eq!(loader.config().theme.text("color.accent"), Some("#000000"));
}

#[test]
fn a_bad_rule_rejects_the_whole_rules_file() {
    let (_vault, mut loader) = vault();
    let text = "[[rule]]\non = \"key\"\nkeys = \"Mod+Shift+Y\"\ndo = \"sync.now\"\n\
                [[rule]]\non = \"key\"\nkeys = \"Hyper+Y\"\ndo = \"sync.now\"\n";
    write(loader.dir(), "rules.toml", text);
    let diagnostics = loader.load_all();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 7);
    assert_eq!(loader.config().rules, editor_config::RuleSet::defaults());
}

#[test]
fn device_settings_load_from_their_own_file() {
    let (_vault, mut loader) = vault();
    write(
        loader.dir(),
        "device.toml",
        "open-tabs = [\"a.md\"]\nactive-tab = 0\n[window]\nwidth = 900\n",
    );
    assert!(loader.load_all().is_empty());
    let device = &loader.config().device;
    assert_eq!(device.open_tabs, ["a.md"]);
    assert_eq!(device.window.width, 900);
    assert_eq!(device.window.height, 800);
}
