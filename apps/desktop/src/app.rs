//! Starting the GPUI application and opening the editor window.

use std::time::Duration;

use gpui::{AppContext, Application, Bounds, WindowBounds, WindowOptions, px, size};

use crate::actions::bind_keys;
use crate::bench::BenchConfig;
use crate::editor::EditorView;
use crate::note::LoadedNote;

const WINDOW_SIZE: (f32, f32) = (900., 700.);
const WINDOW_TITLE: &str = "Editor";
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

/// Opens the editor on `note` and runs until the window closes. With a
/// bench config, runs the benchmark and quits.
pub fn launch(note: LoadedNote, bench: Option<BenchConfig>) {
    if bench.is_some() {
        start_watchdog();
        start_x11_wake();
    }
    Application::new().run(move |cx| {
        bind_keys(cx);
        let bounds = Bounds::centered(None, size(px(WINDOW_SIZE.0), px(WINDOW_SIZE.1)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
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
            window.set_window_title(WINDOW_TITLE);
            window.focus(&view.focus_handle);
            view.set_log_timings(bench.is_none());
            if let Some(config) = bench {
                view.start_bench(config, window, cx);
            }
            cx.activate(true);
        });
        if let Err(error) = started {
            eprintln!("could not start the editor: {error}");
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
