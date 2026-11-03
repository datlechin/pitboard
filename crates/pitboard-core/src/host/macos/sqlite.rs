use crate::host::{Cookie, Jar, JarError};
use rusqlite::{Connection, ErrorCode, OpenFlags, types::Value};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(super) fn cookies(jar: &Path, hosts: &[&str], names: &[&str]) -> Result<Jar, JarError> {
    // Asked first: an open would make a database where there is none.
    match std::fs::symlink_metadata(jar) {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(JarError::Absent),
        Err(e) => return Err(JarError::Unreadable(e.to_string())),
    }
    match std::fs::symlink_metadata(beside(jar, "-journal")) {
        Ok(journal) if journal.len() > 0 => {
            return Err(JarError::Unsettled(
                "its rollback journal holds a write".into(),
            ));
        }
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(JarError::Unreadable(e.to_string())),
    }
    match std::fs::symlink_metadata(beside(jar, "-wal")) {
        Ok(_) => return Err(JarError::Unsettled("it has a write-ahead log".into())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(JarError::Unreadable(e.to_string())),
    }
    let connection = Connection::open_with_flags(
        jar,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| refused(e, At::Open))?;
    connection
        .busy_timeout(Duration::ZERO)
        .map_err(|e| refused(e, At::Read))?;
    let (version, compatible) = meta(&connection)?;
    let cookies = rows(&connection, hosts, names)?;
    Ok(Jar {
        version,
        compatible,
        cookies,
    })
}

fn beside(jar: &Path, suffix: &str) -> PathBuf {
    let mut name = jar.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn meta(connection: &Connection) -> Result<(i64, i64), JarError> {
    let mut statement = connection
        .prepare("SELECT key, value FROM meta")
        .map_err(|e| refused(e, At::Prepare))?;
    let pairs = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Value>(1)?))
        })
        .map_err(|e| refused(e, At::Read))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| refused(e, At::Read))?;
    let number = |key: &str| {
        pairs
            .iter()
            .find(|(name, _)| name == key)
            .and_then(|(_, value)| match value {
                Value::Integer(n) => Some(*n),
                Value::Text(text) => text.parse().ok(),
                _ => None,
            })
            .ok_or_else(|| JarError::Unknown(format!("its meta table has no number `{key}`")))
    };
    Ok((number("version")?, number("last_compatible_version")?))
}

fn rows(connection: &Connection, hosts: &[&str], names: &[&str]) -> Result<Vec<Cookie>, JarError> {
    let list = |n: usize| vec!["?"; n].join(", ");
    let query = format!(
        "SELECT host_key, name, path, encrypted_value, expires_utc FROM cookies \
         WHERE host_key IN ({}) OR name IN ({})",
        list(hosts.len()),
        list(names.len())
    );
    let mut statement = connection
        .prepare(&query)
        .map_err(|e| refused(e, At::Prepare))?;
    statement
        .query_map(
            rusqlite::params_from_iter(hosts.iter().chain(names)),
            |row| {
                Ok(Cookie {
                    host: row.get(0)?,
                    name: row.get(1)?,
                    path: row.get(2)?,
                    stored: row.get(3)?,
                    expires_utc: row.get(4)?,
                })
            },
        )
        .map_err(|e| refused(e, At::Read))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| refused(e, At::Read))
}

// Which call SQLite refused: the same code means another thing at each.
#[derive(Clone, Copy, PartialEq, Eq)]
enum At {
    Open,
    Prepare,
    Read,
}

