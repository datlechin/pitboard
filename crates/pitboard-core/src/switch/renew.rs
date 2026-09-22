//! Keeping parked logins alive, so their usage can be asked and they do not lapse. A parked
//! login is held by pitboard alone, so renewing it puts no second holder on its refresh
//! chain. The login signed in is Claude Code's, and is never renewed here.

use super::{journal, purge, try_exclusive};
use crate::api::{self, ApiError};
use crate::context::Context;
use crate::error::{Error, Result};
use crate::state::{Park, State};
use crate::{park, state, store};
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
    let now = ctx.now();
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
                        cause: None,
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
    /// The parked document as it was, whole, which the fresh tokens are folded into.
    oauth: Value,
    fresh: Option<api::Renewed>,
    /// Anthropic refuses this login for good.
    refused: bool,
}

/// The part of a renewal that talks to Anthropic. Touches no shared state, so several run
/// at once.
fn ask(ctx: &Context, label: &str, held: &Park) -> Result<Asked> {
    let document = park::load(ctx, label, held)?;
    let oauth = park::oauth_in(&document).clone();
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
            oauth: document,
            fresh: Some(fresh),
            refused: false,
        }),
        Err(ApiError::InvalidGrant) => Ok(Asked {
            oauth: document,
            fresh: None,
            refused: true,
        }),
        // Unreachable or asked to slow down: nothing is written and the next run tries.
        Err(ApiError::Network(_) | ApiError::RateLimited) => Ok(Asked {
            oauth: document,
            fresh: None,
            refused: false,
        }),
        Err(e) => Err(Error::RenewalFailed {
            label: label.to_string(),
            cause: Some(crate::error::Cause::of(&e)),
            detail: e.to_string(),
        }),
    }
}

