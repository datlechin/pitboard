//! A machine to run changes against: two accounts of one tool, stores in memory, services
//! that answer from a script, a clock that stands still, and Claude Code's locks given up on
//! at once.
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
use crate::api::scripted::ScriptedApi;
use crate::context::ScratchMachine;
use crate::permit::Permit;
use crate::provider::Owner;
use crate::provider::ProviderId;
use crate::provider::claude::api::Renewed;
use crate::provider::claude::paths as claude;

use crate::host::memory::MemoryHost;
use crate::state::Account;
use crate::time::{Clock, FixedClock};
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::mpsc::RecvTimeoutError;

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

// The tools whose login is a document, for a test's match on a document machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tool {
    Claude,
    Codex,
}

impl Tool {
    pub(crate) fn of(which: ProviderId) -> Tool {
        match which {
            ProviderId::Claude => Tool::Claude,
            ProviderId::Codex => Tool::Codex,
            ProviderId::Desktop => panic!("{which} keeps no login document"),
        }
    }
}

pub(crate) struct Rig {
    pub(crate) ctx: Context,
    pub(crate) mem: Arc<MemoryHost>,
    pub(crate) api: Arc<ScriptedApi>,
    root: tempfile::TempDir,
    /// The keychain item Claude Code's login is in, on a Claude Code machine.
    pub(crate) service: String,
    /// Which tool's accounts this machine holds.
    pub(crate) which: ProviderId,
}

impl Rig {
    /// Where the tool's own files are, for a test that needs to change one.
    pub(crate) fn ctx_home(&self) -> PathBuf {
        self.root.path().to_path_buf()
    }

    // Pitboard's directory moved to the one the schedule renews, which on Windows is not
    // `.pitboard`.
    pub(crate) fn in_the_default_home(mut self) -> Rig {
        let (here, default) = (home::dir(&self.ctx), self.ctx.default_pitboard_home());
        if here != default {
            std::fs::create_dir_all(default.parent().expect("its folder")).expect("made");
            std::fs::rename(&here, &default).expect("moved");
            self.ctx = self.ctx.clone().with_pitboard_home(default);
        }
        self
    }

    /// The live login, read the way the tool reads it.
    pub(crate) fn live(&self) -> Option<Value> {
        provider::document(self.which)
            .read_live(&self.ctx)
            .ok()
            .flatten()
            .map(|credential| credential.raw)
    }

    /// Replace the live login, the way the tool itself would write it.
    pub(crate) fn sign_in(&self, document: &Value) {
        let live = provider::document(self.which)
            .live(&self.ctx)
            .expect("a store to write to");
        store::write_raw(
            &live.chain,
            Permit::for_a_test(),
            &live.service,
            &document.to_string(),
        )
        .expect("the live login is written");
    }

    pub(crate) fn key(&self, label: &str) -> Key {
        Key::new(self.which, label)
    }

    /// From now on the live login's store misbehaves this way: the keychain item for Claude
    /// Code, the `auth.json` file for Codex.
    pub(crate) fn fault_live(&self, fault: store::memory::Fault) {
        let (store, service) = self.live_store();
        store.fault(&service, fault);
    }

