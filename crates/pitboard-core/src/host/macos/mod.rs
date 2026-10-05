//! macOS: the login keychain, files, `ps` and launchd.

mod helper;
mod keychain;
mod launchd;
mod ps;
mod safe_storage;
mod sqlite;

pub(crate) use super::unix::{fs, proc, user};

use super::unix::service;
use super::{Bundle, Host, LoginPath, Os, Process, Scheduler};
use crate::context::{Context, Environment};
use crate::provider::desktop::safe_storage::SafeStorage;
use crate::provider::desktop::types::CookieTable;
use crate::store::{PlainFile, RawStore};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

pub(super) const OS: Os = Os::MacOs;

/// What `posix_spawn` is told when Pitboard starts the person's login shell: a session of its
/// own, and none of this process's descriptors but the three it is handed, whatever in the
/// app opened the rest. `POSIX_SPAWN_SETSID` is 0x0400 in `<sys/spawn.h>` of the macOS 27
/// SDK, and the libc crate does not name it.
pub(super) const SPAWN_FLAGS: libc::c_short =
    0x0400 | libc::POSIX_SPAWN_CLOEXEC_DEFAULT as libc::c_short;

/// macOS has a login shell to ask.
pub(super) fn login_path(env: &Environment) -> LoginPath {
    super::unix::shell::login_path(env, SPAWN_FLAGS)
}

/// The keychain account Pitboard stores its own items under.
///
/// It is Claude Code's derivation, and it stays Claude Code's derivation, because every
/// park already on every machine is filed under whatever this returned the day it was
/// written. Changing it would not move those items; it would make them unfindable, which
/// is the same as deleting every parked login on upgrade.
fn vault_account(ctx: &Context) -> String {
    crate::provider::claude::slot::account_name(ctx)
}

#[derive(Debug)]
struct MacOs {
    scheduler: launchd::Launchd,
}

impl Host for MacOs {
    fn foreign_secrets(&self, ctx: &Context, account: &str) -> Option<Box<dyn RawStore>> {
        Some(Box::new(keychain::Keychain::foreign(
            ctx,
            account.to_string(),
        )))
    }

    fn file(&self, path: PathBuf) -> Box<dyn RawStore> {
        Box::new(PlainFile::at(path))
    }

    fn vault(&self, ctx: &Context) -> Box<dyn RawStore> {
        Box::new(keychain::Keychain::vault(ctx))
    }

    fn vault_is_shared(&self) -> bool {
        true
    }

    fn processes(&self, program: &str) -> Option<Vec<Process>> {
        ps::processes(program)
    }

    fn processes_within(&self, bundle: Bundle<'_>, excluded: &[&str]) -> Option<Vec<Process>> {
        ps::processes_within(bundle, excluded)
    }

    fn pid_alive(&self, pid: u32) -> bool {
        proc::may_be_running(pid)
    }

    fn program_of(&self, pid: u32) -> Option<PathBuf> {
        ps::program_of(pid)
    }

    fn device_of(&self, path: &Path) -> std::io::Result<u64> {
        fs::device(path)
    }

    /// Read by the system's own `sqlite3`, which every Mac has, so Pitboard links no SQLite
    /// of its own.
    fn cookie_table(&self, path: &Path) -> std::io::Result<CookieTable> {
        sqlite::cookie_table(path)
    }

    /// Read by the system's own `plutil`, from the bundle's `Info.plist`.
    fn bundle_version(&self, app: &Path) -> Option<String> {
        let mut plutil = Command::new("/usr/bin/plutil");
        plutil
            .args(["-extract", "CFBundleShortVersionString", "raw", "-o", "-"])
            .arg(app.join("Contents/Info.plist"));
        let out = helper::output_within(plutil, b"", Duration::from_secs(5)).ok()?;
        if !out.status.success() {
            return None;
        }
        let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!version.is_empty()).then_some(version)
    }

    fn scheduler(&self) -> Option<&dyn Scheduler> {
        Some(&self.scheduler)
    }
}

pub(super) fn host() -> Arc<dyn Host> {
    Arc::new(MacOs {
        scheduler: launchd::Launchd::new(service::system()),
    })
}

/// Claude Desktop's key is in the login keychain, read through `security`.
pub(super) fn safe_storage() -> Arc<dyn SafeStorage> {
    Arc::new(safe_storage::SecurityCli)
}

/// macOS already says what path a program was started by.
pub(super) fn current_program() -> std::io::Result<PathBuf> {
    std::env::current_exe()
}

/// launchd as a machine in memory has it: real files in the test's own home, and a service
/// manager that asks nobody.
#[cfg(any(test, feature = "test-support"))]
pub(super) fn pretend_scheduler(
    refuse_start: Arc<std::sync::atomic::AtomicBool>,
) -> Box<dyn Scheduler> {
    Box::new(launchd::Launchd::new(Arc::new(service::Pretend {
        refuse_start,
    })))
}

#[cfg(test)]
mod tests {
    use super::sqlite::SQLITE;
    use crate::provider::desktop::cookies;
    use crate::provider::desktop::types::CookieRow;
    use std::process::Command;

