//! Keeping parked logins alive, so their usage can be asked and they do not lapse. A parked
//! login is held by Pitboard alone, so renewing it puts no second holder on its refresh
//! chain. The login signed in is Claude Code's, and is never renewed here.

use super::{journal, purge, try_exclusive};
use crate::context::Context;
use crate::error::{Error, Result};
use crate::permit::Permit;
use crate::provider::{LoginKind, Provider};
use crate::state::{Key, Park, ParkKind, State};
use crate::{park, state};
use serde_json::Value;

/// Renewed this long before its access token expires, so a read just after still answers.
const AHEAD_SECONDS: i64 = 120;

/// What Claude Code asks for when a login records no scopes of its own.
pub(crate) const DEFAULT_SCOPES: [&str; 6] = [
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

/// Why a parked login is being renewed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Due {
    /// What a reading needs: a park whose access token has lapsed cannot be asked about.
    /// This is the one `status` does, and it is the read path's own requirement rather
    /// than a job it does on the side.
    ToBeAsked,
    /// What keeps a park usable: a refresh token has a finite life, and one that lapses
    /// costs a browser sign-in. Also covers everything `ToBeAsked` covers.
    ToStayAlive,
}

impl Due {
    fn covers(self, held: &Park, now: i64) -> bool {
        if !held.restorable_at(now) {
            return false;
        }
        let access_lapsed = !held.askable_at(now + AHEAD_SECONDS);
        match self {
            Due::ToBeAsked => access_lapsed,
            Due::ToStayAlive => {
                access_lapsed
                    || held
                        .refresh_expires_at
                        .is_some_and(|at| state::renewal_due(at, now))
            }
        }
    }
}

/// Renew every parked login whose access token has expired or is about to. Nothing is done
/// while another Pitboard run holds the lock or a switch waits to be finished: a renewal
/// replaces the refresh token, and nothing may install the old copy meanwhile.
pub(crate) fn renew_parked(ctx: &Context, permit: Permit) -> Vec<(Key, Renewal)> {
    renew_due(ctx, permit, Due::ToBeAsked)
}