    /// The store the live login is in, and its name there, for a test that has to fault it
    /// from inside a change.
    pub(crate) fn live_store(&self) -> (Arc<store::memory::MemoryStore>, String) {
        let live = provider::document(self.which)
            .live(&self.ctx)
            .expect("a live store");
        let store = match Tool::of(self.which) {
            Tool::Claude => Arc::clone(self.mem.live()),
            Tool::Codex => self
                .mem
                .file_at(provider::codex::paths::auth_file(&self.ctx)),
        };
        (store, live.service)
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

/// [`document`] with its access token expired a minute ago.
pub(crate) fn lapsed(refresh: &str) -> Value {
    let mut login = document(refresh);
    login["claudeAiOauth"]["expiresAt"] = json!((NOW - 60) * 1000);
    login
}

pub(crate) fn owner(uuid: &str) -> Owner {
    Owner {
        account_uuid: uuid.into(),
        email: format!("{uuid}@example.com"),
        organization_uuid: format!("org-{uuid}"),
    }
}

// Named after the test, so a path in a failure says whose it is.
fn scratch_home(name: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("pitboard-{name}-"))
        .tempdir()
        .expect("a scratch home")
}

// `here` signed in and `there` parked and ready: the shape every switch starts from.
pub(crate) fn machine(name: &str) -> Rig {
    let scratch = scratch_home(name);
    let root = scratch.path();

    let ScratchMachine {
        ctx,
        host: mem,
        api,
    } = ScratchMachine::at(root, |ctx| {
        ctx.with_pitboard_home(root.join(".pitboard"))
            .with_clock(Arc::new(FixedClock::at(NOW)) as Arc<dyn Clock>)
            // A test holding one of Claude Code's locks lets go of it at a point, not after a while.
            .with_lock_waiting(1, std::time::Duration::from_millis(1))
    });

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
    let parked_service = park::reserve(&ctx, Permit::for_a_test(), "there").expect("a free name");
    let parked = park::store_at(
        &ctx,
        Permit::for_a_test(),
        provider::document(crate::provider::ProviderId::Claude),
        &parked_service,
        &oauth("there-refresh", 30),
    )
    .expect("parked");

    let mut state = State::default();
    state.accounts.push(account("here", "here", None));
    state.accounts.push(account("there", "there", Some(parked)));
    record_in_use(
        &ctx,
        &mut state,
        ProviderId::Claude,
        &document("here-refresh"),
    );
    state::save(&ctx, Permit::for_a_test(), &state).expect("saved");

    Rig {
        ctx,
        mem,
        api,
        root: scratch,
        service,
        which: ProviderId::Claude,
    }
}

/// [`machine`], where `elsewhere`, enrolled with nothing parked, has since signed in to
/// Claude Code outside Pitboard over `here`, whose only login that was, as `/login` does:
/// its login in the keychain and its account in the config.
pub(crate) fn signed_in_outside(name: &str) -> Rig {
    let m = machine(name);
    m.api
        .owned_by("access-elsewhere-refresh", owner("elsewhere"));
    let mut state = state::load(&m.ctx).expect("state");
    state.accounts.push(account("elsewhere", "elsewhere", None));
    state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
    m.sign_in(&document("elsewhere-refresh"));
    config_names(&m, "elsewhere");
    m
}

/// Claude Code's config naming `who`, as another Claude Code process that started or signed
/// in on `who`'s login writes it, with the login in the keychain left as it was.
pub(crate) fn config_names(m: &Rig, who: &str) {
    let path = m.root.path().join(".claude.json");
    let mut config: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("a config")).expect("JSON");
    let owner = owner(who);
    config["oauthAccount"] = json!({
        "accountUuid": owner.account_uuid,
        "emailAddress": owner.email,
        "organizationUuid": owner.organization_uuid,
    });
    std::fs::write(&path, config.to_string()).expect("the config is written");
}

/// The activity log's lines of `verb`, as subject and outcome.
pub(crate) fn audit_lines(m: &Rig, verb: &str) -> Vec<(String, String)> {
    crate::audit::read(&m.ctx, 100)
        .into_iter()
        .filter(|entry| entry.verb == verb)
        .map(|entry| (entry.subject, entry.outcome))
        .collect()
}

/// `state.json` as it is on disk, with when it was last written.
pub(crate) fn state_file(m: &Rig) -> (Vec<u8>, std::time::SystemTime) {
    let path = home::dir(&m.ctx).join("state.json");
    let written = std::fs::metadata(&path)
        .and_then(|meta| meta.modified())
        .expect("a state file");
    (std::fs::read(&path).expect("a state file"), written)
}

/// A Codex login as `codex login` writes one, for the account `who`.
///
/// The access token is unique to the refresh token so every login has its own, which is
/// what the scripted usage answers are keyed by.
pub(crate) fn codex_login(who: &str, refresh: &str) -> Value {
    provider::codex::login_for_a_test(
        who,
        &format!("{who}@example.com"),
        &codex_access(refresh),
        refresh,
    )
}

/// The identity Codex's own claims give the account `who`: the ChatGPT account with the
/// person inside it.
pub(crate) fn codex_id(who: &str) -> String {
    format!("{who}_user-{who}")
}

/// The access token [`codex_login`] carries for this refresh token.
pub(crate) fn codex_access(refresh: &str) -> String {
    provider::jwt::unsigned(&json!({"exp": NOW + 10 * 86_400, "for": refresh}))
}

