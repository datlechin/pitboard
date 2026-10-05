//! What Chromium's cookie jar says about who is signed in to Claude Desktop.
//!
//! Only the ciphertext is read, never decrypted here: whose session it is comes from the
//! hash of the ciphertext, and the uuid comes from `config.json`. The jar itself is read by
//! the host, [`crate::host::Host::cookie_table`], without ever needing Claude's key.

pub(crate) use super::types::{CookieRow, CookieTable};
use sha2::{Digest, Sha256};

/// The cookie database's `meta.version` on the build the register names.
pub(crate) const EXPECTED_META: u32 = 24;

/// The prefix of every value Chromium encrypts with the key in the keychain.
const PREFIX: &[u8] = b"v10";

/// Seconds between 1601, Chromium's epoch, and 1970.
const CHROME_EPOCH_OFFSET: i64 = 11_644_473_600;

/// The session a jar holds: a handle on it, never the cookie itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Session {
    /// The SHA-256 of the `sessionKey` cookie's ciphertext, in hex.
    pub fingerprint: String,
    /// When the cookie expires, in epoch seconds. `None` for one that never does.
    pub expires_at: Option<i64>,
}

/// A jar in a form this Pitboard was not written for.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum DesktopFormat {
    #[error("meta version {0}, where {expected} was expected", expected = EXPECTED_META)]
    Meta(u32),
    #[error("the `{0}` cookie is not encrypted the way Pitboard knows")]
    Prefix(String),
}

/// The session `table` holds, if any. A jar of another version, or a value that is not
/// `v10` ciphertext, is refused rather than read as signed out.
pub(crate) fn session(table: &CookieTable) -> Result<Option<Session>, DesktopFormat> {
    if table.meta_version != EXPECTED_META {
        return Err(DesktopFormat::Meta(table.meta_version));
    }
    // Every row read shares the one key, so any of them in another form means all may be.
    if let Some(odd) = table
        .rows
        .iter()
        .find(|row| !row.encrypted_value.starts_with(PREFIX))
    {
        return Err(DesktopFormat::Prefix(odd.name.clone()));
    }
    Ok(cookie(table, "sessionKey").map(|row| Session {
        fingerprint: fingerprint(&row.encrypted_value),
        expires_at: (row.expires_utc != 0).then(|| chrome_to_unix(row.expires_utc)),
    }))
}

/// Whether `sessionKeyV3` holds another session than `sessionKey`, which doctor reports.
/// Where either is missing there is nothing to differ.
// Read by doctor, which says when the two copies of the session disagree.
#[allow(dead_code)]
pub(crate) fn twins_differ(table: &CookieTable) -> bool {
    match (cookie(table, "sessionKey"), cookie(table, "sessionKeyV3")) {
        (Some(plain), Some(v3)) => plain.encrypted_value != v3.encrypted_value,
        _ => false,
    }
}

/// Chromium's time, microseconds since 1601, as epoch seconds.
pub(crate) fn chrome_to_unix(micros: i64) -> i64 {
    micros.div_euclid(1_000_000) - CHROME_EPOCH_OFFSET
}

/// The cookie called `name` for claude.ai, the domain-wide one first, which is the one
/// Claude sets.
pub(crate) fn cookie<'a>(table: &'a CookieTable, name: &str) -> Option<&'a CookieRow> {
    [".claude.ai", "claude.ai"].iter().find_map(|host| {
        table
            .rows
            .iter()
            .find(|row| row.name == name && row.host_key == *host)
    })
}

