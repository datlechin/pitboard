//! Keeping parked logins alive, so their usage can be asked and they do not lapse. A parked
//! login is held by pitboard alone, so renewing it puts no second holder on its refresh
//! chain. The login signed in is Claude Code's, and is never renewed here.

use super::{journal, purge, try_exclusive};
use crate::api::{self, ApiError};
use crate::error::{Error, Result};
use crate::state::{Park, State};
use crate::{park, state, time};
use serde_json::Value;

/// Renewed this long before its access token expires, so a read just after still answers.
const AHEAD_SECONDS: i64 = 120;

/// What Claude Code asks for when a login records no scopes of its own.
const DEFAULT_SCOPES: [&str; 6] = [
    "user:profile",
    "user:inference",
    "user:sessions:claude_code",
    "user:mcp_servers",
    "user:file_upload",
    "user:plugins",
];

#[derive(Debug)]
pub enum Renewal {
    Renewed,
    /// Anthropic refuses the login for good; it has been dropped.
    Refused,
    /// Anthropic could not be reached or asked to slow down; tried again next time.
    Deferred,
    Failed(Error),
}

impl Renewal {
    pub fn code(&self) -> &'static str {
        match self {
            Renewal::Renewed => "renewed",
            Renewal::Refused => "parked_login_refused",
            Renewal::Deferred => "renewal_deferred",
            Renewal::Failed(e) => e.code(),
        }
    }
}

/// Renew every parked login whose access token has expired or is about to. Nothing is done
/// while another pitboard run holds the lock or a switch waits to be finished: a renewal
/// replaces the refresh token, and nothing may install the old copy meanwhile.
pub fn renew_parked() -> Vec<(String, Renewal)> {
    let Some(_exclusive) = try_exclusive() else {
        return Vec::new();
    };
    if journal::pending() {
        return Vec::new();
    }
    let Ok(mut state) = state::load() else {
        return Vec::new();
    };
    let now = time::now();
    let due: Vec<(String, Park)> = state
        .accounts
        .iter()
        .filter_map(|a| {
            let held = a.parked.as_ref()?;
            (!held.askable_at(now + AHEAD_SECONDS) && held.restorable_at(now))
                .then(|| (a.label.clone(), held.clone()))
        })
        .collect();
    let outcomes = due
        .into_iter()
        .map(|(label, held)| {
            let outcome = renew(&mut state, &label, &held).unwrap_or_else(Renewal::Failed);
            (label, outcome)
        })
        .collect();
    purge(&mut state);
    outcomes
}

fn renew(state: &mut State, label: &str, held: &Park) -> Result<Renewal> {
    let oauth = park::load(label, held)?;
    let refresh = oauth["refreshToken"].as_str().unwrap_or_default();
    let mut scopes: Vec<String> = oauth["scopes"]
        .as_array()
        .map(|all| {
            all.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if scopes.is_empty() {
        scopes = DEFAULT_SCOPES.map(str::to_owned).to_vec();
    }

    let fresh = match api::renew(refresh, &scopes) {
        Ok(fresh) => fresh,
        Err(ApiError::InvalidGrant) => {
            state.discard(&held.service);
            state::save(state)?;
            return Ok(Renewal::Refused);
        }
        Err(ApiError::Network(_) | ApiError::RateLimited) => return Ok(Renewal::Deferred),
        Err(e) => {
            return Err(Error::RenewalFailed {
                label: label.to_string(),
                detail: e.to_string(),
            });
        }
    };

    // The old refresh token may already be spent, so the answer is written at once, and a
    // second time under another name if the first write fails.
    let next = park::renewed(&oauth, &fresh, time::now_millis());
    let uuid = state
        .get(label)
        .map(|a| a.account_uuid.clone())
        .unwrap_or_default();
    let store = || park::reserve(&uuid).and_then(|service| park::store_at(&service, &next));
    let parked = store().or_else(|_| store())?;
    state.park(label, parked);
    state::save(state)?;
    Ok(Renewal::Renewed)
}
