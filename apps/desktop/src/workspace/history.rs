//! Back and forward through the places a pane has been: note opens and
//! big cursor jumps.

use std::path::{Path, PathBuf};

/// Places kept in each direction.
const MAX_ENTRIES: usize = 100;

/// A cursor jump of at least this many lines counts as a new place.
pub const BIG_JUMP_LINES: usize = 20;

/// A note and a cursor offset in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub path: PathBuf,
    pub offset: usize,
}

impl Location {
    pub fn new(path: impl Into<PathBuf>, offset: usize) -> Location {
        Location {
            path: path.into(),
            offset,
        }
    }
}

/// One pane's navigation history.
#[derive(Clone, Debug, Default)]
pub struct NavHistory {
    back: Vec<Location>,
    forward: Vec<Location>,
}

impl NavHistory {
    /// Records leaving `from` for somewhere new, which drops the forward
    /// list the way a browser does.
    pub fn push(&mut self, from: Location) {
        if self.back.last() == Some(&from) {
            self.forward.clear();
            return;
        }
        self.back.push(from);
        if self.back.len() > MAX_ENTRIES {
            self.back.remove(0);
        }
        self.forward.clear();
    }

    /// Steps back from `current`, returning where to go.
    pub fn back(&mut self, current: Option<Location>) -> Option<Location> {
        let target = self.back.pop()?;
        if let Some(current) = current {
            self.forward.push(current);
        }
        Some(target)
    }

    /// Steps forward from `current`, returning where to go.
    pub fn forward(&mut self, current: Option<Location>) -> Option<Location> {
        let target = self.forward.pop()?;
        if let Some(current) = current {
            self.back.push(current);
        }
        Some(target)
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// Points entries for a renamed note at its new path.
    pub fn rename(&mut self, from: &Path, to: &Path) {
        for location in self.back.iter_mut().chain(self.forward.iter_mut()) {
            if location.path == from {
                location.path = to.to_path_buf();
            }
        }
    }

    /// Drops entries for a note that no longer exists.
    pub fn forget(&mut self, path: &Path) {
        self.back.retain(|location| location.path != path);
        self.forward.retain(|location| location.path != path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(name: &str, offset: usize) -> Location {
        Location::new(name, offset)
    }

    #[test]
    fn back_and_forward_retrace_steps() {
        let mut history = NavHistory::default();
        history.push(at("a", 0));
        history.push(at("b", 5));
        assert_eq!(history.back(Some(at("c", 1))), Some(at("b", 5)));
        assert_eq!(history.back(Some(at("b", 5))), Some(at("a", 0)));
        assert_eq!(history.back(Some(at("a", 0))), None);
        assert_eq!(history.forward(Some(at("a", 0))), Some(at("b", 5)));
        assert_eq!(history.forward(Some(at("b", 5))), Some(at("c", 1)));
        assert!(!history.can_go_forward());
    }

    #[test]
    fn a_new_place_clears_forward() {
        let mut history = NavHistory::default();
        history.push(at("a", 0));
        history.back(Some(at("b", 0)));
        assert!(history.can_go_forward());
        history.push(at("a", 0));
        assert!(!history.can_go_forward());
    }

    #[test]
    fn renames_and_deletes_update_entries() {
        let mut history = NavHistory::default();
        history.push(at("a", 0));
        history.push(at("b", 0));
        history.rename(Path::new("a"), Path::new("z"));
        history.forget(Path::new("b"));
        assert_eq!(history.back(None), Some(at("z", 0)));
    }
}
