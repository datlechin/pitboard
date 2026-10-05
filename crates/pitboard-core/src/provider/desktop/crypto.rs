//! Chromium's cookie encryption on macOS, as Claude Desktop uses it: the password in the
//! keychain item `Claude Safe Storage`, stretched by PBKDF2-HMAC-SHA1 into an AES-128 key,
//! and every value AES-128-CBC under a fixed IV behind a `v10` prefix.
//!
//! Only live usage decrypts anything, and only once somebody turned it on. Read from
//! Chromium's `os_crypt_mac.mm` and `cookie_util`, and dated in the register under
//! `desktop_cookie_encryption`.

use aes::Aes128;
use aes::cipher::{BlockDecrypt, KeyInit, generic_array::GenericArray};
use sha2::{Digest, Sha256};
use std::num::NonZeroU32;
use zeroize::Zeroizing;

/// The prefix of every value Chromium encrypts with the keychain's key.
const PREFIX: &[u8] = b"v10";
/// Chromium's fixed salt and iteration count for the key on macOS.
const SALT: &[u8] = b"saltysalt";
const ITERATIONS: NonZeroU32 = NonZeroU32::new(1003).expect("not zero");
/// Sixteen spaces: Chromium's IV, the same for every value.
const IV: [u8; 16] = [b' '; 16];
const BLOCK: usize = 16;
/// The cookie database version from which a value starts with the SHA-256 of its host,
/// binding it to the domain it was set for.
const HOST_HASH_FROM: u32 = 24;

#[derive(Debug, thiserror::Error)]
pub(crate) enum CryptoError {
    #[error("the value is not encrypted the way Pitboard knows")]
    NotV10,
    #[error("the value is not a whole number of blocks")]
    BadLength,
    #[error("the value does not decrypt under this key")]
    BadPadding,
    #[error("the value was not set for the host it is stored under")]
    HostHashMismatch,
    #[error("the value is not text")]
    NotUtf8,
}

/// The AES key Chromium derives from the keychain's password.
pub(crate) fn derive_key(password: &[u8]) -> Zeroizing<[u8; 16]> {
    let mut key = Zeroizing::new([0u8; 16]);
    ring::pbkdf2::derive(
        ring::pbkdf2::PBKDF2_HMAC_SHA1,
        ITERATIONS,
        SALT,
        password,
        key.as_mut(),
    );
    key
}

/// The cleartext of one cookie value stored for `host_key` in a database of
/// `meta_version`.
///
/// A key that is not the one the value was encrypted with almost always ends in padding
/// that does not check out, and from version 24 on, always in a host hash that does not.
/// Either way nothing is returned, so a wrong key is never mistaken for a session.
pub(crate) fn decrypt(
    key: &[u8; 16],
    host_key: &str,
    encrypted: &[u8],
    meta_version: u32,
) -> Result<Zeroizing<String>, CryptoError> {
    let body = encrypted.strip_prefix(PREFIX).ok_or(CryptoError::NotV10)?;
    if body.is_empty() || body.len() % BLOCK != 0 {
        return Err(CryptoError::BadLength);
    }
    let cipher = Aes128::new(GenericArray::from_slice(key));
    let mut plain = Zeroizing::new(body.to_vec());
    let mut previous = IV;
    for block in plain.chunks_exact_mut(BLOCK) {
        let mut cipher_block = [0u8; BLOCK];
        cipher_block.copy_from_slice(block);
        cipher.decrypt_block(GenericArray::from_mut_slice(block));
        for (byte, chained) in block.iter_mut().zip(previous) {
            *byte ^= chained;
        }
        previous = cipher_block;
    }
    let pad = usize::from(*plain.last().ok_or(CryptoError::BadLength)?);
    if pad == 0
        || pad > BLOCK
        || plain[plain.len() - pad..]
            .iter()
            .any(|b| usize::from(*b) != pad)
    {
        return Err(CryptoError::BadPadding);
    }
    let unpadded = plain.len() - pad;
    let mut start = 0;
    if meta_version >= HOST_HASH_FROM {
        let hash = Sha256::digest(host_key.as_bytes());
        if unpadded < hash.len() || plain[..hash.len()] != hash[..] {
            return Err(CryptoError::HostHashMismatch);
        }
        start = hash.len();
    }
    let text = std::str::from_utf8(&plain[start..unpadded]).map_err(|_| CryptoError::NotUtf8)?;
    Ok(Zeroizing::new(text.to_owned()))
}

