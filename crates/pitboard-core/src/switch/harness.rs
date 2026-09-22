//! A machine to run changes against: two accounts, stores in memory, an Anthropic that
//! answers from a script, and a clock that stands still.
//!
//! Shared by the tests that kill a change partway ([`super::crash`]) and the tests that
//! make one refuse ([`super::refusals`]), because both need the same starting shape: one
//! account signed in, one parked and ready, and Claude Code's own files where the engine
//! expects them.

use super::*;
use crate::api::scripted::ScriptedApi;
use crate::api::{Api, Owner};

use crate::state::Account;
use crate::store::memory::MemoryPlatform;
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
    pub(super) mem: Arc<MemoryPlatform>,
    pub(super) api: Arc<ScriptedApi>,
    root: PathBuf,
    pub(super) service: String,
}

impl Machine {
    /// Where Claude Code's own files are, for a test that needs to change one.
    pub(super) fn ctx_home(&self) -> PathBuf {
        self.root.clone()
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

    let mem = MemoryPlatform::new();
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
    let parked =
        park::store_at(&ctx, &parked_service, &oauth("there-refresh", 30)).expect("parked");

    let mut state = State::default();
    state.accounts.push(account("here", "here", None));
    state.accounts.push(account("there", "there", Some(parked)));
    state.active = Some("here".into());
    state::save(&ctx, &state).expect("saved");

    Machine {
        ctx,
        mem,
        api,
        root,
        service,
    }
}

pub(super) fn account(label: &str, uuid: &str, parked: Option<Park>) -> Account {
    Account {
        label: label.into(),
        account_uuid: uuid.into(),
        email: format!("{uuid}@example.com"),
        organization_uuid: format!("org-{uuid}"),
        oauth_account: json!({
            "accountUuid": uuid,
            "emailAddress": format!("{uuid}@example.com"),
            "organizationUuid": format!("org-{uuid}"),
        }),
        parked,
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
    // past, which ends the login for the other.
    let mut seen: HashSet<String> = HashSet::new();
    let mut fingerprints = Vec::new();
    for service in m.mem.vault().services() {
        let raw = m.mem.vault().peek(&service).expect("just listed");
        let value: Value = serde_json::from_str(&raw).expect("a park is JSON");
        fingerprints.push((service, park::fingerprint_of(&value)));
    }
    if let Some(raw) = m.mem.live().peek(&m.service) {
        let value: Value = serde_json::from_str(&raw).expect("the live credential is JSON");
        fingerprints.push((
            "the live slot".into(),
            park::fingerprint_of(&value["claudeAiOauth"]),
        ));
    }
    for (place, fingerprint) in fingerprints {
        assert!(
            seen.insert(fingerprint.clone()),
            "{after}: the login in {place} is also somewhere else"
        );
    }

    // Every account can still be got back to. Signed in, or holding a login that can be
    // restored, or holding nothing and saying so, but never holding one that has expired.
    let live_uuid = m
        .mem
        .live()
        .peek(&m.service)
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|doc| {
            doc["claudeAiOauth"]["accessToken"]
                .as_str()
                .map(str::to_owned)
        })
        .and_then(|token| m.api.owner(&m.ctx, &token).ok())
        .map(|o| o.account_uuid);
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
    settle(&m.ctx).map(|_| ())
}
