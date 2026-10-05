//! A Claude Desktop data folder made in a test's own directory, and Pitboard's record of
//! its accounts made by hand: a cookie jar `sqlite3` writes the way Chromium lays it out,
//! a `config.json` naming the account, and parks beside it.
//!
//! Every write is checked by `guard_not_live_dir` first, and nothing here opens, quits or
//! signs out of the app. macOS only, where `sqlite3` and `shasum` are part of the system.

use super::{Env, guard_not_live_dir};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Chromium counts microseconds from 1601; this many seconds separate that from 1970.
pub const CHROME_EPOCH_OFFSET: i64 = 11_644_473_600;

/// A session that runs out a long way from now, in epoch seconds: 2031-01-01.
pub const FAR_OFF: i64 = 1_924_992_000;

/// Whether anything is running from inside a bundle called `Claude.app`: the person's own
/// Claude, which Pitboard is right to refuse to switch under.
pub fn claude_is_running() -> bool {
    let out = Command::new("/bin/ps")
        .args(["-x", "-o", "comm="])
        .output()
        .expect("ps runs");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|line| line.contains("Claude.app/"))
}

/// Hold the test's Claude Desktop data folder the way a Chromium app with a lock does:
/// Chromium's `SingletonLock`, a link naming `<hostname>-<pid>`. Claude Desktop 2.19675.0
/// keeps none (experiment E14), but Pitboard still honours one, and it stands in for the
/// running app without touching the process list. Here it names a stand-in that runs a
/// program called `Claude`, as the app's main process does. Pitboard reads a lock whose
/// process runs another program as stale, since the pid was given to something else. The
/// stand-in is `/bin/sleep` started through a link called `Claude` in the test's own
/// directory, outside any bundle, which `ps` reports as the program it runs; a copy of
/// `/bin/sleep` would be killed at launch, since macOS runs a system program only from its
/// own path. It is killed when the returned guard drops. The person's own Claude is neither asked nor
/// needed.
#[must_use = "the folder is held only while the stand-in runs"]
pub fn hold_with_a_stand_in(env: &Env) -> StandIn {
    let dir = support(env);
    guard_not_live_dir(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let bin = env.root.join("stand-in");
    std::fs::create_dir_all(&bin).unwrap();
    let program = bin.join("Claude");
    std::os::unix::fs::symlink("/bin/sleep", &program).expect("link sleep as the stand-in");
    let child = Command::new(&program)
        .arg("600")
        .spawn()
        .expect("start the stand-in");
    std::os::unix::fs::symlink(
        format!("pitboard-test-host-{}", child.id()),
        dir.join("SingletonLock"),
    )
    .unwrap();
    StandIn(child)
}

/// The process a test's `SingletonLock` names, killed when dropped.
pub struct StandIn(std::process::Child);

impl Drop for StandIn {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Every entry under `dir`, by path relative to it, with its inode: what a move would
/// change. Metadata only; no file is opened.
pub fn inodes_under(dir: &Path) -> Vec<(PathBuf, u64)> {
    use std::os::unix::fs::MetadataExt;
    fn walk(root: &Path, dir: &Path, found: &mut Vec<(PathBuf, u64)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            found.push((path.strip_prefix(root).unwrap().to_path_buf(), meta.ino()));
            if meta.is_dir() {
                walk(root, &path, found);
            }
        }
    }
    let mut found = Vec::new();
    walk(dir, dir, &mut found);
    found.sort();
    found
}

/// One session cookie's ciphertext, distinct per account: `v10` and a few bytes.
pub fn ciphertext(who: char) -> Vec<u8> {
    let mut bytes = b"v10".to_vec();
    bytes.extend_from_slice(&[0xAA, who as u8, 0x00, 0x5A]);
    bytes
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// The SHA-256 of `bytes` in lower-case hex, which is how Pitboard tells one session from
/// another without keeping it. Asked of `shasum`, because this crate has no hash of its own.
pub fn fingerprint(scratch: &Path, bytes: &[u8]) -> String {
    let file = scratch.join("ciphertext");
    std::fs::write(&file, bytes).unwrap();
    let out = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .arg(&file)
        .output()
        .expect("shasum runs");
    std::fs::remove_file(&file).unwrap();
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .expect("a digest")
        .to_string()
}

/// A cookie jar in `dir` holding `who`'s session until `expires_at`, laid out as Chromium's
/// version 24 lays it out, and nothing else.
pub fn write_jar(dir: &Path, who: char, expires_at: i64) {
    guard_not_live_dir(dir);
    std::fs::create_dir_all(dir).unwrap();
    let micros = (expires_at + CHROME_EPOCH_OFFSET) * 1_000_000;
    let sql = format!(
        "CREATE TABLE meta(key LONGVARCHAR NOT NULL UNIQUE PRIMARY KEY, value LONGVARCHAR);\
         INSERT INTO meta VALUES('version','24');\
         CREATE TABLE cookies(creation_utc INTEGER NOT NULL, host_key TEXT NOT NULL, \
         name TEXT NOT NULL, value TEXT NOT NULL, encrypted_value BLOB NOT NULL, \
         expires_utc INTEGER NOT NULL);\
         INSERT INTO cookies VALUES(0,'.claude.ai','sessionKey','',X'{}',{micros});",
        hex(&ciphertext(who))
    );
    let status = Command::new("/usr/bin/sqlite3")
        .args(["-init", "/dev/null"])
        .arg(dir.join("Cookies"))
        .arg(sql)
        .status()
        .expect("sqlite3 runs");
    assert!(status.success(), "sqlite3 could not write the jar");
}

/// This test's Claude Desktop data folder.
pub fn support(env: &Env) -> PathBuf {
    env.root.join("claude-desktop")
}

/// Sign the test's Claude Desktop in to `uuid`, with the session of `who`: a jar, a
/// `config.json` naming the account and keeping a setting of the machine's, and some local
/// storage of the account's.
pub fn sign_in_desktop(env: &Env, who: char, uuid: &str) {
    let dir = support(env);
    guard_not_live_dir(&dir);
    write_jar(&dir, who, FAR_OFF);
    std::fs::write(
        dir.join("config.json"),
        serde_json::to_vec_pretty(&json!({
            "lastKnownAccountUuid": uuid,
            "oauth:tokenCacheV2": format!("cache-of-{who}"),
            "darkMode": "system",
        }))
        .unwrap(),
    )
    .unwrap();
    let leveldb = dir.join("Local Storage/leveldb");
    std::fs::create_dir_all(&leveldb).unwrap();
    std::fs::write(leveldb.join("000003.log"), format!("storage of {who}")).unwrap();
}

/// Who the test's Claude Desktop says is signed in, by `config.json`.
pub fn signed_in_uuid(env: &Env) -> Option<String> {
    let raw = std::fs::read_to_string(support(env).join("config.json")).ok()?;
    let config: Value = serde_json::from_str(&raw).ok()?;
    config["lastKnownAccountUuid"].as_str().map(str::to_owned)
}

/// An account as Pitboard writes one down, parked or not.
pub struct Seeded<'a> {
    pub label: &'a str,
    pub who: char,
    pub uuid: String,
    pub park: Option<&'a str>,
}

/// Pitboard's record and parks, made by hand for a machine where Claude is open and no
/// enrolment can go through. Every account is written exactly as `enroll` and `use` would
/// leave it: its session's fingerprint, and for a parked one a private folder holding that
/// session with a manifest saying whose it is.
pub fn seed(env: &Env, accounts: &[Seeded], active: Option<&str>) {
    let home = env.root.join("pitboard");
    guard_not_live_dir(&home);
    let parks = home.join("desktop/parks");
    std::fs::create_dir_all(&parks).unwrap();
    use std::os::unix::fs::PermissionsExt;
    for dir in [&home, &home.join("desktop"), &parks] {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut written = Vec::new();
    for account in accounts {
        let print = fingerprint(&env.root, &ciphertext(account.who));
        let parked = account.park.map(|name| {
            let dir = parks.join(name);
            write_jar(&dir, account.who, FAR_OFF);
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
            std::fs::write(
                dir.join("config-keys.json"),
                json!({"lastKnownAccountUuid": account.uuid}).to_string(),
            )
            .unwrap();
            std::fs::write(
                dir.join("manifest.json"),
                json!({
                    "account_uuid": account.uuid,
                    "fingerprint": print,
                    "items": ["Cookies"],
                    "parked_at": 1_790_000_000,
                })
                .to_string(),
            )
            .unwrap();
            json!({
                "service": name,
                "parked_at": 1_790_000_000,
                "refresh_fingerprint": print,
                "access_expires_at": FAR_OFF,
                "refresh_expires_at": FAR_OFF,
            })
        });
        written.push(json!({
            "label": account.label,
            "account_uuid": account.uuid,
            "email": format!("{}@example.com", account.label),
            "parked": parked,
            "provider": "desktop",
            "session_fingerprint": print,
            "session_expires_at": FAR_OFF,
        }));
    }
    let mut state = json!({
        "schema": 4,
        "machine": pitboard_core::state::machine_id(),
        "accounts": written,
        "active": {},
        "slot": {},
        "discarded": [],
    });
    if let Some(label) = active {
        state["active"]["desktop"] = json!(label);
    }
    std::fs::write(
        home.join("state.json"),
        serde_json::to_vec_pretty(&state).unwrap(),
    )
    .unwrap();
}

/// Two Claude Desktop accounts: home signed in, work parked.
pub fn home_signed_in_work_parked(env: &Env) -> (String, String) {
    let (home, work) = (env.uuid('h'), env.uuid('w'));
    sign_in_desktop(env, 'h', &home);
    seed(
        env,
        &[
            Seeded {
                label: "home",
                who: 'h',
                uuid: home.clone(),
                park: None,
            },
            Seeded {
                label: "work",
                who: 'w',
                uuid: work.clone(),
                park: Some("pitboard-tree-work-1"),
            },
        ],
        Some("home"),
    );
    (home, work)
}

/// The organisation [`write_history`] files its sample under.
pub const ORG: &str = "0rg00000-0000-4000-8000-000000000001";

/// The app's own record of plan usage, with one sample for the organisation `uuid` is known
/// to have used: a folder the app keeps for it under the account, as the app does.
pub fn write_history(env: &Env, uuid: &str) {
    let dir = support(env);
    guard_not_live_dir(&dir);
    std::fs::create_dir_all(dir.join("claude-code-sessions").join(uuid).join(ORG)).unwrap();
    std::fs::write(
        dir.join("plan-usage-history.json"),
        json!({
            "version": 2,
            "samples": [{"t": 1_790_000_000_000_i64, "org": ORG, "u": {"fh": 0.3, "sd": 0.5}}],
        })
        .to_string(),
    )
    .unwrap();
}
