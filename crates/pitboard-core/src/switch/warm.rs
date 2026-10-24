//! Warming up an account, and changes to which accounts Pitboard warms up and when, under
//! the lock every change takes, so two front ends never warm up one account twice for one
//! period, nor write over each other's setting.
//!
//! A warm-up is decided again under the lock, from the files as they are then, and its attempt
//! recorded before anything runs, so however it ends, killed included, it counts against the
//! day's attempts. The lock is let go while the tool runs, which takes seconds, so no switch
//! waits on it, and taken again to record what it came to. A login is never handed on whole:
//! the tool gets its access token alone, renewed first where it would lapse during the run.

use super::{Result, Settled, identify, renew};
use crate::audit;
use crate::context::Context;
use crate::duty::ledger::Tries;
use crate::duty::warm::decide::{self, Ledger, Standing};
use crate::duty::warm::run;
use crate::duty::warm::{self, Failure, Range};
use crate::error::Error;
use crate::park;
use crate::permit::Permit;
use crate::provider::ProviderId;
use crate::provider::{self, claude};
use crate::state::Key;
use crate::status::{self, Login};
use crate::warning::Warning;
use serde_json::Value;
use std::collections::BTreeSet;

/// How long ahead of its expiry an access token is renewed before it is handed on: well past
/// what a run takes.
const AHEAD_SECONDS: i64 = 300;

/// What a warm-up came to.
#[derive(Debug, Clone, PartialEq)]
pub enum WarmUp {
    /// `account`'s five-hour period started, as it is typed, and resets at `until` where a
    /// reading taken after the run said when.
    Started { account: String, until: Option<i64> },
    /// `account`'s period was running already, until `until`, so there was nothing to start.
    Running { account: String, until: i64 },
    /// No account turned on was due by the time the lock was held.
    Nothing,
    /// Nothing is due, as a look at the files alone says, with where each account stands.
    Stands(Box<warm::Overview>),
}

/// Which account a warm-up is of.
pub(crate) enum Target {
    /// The first one turned on that is due, as Pitboard does by itself.
    Due,
    /// This one, which somebody asked for now.
    Account(Key),
}

/// Warms up `target`, as [`crate::duty::warm`] says when, and records what it came to in the
/// activity log as `warm-up` of the account, with `ok` or why it started nothing. With what a
/// change of Claude Code's login warns of where its login in use was renewed.
pub(crate) fn warm_up(settled: Settled, target: Target) -> Result<(WarmUp, Vec<Warning>)> {
    let Settled {
        _exclusive: exclusive,
        mut state,
        ctx,
        permit,
    } = settled;
    let ctx = &ctx;
    let now = ctx.now();
    let zone = ctx.zone();
    let setting = warm::load(ctx)?;
    let overview = warm::overview(ctx, &state, &setting);
    let (key, standing) = match target {
        Target::Account(key) => {
            let standing = overview
                .accounts
                .iter()
                .find(|choice| choice.qualified == key.qualified())
                .map(|choice| choice.now.clone());
            (key, standing)
        }
        Target::Due => {
            let due = overview
                .accounts
                .iter()
                .find(|choice| choice.standing == Some(Standing::Due))
                .and_then(|choice| {
                    state
                        .accounts
                        .iter()
                        .map(crate::state::Account::key)
                        .find(|key| key.qualified() == choice.qualified)
                });
            match due {
                Some(key) => (key, Some(Standing::Due)),
                None => return Ok((WarmUp::Nothing, Vec::new())),
            }
        }
    };
    let label = state.typed(&key);
    let account = state
        .get(&key)
        .cloned()
        .ok_or_else(|| Error::AccountUnknown {
            label: label.clone(),
            enrolled: state.labels(key.provider),
        })?;
    let Some(standing) = standing.filter(|_| key.provider == ProviderId::Claude) else {
        return Err(Error::WarmUpUnsupported {
            tool: key.provider,
            label,
        });
    };
    if let Some(halt) = warm::halted(ctx) {
        return Err(Error::WarmUpHalted { halt });
    }
    match standing {
        Standing::Due => {}
        Standing::Running { until, .. } => {
            let running = WarmUp::Running {
                account: label,
                until,
            };
            return Ok((running, Vec::new()));
        }
        Standing::Held(hold) => return Err(not_due(label, &hold, now)),
        Standing::Resting { .. } | Standing::Waiting { .. } => {
            unreachable!("asked for now, nothing rests or waits")
        }
    }
    let run_lock = RunLock::claim(ctx, permit)?;
    // Counted before the login is fetched, so an account whose login cannot be had is not the
    // first one due at every look, ahead of the others.
    let mut ledger = Ledger::load(ctx);
    ledger.of(&account.id, now, &zone).tries.attempt(now);
    ledger.save(ctx, permit)?;
    let home = run::home(ctx, ProviderId::Claude);
    let handed = hand_over(ctx, permit, &mut state, &key, &account, &home, now);
    // The tool runs for seconds; nothing else waits on Pitboard's lock meanwhile.
    drop(exclusive);
    let came = handed.and_then(|handed| {
        let ran = run::run(ctx, permit, handed.headless);
        let started = ran
            .map_err(|error| Failure::Failed(error.to_string()))
            .and_then(|ran| {
                if ran.timed_out {
                    return Err(Failure::TimedOut);
                }
                claude::warm::verdict(&ran)
            })
            .and_then(|()| started(ctx, permit, &account.id, &handed.login, handed.which));
        if started == Err(Failure::LimitReached) {
            // So what holds it back from now on is the reading's, which says until when.
            let _ = status::ask_one(
                ctx,
                permit,
                provider::claude_code(),
                &account.id,
                &handed.login,
                handed.which,
            );
        }
        started
            .map(|until| (until, handed.warnings))
            .map_err(|why| Error::WarmUpFailed {
                label: label.clone(),
                why,
            })
    });
    run::clear(permit, &home);
    drop(run_lock);
    record(ctx, permit, &account.id, &label, now, &came)?;
    came.map(|(until, warnings)| {
        let started = WarmUp::Started {
            account: label,
            until,
        };
        (started, warnings)
    })
}

