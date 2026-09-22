//! Keeping parked logins alive, so their usage can be asked and they do not lapse. A parked
//! login is held by pitboard alone, so renewing it puts no second holder on its refresh
//! chain. The login signed in is Claude Code's, and is never renewed here.

use super::{journal, purge, try_exclusive};
use crate::api::{self, ApiError};
use crate::context::Context;
use crate::error::{Error, Result};
use crate::state::{Park, State};
use crate::{park, state, store, time};
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
#[non_exhaustive]
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
pub fn renew_parked(ctx: &Context) -> Vec<(String, Renewal)> {
    let Some(_exclusive) = try_exclusive(ctx) else {
        return Vec::new();
    };
    if journal::pending(ctx) {
        return Vec::new();
    }
    let Ok(mut state) = state::load(ctx) else {
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
    // Each renewal is a round trip that can take as long as the request timeout, so they
    // are asked together. What comes back is then written one at a time: the state file is
    // one file, and the order of writes to it is not something to leave to chance.
    let asked: Vec<(String, Park, Result<Asked>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = due
            .into_iter()
            .map(|(label, held)| {
                let ctx = &*ctx;
                let handle = scope.spawn({
                    let label = label.clone();
                    let held = held.clone();
                    move || ask(ctx, &label, &held)
                });
                (label, held, handle)
            })
            .collect();
        handles
            .into_iter()
            .map(|(label, held, handle)| {
                let answer = handle.join().unwrap_or_else(|_| {
                    Err(Error::RenewalFailed {
                        label: label.clone(),
                        detail: "the renewal thread stopped".into(),
                    })
                });
                (label, held, answer)
            })
            .collect()
    });
    let outcomes = asked
        .into_iter()
        .map(|(label, held, answer)| {
            let outcome =
                apply(ctx, &mut state, &label, &held, answer).unwrap_or_else(Renewal::Failed);
            (label, outcome)
        })
        .collect();
    purge(ctx, &mut state);
    outcomes
}

/// What one round trip produced, before anything is written down.
struct Asked {
    /// The parked document as it was, which the fresh tokens are folded into.
    oauth: Value,
    fresh: Option<api::Renewed>,
    /// Anthropic refuses this login for good.
    refused: bool,
}

/// The part of a renewal that talks to Anthropic. Touches no shared state, so several run
/// at once.
fn ask(ctx: &Context, label: &str, held: &Park) -> Result<Asked> {
    let oauth = park::load(ctx, label, held)?;
    let refresh = oauth["refreshToken"].as_str().unwrap_or_default();
    let mut scopes: Vec<String> = oauth["scopes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    if scopes.is_empty() {
        scopes = DEFAULT_SCOPES.map(str::to_owned).to_vec();
    }
    let client_id = oauth["clientId"].as_str();
    match api::renew(ctx, refresh, &scopes, client_id) {
        Ok(fresh) => Ok(Asked {
            oauth,
            fresh: Some(fresh),
            refused: false,
        }),
        Err(ApiError::InvalidGrant) => Ok(Asked {
            oauth,
            fresh: None,
            refused: true,
        }),
        // Unreachable or asked to slow down: nothing is written and the next run tries.
        Err(ApiError::Network(_) | ApiError::RateLimited) => Ok(Asked {
            oauth,
            fresh: None,
            refused: false,
        }),
        Err(e) => Err(Error::RenewalFailed {
            label: label.to_string(),
            detail: e.to_string(),
        }),
    }
}

/// The part that writes: one at a time, in the order the accounts are listed.
fn apply(
    ctx: &Context,
    state: &mut State,
    label: &str,
    held: &Park,
    asked: Result<Asked>,
) -> Result<Renewal> {
    let asked = asked?;
    if asked.refused {
        state.discard(&held.service);
        state::save(ctx, state)?;
        return Ok(Renewal::Refused);
    }
    let Some(fresh) = asked.fresh else {
        return Ok(Renewal::Deferred);
    };

    // The old refresh token may already be spent, so the answer is written at once, and a
    // second time under another name if the first write fails.
    let next = park::renewed(&asked.oauth, &fresh, time::now_millis());
    let uuid = state
        .get(label)
        .map(|a| a.account_uuid.clone())
        .unwrap_or_default();
    let store =
        || park::reserve(ctx, &uuid).and_then(|service| park::store_at(ctx, &service, &next));
    let parked = match store().or_else(|_| store()) {
        Ok(parked) => parked,
        Err(e) => {
            // Anthropic has already spent the old refresh token, so the copy pitboard holds
            // is dead whatever happens next. Dropping it now means status stops offering a
            // login that cannot work and says to sign in again instead.
            state.discard(&held.service);
            let _ = state::save(ctx, state);
            return Err(Error::RenewalFailed {
                label: label.to_string(),
                detail: e.to_string(),
            });
        }
    };
    state.park(label, parked.clone());
    if let Err(e) = state::save(ctx, state) {
        // Nothing that survives names the copy just written. The record on disk still
        // points at the spent one, which the next renewal will be refused and drop.
        let _ = store::vault_delete(ctx, &parked.service);
        return Err(e);
    }
    Ok(Renewal::Renewed)
}