/// `who`'s Codex account, as enrolling it from [`codex_login`] writes one: its ChatGPT
/// account is its workspace, as Codex's own claims give it.
pub(crate) fn codex_account(label: &str, who: &str, parked: Option<Park>) -> Account {
    Account {
        last_used_at: None,
        replaced_at: None,
        plan: Some("Pro".into()),
        label: label.into(),
        id: codex_id(who),
        account_uuid: codex_id(who),
        email: format!("{who}@example.com"),
        parked,
        detail: state::Detail::Codex {
            workspace_id: Some(who.into()),
        },
    }
}

pub(crate) use crate::desktop_testing::{HOME, WORK};

pub(crate) const STRANGER: crate::desktop_testing::Person = crate::desktop_testing::Person {
    uuid: "0b7e2c44-2222-4d1f-8a35-91c0de77aa01",
    session: "stranger-session",
};

// Claude closed and signed in to `work`, enrolled as `desktop/work` by the real `enroll`.
pub(crate) fn desktop_machine(name: &str) -> Rig {
    let scratch = scratch_home(name);
    let root = scratch.path();
    let ScratchMachine {
        ctx,
        host: mem,
        api,
    } = ScratchMachine::at(root, |ctx| {
        ctx.with_pitboard_home(root.join(".pitboard"))
            .with_clock(Arc::new(FixedClock::at(NOW)) as Arc<dyn Clock>)
            .with_awaited_every(std::time::Duration::from_millis(2))
            .with_scratch_claude(root.join("claude"))
    });
    let m = Rig {
        ctx,
        mem,
        api,
        root: scratch,
        service: String::new(),
        which: ProviderId::Desktop,
    };
    m.claude().signed_in_as(&WORK);
    let (settled, _) = settle(&m.ctx, Permit::for_a_test(), Some(ProviderId::Desktop))
        .expect("nothing to recover");
    enroll(settled, &m.key("work"), None).expect("`work` enrolled");
    m
}

// `work` in use and `home` parked, by the real sign-in inside Claude and the real switch.
pub(crate) fn desktop_pair(name: &str) -> Rig {
    let m = desktop_machine(name);
    sign_in_inside(&m, "home", &HOME).expect("`home` added, `work` parked");
    let (settled, _) = settle(&m.ctx, Permit::for_a_test(), Some(ProviderId::Desktop))
        .expect("nothing to recover");
    switch(settled, &m.key("work")).expect("back to `work`");
    m
}

// `pitboard enroll desktop/<label> --sign-in`, with Claude opened, signed in to `who` and quit
// once the wait has said it saw Claude open.
pub(crate) fn sign_in_inside(
    m: &Rig,
    label: &str,
    who: &crate::desktop_testing::Person,
) -> Result<(Enrolled, Vec<Warning>)> {
    let typed = format!("desktop/{label}");
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());
    let watched = pitboard
        .sign_in_watched(&typed)
        .map_err(|failed| failed.error)?;
    let said = watched.said();
    let claude = m.claude();
    claude.opens();
    let wait = std::time::Duration::from_secs(10);
    loop {
        match said.within(wait) {
            Ok(line) if line == "Claude is open.\n" => break,
            Ok(_) => {}
            Err(why) => panic!("the wait never said Claude is open: {why}"),
        }
    }
    claude.signs_in(who);
    claude.reports();
    claude.quits();
    // Its lines end with its thread, which `finish` would otherwise wait for without end.
    loop {
        match said.within(wait) {
            Ok(_) => {}
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => panic!("the wait never ended once Claude quit"),
        }
    }
    let login = watched.finish()?;
    pitboard
        .enroll_signed_in(&typed, login)
        .map(|done| (done.value, done.warnings))
        .map_err(|failed| failed.error)
}

pub(crate) fn sessions(m: &Rig) -> Vec<(String, String)> {
    let LoginKind::Folder(app) = provider::of(m.which) else {
        return Vec::new();
    };
    let mut places = vec![(
        "Claude".to_string(),
        m.root.path().join("claude/data/Cookies"),
    )];
    for name in store::folder::list(&m.ctx)
        .ok()
        .flatten()
        .unwrap_or_default()
    {
        let jar = store::folder::item(&m.ctx, &name, app.items()[0]);
        places.push((name, jar));
    }
    places
        .into_iter()
        .filter(|(_, jar)| jar.exists())
        .filter_map(|(place, jar)| Some((place, app.session(&m.ctx, &jar).ok()??.login)))
        .collect()
}

