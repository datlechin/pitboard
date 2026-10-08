//! Whose login each tool has stored, learned once per login and recorded.
//!
//! A login its service has named is known by its refresh token's fingerprint from then on, so
//! only a login that changed since is asked about: one the tool renewed, or one a sign-in
//! outside Pitboard put there. Whatever a read or a change learns is recorded, so a sign-in
//! outside Pitboard that replaced the only login of the account in use is said from the
//! first read that finds it, and the activity log says when it was found.

use super::{Result, journal};
use crate::api::Owner;
use crate::context::Context;
use crate::in_use::{self, InUse};
use crate::provider::{self, ProviderError, ProviderId};
use crate::service::Permit;
use crate::state::{self, Account, Key, State};
use crate::{audit, fault};
use serde_json::Value;

/// Whose a login is, and since when that has been known.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Found {
    pub(crate) owner: Owner,
    /// The login's fingerprint, as [`provider::Provider::fingerprint`] gives it.
    pub(crate) login: String,
    /// When its service said whose it is.
    pub(crate) known_at: i64,
}

impl Found {
    /// The record of it, with what the tool's own record named before its store was read.
    pub(crate) fn recorded(self, named: Option<String>) -> InUse {
        InUse {
            owner: Some(self.owner),
            login: self.login,
            known_at: self.known_at,
            named,
        }
    }
}

/// The record of a store holding no login, as found at `at`, where the tool's own record,
/// `named` as read before the store, names nobody either. Where it names somebody, the login
/// is somewhere Pitboard does not look ([`super::nothing_signed_in`]), and nothing is known
/// of whose the store holds.
pub(crate) fn nobody(named: Option<&str>, at: i64) -> Option<InUse> {
    named.is_none().then(|| InUse {
        owner: None,
        login: String::new(),
        known_at: at,
        named: None,
    })
}

/// Whose `document`, read from `which`'s store, is. The owner the record names where it is
/// the very login the record was made for, by its fingerprint, which asks nobody; otherwise
/// what the tool's service says now. A login with no refresh token has no fingerprint, and
/// is always asked about.
pub(crate) fn whose(
    ctx: &Context,
    state: &State,
    which: ProviderId,
    document: &Value,
) -> std::result::Result<Found, ProviderError> {
    let tool = provider::of(which);
    let login = tool.fingerprint(document);
    if let Some(known) = state.in_use(which)
        && let Some(owner) = &known.owner
        && !login.is_empty()
        && known.login == login
    {
        return Ok(Found {
            owner: owner.clone(),
            login,
            known_at: known.known_at,
        });
    }
    let owner = tool
        .identify(ctx, &provider::Credential::new(which, document.clone()))
        .map(Owner::from)?;
    Ok(Found {
        owner,
        login,
        known_at: ctx.now(),
    })
}

/// What a tool's live store holds, and whose it is.
pub(crate) enum Live {
    /// No login, and the tool's own record names nobody either.
    Nothing,
    /// A login: the store it was read from, the login whole, and whose it is.
    Login {
        store: provider::LiveStore,
        document: Value,
        owner: Owner,
    },
}

/// What `which`'s store holds now and whose it is, with the record of that, not recorded
/// yet. The tool's own record is read before its store, as [`in_use::named`] says why. A
/// store holding nothing while the tool's own record names somebody is refused as
/// [`super::nothing_signed_in`] says it: nothing is known of whose the login is.
pub(crate) fn look(ctx: &Context, state: &State, which: ProviderId) -> Result<(Live, InUse)> {
    let store = super::live_store(ctx, which)?;
    let named = in_use::named(ctx, which);
    match super::read_stored(which, &store)? {
        None => {
            let nobody = nobody(named.as_deref(), ctx.now())
                .ok_or_else(|| super::nothing_signed_in(ctx, which))?;
            Ok((Live::Nothing, nobody))
        }
        Some((_, document)) => {
            let found = whose(ctx, state, which, &document)
                .map_err(|error| super::unidentified(which, error))?;
            let owner = found.owner.clone();
            let live = Live::Login {
                store,
                document,
                owner,
            };
            Ok((live, found.recorded(named)))
        }
    }
}

