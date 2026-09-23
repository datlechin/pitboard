//! A machine to run changes against: two accounts of one tool, stores in memory, services
//! that answer from a script, and a clock that stands still.
//!
//! Shared by the tests that kill a change partway ([`super::crash`]) and the tests that
//! make one refuse ([`super::refusals`]), because both need the same starting shape: one
//! account signed in, one parked and ready, and the tool's own files where the engine
//! expects them. There is one for each tool, and the invariants in [`hold`] are asked of
//! every one of them through the provider boundary, because what must be true after a
//! crash is a fact about parking a login and not about any one tool's.

use super::*;
use crate::api::Owner;
use crate::api::scripted::ScriptedApi;
use crate::provider::ProviderId;
use crate::provider::claude::paths as claude;

use crate::state::Account;
use crate::store::memory::MemoryHost;
use crate::time::{Clock, FixedClock};
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;

pub(super) const NOW: i64 = 1_760_000_000;

/// Every place a change can be killed, named the way the code names it.
pub(super) const POINTS: [&str; 6] = [
    "switch.journal_written",
    "switch.park_stored",
    "switch.park_recorded",
    "switch.installed",
    "switch.recorded",
    "switch.config_updated",
];

pub(super) struct Machine {
    pub(super) ctx: Context,
    pub(super) mem: Arc<MemoryHost>,
    pub(super) api: Arc<ScriptedApi>,
    root: PathBuf,
    /// The keychain item Claude Code's login is in, on a Claude Code machine.
    pub(super) service: String,
    /// Which tool's accounts this machine holds.
    pub(super) which: ProviderId,
}

impl Machine {
    /// Where the tool's own files are, for a test that needs to change one.
    pub(super) fn ctx_home(&self) -> PathBuf {
        self.root.clone()
    }

    /// The live login, read the way the tool reads it.
    pub(super) fn live(&self) -> Option<Value> {
        crate::provider::of(self.which)
            .read_live(&self.ctx)
            .ok()
            .flatten()
            .map(|credential| credential.raw)
    }

    /// Replace the live login, the way the tool itself would write it.
    pub(super) fn sign_in(&self, document: &Value) {
        let live = crate::provider::of(self.which)
            .live(&self.ctx)
            .expect("a store to write to");
        store::write_raw(&live.chain, &live.service, &document.to_string())
            .expect("the live login is written");
    }

    pub(super) fn key(&self, label: &str) -> Key {
        Key::new(self.which, label)
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub(super) fn oauth(refresh: &str, expires_in_days: i64) -> Value {
    json!({
        "accessToken": format!("access-{refresh}"),
        "refreshToken": refresh,
        "expiresAt": (NOW + 3600) * 1000,
        "refreshTokenExpiresAt": (NOW + expires_in_days * 86_400) * 1000,
        "scopes": ["user:profile", "user:inference"],
    })
}

pub(super) fn document(refresh: &str) -> Value {
    json!({
        "claudeAiOauth": oauth(refresh, 30),
        "organizationUuid": "org-of-the-outgoing-account",
        "mcpOAuth": {"some-server": {"token": "unrelated"}},
    })
}

pub(super) fn owner(uuid: &str) -> Owner {
    Owner {
        account_uuid: uuid.into(),
        email: format!("{uuid}@example.com"),
        organization_uuid: format!("org-{uuid}"),
    }
}

/// Two accounts: `here` is signed in, `there` is parked and ready. The shape every switch
/// starts from.
pub(super) fn machine(name: &str) -> Machine {
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
    std::fs::create_dir_all(root.join(".pitboard")).expect("a pitboard home");
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
pub(super) fn codex_login(who: &str, refresh: &str) -> Value {
    json!({
        "auth_mode": "chatgpt",
        "OPENAI_API_KEY": null,
        "tokens": {
            "id_token": crate::provider::jwt::unsigned(&json!({
                "email": format!("{who}@example.com"),
                "exp": NOW + 3600,
                OPENAI: {"chatgpt_account_id": who, "chatgpt_plan_type": "pro"},
            })),
            "access_token": codex_access(refresh),
            "refresh_token": refresh,
            "account_id": who,
        },
        "last_refresh": "2025-10-09T08:00:00Z",
    })
}

/// The access token [`codex_login`] carries for this refresh token.
pub(super) fn codex_access(refresh: &str) -> String {
    crate::provider::jwt::unsigned(&json!({"exp": NOW + 10 * 86_400, "for": refresh}))
}

pub(super) fn codex_account(label: &str, uuid: &str, parked: Option<Park>) -> Account {
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
pub(super) fn codex_machine(name: &str) -> Machine {
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
            },
        );
    }

    std::fs::create_dir_all(machine.root.join(".pitboard")).expect("a pitboard home");
    let parked_service = park::reserve(&machine.ctx, "there").expect("a free name");
    let parked = park::store_at(
        &machine.ctx,
        ProviderId::Codex,
        &parked_service,
        &codex_login("there", "there-refresh"),
    )
    .expect("parked");

    let mut state = State::default();
    state.accounts.push(codex_account("here", "here", None));
    state
        .accounts
        .push(codex_account("there", "there", Some(parked)));
    state.set_active(ProviderId::Codex, Some("here".into()));
    state::save(&machine.ctx, &state).expect("saved");
    machine
}

pub(super) fn account(label: &str, uuid: &str, parked: Option<Park>) -> Account {
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
pub(super) fn hold(m: &Machine, after: &str) {
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
pub(super) fn recover(m: &Machine) -> Result<()> {
    settle(&m.ctx, None).map(|_| ())
}