impl Rig {
    pub(crate) fn claude(&self) -> crate::desktop_testing::FakeClaude {
        crate::desktop_testing::FakeClaude::on(
            Arc::clone(&self.mem),
            &self.ctx,
            crate::desktop_testing::JarAs::StandIn,
        )
    }
}

// The same for Codex, OpenAI answering for both.
pub(crate) fn codex_machine(name: &str) -> Rig {
    let scratch = scratch_home(name);
    let root = scratch.path();
    std::fs::create_dir_all(root.join(".codex")).expect("a scratch codex home");

    let ScratchMachine {
        ctx,
        host: mem,
        api,
    } = ScratchMachine::at(root, |ctx| {
        ctx.with_pitboard_home(root.join(".pitboard"))
            .with_codex_home(root.join(".codex").to_string_lossy().into_owned())
            .with_clock(Arc::new(FixedClock::at(NOW)) as Arc<dyn Clock>)
    });
    let machine = Rig {
        ctx,
        mem,
        api,
        root: scratch,
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
                answered_at: Some(NOW),
                lists_every_limit: false,
                source: crate::usage::Source::Live,
            },
        );
    }

    std::fs::create_dir_all(machine.root.path().join(".pitboard")).expect("a Pitboard home");
    let parked_service =
        park::reserve(&machine.ctx, Permit::for_a_test(), &codex_id("there")).expect("a free name");
    let parked = park::store_at(
        &machine.ctx,
        Permit::for_a_test(),
        provider::document(ProviderId::Codex),
        &parked_service,
        &codex_login("there", "there-refresh"),
    )
    .expect("parked");

    let mut state = State::default();
    state.accounts.push(codex_account("here", "here", None));
    state
        .accounts
        .push(codex_account("there", "there", Some(parked)));
    record_in_use(
        &machine.ctx,
        &mut state,
        ProviderId::Codex,
        &codex_login("here", "here-refresh"),
    );
    state::save(&machine.ctx, Permit::for_a_test(), &state).expect("saved");
    machine
}

/// `here`'s login on `login`, recorded as its service said it now, with what the tool's own
/// record names: what enrolling `here` while signed in records. When `here` came to be in use
/// stays unrecorded, so no test starts inside the minutes the automatic switch leaves an
/// account to settle.
pub(crate) fn record_in_use(ctx: &Context, state: &mut State, which: ProviderId, login: &Value) {
    let here = state
        .get(&Key::new(which, "here"))
        .expect("`here` is enrolled");
    let found = InUse {
        owner: Some(here.owner()),
        login: provider::document(which).fingerprint(login),
        known_at: NOW,
        named: in_use::named(ctx, which),
    };
    state.in_use.insert(which.code().to_string(), found);
}

/// A login of the account `who` as this machine's tool writes one, with the service taught
/// whose it is where the tool has to ask.
pub(crate) fn login_of(m: &Rig, who: &str, refresh: &str) -> Value {
    match Tool::of(m.which) {
        Tool::Claude => {
            m.api.owned_by(&format!("access-{refresh}"), owner(who));
            document(refresh)
        }
        Tool::Codex => codex_login(who, refresh),
    }
}

/// A sign-in the tool finished as `who`, left where a finished one leaves its login.
pub(crate) fn signed_in(m: &Rig, who: &str, refresh: &str) -> SignIn {
    planted(
        &m.ctx,
        Permit::for_a_test(),
        provider::document(m.which),
        login_of(m, who, refresh),
    )
    .expect("a sign-in")
}