// `warnings`: what renewing the login warned of.
struct Handed {
    headless: run::Headless,
    login: Value,
    which: Login,
    warnings: Vec<Warning>,
}

/// The run for `account`, given its access token alone, renewed first where it would lapse
/// during the run: its parked login, or the one Claude Code has stored where it is in use.
fn hand_over(
    ctx: &Context,
    permit: Permit,
    state: &mut crate::state::State,
    key: &Key,
    account: &crate::state::Account,
    home: &std::path::Path,
    now: i64,
) -> Result<Handed> {
    let label = state.typed(key);
    let (login, which, warnings) = match account.parked.clone() {
        Some(mut held) => {
            if held
                .access_expires_at
                .is_some_and(|at| at <= now + AHEAD_SECONDS)
            {
                held = renew::renew_one(ctx, permit, state, provider::claude_code(), key, &held)?
                    .ok_or_else(|| Error::WarmUpFailed {
                    label: label.clone(),
                    why: Failure::Unreachable,
                })?;
            }
            (
                park::load(ctx, provider::claude_code(), key, &held)?,
                Login::Parked,
                Vec::new(),
            )
        }
        None => {
            let (live, found) =
                identify::fresh(ctx, permit, state, provider::claude_code(), AHEAD_SECONDS)?;
            identify::keep(ctx, permit, state, provider::claude_code(), found)?;
            match live {
                identify::Live::Login(login) if account.owned_by(&login.owner) => {
                    let warnings = login.lock_warning(ProviderId::Claude).into_iter().collect();
                    (login.document, Login::Live, warnings)
                }
                _ => return Err(not_due(label, &warm::Hold::NothingParked, now)),
            }
        }
    };
    run::prepare(permit, home).map_err(|source| Error::HomeUnwritable {
        path: home.to_path_buf(),
        source,
    })?;
    let headless =
        claude::warm::headless(ctx, permit, home, &login).map_err(|error| Error::WarmUpFailed {
            label,
            why: Failure::Failed(error.to_string()),
        })?;
    Ok(Handed {
        headless,
        login,
        which,
        warnings,
    })
}

// As the runs measured asked, 20 seconds after (`request_starts_the_five_hour_period`).
const ASKED_AGAIN_AFTER: u64 = 20;

// The reset a reading after the run gives, asked once more, as late as the runs measured asked,
// where the first has no period running.
fn started(
    ctx: &Context,
    permit: Permit,
    id: &str,
    login: &Value,
    which: Login,
) -> std::result::Result<Option<i64>, Failure> {
    for pause in [0, ASKED_AGAIN_AFTER] {
        ctx.clock.pause(pause);
        let Some(reading) = status::ask_one(ctx, permit, provider::claude_code(), id, login, which)
        else {
            return Ok(None);
        };
        let until = decide::startable(&reading)
            .and_then(|session| session.resets_at)
            .filter(|&at| at > ctx.now());
        if until.is_some() {
            return Ok(until);
        }
    }
    Err(Failure::NotStarted)
}

/// What a warm-up of the account with `id`, begun at `began`, came to, kept under the lock
/// again, and written in the activity log. The next attempt waits from when this one ended.
fn record(
    ctx: &Context,
    permit: Permit,
    id: &str,
    label: &str,
    began: i64,
    came: &Result<(Option<i64>, Vec<Warning>)>,
) -> Result<()> {
    let _exclusive = super::exclusive(ctx, permit)?;
    let now = ctx.now();
    let mut ledger = Ledger::load(ctx);
    let warmed = ledger.of(id, now, &ctx.zone());
    match came {
        Ok(_) => {
            warmed.tries = Tries::default();
            warmed.started = Some(began);
        }
        Err(error) => {
            warmed.tries.at = now;
            if mended_by_waiting(error) {
                warmed.tries.waited();
            }
        }
    }
    let saved = ledger.save(ctx, permit);
    let code = came.as_ref().map_or_else(Error::code, |_| "ok");
    audit::record(ctx, permit, "warm-up", label, code);
    saved
}

/// Whether waiting may mend what a warm-up failed over, so it is not counted against the day's
/// attempts: a run that may answer later, or a service out of reach while the login was had.
fn mended_by_waiting(error: &Error) -> bool {
    match error {
        Error::WarmUpFailed { why, .. } => why.mended_by_waiting(),
        other => other
            .cause()
            .is_some_and(crate::error::Cause::worth_retrying),
    }
}

