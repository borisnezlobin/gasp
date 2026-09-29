//! Handing a finished PDF to the system to print. On macOS that's the
//! standard print panel, as a sheet on the window; elsewhere the PDF is
//! written to the temporary folder and opened in the system viewer, whose
//! Print command takes it from there.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{App, Task, Window};

/// Shows the system's way of printing `pdf`, titled `title` (the note's
/// file name). Resolves once the panel is up, or the file is open.
pub fn send_to_printer(
    pdf: Arc<Vec<u8>>,
    title: String,
    print_path: PathBuf,
    window: &mut Window,
    cx: &mut App,
) -> Task<anyhow::Result<()>> {
    platform::send_to_printer(pdf, title, print_path, window, cx)
}

#[cfg(target_os = "macos")]
mod platform {
    use std::path::PathBuf;
    use std::sync::Arc;

    use anyhow::Context as _;
    use gpui::{App, Task, Window};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    pub fn send_to_printer(
        pdf: Arc<Vec<u8>>,
        title: String,
        _: PathBuf,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<()>> {
        if crate::sandbox::blocks("the print panel") {
            return Task::ready(Ok(()));
        }
        // `Window::window_handle` is GPUI's own handle; this is the
        // platform's.
        let view = HasWindowHandle::window_handle(window)
            .ok()
            .and_then(|handle| match handle.as_raw() {
                RawWindowHandle::AppKit(appkit) => Some(appkit.ns_view.as_ptr()),
                _ => None,
            });
        // The panel runs outside of this update, as GPUI's own sheets do,
        // so AppKit calling back into the app finds it free.
        cx.spawn(async move |_| {
            let view = view.context("The window isn’t ready to print from")?;
            super::super::macos::show_print_panel(view, &pdf, &title)
        })
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use std::path::PathBuf;
    use std::sync::Arc;

    use anyhow::Context as _;
    use gpui::{App, AppContext, Task, Window};

    pub fn send_to_printer(
        pdf: Arc<Vec<u8>>,
        _: String,
        print_path: PathBuf,
        _: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<()>> {
        let destination = print_path.clone();
        let writing = cx.background_spawn(async move {
            std::fs::write(&destination, pdf.as_slice())
                .with_context(|| format!("Couldn’t write {}", destination.display()))
        });
        cx.spawn(async move |cx| {
            writing.await?;
            cx.update(|cx| crate::sandbox::open_with_system(&print_path, cx))?;
            Ok(())
        })
    }
}
