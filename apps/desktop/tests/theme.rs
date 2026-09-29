//! Light and dark mode: the `appearance.theme` setting, following the
//! system, and every surface repainting when the mode changes.

use gasp_config::CONFIG_DIR;
use std::path::Path;

use gasp_config::RuleSet;
use gasp_desktop::actions::bind_keys;
use gasp_desktop::features;
use gasp_desktop::settings_view::SettingsView;
use gasp_desktop::text_input::TextInput;
use gasp_desktop::ui::{installed_fonts, is_dark, set_installed_fonts, set_system_dark, ui_theme};
use gasp_desktop::workspace::{OpenIn, Workspace};
use gpui::{AppContext as _, Entity, Hsla, SharedString, TestAppContext, VisualTestContext};
use tempfile::TempDir;

fn vault(settings: &str) -> TempDir {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("Note.md"), "# Note\n\nSome text.\n").unwrap();
    write_settings(vault.path(), settings);
    vault
}

fn write_settings(vault: &Path, settings: &str) {
    let path = vault.join(CONFIG_DIR).join("settings.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, settings).unwrap();
}

fn open_workspace<'a>(
    cx: &'a mut TestAppContext,
    vault: &Path,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
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
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace
                .open_path(Path::new("Note.md"), OpenIn::ActiveTab, window, cx)
                .unwrap()
        })
    });
    cx.run_until_parked();
    (workspace, cx)
}

fn is_darkish(color: Hsla) -> bool {
    color.l < 0.2
}

