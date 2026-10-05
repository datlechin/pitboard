//! A machine to run changes against: two accounts of one tool, stores in memory, services
//! that answer from a script, and a clock that stands still.
//!
//! Shared by the tests that kill a change partway ([`super::crash`]), the tests that make
//! one refuse ([`super::refusals`]), the tests of what a sign-in enrols ([`super::enroll`]),
//! and the tests of what a change refused over its name still settles ([`crate::service`]),
//! because all of them need the same starting shape:
//! one account signed in, one parked and ready, and the tool's own files where the engine
//! expects them. There is one for each tool, and the invariants in [`hold`] are asked of
//! every one of them through the provider boundary, because what must be true after a
//! crash is a fact about parking a login and not about any one tool's.

use super::*;
use crate::api::Owner;
use crate::api::scripted::ScriptedApi;
use crate::provider::ProviderId;
use crate::provider::claude::paths as claude;

use crate::host::memory::MemoryHost;
use crate::state::Account;
use crate::time::{Clock, FixedClock};
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;

pub(crate) const NOW: i64 = 1_760_000_000;

/// Every place a change can be killed, named the way the code names it.
pub(crate) const POINTS: [&str; 6] = [
    "switch.journal_written",
    "switch.park_stored",
    "switch.park_recorded",
    "switch.installed",
    "switch.recorded",
    "switch.config_updated",
];

pub(crate) struct Machine {
    pub(crate) ctx: Context,
    pub(crate) mem: Arc<MemoryHost>,
    pub(crate) api: Arc<ScriptedApi>,
    root: PathBuf,
    /// The keychain item Claude Code's login is in, on a Claude Code machine.
    pub(crate) service: String,
    /// Which tool's accounts this machine holds.
    pub(crate) which: ProviderId,
}

impl Machine {
    /// Where the tool's own files are, for a test that needs to change one.
    pub(crate) fn ctx_home(&self) -> PathBuf {
        self.root.clone()
    }

    /// The live login, read the way the tool reads it.
    pub(crate) fn live(&self) -> Option<Value> {
        crate::provider::of(self.which)
            .read_live(&self.ctx)
            .ok()
            .flatten()
            .map(|credential| credential.raw)
    }

    /// Replace the live login, the way the tool itself would write it.
    pub(crate) fn sign_in(&self, document: &Value) {
        let live = crate::provider::of(self.which)
            .live(&self.ctx)
            .expect("a store to write to");
        store::write_raw(&live.chain, &live.service, &document.to_string())
            .expect("the live login is written");
    }

    pub(crate) fn key(&self, label: &str) -> Key {
        Key::new(self.which, label)
    }

    /// From now on the live login's store misbehaves this way: the keychain item for Claude
    /// Code, the `auth.json` file for Codex.
    pub(crate) fn fault_live(&self, fault: crate::store::memory::Fault) {
        let (store, service) = self.live_store();
        store.fault(&service, fault);
    }

