//! Setting up sync on the phone: reading the repository the person typed,
//! keeping their token in the Keychain and cloning the notes into the
//! app's container.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gasp_config::settings::SyncSettings;
use gasp_sync::phase::plain_git_message;
use gasp_sync::{CredentialStore, InPlaceSetup, SyncError, Token, Vault};

use crate::sync::vault_config;
use crate::vault::{VaultError, VaultFolder};

/// What the setup screen collects.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SyncSetup {
    /// As typed: `you/notes`, `github.com/you/notes`, a full address, or a
    /// `file://` folder.
    pub repository: String,
    /// Empty means `master`.
    pub branch: String,
    /// A GitHub token. A folder on this device needs none.
    pub token: String,
    /// Where the clone goes. It mustn't exist yet.
    pub folder: String,
}

/// The address git clones from, for what the person typed: GitHub
/// shorthand becomes an HTTPS address, anything with a scheme stays.
#[uniffi::export]
pub fn repository_url(typed: String) -> Option<String> {
    gasp_sync::repository_url(&typed)
}

/// Whether the remote signs in with a token: GitHub over HTTPS does, a
/// folder doesn't.
pub(crate) fn takes_token(url: &str) -> bool {
    gasp_sync::url_takes_token(url)
}

/// Clones the notes and keeps the token, so the folder can be opened as
/// a synced vault. A clone that fails leaves nothing behind.
#[uniffi::export]
pub fn set_up_sync(setup: SyncSetup) -> Result<(), VaultError> {
    set_up_with(&setup, &*token_store())
}

pub(crate) fn set_up_with(
    setup: &SyncSetup,
    store: &dyn CredentialStore,
) -> Result<(), VaultError> {
    let url = repository_url(setup.repository.clone())
        .ok_or_else(|| refused("Enter your notes repository, such as github.com/you/notes."))?;
    let token = setup.token.trim();
    if token.is_empty() && takes_token(&url) {
        return Err(refused(
            "Paste a GitHub token that can read and write this repository.",
        ));
    }
    let token = (!token.is_empty()).then(|| Token::new(token));
    let branch = match setup.branch.trim() {
        "" => SyncSettings::default().branch,
        branch => branch.to_owned(),
    };
    let folder = PathBuf::from(&setup.folder);
    if folder.exists() {
        return Err(refused(
            "There's already a folder where the notes would go.",
        ));
    }
    clone_into(&url, &branch, token.clone(), &folder)?;
    let kept = token.map_or(Ok(()), |token| {
        store.save(&url, &token).map_err(|error| {
            refused(&format!(
                "The token couldn't be kept in the Keychain: {error}"
            ))
        })
    });
    if kept.is_err() {
        // Without its token the clone can't sync, and trying again would
        // clone beside it, so it goes.
        let _ = std::fs::remove_dir_all(&folder);
    }
    kept?;
    use_branch_in_settings(&folder, &branch)
}

/// Makes the folder `folder` a clone of the repository where it is,
/// merging the notes it has with the repository's, as the desktop does.
/// It needn't exist: an empty one is made. Used when signing in with
/// GitHub, whose new repositories are empty and so can't be cloned.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct InPlaceSyncSetup {
    /// The repository's HTTPS address.
    pub url: String,
    /// Empty means `master`.
    pub branch: String,
    pub token: String,
    pub folder: String,
    /// Names this device in the first commit.
    pub device: String,
}

/// What setting up in place did, for the finished screen.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct InPlaceSetupSummary {
    pub brought_in: u32,
    pub sent: u32,
    /// Notes that differ between the folder and the repository and wait
    /// for a person.
    pub waiting: u32,
}

#[uniffi::export]
pub fn set_up_sync_in_place(setup: InPlaceSyncSetup) -> Result<InPlaceSetupSummary, VaultError> {
    set_up_in_place_with(&setup, &*token_store())
}