/// What Chromium would store for `plain` under `key`, for a test that needs a jar a
/// scripted key opens. Checked against `openssl`'s output in the tests below.
#[cfg(test)]
pub(crate) fn encrypt(key: &[u8; 16], host_key: &str, plain: &str) -> Vec<u8> {
    use aes::cipher::BlockEncrypt;
    let cipher = Aes128::new(GenericArray::from_slice(key));
    let mut data = Sha256::digest(host_key.as_bytes()).to_vec();
    data.extend_from_slice(plain.as_bytes());
    let pad = BLOCK - data.len() % BLOCK;
    data.extend(std::iter::repeat_n(pad as u8, pad));
    let mut previous = IV;
    for block in data.chunks_exact_mut(BLOCK) {
        for (byte, chained) in block.iter_mut().zip(previous) {
            *byte ^= chained;
        }
        cipher.encrypt_block(GenericArray::from_mut_slice(block));
        previous.copy_from_slice(block);
    }
    let mut value = PREFIX.to_vec();
    value.extend(data);
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every vector here is made from a password that never was anybody's key:
    //
    //   python3 -c 'import hashlib; print(hashlib.pbkdf2_hmac("sha1",
    //       b"not-a-real-password", b"saltysalt", 1003, 16).hex())'
    //
    // gives KEY below, and the values are that key's encryption of a made-up session:
    //
    //   printf 'pitboard-test-session' | openssl enc -aes-128-cbc -K $KEY \
    //       -iv 20202020202020202020202020202020 | xxd -p
    //
    //   python3 -c 'import hashlib, sys; sys.stdout.buffer.write(
    //       hashlib.sha256(b".claude.ai").digest() + b"pitboard-test-session")' \
    //     | openssl enc -aes-128-cbc -K $KEY -iv 20202020202020202020202020202020 | xxd -p
    const PASSWORD: &[u8] = b"not-a-real-password";
    const KEY: &str = "f8f470d64968b885494be775c53ac65d";
    const PLAIN: &str = "7d815d7ea2c0133cfe85fb74b40dde77e85ca43851690e6d2916a23c3bb20754";
    const HOSTED: &str = "ae0facfb65c309d24eec5b4843e2b42b2d5f87a5827b2da58da8e51fe94e4d40\
                          57d9b1732ab8aa2e5214c6f34b774f0c4a4a115dc7aef16a142779a20e68a609";

    fn v10(hex_value: &str) -> Vec<u8> {
        let mut value = b"v10".to_vec();
        value.extend(hex::decode(hex_value).unwrap());
        value
    }

    #[test]
    fn decrypts_what_chromium_encrypts() {
        let key = derive_key(PASSWORD);
        assert_eq!(hex::encode(*key), KEY);
        let plain = decrypt(&key, ".claude.ai", &v10(PLAIN), 23).unwrap();
        assert_eq!(plain.as_str(), "pitboard-test-session");
        assert!(matches!(
            decrypt(&key, ".claude.ai", &hex::decode(PLAIN).unwrap(), 23),
            Err(CryptoError::NotV10)
        ));
        assert!(matches!(
            decrypt(&key, ".claude.ai", b"v10short", 23),
            Err(CryptoError::BadLength)
        ));
    }

    #[test]
    fn version_24_strips_the_host_hash() {
        let key = derive_key(PASSWORD);
        let plain = decrypt(&key, ".claude.ai", &v10(HOSTED), 24).unwrap();
        assert_eq!(plain.as_str(), "pitboard-test-session");
        // The other tests' jars are made by `encrypt`, which makes what openssl made.
        assert_eq!(
            encrypt(&key, ".claude.ai", "pitboard-test-session"),
            v10(HOSTED)
        );
    }

    #[test]
    fn a_wrong_host_hash_is_refused() {
        let key = derive_key(PASSWORD);
        assert!(matches!(
            decrypt(&key, "claude.ai", &v10(HOSTED), 24),
            Err(CryptoError::HostHashMismatch)
        ));
        // A value written before the hash was added has none to strip.
        assert!(matches!(
            decrypt(&key, ".claude.ai", &v10(PLAIN), 24),
            Err(CryptoError::HostHashMismatch)
        ));
    }

    /// Another machine's key opens nothing: the padding or the host hash gives it away.
    #[test]
    fn another_key_decrypts_nothing() {
        let key = derive_key(b"somebody else's password");
        assert!(matches!(
            decrypt(&key, ".claude.ai", &v10(HOSTED), 24),
            Err(CryptoError::BadPadding | CryptoError::HostHashMismatch)
        ));
    }
}