    /// The query goes through the system's own `sqlite3`, against a database made the way
    /// Chromium's is laid out, in a folder whose name needs escaping in a URI.
    #[test]
    fn reads_a_real_database_through_sqlite3() {
        let dir = std::env::temp_dir().join(format!(
            "pitboard-cookies #1 ?{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("Cookies");
        let made = Command::new(SQLITE)
            .arg(&db)
            .arg(
                "CREATE TABLE meta(key LONGVARCHAR NOT NULL UNIQUE PRIMARY KEY, value LONGVARCHAR);\
                 INSERT INTO meta VALUES('version','24'),('last_compatible_version','24');\
                 CREATE TABLE cookies(creation_utc INTEGER NOT NULL, host_key TEXT NOT NULL, \
                   name TEXT NOT NULL, value TEXT NOT NULL, encrypted_value BLOB NOT NULL, \
                   expires_utc INTEGER NOT NULL);\
                 INSERT INTO cookies VALUES(1,'.claude.ai','sessionKey','',X'763130AABB00',\
                   13437792000000000);\
                 INSERT INTO cookies VALUES(1,'.claude.ai','lastActiveOrg','',X'763130CC',0);\
                 INSERT INTO cookies VALUES(1,'.claude.ai','cf_clearance','',X'763130DD',0);\
                 INSERT INTO cookies VALUES(1,'example.com','sessionKey','',X'763130EE',0);",
            )
            .status()
            .unwrap();
        assert!(made.success());

        let host = crate::host::current();
        let table = host.cookie_table(&db).expect("read");
        assert_eq!(table.meta_version, 24);
        let mut rows = table.rows.clone();
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(
            rows,
            vec![
                CookieRow {
                    host_key: ".claude.ai".into(),
                    name: "lastActiveOrg".into(),
                    encrypted_value: vec![0x76, 0x31, 0x30, 0xcc],
                    expires_utc: 0,
                },
                CookieRow {
                    host_key: ".claude.ai".into(),
                    name: "sessionKey".into(),
                    encrypted_value: vec![0x76, 0x31, 0x30, 0xaa, 0xbb, 0x00],
                    expires_utc: 13_437_792_000_000_000,
                },
            ]
        );
        assert!(cookies::session(&table).unwrap().is_some());

        // Read while another process holds the database locked, the way a running Claude
        // can, without waiting and without taking a lock of its own.
        let mut holder = Command::new(SQLITE)
            .arg(&db)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        {
            use std::io::{BufRead, Write};
            let mut stdin = holder.stdin.take().unwrap();
            stdin
                .write_all(b"BEGIN EXCLUSIVE;\nSELECT 'locked';\n")
                .unwrap();
            let mut line = String::new();
            std::io::BufReader::new(holder.stdout.as_mut().unwrap())
                .read_line(&mut line)
                .unwrap();
            assert_eq!(line.trim(), "locked");
            let started = std::time::Instant::now();
            let held = host.cookie_table(&db).expect("read under a lock");
            assert_eq!(held, table);
            assert!(started.elapsed() < std::time::Duration::from_secs(2));
            drop(stdin);
        }
        let _ = holder.kill();
        let _ = holder.wait();

        // A write still in the journal is never read around.
        std::fs::write(dir.join("Cookies-journal"), b"a write in progress").unwrap();
        let busy = host.cookie_table(&db).unwrap_err();
        assert_eq!(busy.kind(), std::io::ErrorKind::ResourceBusy, "{busy}");
        std::fs::write(dir.join("Cookies-journal"), b"").unwrap();
        assert_eq!(host.cookie_table(&db).expect("an empty journal"), table);

        // No jar at all is "not found", which a caller reads as signed out.
        let missing = host.cookie_table(&dir.join("Nothing")).unwrap_err();
        assert_eq!(missing.kind(), std::io::ErrorKind::NotFound);
        // Something that is not a database is an error, never an empty jar.
        std::fs::write(
            dir.join("Garbage"),
            b"not a database at all, not even close",
        )
        .unwrap();
        assert!(host.cookie_table(&dir.join("Garbage")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The version is the bundle's own, read by the system's `plutil`, and no app is no
    /// version rather than an error.
    #[test]
    fn the_installed_version_is_the_bundles() {
        let app = std::env::temp_dir().join(format!(
            "pitboard-version-{}-{:?}/Claude.app",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(app.parent().unwrap());
        std::fs::create_dir_all(app.join("Contents")).unwrap();
        let host = crate::host::current();
        assert_eq!(host.bundle_version(&app), None);
        std::fs::write(
            app.join("Contents/Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.anthropic.claudefordesktop</string>
<key>CFBundleShortVersionString</key><string>2.19675.0</string>
</dict></plist>"#,
        )
        .unwrap();
        assert_eq!(host.bundle_version(&app).as_deref(), Some("2.19675.0"));
        let _ = std::fs::remove_dir_all(app.parent().unwrap());
    }
}
