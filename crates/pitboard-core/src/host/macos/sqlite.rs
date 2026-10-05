//! Chromium's cookie jar, read by the system's own `sqlite3`.
//!
//! Only the rows Pitboard needs are read, and only their ciphertext: whose session it is,
//! [`crate::provider::desktop::cookies`] says from them. The jar is opened as immutable, so
//! reading it never takes a lock a running Claude could be waiting on; a jar with a write
//! still in its journal is refused rather than read around it.

use super::helper;
use crate::provider::desktop::types::{CookieRow, CookieTable};
use std::io;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// The system's own `sqlite3`, which every Mac has, so Pitboard links no SQLite of its own.
pub(super) const SQLITE: &str = "/usr/bin/sqlite3";

/// What Pitboard reads of the jar.
const QUERY: &str = "SELECT key, value FROM meta WHERE key='version'; \
     SELECT host_key, name, hex(encrypted_value) AS v, expires_utc FROM cookies \
     WHERE host_key IN ('.claude.ai','claude.ai') \
     AND name IN ('sessionKey','sessionKeyV3','lastActiveOrg');";

/// How long `sqlite3` has to answer before it is stopped.
const DEADLINE: Duration = Duration::from_secs(5);

/// The jar at `db`. A jar that is not there is `NotFound`, which a caller reads as signed
/// out; anything else that goes wrong is an error, never an empty jar.
///
/// The database is opened as immutable. Read-only is not enough: `mode=ro` still takes a
/// shared lock, and against a holder of an exclusive one, as Chromium is while it runs,
/// that waits and then answers "database is locked". Immutable takes no lock, and so also
/// ignores a journal a write is still in, which would let a half-written jar be read. So
/// the jar is read only while its journal and log are empty, as the register's
/// `desktop_cookie_journal_idle` says they are between writes, and only if neither the
/// journal nor the jar changed while it was read. A jar mid-write is `ResourceBusy`, and
/// asking again a moment later reads it.
pub(super) fn cookie_table(db: &Path) -> io::Result<CookieTable> {
    // Checked first, the jar included: `sqlite3` would make an empty database where there
    // is none.
    let before = quiet(db)?;
    let mut sqlite = Command::new(SQLITE);
    sqlite
        .args(["-readonly", "-json", "-init", "/dev/null"])
        .arg(format!("file:{}?immutable=1", uri_path(db)))
        .arg(QUERY);
    let out = helper::output_within(sqlite, b"", DEADLINE)?;
    if !out.status.success() {
        // What sqlite3 says names the file and the fault, and holds no cookie.
        return Err(io::Error::other(format!(
            "sqlite3 could not read {}: {}",
            db.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    if quiet(db)? != before {
        return Err(io::Error::new(
            io::ErrorKind::ResourceBusy,
            format!("{} was written to while it was read", db.display()),
        ));
    }
    parse(&out.stdout)
}

/// The jar's size and when it was last written, which change with every write that reaches
/// it.
#[derive(Debug, PartialEq, Eq)]
struct Quiet {
    len: u64,
    modified: Option<std::time::SystemTime>,
}

/// The jar at `db` as it stands, refused as `ResourceBusy` while its rollback journal or
/// write-ahead log holds anything, which is a write not yet finished. A jar that is not
/// there is `NotFound`.
fn quiet(db: &Path) -> io::Result<Quiet> {
    for suffix in ["-journal", "-wal"] {
        let mut side = db.as_os_str().to_owned();
        side.push(suffix);
        let side = Path::new(&side);
        match std::fs::symlink_metadata(side) {
            Ok(found) if found.len() > 0 => {
                return Err(io::Error::new(
                    io::ErrorKind::ResourceBusy,
                    format!("{} holds a write not yet finished", side.display()),
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    let jar = std::fs::metadata(db)?;
    Ok(Quiet {
        len: jar.len(),
        modified: jar.modified().ok(),
    })
}

/// `path` for a `file:` URI: every byte but the unreserved ones and `/` escaped, so a `?`
/// or `#` in a folder's name is part of the path and not the start of something else.
fn uri_path(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::new();
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

/// `sqlite3 -json`'s answer to [`QUERY`]: one array per statement that returned rows.
fn parse(stdout: &[u8]) -> io::Result<CookieTable> {
    use serde_json::Value;
    let bad = |what: String| io::Error::new(io::ErrorKind::InvalidData, what);
    let mut table = CookieTable::default();
    let mut saw_meta = false;
    for result in serde_json::Deserializer::from_slice(stdout).into_iter::<Vec<Value>>() {
        let rows = result.map_err(|e| bad(format!("sqlite3 answered something else: {e}")))?;
        for row in rows {
            if let Some(value) = row.get("value") {
                // The version is stored as text, though a number would do as well.
                table.meta_version = match value {
                    Value::String(text) => text.parse().ok(),
                    Value::Number(n) => n.as_u64().and_then(|n| u32::try_from(n).ok()),
                    _ => None,
                }
                .ok_or_else(|| bad("the cookie database's version is not a number".into()))?;
                saw_meta = true;
            } else {
                let text = |key: &str| {
                    row.get(key)
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .ok_or_else(|| bad(format!("a cookie row has no `{key}`")))
                };
                table.rows.push(CookieRow {
                    host_key: text("host_key")?,
                    name: text("name")?,
                    encrypted_value: hex::decode(text("v")?)
                        .map_err(|_| bad("a cookie value is not hex".into()))?,
                    expires_utc: row
                        .get("expires_utc")
                        .and_then(Value::as_i64)
                        .ok_or_else(|| bad("a cookie row has no `expires_utc`".into()))?,
                });
            }
        }
    }
    if !saw_meta {
        return Err(bad("the cookie database says no version".into()));
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Opened as immutable, `sqlite3` ignores a journal, so a jar with a write still in
    /// its journal or log is refused rather than read half-written. An empty journal is
    /// what the running app leaves between writes, and is no write.
    #[test]
    fn a_jar_mid_write_is_refused() {
        let dir = std::env::temp_dir().join(format!(
            "pitboard-cookies-quiet-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("Cookies");

        assert_eq!(
            quiet(&db).unwrap_err().kind(),
            std::io::ErrorKind::NotFound,
            "no jar is not found, which a caller reads as signed out"
        );
        std::fs::write(&db, b"rows").unwrap();
        let alone = quiet(&db).expect("no journal at all");
        std::fs::write(dir.join("Cookies-journal"), b"").unwrap();
        assert_eq!(quiet(&db).expect("an empty journal"), alone);

        std::fs::write(dir.join("Cookies-journal"), b"pages not yet written back").unwrap();
        let busy = quiet(&db).unwrap_err();
        assert_eq!(busy.kind(), std::io::ErrorKind::ResourceBusy, "{busy}");
        std::fs::write(dir.join("Cookies-journal"), b"").unwrap();
        std::fs::write(dir.join("Cookies-wal"), b"frames").unwrap();
        assert_eq!(
            quiet(&db).unwrap_err().kind(),
            std::io::ErrorKind::ResourceBusy
        );
        std::fs::remove_file(dir.join("Cookies-wal")).unwrap();

        // A jar that changed while it was read is told apart from one that did not.
        std::fs::write(&db, b"more rows").unwrap();
        assert_ne!(quiet(&db).unwrap(), alone);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
