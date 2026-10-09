//! Putting HTML and its plain text on the Linux clipboard together, which
//! GPUI's clipboard (plain text and images only) can't: mail, office
//! suites and Google Docs paste the HTML, and plain-text fields the text.
//!
//! On X11 the clipboard holds no data of its own: the app that copied
//! answers each paste. So one `arboard` clipboard lives as long as the
//! app, and goes on answering until something else is copied.

use std::sync::{Mutex, OnceLock};

use anyhow::anyhow;

static CLIPBOARD: OnceLock<Mutex<Option<arboard::Clipboard>>> = OnceLock::new();

pub fn copy_html_and_text(html: &str, plain: &str) -> anyhow::Result<()> {
    let slot = CLIPBOARD.get_or_init(|| Mutex::new(None));
    let mut slot = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if slot.is_none() {
        *slot = Some(arboard::Clipboard::new().map_err(|error| anyhow!("{error}"))?);
    }
    let clipboard = slot.as_mut().expect("made above");
    clipboard
        .set_html(html, Some(plain))
        .map_err(|error| anyhow!("{error}"))
}