fn refused(error: rusqlite::Error, at: At) -> JarError {
    let detail = error.to_string();
    let Some(code) = error.sqlite_error_code() else {
        return match error {
            rusqlite::Error::InvalidColumnType(..)
            | rusqlite::Error::FromSqlConversionFailure(..) => JarError::Unknown(detail),
            _ => JarError::Unreadable(detail),
        };
    };
    let extended = error.sqlite_error().map(|e| e.extended_code);
    let rollback = extended == Some(rusqlite::ffi::SQLITE_READONLY_ROLLBACK);
    let generic = extended.is_some_and(|code| code & 0xff == rusqlite::ffi::SQLITE_ERROR);
    match code {
        ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => JarError::Unsettled(detail),
        ErrorCode::ReadOnly if rollback => JarError::Unsettled(detail),
        // Once the jar is open, a file that does not open is its journal.
        ErrorCode::CannotOpen if at != At::Open => JarError::Unsettled(detail),
        ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt => JarError::Unknown(detail),
        // Both statements are fixed text, so this is a table or a column that is not there.
        ErrorCode::Unknown if generic && at == At::Prepare => JarError::Unknown(detail),
        _ => JarError::Unreadable(detail),
    }
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) fn write_jar(path: &Path, cookies: &[Cookie]) -> io::Result<()> {
    let failed = |e: rusqlite::Error| io::Error::other(e.to_string());
    let connection = Connection::open(path).map_err(failed)?;
    // TRUNCATE, as Chromium's jar keeps an empty journal between writes.
    connection
        .execute_batch(
            "PRAGMA journal_mode=TRUNCATE;
             DROP TABLE IF EXISTS meta;
             DROP TABLE IF EXISTS cookies;
             CREATE TABLE meta(key LONGVARCHAR NOT NULL UNIQUE PRIMARY KEY, value LONGVARCHAR);
             INSERT INTO meta VALUES ('version', '24'), ('last_compatible_version', '24');
             CREATE TABLE cookies(creation_utc INTEGER NOT NULL, host_key TEXT NOT NULL,
                 top_frame_site_key TEXT NOT NULL DEFAULT '', name TEXT NOT NULL,
                 value TEXT NOT NULL DEFAULT '', encrypted_value BLOB NOT NULL,
                 path TEXT NOT NULL, expires_utc INTEGER NOT NULL);",
        )
        .map_err(failed)?;
    for cookie in cookies {
        connection
            .execute(
                "INSERT INTO cookies(creation_utc, host_key, name, encrypted_value, path, \
                 expires_utc) VALUES (0, ?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    cookie.host,
                    cookie.name,
                    cookie.stored,
                    cookie.path,
                    cookie.expires_utc
                ],
            )
            .map_err(failed)?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;

    struct Scratch(tempfile::TempDir);

    impl Scratch {
        fn new() -> Scratch {
            Scratch(tempfile::tempdir().expect("a scratch folder"))
        }

        fn path(&self) -> &Path {
            self.0.path()
        }

        fn jar(&self) -> PathBuf {
            self.path().join("Cookies")
        }

        fn listing(&self) -> Vec<(String, u64)> {
            let mut found: Vec<(String, u64)> = std::fs::read_dir(self.path())
                .expect("a listing")
                .flatten()
                .map(|entry| {
                    let len = entry.metadata().map(|m| m.len()).unwrap_or_default();
                    (entry.file_name().to_string_lossy().into_owned(), len)
                })
                .collect();
            found.sort();
            found
        }
    }

    fn session(host: &str, name: &str) -> Cookie {
        Cookie {
            host: host.into(),
            name: name.into(),
            path: "/".into(),
            stored: b"v10secret".to_vec(),
            expires_utc: 13_400_000_000_000_000,
        }
    }

    const HOSTS: &[&str] = &[".claude.ai", "claude.ai"];
    const NAMES: &[&str] = &["sessionKey", "sessionKeyV3"];

    #[test]
    fn a_jar_reads_its_session_rows_and_leaves_its_folder_as_it_was() {
        let scratch = Scratch::new();
        write_jar(
            &scratch.jar(),
            &[
                session(".claude.ai", "sessionKey"),
                session(".claude.ai", "lastActiveOrg"),
                session(".example.com", "sessionKeyV3"),
                session(".example.com", "other"),
            ],
        )
        .expect("a jar");
        let before = scratch.listing();
        let jar = cookies(&scratch.jar(), HOSTS, NAMES).expect("read");
        assert_eq!((jar.version, jar.compatible), (24, 24));
        let mut read: Vec<(String, String)> = jar
            .cookies
            .iter()
            .map(|c| (c.host.clone(), c.name.clone()))
            .collect();
        read.sort();
        assert_eq!(
            read,
            [
                (".claude.ai".to_string(), "lastActiveOrg".to_string()),
                (".claude.ai".into(), "sessionKey".into()),
                (".example.com".into(), "sessionKeyV3".into())
            ]
        );
        let key = jar.cookies.iter().find(|c| c.name == "sessionKey").unwrap();
        assert_eq!(key.stored, b"v10secret");
        assert_eq!(key.expires_utc, 13_400_000_000_000_000);
        assert!(!format!("{key:?}").contains("secret"), "{key:?}");
        assert_eq!(scratch.listing(), before);
    }

    #[test]
    fn a_jar_with_no_session_reads_empty_and_none_is_absent() {
        let scratch = Scratch::new();
        assert!(matches!(
            cookies(&scratch.jar(), HOSTS, NAMES),
            Err(JarError::Absent)
        ));
        assert!(scratch.listing().is_empty(), "nothing made");
        write_jar(&scratch.jar(), &[session(".example.com", "other")]).expect("a jar");
        let jar = cookies(&scratch.jar(), HOSTS, NAMES).expect("read");
        assert!(jar.cookies.is_empty());
    }

    #[test]
    fn a_hot_journal_or_a_log_is_unsettled_and_never_opened() {
        let scratch = Scratch::new();
        write_jar(&scratch.jar(), &[session(".claude.ai", "sessionKey")]).expect("a jar");
        std::fs::write(scratch.path().join("Cookies-journal"), b"a write").expect("a journal");
        assert!(matches!(
            cookies(&scratch.jar(), HOSTS, NAMES),
            Err(JarError::Unsettled(_))
        ));
        std::fs::write(scratch.path().join("Cookies-journal"), b"").expect("emptied");
        std::fs::write(scratch.path().join("Cookies-wal"), b"").expect("a log");
        assert!(matches!(
            cookies(&scratch.jar(), HOSTS, NAMES),
            Err(JarError::Unsettled(_))
        ));
    }

    #[test]
    fn a_missing_column_or_table_or_a_file_that_is_no_database_is_unknown() {
        let scratch = Scratch::new();
        let connection = Connection::open(scratch.jar()).expect("a database");
        connection
            .execute_batch(
                "CREATE TABLE meta(key LONGVARCHAR, value LONGVARCHAR);
                 INSERT INTO meta VALUES ('version', '24'), ('last_compatible_version', '24');
                 CREATE TABLE cookies(host_key TEXT, name TEXT, path TEXT, expires_utc INTEGER);",
            )
            .expect("a jar of another shape");
        drop(connection);
        assert!(matches!(
            cookies(&scratch.jar(), HOSTS, NAMES),
            Err(JarError::Unknown(_))
        ));
        let other = scratch.path().join("Other");
        Connection::open(&other)
            .and_then(|c| c.execute_batch("CREATE TABLE cookies(x);"))
            .expect("no meta");
        assert!(matches!(
            cookies(&other, HOSTS, NAMES),
            Err(JarError::Unknown(_))
        ));
        let text = scratch.path().join("Text");
        std::fs::write(&text, vec![b'x'; 4096]).expect("not a database");
        assert!(matches!(
            cookies(&text, HOSTS, NAMES),
            Err(JarError::Unknown(_))
        ));
    }
}
