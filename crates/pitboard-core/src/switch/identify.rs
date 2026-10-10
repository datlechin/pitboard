//! Whose login each tool has stored, learned once per login and recorded.
//!
//! A login its service has named is known by its refresh token's fingerprint from then on, so
//! only a login that changed since is asked about: one the tool renewed, or one a sign-in
//! outside Pitboard put there. Whatever a read or a change learns is recorded, so a sign-in
//! outside Pitboard that replaced the only login of the account in use is said from the
//! first read that finds it, and the activity log says when it was found.
//!
//! A login asked about whose access token has lapsed is renewed first, as Claude Code renews
//! it ([`super::refresh`]): Claude Code renews it only where a session uses it, and a session
//! signed in with a file behind the keychain never does.

use super::refresh::{self, Place, Progress, Refreshing, Renewed, Stop};
use super::{Result, journal};
use crate::api::Owner;
use crate::context::Context;
use crate::error::Error;
use crate::in_use::{self, InUse};
use crate::provider::{self, ProviderError, ProviderId};
use crate::service::Permit;
use crate::state::{self, Account, Key, State};
use crate::{audit, fault, store};
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
    if let Some(found) = recorded(state, which, document) {
        return Ok(found);
    }
    let tool = provider::of(which);
    let owner = tool
        .identify(ctx, &provider::Credential::new(which, document.clone()))
        .map(Owner::from)?;
    Ok(Found {
        owner,
        login: tool.fingerprint(document),
        known_at: ctx.now(),
    })
}

/// Whose `document` is where it is the very login the record was made for.
fn recorded(state: &State, which: ProviderId, document: &Value) -> Option<Found> {
    let login = provider::of(which).fingerprint(document);
    let known = state.in_use(which)?;
    if login.is_empty() || known.login != login {
        return None;
    }
    Some(Found {
        owner: known.owner.clone()?,
        login,
        known_at: known.known_at,
    })
}

/// [`whose`], where that needs no renewal. `None` where it needs the tool's service and the
/// login's access token has lapsed: by its own expiry, and it is then not sent, or by the
/// service refusing it. Never `None` for a tool whose login says whose it is by itself.
pub(crate) fn told(
    ctx: &Context,
    state: &State,
    which: ProviderId,
    document: &Value,
) -> std::result::Result<Option<Found>, ProviderError> {
    let tool = provider::of(which);
    if tool.identifies_by_itself() {
        return whose(ctx, state, which, document).map(Some);
    }
    if let Some(found) = recorded(state, which, document) {
        return Ok(Some(found));
    }
    let lapsed = tool
        .expiry(document)
        .access_expires_at
        .is_some_and(|at| at <= ctx.now());
    if lapsed {
        return Ok(None);
    }
    match whose(ctx, state, which, document) {
        Err(ProviderError::Unauthorized) => Ok(None),
        asked => asked.map(Some),
    }
}

/// Why whose a lapsed login is could not be told by renewing it.
enum Unrenewed {
    /// Anthropic refuses its refresh token for good, or it has none, so only signing in
    /// again mends it.
    Refused,
    Asked(ProviderError),
    Failed(Error),
}

impl Unrenewed {
    /// As a change refuses over it.
    fn said(self, which: ProviderId) -> Error {
        match self {
            Unrenewed::Refused => Error::SessionExpired {
                tool: which,
                refused: true,
            },
            Unrenewed::Asked(error) => super::unidentified(which, error),
            Unrenewed::Failed(error) => error,
        }
    }
}

/// Whose `document`, the login `store` holds, is, where [`told`] says that needs it renewed.
/// Under Claude Code's refresh lock, the login is read again: one changed meanwhile, as a
/// session's renewal leaves it, is told as it is, with nothing sent. Otherwise its refresh
/// token is sent and the answer saved where the login is kept, and whose it is asked with the
/// renewed access token. What the store holds then, whose it is, and whether a lock of
/// Claude Code's stopped being Pitboard's meanwhile. Under Pitboard's lock, which the caller
/// holds, with no renewal left waiting for the slot ([`refresh::finished`]).
fn renewed(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    which: ProviderId,
    store: &provider::LiveStore,
    document: &Value,
) -> std::result::Result<(Value, Found, bool), Unrenewed> {
    if which == ProviderId::Claude && crate::settings::custom_oauth(ctx) {
        return Err(Unrenewed::Failed(Error::CustomOauthEndpoint));
    }
    fault::point("identify.lapsed");
    // A tool that takes no refresh lock is left to renew its own login.
    let refreshing = Refreshing::take(ctx, permit, which)
        .map_err(Unrenewed::Failed)?
        .ok_or(Unrenewed::Asked(ProviderError::Unauthorized))?;
    let told = renewed_under_the_lock(ctx, permit, state, which, store, document);
    let lost = Refreshing::let_go(Some(refreshing));
    told.map(|(document, found, lock_lost)| (document, found, lock_lost || lost))
}

