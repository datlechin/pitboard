//! The switch Pitboard makes by itself, under the lock every change takes.
//!
//! What a front end decided from a look at its files may be out of date by the time it holds
//! the lock: the app and `pitboard watch` can both be running, and either, or a person, may
//! have switched meanwhile. So the decision is made again here, from the files as they are
//! now, and the switch goes ahead only for the same plan, and only away from the account
//! that is still signed in. Two front ends that decided the same thing make one switch; one
//! that decided from numbers a switch has since made untrue makes none.

use super::{Outcome, Settled, switch_held};
use crate::autoswitch::{Auto, Decision, Ledger, Plan, Threshold, decide};
use crate::error::{Error, Result};
use crate::service::Warning;

/// The switch for `plan`, if it is still the one to make.
///
/// The attempt is recorded before anything moves, so however the switch ends, killed
/// included, it counts against the limit's attempts, and the front ends never try one limit
/// more often than [`crate::autoswitch::ATTEMPTS`] times between them. What it came to is
/// recorded before the lock is let go.
pub(crate) fn automatically(
    settled: Settled,
    plan: &Plan,
    threshold: Threshold,
) -> Result<(Auto, Vec<Warning>)> {
    let Settled {
        _exclusive,
        state,
        ctx,
        permit,
    } = settled;
    let ctx = &ctx;
    let now = ctx.now();
    let rows = crate::status::gather_offline(ctx, &state).rows;
    let mut ledger = Ledger::load(ctx);
    match decide(&state, &rows, &ledger, threshold, now) {
        Decision::Switch(again) if again.same(plan) => {}
        _ => return Ok((Auto::Idle, Vec::new())),
    }
    let to_id = state
        .get(&plan.to)
        .map(|account| account.id.clone())
        .unwrap_or_default();
    ledger.attempt(plan, now);
    ledger.save(ctx, permit)?;
    match switch_held(state, ctx, permit, &plan.to, Some(&plan.from_id)) {
        Ok((
            Outcome::Switched {
                from, to, adoption, ..
            },
            warnings,
        )) => {
            ledger.switched(plan);
            // The switch was made. A record of it that could not be written costs at most
            // one more attempt at a limit of the account it left, should that account be
            // put back in use before it resets.
            let _ = ledger.save(ctx, permit);
            Ok((
                Auto::Switched {
                    from,
                    to,
                    limit: plan.limit.clone(),
                    adoption,
                },
                warnings,
            ))
        }
        // Somebody switched first: to this account, or away from the one decided on.
        Ok((Outcome::AlreadyActive { .. }, _)) | Err(Error::SwitchOvertaken) => {
            Ok((Auto::Idle, Vec::new()))
        }
        Err(error) => {
            if over_the_account_switched_to(&error) {
                ledger.pass_over(plan, &to_id);
                let _ = ledger.save(ctx, permit);
            } else if mended_by_waiting(&error) {
                ledger.waited(plan);
                let _ = ledger.save(ctx, permit);
            }
            Err(error)
        }
    }
}

/// A refusal that is about this moment: Anthropic out of reach or overloaded, Claude Code
/// renewing its login as the switch read it, or its lock held. Trying again later may well
/// switch, so it is not counted against the limit's attempts.
fn mended_by_waiting(error: &Error) -> bool {
    matches!(error, Error::SignedInAccountChanged | Error::Lock(_))
        || error
            .cause()
            .is_some_and(crate::error::Cause::worth_retrying)
}

