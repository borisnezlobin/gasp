//! Making, renaming and removing notes, daily notes, templates, attached
//! images and following links: the same operations the desktop runs.

use std::path::{Path, PathBuf};

use gasp_vault::attachments::{attachments_dir, embed, save_attachment};
use gasp_vault::entries::EntryKind;
use gasp_vault::knowledge::daily::ensure_daily_note;
use gasp_vault::knowledge::templates::{TemplateContext, fill, list_templates, templates_folder};
use gasp_vault::knowledge::{dates, links, note_title};
use gasp_vault::ops::{self, slash_path, unique_name, validate_name};

use crate::vault::{VaultError, VaultFolder};

/// The name new notes start from.
const UNTITLED: &str = "Untitled";

/// Where a followed link goes.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LinkDestination {
    /// A web address, for the browser.
    Web { url: String },
    /// A note, made empty if it didn't exist, and the heading to show.
    Note {
        path: String,
        heading: Option<String>,
    },
    /// A heading in the note the link is in.
    Heading { heading: String },
}

impl From<ops::NameError> for VaultError {
    fn from(error: ops::NameError) -> Self {
        VaultError::Refused {
            message: error.to_string(),
        }
    }
}

#[uniffi::export]
impl VaultFolder {
    /// Makes an empty note in `folder` called `title`, or "Untitled" with
    /// the next free number, and returns its path.
    pub fn create_note(&self, folder: String, title: Option<String>) -> Result<String, VaultError> {
        let folder_path = self.folder_path(&folder)?;
        let base = title.unwrap_or_else(|| UNTITLED.to_owned());
        validate_name(&base)?;
        let relative_folder = folder_path
            .strip_prefix(&self.root)
            .unwrap_or(Path::new(""));
        let name = unique_name(&self.root, relative_folder, &base, EntryKind::Note);
        let created = ops::create(&self.root, relative_folder, &name, EntryKind::Note)?;
        self.forget_index();
        Ok(slash_path(&created))
    }

    /// Gives a note a new title in the same folder, updating links to it
    /// when `files.update-links-on-rename` is on. Returns its new path.
    pub fn rename_note(&self, path: String, title: String) -> Result<String, VaultError> {
        self.note_path(&path)?;
        validate_name(&title)?;
        let from = PathBuf::from(&path);
        let to = from.with_file_name(format!("{}.md", title.trim()));
        let update_links = self.config().settings.files.update_links_on_rename;
        let renamed = ops::rename(&self.root, &from, &to, update_links)?;
        if let Some(store) = self.snapshot_store() {
            store.moved(&renamed.from, &renamed.to).ok();
        }
        self.forget_index();
        Ok(slash_path(&renamed.to))
    }

    /// Moves a note into `folder` (vault-relative; empty is the root),
    /// keeping its name and updating links to it when
    /// `files.update-links-on-rename` is on. Returns its new path.
    pub fn move_note(&self, path: String, folder: String) -> Result<String, VaultError> {
        self.note_path(&path)?;
        self.move_entry(&path, &folder)
    }

    /// Makes a folder called `name` in `parent` and returns its path.
    pub fn create_folder(&self, parent: String, name: String) -> Result<String, VaultError> {
        let parent_path = self.folder_path(&parent)?;
        let relative = parent_path
            .strip_prefix(&self.root)
            .unwrap_or(Path::new(""));
        let created = ops::create(&self.root, relative, &name, EntryKind::Folder)?;
        self.forget_index();
        Ok(slash_path(&created))
    }

    /// Gives a folder a new name in the same place, updating links to the
    /// notes in it. Returns its new path.
    pub fn rename_folder(&self, folder: String, name: String) -> Result<String, VaultError> {
        self.existing_folder(&folder)?;
        validate_name(&name)?;
        let from = PathBuf::from(&folder);
        let to = from.with_file_name(name.trim());
        self.moved(&from, &to)
    }

    /// Moves a folder, with everything in it, into `into` (empty is the
    /// root). Returns its new path.
    pub fn move_folder(&self, folder: String, into: String) -> Result<String, VaultError> {
        self.existing_folder(&folder)?;
        self.move_entry(&folder, &into)
    }

    /// Moves a folder and everything in it to the trash, as notes go.
    pub fn trash_folder(&self, folder: String) -> Result<(), VaultError> {
        self.existing_folder(&folder)?;
        let mode = self.config().settings.files.trash;
        ops::trash(&self.root, Path::new(&folder), mode)?;
        self.forget_index();
        Ok(())
    }