fn fingerprint(ciphertext: &[u8]) -> String {
    hex::encode(Sha256::digest(ciphertext))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(host_key: &str, name: &str, value: &[u8], expires_utc: i64) -> CookieRow {
        CookieRow {
            host_key: host_key.into(),
            name: name.into(),
            encrypted_value: value.to_vec(),
            expires_utc,
        }
    }

    fn jar(rows: Vec<CookieRow>) -> CookieTable {
        CookieTable {
            meta_version: EXPECTED_META,
            rows,
        }
    }

    /// 2026-10-30T00:00:00Z in Chromium's time.
    const EXPIRES: i64 = (1_793_318_400 + 11_644_473_600) * 1_000_000;

    #[test]
    fn a_signed_out_jar_has_no_session() {
        assert_eq!(session(&jar(Vec::new())), Ok(None));
        let other = jar(vec![row(".claude.ai", "lastActiveOrg", b"v10org", EXPIRES)]);
        assert_eq!(session(&other), Ok(None));
    }

    #[test]
    fn the_fingerprint_is_the_hash_of_the_ciphertext() {
        let ciphertext = b"v10\x01\x02\x03ciphertext";
        let found = session(&jar(vec![
            row(".claude.ai", "lastActiveOrg", b"v10org", EXPIRES),
            row(".claude.ai", "sessionKey", ciphertext, EXPIRES),
            row(".claude.ai", "sessionKeyV3", b"v10another", EXPIRES),
        ]))
        .unwrap()
        .expect("signed in");
        // printf 'v10\x01\x02\x03ciphertext' | shasum -a 256
        use sha2::{Digest, Sha256};
        assert_eq!(found.fingerprint, hex::encode(Sha256::digest(ciphertext)));
        assert_eq!(found.fingerprint.len(), 64);
        assert_eq!(found.expires_at, Some(1_793_318_400));

        // A session cookie with no expiry says so rather than 1601.
        let forever = session(&jar(vec![row("claude.ai", "sessionKey", ciphertext, 0)]))
            .unwrap()
            .unwrap();
        assert_eq!(forever.expires_at, None);
        assert_eq!(forever.fingerprint, found.fingerprint);
    }

    /// The two copies of the session are told apart, so doctor can say when they differ,
    /// and the plain one is the one that names the login.
    #[test]
    fn twins_that_differ_are_noticed() {
        let same = jar(vec![
            row(".claude.ai", "sessionKey", b"v10same", EXPIRES),
            row(".claude.ai", "sessionKeyV3", b"v10same", EXPIRES),
        ]);
        assert!(!twins_differ(&same));
        let apart = jar(vec![
            row(".claude.ai", "sessionKey", b"v10one", EXPIRES),
            row(".claude.ai", "sessionKeyV3", b"v10two", EXPIRES),
        ]);
        assert!(twins_differ(&apart));
        assert!(!twins_differ(&jar(vec![row(
            ".claude.ai",
            "sessionKey",
            b"v10one",
            EXPIRES
        )])));
    }

    #[test]
    fn another_meta_version_is_refused() {
        let mut table = jar(vec![row(".claude.ai", "sessionKey", b"v10x", EXPIRES)]);
        table.meta_version = 25;
        assert_eq!(session(&table), Err(DesktopFormat::Meta(25)));
        table.meta_version = 0;
        assert_eq!(session(&table), Err(DesktopFormat::Meta(0)));
    }

    #[test]
    fn a_value_not_v10_is_refused() {
        for value in [&b"v11abc"[..], b"plain", b""] {
            let table = jar(vec![row(".claude.ai", "sessionKey", value, EXPIRES)]);
            assert_eq!(
                session(&table),
                Err(DesktopFormat::Prefix("sessionKey".into()))
            );
        }
        // Any of the rows read, not only the session, since they share one key.
        let table = jar(vec![
            row(".claude.ai", "sessionKey", b"v10ok", EXPIRES),
            row(".claude.ai", "lastActiveOrg", b"v20org", EXPIRES),
        ]);
        assert_eq!(
            session(&table),
            Err(DesktopFormat::Prefix("lastActiveOrg".into()))
        );
    }

    #[test]
    fn chrome_time_converts() {
        assert_eq!(chrome_to_unix(11_644_473_600 * 1_000_000), 0);
        assert_eq!(chrome_to_unix(EXPIRES), 1_793_318_400);
        assert_eq!(chrome_to_unix(EXPIRES + 999_999), 1_793_318_400);
    }
}