/// The service answers a renewal of the login on `refresh` with one on `renewed`.
pub(crate) fn renews(m: &Rig, refresh: &str, renewed: &str) {
    match Tool::of(m.which) {
        Tool::Claude => {
            m.api.renews(
                refresh,
                Renewed {
                    access_token: format!("access-{renewed}"),
                    refresh_token: Some(renewed.into()),
                    expires_in: 3600,
                    refresh_token_expires_in: Some(30 * 86_400),
                    scopes: None,
                    at: None,
                    owner: None,
                },
            );
        }
        Tool::Codex => {
            m.api.codex_renews(
                refresh,
                provider::codex::api::Fresh {
                    id_token: None,
                    access_token: Some(codex_access(renewed)),
                    refresh_token: Some(renewed.into()),
                    at: Some(NOW),
                },
            );
        }
    }
}

/// What a Claude Code session did while renewing the login a store holds, as
/// [`renews_meanwhile`] plays it.
#[derive(Default)]
pub(crate) struct Session {
    /// It found Claude Code's refresh lock held, so it waited.
    pub(crate) waited: std::cell::Cell<bool>,
    /// The refresh token it sent, and the login it was renewed to, to save once Claude Code's
    /// write lock is let go of.
    saving: std::cell::RefCell<Option<(String, Value)>>,
}

impl Session {
    pub(crate) fn new() -> std::rc::Rc<Session> {
        std::rc::Rc::new(Session::default())
    }
}

/// Where `target`'s lock is, as [`lock::acquire`] and proper-lockfile make it.
pub(crate) fn lock_dir(target: &std::path::Path) -> PathBuf {
    PathBuf::from(format!("{}.lock", target.display()))
}

/// What Claude Code's write lock guards.
pub(crate) fn write_target(m: &Rig) -> PathBuf {
    provider::document(ProviderId::Claude)
        .write_lock(&m.ctx)
        .expect("Claude Code takes one")
}

/// Takes Claude Code's write lock at the moment it is called, as a session writing its
/// credentials does.
pub(crate) fn takes_the_write_lock(m: &Rig) -> impl FnOnce() + 'static {
    let writing = lock_dir(&write_target(m));
    move || {
        let parent = writing.parent().expect("the storage directory");
        std::fs::create_dir_all(parent).expect("the storage directory is made");
        std::fs::create_dir(&writing).expect("the write lock is free");
    }
}

/// Lets go of Claude Code's write lock at the moment it is called, as a session that has
/// written does.
pub(crate) fn lets_go_of_the_write_lock(m: &Rig) -> impl FnOnce() + 'static {
    let writing = lock_dir(&write_target(m));
    move || std::fs::remove_dir(&writing).expect("the session lets go")
}

/// A session renewing the login `store` holds at the moment it is called, as Claude Code
/// 2.1.294 does (the register's `refresh_lock`): the keychain's, or the file's for a session
/// that signed in with the file. It takes the refresh lock, as a directory, without waiting
/// here: held, it waits, which is all that happens. Taken, it sends the refresh token stored,
/// which spends it, then saves the renewed login, with an access token good for an hour,
/// under the write lock where it can take it, and otherwise once [`saves`] says it is let go
/// of.
pub(crate) fn renews_meanwhile(
    m: &Rig,
    store: Arc<store::memory::MemoryStore>,
    session: &std::rc::Rc<Session>,
) -> impl FnOnce() + 'static {
    let (service, api) = (m.service.clone(), Arc::clone(&m.api));
    let refreshing = lock_dir(&claude::refresh_lock(&m.ctx));
    let writing = lock_dir(&write_target(m));
    let session = std::rc::Rc::clone(session);
    move || {
        let parent = refreshing.parent().expect("the storage directory");
        std::fs::create_dir_all(parent).expect("the storage directory is made");
        match std::fs::create_dir(&refreshing) {
            Ok(()) => {}
            Err(held) if held.kind() == std::io::ErrorKind::AlreadyExists => {
                session.waited.set(true);
                return;
            }
            Err(error) => panic!("the refresh lock: {error}"),
        }
        let held = store.peek(&service).expect("a login is stored");
        let mut login: Value = serde_json::from_str(&held).expect("JSON");
        let sent = login["claudeAiOauth"]["refreshToken"]
            .as_str()
            .expect("a refresh token")
            .to_string();
        api.renew_trouble(&sent, crate::api::scripted::Trouble::InvalidGrant);
        login["claudeAiOauth"]["refreshToken"] = json!(format!("{sent}-by-a-session"));
        login["claudeAiOauth"]["accessToken"] = json!(format!("access-{sent}-by-a-session"));
        login["claudeAiOauth"]["expiresAt"] = json!((NOW + 3600) * 1000);
        *session.saving.borrow_mut() = Some((sent, login));
        if std::fs::create_dir(&writing).is_ok() {
            save(&store, &service, &session);
            let _ = std::fs::remove_dir(&writing);
            let _ = std::fs::remove_dir(&refreshing);
        }
    }
}

