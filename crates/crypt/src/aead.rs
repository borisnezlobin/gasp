//! XChaCha20-Poly1305 with a fresh random nonce each time, stored in front
//! of the ciphertext. Its 24-byte nonce is long enough that random ones
//! never repeat in practice.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

use crate::key::{Key, fill_random};

const NONCE_LEN: usize = 24;

/// `plain` sealed under `key`, bound to `aad`: the nonce, then the
/// ciphertext and its tag.
pub(crate) fn seal_bytes(key: &Key, plain: &[u8], aad: &[u8]) -> Vec<u8> {
    let mut nonce = [0u8; NONCE_LEN];
    fill_random(&mut nonce);
    let cipher = XChaCha20Poly1305::new(key.bytes().into());
    let sealed = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad })
        .expect("sealing never fails for a valid key and nonce");
    let mut out = Vec::with_capacity(NONCE_LEN + sealed.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    out
}

/// What [`seal_bytes`] sealed, or `None` when the key or `aad` is wrong or
/// the bytes were changed.
pub(crate) fn open_bytes(key: &Key, sealed: &[u8], aad: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    let (nonce, ciphertext) = sealed.split_at_checked(NONCE_LEN)?;
    let cipher = XChaCha20Poly1305::new(key.bytes().into());
    cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .ok()
        .map(Zeroizing::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_bytes_open_only_with_their_key_and_aad() {
        let key = Key::random();
        let sealed = seal_bytes(&key, b"whale song", b"one");
        assert_eq!(
            open_bytes(&key, &sealed, b"one").unwrap().as_slice(),
            b"whale song"
        );
        assert!(open_bytes(&key, &sealed, b"two").is_none());
        assert!(open_bytes(&Key::random(), &sealed, b"one").is_none());
        assert!(open_bytes(&key, &sealed[..10], b"one").is_none());
    }

    #[test]
    fn sealing_twice_gives_different_bytes() {
        let key = Key::random();
        assert_ne!(
            seal_bytes(&key, b"same", b""),
            seal_bytes(&key, b"same", b"")
        );
    }
}