// A warm-up that came to nothing, by its account and code where it is of an account: Claude
// Code's words differ on every run.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Apart {
    pub account: Option<String>,
    pub key: String,
}

pub fn told_apart(error: &Error) -> Apart {
    match error {
        Error::WarmUpFailed { label, .. } | Error::WarmUpNotDue { label, .. } => Apart {
            account: Some(label.clone()),
            key: format!("{label}/{}", error.code()),
        },
        error => Apart {
            account: None,
            key: format!("{}/{error}", error.code()),
        },
    }
}

// What a front end has said of warm-ups that came to nothing: each once, until its account
// warms up.
#[derive(Debug, Default)]
pub struct Told(BTreeSet<Apart>);

impl Told {
    // Whether `apart` has not been said yet; it has from now on.
    pub fn untold(&mut self, apart: &Apart) -> bool {
        self.0.insert(apart.clone())
    }

    pub fn warmed(&mut self, account: &str) {
        self.0
            .retain(|apart| apart.account.as_deref() != Some(account));
    }
}

/// Why `label` is not warmed up when somebody asked for it now.
fn not_due(label: String, hold: &warm::Hold, now: i64) -> Error {
    Error::WarmUpNotDue {
        label,
        why: crate::words::warm_standing(&Standing::Held(hold.clone()), &|at| {
            crate::time::moment(at, now)
        }),
        code: hold.code(),
    }
}

// One warm-up's run at a time on this machine, held for as long as it runs, by whichever
// front end started it: two runs would share the tool's private home.
pub(super) struct RunLock {
    _lock: std::fs::File,
}

impl RunLock {
    fn path(ctx: &Context) -> std::path::PathBuf {
        crate::home::dir(ctx).join("warm.lock")
    }

    fn claim(ctx: &Context, permit: Permit) -> Result<RunLock> {
        let path = RunLock::path(ctx);
        let file = crate::host::fs::open_private_lock(permit, &path).map_err(|source| {
            Error::HomeUnwritable {
                path: path.clone(),
                source,
            }
        })?;
        match file.try_lock() {
            Ok(()) => Ok(RunLock { _lock: file }),
            Err(std::fs::TryLockError::WouldBlock) => Err(Error::WarmUpInProgress),
            Err(std::fs::TryLockError::Error(source)) => {
                Err(Error::HomeUnwritable { path, source })
            }
        }
    }

    pub(super) fn is_held(ctx: &Context) -> bool {
        std::fs::File::open(RunLock::path(ctx))
            .is_ok_and(|lock| matches!(lock.try_lock(), Err(std::fs::TryLockError::WouldBlock)))
    }
}

/// Warms up `key`'s account, or stops. Whether that changed anything.
pub(crate) fn turn(settled: Settled, key: &Key, on: bool) -> Result<bool> {
    let Settled {
        _exclusive,
        state,
        ctx,
        permit,
    } = settled;
    let account = state.get(key).ok_or_else(|| Error::AccountUnknown {
        label: key.typed(),
        enrolled: state.labels(key.provider),
    })?;
    if on && key.provider != ProviderId::Claude {
        return Err(Error::WarmUpUnsupported {
            tool: key.provider,
            label: key.typed(),
        });
    }
    let mut setting = warm::load(&ctx)?;
    let changed = setting.turn(&account.id, on);
    if changed {
        warm::save(&ctx, permit, &setting)?;
    }
    Ok(changed)
}