/// The session's save, held up by Claude Code's write lock, once that is let go of: only
/// where `store` still holds the refresh token it sent, as Claude Code's save compares it, so
/// nothing where that login has gone.
pub(crate) fn saves(m: &Rig, store: &store::memory::MemoryStore, session: &Session) {
    if session.saving.borrow().is_none() {
        return;
    }
    save(store, &m.service, session);
    let _ = std::fs::remove_dir(lock_dir(&claude::refresh_lock(&m.ctx)));
}

fn save(store: &store::memory::MemoryStore, service: &str, session: &Session) {
    let Some((sent, login)) = session.saving.borrow_mut().take() else {
        return;
    };
    let holds = store
        .peek(service)
        .and_then(|held| serde_json::from_str::<Value>(&held).ok())
        .is_some_and(|held| held["claudeAiOauth"]["refreshToken"] == sent.as_str());
    if holds {
        store.plant(service, &login.to_string());
    }
}

/// A limit of `kind`, `percent` used, resetting an hour from now: what Anthropic answers of
/// one, for the tests of what Pitboard switches by itself.
pub(crate) fn window(kind: &str, percent: f64) -> crate::usage::Window {
    crate::usage::Window {
        kind: kind.into(),
        scope: None,
        percent,
        resets_at: Some(NOW + 3600),
        is_active: true,
        severity: None,
        length_seconds: crate::usage::LimitKind::from(kind).length(),
    }
}

/// Anthropic's usage answer for a five-hour limit `percent` used, resetting an hour from now,
/// in the shape `GET /api/oauth/usage` gives it and Claude Code caches it.
pub(crate) fn usage_answer(percent: f64) -> Value {
    let resets_at = jiff::Timestamp::from_second(NOW + 3600).expect("a time");
    json!({"limits": [{"kind": "session", "percent": percent, "resets_at": resets_at.to_string()}]})
}

/// Claude Code's usage cache, put in its config as 2.1.294 writes one: stamped with `here`,
/// the account the config names, whichever login `answer` was asked with.
pub(crate) fn cache_usage(m: &Rig, answer: Value) {
    let path = m.root.path().join(".claude.json");
    let mut config: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("a config")).expect("JSON");
    config["cachedUsageUtilization"] = json!({
        "fetchedAtMs": NOW * 1000,
        "accountUuid": "here",
        "utilization": answer,
    });
    std::fs::write(&path, config.to_string()).expect("the config is written");
}

pub(crate) fn account(label: &str, uuid: &str, parked: Option<Park>) -> Account {
    Account {
        last_used_at: None,
        replaced_at: None,
        plan: None,
        label: label.into(),
        id: uuid.into(),
        account_uuid: uuid.into(),
        email: format!("{uuid}@example.com"),
        parked,
        detail: state::Detail::Claude {
            organization_uuid: format!("org-{uuid}"),
            oauth_account: json!({
                "accountUuid": uuid,
                "emailAddress": format!("{uuid}@example.com"),
                "organizationUuid": format!("org-{uuid}"),
            }),
        },
    }
}

/// `who`'s login to the organisation `org`, enrolled as `label` by this version.
pub(crate) fn in_organisation(label: &str, who: &str, org: &str, parked: Option<Park>) -> Account {
    let owner = Owner {
        organization_uuid: org.into(),
        ..owner(who)
    };
    Account {
        last_used_at: None,
        replaced_at: None,
        plan: None,
        label: label.into(),
        id: state::new_id(ProviderId::Claude, &owner),
        account_uuid: owner.account_uuid,
        email: owner.email,
        parked,
        detail: state::Detail::Claude {
            organization_uuid: owner.organization_uuid,
            oauth_account: json!({"accountUuid": who, "organizationUuid": org}),
        },
    }
}

