//! Keys: 32 bytes, made from a password with Argon2id or drawn at random.

use argon2::{Algorithm, Argon2, Params, Version};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::CryptError;

pub const KEY_LEN: usize = 32;
pub const SALT_LEN: usize = 16;

/// A secret key, wiped from memory when dropped.
#[derive(Clone, PartialEq, Eq)]
pub struct Key(Zeroizing<[u8; KEY_LEN]>);

impl Key {
    pub fn random() -> Key {
        let mut bytes = Zeroizing::new([0u8; KEY_LEN]);
        fill_random(bytes.as_mut());
        Key(bytes)
    }

    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Key {
        Key(Zeroizing::new(bytes))
    }

    pub fn bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Key(hidden)")
    }
}

/// How much memory and time turning a password into a key takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl KdfParams {
    /// 64 MiB and three passes: a few tenths of a second on a phone, paid
    /// once per unlock, and costly for anyone guessing passwords.
    pub const STANDARD: KdfParams = KdfParams {
        memory_kib: 64 * 1024,
        iterations: 3,
        parallelism: 1,
    };

    /// The smallest Argon2 allows, for tests.
    pub const QUICK: KdfParams = KdfParams {
        memory_kib: 8,
        iterations: 1,
        parallelism: 1,
    };
}

/// The key `password` and `salt` make.
pub fn derive_key(password: &str, salt: &[u8], params: KdfParams) -> Result<Key, CryptError> {
    let params = Params::new(
        params.memory_kib,
        params.iterations,
        params.parallelism,
        Some(KEY_LEN),
    )
    .map_err(|_| CryptError::Corrupt)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    argon
        .hash_password_into(password.as_bytes(), salt, out.as_mut())
        .map_err(|_| CryptError::Corrupt)?;
    Ok(Key(out))
}

pub fn random_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    fill_random(&mut salt);
    salt
}

pub(crate) fn fill_random(buffer: &mut [u8]) {
    getrandom::getrandom(buffer).expect("the system has a source of random bytes");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_password_and_salt_make_the_same_key() {
        let salt = random_salt();
        let one = derive_key("tide", &salt, KdfParams::QUICK).unwrap();
        let two = derive_key("tide", &salt, KdfParams::QUICK).unwrap();
        assert_eq!(one, two);
        assert_ne!(one, derive_key("Tide", &salt, KdfParams::QUICK).unwrap());
    }

    #[test]
    fn a_key_never_prints_its_bytes() {
        assert_eq!(format!("{:?}", Key::random()), "Key(hidden)");
    }
}