/// Whose login `which` has stored now, recorded in `state`, saved where that changed, and
/// written in the activity log where it changed outside Pitboard. For a change, which holds
/// Pitboard's lock.
pub(crate) fn now(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    which: ProviderId,
) -> Result<Live> {
    let (live, found) = look(ctx, state, which)?;
    let (changed, noticed) = record(state, which, found, ctx.now());
    if changed {
        state::save(ctx, permit, state)?;
    }
    write_down(ctx, permit, &noticed);
    Ok(live)
}

/// Record what a read found of whose login each tool has stored, where that is not what the
/// record says already, and write in the activity log what changed outside Pitboard.
///
/// A read that finds what the record says writes nothing. One that would change it takes
/// Pitboard's lock only where nobody holds it, and records nothing while a switch waits to be
/// finished, which may be about to put another login in the store: the next read records
/// what it finds then. Each tool's store is read again under the lock, and a login that moved
/// since it was asked about is not recorded, so an answer about one login is never filed for
/// another. Nothing of the tool's own is written.
pub(crate) fn record_read(ctx: &Context, permit: Permit, found: &[(ProviderId, InUse)]) {
    let news = |state: &State| -> Vec<(ProviderId, InUse)> {
        found
            .iter()
            .filter(|(which, found)| {
                !state
                    .in_use(*which)
                    .is_some_and(|known| known.agrees(*which, found))
            })
            .cloned()
            .collect()
    };
    let Ok(state) = state::load(ctx) else {
        return;
    };
    if news(&state).is_empty() {
        return;
    }
    let Some(_exclusive) = super::try_exclusive(ctx, permit) else {
        return;
    };
    if journal::pending(ctx) {
        return;
    }
    let Ok(mut state) = state::load(ctx) else {
        return;
    };
    fault::point("identify.recording");
    let mut changed = false;
    let mut noticed = Vec::new();
    for (which, found) in news(&state) {
        let expected = found.owner.as_ref().map(|_| found.login.clone());
        if holding(ctx, which) != Some(expected) {
            continue;
        }
        let (recorded, said) = record(&mut state, which, found, ctx.now());
        changed |= recorded;
        noticed.extend(said);
    }
    if changed && state::save(ctx, permit, &state).is_ok() {
        write_down(ctx, permit, &noticed);
    }
}

/// What `which`'s store holds now, by its login's fingerprint: `Some(None)` where it holds no
/// login, and `None` where it cannot be read or holds no one account's login.
fn holding(ctx: &Context, which: ProviderId) -> Option<Option<String>> {
    let live = super::live_store(ctx, which).ok()?;
    let stored = super::read_stored(which, &live).ok()?;
    Some(stored.map(|(_, document)| provider::of(which).fingerprint(&document)))
}

/// What recording whose login a tool has stored found changed outside Pitboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Noticed {
    /// The store holds another account's login than the record said, or one where it said
    /// none: this account's, where it is enrolled.
    SignedIn(Option<Key>),
    /// The store holds no login where the record said it held one.
    SignedOut,
    /// This account's only login was the one replaced.
    Replaced(Key),
}

/// Record `found` in `state` as said at `at`: whether the record changed, and what that
/// found changed outside Pitboard. A tool with no record before has nothing to compare with,
/// so nothing is noticed of it.
pub(crate) fn record(
    state: &mut State,
    which: ProviderId,
    found: InUse,
    at: i64,
) -> (bool, Vec<Noticed>) {
    let before = state.in_use(which).map(|known| known.account(which));
    let now = found.account(which);
    let now_enrolled = found
        .owner
        .as_ref()
        .and_then(|owner| state.account_of(which, owner))
        .map(Account::key);
    let identified = state.identified(which, found, at);
    let mut noticed = Vec::new();
    if before.is_some_and(|before| before != now) {
        noticed.push(match now {
            Some(_) => Noticed::SignedIn(now_enrolled),
            None => Noticed::SignedOut,
        });
    }
    noticed.extend(
        identified
            .replaced
            .map(|replaced| Noticed::Replaced(replaced.key)),
    );
    (identified.changed, noticed)
}