    /// Moves a note to the trash that `files.trash` names; on the phone the
    /// system trash is the vault's own `.trash` folder.
    pub fn trash_note(&self, path: String) -> Result<(), VaultError> {
        self.note_path(&path)?;
        let mode = self.config().settings.files.trash;
        ops::trash(&self.root, Path::new(&path), mode)?;
        self.forget_index();
        Ok(())
    }

    /// Today's daily note, made from its template the first time.
    pub fn daily_note(&self) -> Result<String, VaultError> {
        let settings = self.config().settings.clone();
        let (path, created) = ensure_daily_note(
            &self.root,
            &settings.daily_notes,
            &settings.templates,
            dates::now(),
        )?;
        if created {
            self.forget_index();
        }
        Ok(self.relative(&path))
    }

    /// The templates in the templates folder, without `.md`.
    pub fn templates(&self) -> Vec<String> {
        let folder = self.config().settings.templates.folder.clone();
        list_templates(&templates_folder(&self.root, &folder))
    }

    /// The template's text with `{{title}}`, `{{date}}` and `{{time}}`
    /// filled in for the note at `note_path`.
    pub fn template_text(&self, name: String, note_path: String) -> Result<String, VaultError> {
        let settings = self.config().settings.templates.clone();
        let file = templates_folder(&self.root, &settings.folder).join(format!("{name}.md"));
        let template = std::fs::read_to_string(file)?;
        let context = TemplateContext {
            title: note_title(Path::new(&note_path)),
            now: dates::now(),
            date_format: settings.date_format,
            time_format: settings.time_format,
        };
        Ok(fill(&template, &context))
    }

    /// Saves an image for the note in its attachments folder and returns
    /// the embed that shows it.
    pub fn save_attachment(
        &self,
        note_path: String,
        bytes: Vec<u8>,
        extension: String,
    ) -> Result<String, VaultError> {
        let note = self.note_path(&note_path)?;
        let folder = self.config().settings.files.attachments_folder.clone();
        let dir = attachments_dir(&note, &folder);
        let name = save_attachment(&dir, &note_title(&note), &extension, &bytes)?;
        Ok(embed(&name))
    }

    /// Where the link `target` in the note at `from_path` goes. A note
    /// that doesn't exist yet is made empty, as the desktop does.
    pub fn resolve_link(
        &self,
        from_path: String,
        target: String,
    ) -> Result<LinkDestination, VaultError> {
        if links::WEB_SCHEMES
            .iter()
            .any(|scheme| target.starts_with(scheme))
        {
            return Ok(LinkDestination::Web { url: target });
        }
        let (note, heading) = links::split_target(&target);
        let heading = heading.map(str::to_owned);
        if note.is_empty() {
            return Ok(heading.map_or(
                LinkDestination::Heading {
                    heading: String::new(),
                },
                |heading| LinkDestination::Heading { heading },
            ));
        }
        let from = self.root.join(&from_path);
        let path = links::resolve_note(&self.root, Some(&from), note)
            .unwrap_or_else(|| self.root.join(format!("{note}.md")));
        let relative = self.relative(&path);
        self.note_path(&relative)?;
        if !path.exists() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, "")?;
            self.forget_index();
        }
        Ok(LinkDestination::Note {
            path: relative,
            heading,
        })
    }
}

impl VaultFolder {
    pub(crate) fn relative(&self, path: &Path) -> String {
        slash_path(path.strip_prefix(&self.root).unwrap_or(path))
    }
}

impl VaultFolder {
    /// A folder that exists in the vault, other than the root.
    fn existing_folder(&self, folder: &str) -> Result<PathBuf, VaultError> {
        let path = self.folder_path(folder)?;
        if folder.is_empty() || !path.is_dir() {
            return Err(VaultError::Refused {
                message: format!("{folder} isn't a folder in this vault"),
            });
        }
        Ok(path)
    }

    /// Moves `path` into `folder`, keeping its name.
    fn move_entry(&self, path: &str, folder: &str) -> Result<String, VaultError> {
        self.folder_path(folder)?;
        let from = PathBuf::from(path);
        let name = from.file_name().ok_or_else(|| VaultError::Refused {
            message: format!("{path} has no name to move"),
        })?;
        self.moved(&from, &Path::new(folder).join(name))
    }

