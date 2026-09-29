//! The link between `gasp mcp` and a running desktop app with the same
//! vault open, for what only the app knows: its tabs, cursor and
//! selection, its commands, and notes with unsaved edits.
//!
//! The app listens on a Unix domain socket (a localhost TCP port on
//! Windows) named after the vault, in a folder only the user can read.
//! Beside it, a file readable only by the user holds a random token the
//! app picks each time it starts; a request without it is dropped. The
//! server opens one connection per request and writes one JSON line;
//! the app answers with one JSON line and closes it. When nothing
//! answers, the app isn't running with this vault open.
//!
//! [`listener`] is the app's half. It blocks in `accept` on its own
//! thread, so it costs nothing while idle, and hands each request to a
//! function the app gives it. [`client`] is the server's half.

pub mod client;
pub mod endpoint;
pub mod listener;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What the server asks the app. Paths are vault-relative.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Open tabs and panes, the active note, the cursor and selection:
    /// an [`EditorState`].
    State,
    /// Runs a command from the palette in the active pane.
    RunCommand { id: String },
    /// Opens a note in the active pane, at a line (1-based) if given.
    OpenNote { path: String, line: Option<usize> },
    /// The note's text as the app has it: a [`Buffer`].
    Buffer { path: String },
    /// Replaces an open note's text as one undoable edit, if the app
    /// still has `old`, then saves it.
    Edit {
        path: String,
        old: String,
        new: String,
    },
    /// Moves a note or file, updating links as a rename in the app does,
    /// in open editors where it can.
    Move { from: String, to: String },
}

/// The app's view of one note.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Buffer {
    /// Whether a tab has the note open.
    pub open: bool,
    /// Whether it has edits not yet saved.
    pub dirty: bool,
    /// The text in the editor, when open.
    pub text: Option<String>,
}

/// One tab.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabState {
    /// Vault-relative; empty for a tab that isn't a note, such as settings.
    pub path: String,
    pub title: String,
    pub dirty: bool,
}

/// One pane of tabs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneState {
    pub tabs: Vec<TabState>,
    /// Index into `tabs`.
    pub active_tab: Option<usize>,
}

/// Where the cursor is in the active note.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursorState {
    /// Byte offset in the note's text.
    pub offset: usize,
    /// 1-based.
    pub line: usize,
    /// 1-based, in characters.
    pub column: usize,
    /// Byte offset of the other end of the selection.
    pub anchor: usize,
    /// The selected text; empty when nothing is selected.
    pub selection: String,
}

/// What the app shows.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorState {
    pub vault: String,
    pub panes: Vec<PaneState>,
    /// Index into `panes`.
    pub active_pane: usize,
    /// The note in the active pane, vault-relative.
    pub active_note: Option<String>,
    pub cursor: Option<CursorState>,
}

/// A request as sent: the token, then the request's fields.
#[derive(Debug, Serialize, Deserialize)]
struct Envelope {
    token: String,
    #[serde(flatten)]
    request: Request,
}

/// The app's answer: `result` on success, else `error` in words.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Reply {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl Reply {
    fn from_result(result: Result<Value, String>) -> Reply {
        match result {
            Ok(value) => Reply {
                result: Some(value),
                error: None,
            },
            Err(message) => Reply {
                result: None,
                error: Some(message),
            },
        }
    }

    fn into_result(self) -> Result<Value, String> {
        match self.error {
            Some(message) => Err(message),
            None => Ok(self.result.unwrap_or(Value::Null)),
        }
    }
}

/// The longest request line read: an edit carries a note's text twice.
const MAX_LINE_BYTES: u64 = 64 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_read_as_flat_json() {
        let envelope = Envelope {
            token: "t".into(),
            request: Request::OpenNote {
                path: "a.md".into(),
                line: Some(3),
            },
        };
        let json = serde_json::to_value(&envelope).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"token": "t", "op": "open_note", "path": "a.md", "line": 3})
        );
        let back: Envelope = serde_json::from_value(json).unwrap();
        assert_eq!(back.request, envelope.request);
    }

    #[test]
    fn replies_carry_a_result_or_an_error() {
        let ok = Reply::from_result(Ok(serde_json::json!(1)));
        assert_eq!(ok.into_result(), Ok(serde_json::json!(1)));
        let failed = Reply::from_result(Err("no".into()));
        assert_eq!(failed.into_result(), Err("no".to_string()));
    }
}