/// One `in-use` line per thing noticed: the account now in use, by label and with none where
/// it is not enrolled, then the account whose only login went.
///
/// Written once the record is saved, and never again: after a crash or a failed write in
/// between, the next read agrees with the record and notices nothing, so the line is lost
/// and the warning still stands.
pub(crate) fn write_down(ctx: &Context, permit: Permit, noticed: &[Noticed]) {
    for noticed in noticed {
        let (subject, outcome) = match noticed {
            Noticed::SignedIn(now) => (
                now.as_ref().map(Key::typed).unwrap_or_default(),
                "signed_in_outside",
            ),
            Noticed::SignedOut => (String::new(), "signed_out_outside"),
            Noticed::Replaced(key) => (key.typed(), "login_replaced"),
        };
        audit::record(ctx, permit, "in-use", &subject, outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::super::harness::{
        document, in_use_lines, machine, owner, signed_in_outside, state_file,
    };
    use super::*;
    use crate::service::Pitboard;

    /// Claude Code took another login while Anthropic was being asked about the one it had:
    /// the answer is about a login no longer stored, and is not filed for the one that is.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn an_answer_about_a_login_replaced_meanwhile_is_not_recorded() {
        let m = signed_in_outside("identify-meanwhile");
        m.api.owned_by("access-third-refresh", owner("third"));
        let third = document("third-refresh");
        let before = state_file(&m);

        crate::fault::meanwhile(
            "identify.recording",
            {
                let (store, service) = m.live_store();
                let third = third.to_string();
                move || store.plant(&service, &third)
            },
            || Pitboard::new(m.ctx.clone()).status(false),
        )
        .expect("a read");

        assert_eq!(
            m.live(),
            Some(third),
            "the login moved while it was asked about"
        );
        assert_eq!(state_file(&m), before);
        assert!(in_use_lines(&m).is_empty());
    }

    /// A switch that stopped partway may be about to put another login in the store, and the
    /// change that finishes it records what it puts there. A read records nothing meanwhile,
    /// whatever it finds: here a login of an account nobody enrolled.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_read_while_a_switch_is_interrupted_records_nothing() {
        let m = machine("identify-interrupted");
        m.api
            .owned_by("access-elsewhere-refresh", owner("elsewhere"));
        let settled = super::super::settle(&m.ctx, Permit::for_a_test(), None)
            .expect("nothing to recover")
            .0;
        let died = crate::fault::killing("switch.park_recorded", || {
            super::super::switch(settled, &m.key("there"))
        });
        assert_eq!(died.unwrap_err(), "switch.park_recorded");
        m.sign_in(&document("elsewhere-refresh"));
        let before = state_file(&m);

        Pitboard::new(m.ctx.clone()).status(false).expect("a read");

        assert!(super::super::interrupted(&m.ctx), "the switch still waits");
        assert_eq!(state_file(&m), before);
        assert!(in_use_lines(&m).is_empty());
    }

    /// A login is known by its fingerprint only where the record was made for that very
    /// login. A login with no refresh token has no fingerprint, and a record brought forward
    /// from schema 5 names no login, so neither is ever taken for the other.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_login_with_no_fingerprint_is_always_asked_about() {
        let m = machine("identify-no-fingerprint");
        let mut state = state::load(&m.ctx).expect("state");
        let brought_forward = InUse::of(state.get(&m.key("here")).expect("enrolled"), "", 0);
        state
            .in_use
            .insert(ProviderId::Claude.code().into(), brought_forward);
        let no_refresh =
            serde_json::json!({"claudeAiOauth": {"accessToken": "access-here-refresh"}});

        let found = whose(&m.ctx, &state, ProviderId::Claude, &no_refresh).expect("named");

        assert_eq!(found.owner, owner("here"));
        assert_eq!(
            m.api.asked(),
            [crate::api::scripted::Asked::Owner(
                "access-here-refresh".into()
            )]
        );
    }
}