/// [`renewed`], with Claude Code's refresh lock held. A login another writer changes under
/// it, between its last reading and the renewal, is read again once.
fn renewed_under_the_lock(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    which: ProviderId,
    store: &provider::LiveStore,
    document: &Value,
) -> std::result::Result<(Value, Found, bool), Unrenewed> {
    let tool = provider::of(which);
    let failed = Unrenewed::Failed;
    let mut last = document.clone();
    for _ in 0..2 {
        let (seen, now) = super::read_stored(which, store)
            .map_err(failed)?
            .ok_or_else(|| failed(super::nothing_signed_in(ctx, which)))?;
        if now != last
            && let Some(found) = told(ctx, state, which, &now).map_err(Unrenewed::Asked)?
        {
            return Ok((now, found, false));
        }
        let login = tool
            .slice(&now)
            .map_err(|e| failed(super::shape(which, e)))?;
        if tool.fingerprint(&login).is_empty() {
            return Err(Unrenewed::Refused);
        }
        let kept = store::holder(&store.chain, &store.service)
            .map_err(|e| failed(e.into()))?
            .ok_or_else(|| failed(super::nothing_signed_in(ctx, which)))?;
        let place = Place {
            store: kept,
            service: &store.service,
        };
        let mut progress = Progress::default();
        match refresh::renew(
            ctx,
            permit,
            state,
            which,
            place,
            &seen,
            &login,
            &mut progress,
        ) {
            Ok(Renewed::Saved(saved)) => {
                let renewed: Value = serde_json::from_str(&saved)
                    .map_err(|e| failed(store::Error::Malformed(e.to_string()).into()))?;
                let owner = tool
                    .identify(ctx, &provider::Credential::new(which, renewed.clone()))
                    .map(Owner::from)
                    .map_err(Unrenewed::Asked)?;
                let found = Found {
                    owner,
                    login: tool.fingerprint(&renewed),
                    known_at: ctx.now(),
                };
                return Ok((renewed, found, progress.lock_lost));
            }
            Ok(Renewed::Refused) => return Err(Unrenewed::Refused),
            Err(Stop::Changed) => last = now,
            Err(Stop::Unrenewed(error)) => return Err(Unrenewed::Asked(error)),
            Err(Stop::Failed(error)) if progress.lost() => {
                return Err(failed(Error::RenewalLost {
                    tool: which,
                    error: Box::new(error),
                }));
            }
            Err(Stop::Failed(error)) => return Err(failed(error)),
        }
    }
    Err(failed(Error::SignedInAccountChanged))
}

/// [`told`] for a read, which renews a lapsed login only where it takes Pitboard's lock
/// without waiting and no switch waits to be finished, as a read renews a park, and then as
/// a change does. Otherwise the service is taken to have refused the lapsed access token,
/// and nothing is renewed. Where it was renewed, the login stored then comes with whose it
/// is, for the read to ask its usage with.
pub(crate) fn told_by_a_read(
    ctx: &Context,
    permit: Permit,
    state: &State,
    which: ProviderId,
    document: &Value,
) -> std::result::Result<(Option<Value>, Found), ProviderError> {
    if let Some(found) = told(ctx, state, which, document)? {
        return Ok((None, found));
    }
    let untold = || ProviderError::Unauthorized;
    let _exclusive = super::try_exclusive(ctx, permit).ok_or_else(untold)?;
    if journal::pending(ctx) {
        return Err(untold());
    }
    let mut state = state::load(ctx).map_err(|_| untold())?;
    refresh::finished(ctx, permit, &mut state, which).map_err(|_| untold())?;
    let store = super::live_store(ctx, which).map_err(|_| untold())?;
    match renewed(ctx, permit, &mut state, which, &store, document) {
        Ok((renewed, found, _)) => Ok((Some(renewed), found)),
        // A renewal lost spent the refresh token stored, which Anthropic refuses from then on.
        Err(Unrenewed::Refused | Unrenewed::Failed(Error::RenewalLost { .. })) => {
            Err(ProviderError::InvalidGrant {
                service: which.service(),
            })
        }
        Err(Unrenewed::Asked(error)) => Err(error),
        Err(Unrenewed::Failed(_)) => Err(untold()),
    }
}

/// What a tool's live store holds, and whose it is.
pub(crate) enum Live {
    /// No login, and the tool's own record names nobody either.
    Nothing,
    Login(Login),
}

/// A login a tool's live store holds: the store it was read from, the login whole, and whose
/// it is.
pub(crate) struct Login {
    pub(crate) store: provider::LiveStore,
    pub(crate) document: Value,
    pub(crate) owner: Owner,
    /// A lock of Claude Code's stopped being Pitboard's while the login was renewed to tell
    /// whose it is.
    pub(crate) lock_lost: bool,
}

impl Login {
    /// What a change that told whose this login is says of a lock lost while renewing it.
    pub(crate) fn lock_warning(&self, which: ProviderId) -> Option<crate::service::Warning> {
        self.lock_lost
            .then_some(crate::service::Warning::LockCompromised { tool: which })
    }
}

