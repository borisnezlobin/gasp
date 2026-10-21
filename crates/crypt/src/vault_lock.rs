//! The vault password as `.gasp/encryption.toml` keeps it: a salt, how
//! hard its key is to make, and a check that tells a right password from
//! a wrong one before any note is touched. The password is never stored.
//! The file syncs with the vault, so every device asks for the same one.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};

use crate::CryptError;
use crate::aead::{open_bytes, seal_bytes};
use crate::key::{KdfParams, Key, derive_key, random_salt};

/// The file in the vault's config folder.
pub const LOCK_FILE: &str = "encryption.toml";

const VERSION: u32 = 1;
const CHECK_TEXT: &[u8] = b"gasp vault key";
const CHECK_AAD: &[u8] = b"gasp vault check v1";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct LockFile {
    version: u32,
    salt: String,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    check: String,
}

/// What it takes to turn the vault password back into the vault's key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultLock {
    salt: Vec<u8>,
    params: KdfParams,
    check: Vec<u8>,
}

impl VaultLock {
    /// A lock for a new vault password, and the key it makes.
    pub fn create(password: &str, params: KdfParams) -> Result<(VaultLock, Key), CryptError> {
        let salt = random_salt().to_vec();
        let key = derive_key(password, &salt, params)?;
        let check = seal_bytes(&key, CHECK_TEXT, CHECK_AAD);
        Ok((
            VaultLock {
                salt,
                params,
                check,
            },
            key,
        ))
    }

    /// The vault's key, when `password` is the vault password.
    pub fn unlock(&self, password: &str) -> Result<Key, CryptError> {
        let key = derive_key(password, &self.salt, self.params)?;
        if self.accepts(&key) {
            Ok(key)
        } else {
            Err(CryptError::WrongPassword)
        }
    }

    /// Whether `key` is this vault's key: a key kept for Touch ID stops
    /// working once another device changes the password.
    pub fn accepts(&self, key: &Key) -> bool {
        open_bytes(key, &self.check, CHECK_AAD).is_some_and(|text| text.as_slice() == CHECK_TEXT)
    }

    pub fn to_toml(&self) -> String {
        let file = LockFile {
            version: VERSION,
            salt: BASE64.encode(&self.salt),
            memory_kib: self.params.memory_kib,
            iterations: self.params.iterations,
            parallelism: self.params.parallelism,
            check: BASE64.encode(&self.check),
        };
        toml::to_string(&file).expect("the lock file always serialises")
    }

    pub fn from_toml(text: &str) -> Result<VaultLock, CryptError> {
        let file: LockFile = toml::from_str(text).map_err(|_| CryptError::Corrupt)?;
        if file.version > VERSION {
            return Err(CryptError::NewerVersion);
        }
        Ok(VaultLock {
            salt: BASE64.decode(file.salt).map_err(|_| CryptError::Corrupt)?,
            params: KdfParams {
                memory_kib: file.memory_kib,
                iterations: file.iterations,
                parallelism: file.parallelism,
            },
            check: BASE64.decode(file.check).map_err(|_| CryptError::Corrupt)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_right_password_unlocks_and_a_wrong_one_is_told_apart() {
        let (lock, key) = VaultLock::create("barnacle", KdfParams::QUICK).unwrap();
        assert_eq!(lock.unlock("barnacle").unwrap(), key);
        assert_eq!(lock.unlock("Barnacle"), Err(CryptError::WrongPassword));
    }

    #[test]
    fn the_lock_survives_its_file() {
        let (lock, key) = VaultLock::create("krill", KdfParams::QUICK).unwrap();
        let text = lock.to_toml();
        assert!(!text.contains("krill"));
        let back = VaultLock::from_toml(&text).unwrap();
        assert_eq!(back, lock);
        assert!(back.accepts(&key));
    }

    #[test]
    fn a_key_from_an_old_password_is_refused() {
        let (_, old) = VaultLock::create("old", KdfParams::QUICK).unwrap();
        let (lock, _) = VaultLock::create("new", KdfParams::QUICK).unwrap();
        assert!(!lock.accepts(&old));
    }

    #[test]
    fn a_lock_from_a_newer_gasp_says_so() {
        let (lock, _) = VaultLock::create("x", KdfParams::QUICK).unwrap();
        let newer = lock.to_toml().replace("version = 1", "version = 9");
        assert_eq!(VaultLock::from_toml(&newer), Err(CryptError::NewerVersion));
    }
}