/// Everything that must be true after a killed change has been recovered, whatever the
/// change was and wherever it died.
pub(crate) fn hold(m: &Rig, after: &str) {
    let state = state::load(&m.ctx)
        .unwrap_or_else(|e| panic!("{after}: the state file must still parse, got {e}"));
    match provider::of(m.which) {
        LoginKind::Document(_) => {}
        LoginKind::Folder(app) => {
            return hold_folder(m, after, &state, app, Holds::all());
        }
    }

    // Nothing in the vault that the state does not name. An item nobody names holds a live
    // refresh token that no command will ever renew or delete, and on macOS nothing can
    // even list it.
    let named: HashSet<&str> = state
        .accounts
        .iter()
        .filter_map(|a| a.parked.as_ref())
        .map(|p| p.service.as_str())
        .chain(state.discarded.iter().map(|d| d.service.as_str()))
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
    let tool = provider::document(m.which);
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
            tool.identify(&m.ctx, &provider::Credential::new(m.which, document))
                .ok()
        })
        .map(|found| found.account_uuid);
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

// `hold`, for a Claude Desktop test where `forget`, `uninstall` or the test deleted what was
// planted.
#[cfg(not(windows))]
pub(crate) fn hold_where_deleted(m: &Rig, after: &str) {
    let state = state::load(&m.ctx)
        .unwrap_or_else(|e| panic!("{after}: the state file must still parse, got {e}"));
    let LoginKind::Folder(app) = provider::of(m.which) else {
        panic!("{after}: a folder tool");
    };
    let holds = Holds {
        nothing_deleted: false,
        ..Holds::all()
    };
    hold_folder(m, after, &state, app, holds);
}

// `hold`, for a Claude Desktop test that puts one session in two places on purpose (rows 14
// to 20).
#[cfg(not(windows))]
pub(crate) fn hold_where_twice(m: &Rig, after: &str) {
    let state = state::load(&m.ctx)
        .unwrap_or_else(|e| panic!("{after}: the state file must still parse, got {e}"));
    let LoginKind::Folder(app) = provider::of(m.which) else {
        panic!("{after}: a folder tool");
    };
    let holds = Holds {
        one_place: false,
        beside_its_name: false,
        ..Holds::all()
    };
    hold_folder(m, after, &state, app, holds);
}

// The checks of `hold` a test may leave out, each for what the test itself does.
#[derive(Clone, Copy)]
struct Holds {
    // check 3: each session in one place
    one_place: bool,
    // check 5: no session in Claude's folder beside another account's name
    beside_its_name: bool,
    // check 8: nothing planted was deleted
    nothing_deleted: bool,
}

impl Holds {
    fn all() -> Holds {
        Holds {
            one_place: true,
            beside_its_name: true,
            nothing_deleted: true,
        }
    }
}