/// Warms up within `range` of each day.
pub(crate) fn range(settled: Settled, range: Range) -> Result<Range> {
    let Settled {
        _exclusive,
        ctx,
        permit,
        ..
    } = settled;
    let mut setting = warm::load(&ctx)?;
    if setting.range != range {
        setting.range = range;
        warm::save(&ctx, permit, &setting)?;
    }
    Ok(range)
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::super::harness::{audit_lines, codex_account, machine};
    use crate::duty::warm::{self, Range};
    use crate::error::Error;
    use crate::permit::Permit;
    use crate::service::Pitboard;
    use crate::state;

    #[test]
    fn a_failure_is_told_apart_by_its_account_and_code_and_not_its_words() {
        let failed = |label: &str, why| Error::WarmUpFailed {
            label: label.into(),
            why,
        };
        let said = |words: &str| warm::Failure::Failed(words.into());
        assert_eq!(
            super::told_apart(&failed("work", said("request 7f3a timed out"))),
            super::told_apart(&failed("work", said("request 91c0 timed out")))
        );
        assert_ne!(
            super::told_apart(&failed("work", said("a"))),
            super::told_apart(&failed("home", said("a")))
        );
        assert_ne!(
            super::told_apart(&failed("work", said("a"))),
            super::told_apart(&failed("work", warm::Failure::TimedOut))
        );
    }

    #[test]
    fn a_failure_is_said_again_once_its_own_account_warms_up() {
        let work = super::told_apart(&Error::WarmUpFailed {
            label: "work".into(),
            why: warm::Failure::TimedOut,
        });
        let busy = super::told_apart(&Error::WarmUpInProgress);
        let mut told = super::Told::default();
        assert!(told.untold(&work));
        assert!(told.untold(&busy));
        assert!(!told.untold(&work));
        told.warmed("home");
        assert!(!told.untold(&work), "another account warmed up");
        told.warmed("work");
        assert!(told.untold(&work));
        assert!(!told.untold(&busy), "of no account");
    }

    #[test]
    fn a_claude_code_account_is_turned_on_and_off_and_each_is_recorded() {
        let m = machine("warm-turn");
        let pitboard = Pitboard::new(m.ctx.clone());
        let done = pitboard.warm_turn("there", true).expect("turned on");
        assert!(done.value, "it changed");
        let again = pitboard.warm_turn("there", true).expect("turned on");
        assert!(!again.value, "already on");

        let setting = warm::load(&m.ctx).expect("readable");
        assert!(setting.is_on("there"));
        assert_eq!(setting.range, Range::DEFAULT);
        let overview = pitboard.warm_overview().expect("an overview");
        let on: Vec<(&str, bool)> = overview
            .accounts
            .iter()
            .map(|choice| (choice.account.as_str(), choice.on))
            .collect();
        assert_eq!(on, [("here", false), ("there", true)]);

        pitboard.warm_turn("there", false).expect("turned off");
        assert!(!warm::load(&m.ctx).expect("readable").any());
        assert_eq!(
            audit_lines(&m, "warm-on"),
            [
                ("there".to_owned(), "ok".to_owned()),
                ("there".to_owned(), "ok".to_owned())
            ]
        );
        assert_eq!(
            audit_lines(&m, "warm-off"),
            [("there".to_owned(), "ok".to_owned())]
        );
    }

    #[test]
    fn a_codex_account_is_not_warmed_up() {
        let m = machine("warm-codex");
        let mut state = state::load(&m.ctx).expect("state");
        state.accounts.push(codex_account("main", "main", None));
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        let pitboard = Pitboard::new(m.ctx.clone());
        let refused = pitboard.warm_turn("codex/main", true).expect_err("refused");
        assert_eq!(refused.error.code(), "warm_up_unsupported");
        let refused = pitboard.warm_now("codex/main").expect_err("refused");
        assert_eq!(refused.error.code(), "warm_up_unsupported");
        assert!(!warm::path(&m.ctx).exists(), "nothing written");
        pitboard
            .warm_turn("codex/main", false)
            .expect("turning one off is always fine");
    }

    #[test]
    fn a_claude_desktop_account_is_not_warmed_up_and_says_why() {
        let m = machine("warm-desktop");
        let mut state = state::load(&m.ctx).expect("state");
        state.accounts.push(state::Account {
            label: "work".into(),
            id: "desktop_work".into(),
            account_uuid: "work".into(),
            email: String::new(),
            parked: None,
            last_used_at: None,
            replaced_at: None,
            plan: None,
            detail: state::Detail::Desktop {},
        });
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        let pitboard = Pitboard::new(m.ctx.clone());
        let turned = pitboard
            .warm_turn("desktop/work", true)
            .expect_err("refused");
        let now = pitboard.warm_now("desktop/work").expect_err("refused");
        for refused in [turned.error, now.error] {
            assert_eq!(refused.code(), "warm_up_unsupported");
            assert_eq!(
                refused.to_string(),
                "`desktop/work` is a Claude Desktop account, and Pitboard sends no request for \
                 one, so it warms up none."
            );
        }
        assert!(!warm::path(&m.ctx).exists(), "nothing written");
    }

    #[test]
    fn every_system_warms_up_accounts() {
        use crate::host::Os;
        let m = machine("warm-windows");
        for os in [Os::MacOs, Os::Linux, Os::Windows] {
            let mut ctx = m.ctx.clone();
            ctx.os = os;
            let overview = Pitboard::new(ctx).warm_overview().expect("an overview");
            assert_eq!(overview.halted, None, "{os:?}");
        }
    }

    #[test]
    fn the_span_is_kept_and_recorded() {
        let m = machine("warm-range");
        let pitboard = Pitboard::new(m.ctx.clone());
        let range = Range::parse("07:30", "23:00").expect("a span");
        pitboard.warm_range(range).expect("kept");
        assert_eq!(warm::load(&m.ctx).expect("readable").range, range);
        assert_eq!(
            audit_lines(&m, "warm-range"),
            [("07:30-23:00".to_owned(), "ok".to_owned())]
        );
        let text = std::fs::read_to_string(warm::path(&m.ctx)).expect("written");
        assert_eq!(
            text,
            r#"{"version":1,"range":{"from":"07:30","to":"23:00"},"accounts":[]}"#
        );
    }

    #[test]
    fn a_forgotten_account_is_warmed_up_no_more() {
        let m = machine("warm-forget");
        let pitboard = Pitboard::new(m.ctx.clone());
        pitboard.warm_turn("there", true).expect("turned on");
        pitboard.forget("there").expect("forgotten");
        assert!(!warm::load(&m.ctx).expect("readable").any());
    }

    #[test]
    fn a_setting_a_newer_pitboard_wrote_is_neither_read_nor_written_over() {
        let m = machine("warm-newer");
        let path = warm::path(&m.ctx);
        std::fs::write(&path, r#"{"version":2,"days":["mon"]}"#).expect("written");
        let refused = warm::load(&m.ctx).expect_err("refused");
        assert!(matches!(
            refused,
            Error::StateFromNewerVersion {
                found: 2,
                expected: 1,
                ..
            }
        ));
        let pitboard = Pitboard::new(m.ctx.clone());
        assert!(pitboard.warm_turn("there", true).is_err());
        let kept = std::fs::read_to_string(&path).expect("still there");
        assert!(kept.contains("days"), "{kept}");

        std::fs::write(&path, "{not json").expect("written");
        assert_eq!(
            warm::load(&m.ctx).expect_err("refused").code(),
            "warm_setting_corrupt"
        );
    }

    mod running {
        use super::super::super::harness::{NOW, Rig, audit_lines, lapsed, machine, owner, renews};
        use super::super::WarmUp;
        use crate::duty::Look;
        use crate::duty::warm::run::{Ran, Started, WarmScript};
        use crate::duty::warm::{Range, Standing};
        use crate::permit::Permit;
        use crate::service::Pitboard;
        use crate::time::{Clock, FixedClock};
        use crate::usage::{Snapshot, Source, Window};
        use jiff::tz::TimeZone;
        use std::sync::{Arc, Mutex};

        const HOUR: i64 = 3_600;
        const ANSWERED: &str = r#"{"type":"result","subtype":"success","is_error":false,"result":"OK","api_error_status":null}"#;
        const LIMIT_REACHED: &str = r#"{"type":"result","subtype":"success","is_error":true,"api_error_status":429,"api_error":"usage_limit_reached","result":"You've hit your weekly limit"}"#;

        /// Claude Code's headless run, as a test plays it: what it prints, and every run it
        /// was started as.
        #[derive(Debug)]
        struct Script {
            ran: Ran,
            runs: Mutex<Vec<Started>>,
        }

        impl WarmScript for Script {
            fn run(&self, started: &Started) -> Ran {
                self.runs.lock().expect("runs").push(started.clone());
                self.ran.clone()
            }
        }

        impl Script {
            fn runs(&self) -> Vec<Started> {
                self.runs.lock().expect("runs").clone()
            }
        }

        fn window(kind: &str, percent: f64, resets_at: Option<i64>) -> Window {
            Window {
                kind: kind.into(),
                scope: None,
                percent,
                resets_at,
                is_active: true,
                severity: None,
                length_seconds: None,
            }
        }

        fn reading(session: Option<i64>, weekly: f64) -> Snapshot {
            Snapshot {
                windows: vec![
                    window("session", 0.0, session),
                    window("weekly_all", weekly, Some(NOW + 40 * HOUR)),
                ],
                observed_at: Some(NOW - 60),
                answered_at: Some(NOW - 60),
                lists_every_limit: true,
                source: Source::Live,
            }
        }

        /// Pitboard's reading of `id`: its five-hour period not running, with `weekly` used.
        fn idle(m: &Rig, id: &str, weekly: f64) {
            crate::usage::readings::answered(
                &m.ctx,
                Permit::for_a_test(),
                &[(id.to_owned(), reading(None, weekly))],
            );
        }

        /// [`machine`], with `there` turned on for warm-ups, at 08:53 UTC, a Claude Code program
        /// Pitboard finds, and its run played by a script that exits `exit` printing `prints`.
        fn warming(name: &str, exit: i32, prints: &str) -> (Rig, Arc<Script>) {
            let ran = Ran {
                exit: Some(exit),
                stdout: prints.to_owned(),
                timed_out: false,
            };
            warming_as(
                name,
                ran,
                Arc::new(FixedClock::at(NOW).in_zone(TimeZone::UTC)),
            )
        }

        /// [`warming`], with the run coming to `ran`, on `clock`.
        fn warming_as(name: &str, ran: Ran, clock: Arc<dyn Clock>) -> (Rig, Arc<Script>) {
            let mut m = machine(name);
            let program = m
                .ctx_home()
                .join("bin")
                .join(format!("claude{}", std::env::consts::EXE_SUFFIX));
            std::fs::create_dir_all(program.parent().expect("a folder")).expect("bin");
            std::fs::write(&program, "#!/bin/sh\nexit 99\n").expect("a program");
            crate::host::fs::testing::make_runnable(&program).expect("runnable");
            let script = Arc::new(Script {
                ran,
                runs: Mutex::new(Vec::new()),
            });
            m.ctx = m
                .ctx
                .clone()
                .with_claude_program(program)
                .with_clock(clock)
                .with_warm_script(Arc::clone(&script) as Arc<dyn WarmScript>);
            Pitboard::new(m.ctx.clone())
                .warm_turn("there", true)
                .expect("turned on");
            (m, script)
        }

        fn warmed(m: &Rig) -> crate::service::Changing<WarmUp> {
            Pitboard::new(m.ctx.clone()).warm_up()
        }

        #[test]
        fn a_parked_account_due_is_warmed_up_with_its_access_token_alone() {
            let (m, script) = warming("warm-parked", 0, ANSWERED);
            idle(&m, "there", 20.0);
            m.api
                .using("access-there-refresh", reading(Some(NOW + 5 * HOUR), 20.0));

            let done = warmed(&m).expect("warmed up").value;
            assert_eq!(
                done,
                WarmUp::Started {
                    account: "there".into(),
                    until: Some(NOW + 5 * HOUR)
                }
            );
            let runs = script.runs();
            assert_eq!(runs.len(), 1);
            let run = &runs[0];
            assert_eq!(
                run.args,
                [
                    "-p",
                    "Reply with the single word OK.",
                    "--model",
                    "haiku",
                    "--tools",
                    "",
                    "--no-session-persistence",
                    "--strict-mcp-config",
                    "--disable-slash-commands",
                    "--output-format",
                    "json"
                ]
            );
            let home = crate::home::dir(&m.ctx).join("warm").join("claude");
            let env = |name: &str| {
                run.env
                    .iter()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| value.clone())
            };
            let windows = m.ctx.os == crate::host::Os::Windows;
            let named = if windows {
                "CLAUDE_CODE_OAUTH_TOKEN"
            } else {
                "CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR"
            };
            assert_eq!(run.access_token(), Some("access-there-refresh"));
            assert_eq!(
                env("CLAUDE_CONFIG_DIR").map(std::path::PathBuf::from),
                Some(home.join("config"))
            );
            assert_eq!(
                env("HOME").map(std::path::PathBuf::from),
                Some(home.join("home"))
            );
            assert_eq!(
                env("CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR").as_deref(),
                (!windows).then_some("3")
            );
            assert_eq!(run.dir.as_deref(), Some(home.join("work").as_path()));
            let mut allowed = vec![
                "PATH",
                "HOME",
                "TMPDIR",
                "LANG",
                "TERM",
                "NO_COLOR",
                "CLAUDE_CONFIG_DIR",
                "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
                "DISABLE_AUTOUPDATER",
                named,
            ];
            allowed.extend(if windows {
                &[
                    "USERNAME",
                    "USERPROFILE",
                    "TEMP",
                    "TMP",
                    "SystemRoot",
                    "CLAUDE_CODE_SUBPROCESS_ENV_SCRUB",
                    "CLAUDE_CODE_GIT_BASH_PATH",
                ][..]
            } else {
                &["USER"][..]
            });
            for (name, _) in &run.env {
                assert!(allowed.contains(&name.as_str()), "{name} reached the run");
            }
            assert!(
                !run.env
                    .iter()
                    .filter(|(name, _)| name != "CLAUDE_CODE_OAUTH_TOKEN")
                    .any(|(_, value)| value.contains("there-refresh")),
                "no refresh token is handed on"
            );
            assert_eq!(
                audit_lines(&m, "warm-up"),
                [("there".to_owned(), "ok".to_owned())]
            );

            let again = warmed(&m).expect("looked again").value;
            assert!(
                matches!(&again, WarmUp::Stands(overview) if overview.accounts.iter().any(|c| {
                    c.account == "there" && matches!(c.standing, Some(Standing::Running { .. }))
                })),
                "its period runs now: {again:?}"
            );
            assert_eq!(script.runs().len(), 1);
        }

        /// The account in use is warmed up with the login Claude Code has stored, renewed first
        /// under Claude Code's own locks where it would lapse during the run, and saved where
        /// Claude Code keeps it.
        #[test]
        fn the_account_in_use_is_warmed_up_with_its_login_renewed_first_where_it_lapses() {
            let (m, script) = warming("warm-in-use", 0, ANSWERED);
            Pitboard::new(m.ctx.clone())
                .warm_turn("here", true)
                .expect("turned on");
            Pitboard::new(m.ctx.clone())
                .warm_turn("there", false)
                .expect("turned off");
            m.sign_in(&lapsed("here-refresh"));
            renews(&m, "here-refresh", "here-renewed");
            m.api.owned_by("access-here-renewed", owner("here"));
            idle(&m, "here", 20.0);
            m.api
                .using("access-here-renewed", reading(Some(NOW + 5 * HOUR), 20.0));

            let done = warmed(&m).expect("warmed up").value;
            assert!(matches!(done, WarmUp::Started { ref account, .. } if account == "here"));
            assert_eq!(script.runs()[0].access_token(), Some("access-here-renewed"));
            let live = m.live().expect("a login stored");
            assert_eq!(
                live.pointer("/claudeAiOauth/refreshToken"),
                Some(&serde_json::json!("here-renewed")),
                "the renewal is Claude Code's login from now on"
            );
        }

        #[test]
        fn a_limit_used_up_is_waited_on_and_not_counted() {
            let (m, _) = warming("warm-limit", 1, LIMIT_REACHED);
            idle(&m, "there", 20.0);
            m.api.using("access-there-refresh", reading(None, 100.0));
            let refused = warmed(&m).expect_err("refused");
            assert_eq!(refused.error.code(), "warm_up_limit_reached");
            let ledger = crate::duty::warm::decide::Ledger::load(&m.ctx);
            let tries = &ledger.get("there").expect("kept").tries;
            assert_eq!((tries.attempts, tries.waits), (0, 1));
            assert_eq!(
                audit_lines(&m, "warm-up"),
                [("there".to_owned(), "warm_up_limit_reached".to_owned())]
            );
            let overview = Pitboard::new(m.ctx.clone())
                .warm_overview()
                .expect("an overview");
            let there = overview
                .accounts
                .iter()
                .find(|choice| choice.account == "there")
                .expect("there");
            assert!(
                matches!(
                    there.standing,
                    Some(Standing::Held(crate::duty::warm::Hold::LimitUsedUp { .. }))
                ),
                "the reading after it says so: {:?}",
                there.standing
            );
        }

        #[test]
        fn a_request_answered_that_starts_nothing_counts_against_the_day() {
            let (m, _) = warming("warm-not-started", 0, ANSWERED);
            idle(&m, "there", 20.0);
            m.api.using("access-there-refresh", reading(None, 20.0));
            let refused = warmed(&m).expect_err("refused");
            assert_eq!(refused.error.code(), "warm_up_not_started");
            let ledger = crate::duty::warm::decide::Ledger::load(&m.ctx);
            assert_eq!(ledger.get("there").expect("kept").tries.attempts, 1);
        }

        #[test]
        fn asked_for_now_a_running_period_is_said_and_a_held_one_refused() {
            let (m, script) = warming("warm-now", 0, ANSWERED);
            let pitboard = Pitboard::new(m.ctx.clone());
            crate::usage::readings::answered(
                &m.ctx,
                Permit::for_a_test(),
                &[("there".to_owned(), reading(Some(NOW + HOUR), 20.0))],
            );
            assert_eq!(
                pitboard.warm_now("there").expect("said").value,
                WarmUp::Running {
                    account: "there".into(),
                    until: NOW + HOUR
                }
            );
            idle(&m, "there", 100.0);
            let held = pitboard.warm_now("there").expect_err("held");
            assert_eq!(held.error.code(), "limit_used_up");
            assert!(script.runs().is_empty());
        }

        #[test]
        fn nothing_is_warmed_up_outside_the_span() {
            let (m, script) = warming("warm-span", 0, ANSWERED);
            idle(&m, "there", 20.0);
            let pitboard = Pitboard::new(m.ctx.clone());
            pitboard
                .warm_range(Range::parse("20:00", "23:00").expect("a span"))
                .expect("kept");
            assert!(matches!(pitboard.warm_look(), Ok(Look::Stands(_))));
            assert!(matches!(
                pitboard.warm_up().expect("looked").value,
                WarmUp::Stands(_)
            ));
            assert!(script.runs().is_empty());
            pitboard
                .warm_range(Range::parse("06:00", "22:00").expect("a span"))
                .expect("kept");
            assert!(matches!(pitboard.warm_look(), Ok(Look::Act)));
        }

        /// A test's clock that does `meanwhile` the first time Pitboard pauses on it.
        struct Meanwhile {
            clock: FixedClock,
            meanwhile: Mutex<Option<Box<dyn FnOnce() + Send>>>,
        }

        impl std::fmt::Debug for Meanwhile {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct("Meanwhile")
                    .field("clock", &self.clock)
                    .finish()
            }
        }

        impl Clock for Meanwhile {
            fn now(&self) -> i64 {
                self.clock.now()
            }

            fn now_millis(&self) -> i64 {
                self.clock.now_millis()
            }

            fn zone(&self) -> TimeZone {
                self.clock.zone()
            }

            fn pause(&self, seconds: u64) {
                if seconds > 0
                    && let Some(meanwhile) = self.meanwhile.lock().expect("meanwhile").take()
                {
                    meanwhile();
                }
                self.clock.pause(seconds);
            }
        }

        /// A reading taken at once after the run may not show the period yet, so it is asked
        /// for again as late as the measured runs asked before the run counts as starting
        /// nothing.
        #[test]
        fn a_period_the_first_reading_after_the_run_misses_is_asked_about_again() {
            let api = Arc::new(Mutex::new(None::<Arc<crate::api::scripted::ScriptedApi>>));
            let later = Arc::clone(&api);
            let clock = Meanwhile {
                clock: FixedClock::at(NOW).in_zone(TimeZone::UTC),
                meanwhile: Mutex::new(Some(Box::new(move || {
                    if let Some(api) = later.lock().expect("api").as_ref() {
                        api.using("access-there-refresh", reading(Some(NOW + 5 * HOUR), 20.0));
                    }
                }))),
            };
            let answered = Ran {
                exit: Some(0),
                stdout: ANSWERED.into(),
                timed_out: false,
            };
            let (m, _) = warming_as("warm-asked-again", answered, Arc::new(clock));
            *api.lock().expect("api") = Some(Arc::clone(&m.api));
            idle(&m, "there", 20.0);
            m.api.using("access-there-refresh", reading(None, 20.0));

            let done = warmed(&m).expect("warmed up").value;
            assert_eq!(
                done,
                WarmUp::Started {
                    account: "there".into(),
                    until: Some(NOW + 5 * HOUR)
                }
            );
            assert_eq!(m.ctx.now(), NOW + 20, "asked again 20 seconds on");
        }

        #[test]
        fn a_run_past_its_ceiling_waits_its_turn_and_is_not_counted() {
            let timed_out = Ran {
                exit: None,
                stdout: String::new(),
                timed_out: true,
            };
            let clock = Arc::new(FixedClock::at(NOW).in_zone(TimeZone::UTC));
            let (m, _) = warming_as("warm-timed-out", timed_out, clock);
            idle(&m, "there", 20.0);
            let refused = warmed(&m).expect_err("refused");
            assert_eq!(refused.error.code(), "warm_up_timed_out");
            let ledger = crate::duty::warm::decide::Ledger::load(&m.ctx);
            let tries = &ledger.get("there").expect("kept").tries;
            assert_eq!((tries.attempts, tries.waits), (0, 1));
            assert!(matches!(
                Pitboard::new(m.ctx.clone()).warm_look(),
                Ok(Look::Stands(_))
            ));
        }

        #[test]
        fn a_refused_login_counts_against_the_day() {
            let (m, _) = warming(
                "warm-login-refused",
                1,
                r#"{"is_error":true,"api_error_status":401,"result":"Invalid bearer token"}"#,
            );
            idle(&m, "there", 20.0);
            let refused = warmed(&m).expect_err("refused");
            assert_eq!(refused.error.code(), "warm_up_login_refused");
            let ledger = crate::duty::warm::decide::Ledger::load(&m.ctx);
            assert_eq!(ledger.get("there").expect("kept").tries.attempts, 1);
            assert_eq!(
                audit_lines(&m, "warm-up"),
                [("there".to_owned(), "warm_up_login_refused".to_owned())]
            );
            assert!(
                !crate::home::dir(&m.ctx)
                    .join("warm")
                    .join("claude")
                    .exists(),
                "the run's home is gone, its config folder with it"
            );
        }

        /// A parked login that cannot be renewed for now, with Anthropic out of reach, waits
        /// its turn like a run that could not reach it: counted for nothing, and nothing run.
        #[test]
        fn a_park_that_cannot_be_renewed_for_now_waits_its_turn() {
            let (m, script) = warming("warm-renew-offline", 0, ANSWERED);
            idle(&m, "there", 20.0);
            let mut state = crate::state::load(&m.ctx).expect("state");
            let there = state
                .accounts
                .iter_mut()
                .find(|account| account.id == "there")
                .expect("there");
            there.parked.as_mut().expect("parked").access_expires_at = Some(NOW);
            crate::state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
            m.api
                .renew_trouble("there-refresh", crate::api::scripted::Trouble::Offline);

            let refused = warmed(&m).expect_err("refused");
            assert!(script.runs().is_empty());
            let ledger = crate::duty::warm::decide::Ledger::load(&m.ctx);
            let tries = &ledger.get("there").expect("kept").tries;
            assert_eq!(
                (tries.attempts, tries.waits),
                (0, 1),
                "{}",
                refused.error.code()
            );
        }

        /// One warm-up runs at a time: another front end's holds this one back, which counts
        /// for nothing and runs nothing.
        #[test]
        fn a_warm_up_running_elsewhere_holds_this_one_back_uncounted() {
            let (m, script) = warming("warm-in-progress", 0, ANSWERED);
            idle(&m, "there", 20.0);
            let held = std::fs::File::create(crate::home::dir(&m.ctx).join("warm.lock"))
                .expect("a lock file");
            held.lock().expect("held");
            let refused = warmed(&m).expect_err("refused");
            assert_eq!(refused.error.code(), "warm_up_in_progress");
            assert!(
                crate::duty::warm::decide::Ledger::load(&m.ctx)
                    .get("there")
                    .is_none()
            );
            assert!(script.runs().is_empty());
        }

        /// Where no reading can be had after the run, the period it started is taken on trust,
        /// from when the run began.
        #[test]
        fn with_no_reading_after_it_the_period_started_is_taken_on_trust() {
            let (m, _) = warming("warm-no-reading", 0, ANSWERED);
            idle(&m, "there", 20.0);
            m.api.token_trouble(
                "access-there-refresh",
                crate::api::scripted::Trouble::Offline,
            );
            let done = warmed(&m).expect("warmed up").value;
            assert_eq!(
                done,
                WarmUp::Started {
                    account: "there".into(),
                    until: None
                }
            );
            let overview = Pitboard::new(m.ctx.clone())
                .warm_overview()
                .expect("an overview");
            let there = overview
                .accounts
                .iter()
                .find(|choice| choice.account == "there")
                .expect("there");
            assert!(
                matches!(there.standing, Some(Standing::Running { .. })),
                "{:?}",
                there.standing
            );
            assert!(
                !crate::home::dir(&m.ctx)
                    .join("warm")
                    .join("claude")
                    .exists(),
                "the run's home is gone"
            );
        }

        /// With Claude Code not where Pitboard looks, nothing is warmed up, and the overview
        /// says why.
        #[test]
        fn nothing_is_warmed_up_without_claude_code() {
            let mut m = machine("warm-no-program");
            let absent = m.ctx_home().join("bin").join("claude");
            m.ctx = m.ctx.clone().with_claude_program(absent);
            let pitboard = Pitboard::new(m.ctx.clone());
            pitboard.warm_turn("there", true).expect("turned on");
            idle(&m, "there", 20.0);
            let overview = pitboard.warm_overview().expect("an overview");
            assert_eq!(
                overview.halted,
                Some(crate::duty::warm::Halt::ProgramMissing)
            );
            assert!(matches!(pitboard.warm_look(), Ok(Look::Stands(_))));
            let refused = pitboard.warm_now("there").expect_err("refused");
            assert_eq!(refused.error.code(), "claude_not_found");
        }
    }
}