    /// The store the live login is in, and its name there, for a test that has to fault it
    /// from inside a change.
    pub(crate) fn live_store(&self) -> (Arc<crate::store::memory::MemoryStore>, String) {
        let live = crate::provider::of(self.which)
            .live(&self.ctx)
            .expect("a live store");
        let store = match self.which {
            ProviderId::Claude => Arc::clone(self.mem.live()),
            ProviderId::Codex => self
                .mem
                .file_at(crate::provider::codex::paths::auth_file(&self.ctx)),
            ProviderId::Desktop => unreachable!("no machine keeps Claude Desktop in a vault"),
        };
        (store, live.service)
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub(crate) fn oauth(refresh: &str, expires_in_days: i64) -> Value {
    json!({
        "accessToken": format!("access-{refresh}"),
        "refreshToken": refresh,
        "expiresAt": (NOW + 3600) * 1000,
        "refreshTokenExpiresAt": (NOW + expires_in_days * 86_400) * 1000,
        "scopes": ["user:profile", "user:inference"],
    })
}

pub(crate) fn document(refresh: &str) -> Value {
    json!({
        "claudeAiOauth": oauth(refresh, 30),
        "organizationUuid": "org-of-the-outgoing-account",
        "mcpOAuth": {"some-server": {"token": "unrelated"}},
    })
}

pub(crate) fn owner(uuid: &str) -> Owner {
    Owner {
        account_uuid: uuid.into(),
        email: format!("{uuid}@example.com"),
        organization_uuid: format!("org-{uuid}"),
    }
}

/// Two accounts: `here` is signed in, `there` is parked and ready. The shape every switch
/// starts from.
pub(crate) fn machine(name: &str) -> Machine {
    let root = std::env::temp_dir().join(format!(
        "pitboard-crash-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a scratch home");

    let mem = MemoryHost::new();
    let api = ScriptedApi::new();
    let ctx = Context::new(root.clone())
        .with_pitboard_home(root.join(".pitboard"))
        .with_memory_stores(Arc::clone(&mem))
        .with_scripted_api(Arc::clone(&api))
        .with_clock(Arc::new(FixedClock::at(NOW)) as Arc<dyn Clock>);

    // Claude Code's own files: the live credential, and the config a switch rewrites.
    let service = claude::live_service(&ctx);
    mem.live()
        .plant(&service, &document("here-refresh").to_string());
    std::fs::write(
        root.join(".claude.json"),
        json!({
            "oauthAccount": {
                "accountUuid": "here",
                "emailAddress": "here@example.com",
                "organizationUuid": "org-here",
            },
            "cachedArtifactRoster": {"org": "org-here"},
            "numStartups": 7,
        })
        .to_string(),
    )
    .expect("a config file");

    api.owned_by("access-here-refresh", owner("here"));
    api.owned_by("access-there-refresh", owner("there"));

    // `there` holds a parked login, written the way a switch would have written it.
    std::fs::create_dir_all(root.join(".pitboard")).expect("a Pitboard home");
    let parked_service = park::reserve(&ctx, "there").expect("a free name");
    let parked = park::store_at(
        &ctx,
        crate::provider::ProviderId::Claude,
        &parked_service,
        &oauth("there-refresh", 30),
    )
    .expect("parked");

    let mut state = State::default();
    state.accounts.push(account("here", "here", None));
    state.accounts.push(account("there", "there", Some(parked)));
    state.set_active(ProviderId::Claude, Some("here".into()));
    state::save(&ctx, &state).expect("saved");

    Machine {
        ctx,
        mem,
        api,
        root,
        service,
        which: ProviderId::Claude,
    }
}

/// Where OpenAI puts its own claims in a standard token.
const OPENAI: &str = "https://api.openai.com/auth";

/// A Codex login as `codex login` writes one, for the account `who`.
///
/// The access token is unique to the refresh token so every login has its own, which is
/// what the scripted usage answers are keyed by.
pub(crate) fn codex_login(who: &str, refresh: &str) -> Value {
    json!({
        "auth_mode": "chatgpt",
        "OPENAI_API_KEY": null,
        "tokens": {
            "id_token": crate::provider::jwt::unsigned(&json!({
                "email": format!("{who}@example.com"),
                "exp": NOW + 3600,
                OPENAI: {
                    "chatgpt_account_id": who,
                    "chatgpt_user_id": format!("user-{who}"),
                    "chatgpt_plan_type": "pro",
                },
            })),
            "access_token": codex_access(refresh),
            "refresh_token": refresh,
            "account_id": who,
        },
        "last_refresh": "2025-10-09T08:00:00Z",
    })
}

/// The identity Codex's own claims give the account `who`: the ChatGPT account with the
/// person inside it.
pub(crate) fn codex_id(who: &str) -> String {
    format!("{who}_user-{who}")
}

/// The access token [`codex_login`] carries for this refresh token.
pub(crate) fn codex_access(refresh: &str) -> String {
    crate::provider::jwt::unsigned(&json!({"exp": NOW + 10 * 86_400, "for": refresh}))
}

pub(crate) fn codex_account(label: &str, uuid: &str, parked: Option<Park>) -> Account {
    Account {
        last_used_at: None,
        label: label.into(),
        account_uuid: uuid.into(),
        email: format!("{uuid}@example.com"),
        parked,
        detail: crate::state::Detail::Codex {
            workspace_id: None,
            plan: Some("pro".into()),
        },
    }
}

/// The same shape for Codex: `here` is signed in, `there` is parked and ready, and OpenAI
/// answers for both.
pub(crate) fn codex_machine(name: &str) -> Machine {
    let root = std::env::temp_dir().join(format!(
        "pitboard-crash-codex-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".codex")).expect("a scratch codex home");

    let mem = MemoryHost::new();
    let api = ScriptedApi::new();
    let ctx = Context::new(root.clone())
        .with_pitboard_home(root.join(".pitboard"))
        .with_codex_home(root.join(".codex").to_string_lossy().into_owned())
        .with_memory_stores(Arc::clone(&mem))
        .with_scripted_api(Arc::clone(&api))
        .with_clock(Arc::new(FixedClock::at(NOW)) as Arc<dyn Clock>);
    let machine = Machine {
        ctx,
        mem,
        api,
        root,
        service: String::new(),
        which: ProviderId::Codex,
    };
    machine.sign_in(&codex_login("here", "here-refresh"));
    for refresh in ["here-refresh", "there-refresh"] {
        machine.api.using(
            &codex_access(refresh),
            crate::usage::Snapshot {
                windows: Vec::new(),
                observed_at: Some(NOW),
                account_uuid: None,
                source: crate::usage::Source::Live,
                verified: true,
            },
        );
    }

    std::fs::create_dir_all(machine.root.join(".pitboard")).expect("a Pitboard home");
    let parked_service = park::reserve(&machine.ctx, &codex_id("there")).expect("a free name");
    let parked = park::store_at(
        &machine.ctx,
        ProviderId::Codex,
        &parked_service,
        &codex_login("there", "there-refresh"),
    )
    .expect("parked");

    let mut state = State::default();
    state
        .accounts
        .push(codex_account("here", &codex_id("here"), None));
    state
        .accounts
        .push(codex_account("there", &codex_id("there"), Some(parked)));
    state.set_active(ProviderId::Codex, Some("here".into()));
    state::save(&machine.ctx, &state).expect("saved");
    machine
}

/// A login of the account `who` as this machine's tool writes one, with the service taught
/// whose it is where the tool has to ask.
pub(crate) fn login_of(m: &Machine, who: &str, refresh: &str) -> Value {
    match m.which {
        ProviderId::Claude => {
            m.api.owned_by(&format!("access-{refresh}"), owner(who));
            document(refresh)
        }
        ProviderId::Codex => codex_login(who, refresh),
        ProviderId::Desktop => unreachable!("no machine keeps Claude Desktop in a vault"),
    }
}

/// A sign-in the tool finished as `who`, left where a finished one leaves its login.
pub(crate) fn signed_in(m: &Machine, who: &str, refresh: &str) -> enroll::SignIn {
    enroll::planted(&m.ctx, m.which, login_of(m, who, refresh)).expect("a sign-in")
}

/// The service answers a renewal of the login on `refresh` with one on `renewed`.
pub(crate) fn renews(m: &Machine, refresh: &str, renewed: &str) {
    match m.which {
        ProviderId::Claude => {
            m.api.renews(
                refresh,
                crate::api::Renewed {
                    access_token: format!("access-{renewed}"),
                    refresh_token: Some(renewed.into()),
                    expires_in: 3600,
                    refresh_token_expires_in: Some(30 * 86_400),
                    scopes: None,
                    at: None,
                },
            );
        }
        ProviderId::Codex => {
            m.api.codex_renews(
                refresh,
                crate::provider::codex::api::Fresh {
                    id_token: None,
                    access_token: Some(codex_access(renewed)),
                    refresh_token: Some(renewed.into()),
                    at: Some(NOW),
                },
            );
        }
        ProviderId::Desktop => unreachable!("no machine keeps Claude Desktop in a vault"),
    }
}

pub(crate) fn account(label: &str, uuid: &str, parked: Option<Park>) -> Account {
    Account {
        last_used_at: None,
        label: label.into(),
        account_uuid: uuid.into(),
        email: format!("{uuid}@example.com"),
        parked,
        detail: crate::state::Detail::Claude {
            organization_uuid: format!("org-{uuid}"),
            oauth_account: json!({
                "accountUuid": uuid,
                "emailAddress": format!("{uuid}@example.com"),
                "organizationUuid": format!("org-{uuid}"),
            }),
        },
    }
}

/// Everything that must be true after a killed change has been recovered, whatever the
/// change was and wherever it died.
pub(crate) fn hold(m: &Machine, after: &str) {
    let state = state::load(&m.ctx)
        .unwrap_or_else(|e| panic!("{after}: the state file must still parse, got {e}"));

    // Nothing in the vault that the state does not name. An item nobody names holds a live
    // refresh token that no command will ever renew or delete, and on macOS nothing can
    // even list it.
    let named: HashSet<&str> = state
        .accounts
        .iter()
        .filter_map(|a| a.parked.as_ref())
        .map(|p| p.service.as_str())
        .chain(state.discarded.iter().map(String::as_str))
        .collect();
    for service in m.mem.vault().services() {
        assert!(
            named.contains(service.as_str()),
            "{after}: {service} holds a login nothing on this machine names"
        );
    }

    // One refresh token, one place. A token in two places is a token one holder will rotate
    // past, which ends the login for the other; for a tool whose sign-out revokes what it
    // finds, it is a token the person's own next sign-out kills in both.
    let tool = crate::provider::of(m.which);
    let mut seen: HashSet<String> = HashSet::new();
    let mut fingerprints = Vec::new();
    for service in m.mem.vault().services() {
        let raw = m.mem.vault().peek(&service).expect("just listed");
        let value: Value = serde_json::from_str(&raw).expect("a park is JSON");
        fingerprints.push((service, tool.fingerprint(&value)));
    }
    let live = m.live();
    if let Some(document) = &live {
        fingerprints.push(("the live slot".into(), tool.fingerprint(document)));
    }
    for (place, fingerprint) in fingerprints {
        assert!(
            seen.insert(fingerprint.clone()),
            "{after}: the login in {place} is also somewhere else"
        );
    }

    // Every account can still be got back to. Signed in, or holding a login that can be
    // restored, or holding nothing and saying so, but never holding one that has expired.
    let live_uuid = live
        .and_then(|document| {
            tool.identify(&m.ctx, &crate::provider::Credential::new(m.which, document))
                .ok()
        })
        .map(|found| found.account_id);
    for a in &state.accounts {
        if Some(&a.account_uuid) == live_uuid.as_ref() {
            continue;
        }
        if let Some(park) = &a.parked {
            assert!(
                park.restorable_at(NOW),
                "{after}: {} holds a login that can no longer be restored",
                a.label
            );
            assert!(
                m.mem.vault().peek(&park.service).is_some(),
                "{after}: {} names a park that is not in the vault",
                a.label
            );
        }
    }
}

/// Recovery, run the way the next command runs it.
pub(crate) fn recover(m: &Machine) -> Result<()> {
    settle(&m.ctx, None).map(|_| ())
}

/// Every place a Claude Desktop switch can be killed, in the order it reaches them.
pub(crate) const TREE_POINTS: [&str; 9] = [
    "tree.journal_written",
    "tree.item_parked",
    "tree.live_parked",
    "tree.park_stored",
    "tree.park_recorded",
    "tree.item_installed",
    "tree.installed",
    "tree.config_spliced",
    "tree.recorded",
];

/// Where Claude Desktop runs from, as a test says it is running.
pub(crate) const APP_PATH: &str = "/Applications/Claude.app/Contents/MacOS/Claude";

/// A cookie jar holding one session, whose ciphertext is `value`.
pub(crate) fn jar(value: &str, expires_at: i64) -> crate::provider::desktop::types::CookieTable {
    use crate::provider::desktop::types::{CookieRow, CookieTable};
    CookieTable {
        meta_version: 24,
        rows: vec![CookieRow {
            host_key: ".claude.ai".into(),
            name: "sessionKey".into(),
            encrypted_value: value.as_bytes().to_vec(),
            expires_utc: (expires_at + 11_644_473_600) * 1_000_000,
        }],
    }
}

/// The items each account of a Claude Desktop machine holds, wherever it is.
fn desktop_items(label: &str) -> &'static [&'static str] {
    match label {
        "here" => &[
            "Cookies",
            "Cookies-journal",
            "Local Storage",
            "Session Storage",
            "IndexedDB/https_claude.ai_0.indexeddb.leveldb",
            "WebStorage",
        ],
        _ => &["Cookies", "Local Storage", "File System"],
    }
}

/// How whole an account of a Claude Desktop machine is.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Whole {
    /// Signed in to the app, every item of it in the data folder.
    Live,
    /// Parked, every item of it in its park, its session the one the state names.
    Parked,
    /// Neither, and why.
    Broken(String),
}

/// A Claude Desktop machine: `here` signed in to the app, `there` parked and ready.
pub(crate) struct DesktopMachine {
    pub(crate) ctx: Context,
    pub(crate) mem: Arc<MemoryHost>,
    root: PathBuf,
}

impl Drop for DesktopMachine {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn write_file(path: &std::path::Path, contents: &[u8]) {
    std::fs::create_dir_all(path.parent().expect("a file in a folder")).expect("a folder");
    std::fs::write(path, contents).expect("a file");
}

/// Every regular file under `dir`, by path.
pub(crate) fn files_under(dir: &std::path::Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            files_under(&path, found);
        } else if kind.is_file() {
            found.push(path);
        }
    }
}

impl DesktopMachine {
    pub(crate) fn key(&self, label: &str) -> Key {
        Key::new(ProviderId::Desktop, label)
    }

    /// The app's data folder.
    pub(crate) fn support(&self) -> PathBuf {
        self.root.join("Claude")
    }

    /// The name of `there`'s park as the machine starts.
    pub(crate) fn there_park(&self) -> String {
        format!(
            "{}there-{}",
            crate::provider::desktop::paths::PARK_PREFIX,
            NOW * 1000 - 1000
        )
    }

    /// Somebody signed in to the account `uuid` in the app, with a session of `value`.
    pub(crate) fn plant_live(&self, uuid: &str, value: &str) {
        let support = self.support();
        let cookies = support.join("Cookies");
        if !cookies.exists() {
            write_file(&cookies, b"");
        }
        self.mem
            .plant_cookies(&cookies, jar(value, NOW + 30 * 86_400));
        let path = support.join("config.json");
        let mut config: Value = std::fs::read(&path)
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_else(|| json!({}));
        config["lastKnownAccountUuid"] = json!(uuid);
        config["oauth:tokenCache"] = json!(format!("cache-{uuid}"));
        std::fs::write(&path, config.to_string()).expect("a config");
    }

    /// Every regular file Pitboard and the app hold between them, by where it is under the
    /// machine, and its inode. Pitboard's own records are left out: they are rewritten, not
    /// moved.
    pub(crate) fn inodes(&self) -> std::collections::BTreeMap<PathBuf, u64> {
        use std::os::unix::fs::MetadataExt;
        let mut found = Vec::new();
        let desktop = crate::provider::desktop::paths::desktop_home(&self.ctx);
        for dir in [
            self.support(),
            desktop.join("parks"),
            desktop.join("strays"),
        ] {
            files_under(&dir, &mut found);
        }
        found
            .into_iter()
            .filter(|path| {
                !matches!(
                    path.file_name().and_then(|n| n.to_str()),
                    Some("config.json" | "config-keys.json" | "manifest.json")
                )
            })
            .map(|path| {
                let inode = std::fs::symlink_metadata(&path).expect("just listed").ino();
                let relative = path.strip_prefix(&self.root).unwrap_or(&path).to_path_buf();
                (relative, inode)
            })
            .collect()
    }

    /// Whether anything set aside in the strays directory is the file `inode`.
    pub(crate) fn strays_hold(&self, inode: u64) -> bool {
        use std::os::unix::fs::MetadataExt;
        let mut found = Vec::new();
        files_under(
            &crate::provider::desktop::paths::strays_dir(&self.ctx),
            &mut found,
        );
        found
            .iter()
            .any(|path| std::fs::symlink_metadata(path).is_ok_and(|m| m.ino() == inode))
    }

    /// Where the account under `label` is, and whether all of it is there.
    pub(crate) fn whole(&self, label: &str) -> Whole {
        use crate::provider::desktop::{identity, paths};
        let state = state::load(&self.ctx).expect("the state parses");
        let key = self.key(label);
        let Some(account) = state.get(&key) else {
            return Whole::Broken(format!("{label} is not enrolled"));
        };
        let support = self.support();
        let live = identity::identify_tree(&self.ctx, &support).ok().flatten();
        if live
            .as_ref()
            .is_some_and(|found| found.account_uuid == account.account_uuid)
        {
            if state.active_for(ProviderId::Desktop) != Some(label) {
                return Whole::Broken(format!("{label} is signed in but not active"));
            }
            if account.parked.is_some() {
                return Whole::Broken(format!("{label} is signed in and parked"));
            }
            for item in desktop_items(label) {
                if !support.join(item).exists() {
                    return Whole::Broken(format!("{label} is signed in without {item}"));
                }
            }
            return Whole::Live;
        }
        let Some(park) = &account.parked else {
            return Whole::Broken(format!("{label} is neither signed in nor parked"));
        };
        if state.active_for(ProviderId::Desktop) == Some(label) {
            return Whole::Broken(format!("{label} is parked but active"));
        }
        let dir = paths::parks_dir(&self.ctx).join(&park.service);
        for item in desktop_items(label) {
            if !dir.join(item).exists() {
                return Whole::Broken(format!("{label} is parked without {item}"));
            }
        }
        match identity::session_of(&self.ctx, &dir) {
            Ok(Some(session)) if session.fingerprint == park.refresh_fingerprint => Whole::Parked,
            other => Whole::Broken(format!("{label}'s park holds {other:?}")),
        }
    }

    /// Switch to `there`, killed at `point`. `Err(point)` where it died there.
    pub(crate) fn crash_at(&self, point: &'static str) -> std::result::Result<(), String> {
        fault::killing(point, || {
            let (settled, _) = settle(&self.ctx, Some(ProviderId::Desktop))?;
            switch(settled, &self.key("there"))
        })
        .map(|_| ())
    }

    /// Recovery, run the way the next Claude Desktop command runs it.
    pub(crate) fn recover(&self) -> Result<Option<Recovered>> {
        let (_, found) = settle(&self.ctx, Some(ProviderId::Desktop))?;
        Ok(found.into_iter().find_map(|warning| match warning {
            Warning::Recovered(r) => Some(r),
            _ => None,
        }))
    }
}

/// `here` signed in to Claude Desktop, with the machine's own files beside its login, and
/// `there` parked the way a switch parks one.
pub(crate) fn desktop_machine(name: &str) -> DesktopMachine {
    use crate::provider::desktop::{identity, paths};
    use crate::state::Detail;
    let root = std::env::temp_dir().join(format!(
        "pitboard-crash-desktop-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let support = root.join("Claude");
    std::fs::create_dir_all(&support).expect("a scratch data folder");
    let mem = MemoryHost::new();
    let ctx = Context::new(root.clone())
        .with_pitboard_home(root.join(".pitboard"))
        .with_desktop_dir(support.to_string_lossy().into_owned())
        .with_memory_stores(Arc::clone(&mem))
        .with_clock(Arc::new(FixedClock::at(NOW)) as Arc<dyn Clock>);
    let m = DesktopMachine { ctx, mem, root };
    crate::store::tree::ensure_private_dir(&m.root.join(".pitboard")).expect("a Pitboard home");

    // `here`'s login, and what belongs to the machine beside it.
    for file in [
        "Cookies",
        "Cookies-journal",
        "Local Storage/leveldb/000003.log",
        "Session Storage/000003.log",
        "IndexedDB/https_claude.ai_0.indexeddb.leveldb/000003.log",
        "WebStorage/QuotaManager",
        "claude_desktop_config.json",
        "plan-usage-history.json",
        "Cache/data_0",
        "IndexedDB/https_other.example_0.indexeddb.leveldb/000003.log",
    ] {
        write_file(&support.join(file), file.as_bytes());
    }
    m.mem
        .plant_cookies(&support.join("Cookies"), jar("v10here", NOW + 30 * 86_400));
    std::fs::write(
        support.join("config.json"),
        json!({
            "lastKnownAccountUuid": "here",
            "oauth:tokenCache": "cache-here",
            "oauth:tokenCacheV2": "cache2-here",
            "locale": "en-US",
        })
        .to_string(),
    )
    .expect("a config");

    // `there`'s park, as a switch leaves one.
    let parks = paths::parks_dir(&m.ctx);
    crate::store::tree::ensure_private_dir(&paths::desktop_home(&m.ctx)).expect("a home");
    crate::store::tree::ensure_private_dir(&parks).expect("a parks dir");
    let dir = parks.join(m.there_park());
    crate::store::tree::ensure_private_dir(&dir).expect("a park");
    for file in [
        "Cookies",
        "Local Storage/leveldb/000005.log",
        "File System/000/t/00",
    ] {
        write_file(&dir.join(file), file.as_bytes());
    }
    m.mem
        .plant_cookies(&dir.join("Cookies"), jar("v10there", NOW + 30 * 86_400));
    let there = identity::session_of(&m.ctx, &dir)
        .expect("a readable jar")
        .expect("a session");
    super::tree::write_secret_json(
        &dir.join(super::tree::CONFIG_KEYS_FILE),
        &json!({"lastKnownAccountUuid": "there", "oauth:tokenCache": "cache-there"}),
    )
    .expect("the park's keys");
    super::tree::write_secret_json(
        &dir.join(super::tree::MANIFEST_FILE),
        &super::tree::Manifest {
            account_uuid: "there".into(),
            fingerprint: there.fingerprint.clone(),
            items: desktop_items("there")
                .iter()
                .map(|s| s.to_string())
                .collect(),
            parked_at: NOW - 1,
        },
    )
    .expect("the park's manifest");

    let here = identity::session_of(&m.ctx, &support)
        .expect("a readable jar")
        .expect("a session");
    let desktop = |label: &str, fingerprint: &str, parked: Option<Park>| Account {
        label: label.into(),
        account_uuid: label.into(),
        email: String::new(),
        parked,
        last_used_at: None,
        detail: Detail::Desktop {
            organization_uuid: None,
            session_fingerprint: fingerprint.into(),
            session_expires_at: Some(NOW + 30 * 86_400),
        },
    };
    let mut state = State::default();
    state
        .accounts
        .push(desktop("here", &here.fingerprint, None));
    state.accounts.push(desktop(
        "there",
        &there.fingerprint,
        Some(Park {
            service: m.there_park(),
            parked_at: NOW - 1,
            refresh_fingerprint: there.fingerprint.clone(),
            access_expires_at: there.expires_at,
            refresh_expires_at: there.expires_at,
        }),
    ));
    state.set_active(ProviderId::Desktop, Some("here".into()));
    state::save(&m.ctx, &state).expect("saved");
    m
}
