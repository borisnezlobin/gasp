//! This iPhone's own SSH key, for a repository it reaches over SSH: where
//! the key is kept, and its public half to add on GitHub.

use crate::vault::VaultError;

/// The public half of this device's SSH key.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DeviceSshKey {
    /// One line, as GitHub's "New SSH key" page takes it.
    pub public: String,
    /// `SHA256:…`, as GitHub lists the key once it's added.
    pub fingerprint: String,
}

/// Says where this device's SSH key is kept, such as a folder in the
/// app's Application Support. Call it once as the app starts.
#[uniffi::export]
pub fn use_ssh_key_folder(folder: String) {
    gasp_sync::ssh::use_key_folder(folder);
}

/// This device's SSH key, made the first time it's asked for. `label`
/// names the device in GitHub's list of keys.
#[uniffi::export]
pub fn this_device_ssh_key(label: String) -> Result<DeviceSshKey, VaultError> {
    let key = gasp_sync::ssh::device_key(&label).map_err(|error| VaultError::Refused {
        message: format!("This iPhone's SSH key couldn't be made. {error}"),
    })?;
    Ok(DeviceSshKey {
        public: key.public,
        fingerprint: key.fingerprint,
    })
}

/// Whether what the person typed is an SSH address, such as
/// `git@github.com:you/notes.git`, which signs in with this device's key
/// instead of a token.
#[uniffi::export]
pub fn repository_uses_ssh(typed: String) -> bool {
    gasp_sync::repository_url(&typed).is_some_and(|url| gasp_sync::url_uses_ssh(&url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_addresses_are_told_apart_from_https_ones() {
        assert!(repository_uses_ssh("git@github.com:you/notes.git".to_owned()));
        assert!(repository_uses_ssh("ssh://git@github.com/you/notes.git".to_owned()));
        assert!(!repository_uses_ssh("github.com/you/notes".to_owned()));
        assert!(!repository_uses_ssh("https://github.com/you/notes.git".to_owned()));
    }
}