/// The same, for whichever reason.
pub(crate) fn renew_due(ctx: &Context, permit: Permit, due: Due) -> Vec<(Key, Renewal)> {
    let Some(_exclusive) = try_exclusive(ctx, permit) else {
        return Vec::new();
    };
    if journal::pending(ctx) {
        return Vec::new();
    }
    let Ok(mut state) = state::load(ctx) else {
        return Vec::new();
    };
    let now = ctx.now();
    let covered = |state: &State| -> Vec<(Key, &'static dyn Provider, Park)> {
        state
            .accounts
            .iter()
            .filter_map(|a| {
                let tool = match crate::provider::of(a.provider()) {
                    LoginKind::Document(tool) => tool,
                    // An app's login is never renewed.
                    LoginKind::Folder(_) => return None,
                };
                let held = a.parked.as_ref()?;
                due.covers(held, now).then(|| (a.key(), tool, held.clone()))
            })
            .collect()
    };
    let mut to_renew = covered(&state);
    // A park that copies the login in use holds the refresh token its tool is about to
    // present, and renewing it would spend that token under the tool. It is dropped instead,
    // as the next change would drop it. Looked for only when something is due, because
    // finding one reads each tool's login.
    if !to_renew.is_empty() {
        let _ = super::drop_live_twins(ctx, permit, &mut state);
        to_renew = covered(&state);
    }
    // Each renewal is a round trip that can take as long as the request timeout, so they
    // are asked together. What comes back is then written one at a time: the state file is
    // one file, and the order of writes to it is not something to leave to chance.
    let asked: Vec<(Key, &'static dyn Provider, Park, Result<Asked>)> =
        std::thread::scope(|scope| {
            let handles: Vec<_> = to_renew
                .into_iter()
                .map(|(key, tool, held)| {
                    let ctx = &*ctx;
                    let handle = scope.spawn({
                        let key = key.clone();
                        let held = held.clone();
                        move || ask(ctx, permit, tool, &key, &held)
                    });
                    (key, tool, held, handle)
                })
                .collect();
            handles
                .into_iter()
                .map(|(key, tool, held, handle)| {
                    let answer = handle.join().unwrap_or_else(|_| {
                        Err(Error::RenewalFailed {
                            label: key.typed(),
                            cause: None,
                            detail: "the renewal thread stopped".into(),
                        })
                    });
                    (key, tool, held, answer)
                })
                .collect()
        });
    let outcomes = asked
        .into_iter()
        .map(|(key, tool, held, answer)| {
            let outcome = apply(ctx, permit, &mut state, tool, &key, &held, answer)
                .unwrap_or_else(Renewal::Failed);
            (key, outcome)
        })
        .collect();
    purge(ctx, permit, &mut state);
    outcomes
}

/// What one round trip produced, before anything is written down.
struct Asked {
    /// The parked document with fresh tokens already folded in, the way the tool that owns
    /// it stores its own after renewing, so it reads the same once restored.
    renewed: Option<Value>,
    /// The service refuses this login for good.
    refused: bool,
}

/// The part of a renewal that talks to Anthropic. Touches no shared state, so several run
/// at once.
fn ask(
    ctx: &Context,
    permit: Permit,
    tool: &'static dyn Provider,
    key: &Key,
    held: &Park,
) -> Result<Asked> {
    let document = park::load(ctx, tool, key, held)?;
    // The service spends the old refresh token as it answers, so an answer that could not
    // be written back would lose the login. Where it could not be, the service is not
    // asked: a renewed login is this one with new tokens of the same length, so this one's
    // size answers for it. On macOS with PITBOARD_NO_ARGV, that is every Codex park.
    park::price(ctx, key.provider, &key.typed(), &held.service, &document)?;
    let credential = crate::provider::Credential::new(key.provider, document);
    match tool.renew(ctx, permit, &credential) {
        Ok(fresh) => Ok(Asked {
            renewed: Some(fresh.raw),
            refused: false,
        }),
        Err(crate::provider::ProviderError::InvalidGrant { .. }) => Ok(Asked {
            renewed: None,
            refused: true,
        }),
        // Kept, as Claude Code keeps it: the login works again once the hold ends.
        Err(crate::provider::ProviderError::OnHold { link, .. }) => Err(Error::AccountOnHold {
            tool: key.provider,
            login: crate::error::Unidentified::Parked { label: key.typed() },
            link,
        }),
        // Unreachable or asked to slow down: nothing is written and the next run tries.
        Err(
            crate::provider::ProviderError::Network { .. }
            | crate::provider::ProviderError::RateLimited { .. },
        ) => Ok(Asked {
            renewed: None,
            refused: false,
        }),
        Err(e) => Err(Error::RenewalFailed {
            label: key.typed(),
            cause: Some(crate::error::Cause::of_provider(&e)),
            detail: e.to_string(),
        }),
    }
}

/// Renew one parked login now, for a caller that needs it usable rather than merely
/// present. Returns the park that replaces it, or `None` when Anthropic could not be
/// reached or asked for less traffic, which is a reason to stop and not a reason to act.
pub(super) fn renew_one(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    tool: &'static dyn Provider,
    key: &Key,
    held: &Park,
) -> Result<Option<Park>> {
    match apply(
        ctx,
        permit,
        state,
        tool,
        key,
        held,
        ask(ctx, permit, tool, key, held),
    )? {
        Renewal::Renewed => Ok(state.get(key).and_then(|a| a.parked.clone())),
        Renewal::Refused => Err(Error::ParkedLoginRefused {
            tool: key.provider,
            label: state.typed(key),
        }),
        Renewal::Deferred => Ok(None),
        Renewal::Failed(e) => Err(e),
    }
}

/// The part that writes: one at a time, in the order the accounts are listed.
fn apply(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    tool: &'static dyn Provider,
    key: &Key,
    held: &Park,
    asked: Result<Asked>,
) -> Result<Renewal> {
    let asked = asked?;
    if asked.refused {
        // Refused, not spent: nothing was taken from it. One `repair` gave back is left for
        // the Pitboard that wrote it, which will be refused the same way.
        state.release(&held.service);
        state::save(ctx, permit, state)?;
        return Ok(Renewal::Refused);
    }
    let Some(next) = asked.renewed else {
        return Ok(Renewal::Deferred);
    };
    // A copy `repair` gave back is left for the Pitboard that wrote it only while it is
    // unused, and the service has just spent it. Saved as used before the answer is
    // written, so a run killed between writing the answer and recording it leaves a spent
    // copy the next change deletes, rather than one it lets go for a Pitboard that would
    // present a spent token. A save that fails here must not stop the answer being written:
    // that is the account's only working login now.
    if state.is_foreign(&held.service) {
        state.used_here(&held.service);
        let _ = state::save(ctx, permit, state);
    }

    // The old refresh token may already be spent, so the answer is written at once, and a
    // second time under another name if the first write fails.
    let id = state.get(key).map(|a| a.id.clone()).unwrap_or_default();
    let store = || {
        park::reserve(ctx, permit, &id)
            .and_then(|service| park::store_at(ctx, permit, tool, &service, &next))
    };
    let parked = match store().or_else(|_| store()) {
        Ok(parked) => parked,
        Err(e) => {
            // Anthropic has already spent the old refresh token, so the copy Pitboard holds
            // is dead whatever happens next. Dropping it now means status stops offering a
            // login that cannot work and says to sign in again instead.
            state.discard(&held.service, ParkKind::Vault);
            let _ = state::save(ctx, permit, state);
            return Err(Error::RenewalFailed {
                label: state.typed(key),
                // The service answered; it is this machine that could not keep the answer.
                cause: None,
                detail: e.to_string(),
            });
        }
    };
    crate::fault::point("renew.park_stored");
    // The renewal spent the copy it replaces, whoever wrote it, so that one is discarded
    // rather than merely replaced.
    state.discard(&held.service, ParkKind::Vault);
    state.park(key, parked.clone());
    // A save that fails leaves the fresh copy where it is. Its name is on Pitboard's own
    // list of names it wrote, so the next command gives it back to the account in place of
    // the spent one. Deleting it here, as this once did, threw away the only login the
    // account had left: the service had already spent the one the record still names.
    state::save(ctx, permit, state)?;
    Ok(Renewal::Renewed)
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;
    use crate::api::scripted::{Asked as Question, ScriptedApi, Trouble};
    use crate::context::ScratchMachine;
    use crate::host::memory::MemoryHost;
    use crate::provider::claude::api::Renewed;
    use crate::state::Account;
    use crate::store::memory::Fault;
    use crate::time::FixedClock;
    use serde_json::json;
    use std::sync::Arc;

    const NOW: i64 = 1_760_000_000;

    struct Scratch {
        ctx: Context,
        mem: Arc<MemoryHost>,
        api: Arc<ScriptedApi>,
        home: tempfile::TempDir,
    }

    fn machine(name: &str) -> Scratch {
        let home = tempfile::Builder::new()
            .prefix(&format!("pitboard-renew-{name}-"))
            .tempdir()
            .expect("a scratch home");
        let ScratchMachine {
            ctx,
            host: mem,
            api,
        } = ScratchMachine::at(home.path(), |ctx| {
            ctx.with_pitboard_home(home.path().to_path_buf())
                .with_clock(Arc::new(FixedClock::at(NOW)) as Arc<dyn crate::time::Clock>)
        });
        Scratch {
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
    fn with_park(m: &Scratch, label: &str, refresh: &str, access_expires_at: i64) -> Park {
        park_in(&m.ctx, label, refresh, access_expires_at)
    }

    /// The same, in the Pitboard directory `ctx` names.
    fn park_in(ctx: &Context, label: &str, refresh: &str, access_expires_at: i64) -> Park {
        let service = park::reserve(ctx, Permit::for_a_test(), "acc").expect("a free name");
        let park = park::store_at(
            ctx,
            Permit::for_a_test(),
            crate::provider::document(crate::provider::ProviderId::Claude),
            &service,
            &oauth(refresh, access_expires_at),
        )
        .expect("parked");
        let mut state = State::default();
        state.accounts.push(Account {
            last_used_at: None,
            replaced_at: None,
            plan: None,
            label: label.into(),
            id: "acc".into(),
            account_uuid: "acc".into(),
            email: "me@example.com".into(),
            detail: state::Detail::Claude {
                organization_uuid: "org".into(),
                oauth_account: json!({}),
            },
            parked: Some(park.clone()),
        });
        state::save(ctx, Permit::for_a_test(), &state).expect("saved");
        park
    }

    fn fresh(refresh: &str) -> Renewed {
        Renewed {
            access_token: "new-access".into(),
            refresh_token: Some(refresh.into()),
            expires_in: 3600,
            refresh_token_expires_in: Some(30 * 86_400),
            scopes: None,
            at: None,
            owner: None,
        }
    }

    fn outcome(outcomes: &[(Key, Renewal)], label: &str) -> String {
        outcomes
            .iter()
            .find(|(key, _)| key.label == label)
            .map(|(_, r)| r.code().to_string())
            .unwrap_or_else(|| "not attempted".into())
    }

    /// The lifetimes a renewal answers with are relative, so what they are added to decides
    /// when the login expires. A machine whose clock is wrong must not get an expiry to
    /// match, or every status renews the park again and rotates the refresh chain on a loop.
    #[test]
    fn a_renewed_expiry_is_measured_from_anthropics_clock_not_this_machines() {
        let m = machine("anchored");
        with_park(&m, "work", "old", NOW - 1);
        // This machine believes it is two hours later than it is.
        let server_now = NOW - 7200;
        m.api.renews(
            "old",
            Renewed {
                access_token: "new-access".into(),
                refresh_token: Some("new".into()),
                expires_in: 3600,
                refresh_token_expires_in: Some(30 * 86_400),
                scopes: None,
                at: Some(server_now),
                owner: None,
            },
        );

        renew_parked(&m.ctx, Permit::for_a_test());

        let park = state::load(&m.ctx)
            .expect("state")
            .get(&Key::new(crate::provider::ProviderId::Claude, "work"))
            .expect("account")
            .parked
            .clone()
            .expect("renewed");
        assert_eq!(
            park.access_expires_at,
            Some(server_now + 3600),
            "an hour after the answer, not an hour after this machine's idea of now"
        );
        assert_eq!(park.refresh_expires_at, Some(server_now + 30 * 86_400));
    }

    /// An answer with no `Date` leaves the local clock as all there is, which is what it
    /// always was.
    #[test]
    fn an_answer_with_no_clock_of_its_own_falls_back_to_this_machines() {
        let m = machine("unanchored");
        with_park(&m, "work", "old", NOW - 1);
        m.api.renews("old", fresh("new"));

        renew_parked(&m.ctx, Permit::for_a_test());

        let park = state::load(&m.ctx)
            .expect("state")
            .get(&Key::new(crate::provider::ProviderId::Claude, "work"))
            .expect("account")
            .parked
            .clone()
            .expect("renewed");
        assert_eq!(park.access_expires_at, Some(NOW + 3600));
    }

    /// The budget question, asked of the code rather than of a stopwatch: a park whose
    /// access token is still good is not a reason to talk to Anthropic at all.
    #[test]
    fn a_park_that_is_not_due_is_not_asked_about() {
        let m = machine("not-due");
        with_park(&m, "work", "r", NOW + 3600);

        let outcomes = renew_parked(&m.ctx, Permit::for_a_test());

        assert!(outcomes.is_empty());
        assert_eq!(m.api.calls(), 0, "nothing was due, so nothing was asked");
    }

    #[test]
    fn a_due_park_is_renewed_and_the_spent_copy_is_dropped() {
        let m = machine("renewed");
        let before = with_park(&m, "work", "old", NOW - 1);
        m.api.renews("old", fresh("new"));

        let outcomes = renew_parked(&m.ctx, Permit::for_a_test());

        assert_eq!(outcome(&outcomes, "work"), "renewed");
        assert_eq!(m.api.asked(), vec![Question::Renew("old".into())]);
        let state = state::load(&m.ctx).expect("state");
        let now = state
            .get(&Key::new(crate::provider::ProviderId::Claude, "work"))
            .expect("account")
            .parked
            .clone()
            .expect("park");
        assert_ne!(now.service, before.service, "a renewal takes a new name");
        assert_eq!(
            m.mem.vault().services(),
            vec![now.service],
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

        let outcomes = renew_parked(&m.ctx, Permit::for_a_test());

        assert_eq!(outcome(&outcomes, "work"), "parked_login_refused");
        let state = state::load(&m.ctx).expect("state");
        assert!(
            state
                .get(&Key::new(crate::provider::ProviderId::Claude, "work"))
                .expect("account")
                .parked
                .is_none()
        );
        assert!(m.mem.vault().services().is_empty());
    }

    #[test]
    fn a_login_of_an_account_on_hold_is_kept() {
        let m = machine("on-hold");
        let before = with_park(&m, "work", "old", NOW - 1);
        m.api.renew_trouble("old", Trouble::OnHold);

        let outcomes = renew_parked(&m.ctx, Permit::for_a_test());

        assert_eq!(outcome(&outcomes, "work"), "account_on_hold");
        let state = state::load(&m.ctx).expect("state");
        let kept = state
            .get(&Key::new(crate::provider::ProviderId::Claude, "work"))
            .expect("account")
            .parked
            .clone();
        assert_eq!(kept.as_ref().map(|p| &p.service), Some(&before.service));
        assert_eq!(m.mem.vault().services(), vec![before.service]);
    }

    /// Being unreachable, or being asked to slow down, must change nothing at all: the park
    /// that is still there is the one thing standing between the user and a browser.
    #[test]
    fn a_renewal_that_could_not_happen_leaves_the_park_alone() {
        for trouble in [Trouble::Offline, Trouble::RateLimited] {
            let m = machine(&format!("deferred-{trouble:?}"));
            let before = with_park(&m, "work", "old", NOW - 1);
            m.api.renew_trouble("old", trouble);

            let outcomes = renew_parked(&m.ctx, Permit::for_a_test());

            assert_eq!(outcome(&outcomes, "work"), "renewal_deferred");
            let state = state::load(&m.ctx).expect("state");
            assert_eq!(
                state
                    .get(&Key::new(crate::provider::ProviderId::Claude, "work"))
                    .expect("account")
                    .parked,
                Some(before.clone()),
                "{trouble:?} must not spend or drop anything"
            );
            assert_eq!(m.mem.vault().services(), vec![before.service.clone()]);
        }
    }

    /// With the argument line forbidden, a login past the keychain's standard input cannot
    /// be written back, and the service spends the old refresh token as it answers. It is
    /// not asked: the park stays as it was, and the reason is said. It used to be asked,
    /// and the login was lost.
    #[test]
    fn a_park_that_could_not_be_written_back_is_not_renewed() {
        let mut m = machine("no-argv");
        m.ctx = m.ctx.clone().with_argv_fallback(false);
        let before = with_park(&m, "work", "old", NOW - 1);
        m.mem.vault().takes_on_stdin(16);
        m.api.renews("old", fresh("new"));

        let outcomes = renew_parked(&m.ctx, Permit::for_a_test());

        assert_eq!(outcome(&outcomes, "work"), "credential_too_large");
        assert_eq!(
            m.api.calls(),
            0,
            "the service was not asked, so nothing was spent"
        );
        let state = state::load(&m.ctx).expect("state");
        assert_eq!(
            state
                .get(&Key::new(crate::provider::ProviderId::Claude, "work"))
                .expect("account")
                .parked,
            Some(before.clone())
        );
        assert_eq!(m.mem.vault().services(), vec![before.service]);
    }

    /// The same login with the argument line allowed is renewed as usual.
    #[test]
    fn a_park_past_the_ceiling_is_renewed_where_the_argument_line_is_allowed() {
        let m = machine("argv");
        with_park(&m, "work", "old", NOW - 1);
        m.mem.vault().takes_on_stdin(16);
        m.api.renews("old", fresh("new"));

        let outcomes = renew_parked(&m.ctx, Permit::for_a_test());

        assert_eq!(outcome(&outcomes, "work"), "renewed");
    }

    /// The schedule belongs to the default home, and `launchctl setenv` or a systemd user
    /// environment can still give its job a `PITBOARD_HOME`. A run the schedule started
    /// renews the default home's parked logins and no other's, and a run by hand renews the
    /// ones in the home `PITBOARD_HOME` names. It renewed that home on every run, and on
    /// Linux nothing told a run of the schedule apart. Told both ways it is told: by the
    /// job the scheduler says started this process, which a test says here as launchd's
    /// `XPC_SERVICE_NAME` or systemd's unit would name it, and by the marker the systemd
    /// unit runs Pitboard with.
    #[test]
    fn a_run_the_schedule_started_renews_the_default_home_whatever_pitboard_home_says() {
        use crate::host::{OS, Os};
        use crate::service::Pitboard;
        let job = match OS {
            Os::MacOs => "com.datlechin.pitboard.renew".to_string(),
            Os::Linux => "pitboard-renew.service".to_string(),
            Os::Windows => crate::host::task::path(
                crate::schedule::Job::Renew,
                &crate::host::user::sid().expect("a SID"),
            ),
        };
        let job = job.as_str();
        type Told = fn(Context, &str) -> Context;
        let ways: [(&str, Told); 2] = [
            ("as the scheduler says", |ctx, job| {
                ctx.with_scheduled_job(job.into())
            }),
            ("by the marker", |ctx, _| ctx.started_by_the_schedule()),
        ];
        for (way, scheduled) in ways {
            let m = machine(&format!("scheduled-{}", way.len()));
            let default = m
                .ctx
                .clone()
                .with_pitboard_home(m.ctx.default_pitboard_home());
            let elsewhere = m
                .ctx
                .clone()
                .with_pitboard_home(m.home.path().join("elsewhere"));
            park_in(&default, "work", "default-old", NOW - 1);
            park_in(&elsewhere, "other", "elsewhere-old", NOW - 1);
            m.api.renews("default-old", fresh("default-new"));
            m.api.renews("elsewhere-old", fresh("elsewhere-new"));
            let renewed = |outcomes: Vec<(Key, Renewal)>| -> Vec<(String, String)> {
                outcomes
                    .into_iter()
                    .map(|(key, outcome)| (key.label, outcome.code().to_string()))
                    .collect()
            };

            let run = Pitboard::new(scheduled(elsewhere.clone(), job))
                .renew()
                .expect("a run");
            assert_eq!(renewed(run), [("work".into(), "renewed".into())], "{way}");
            assert_eq!(
                m.api.asked(),
                [Question::Renew("default-old".into())],
                "{way}"
            );
            assert_eq!(
                crate::audit::read(&default, 10)
                    .iter()
                    .map(|e| (e.verb.as_str(), e.outcome.as_str()))
                    .collect::<Vec<_>>(),
                [("renew", "renewed")],
                "{way}: recorded in the home it renewed"
            );

            let by_hand = Pitboard::new(elsewhere).renew().expect("a run");
            assert_eq!(
                renewed(by_hand),
                [("other".into(), "renewed".into())],
                "{way}"
            );
        }
    }

    /// A run of the schedule takes no notice of a `PITBOARD_HOME` that is empty or relative
    /// either, which every other run refuses: it renews the default home, and the gate it
    /// asks is asked of that home. By hand, the same `PITBOARD_HOME` is refused with
    /// `home_not_absolute`, having asked nobody.
    #[test]
    fn a_run_the_schedule_started_renews_the_default_home_where_pitboard_home_is_not_full() {
        use crate::service::Pitboard;
        for named in ["", "relative"] {
            let m = machine(&format!("scheduled-not-full-{}", named.len()));
            let default = m
                .ctx
                .clone()
                .with_pitboard_home(m.ctx.default_pitboard_home());
            park_in(&default, "work", "default-old", NOW - 1);
            m.api.renews("default-old", fresh("default-new"));
            let given = m.ctx.clone().with_pitboard_home(named.into());

            let by_hand = Pitboard::new(given.clone()).renew();
            assert!(
                matches!(by_hand, Err(Error::HomeNotAbsolute { .. })),
                "{named:?}: {by_hand:?}"
            );
            assert_eq!(m.api.asked(), [], "{named:?}");

            let run = Pitboard::new(given.started_by_the_schedule())
                .renew()
                .expect("a run");
            assert_eq!(
                run.iter()
                    .map(|(key, outcome)| (key.label.as_str(), outcome.code()))
                    .collect::<Vec<_>>(),
                [("work", "renewed")],
                "{named:?}"
            );
        }
    }

    /// A build for tests that runs as the schedule with this account's own home is stopped
    /// before it reads anything, since it would renew the real Pitboard directory: a test
    /// gives a run of the schedule a home of its own. Nothing here reads or writes the home;
    /// the context is all that is made.
    #[test]
    #[should_panic(expected = "with this account's own home")]
    fn a_run_the_schedule_started_in_a_build_for_tests_never_renews_the_real_home() {
        let own = {
            let _real = crate::host::user::testing::reaching_the_real_home();
            crate::host::user::home().expect("this account has a home")
        };
        let ctx = Context::new(own)
            .with_memory_stores(MemoryHost::new())
            .started_by_the_schedule();
        let _ = crate::schedule::for_its_run(&ctx, crate::schedule::Job::Renew);
    }

    /// The failure this module's comments describe and no test could reach: Anthropic has
    /// already spent the old refresh token, and the fresh one cannot be written down. The
    /// copy Pitboard holds is dead either way, so it is dropped rather than left to be
    /// offered as a login that cannot work.
    #[test]
    fn a_renewal_whose_answer_cannot_be_stored_drops_the_spent_park() {
        let m = machine("write-lost");
        let before = with_park(&m, "work", "old", NOW - 1);
        m.api.renews("old", fresh("new"));
        m.mem
            .vault()
            .fault_all(Fault::FailWrite("the keychain refused".into()));

        let outcomes = renew_parked(&m.ctx, Permit::for_a_test());

        assert_eq!(outcome(&outcomes, "work"), "renewal_failed");
        let state = state::load(&m.ctx).expect("state");
        assert!(
            state
                .get(&Key::new(crate::provider::ProviderId::Claude, "work"))
                .expect("account")
                .parked
                .is_none(),
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
