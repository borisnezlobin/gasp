//! The migrator's output must load cleanly through the real config loader.

use std::path::PathBuf;

use editor_config::{ConfigLoader, Severity};
use editor_migrate::migrate_obsidian;

fn reference_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../reference/obsidian")
}

#[test]
fn migrated_files_load_without_errors() {
    let vault = tempfile::tempdir().unwrap();
    let migration = migrate_obsidian(&reference_dir()).unwrap();
    migration.write_to(&vault.path().join(".editor")).unwrap();

    let mut loader = ConfigLoader::for_vault(vault.path());
    let errors: Vec<_> = loader
        .load_all()
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");

    let config = loader.config();
    assert_eq!(config.theme.text("font.text"), Some("Charter"));
    assert_eq!(config.theme.text("font.code"), Some("Courier New"));
    assert_eq!(config.theme.text("color.accent"), Some("#000000"));
    assert_eq!(config.settings.appearance.base_font_size, 12);
}
