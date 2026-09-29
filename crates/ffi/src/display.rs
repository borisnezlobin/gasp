//! How a vault's notes are drawn right now: when Markdown symbols show and
//! whether sentences are tinted by length. Every open note of a vault
//! shares one, so cycling the symbols changes them all.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use gasp_config::Settings;
use gasp_config::settings::{SymbolMode, SymbolSettings};
use gasp_prose::segment::Thresholds;

#[derive(Clone, Debug, Default)]
pub(crate) struct DisplayState {
    pub symbols: SymbolSettings,
    /// Where short and long sentences start, while highlighting is on.
    pub sentence_lengths: Option<Thresholds>,
}

impl DisplayState {
    pub fn from_settings(settings: &Settings) -> Self {
        let sentence = &settings.prose.sentence_length;
        Self {
            symbols: settings.markdown.symbols.clone(),
            sentence_lengths: sentence.enabled.then_some(Thresholds {
                short_below: sentence.short_below as usize,
                long_above: sentence.long_above as usize,
            }),
        }
    }
}

/// When Markdown symbols show, for the phone to name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SymbolVisibility {
    AlwaysShown,
    AroundCursor,
    AlwaysHidden,
}

impl From<SymbolMode> for SymbolVisibility {
    fn from(mode: SymbolMode) -> Self {
        match mode {
            SymbolMode::AlwaysShown => SymbolVisibility::AlwaysShown,
            SymbolMode::AroundCursor => SymbolVisibility::AroundCursor,
            SymbolMode::AlwaysHidden => SymbolVisibility::AlwaysHidden,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SharedDisplay(Arc<Mutex<DisplayState>>);

impl SharedDisplay {
    pub fn new(state: DisplayState) -> Self {
        Self(Arc::new(Mutex::new(state)))
    }

    pub fn lock(&self) -> MutexGuard<'_, DisplayState> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
