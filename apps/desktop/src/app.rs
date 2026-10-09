//! Starting the GPUI application: a vault window, or the lone editor the
//! layout benchmark drives.

use std::time::Duration;

use gpui::{App, AppContext, Application, Bounds, WindowBounds, WindowOptions, px, size};

use crate::actions::bind_keys;
use crate::bench::BenchConfig;
use crate::editor::EditorView;
use crate::icons::Assets;
use crate::note::LoadedNote;
use crate::trace;
use crate::workspace::menus::{built_in_available, set_app_menus};
use crate::workspace::prompt::use_in_window_prompts;
use crate::workspace::window::{LaunchTarget, open_target};

const BENCH_WINDOW_SIZE: (f32, f32) = (900., 700.);
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
const WAKE_DELAY: Duration = Duration::from_millis(500);
const BENCH_TIMEOUT: Duration = Duration::from_secs(600);

/// Whether this machine can open a window. Only Linux can lack one.
pub fn has_display() -> bool {
    if !cfg!(any(target_os = "linux", target_os = "freebsd")) {
        return true;
    }
    ["DISPLAY", "WAYLAND_DISPLAY"]
        .iter()
        .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

/// Opens `target` (a vault, or the empty state) and runs until the last
/// window closes or the app quits.
pub fn launch(target: LaunchTarget) {
    let reading = target.start_reading();
    crate::first_frame::hold();
    let application = {
        let _span = trace::span("gpui-platform");
        Application::new().with_assets(Assets)
    };
    application.run(move |cx| {
        trace::mark("gpui-ready");
        {
            let _span = trace::span("bind-keys");
            bind_keys(cx);
            crate::features::bind_view_keys(cx);
        }
        crate::first_frame::defer(set_menus);
        use_in_window_prompts(cx);
        #[cfg(target_os = "macos")]
        crate::look_up::install(cx);
        crate::window_drag::install();
        let _span = trace::span("open-window-total");
        if let Err(error) = open_target(target, reading, cx) {
            eprintln!("could not open a window: {error}");
            std::process::exit(1);
        }
        // Only the font menus and font fallbacks need the full list;
        // it's made off the main thread once the window is up.
        crate::ui::load_installed_fonts(cx);
        crate::telemetry::schedule(cx);
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::update::install(cx);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        trace::mark("launched");
    });
}

/// Builds the menu bar. The first frame doesn't show it, and AppKit takes
/// about 15 ms to build it (it adds the Edit menu's system items, which
/// loads Writing Tools), so launch leaves it until the window is on screen.
fn set_menus(cx: &mut App) {
    let _span = trace::span("app-menus");
    set_app_menus(cx, &built_in_available(&crate::features::WIRED_COMMANDS));
}

/// Opens a lone editor on `note`, runs the layout benchmark and quits.
pub fn launch_bench(note: LoadedNote, bench: BenchConfig) {
    start_watchdog();
    start_x11_wake();
    let hidden = bench.hidden && cfg!(target_os = "macos");
    #[cfg(target_os = "macos")]
    if hidden {
        crate::snapshot::hidden::prepare();
    }
    let application = Application::new().with_assets(Assets);
    #[cfg(target_os = "macos")]
    if hidden {
        crate::snapshot::hidden::keep_app_in_background();
    }
    application.run(move |cx| {
        bind_keys(cx);
        // The app lists the fonts just after its first frame and the
        // theme then settles on installed ones; the bench measures that
        // settled state, not the frame before it.
        let names = cx.text_system().all_font_names();
        crate::ui::set_installed_fonts(names, cx);
        let bounds = Bounds::centered(
            None,
            size(px(BENCH_WINDOW_SIZE.0), px(BENCH_WINDOW_SIZE.1)),
            cx,
        );
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            show: !hidden,
            focus: !hidden,
            ..Default::default()
        };
        let opened = cx.open_window(options, |_, cx| {
            cx.new(|cx| EditorView::new(&note.text, note.image_dirs.clone(), cx))
        });
        let window = match opened {
            Ok(window) => window,
            Err(error) => {
                eprintln!("could not open a window: {error}");
                std::process::exit(1);
            }
        };
        let started = window.update(cx, |view, window, cx| {
            window.set_window_title(gasp_config::APP_NAME);
            window.focus(&view.focus_handle);
            view.set_log_timings(false);
            if !bench.prose {
                let mut prose = gasp_config::settings::ProseSettings::default();
                prose.sentence_length.enabled = false;
                prose.grammar.enabled = false;
                view.apply_prose_settings(&prose, cx);
            }
            view.start_bench(bench, window, cx);
            if !hidden {
                cx.activate(true);
            }
            #[cfg(target_os = "macos")]
            if hidden && let Err(error) = crate::snapshot::hidden::keep_drawing(window, cx) {
                eprintln!("could not draw the hidden window: {error}");
                std::process::exit(1);
            }
        });
        if let Err(error) = started {
            eprintln!("could not start {}: {error}", gasp_config::APP_NAME);
        }
    });
}

/// GPUI's X11 client can miss its window's map event on a bare X server
/// (no window manager, as under xvfb-run): the event is queued while it
/// waits for a reply, so its event loop never wakes to start drawing
/// frames. Moving the pointer over the window from a second connection
/// sends it a fresh event. Only the benchmark does this, since it moves the
/// pointer.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn start_x11_wake() {
    if !has_x11_display() {
        return;
    }
    std::thread::spawn(|| {
        std::thread::sleep(WAKE_DELAY);
        if let Err(error) = crate::x11_wake::move_pointer_to_screen_center() {
            eprintln!("could not wake the X11 event loop: {error}");
        }
    });
}

#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
fn start_x11_wake() {}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn has_x11_display() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_none_or(|value| value.is_empty())
        && std::env::var_os("DISPLAY").is_some_and(|value| !value.is_empty())
}

/// A benchmark that stops getting frames would otherwise hang CI.
fn start_watchdog() {
    std::thread::spawn(|| {
        std::thread::sleep(BENCH_TIMEOUT);
        eprintln!("benchmark timed out after {}s", BENCH_TIMEOUT.as_secs());
        std::process::exit(3);
    });
}