/// A refusal that is about the account switched to, and not about this moment: trying it
/// again would be refused again, and another account may be fine.
fn over_the_account_switched_to(error: &Error) -> bool {
    matches!(
        error,
        Error::AccountUnknown { .. }
            | Error::NothingParked { .. }
            | Error::ParkedLoginExpired { .. }
            | Error::ParkedLoginRefused { .. }
            | Error::ParkedLoginBelongsElsewhere { .. }
            | Error::ParkedCredentialMissing { .. }
            | Error::ParkedCredentialCorrupt { .. }
            | Error::CredentialTooLarge { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::super::harness::{
        Machine, NOW, cache_usage, document, hold, machine, owner, usage_answer, window,
    };
    use super::super::*;
    use crate::api::scripted::Trouble;
    use crate::autoswitch::{Auto, RETRY_SECONDS, Skip, Threshold};
    use crate::service::{Permit, Pitboard};
    use crate::time::{Clock, FixedClock};
    use crate::usage::{Snapshot, Source};
    use std::sync::Arc;

    /// What Pitboard has measured of `uuid`'s five-hour and weekly limits.
    fn measured(m: &Machine, uuid: &str, session: f64, weekly: f64) {
        crate::readings::answered(
            &m.ctx,
            Permit::for_a_test(),
            &[(
                uuid.to_owned(),
                Snapshot {
                    windows: vec![window("session", session), window("weekly_all", weekly)],
                    observed_at: Some(NOW),
                    answered_at: Some(NOW),
                    lists_every_limit: true,
                    source: Source::Live,
                },
            )],
        );
    }

    /// `here` in use at `session` of its five-hour limit, `there` parked with room.
    fn nearly_out(name: &str, session: f64) -> (Machine, Arc<FixedClock>) {
        let mut m = machine(name);
        let clock = Arc::new(FixedClock::at(NOW));
        m.ctx = m
            .ctx
            .clone()
            .with_clock(Arc::clone(&clock) as Arc<dyn Clock>);
        measured(&m, "here", session, 20.0);
        measured(&m, "there", 10.0, 30.0);
        (m, clock)
    }

    fn auto(m: &Machine) -> crate::service::Changing<Auto> {
        Pitboard::new(m.ctx.clone()).auto_switch(Threshold::DEFAULT)
    }

    fn live_refresh(m: &Machine) -> Option<String> {
        m.live()?
            .pointer("/claudeAiOauth/refreshToken")?
            .as_str()
            .map(str::to_owned)
    }

    /// The automatic switches the audit log records, and what each came to.
    fn logged(m: &Machine) -> Vec<(String, String, String)> {
        crate::audit::read(&m.ctx, 100)
            .into_iter()
            .filter(|entry| entry.verb == "auto-switch")
            .map(|entry| (entry.verb, entry.subject, entry.outcome))
            .collect()
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn an_account_at_the_share_is_switched_from_and_the_switch_is_recorded_as_automatic() {
        let (m, _) = nearly_out("auto-switches", 96.0);
        let done = auto(&m).expect("a switch");
        let Auto::Switched {
            from,
            to,
            limit,
            adoption,
        } = done.value
        else {
            panic!("a switch, not {:?}", done.value);
        };
        assert_eq!(
            (from.as_str(), to.as_str(), limit.kind.as_str()),
            ("here", "there", "session")
        );
        assert!(matches!(
            adoption,
            crate::provider::Adoption::PollingWithin(33)
        ));
        assert_eq!(live_refresh(&m).as_deref(), Some("there-refresh"));
        assert_eq!(
            logged(&m),
            [("auto-switch".into(), "there".into(), "ok".into())]
        );
        hold(&m, "after an automatic switch");

        let lines = crate::audit::read(&m.ctx, 100).len();
        assert!(
            matches!(auto(&m).expect("a look").value, Auto::Idle),
            "the account switched to has room, and nothing more is done"
        );
        assert_eq!(
            crate::audit::read(&m.ctx, 100).len(),
            lines,
            "a look that does nothing records nothing"
        );
    }

    /// A file behind the keychain holds sessions already running to their account until
    /// their login is next renewed, and holding back would leave every session, new ones
    /// too, on the account at the share. So the automatic switch switches, and says when
    /// running sessions take it, and that the file is there.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    #[cfg_attr(
        target_os = "linux",
        ignore = "on Linux the file is Claude Code's only store, never behind a keychain"
    )]
    fn with_a_file_behind_the_keychain_the_automatic_switch_switches_and_says_so() {
        let (m, _) = nearly_out("auto-file-behind", 96.0);
        let file = crate::provider::claude::live::credential_file(&m.ctx);
        m.mem
            .file_at(file.clone())
            .plant(&m.service, &document("left-by-a-sign-in").to_string());

        let done = auto(&m).expect("a switch");
        let Auto::Switched { to, adoption, .. } = done.value else {
            panic!("a switch, not {:?}", done.value);
        };
        assert_eq!(to, "there");
        assert_eq!(adoption, crate::provider::Adoption::AtRenewal { file });
        assert_eq!(live_refresh(&m).as_deref(), Some("there-refresh"));
        assert_eq!(
            done.warnings
                .iter()
                .filter(|w| w.code() == "fallback_login")
                .count(),
            1,
            "{:?}",
            done.warnings
        );
        assert_eq!(
            logged(&m),
            [("auto-switch".into(), "there".into(), "ok".into())]
        );
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_limit_below_the_share_takes_no_lock_and_changes_nothing() {
        let (m, _) = nearly_out("auto-below", 94.0);
        let before = m.mem.live().peek(&m.service);
        assert!(matches!(auto(&m).expect("a look").value, Auto::Idle));
        assert_eq!(m.mem.live().peek(&m.service), before);
        assert!(crate::audit::read(&m.ctx, 100).is_empty());
        assert!(!m.ctx_home().join(".pitboard/autoswitch.json").exists());
        assert_eq!(
            m.api.calls(),
            0,
            "nobody is asked anything to find nothing to do: {:?}",
            m.api.asked()
        );
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn with_no_account_to_go_to_nothing_moves() {
        let (m, _) = nearly_out("auto-no-room", 97.0);
        measured(&m, "there", 96.0, 30.0);
        let before = m.mem.live().peek(&m.service);
        let Auto::NoRoom { from, limit } = auto(&m).expect("a look").value else {
            panic!("no room");
        };
        assert_eq!((from.as_str(), limit.kind.as_str()), ("here", "session"));
        assert_eq!(m.mem.live().peek(&m.service), before);
        assert!(logged(&m).is_empty());
    }

    /// Claude Code stamps its usage cache with the account its config names, whichever login
    /// it asked with, so after a switch it can hold the numbers of the account switched away
    /// from under the name of the one switched to. Taken as `here`'s, a full five-hour limit
    /// in it switched `here` away at 10%.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_usage_cache_in_claude_codes_config_moves_nothing() {
        let (m, _) = nearly_out("auto-cache", 10.0);
        cache_usage(&m, usage_answer(100.0));
        let looked = auto(&m).expect("a look").value;
        assert!(matches!(looked, Auto::Idle), "{looked:?}");
        assert_eq!(live_refresh(&m).as_deref(), Some("here-refresh"));
    }

    /// Two front ends that decided the same switch make it once: the second finds, under the
    /// lock, that it has been made.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn two_front_ends_that_decided_one_switch_make_it_once() {
        let (m, _) = nearly_out("auto-twice", 96.0);
        let state = state::load(&m.ctx).expect("state");
        let plan = |m: &Machine| match crate::autoswitch::look(&m.ctx, &state, Threshold::DEFAULT) {
            crate::autoswitch::Next::Switch(plan) => plan,
            other => panic!("a switch, not {other:?}"),
        };
        let (first, second) = (plan(&m), plan(&m));
        for plan in [&first, &second] {
            let settled = settle(&m.ctx, Permit::for_a_test(), None)
                .expect("settled")
                .0;
            let _ = automatically(settled, plan, Threshold::DEFAULT).expect("no failure");
        }
        assert_eq!(live_refresh(&m).as_deref(), Some("there-refresh"));
        assert_eq!(
            crate::autoswitch::Ledger::load(&m.ctx),
            {
                let mut once = crate::autoswitch::Ledger::default();
                once.attempt(&first, NOW);
                once.switched(&first);
                once
            },
            "one attempt, which switched"
        );
        hold(&m, "after two front ends decided one switch");
    }

    /// The account signed in changed since the look, by a sign-in in Claude Code that
    /// Pitboard's record has not caught up with: the switch is not made from the account
    /// that is in use now, which nobody decided to leave. Whose login the switch found is
    /// recorded, and the activity log says what changed outside Pitboard.
    #[test]
    #[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
    fn a_switch_decided_away_from_an_account_no_longer_in_use_is_not_made() {
        let (m, _) = nearly_out("auto-overtaken", 96.0);
        let elsewhere = document("elsewhere-refresh");
        m.api
            .owned_by("access-elsewhere-refresh", owner("elsewhere"));
        let mut state = state::load(&m.ctx).expect("state");
        state.accounts.push(super::super::harness::account(
            "elsewhere",
            "elsewhere",
            None,
        ));
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        m.sign_in(&elsewhere);

        assert!(matches!(auto(&m).expect("no failure").value, Auto::Idle));
        assert_eq!(m.live(), Some(elsewhere), "the login in use stays in use");
        assert_eq!(
            super::super::harness::in_use_lines(&m),
            [
                ("elsewhere".to_string(), "signed_in_outside".to_string()),
                ("here".to_string(), "login_replaced".to_string()),
            ]
        );
        assert_eq!(
            crate::audit::read(&m.ctx, 100).len(),
            2,
            "and nothing else, since no switch was made"
        );
        let state = state::load(&m.ctx).expect("state");
        assert!(
            state
                .get(&m.key("there"))
                .and_then(|a| a.parked.as_ref())
                .is_some(),
            "and the account it would have switched to is still parked"
        );
    }

    /// Automatic switches happen while Claude Code is busy, which is when it renews its
    /// login. One renewed partway through a switch is the login in use from then on: the
    /// switch stops with nothing installed over it, the account it would have switched to
    /// keeps its parked login, and the next attempt, a minute on, switches.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_login_claude_code_renews_partway_through_is_not_written_over() {
        let (m, clock) = nearly_out("auto-renewed-meanwhile", 96.0);
        let renewed = document("here-renewed");
        m.api.owned_by("access-here-renewed", owner("here"));
        let failed = crate::fault::meanwhile(
            "switch.park_recorded",
            {
                let (store, service) = m.live_store();
                let renewed = renewed.to_string();
                move || store.plant(&service, &renewed)
            },
            || auto(&m),
        )
        .expect_err("the switch stops");
        assert!(
            matches!(failed.error, Error::SignedInAccountChanged),
            "{:?}",
            failed.error
        );
        assert_eq!(
            m.live(),
            Some(renewed.clone()),
            "the renewed login is untouched"
        );
        assert_eq!(
            logged(&m),
            [(
                "auto-switch".into(),
                "there".into(),
                "signed_in_account_changed".into()
            )]
        );
        hold(&m, "after a renewal partway through an automatic switch");

        assert!(
            matches!(auto(&m).expect("a look").value, Auto::Idle),
            "not straight away"
        );
        clock.advance(RETRY_SECONDS);
        assert!(matches!(
            auto(&m).expect("a switch").value,
            Auto::Switched { .. }
        ));
        assert_eq!(live_refresh(&m).as_deref(), Some("there-refresh"));
        let state = state::load(&m.ctx).expect("state");
        let here = state
            .get(&m.key("here"))
            .and_then(|a| a.parked.clone())
            .expect("parked");
        assert_eq!(
            m.mem
                .vault()
                .peek(&here.service)
                .map(|raw| raw.contains("here-renewed")),
            Some(true),
            "the login parked is the one Claude Code renewed"
        );
        hold(&m, "after the next attempt");
    }

    /// Anthropic out of reach as the switch asks whose the login going in is: nothing moves,
    /// and however many times that happens, the switch is made once Anthropic answers again,
    /// a little later each time. The login going out is known by its fingerprint and needs
    /// no answer.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn an_outage_does_not_use_up_the_attempts() {
        let (m, clock) = nearly_out("auto-outage", 96.0);
        m.api
            .token_trouble("access-there-refresh", Trouble::Offline);
        for wait in [60, 120, 240, 480] {
            let failed = auto(&m).expect_err("Anthropic is out of reach");
            assert_eq!(failed.error.code(), "identity_unverifiable");
            assert_eq!(live_refresh(&m).as_deref(), Some("here-refresh"));
            clock.advance(wait);
        }
        m.api.owned_by("access-there-refresh", owner("there"));
        assert!(matches!(
            auto(&m).expect("a switch").value,
            Auto::Switched { .. }
        ));
        assert_eq!(live_refresh(&m).as_deref(), Some("there-refresh"));
        hold(&m, "after an outage");
    }

    /// A parked login Anthropic refuses is no switch, and the next attempt goes to another
    /// account with room rather than to the same one again.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn an_account_whose_login_is_refused_is_passed_over_for_the_next() {
        let (m, clock) = nearly_out("auto-refused", 96.0);
        let parked = park::reserve(&m.ctx, Permit::for_a_test(), "other").expect("a name");
        let parked = park::store_at(
            &m.ctx,
            Permit::for_a_test(),
            ProviderId::Claude,
            &parked,
            &super::super::harness::oauth("other-refresh", 30),
        )
        .expect("parked");
        let mut state = state::load(&m.ctx).expect("state");
        state.accounts.push(super::super::harness::account(
            "other",
            "other",
            Some(parked),
        ));
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        m.api.owned_by("access-other-refresh", owner("other"));
        measured(&m, "other", 40.0, 40.0);
        m.api
            .token_trouble("access-there-refresh", Trouble::Unauthorized);
        m.api.renew_trouble("there-refresh", Trouble::InvalidGrant);

        let failed = auto(&m).expect_err("a refused login is not a switch");
        assert_eq!(failed.error.code(), "parked_login_refused");
        clock.advance(RETRY_SECONDS);
        let Auto::Switched { to, .. } = auto(&m).expect("a switch").value else {
            panic!("a switch to the other account");
        };
        assert_eq!(to, "other");
        assert_eq!(live_refresh(&m).as_deref(), Some("other-refresh"));
        hold(&m, "after passing one account over");
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_switch_waiting_to_be_finished_is_left_to_the_change_that_finishes_it() {
        let (m, _) = nearly_out("auto-interrupted", 96.0);
        let settled = settle(&m.ctx, Permit::for_a_test(), None)
            .expect("settled")
            .0;
        let died =
            crate::fault::killing("switch.park_recorded", || switch(settled, &m.key("there")));
        assert_eq!(died.unwrap_err(), "switch.park_recorded");
        let journal =
            std::fs::read(m.ctx_home().join(".pitboard/journal.json")).expect("a journal");

        let Auto::Skipped { why, .. } = auto(&m).expect("a look").value else {
            panic!("skipped");
        };
        assert_eq!(why, Skip::SwitchInterrupted);
        assert_eq!(
            std::fs::read(m.ctx_home().join(".pitboard/journal.json")).ok(),
            Some(journal),
            "nothing finished it but a change somebody makes"
        );
    }

    /// Claude Code authenticated some other way uses none of the logins Pitboard moves, so
    /// a switch would change nothing its sessions see.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn claude_code_signed_in_some_other_way_is_not_switched() {
        let (m, _) = nearly_out("auto-overridden", 96.0);
        let config = m.ctx_home().join(".claude");
        std::fs::create_dir_all(&config).expect("a config dir");
        std::fs::write(
            config.join("settings.json"),
            serde_json::json!({"apiKeyHelper": "/usr/local/bin/get-key"}).to_string(),
        )
        .expect("settings");
        let before = m.mem.live().peek(&m.service);
        let Auto::Skipped { why, .. } = auto(&m).expect("a look").value else {
            panic!("skipped");
        };
        let Skip::Overridden(names) = why else {
            panic!("overridden, not {why:?}");
        };
        assert!(names[0].starts_with("apiKeyHelper in "), "{names:?}");
        assert_eq!(m.mem.live().peek(&m.service), before);
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn as_root_nothing_is_switched_or_recorded() {
        let (m, _) = nearly_out("auto-root", 96.0);
        m.mem
            .runs_with(crate::host::Elevation::Elevated { why: "root" });
        let before = m.mem.live().peek(&m.service);
        let failed = auto(&m).expect_err("refused at the gate");
        assert_eq!(failed.error.code(), "elevated");
        assert_eq!(m.mem.live().peek(&m.service), before);
        assert!(logged(&m).is_empty());
        assert!(!m.ctx_home().join(".pitboard/autoswitch.json").exists());
    }
}