pub(crate) fn set_up_in_place_with(
    setup: &InPlaceSyncSetup,
    store: &dyn CredentialStore,
) -> Result<InPlaceSetupSummary, VaultError> {
    let branch = match setup.branch.trim() {
        "" => SyncSettings::default().branch,
        branch => branch.to_owned(),
    };
    let settings = SyncSettings {
        branch: branch.clone(),
        ..SyncSettings::default()
    };
    let config = vault_config(&settings).map_err(|message| refused(&message))?;
    let folder = PathBuf::from(&setup.folder);
    std::fs::create_dir_all(&folder)?;
    let token = (!setup.token.trim().is_empty()).then(|| Token::new(setup.token.trim()));
    let report = gasp_sync::set_up_in_place(&InPlaceSetup {
        root: &folder,
        url: &setup.url,
        config,
        token: token.clone(),
        author: gasp_sync::sync_author(&setup.device),
        device: &setup.device,
    })
    .map_err(|error| refused(&gasp_sync::setup_problem(&error, &setup.url)))?;
    if let Some(token) = token {
        store.save(&setup.url, &token).map_err(|error| {
            refused(&format!(
                "Your notes are set up to sync, but the token couldn't be kept in the Keychain: {error}"
            ))
        })?;
    }
    use_branch_in_settings(&folder, &branch)?;
    let count = |paths: &[PathBuf]| u32::try_from(paths.len()).unwrap_or(u32::MAX);
    Ok(InPlaceSetupSummary {
        brought_in: count(&report.brought_in),
        sent: count(&report.sent),
        waiting: count(&report.waiting),
    })
}

/// Clones into a folder beside `folder` first, and moves it into place
/// once the clone is whole.
fn clone_into(
    url: &str,
    branch: &str,
    token: Option<Token>,
    folder: &Path,
) -> Result<(), VaultError> {
    let partial = folder.with_extension("cloning");
    let _ = std::fs::remove_dir_all(&partial);
    if let Some(parent) = folder.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let settings = SyncSettings {
        branch: branch.to_owned(),
        ..SyncSettings::default()
    };
    let config = vault_config(&settings).map_err(|message| refused(&message))?;
    let cloned = Vault::clone_remote(url, &partial, config, token).map(drop);
    if let Err(error) = cloned {
        let _ = std::fs::remove_dir_all(&partial);
        return Err(refused(&clone_problem(&error, url, branch)));
    }
    std::fs::rename(&partial, folder)?;
    Ok(())
}

/// Makes the vault's sync settings name the branch that was cloned, so
/// the clone opens as set up.
fn use_branch_in_settings(folder: &Path, branch: &str) -> Result<(), VaultError> {
    let vault = VaultFolder::open(folder.to_string_lossy().into_owned())?;
    if vault.config().settings.sync.branch == branch {
        return Ok(());
    }
    vault.set_setting(
        "sync.branch".to_owned(),
        crate::settings::SettingValue::Text {
            value: branch.to_owned(),
        },
    )
}

fn clone_problem(error: &SyncError, url: &str, branch: &str) -> String {
    match error {
        SyncError::Auth(_) => {
            "GitHub didn't accept this token. Check that it can read and write the repository's contents.".to_owned()
        }
        SyncError::Offline(_) => format!("{url} can't be reached. Check the address and your connection."),
        error if error.is_not_found() => {
            format!("The repository has no branch called {branch}.")
        }
        other => format!("Cloning stopped. Git said: {}", plain_git_message(&other.to_string())),
    }
}

fn refused(message: &str) -> VaultError {
    VaultError::Refused {
        message: message.to_owned(),
    }
}

/// Where tokens are kept: the Keychain on the phone. Anywhere else, which
/// is only the tests and the bindings generator, they stay in memory.
pub(crate) fn token_store() -> Arc<dyn CredentialStore> {
    #[cfg(target_os = "ios")]
    {
        Arc::new(gasp_sync::KeychainStore)
    }
    #[cfg(not(target_os = "ios"))]
    {
        static STORE: std::sync::OnceLock<Arc<gasp_sync::InMemoryCredentialStore>> =
            std::sync::OnceLock::new();
        STORE.get_or_init(Default::default).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_shorthand_becomes_an_https_address() {
        let url = |typed: &str| repository_url(typed.to_owned());
        assert_eq!(
            url("you/notes").as_deref(),
            Some("https://github.com/you/notes")
        );
        assert_eq!(
            url(" github.com/you/notes/ ").as_deref(),
            Some("https://github.com/you/notes")
        );
        assert_eq!(
            url("https://github.com/you/notes.git").as_deref(),
            Some("https://github.com/you/notes.git")
        );
        assert_eq!(
            url("file:///tmp/notes.git").as_deref(),
            Some("file:///tmp/notes.git")
        );
        assert_eq!(url("notes"), None);
        assert_eq!(url("you/my notes"), None);
        assert!(takes_token("https://github.com/you/notes"));
        assert!(!takes_token("file:///tmp/notes.git"));
    }
}