/// What `which`'s store holds now and whose it is, with the record of that, not recorded
/// yet. The tool's own record is read before its store, as [`in_use::named`] says why. A
/// store holding nothing while the tool's own record names somebody is refused as
/// [`super::nothing_signed_in`] says it: nothing is known of whose the login is. A login
/// whose access token has lapsed is renewed first, where telling needs the tool's service,
/// and what the store holds is then the renewed login. For a change, which holds Pitboard's
/// lock and acts on that login once it is told, so a renewal left waiting for the slot is
/// saved first, and where it cannot be yet, why stops it ([`refresh::finished`]).
pub(crate) fn look(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    which: ProviderId,
) -> Result<(Live, InUse)> {
    refresh::finished(ctx, permit, state, which)?;
    let store = super::live_store(ctx, which)?;
    let named = in_use::named(ctx, which);
    match super::read_stored(which, &store)? {
        None => {
            let nobody = nobody(named.as_deref(), ctx.now())
                .ok_or_else(|| super::nothing_signed_in(ctx, which))?;
            Ok((Live::Nothing, nobody))
        }
        Some((_, document)) => {
            let (document, found, lock_lost) = match told(ctx, state, which, &document) {
                Ok(Some(found)) => (document, found, false),
                Ok(None) => renewed(ctx, permit, state, which, &store, &document)
                    .map_err(|untold| untold.said(which))?,
                Err(error) => return Err(super::unidentified(which, error)),
            };
            let owner = found.owner.clone();
            let live = Live::Login(Login {
                store,
                document,
                owner,
                lock_lost,
            });
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
    let (live, found) = look(ctx, permit, state, which)?;
    keep(ctx, permit, state, which, found)?;
    Ok(live)
}

/// [`now`] for what [`look`] found already, for a change that does something of its own where
/// whose login is stored could not be told.
pub(crate) fn keep(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    which: ProviderId,
    found: InUse,
) -> Result<()> {
    let (changed, noticed) = record(state, which, found, ctx.now());
    if changed {
        state::save(ctx, permit, state)?;
    }
    write_down(ctx, permit, &noticed);
    Ok(())
}

/// Why whose login is stored could not be told, in a few words, for a sentence that says what
/// to do itself: an error's own advice would be about something else.
pub(crate) fn untold(error: &Error) -> String {
    match error {
        Error::SessionExpired { refused: true, .. } => {
            "its login has expired and cannot be renewed".into()
        }
        Error::SessionExpired { tool, .. } => {
            format!("{} refused its access token", tool.service())
        }
        Error::IdentityUnverifiable { detail, .. }
        | Error::LiveCredentialShapeUnexpected { detail, .. } => detail.clone(),
        Error::LiveStoreUnsupported { reason, .. } => reason.clone(),
        Error::LiveCredentialElsewhere { email } => {
            format!("its config names {email}, and Pitboard cannot find that login")
        }
        Error::Store(e) => e.to_string(),
        Error::RenewalLost { error, .. } => untold(error),
        other => other.code().replace('_', " "),
    }
}

/// Record what a read found of whose login each tool has stored, where that is not what the
/// record says already, and write in the activity log what changed outside Pitboard.
///
/// A read that finds what the record says writes nothing. One that would change it takes
/// Pitboard's lock only where nobody holds it, and records nothing while a switch waits to be
/// finished, which may be about to put another login in the store: the next read records
/// what it finds then. Each tool's own record and store are read again under the lock, and a
/// tool where either moved since the read is not recorded, so an answer about one login is
/// never filed for another, and what the read saw the tool's own record name is never filed
/// over what `use` wrote there since. Nothing of the tool's own is written.
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
        if in_use::named(ctx, which) != found.named || holding(ctx, which) != Some(expected) {
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
    use super::super::Settled;
    use super::super::harness::{
        Machine, NOW, Session, audit_lines, codex_id, codex_login, codex_machine, config_names,
        document, hold, lapsed, lock_dir, machine, owner, record_in_use, recover, renews,
        renews_meanwhile, saves, signed_in_outside, state_file, takes_the_write_lock, window,
        write_target,
    };
    use super::*;
    use crate::api::scripted::{Asked, Trouble};
    use crate::error::Cause;
    use crate::service::Pitboard;
    use crate::store::memory::Fault;
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::Arc;

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
        assert!(audit_lines(&m, "in-use").is_empty());
    }

    /// `pitboard use` wrote the account in use into Claude Code's config while a read was
    /// asking about the login: what the read saw the config name is past, and is not filed
    /// over what `use` recorded.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn what_a_read_saw_the_config_name_before_it_was_written_is_not_recorded() {
        let m = machine("identify-config-written-meanwhile");
        config_names(&m, "there");
        let mut state = state::load(&m.ctx).expect("state");
        let (_, found) =
            look(&m.ctx, Permit::for_a_test(), &mut state, ProviderId::Claude).expect("a look");
        let pitboard = Pitboard::new(m.ctx.clone());
        pitboard.switch_to("here").expect("already in use");
        let before = state_file(&m);

        record_read(&m.ctx, Permit::for_a_test(), &[(ProviderId::Claude, found)]);

        assert_eq!(state_file(&m), before);
        let read = pitboard.status_offline().expect("a read of what is known");
        let codes: Vec<&str> = read
            .warnings
            .iter()
            .map(crate::service::Warning::code)
            .collect();
        assert!(codes.is_empty(), "{codes:?}");
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
        assert!(audit_lines(&m, "in-use").is_empty());
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

    /// `here`'s login renewed since Anthropic last named it, and lapsed since, as Claude Code
    /// leaves it where every session signed in with a file behind the keychain: nobody renews
    /// the keychain's. Anthropic renews it to `here-again`.
    fn lapsed_stored(name: &str) -> Machine {
        let m = machine(name);
        m.sign_in(&lapsed("here-renewed"));
        renews(&m, "here-renewed", "here-again");
        m.api.owned_by("access-here-again", owner("here"));
        m
    }

    /// Whose login Claude Code has stored, told as a change tells it, under Pitboard's lock.
    fn told_now(m: &Machine) -> Result<Live> {
        told_in(&m.ctx)
    }

    fn told_in(ctx: &Context) -> Result<Live> {
        let Settled {
            _exclusive,
            mut state,
            ctx,
            permit,
        } = super::super::settle(ctx, Permit::for_a_test(), None)?.0;
        now(&ctx, permit, &mut state, ProviderId::Claude)
    }

    /// Killed once Anthropic has answered a renewal of `here`'s lapsed login, which spends
    /// the refresh token the keychain still holds.
    fn killed_once_answered(m: &Machine) {
        let died = crate::fault::killing("refresh.exchanged", || told_now(m).map(|_| ()));
        assert_eq!(died.err().as_deref(), Some("refresh.exchanged"));
        m.api.renew_trouble("here-renewed", Trouble::InvalidGrant);
    }

    /// The vault items holding a login on `refresh`.
    fn copies_of(m: &Machine, refresh: &str) -> Vec<String> {
        let held = format!("\"refreshToken\":\"{refresh}\"");
        m.mem
            .vault()
            .services()
            .into_iter()
            .filter(|service| {
                m.mem
                    .vault()
                    .peek(service)
                    .is_some_and(|login| login.contains(&held))
            })
            .collect()
    }

    /// Puts something in the place of Claude Code's write lock at the moment it is called,
    /// which taking the lock over fails on at once, as a file there does.
    fn jams_the_write_lock(m: &Machine) -> impl FnOnce() + 'static {
        let stuck = lock_dir(&write_target(m));
        move || {
            let parent = stuck.parent().expect("the storage directory");
            std::fs::create_dir_all(parent).expect("the storage directory is made");
            let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
            std::fs::File::create(&stuck)
                .and_then(|lock| lock.set_modified(long_ago))
                .expect("something in the lock's place");
        }
    }

    /// Locks the login keychain at the moment it is called, which on macOS holds Claude Code's
    /// login and Pitboard's vault both.
    fn locks_the_keychain(m: &Machine) -> impl FnOnce() + Send + 'static {
        let mem = Arc::clone(&m.mem);
        move || mem.lock_keychain()
    }

    fn owner_told(told: Result<Live>) -> Owner {
        match told.expect("told") {
            Live::Login(login) => login.owner,
            Live::Nothing => panic!("a login is stored"),
        }
    }

    fn stored_refresh(m: &Machine) -> Option<String> {
        m.live()?
            .pointer("/claudeAiOauth/refreshToken")?
            .as_str()
            .map(str::to_owned)
    }

    fn renewals(m: &Machine) -> Vec<Asked> {
        m.api
            .asked()
            .into_iter()
            .filter(|asked| matches!(asked, Asked::Renew(_)))
            .collect()
    }

    /// The owner's machine on 10 October 2026: no session renewed the keychain's login, its
    /// access token lapsed, and every change stopped at `session_expired`. Pitboard renews it
    /// as Claude Code renews it, under its refresh lock, never sending the lapsed access
    /// token, saves the renewed login with the document's other keys as they were, and names
    /// and records it.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_lapsed_login_is_renewed_under_the_refresh_lock_and_named() {
        let m = lapsed_stored("identify-renews");
        let refreshing = lock_dir(&crate::provider::claude::paths::refresh_lock(&m.ctx));
        let held = Rc::new(Cell::new(false));
        let looked = {
            let (held, refreshing) = (Rc::clone(&held), refreshing.clone());
            move || held.set(refreshing.is_dir())
        };

        let told = crate::fault::meanwhile("refresh.exchanged", looked, || told_now(&m));

        assert_eq!(owner_told(told), owner("here"));
        assert!(held.get(), "renewed under Claude Code's refresh lock");
        assert!(!refreshing.exists(), "and let go of it");
        assert_eq!(stored_refresh(&m).as_deref(), Some("here-again"));
        assert_eq!(
            m.live().expect("stored")["mcpOAuth"],
            document("x")["mcpOAuth"]
        );
        assert_eq!(
            m.api.asked(),
            [
                Asked::Renew("here-renewed".into()),
                Asked::Owner("access-here-again".into())
            ]
        );
        let state = state::load(&m.ctx).expect("state");
        let recorded = state.in_use(ProviderId::Claude).expect("recorded");
        assert_eq!(recorded.owner, Some(owner("here")));
        assert_eq!(recorded.login, crate::store::fingerprint("here-again"));
        assert_eq!(state.renewing, []);
        assert!(crate::pending::outstanding(&m.ctx, Some(&state)).is_empty());
        hold(&m, "after a renewal");
    }

    /// A session that renewed the login while Pitboard waited for Claude Code's refresh
    /// lock has saved what it renewed to by then. Read again under the lock, that is the
    /// login told, and no refresh token is sent.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_login_a_session_renewed_meanwhile_is_told_and_nothing_is_sent() {
        let m = lapsed_stored("identify-renewed-meanwhile");
        m.api
            .owned_by("access-here-renewed-by-a-session", owner("here"));
        let session = Session::new();
        let keychain = Arc::clone(m.mem.live());

        let told = crate::fault::meanwhile(
            "identify.lapsed",
            renews_meanwhile(&m, Arc::clone(&keychain), &session),
            || told_now(&m),
        );
        saves(&m, &keychain, &session);

        assert_eq!(owner_told(told), owner("here"));
        assert!(!session.waited.get());
        assert_eq!(renewals(&m), []);
        assert_eq!(
            stored_refresh(&m).as_deref(),
            Some("here-renewed-by-a-session")
        );
    }

    /// A login Anthropic refuses for good can only be signed in to again, which is said, and
    /// nothing is written to the store or the vault. Pitboard's record ends as it was: the
    /// copy's name written down before the refresh token was sent is let go of.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_lapsed_login_anthropic_refuses_says_sign_in_again_and_writes_nothing() {
        let m = machine("identify-refused");
        m.sign_in(&lapsed("here-renewed"));
        m.api.renew_trouble("here-renewed", Trouble::InvalidGrant);
        let (before, stored, vault) = (state_file(&m), m.live(), m.mem.vault().services());

        let refused = told_now(&m).err().expect("refused");

        assert_eq!(refused.code(), "session_expired");
        assert_eq!(refused.cause(), Some(Cause::LoginRefused));
        assert!(
            refused
                .to_string()
                .ends_with("Run `claude`, sign in, then try again."),
            "{refused}"
        );
        assert_eq!(state_file(&m).0, before.0);
        assert_eq!(m.live(), stored);
        assert_eq!(m.mem.vault().services(), vault);
    }

    /// A renewed login the keychain could not take, past its standard input with the
    /// argument line forbidden, would be spent and lost, so its refresh token is not sent and
    /// nothing changes.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_login_the_keychain_could_not_take_back_is_not_renewed() {
        let mut m = lapsed_stored("identify-too-large");
        m.ctx = m.ctx.clone().with_argv_fallback(false);
        m.mem.live().takes_on_stdin(16);
        let stored = m.live();

        let refused = told_now(&m).err().expect("refused");

        assert_eq!(refused.code(), "credential_too_large");
        assert_eq!(renewals(&m), []);
        assert_eq!(m.live(), stored);
    }

    /// The exchange spends the refresh token sent, so once Anthropic has answered, Claude
    /// Code's write lock is waited for longer than a change waits: here a lock a killed
    /// session left 7 seconds ago, which nobody may take for 8 more. The renewed login is
    /// saved all the same.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_write_lock_held_once_the_refresh_token_is_spent_is_waited_for() {
        let m = lapsed_stored("identify-write-lock-after");

        let told = crate::fault::meanwhile(
            "refresh.exchanged",
            takes_the_write_lock(&m, std::time::Duration::from_secs(7)),
            || told_now(&m),
        );

        assert_eq!(owner_told(told), owner("here"));
        assert_eq!(stored_refresh(&m).as_deref(), Some("here-again"));
    }

    /// Killed once Anthropic has answered, the renewed login is saved, or kept where the
    /// next run saves it from: Anthropic has spent the refresh token stored by then. The
    /// next change saves it, and so does the next read, which must tell whose the login is,
    /// keeping no second copy and renewing nothing again.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn killed_once_anthropic_answered_the_renewed_login_is_saved_or_kept() {
        for (point, next) in [
            ("refresh.exchanged", "change"),
            ("refresh.exchanged", "read"),
            ("refresh.saved", "change"),
        ] {
            let m = lapsed_stored(&format!("identify-killed-{point}-{next}"));

            let died = crate::fault::killing(point, || told_now(&m).map(|_| ()));
            assert_eq!(died.err().as_deref(), Some(point));
            m.api.renew_trouble("here-renewed", Trouble::InvalidGrant);

            let kept = m.mem.vault().services().into_iter().any(|service| {
                m.mem
                    .vault()
                    .peek(&service)
                    .is_some_and(|held| held.contains("\"refreshToken\":\"here-again\""))
            });
            assert!(
                kept || stored_refresh(&m).as_deref() == Some("here-again"),
                "{point}: the renewed login is nowhere"
            );

            if next == "change" {
                recover(&m).expect("the next change");
            } else {
                Pitboard::new(m.ctx.clone()).status(false).expect("a read");
            }
            assert_eq!(
                stored_refresh(&m).as_deref(),
                Some("here-again"),
                "{point}, {next}"
            );
            assert!(
                state::load(&m.ctx).expect("state").renewing.is_empty(),
                "{point}, {next}"
            );
            recover(&m).expect("the next change");
            hold(&m, point);
            assert_eq!(owner_told(told_now(&m)), owner("here"), "{point}");
            assert_eq!(
                renewals(&m),
                [Asked::Renew("here-renewed".into())],
                "{point}, {next}"
            );
        }
    }

    /// Once Anthropic has answered, the keychain still holds the spent refresh token. A save
    /// that fails then, the keychain locking or the write failing twice, keeps the renewed
    /// login in the vault, and the next change saves it.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_renewed_login_that_cannot_be_saved_yet_is_kept_for_the_next_change() {
        for (fault, code) in [
            (Fault::Locked, "credential_store_locked"),
            (
                Fault::FailWrite("disk full".into()),
                "credential_write_failed",
            ),
        ] {
            let m = lapsed_stored(&format!("identify-unsaved-{code}"));
            let (keychain, service) = m.live_store();
            let faulted = {
                let (keychain, service) = (Arc::clone(&keychain), service.clone());
                let api = Arc::clone(&m.api);
                move || {
                    api.renew_trouble("here-renewed", Trouble::InvalidGrant);
                    keychain.fault(&service, fault);
                }
            };

            let refused = crate::fault::meanwhile("refresh.exchanged", faulted, || told_now(&m))
                .err()
                .expect("not saved");

            assert_eq!(refused.code(), code);
            assert_eq!(copies_of(&m, "here-again").len(), 1, "{code}");
            assert!(
                keychain
                    .peek(&service)
                    .is_some_and(|held| held.contains("\"refreshToken\":\"here-renewed\"")),
                "{code}"
            );
            keychain.heal(&service);
            assert_eq!(owner_told(told_now(&m)), owner("here"), "{code}");
            assert_eq!(stored_refresh(&m).as_deref(), Some("here-again"), "{code}");
            assert_eq!(renewals(&m), [Asked::Renew("here-renewed".into())]);
            assert_eq!(copies_of(&m, "here-again"), Vec::<String>::new());
            hold(&m, code);
        }
    }

    /// A renewal left in the vault is saved before anything is sent, since the store still
    /// holds the refresh token it spent. Where it cannot be saved yet, Claude Code's write
    /// lock being in the way, the change stops with that and sends nothing.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_renewal_left_unsaved_is_saved_before_anything_is_sent() {
        let m = lapsed_stored("identify-left-unsaved");
        killed_once_answered(&m);
        jams_the_write_lock(&m)();
        let stuck = lock_dir(&write_target(&m));
        let lifted = Rc::new(Cell::new(false));
        let lift = {
            let (stuck, lifted) = (stuck.clone(), Rc::clone(&lifted));
            move || {
                std::fs::remove_file(&stuck).expect("the lock is let go of");
                lifted.set(true);
            }
        };

        let refused = crate::fault::meanwhile("identify.lapsed", lift, || told_now(&m))
            .err()
            .expect("the copy cannot be saved");

        assert_eq!(refused.code(), "switch_in_progress");
        assert!(!lifted.get(), "stopped before the renewal");
        assert_eq!(renewals(&m), [Asked::Renew("here-renewed".into())]);
        assert_eq!(copies_of(&m, "here-again").len(), 1);
        std::fs::remove_file(&stuck).expect("the lock is let go of");
        assert_eq!(owner_told(told_now(&m)), owner("here"));
        assert_eq!(stored_refresh(&m).as_deref(), Some("here-again"));
        assert_eq!(renewals(&m), [Asked::Renew("here-renewed".into())]);
    }

    /// A renewal left unsaved for one of Claude Code's credential slots stays written down
    /// while a change under another slot renews that slot's login, and the next change in
    /// its own slot saves it.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_renewal_left_for_another_slot_is_kept_for_it() {
        let m = lapsed_stored("identify-two-slots");
        killed_once_answered(&m);
        let [left] = copies_of(&m, "here-again")
            .try_into()
            .expect("one copy left");
        let dir = m.ctx_home().join("other");
        std::fs::create_dir_all(&dir).expect("the other config directory");
        let other = m
            .ctx
            .clone()
            .with_claude_config_dir(dir.display().to_string());
        m.mem.live().plant(
            &crate::provider::claude::paths::live_service(&other),
            &lapsed("other-renewed").to_string(),
        );
        renews(&m, "other-renewed", "other-again");
        m.api.owned_by("access-other-again", owner("there"));

        assert_eq!(owner_told(told_in(&other)), owner("there"));

        assert!(state::load(&m.ctx).expect("state").names(&left));
        assert!(m.mem.vault().peek(&left).is_some());
        assert_eq!(owner_told(told_now(&m)), owner("here"));
        assert_eq!(stored_refresh(&m).as_deref(), Some("here-again"));
        assert_eq!(
            renewals(&m),
            [
                Asked::Renew("here-renewed".into()),
                Asked::Renew("other-renewed".into())
            ]
        );
        hold(&m, "both slots saved");
    }

    /// A renewal left in the vault for the keychain's login stops a switch from parking that
    /// login, whose refresh token it spent, though Pitboard knows whose that login is by its
    /// fingerprint and renews nothing. Once the renewal is saved, the login parked for the
    /// account switched from is the renewed one.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_switch_parks_nothing_while_a_renewal_of_the_login_stored_waits() {
        let m = lapsed_stored("identify-switch-waits");
        killed_once_answered(&m);
        let [copy] = copies_of(&m, "here-again")
            .try_into()
            .expect("one copy left");
        let mut state = state::load(&m.ctx).expect("state");
        record_in_use(
            &m.ctx,
            &mut state,
            ProviderId::Claude,
            &lapsed("here-renewed"),
        );
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        m.mem
            .vault()
            .fault(&copy, Fault::Unreadable("damaged".into()));
        let parked = |m: &Machine| {
            state::load(&m.ctx)
                .expect("state")
                .get(&m.key("here"))
                .and_then(|account| account.parked.clone())
                .map(|parked| parked.refresh_fingerprint)
        };
        let pitboard = Pitboard::new(m.ctx.clone());

        let refused = pitboard
            .switch_to("there")
            .expect_err("a renewal of the login stored waits")
            .error;

        assert_eq!(refused.code(), "credential_store_unreadable");
        assert_eq!(parked(&m), None);
        assert_eq!(stored_refresh(&m).as_deref(), Some("here-renewed"));
        m.mem.vault().heal(&copy);
        pitboard.switch_to("there").expect("switched");
        assert_eq!(parked(&m), Some(crate::store::fingerprint("here-again")));
        assert_eq!(m.mem.vault().peek(&copy), None);
        assert_eq!(renewals(&m), [Asked::Renew("here-renewed".into())]);
        hold(&m, "after a switch from a renewal saved late");
    }

    /// On macOS the vault is the keychain Claude Code's login is in. Locked once that login
    /// is read again under Claude Code's write lock, it stops the renewal before the refresh
    /// token is sent, since the copy's name is written down first and the vault has to answer
    /// for that: nothing is sent and nothing written.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_keychain_locked_before_the_refresh_token_is_sent_stops_the_renewal() {
        let m = lapsed_stored("identify-locked-before-sending");
        let before = state_file(&m);

        let refused =
            crate::fault::meanwhile("refresh.checked", locks_the_keychain(&m), || told_now(&m))
                .err()
                .expect("the keychain is locked");

        assert_eq!(refused.code(), "credential_store_locked");
        assert_eq!(renewals(&m), []);
        assert_eq!(state_file(&m), before);
        m.mem.unlock_keychain();
        assert_eq!(owner_told(told_now(&m)), owner("here"));
        assert_eq!(stored_refresh(&m).as_deref(), Some("here-again"));
    }

    /// Locked while Anthropic answers, the keychain takes neither the copy of the renewed
    /// login nor the renewed login itself, so the renewal is lost and the refresh token stored
    /// is spent. What stops says so, and to sign in, and nothing names a copy that was never
    /// made.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_keychain_locked_while_anthropic_answers_says_the_renewal_is_lost() {
        const LOST: &str = "Pitboard renewed Claude Code's login and could not save the renewed \
                            login, so the refresh token stored is spent. Run `claude`, sign in, \
                            then try again.";
        let m = lapsed_stored("identify-locked-while-answering");
        m.api.while_renewing("here-renewed", locks_the_keychain(&m));

        let refused = told_now(&m).err().expect("the keychain is locked");

        assert_eq!(refused.code(), "credential_store_locked");
        assert!(refused.to_string().ends_with(LOST), "{refused}");
        assert_eq!(state::load(&m.ctx).expect("state").renewing, []);
        assert_eq!(copies_of(&m, "here-again"), Vec::<String>::new());
    }

    /// The keychain locks once the copy of the renewed login is written into the vault and
    /// before it is read back, so the copy's write cannot tell, and the renewed login's save
    /// fails. The copy is there all the same, the only renewed login: its name is kept, and
    /// the next change saves it, renewing nothing again.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_copy_written_and_not_read_back_is_kept_for_the_next_change() {
        let m = lapsed_stored("identify-copy-not-read-back");
        m.mem.vault().fault_all(Fault::LocksAfterWrite);
        let (keychain, service) = m.live_store();
        let locks = {
            let (keychain, service) = (Arc::clone(&keychain), service.clone());
            move || keychain.fault(&service, Fault::Locked)
        };

        let refused = crate::fault::meanwhile("refresh.exchanged", locks, || told_now(&m))
            .err()
            .expect("the keychain is locked");

        assert_eq!(refused.code(), "credential_store_locked");
        assert_eq!(copies_of(&m, "here-again").len(), 1);
        m.mem.vault().heal_all();
        keychain.heal(&service);
        m.api.renew_trouble("here-renewed", Trouble::InvalidGrant);
        assert_eq!(owner_told(told_now(&m)), owner("here"));
        assert_eq!(stored_refresh(&m).as_deref(), Some("here-again"));
        assert_eq!(renewals(&m), [Asked::Renew("here-renewed".into())]);
        assert_eq!(copies_of(&m, "here-again"), Vec::<String>::new());
        hold(&m, "after a copy saved late");
    }

    /// A renewed login saved without Claude Code's write lock, which was never let go of
    /// once Anthropic had answered, is said as a switch says a lock it lost.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_renewal_saved_without_the_write_lock_is_said() {
        let m = lapsed_stored("identify-lock-lost");

        let done = crate::fault::meanwhile("refresh.exchanged", jams_the_write_lock(&m), || {
            Pitboard::new(m.ctx.clone()).update_config("here")
        })
        .expect("in use");

        assert_eq!(stored_refresh(&m).as_deref(), Some("here-again"));
        let codes: Vec<&str> = done
            .warnings
            .iter()
            .map(crate::service::Warning::code)
            .collect();
        assert_eq!(codes, ["lock_compromised"]);
    }

    /// A read asks Anthropic whose a login is that changed since it last named one, and
    /// renews it first where its access token has lapsed, as a change does: the account in
    /// use is told and its usage asked with the renewed login.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_read_renews_a_lapsed_login_to_tell_whose_it_is() {
        let m = lapsed_stored("identify-read-renews");
        m.api.using(
            "access-here-again",
            crate::usage::Snapshot {
                windows: vec![window("session", 40.0)],
                observed_at: Some(NOW),
                answered_at: Some(NOW),
                lists_every_limit: true,
                source: crate::usage::Source::Live,
            },
        );

        let read = Pitboard::new(m.ctx.clone()).status(false).expect("a read");

        let here = read
            .value
            .rows
            .iter()
            .find(|row| row.signed_in)
            .expect("an account in use");
        assert_eq!(here.label.as_deref(), Some("here"));
        assert_eq!(here.stale, None);
        assert_eq!(
            here.usage.as_ref().map(|usage| usage.windows[0].percent),
            Some(40.0)
        );
        assert_eq!(renewals(&m), [Asked::Renew("here-renewed".into())]);
        assert_eq!(stored_refresh(&m).as_deref(), Some("here-again"));
        let state = state::load(&m.ctx).expect("state");
        assert_eq!(
            state.in_use(ProviderId::Claude).map(|r| r.login.clone()),
            Some(crate::store::fingerprint("here-again"))
        );
    }

    /// Nothing is renewed for its usage alone. The login Anthropic last named, lapsed, is
    /// known by its fingerprint, so its usage is asked as it is, and Claude Code renews it.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_read_never_renews_a_lapsed_login_it_knows_for_its_usage() {
        let m = machine("identify-read-known-lapsed");
        m.sign_in(&lapsed("here-refresh"));
        renews(&m, "here-refresh", "here-again");

        let read = Pitboard::new(m.ctx.clone()).status(false).expect("a read");

        let here = read
            .value
            .rows
            .iter()
            .find(|row| row.signed_in)
            .expect("an account in use");
        assert_eq!(here.label.as_deref(), Some("here"));
        assert_eq!(here.stale, Some(crate::status::Stale::SessionExpired));
        assert_eq!(renewals(&m), []);
        assert_eq!(stored_refresh(&m).as_deref(), Some("here-refresh"));
    }

    /// A read that finds the login refused for good says the account in use has to sign in
    /// again, in words for a login in use rather than a parked one, and writes nothing.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_read_says_sign_in_again_where_the_login_is_refused_for_good() {
        let m = machine("identify-read-refused");
        m.sign_in(&lapsed("here-renewed"));
        m.api.renew_trouble("here-renewed", Trouble::InvalidGrant);
        let stored = m.live();

        let read = Pitboard::new(m.ctx.clone()).status(false).expect("a read");

        let here = read
            .value
            .rows
            .iter()
            .find(|row| row.signed_in)
            .expect("the account last named");
        assert_eq!(here.stale, Some(crate::status::Stale::LoginRefused));
        assert_eq!(
            here.explanation(),
            Some("Claude Code's login is no longer accepted; run `claude` and sign in again")
        );
        assert_eq!(m.live(), stored);
    }

    /// A read that renews the login stored and can neither save the renewed login nor keep a
    /// copy of it has spent the refresh token stored, which Anthropic then refuses: the
    /// account in use is marked so, in words that say to sign in again.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_read_that_loses_the_renewal_says_sign_in_again() {
        let m = lapsed_stored("identify-read-lost");
        // A copy would go on the argument line, so none is made.
        m.mem.vault().takes_on_stdin(16);
        let (keychain, service) = m.live_store();
        m.api.while_renewing("here-renewed", move || {
            keychain.fault(&service, Fault::Locked);
        });

        let read = Pitboard::new(m.ctx.clone()).status(false).expect("a read");

        let here = read
            .value
            .rows
            .iter()
            .find(|row| row.signed_in)
            .expect("the account last named");
        assert_eq!(here.stale, Some(crate::status::Stale::LoginRefused));
        assert_eq!(
            here.explanation(),
            Some("Claude Code's login is no longer accepted; run `claude` and sign in again")
        );
        assert_eq!(renewals(&m), [Asked::Renew("here-renewed".into())]);
    }

    /// Codex's login says whose it is by itself, so it is never renewed to tell, however
    /// long ago its access token lapsed.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_codex_login_is_told_by_its_claims_and_never_renewed() {
        let m = codex_machine("identify-codex-lapsed");
        let mut login = codex_login("here", "here-again");
        login["tokens"]["access_token"] = serde_json::json!(crate::provider::jwt::unsigned(
            &serde_json::json!({"exp": NOW - 60, "for": "here-again"})
        ));
        m.sign_in(&login);

        let Settled {
            _exclusive,
            mut state,
            ctx,
            permit,
        } = super::super::settle(&m.ctx, Permit::for_a_test(), None)
            .expect("settled")
            .0;
        let Live::Login(told) = now(&ctx, permit, &mut state, ProviderId::Codex).expect("told")
        else {
            panic!("a login is stored");
        };

        assert_eq!(told.owner.account_uuid, codex_id("here"));
        assert_eq!(m.api.asked(), []);
    }
}