/// Renew one parked login now, for a caller that needs it usable rather than merely
/// present. Returns the park that replaces it, or `None` when Anthropic could not be
/// reached or asked for less traffic, which is a reason to stop and not a reason to act.
pub(super) fn renew_one(
    ctx: &Context,
    state: &mut State,
    label: &str,
    held: &Park,
) -> Result<Option<Park>> {
    match apply(ctx, state, label, held, ask(ctx, label, held))? {
        Renewal::Renewed => Ok(state.get(label).and_then(|a| a.parked.clone())),
        Renewal::Refused => Err(Error::ParkedLoginRefused {
            label: label.to_string(),
        }),
        Renewal::Deferred => Ok(None),
        Renewal::Failed(e) => Err(e),
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
    let next = park::renewed(&asked.oauth, &fresh, ctx.now_millis());
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
                // Anthropic answered; it is this machine that could not keep the answer.
                cause: None,
                detail: e.to_string(),
            });
        }
    };
    crate::fault::point("renew.park_stored");
    state.park(label, parked.clone());
    if let Err(e) = state::save(ctx, state) {
        // Nothing that survives names the copy just written. The record on disk still
        // points at the spent one, which the next renewal will be refused and drop.
        let _ = store::vault_delete(ctx, &parked.service);
        return Err(e);
    }
    Ok(Renewal::Renewed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Renewed;
    use crate::api::scripted::{Asked as Question, ScriptedApi, Trouble};
    use crate::state::Account;
    use crate::store::memory::{Fault, MemoryPlatform};
    use crate::time::FixedClock;
    use serde_json::json;
    use std::sync::Arc;

    const NOW: i64 = 1_760_000_000;

    struct Machine {
        ctx: Context,
        mem: Arc<MemoryPlatform>,
        api: Arc<ScriptedApi>,
        home: std::path::PathBuf,
    }

    impl Drop for Machine {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.home);
        }
    }

    /// A machine with no keychain, no network and a clock that stands still.
    fn machine(name: &str) -> Machine {
        let home = std::env::temp_dir().join(format!(
            "pitboard-renew-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("a scratch home");
        let mem = MemoryPlatform::new();
        let api = ScriptedApi::new();
        let ctx = Context::new(home.clone())
            .with_pitboard_home(home.clone())
            .with_memory_stores(Arc::clone(&mem))
            .with_scripted_api(Arc::clone(&api))
            .with_clock(Arc::new(FixedClock::at(NOW)) as Arc<dyn crate::time::Clock>);
        Machine {
            ctx,
            mem,
            api,
            home,
        }
    }

    fn oauth(refresh: &str, access_expires_at: i64) -> Value {
        json!({
            "refreshToken": refresh,
            "accessToken": "a",
            "expiresAt": access_expires_at * 1000,
            "refreshTokenExpiresAt": (NOW + 30 * 86_400) * 1000
        })
    }

    /// One account holding one park, written the way a switch would have written it.
    fn with_park(m: &Machine, label: &str, refresh: &str, access_expires_at: i64) -> Park {
        let service = park::reserve(&m.ctx, "acc").expect("a free name");
        let park =
            park::store_at(&m.ctx, &service, &oauth(refresh, access_expires_at)).expect("parked");
        let mut state = State::default();
        state.accounts.push(Account {
            label: label.into(),
            account_uuid: "acc".into(),
            email: "me@example.com".into(),
            organization_uuid: "org".into(),
            oauth_account: json!({}),
            parked: Some(park.clone()),
        });
        state::save(&m.ctx, &state).expect("saved");
        park
    }

    fn fresh(refresh: &str) -> Renewed {
        Renewed {
            access_token: "new-access".into(),
            refresh_token: Some(refresh.into()),
            expires_in: 3600,
            refresh_token_expires_in: Some(30 * 86_400),
            scopes: None,
        }
    }

    fn outcome(outcomes: &[(String, Renewal)], label: &str) -> String {
        outcomes
            .iter()
            .find(|(l, _)| l == label)
            .map(|(_, r)| r.code().to_string())
            .unwrap_or_else(|| "not attempted".into())
    }

    /// The budget question, asked of the code rather than of a stopwatch: a park whose
    /// access token is still good is not a reason to talk to Anthropic at all.
    #[test]
    fn a_park_that_is_not_due_is_not_asked_about() {
        let m = machine("not-due");
        with_park(&m, "work", "r", NOW + 3600);

        let outcomes = renew_parked(&m.ctx);

        assert!(outcomes.is_empty());
        assert_eq!(m.api.calls(), 0, "nothing was due, so nothing was asked");
    }

    #[test]
    fn a_due_park_is_renewed_and_the_spent_copy_is_dropped() {
        let m = machine("renewed");
        let before = with_park(&m, "work", "old", NOW - 1);
        m.api.renews("old", fresh("new"));

        let outcomes = renew_parked(&m.ctx);

        assert_eq!(outcome(&outcomes, "work"), "renewed");
        assert_eq!(m.api.asked(), vec![Question::Renew("old".into())]);
        let state = state::load(&m.ctx).expect("state");
        let now = state
            .get("work")
            .expect("account")
            .parked
            .clone()
            .expect("park");
        assert_ne!(now.service, before.service, "a renewal takes a new name");
        assert_eq!(
            m.mem.vault().services(),
            vec![now.service.clone()],
            "the spent copy is deleted, not left behind"
        );
    }

    /// The answer that ends a park. The account keeps its label and its email, so the way
    /// back is one sign-in rather than an enrolment.
    #[test]
    fn a_login_anthropic_no_longer_accepts_is_dropped() {
        let m = machine("refused");
        with_park(&m, "work", "old", NOW - 1);
        m.api.renew_trouble("old", Trouble::InvalidGrant);

        let outcomes = renew_parked(&m.ctx);

        assert_eq!(outcome(&outcomes, "work"), "parked_login_refused");
        let state = state::load(&m.ctx).expect("state");
        assert!(state.get("work").expect("account").parked.is_none());
        assert!(m.mem.vault().services().is_empty());
    }

    /// Being unreachable, or being asked to slow down, must change nothing at all: the park
    /// that is still there is the one thing standing between the user and a browser.
    #[test]
    fn a_renewal_that_could_not_happen_leaves_the_park_alone() {
        for trouble in [Trouble::Offline, Trouble::RateLimited] {
            let m = machine(&format!("deferred-{trouble:?}"));
            let before = with_park(&m, "work", "old", NOW - 1);
            m.api.renew_trouble("old", trouble);

            let outcomes = renew_parked(&m.ctx);

            assert_eq!(outcome(&outcomes, "work"), "renewal_deferred");
            let state = state::load(&m.ctx).expect("state");
            assert_eq!(
                state.get("work").expect("account").parked,
                Some(before.clone()),
                "{trouble:?} must not spend or drop anything"
            );
            assert_eq!(m.mem.vault().services(), vec![before.service.clone()]);
        }
    }

    /// The failure this module's comments describe and no test could reach: Anthropic has
    /// already spent the old refresh token, and the fresh one cannot be written down. The
    /// copy pitboard holds is dead either way, so it is dropped rather than left to be
    /// offered as a login that cannot work.
    #[test]
    fn a_renewal_whose_answer_cannot_be_stored_drops_the_spent_park() {
        let m = machine("write-lost");
        let before = with_park(&m, "work", "old", NOW - 1);
        m.api.renews("old", fresh("new"));
        m.mem
            .vault()
            .fault_all(Fault::FailWrite("the keychain refused".into()));

        let outcomes = renew_parked(&m.ctx);

        assert_eq!(outcome(&outcomes, "work"), "renewal_failed");
        let state = state::load(&m.ctx).expect("state");
        assert!(
            state.get("work").expect("account").parked.is_none(),
            "a park whose refresh token Anthropic has spent must not stay on offer"
        );
        assert!(
            !m.mem.vault().services().contains(&before.service),
            "the dead copy is deleted in the same run, not left behind unnamed"
        );
        assert!(
            state.discarded.is_empty(),
            "and nothing is left listed for a later run to retry"
        );
    }
}
