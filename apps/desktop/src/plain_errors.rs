//! Errors as a person reads them. A notice says what failed in its
//! headline and, underneath, why in plain words with what to do next.
//! The system's own wording, such as "stream did not contain valid
//! UTF-8", goes to the log for debugging and never into a notice.

use std::io::{self, ErrorKind};

/// Said when nothing more specific is known.
pub const TRY_AGAIN: &str = "Try again. If it keeps happening, restart Gasp.";

const OFFLINE: &str = "Gasp couldn’t reach the internet. Check your connection, then try again.";

/// One plain reason for each kind of file or network failure people meet.
const IO_REASONS: &[(ErrorKind, &str)] = &[
    (
        ErrorKind::NotFound,
        "It isn’t there anymore. It may have been moved or deleted.",
    ),
    (
        ErrorKind::PermissionDenied,
        "macOS isn’t letting Gasp use it. Check its permissions in Finder.",
    ),
    (
        ErrorKind::StorageFull,
        "Your disk is full. Free up some space, then try again.",
    ),
    (
        ErrorKind::QuotaExceeded,
        "Your disk is full. Free up some space, then try again.",
    ),
    (
        ErrorKind::ReadOnlyFilesystem,
        "That folder can’t be changed. Move the vault somewhere you can write to.",
    ),
    (
        ErrorKind::AlreadyExists,
        "Something with that name is already there. Try another name.",
    ),
    (ErrorKind::IsADirectory, "That’s a folder, not a file."),
    (
        ErrorKind::DirectoryNotEmpty,
        "That folder still has files in it.",
    ),
    (ErrorKind::FileTooLarge, "It’s too big to open."),
    (ErrorKind::InvalidData, "It isn’t a file Gasp can read."),
    (ErrorKind::UnexpectedEof, "It isn’t a file Gasp can read."),
    (ErrorKind::TimedOut, OFFLINE),
    (ErrorKind::ConnectionRefused, OFFLINE),
    (ErrorKind::ConnectionReset, OFFLINE),
    (ErrorKind::ConnectionAborted, OFFLINE),
    (ErrorKind::NotConnected, OFFLINE),
    (ErrorKind::HostUnreachable, OFFLINE),
    (ErrorKind::NetworkUnreachable, OFFLINE),
    (ErrorKind::NetworkDown, OFFLINE),
];

/// Something that went wrong, with the sentence a notice shows for it.
pub trait PlainReason: std::fmt::Display {
    fn plain_reason(&self) -> &'static str {
        TRY_AGAIN
    }
}

impl PlainReason for io::Error {
    fn plain_reason(&self) -> &'static str {
        IO_REASONS
            .iter()
            .find(|(kind, _)| *kind == self.kind())
            .map_or(TRY_AGAIN, |(_, reason)| reason)
    }
}

impl PlainReason for gasp_sync::SyncError {
    fn plain_reason(&self) -> &'static str {
        match self {
            gasp_sync::SyncError::Io(error) => error.plain_reason(),
            gasp_sync::SyncError::Offline(_) => OFFLINE,
            _ => TRY_AGAIN,
        }
    }
}

impl PlainReason for anyhow::Error {
    fn plain_reason(&self) -> &'static str {
        self.chain()
            .find_map(|cause| cause.downcast_ref::<io::Error>())
            .map_or(TRY_AGAIN, PlainReason::plain_reason)
    }
}

impl PlainReason for String {}

impl PlainReason for &str {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_errors_read_as_what_happened_and_what_to_do() {
        let missing = io::Error::from(ErrorKind::NotFound);
        assert!(missing.plain_reason().contains("moved or deleted"));
        let not_text = io::Error::new(ErrorKind::InvalidData, "stream did not contain valid UTF-8");
        assert_eq!(not_text.plain_reason(), "It isn’t a file Gasp can read.");
        assert_eq!(io::Error::other("anything").plain_reason(), TRY_AGAIN);
        let wrapped = anyhow::Error::from(io::Error::from(ErrorKind::StorageFull));
        assert!(wrapped.plain_reason().contains("disk is full"));
    }
}
