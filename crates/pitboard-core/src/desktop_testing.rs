// The Claude Desktop the tests play, on a scratch folder. Nothing here touches the real one.

use crate::context::Context;
use crate::host::memory::{MemoryHost, StandInCookie, StandInJar};
use crate::host::{Cookie, Host};
use crate::state::Key;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Person {
    pub uuid: &'static str,
    pub session: &'static str,
}

impl Person {
    // The fingerprint Pitboard keeps of this person's session cookie as stored.
    pub fn login(&self) -> String {
        crate::store::fingerprint(&hex::encode(self.stored()))
    }

    fn stored(&self) -> Vec<u8> {
        [b"v10".as_slice(), self.session.as_bytes()].concat()
    }

    fn tokens(&self) -> String {
        format!("\"acct:{}|{}\"", self.uuid, self.session)
    }
}

// The people of the app's Claude Desktop accounts, in its model's tests and its fixtures.
pub const WORK: Person = Person {
    uuid: "6f1c0a52-1111-4a6e-9d0b-2f6c1a9e7b10",
    session: "work-session",
};
pub const HOME: Person = Person {
    uuid: "b2aa91c4-3333-4c7a-a1d2-5e8f0c21d4e5",
    session: "home-session",
};
pub const TRAVEL: Person = Person {
    uuid: "c3bb02d5-5555-4d8b-b2e3-6f9a1d32e5f6",
    session: "travel-session",
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JarAs {
    // a real SQLite jar, for the real hosts
    Sqlite,
    // the JSON `MemoryHost` reads in its place
    StandIn,
}

// Four weeks after the core tests' clock, and long past for the command line's tests, which run
// on the real one. In Chromium's microseconds since 1601.
const EXPIRES_UTC: i64 = (1_762_500_000 + 11_644_473_600) * 1_000_000;
// What Claude leaves in each token cache after a Log out (E1b).
pub const EMPTIED: &str = "\"0123456789abcdefghijklmnopqr\"";

pub struct FakeClaude {
    data: PathBuf,
    app: PathBuf,
    host: Arc<MemoryHost>,
    jar: JarAs,
    mind: std::sync::Mutex<Mind>,
}

// What a running Claude holds in memory and writes as it quits.
#[derive(Default)]
struct Mind {
    // None: whoever the folder held when it opened. Some(None): logged out.
    session: Option<Option<Person>>,
    reported: bool,
}

const SETTINGS: &str = "config.json";
const SIDECAR: &str = "config.json.journal";
// A key Claude writes while it runs and takes out, through a sidecar, as it quits.
const RUNNING: &str = "updaterRunningInstance";
pub const STORES: [(&str, &str); 4] = [
    ("Local Storage/leveldb", "000003.log"),
    ("Session Storage", "000003.log"),
    (
        "IndexedDB/https_claude.ai_0.indexeddb.leveldb",
        "000003.log",
    ),
    ("WebStorage/QuotaManager", "db"),
];

impl FakeClaude {
    pub fn on(host: Arc<MemoryHost>, ctx: &Context, jar: JarAs) -> FakeClaude {
        let scratch = ctx
            .scratch_claude()
            .expect("a context naming a scratch Claude");
        FakeClaude {
            data: scratch.data.clone(),
            app: scratch.app.clone(),
            host,
            jar,
            mind: std::sync::Mutex::default(),
        }
    }

    fn mind(&self) -> std::sync::MutexGuard<'_, Mind> {
        self.mind
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn data(&self) -> PathBuf {
        self.data.clone()
    }

    pub fn program(&self) -> PathBuf {
        self.app.join("Contents/MacOS/Claude")
    }

    // Listed, its own program and a helper it started. It makes the stores that are missing
    // and writes a key of its own into its settings.
    pub fn opens(&self) {
        *self.mind() = Mind::default();
        opened(&self.data());
        self.lists();
    }

    fn lists(&self) {
        let helper = self.app.join(
            "Contents/Frameworks/Claude Helper (Renderer).app/Contents/MacOS/\
             Claude Helper (Renderer)",
        );
        let (program, helper) = (self.program(), helper);
        let program = program.to_string_lossy();
        let helper = helper.to_string_lossy();
        self.host.runs_at("Claude", &[program.as_ref()]);
        self.host
            .runs_at("Claude Helper (Renderer)", &[helper.as_ref()]);
        self.host.child_of(&helper, &program);
    }

    // Unlisted, writing nothing.
    pub fn crashes(&self) {
        self.host.runs_at("Claude", &[]);
        self.host.runs_at("Claude Helper (Renderer)", &[]);
    }

    // The session in memory, nothing on disk yet.
    pub fn signs_in(&self, who: &Person) {
        *self.mind() = Mind {
            session: Some(Some(*who)),
            reported: false,
        };
    }

    // The account's name is written as Claude quits (E4).
    pub fn reports(&self) {
        self.mind().reported = true;
    }

    // Another session value under the same name, as on 5 October: the account stays signed in.
    pub fn renews(&self, who: &Person, session: &'static str) {
        *self.mind() = Mind {
            session: Some(Some(Person {
                uuid: who.uuid,
                session,
            })),
            reported: true,
        };
    }

    // The session gone, both caches the emptied value, the name kept (E1b).
    pub fn logs_out(&self) {
        *self.mind() = Mind {
            session: Some(None),
            reported: false,
        };
    }

    // Writes the jar and `config.json` whole, then leaves its own key's removal in a valid
    // sidecar, as 2.7032.0 does. Unlisted (E8).
    pub fn quits(&self) {
        let mind = std::mem::take(&mut *self.mind());
        quit(&self.data(), self.jar, &mind);
        self.crashes();
    }

    pub fn quits_before_reporting(&self) {
        self.mind().reported = false;
        self.quits();
    }

    // A sidecar that names a `config.json` since written over: Claude drops it when it opens.
    pub fn leaves_a_stale_sidecar(&self) {
        let body = "{\n\t\"locale\": \"fr-FR\"\n}";
        std::fs::write(
            self.data().join(SIDECAR),
            format!("{} {} .\n{body}", sha(body.as_bytes()), sha(b"gone")),
        )
        .expect("a sidecar");
    }

    // A whole signed-in data folder, as Claude leaves one when it quits.
    pub fn signed_in_as(&self, who: &Person) {
        plant(&self.data(), who, self.jar);
    }

    // Whether its stand-in jar holds a claude.ai session, as a Claude that opens signed in has.
    pub fn holds_a_session(&self) -> bool {
        let jar = self.data().join("Cookies");
        let found = self.host.cookies(&jar, &[".claude.ai"], &["sessionKey"]);
        // The host also gives a cookie of that name from any other site.
        found.is_ok_and(|jar| {
            jar.cookies
                .iter()
                .any(|cookie| cookie.host == ".claude.ai" && cookie.name == "sessionKey")
        })
    }
}

// `who` signs in inside Claude while `pitboard` waits, then is enrolled as `label`.
pub fn signs_in_inside(
    pitboard: &crate::service::Pitboard,
    claude: &FakeClaude,
    label: &str,
    who: &Person,
) -> Result<(), String> {
    let watched = pitboard
        .sign_in_watched(label)
        .map_err(|failed| format!("parking for a sign-in inside Claude: {}", failed.error))?;
    claude.opens();
    let steps = watched.steps();
    let given_up = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while steps.now() != Some(crate::switch::WaitStep::Open) {
        if std::time::Instant::now() > given_up {
            watched.cancel();
            return Err("the wait never saw Claude open".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    claude.signs_in(who);
    claude.reports();
    claude.quits();
    let login = watched
        .finish()
        .map_err(|error| format!("waiting for Claude to quit: {error}"))?;
    pitboard
        .enroll_signed_in(label, login)
        .map(drop)
        .map_err(|failed| {
            format!(
                "enrolling {label}, signed in inside Claude: {}",
                failed.error
            )
        })
}

// The stores Claude makes when it opens, and a key of its own in its settings.
fn opened(data: &Path) {
    std::fs::create_dir_all(data).expect("a data folder");
    for (store, file) in STORES {
        if !data.join(store).exists() {
            std::fs::create_dir_all(data.join(store)).expect("a store");
            std::fs::write(data.join(store).join(file), "made by Claude").expect("a file");
        }
    }
    let mut pairs = settings(data);
    if !pairs.iter().any(|(key, _)| key == RUNNING) {
        pairs.push((RUNNING.into(), "true".into()));
        write_settings(data, &pairs);
    }
}

fn quit(data: &Path, jar_as: JarAs, mind: &Mind) {
    let jar = data.join("Cookies");
    let held = match mind.session {
        Some(session) => {
            write_jar(
                &jar,
                jar_as,
                &session.map_or_else(Vec::new, |who| cookies(&who)),
            );
            std::fs::write(data.join("Cookies-journal"), b"").expect("a journal");
            Some(session)
        }
        None if !jar.exists() => {
            write_jar(&jar, jar_as, &[]);
            std::fs::write(data.join("Cookies-journal"), b"").expect("a journal");
            Some(None)
        }
        None => None,
    };
    let mut pairs = settings(data);
    match held {
        Some(Some(who)) => {
            set(&mut pairs, "oauth:tokenCache", who.tokens());
            set(&mut pairs, "oauth:tokenCacheV2", who.tokens());
            if mind.reported {
                set(
                    &mut pairs,
                    "lastKnownAccountUuid",
                    format!("\"{}\"", who.uuid),
                );
            }
        }
        Some(None) => {
            set(&mut pairs, "oauth:tokenCache", EMPTIED.into());
            set(&mut pairs, "oauth:tokenCacheV2", EMPTIED.into());
        }
        None => {}
    }
    write_settings(data, &pairs);
    let on_disk = std::fs::read(data.join(SETTINGS)).expect("settings");
    pairs.retain(|(key, _)| key != RUNNING);
    let body = body_of(&pairs);
    std::fs::write(
        data.join(SIDECAR),
        format!("{} {} .\n{body}", sha(body.as_bytes()), sha(&on_disk)),
    )
    .expect("a sidecar");
}

// What Claude writes over whatever `folder` holds as it quits signed in to `who`.
pub fn sign_in_on_disk(folder: &Path, who: &Person, jar: JarAs) {
    crate::context::refuse_a_real_claude(folder);
    opened(folder);
    quit(
        folder,
        jar,
        &Mind {
            session: Some(Some(*who)),
            reported: true,
        },
    );
}

// The settings as Claude holds them: a valid sidecar's body, else the file's.
fn settings(data: &Path) -> Vec<(String, String)> {
    let on_disk = std::fs::read(data.join(SETTINGS)).ok();
    let pending = std::fs::read_to_string(data.join(SIDECAR))
        .ok()
        .and_then(|written| {
            let (header, body) = written.split_once('\n')?;
            let fields: Vec<&str> = header.split(' ').collect();
            let file = on_disk.as_deref().map_or_else(|| "-".to_string(), sha);
            (fields[0] == sha(body.as_bytes()) && fields[1] == file).then(|| body.to_string())
        });
    let _ = std::fs::remove_file(data.join(SIDECAR));
    let raw = pending
        .or_else(|| on_disk.map(|bytes| String::from_utf8(bytes).expect("UTF-8")))
        .unwrap_or_else(|| "{}".into());
    let parsed: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&raw).expect("settings are JSON");
    parsed
        .into_iter()
        .map(|(key, value)| (key, value.to_string()))
        .collect()
}

fn write_settings(data: &Path, pairs: &[(String, String)]) {
    let path = data.join(SETTINGS);
    let fresh = !path.exists();
    std::fs::write(&path, body_of(pairs)).expect("settings");
    if fresh {
        let _ = crate::host::fs::testing::make_private(&path);
    }
}

fn sha(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

fn set(pairs: &mut Vec<(String, String)>, key: &str, value: String) {
    match pairs.iter_mut().find(|(held, _)| held == key) {
        Some((_, held)) => *held = value,
        None => pairs.push((key.into(), value)),
    }
}

fn body_of(pairs: &[(String, String)]) -> String {
    let lines: Vec<String> = pairs
        .iter()
        .map(|(key, value)| format!("\t{}: {value}", serde_json::Value::from(key.as_str())))
        .collect();
    format!("{{\n{}\n}}", lines.join(",\n"))
}

fn cookies(who: &Person) -> Vec<Cookie> {
    vec![
        Cookie {
            host: ".claude.ai".into(),
            name: "sessionKey".into(),
            path: "/".into(),
            stored: who.stored(),
            expires_utc: EXPIRES_UTC,
        },
        Cookie {
            host: ".claude.ai".into(),
            name: "lastActiveOrg".into(),
            path: "/".into(),
            stored: b"v10org".to_vec(),
            expires_utc: EXPIRES_UTC,
        },
    ]
}

pub fn plant(folder: &Path, who: &Person, jar: JarAs) {
    crate::context::refuse_a_real_claude(folder);
    std::fs::create_dir_all(folder).expect("a scratch data folder");
    write_jar(&folder.join("Cookies"), jar, &cookies(who));
    std::fs::write(folder.join("Cookies-journal"), b"").expect("a journal");
    for (store, file) in STORES.into_iter().chain([("Cache/Cache_Data", "index")]) {
        std::fs::create_dir_all(folder.join(store)).expect("a store");
        std::fs::write(folder.join(store).join(file), who.session).expect("a store's file");
    }
    std::fs::write(folder.join("Preferences"), b"{}").expect("preferences");
    let settings = folder.join("config.json");
    std::fs::write(
        &settings,
        format!(
            "{{\n\t\"locale\": \"en-US\",\n\t\"oauth:tokenCache\": {tokens},\n\t\
             \"userThemeMode\": \"dark\",\n\t\"oauth:tokenCacheV2\": {tokens},\n\t\
             \"lastKnownAccountUuid\": \"{uuid}\"\n}}",
            tokens = who.tokens(),
            uuid = who.uuid
        ),
    )
    .expect("settings");
    // Not the mode a new file gets, so a write that loses it shows. Windows sets none yet.
    let _ = crate::host::fs::testing::make_private(&settings);
}

// Claude is listed from the leave's third look, the one after its last write: no fault point.
pub fn leave_a_move_waiting(ctx: &Context, host: &MemoryHost, account: &Key) {
    let program = ctx
        .scratch_claude()
        .expect("a context naming a scratch Claude")
        .app
        .join("Contents/MacOS/Claude");
    let program = program.to_string_lossy();
    let state = crate::state::load(ctx).expect("the index");
    let to = state
        .accounts
        .iter()
        .map(|enrolled| Key::new(enrolled.provider(), enrolled.label.clone()))
        .find(|key| key.provider == account.provider && key != account)
        .expect("another account to switch to");
    host.listed_from(3, &program);
    let stopped = crate::service::Pitboard::new(ctx.clone()).switch_to(&to.qualified());
    host.listed_no_longer(&program);
    let code = stopped.map(drop).map_err(|failed| failed.error.code());
    assert_eq!(
        code,
        Err("app_opened_midway"),
        "the move stops after its last write"
    );
}

pub fn write_jar(path: &Path, jar: JarAs, cookies: &[Cookie]) {
    crate::context::refuse_a_real_claude(path);
    match jar {
        JarAs::Sqlite => crate::host::write_jar(path, cookies).expect("a jar"),
        JarAs::StandIn => write_stand_in_jar(path, 24, cookies),
    }
}

pub fn write_stand_in_jar(path: &Path, compatible: i64, cookies: &[Cookie]) {
    crate::context::refuse_a_real_claude(path);
    let jar = StandInJar {
        version: compatible,
        compatible,
        cookies: cookies
            .iter()
            .map(|cookie| StandInCookie {
                host: cookie.host.clone(),
                name: cookie.name.clone(),
                path: cookie.path.clone(),
                stored: hex::encode(&cookie.stored),
                expires_utc: cookie.expires_utc,
            })
            .collect(),
    };
    std::fs::write(path, serde_json::to_vec(&jar).expect("JSON")).expect("a stand-in jar");
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;

    // By the guard, not by a write that failed: SQLite is macOS's alone.
    fn refused(write: &dyn Fn()) -> bool {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(write))
            .err()
            .and_then(|panicked| panicked.downcast::<String>().ok())
            .is_some_and(|said| said.contains("give it a folder of its own"))
    }

    // Under a fake home, so a write the guard lets through lands in a scratch.
    #[test]
    fn nothing_here_writes_inside_the_accounts_own_application_support() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        let _guarding = crate::context::testing::also_guarding(&home);
        // Where the guard looks on each system.
        let below = match crate::host::OS {
            crate::host::Os::MacOs | crate::host::Os::Linux => {
                Path::new("Library/Application Support/Claude")
            }
            crate::host::Os::Windows => Path::new(r"AppData\Roaming\Claude"),
        };
        let data = home.join(below);
        let jar = data.join("Cookies");
        let planted = refused(&|| plant(&data, &WORK, JarAs::StandIn));
        let signed_in = refused(&|| sign_in_on_disk(&data, &WORK, JarAs::StandIn));
        let sqlite = refused(&|| write_jar(&jar, JarAs::Sqlite, &[]));
        let stand_in = refused(&|| write_jar(&jar, JarAs::StandIn, &[]));
        let bare = refused(&|| write_stand_in_jar(&jar, 24, &[]));
        let climbed = home.join("never-made").join("..").join(below);
        let planted_through = refused(&|| plant(&climbed, &WORK, JarAs::StandIn));
        let signed_in_through = refused(&|| sign_in_on_disk(&climbed, &WORK, JarAs::StandIn));
        let written = home.exists();
        assert!(planted, "plant");
        assert!(signed_in, "sign_in_on_disk");
        assert!(sqlite, "write_jar, SQLite");
        assert!(stand_in, "write_jar, the stand-in");
        assert!(bare, "write_stand_in_jar");
        assert!(planted_through, "plant, through `..`");
        assert!(signed_in_through, "sign_in_on_disk, through `..`");
        assert!(!written, "nothing was written");
    }

    // A link to a file not made yet resolves nowhere, and a write through it makes the file.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows makes no link"
    )]
    fn nothing_here_writes_through_a_link_to_a_file_not_made_yet() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let root = scratch.path().join("root");
        let home = root.join("home");
        let data = home.join("Library/Application Support/Claude");
        let scratch = root.join("scratch");
        for made in [&data, &scratch] {
            std::fs::create_dir_all(made).expect("a folder of the test's own");
        }
        let _guarding = crate::context::testing::also_guarding(&home);
        let jar = scratch.join("Cookies");
        crate::host::fs::testing::link(&data.join("Cookies"), &jar).expect("a link");
        let sqlite = refused(&|| write_jar(&jar, JarAs::Sqlite, &[]));
        let stand_in = refused(&|| write_jar(&jar, JarAs::StandIn, &[]));
        let bare = refused(&|| write_stand_in_jar(&jar, 24, &[]));
        let written = data.join("Cookies").exists();
        assert!(sqlite, "write_jar, SQLite");
        assert!(stand_in, "write_jar, the stand-in");
        assert!(bare, "write_stand_in_jar");
        assert!(!written, "nothing was written");
    }
}