// What must hold of a tool whose login is its app's files, after every step of a change.
fn hold_folder(
    m: &Rig,
    after: &str,
    state: &State,
    app: &'static dyn provider::folder::Folder,
    holds: Holds,
) {
    // Every folder under `parks` is named, and no folder's name reaches the vault.
    let parks = store::folder::list(&m.ctx)
        .expect("parks listed")
        .unwrap_or_default();
    let pending = std::fs::read_to_string(home::dir(&m.ctx).join("pending")).unwrap_or_default();
    for name in &parks {
        let named =
            state.names(name) || pending.lines().any(|line| line == format!("folder {name}"));
        assert!(named, "{after}: {name} under parks is named by nothing");
    }
    for dropped in &state.discarded {
        if parks.contains(&dropped.service) {
            assert_eq!(
                dropped.kind,
                ParkKind::Folder,
                "{after}: {}",
                dropped.service
            );
        }
    }
    for touch in m.mem.trail() {
        assert!(
            !(touch.store == "vault"
                && (touch.service.starts_with("pitboard-park-desktop_")
                    || touch.service.starts_with("pitboard-aside-"))),
            "{after}: {touch:?} asked the vault of a folder"
        );
    }

    // Every account is in use, holds a park whose folder holds its session, or holds nothing.
    let in_use = state.account_in_use(m.which).map(Account::key);
    for account in state.accounts.iter().filter(|a| a.provider() == m.which) {
        let Some(park) = &account.parked else {
            continue;
        };
        if in_use.as_ref() == Some(&account.key()) {
            continue;
        }
        // A move under way holds this park: where its jar is, recovery reads.
        if state
            .moving(m.which)
            .is_some_and(|moving| moving.park == park.service)
        {
            continue;
        }
        let manifest = store::folder::manifest(&m.ctx, &park.service)
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("{after}: {} names a park with no manifest", account.label));
        assert_eq!(
            manifest.login.as_deref(),
            Some(park.login.as_str()),
            "{after}"
        );
        let jar = store::folder::item(&m.ctx, &park.service, app.items()[0]);
        let session = app.session(&m.ctx, &jar).expect("its jar reads");
        assert_eq!(
            session.map(|s| s.login),
            Some(park.login.clone()),
            "{after}"
        );
    }

    // One session, one place: Claude's folder or one folder under `parks`.
    if holds.one_place {
        let mut seen = HashSet::new();
        for (place, login) in sessions(m) {
            assert!(
                seen.insert(login),
                "{after}: the session in {place} is also somewhere else"
            );
        }
    }

    // Nothing was deleted: Claude makes a store only where one is missing, so never these bytes.
    let data = m.root.path().join("claude/data");
    if holds.nothing_deleted {
        for (store, file) in crate::desktop_testing::STORES {
            let within = std::path::Path::new(store).join(file);
            let places = std::iter::once(data.join(&within)).chain(
                parks
                    .iter()
                    .map(|name| store::folder::item(&m.ctx, name, store).join(file)),
            );
            let found = places
                .filter(|place| {
                    std::fs::read(place).is_ok_and(|held| held == WORK.session.as_bytes())
                })
                .count();
            assert_eq!(
                found,
                1,
                "{after}: `work`'s {} is in {found} places",
                within.display()
            );
        }
    }

    // What is not an account's, in the settings and in the folder, is as `plant` wrote it.
    if !data.exists() {
        return;
    }
    let config = data.join("config.json");
    assert!(config.exists(), "{after}: config.json is gone");
    let settings = app
        .settings(&data)
        .unwrap_or_else(|e| panic!("{after}: config.json: {e:?}"));

    // The name key, where there is one, names every account the index records the session in
    // Claude's folder for: in use, parked, or moving.
    let jar = data.join(app.items()[0]);
    let session = jar
        .exists()
        .then(|| app.session(&m.ctx, &jar).ok().flatten())
        .flatten();
    if holds.beside_its_name
        && let (Some(session), Some(named)) = (session, settings.named(app))
    {
        let tool = m.which;
        let in_use = state
            .in_use(tool)
            .filter(|record| record.login == session.login)
            .and_then(|_| state.account_in_use(tool));
        let parked = state.accounts.iter().filter(|a| {
            a.provider() == tool
                && a.parked
                    .as_ref()
                    .is_some_and(|park| park.login == session.login)
        });
        let moving = state
            .moving(tool)
            .filter(|moving| moving.login == session.login)
            .and_then(|moving| state.by_id(tool, &moving.id));
        for account in in_use.into_iter().chain(parked).chain(moving) {
            assert_eq!(
                account.account_uuid, named,
                "{after}: {}'s session beside another name",
                account.label
            );
        }
    }
    let others: Vec<(&str, &str)> = settings
        .body
        .iter()
        .filter(|(key, _)| !app.keys().contains(&key.as_str()))
        // Claude's own while it runs.
        .filter(|(key, _)| key != "updaterRunningInstance")
        .map(|(key, value)| (key.as_str(), value.get()))
        .collect();
    assert_eq!(
        others,
        [("locale", "\"en-US\""), ("userThemeMode", "\"dark\"")],
        "{after}"
    );
    assert!(
        crate::host::fs::testing::is_private(&config).expect("its mode"),
        "{after}: config.json's mode changed"
    );
    for (stays, held) in [
        ("Preferences", "{}"),
        ("Cache/Cache_Data/index", WORK.session),
    ] {
        assert_eq!(
            std::fs::read_to_string(data.join(stays)).ok().as_deref(),
            Some(held),
            "{after}: {stays}"
        );
    }
}

/// Recovery, run the way the next command runs it.
pub(crate) fn recover(m: &Rig) -> Result<()> {
    settle(&m.ctx, Permit::for_a_test(), None).map(|_| ())
}