fn note_background(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Hsla {
    cx.update(|_, cx| {
        let editor = workspace.read(cx).active_editor(cx).unwrap();
        editor.read(cx).theme().background
    })
}

fn reload(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) {
    cx.update(|_, cx| workspace.update(cx, |workspace, cx| workspace.reload_config(cx)));
    cx.run_until_parked();
}

#[gpui::test]
fn the_dark_setting_darkens_the_chrome_and_the_note(cx: &mut TestAppContext) {
    let vault = vault("[appearance]\ntheme = \"dark\"\n");
    let (workspace, cx) = open_workspace(cx, vault.path());
    assert!(cx.update(|_, cx| is_dark(cx)));
    let ui = cx.update(|_, cx| ui_theme(cx));
    assert!(is_darkish(ui.app_background));
    assert!(is_darkish(ui.note_background));
    assert!(ui.text.l > 0.8, "text is light on the dark surfaces");
    // Popovers sit a step lighter than the page they float over.
    assert!(ui.menu_background.l > ui.note_background.l);
    assert!(is_darkish(note_background(&workspace, cx)));

    write_settings(vault.path(), "[appearance]\ntheme = \"light\"\n");
    reload(&workspace, cx);
    assert!(!cx.update(|_, cx| is_dark(cx)));
    assert!(!is_darkish(cx.update(|_, cx| ui_theme(cx)).app_background));
    assert!(!is_darkish(note_background(&workspace, cx)));
}

#[gpui::test]
fn matching_the_system_follows_its_appearance(cx: &mut TestAppContext) {
    let vault = vault("");
    let (workspace, cx) = open_workspace(cx, vault.path());
    assert!(!cx.update(|_, cx| is_dark(cx)), "the test window is light");

    cx.update(|_, cx| set_system_dark(true, cx));
    reload(&workspace, cx);
    assert!(cx.update(|_, cx| is_dark(cx)));
    assert!(is_darkish(note_background(&workspace, cx)));

    // An explicit choice wins over the system.
    write_settings(vault.path(), "[appearance]\ntheme = \"light\"\n");
    reload(&workspace, cx);
    assert!(!cx.update(|_, cx| is_dark(cx)));
}

#[gpui::test]
fn open_settings_repaint_in_the_new_mode(cx: &mut TestAppContext) {
    let vault = vault("[appearance]\ntheme = \"light\"\n");
    let (workspace, cx) = open_workspace(cx, vault.path());
    let root = vault.path().to_path_buf();
    let (settings, cx) = {
        let settings = cx.update(|window, cx| {
            cx.new(|cx| SettingsView::with_rules(root, &RuleSet::defaults(), window, cx))
        });
        (settings, cx)
    };
    cx.update(|_, cx| settings.update(cx, |_, cx| cx.notify()));
    cx.run_until_parked();
    assert!(!is_darkish(
        cx.update(|_, cx| settings.read(cx).style().background)
    ));

    write_settings(vault.path(), "[appearance]\ntheme = \"dark\"\n");
    reload(&workspace, cx);
    // The screen restyles itself the next time it draws.
    let drawn = cx.update(|window, cx| {
        settings.update(cx, |view, cx| {
            gpui::Render::render(view, window, cx);
        });
        settings.read(cx).style().background
    });
    assert!(is_darkish(drawn));
}

// ---- Fonts listed after startup ----

fn note_fonts(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> [SharedString; 3] {
    cx.update(|_, cx| {
        let editor = workspace.read(cx).active_editor(cx).unwrap();
        editor.read(cx).theme().font_families()
    })
}

fn hand_over_fonts(names: &[&str], cx: &mut VisualTestContext) {
    let names = names.iter().map(|name| name.to_string()).collect();
    cx.update(|_, cx| set_installed_fonts(names, cx));
    cx.run_until_parked();
}

#[gpui::test]
fn fonts_listed_after_startup_leave_installed_theme_fonts_alone(cx: &mut TestAppContext) {
    let vault = vault("");
    let (workspace, cx) = open_workspace(cx, vault.path());
    // Nothing lists the fonts in tests; the note draws by the names the
    // theme gives until the list is handed over.
    assert!(cx.update(|_, cx| installed_fonts(cx)).is_none());
    let before = note_fonts(&workspace, cx);
    let ui_before = cx.update(|_, cx| ui_theme(cx));
    let mut installed: Vec<&str> = before.iter().map(|family| family.as_ref()).collect();
    installed.extend([
        ui_before.font_family.as_ref(),
        ui_before.keycap.font_family.as_ref(),
    ]);
    installed.extend(["DejaVu Sans", "Liberation Mono"]);
    hand_over_fonts(&installed, cx);
    // Every font the theme names is installed, so nothing changes.
    assert_eq!(note_fonts(&workspace, cx), before);
    let ui_after = cx.update(|_, cx| ui_theme(cx));
    assert_eq!(ui_after.font_family, ui_before.font_family);
    assert_eq!(ui_after.keycap.font_family, ui_before.keycap.font_family);
}

#[gpui::test]
fn fonts_listed_after_startup_replace_missing_theme_fonts(cx: &mut TestAppContext) {
    let vault = vault("");
    std::fs::write(
        vault.path().join(CONFIG_DIR).join("theme.toml"),
        "[font]\ncode = \"Missing Mono\"\n",
    )
    .unwrap();
    let (workspace, cx) = open_workspace(cx, vault.path());
    assert_eq!(note_fonts(&workspace, cx)[2], "Missing Mono");
    // One mono fallback for each platform.
    let fallbacks = ["Menlo", "Consolas", "Liberation Mono"];
    hand_over_fonts(&fallbacks, cx);
    let code = note_fonts(&workspace, cx)[2].clone();
    assert!(fallbacks.contains(&code.as_ref()), "code font {code}");
}

#[gpui::test]
fn text_inputs_follow_the_fonts_listed_after_startup(cx: &mut TestAppContext) {
    let vault = vault("");
    let (_, cx) = open_workspace(cx, vault.path());
    let named = cx.update(|_, cx| ui_theme(cx).font_family);
    let input = cx.update(|window, cx| cx.new(|cx| TextInput::new(window, cx)));
    let family =
        |cx: &mut VisualTestContext| input.read_with(cx, |input, _| input.font_family().clone());
    assert_eq!(family(cx), named);
    // The interface font isn't installed: the theme falls back, and an
    // input drawn earlier follows it on its next draw.
    hand_over_fonts(&["DejaVu Sans", "Liberation Serif", "Liberation Mono"], cx);
    let fallback = cx.update(|_, cx| ui_theme(cx).font_family);
    assert_ne!(fallback, named, "the theme swapped the missing font");
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            let _ = gpui::Render::render(input, window, cx);
        })
    });
    assert_eq!(family(cx), fallback);
}