    /// Renames or moves `from` to `to`, carrying snapshots and links along.
    fn moved(&self, from: &Path, to: &Path) -> Result<String, VaultError> {
        let update_links = self.config().settings.files.update_links_on_rename;
        let renamed = ops::rename(&self.root, from, to, update_links)?;
        if let Some(store) = self.snapshot_store() {
            store.moved(&renamed.from, &renamed.to).ok();
        }
        self.forget_index();
        Ok(slash_path(&renamed.to))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::tests::vault_with;

    #[test]
    fn new_notes_take_the_next_free_untitled_name() {
        let (_dir, vault) = vault_with(&[("Notes/Untitled.md", "")]);
        assert_eq!(
            vault.create_note("Notes".into(), None).unwrap(),
            "Notes/Untitled 2.md"
        );
        assert!(vault.create_note("../out".into(), None).is_err());
    }

    #[test]
    fn renaming_updates_links_to_the_note() {
        let (_dir, vault) = vault_with(&[("Waves.md", "x"), ("Index.md", "see [[Waves]]")]);
        let renamed = vault
            .rename_note("Waves.md".into(), "Tides".into())
            .unwrap();
        assert_eq!(renamed, "Tides.md");
        assert_eq!(vault.read_note("Index.md".into()).unwrap(), "see [[Tides]]");
    }

    #[test]
    fn notes_move_between_folders_and_links_follow() {
        let (dir, vault) = vault_with(&[("Waves.md", "x"), ("Index.md", "see [[Waves]]")]);
        vault
            .create_folder(String::new(), "Physics".into())
            .unwrap();
        let moved = vault
            .move_note("Waves.md".into(), "Physics".into())
            .unwrap();
        assert_eq!(moved, "Physics/Waves.md");
        assert!(dir.path().join("Physics/Waves.md").is_file());
        assert_eq!(
            vault.move_note(moved, String::new()).unwrap(),
            "Waves.md",
            "back to the root"
        );
        assert!(vault.move_note("Waves.md".into(), "../out".into()).is_err());
        assert!(
            vault
                .move_note("Missing.md".into(), "Physics".into())
                .is_err()
        );
    }

    #[test]
    fn folders_are_made_renamed_moved_and_trashed() {
        let (dir, vault) = vault_with(&[("Old/Waves.md", "x"), ("Index.md", "see [[Old/Waves]]")]);
        let renamed = vault.rename_folder("Old".into(), "Physics".into()).unwrap();
        assert_eq!(renamed, "Physics");
        assert_eq!(
            vault.read_note("Index.md".into()).unwrap(),
            "see [[Physics/Waves]]"
        );
        vault.create_folder(String::new(), "School".into()).unwrap();
        assert_eq!(
            vault.move_folder(renamed, "School".into()).unwrap(),
            "School/Physics"
        );
        assert!(
            vault
                .move_folder("School".into(), "School/Physics".into())
                .is_err(),
            "not into itself"
        );
        assert!(vault.rename_folder(String::new(), "Root".into()).is_err());
        vault.trash_folder("School".into()).unwrap();
        assert!(!dir.path().join("School").exists());
    }

    #[test]
    fn links_go_to_notes_headings_and_the_web() {
        let (_dir, vault) = vault_with(&[("Deep/Target.md", ""), ("Here.md", "")]);
        let resolve = |target: &str| vault.resolve_link("Here.md".into(), target.into()).unwrap();
        assert_eq!(
            resolve("target#Part"),
            LinkDestination::Note {
                path: "Deep/Target.md".into(),
                heading: Some("Part".into())
            }
        );
        assert_eq!(
            resolve("https://a.org"),
            LinkDestination::Web {
                url: "https://a.org".into()
            }
        );
        assert_eq!(
            resolve("#Part"),
            LinkDestination::Heading {
                heading: "Part".into()
            }
        );
        assert!(matches!(resolve("New note"), LinkDestination::Note { .. }));
        assert!(vault.read_note("New note.md".into()).is_ok());
    }

    #[test]
    fn attachments_land_beside_the_note() {
        let (dir, vault) = vault_with(&[("Maths/Lemma.md", "")]);
        let embed = vault
            .save_attachment("Maths/Lemma.md".into(), vec![1, 2, 3], "png".into())
            .unwrap();
        assert_eq!(embed, "![[Lemma-1.png]]");
        assert!(dir.path().join("Maths/images/Lemma-1.png").is_file());
    }

    #[test]
    fn templates_fill_in_the_note_title() {
        let (_dir, vault) = vault_with(&[("Templates/Meeting.md", "# {{title}}\n")]);
        assert_eq!(vault.templates(), ["Meeting"]);
        let text = vault
            .template_text("Meeting".into(), "Notes/Standup.md".into())
            .unwrap();
        assert_eq!(text, "# Standup\n");
    }
}
