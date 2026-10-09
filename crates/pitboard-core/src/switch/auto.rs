//! The switch Pitboard makes by itself, under the lock every change takes.
//!
//! A look at the files found that a switch may be due, or that whose login Claude Code has
//! stored is in doubt. Either may be out of date by the time the lock is held: the app and
//! `pitboard watch` can both be running, and either, or a person, may have switched
//! meanwhile, or signed in outside Pitboard. So whose login is stored is told here, as a
//! switch tells it, and recorded, and the switch is decided once, from that and the files as
//! they are now. The switch rests on that same telling, so nothing comes between the
//! decision and the login it moves. Two front ends that decided the same thing make one
//! switch; one that decided from numbers a switch has since made untrue makes none.

use super::{Settled, Switched, identify, switch_from};
use crate::audit;
use crate::autoswitch::{self, Auto, Blind, Judged, Ledger, Threshold};
use crate::error::{Error, Result};
use crate::provider::ProviderId;
use crate::service::Warning;

/// Switches Claude Code where a limit of the account in use has reached `threshold` and
/// another account has room, as [`crate::autoswitch`] says, and records what it came to.
///
/// Whose login Claude Code has stored is told first. Where it cannot be, nothing is judged
/// from what Pitboard last knew, which may be another account's: that is recorded once, as
/// `auto-stay`, and nobody is asked again until the wait after it is over, as an attempt
/// waits. A store this process cannot read is refused with its error and kept for nobody to
/// wait on, since another front end may read it. A reason not to switch away from a limit at
/// the share is recorded as `auto-stay` too, of the account in use, once for that limit and
/// its reset. The attempt is recorded once the switch is decided and before anything moves,
/// so however the switch ends, killed included, it counts against the limit's attempts, and
/// the front ends never try one limit more often than [`crate::autoswitch::ATTEMPTS`] times
/// between them. Where an attempt that failed leaves no limit to try, why is recorded with
/// the failure, as a decision records it, so the look a front end makes while it waits after
/// the failure stands on that. A switch, and anything that stopped one, is recorded as
/// `auto-switch`, with the account it went to, or none before there was one.
pub(crate) fn automatically(
    settled: Settled,
    threshold: Threshold,
) -> Result<(Auto, Vec<Warning>)> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
        permit,
    } = settled;
    let ctx = &ctx;
    let now = ctx.now();
    let which = ProviderId::Claude;
    let stopped = |subject: &str, error: Error| {
        audit::record(ctx, permit, "auto-switch", subject, error.code());
        error
    };
    let unchosen = "";
    let mut ledger = Ledger::load(ctx);
    let known_at = state.in_use(which).map(|last| last.known_at);
    if let Some(why) = ledger.unidentified(known_at, now) {
        return Ok((Auto::NotWatching { why }, Vec::new()));
    }
    let (live, found) = match identify::look(ctx, &state, which) {
        Ok(looked) => looked,
        Err(error) if unreadable_here(&error) => return Err(stopped(unchosen, error)),
        Err(error) => {
            let why = ledger.not_told(identify::untold(&error), known_at, now);
            ledger
                .save(ctx, permit)
                .map_err(|error| stopped(unchosen, error))?;
            let last = state
                .account_in_use(which)
                .map(|account| state.typed(&account.key()))
                .unwrap_or_default();
            audit::record(ctx, permit, "auto-stay", &last, why.code());
            return Ok((Auto::NotWatching { why }, Vec::new()));
        }
    };
    identify::keep(ctx, permit, &mut state, which, found)
        .map_err(|error| stopped(unchosen, error))?;
    if ledger.told() {
        ledger
            .save(ctx, permit)
            .map_err(|error| stopped(unchosen, error))?;
    }
    let identify::Live::Login(outgoing) = live else {
        return Ok((
            Auto::NotWatching {
                why: Blind::NothingSignedIn,
            },
            Vec::new(),
        ));
    };
    let judged = match autoswitch::watched(&state, Some(&outgoing.owner)) {
        Ok(account) => autoswitch::judge(ctx, &state, account, &ledger, threshold, now),
        Err(why) => return Ok((Auto::NotWatching { why }, Vec::new())),
    };
    let plan = match judged {
        Judged::Switch(plan) => plan,
        Judged::Hold(hold) => {
            if ledger.say(&hold, now) {
                ledger
                    .save(ctx, permit)
                    .map_err(|error| stopped(unchosen, error))?;
                audit::record(
                    ctx,
                    permit,
                    "auto-stay",
                    &state.typed(&hold.from),
                    hold.why.code(),
                );
            }
            return Ok((hold.skipped(&state), Vec::new()));
        }
        Judged::Stands(stands) => return Ok((stands, Vec::new())),
    };
    let to = state.typed(&plan.to);
    let to_id = state
        .get(&plan.to)
        .map(|account| account.id.clone())
        .unwrap_or_default();
    let decided_from = state.clone();
    ledger.attempt(&plan, now);
    ledger
        .save(ctx, permit)
        .map_err(|error| stopped(&to, error))?;
    match switch_from(state, ctx, permit, &plan.to, outgoing) {
        Ok((
            Switched {
                from, to, adoption, ..
            },
            warnings,
        )) => {
            ledger.switched(&plan);
            // The switch was made. A record of it that could not be written costs at most
            // one more attempt at a limit of the account it left, should that account be
            // put back in use before it resets.
            let _ = ledger.save(ctx, permit);
            audit::record(ctx, permit, "auto-switch", &to, "ok");
            Ok((
                Auto::Switched {
                    from,
                    to,
                    limit: plan.limit,
                    adoption,
                },
                warnings,
            ))
        }
        Err(error) => {
            if over_the_account_switched_to(&error) {
                ledger.pass_over(&plan, &to_id);
            } else if mended_by_waiting(&error) {
                ledger.waited(&plan);
            }
            let error = stopped(&to, error);
            let judged = decided_from.get(&plan.from).map(|account| {
                autoswitch::judge(ctx, &decided_from, account, &ledger, threshold, now)
            });
            let spent = match judged {
                Some(Judged::Hold(hold)) => ledger.say(&hold, now).then_some(hold),
                _ => None,
            };
            // A record that could not be written costs at most one more attempt at a limit of
            // this account, or one more decision saying why none is made.
            if ledger.save(ctx, permit).is_ok()
                && let Some(hold) = spent
            {
                audit::record(
                    ctx,
                    permit,
                    "auto-stay",
                    &decided_from.typed(&hold.from),
                    hold.why.code(),
                );
            }
            Err(error)
        }
    }
}

/// A store this process could not read, such as a keychain locked in a session over SSH: its
/// front end paces it, as a refusal before the ledger.
fn unreadable_here(error: &Error) -> bool {
    matches!(
        error,
        Error::Store(crate::store::Error::Locked | crate::store::Error::Unreadable(_))
    )
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
/// again would be refused again, and another account may be fine. A login too large to write
/// is not one: the size that matters is of the document the machine keeps, whichever account
/// goes in, so it is counted against the limit's attempts as any other refusal is.
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
    )
}

#[cfg(test)]
mod tests {
    use super::super::harness::{
        Machine, NOW, cache_usage, config_names, document, hold, machine, owner, state_file,
        usage_answer, window,
    };
    use super::super::*;
    use crate::api::scripted::Trouble;
    use crate::autoswitch::{
        ATTEMPTS, Auto, Blind, Decision, Ledger, Look, RETRY_SECONDS, Skip, Threshold, decide,
    };
    use crate::service::{Permit, Pitboard};
    use crate::store::memory::Fault;
    use crate::time::{Clock, FixedClock};
    use crate::usage::{Snapshot, Source, Window};
    use std::sync::Arc;

    /// What Anthropic has just answered of `uuid`'s five-hour and weekly limits.
    fn measured(m: &Machine, uuid: &str, session: f64, weekly: f64) {
        crate::readings::answered(
            &m.ctx,
            Permit::for_a_test(),
            &[(uuid.to_owned(), answer(m, session, weekly))],
        );
    }

    fn answer(m: &Machine, session: f64, weekly: f64) -> Snapshot {
        Snapshot {
            windows: vec![window("session", session), window("weekly_all", weekly)],
            observed_at: Some(m.ctx.now()),
            answered_at: Some(m.ctx.now()),
            lists_every_limit: true,
            source: Source::Live,
        }
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

    /// What the look at the files alone comes to, where it ends there and takes no lock.
    fn stands(m: &Machine) -> Auto {
        let state = state::load(&m.ctx).expect("state");
        match crate::autoswitch::look(&m.ctx, &state, Threshold::DEFAULT) {
            Look::Stands(stands) => *stands,
            Look::Act => panic!("the look goes on to the lock"),
        }
    }

    /// The account the automatic switch watches, where it found nothing to do.
    fn watching(looked: &Auto) -> Option<&str> {
        match looked {
            Auto::Watching { account, .. } => Some(account),
            _ => None,
        }
    }

    /// The reasons not to switch the audit log records, and of which account.
    fn stays(m: &Machine) -> Vec<(String, String)> {
        super::super::harness::audit_lines(m, "auto-stay")
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
        assert_eq!(
            watching(&auto(&m).expect("a look").value),
            Some("there"),
            "the account switched to has room, and nothing more is done"
        );
        assert_eq!(
            crate::audit::read(&m.ctx, 100).len(),
            lines,
            "a look that does nothing records nothing"
        );
    }

    /// Another Claude Code process can write its own account into Claude Code's config and
    /// leave the login stored as it was. The account switched from is the one Anthropic named
    /// for that login: taken from the config, `there` read as in use with room, and `here`
    /// was left at its limit.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn the_account_anthropic_names_is_the_one_switched_from() {
        let (m, _) = nearly_out("auto-config-names-another", 96.0);
        config_names(&m, "there");

        let done = auto(&m).expect("a switch");
        let Auto::Switched { from, to, .. } = done.value else {
            panic!("a switch, not {:?}", done.value);
        };
        assert_eq!((from.as_str(), to.as_str()), ("here", "there"));
        assert_eq!(live_refresh(&m).as_deref(), Some("there-refresh"));
        assert_eq!(
            logged(&m),
            [("auto-switch".into(), "there".into(), "ok".into())]
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
        assert_eq!(watching(&stands(&m)), Some("here"));
        assert_eq!(watching(&auto(&m).expect("a look").value), Some("here"));
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
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn with_no_account_to_go_to_nothing_moves() {
        let (m, _) = nearly_out("auto-no-room", 97.0);
        measured(&m, "there", 96.0, 30.0);
        let before = m.mem.live().peek(&m.service);
        let Auto::Skipped { from, limit, why } = auto(&m).expect("a look").value else {
            panic!("no room");
        };
        assert_eq!((from.as_str(), limit.kind.as_str()), ("here", "session"));
        assert_eq!(why, Skip::NoRoom { unread: Vec::new() });
        assert_eq!(m.mem.live().peek(&m.service), before);
        assert!(logged(&m).is_empty());
    }

    /// Below the share it says which account it watches, how full its fullest limit is, and
    /// when that was read.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn below_the_share_it_says_which_account_it_watches() {
        let (m, _) = nearly_out("auto-watching", 94.0);
        let looked = stands(&m);
        let Auto::Watching {
            account,
            nearest,
            as_of,
            held_until,
        } = looked
        else {
            panic!("watching, not {looked:?}");
        };
        assert_eq!(account, "here");
        assert_eq!(nearest, Some(window("session", 94.0)));
        assert_eq!((as_of, held_until), (Some(NOW), None));
    }

    /// Anthropic asking for less traffic about the account in use holds back its next reading,
    /// which is what the automatic switch judges, so that is said with what it watches.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_rate_limit_hold_is_said_with_what_it_watches() {
        let (m, _) = nearly_out("auto-held", 60.0);
        crate::budget::record(
            &m.ctx,
            Permit::for_a_test(),
            &[(
                "here".into(),
                crate::budget::Outcome::RateLimited(Some(3600)),
            )],
        );
        let looked = stands(&m);
        assert!(
            matches!(
                looked,
                Auto::Watching {
                    held_until: Some(until),
                    ..
                } if until == NOW + 3600
            ),
            "{looked:?}"
        );
    }

    /// No account with room is said and recorded once for the limit and its reset: the first
    /// look goes to the lock to record it, and the next stands on that record.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn no_room_is_recorded_once_for_a_limit_and_its_reset() {
        let (m, _) = nearly_out("auto-no-room-once", 97.0);
        measured(&m, "there", 96.0, 30.0);
        let no_room = |looked: &Auto| {
            matches!(
                looked,
                Auto::Skipped {
                    why: Skip::NoRoom { .. },
                    ..
                }
            )
        };
        let looked = auto(&m).expect("a look").value;
        assert!(no_room(&looked), "{looked:?}");
        let looked = stands(&m);
        assert!(no_room(&looked), "the next look takes no lock: {looked:?}");
        let looked = auto(&m).expect("a look").value;
        assert!(no_room(&looked), "{looked:?}");
        assert_eq!(stays(&m), [("here".to_string(), "no_room".to_string())]);
        assert!(logged(&m).is_empty(), "nothing attempted");
    }

    /// A reason is told of the window it was recorded under. Anthropic's answers can give one
    /// window's reset a second apart, and front ends tell one reason from another by its
    /// reset, so a reason told with each answer's own was said again for a window it was
    /// recorded once for.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_reason_is_told_of_the_window_it_was_recorded_under() {
        let (m, _) = nearly_out("auto-said-window", 97.0);
        measured(&m, "there", 96.0, 30.0);
        let reset = |looked: Auto| match looked {
            Auto::Skipped { limit, .. } => limit.resets_at,
            other => panic!("skipped, not {other:?}"),
        };
        assert_eq!(reset(auto(&m).expect("a look").value), Some(NOW + 3600));
        let a_second_off = Snapshot {
            windows: vec![
                Window {
                    resets_at: Some(NOW + 3599),
                    ..window("session", 97.0)
                },
                window("weekly_all", 20.0),
            ],
            ..answer(&m, 97.0, 20.0)
        };
        crate::readings::answered(
            &m.ctx,
            Permit::for_a_test(),
            &[("here".to_owned(), a_second_off)],
        );
        assert_eq!(reset(stands(&m)), Some(NOW + 3600));
        assert_eq!(reset(auto(&m).expect("a look").value), Some(NOW + 3600));
        assert_eq!(stays(&m), [("here".to_string(), "no_room".to_string())]);
    }

    /// A reason said is kept until no answer of its window can still run: the next answer
    /// gave that window's reset a second after the one it was said under. Forgotten at its
    /// own reset, it was recorded and told again, under the later reset, in the second between.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_reason_said_is_kept_while_a_later_answer_of_its_window_runs() {
        let (m, clock) = nearly_out("auto-said-past-reset", 97.0);
        let session_resets = |uuid: &str, session: f64, resets_at: i64| {
            let answer = Snapshot {
                windows: vec![
                    Window {
                        resets_at: Some(resets_at),
                        ..window("session", session)
                    },
                    window("weekly_all", 20.0),
                ],
                ..answer(&m, session, 20.0)
            };
            crate::readings::answered(&m.ctx, Permit::for_a_test(), &[(uuid.to_owned(), answer)]);
        };
        session_resets("there", 96.0, NOW + 7200);
        let reset = |looked: Auto| match looked {
            Auto::Skipped {
                limit,
                why: Skip::NoRoom { .. },
                ..
            } => limit.resets_at,
            other => panic!("no room, not {other:?}"),
        };
        assert_eq!(reset(auto(&m).expect("a look").value), Some(NOW + 3600));
        session_resets("here", 97.0, NOW + 3601);

        clock.advance(3600);
        assert_eq!(reset(stands(&m)), Some(NOW + 3600));
        assert_eq!(reset(auto(&m).expect("a look").value), Some(NOW + 3600));
        assert_eq!(stays(&m), [("here".to_string(), "no_room".to_string())]);
    }

    /// A reason said of a limit that gives no reset is kept as an attempt at one is, for a week
    /// after it was said: no look says it again within the week, and the first after it does.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_reason_said_of_a_limit_with_no_reset_is_kept_for_a_week() {
        let (m, clock) = nearly_out("auto-said-no-reset", 97.0);
        let unending = |kind: &str, percent: f64| Window {
            resets_at: None,
            ..window(kind, percent)
        };
        for (uuid, session) in [("here", 97.0), ("there", 96.0)] {
            crate::readings::answered(
                &m.ctx,
                Permit::for_a_test(),
                &[(
                    uuid.to_owned(),
                    Snapshot {
                        windows: vec![unending("session", session), unending("weekly_all", 20.0)],
                        ..answer(&m, session, 20.0)
                    },
                )],
            );
        }
        let no_room = |looked: &Auto| {
            matches!(
                looked,
                Auto::Skipped {
                    why: Skip::NoRoom { .. },
                    ..
                }
            )
        };
        let looked = auto(&m).expect("a look").value;
        assert!(no_room(&looked), "{looked:?}");
        assert_eq!(stays(&m).len(), 1);

        clock.advance(7 * 86_400 - 1);
        let looked = stands(&m);
        assert!(no_room(&looked), "within the week: {looked:?}");

        clock.advance(1);
        let state = state::load(&m.ctx).expect("state");
        assert!(matches!(
            crate::autoswitch::look(&m.ctx, &state, Threshold::DEFAULT),
            Look::Act
        ));
        let looked = auto(&m).expect("a look").value;
        assert!(no_room(&looked), "{looked:?}");
        assert_eq!(stays(&m).len(), 2, "said again after the week");
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
        assert_eq!(watching(&looked), Some("here"), "{looked:?}");
        assert_eq!(live_refresh(&m).as_deref(), Some("here-refresh"));
    }

    /// Two front ends whose looks both found a switch due make it once: the second finds,
    /// under the lock, the account switched to in use, with room.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn two_front_ends_that_decided_one_switch_make_it_once() {
        let (m, _) = nearly_out("auto-twice", 96.0);
        let state = state::load(&m.ctx).expect("state");
        for _ in 0..2 {
            let looked = crate::autoswitch::look(&m.ctx, &state, Threshold::DEFAULT);
            assert!(matches!(looked, Look::Act), "{looked:?}");
        }
        let rows = crate::status::gather_offline(&m.ctx, &state).rows;
        let Decision::Switch(plan) = decide(
            &state,
            &m.key("here"),
            &rows,
            &Ledger::default(),
            Threshold::DEFAULT,
            NOW,
        ) else {
            panic!("a switch decided");
        };

        let mut came_to = Vec::new();
        for _ in 0..2 {
            let settled = settle(&m.ctx, Permit::for_a_test(), None)
                .expect("settled")
                .0;
            came_to.push(
                automatically(settled, Threshold::DEFAULT)
                    .expect("no failure")
                    .0,
            );
        }
        assert!(
            matches!(
                &came_to[..],
                [Auto::Switched { .. }, Auto::Watching { account, .. }] if account == "there"
            ),
            "{came_to:?}"
        );
        assert_eq!(live_refresh(&m).as_deref(), Some("there-refresh"));
        assert_eq!(
            Ledger::load(&m.ctx),
            {
                let mut once = Ledger::default();
                once.attempt(&plan, NOW);
                once.switched(&plan);
                once
            },
            "one attempt, which switched"
        );
        hold(&m, "after two front ends decided one switch");
    }

    /// A sign-in in Claude Code since Pitboard last read, over `here` at the share: under the
    /// lock the automatic switch asks whose the login stored is, records it, and judges the
    /// account signed in, which has room. Nothing is attempted from `here`, which nobody is
    /// on any more, and the activity log says what changed outside Pitboard.
    #[test]
    #[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
    fn a_sign_in_since_the_last_read_is_judged_as_the_account_it_signed_in() {
        let (m, _) = nearly_out("auto-signed-in-since", 96.0);
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
        measured(&m, "elsewhere", 20.0, 20.0);
        m.sign_in(&elsewhere);

        assert_eq!(
            watching(&auto(&m).expect("no failure").value),
            Some("elsewhere")
        );
        assert_eq!(m.live(), Some(elsewhere), "the login in use stays in use");
        assert_eq!(
            crate::autoswitch::Ledger::load(&m.ctx),
            crate::autoswitch::Ledger::default(),
            "no attempt"
        );
        assert!(!m.ctx_home().join(".pitboard/autoswitch.json").exists());
        assert_eq!(
            super::super::harness::audit_lines(&m, "in-use"),
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
        assert_eq!(
            state
                .in_use(ProviderId::Claude)
                .and_then(|record| record.owner.clone()),
            Some(owner("elsewhere"))
        );
        assert_eq!(
            state.get(&m.key("here")).and_then(|a| a.replaced_at),
            Some(NOW)
        );
        assert!(
            state
                .get(&m.key("there"))
                .and_then(|a| a.parked.as_ref())
                .is_some(),
            "and the account it would have switched to is still parked"
        );
    }

    /// Claude Code's config naming another account since Anthropic last named the login
    /// stored puts the account in use in doubt. The automatic switch settles it under its
    /// lock, by the login's fingerprint where it is the one Anthropic named, and records what
    /// the config names, so the next look has nothing to settle and takes no lock.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_doubt_is_settled_under_the_lock_once() {
        let (m, _) = nearly_out("auto-doubt", 94.0);
        config_names(&m, "there");

        let looked = auto(&m).expect("a look").value;
        assert_eq!(watching(&looked), Some("here"), "{looked:?}");
        assert_eq!(m.api.calls(), 0, "{:?}", m.api.asked());
        let state = state::load(&m.ctx).expect("state");
        assert_eq!(
            state
                .in_use(ProviderId::Claude)
                .and_then(|record| record.named.clone()),
            Some(state::new_id(ProviderId::Claude, &owner("there"))),
            "the record says the config names `there`"
        );
        let before = state_file(&m);

        let looked = stands(&m);
        assert_eq!(watching(&looked), Some("here"), "{looked:?}");
        let looked = auto(&m).expect("a look").value;
        assert_eq!(watching(&looked), Some("here"), "{looked:?}");
        assert_eq!(m.api.calls(), 0);
        assert_eq!(state_file(&m), before, "nothing left to settle");
        assert!(!m.ctx_home().join(".pitboard/autoswitch.json").exists());
        let read = Pitboard::new(m.ctx.clone())
            .status_offline()
            .expect("a read");
        let codes: Vec<&str> = read.warnings.iter().map(Warning::code).collect();
        assert_eq!(codes, ["config_names_another"]);
    }

    /// A login too large to write without the argument line, where the argument line is not
    /// allowed, is refused before anything moves, over the login going out. It is about this
    /// machine and not the account switched to, so it counts as an attempt as any refusal
    /// that waiting will not mend does. It passed that account over, and then each other in
    /// turn, until the account in use read as having nowhere to go. The failure that spends
    /// the last attempt records why at once, and the look after it stands on that: a front end
    /// waiting after the failure said it only once its wait was over.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_login_too_large_to_write_counts_as_an_attempt() {
        let (mut m, clock) = nearly_out("auto-too-large", 96.0);
        m.ctx = m.ctx.clone().with_argv_fallback(false);
        m.mem.vault().takes_on_stdin(16);
        let state = state::load(&m.ctx).expect("state");
        let rows = crate::status::gather_offline(&m.ctx, &state).rows;
        let Decision::Switch(plan) = decide(
            &state,
            &m.key("here"),
            &rows,
            &Ledger::default(),
            Threshold::DEFAULT,
            NOW,
        ) else {
            panic!("a switch decided");
        };

        for _ in 1..ATTEMPTS {
            let failed = auto(&m).expect_err("too large to park");
            assert_eq!(failed.error.code(), "credential_too_large");
            assert!(stays(&m).is_empty());
            clock.advance(RETRY_SECONDS);
        }
        let last = auto(&m).expect_err("the last attempt");
        assert_eq!(last.error.code(), "credential_too_large");
        assert_eq!(
            stays(&m),
            [("here".to_string(), "attempts_spent".to_string())],
            "recorded with the failure that left nothing to try"
        );
        let spent_at = clock.now();
        let looked = stands(&m);
        assert!(
            matches!(
                looked,
                Auto::Skipped {
                    why: Skip::GaveUp,
                    ..
                }
            ),
            "the look stands on it, which ends a front end's refusals in a row: {looked:?}"
        );
        clock.advance(RETRY_SECONDS);
        assert!(matches!(
            auto(&m).expect("a look").value,
            Auto::Skipped {
                why: Skip::GaveUp,
                ..
            }
        ));
        assert_eq!(stays(&m).len(), 1, "recorded once");

        let mut tried = Ledger::default();
        for attempt in 0..ATTEMPTS {
            tried.attempt(&plan, NOW + i64::from(attempt) * RETRY_SECONDS);
        }
        let spent = crate::autoswitch::Hold {
            from: plan.from.clone(),
            from_id: plan.from_id.clone(),
            limit: plan.limit.clone(),
            why: Skip::GaveUp,
        };
        tried.say(&spent, spent_at);
        assert_eq!(
            Ledger::load(&m.ctx),
            tried,
            "every attempt counted, and no account passed over"
        );
        assert_eq!(live_refresh(&m).as_deref(), Some("here-refresh"));
    }

    /// The attempts at one limit spent, another at the share is still tried, so nothing is
    /// recorded until none is left, and then of the furthest past the share.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn the_attempts_spent_are_recorded_once_no_limit_is_left_to_try() {
        let (mut m, clock) = nearly_out("auto-spent-two-limits", 96.0);
        measured(&m, "here", 96.0, 97.0);
        m.ctx = m.ctx.clone().with_argv_fallback(false);
        m.mem.vault().takes_on_stdin(16);
        for _ in 0..ATTEMPTS {
            auto(&m).expect_err("too large to park");
            clock.advance(RETRY_SECONDS);
        }
        assert!(stays(&m).is_empty(), "the five-hour limit is left to try");
        for _ in 1..ATTEMPTS {
            auto(&m).expect_err("too large to park");
            clock.advance(RETRY_SECONDS);
        }
        auto(&m).expect_err("the last attempt");
        assert_eq!(
            stays(&m),
            [("here".to_string(), "attempts_spent".to_string())]
        );
        let looked = stands(&m);
        assert!(
            matches!(
                &looked,
                Auto::Skipped {
                    why: Skip::GaveUp,
                    limit,
                    ..
                } if limit.kind == "weekly_all"
            ),
            "{looked:?}"
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

        let looked = auto(&m).expect("a look").value;
        assert!(
            matches!(looked, Auto::Waiting { until, .. } if until == NOW + RETRY_SECONDS),
            "not straight away: {looked:?}"
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
        assert!(
            matches!(stands(&m), Auto::Waiting { until, .. } if until == NOW + RETRY_SECONDS),
            "the look stands on the attempt's wait, which ends a front end's refusals in a row"
        );
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

        let looked = auto(&m).expect("a look").value;
        assert!(
            matches!(
                looked,
                Auto::NotWatching {
                    why: Blind::SwitchInterrupted
                }
            ),
            "{looked:?}"
        );
        assert_eq!(
            std::fs::read(m.ctx_home().join(".pitboard/journal.json")).ok(),
            Some(journal),
            "nothing finished it but a change somebody makes"
        );
    }

    /// A switch another run is making has its record too, for as long as that run holds
    /// Pitboard's lock. A look then says only a decision under the lock can say, which waits
    /// for that run. The same record with the lock free is of a switch nobody is finishing.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_switch_under_way_in_another_run_is_not_one_interrupted() {
        let (m, _) = nearly_out("auto-under-way", 96.0);
        let settled = settle(&m.ctx, Permit::for_a_test(), None)
            .expect("settled")
            .0;
        let died =
            crate::fault::killing("switch.park_recorded", || switch(settled, &m.key("there")));
        assert_eq!(died.unwrap_err(), "switch.park_recorded");
        let state = state::load(&m.ctx).expect("state");
        let look = || crate::autoswitch::look(&m.ctx, &state, Threshold::DEFAULT);
        let lock = m.ctx_home().join(".pitboard/state.lock");

        let another_run = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let held = std::fs::File::open(&lock).expect("the lock's file");
                    held.lock().expect("held");
                    held
                })
                .join()
                .expect("a run holding the lock")
        });
        assert!(matches!(look(), Look::Act), "{:?}", look());
        assert!(lock.exists() && super::super::interrupted(&m.ctx));

        drop(another_run);
        assert!(
            matches!(
                look(),
                Look::Stands(stands) if matches!(
                    *stands,
                    Auto::NotWatching { why: Blind::SwitchInterrupted }
                )
            ),
            "{:?}",
            look()
        );

        std::fs::remove_file(&lock).expect("the lock's file goes");
        assert!(
            matches!(look(), Look::Stands(_)),
            "no file, no holder: {:?}",
            look()
        );
        assert!(!lock.exists(), "a look makes no lock file");
    }

    /// Under a custom OAuth endpoint Claude Code keeps its login where Pitboard does not act,
    /// so there is nothing to watch, whatever the numbers say, and nothing is settled.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn under_a_custom_oauth_endpoint_nothing_is_watched() {
        let (m, _) = nearly_out("auto-custom-oauth", 96.0);
        let config = m.ctx_home().join(".claude");
        std::fs::create_dir_all(&config).expect("a config dir");
        std::fs::write(
            config.join("settings.json"),
            serde_json::json!({"env": {"CLAUDE_CODE_CUSTOM_OAUTH_URL": "https://oauth.example"}})
                .to_string(),
        )
        .expect("settings");
        let looked = auto(&m).expect("a look").value;
        assert!(
            matches!(
                looked,
                Auto::NotWatching {
                    why: Blind::CustomOauth
                }
            ),
            "{looked:?}"
        );
        assert!(crate::audit::read(&m.ctx, 100).is_empty());
        assert_eq!(live_refresh(&m).as_deref(), Some("here-refresh"));
    }

    /// Whose login Claude Code has stored could not be told under the lock, here a login it
    /// renewed while Anthropic is out of reach: nothing is judged from what the record said
    /// before, which may be another account's. That is said and recorded once, and nobody is
    /// asked again until the wait is over, a minute and then twice as long each time, as an
    /// attempt waits. Once Anthropic answers, the account it names is judged.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn an_identification_that_fails_is_said_and_waited_on() {
        let (m, clock) = nearly_out("auto-unidentified", 96.0);
        m.api.token_trouble("access-here-renewed", Trouble::Offline);
        m.sign_in(&document("here-renewed"));
        let stays = || {
            crate::audit::read(&m.ctx, 100)
                .into_iter()
                .filter(|entry| entry.verb == "auto-stay")
                .map(|entry| (entry.subject, entry.outcome))
                .collect::<Vec<_>>()
        };

        let mut asked = 0;
        for (round, wait) in [60, 120].into_iter().enumerate() {
            let looked = auto(&m).expect("no failure").value;
            let Auto::NotWatching {
                why: Blind::Unidentified { until, .. },
            } = looked
            else {
                panic!("not watching, not {looked:?}");
            };
            assert_eq!(until, clock.now() + wait);
            asked += 1;
            assert_eq!(m.api.calls(), asked, "asked once in round {round}");
            assert_eq!(
                stays(),
                vec![("here".to_string(), "not_identified".to_string()); round + 1]
            );

            clock.advance(wait - 1);
            let unidentified = |looked: &Auto| {
                matches!(
                    looked,
                    Auto::NotWatching {
                        why: Blind::Unidentified { .. }
                    }
                )
            };
            let looked = stands(&m);
            assert!(unidentified(&looked), "{looked:?}");
            let looked = auto(&m).expect("no failure").value;
            assert!(unidentified(&looked), "{looked:?}");
            assert_eq!(m.api.calls(), asked, "nobody asked while it waits");
            assert_eq!(stays().len(), round + 1, "and nothing more recorded");
            clock.advance(1);
        }
        assert!(logged(&m).is_empty(), "nothing attempted");

        m.api.owned_by("access-here-renewed", owner("here"));
        let looked = auto(&m).expect("a switch").value;
        assert!(matches!(looked, Auto::Switched { .. }), "{looked:?}");
        assert_eq!(live_refresh(&m).as_deref(), Some("there-refresh"));
    }

    /// A read that records whose a changed login is ends the failures in a row before it, as
    /// telling it under the lock does, though the look then finds nothing to decide and never
    /// takes the lock. The next failure, at Claude Code's next renewal, waits a minute again.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_read_that_tells_whose_the_login_is_ends_the_failures_in_a_row() {
        let (m, clock) = nearly_out("auto-told-by-a-read", 96.0);
        let waits_until = |m: &Machine| match auto(m).expect("no failure").value {
            Auto::NotWatching {
                why: Blind::Unidentified { until, .. },
            } => until,
            other => panic!("not watching, not {other:?}"),
        };
        m.api.token_trouble("access-here-renewed", Trouble::Offline);
        m.sign_in(&document("here-renewed"));
        assert_eq!(waits_until(&m), NOW + RETRY_SECONDS);

        clock.advance(RETRY_SECONDS);
        m.api.owned_by("access-here-renewed", owner("here"));
        m.api.using("access-here-renewed", answer(&m, 50.0, 20.0));
        Pitboard::new(m.ctx.clone()).status(false).expect("a read");
        let looked = stands(&m);
        assert_eq!(watching(&looked), Some("here"), "{looked:?}");

        clock.advance(10 * 60);
        m.api.token_trouble("access-here-again", Trouble::Offline);
        m.sign_in(&document("here-again"));
        measured(&m, "here", 96.0, 20.0);
        assert_eq!(waits_until(&m), clock.now() + RETRY_SECONDS);
    }

    /// A keychain this front end cannot read, as one locked in a session over SSH, is its
    /// failure alone. It is refused with that error and kept for nobody to wait on, so another
    /// front end that reads the keychain, such as the app, switches at once. No account was
    /// chosen, so the line names none: a label may be `claude`.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_keychain_one_front_end_cannot_read_holds_back_no_other() {
        let (m, _) = nearly_out("auto-locked-here", 96.0);
        m.fault_live(Fault::Locked);

        let failed = auto(&m).expect_err("the keychain is locked");
        assert_eq!(failed.error.code(), "credential_store_locked");
        assert!(!m.ctx_home().join(".pitboard/autoswitch.json").exists());
        assert_eq!(
            logged(&m),
            [(
                "auto-switch".into(),
                String::new(),
                "credential_store_locked".into()
            )]
        );

        m.mem.live().heal(&m.service);
        let looked = auto(&m).expect("a switch").value;
        assert!(matches!(looked, Auto::Switched { .. }), "{looked:?}");
        assert_eq!(live_refresh(&m).as_deref(), Some("there-refresh"));
    }

    /// The login Claude Code has stored is of an account nobody enrolled: there is no reading
    /// of it to judge and no account Pitboard could park it under, so that is said, and once
    /// recorded, said again from the record alone.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn an_account_nobody_enrolled_in_use_is_not_watched() {
        let (m, _) = nearly_out("auto-not-enrolled", 96.0);
        m.api.owned_by("access-stranger-refresh", owner("stranger"));
        m.sign_in(&document("stranger-refresh"));
        config_names(&m, "stranger");
        let not_enrolled = |looked: &Auto| {
            matches!(
                looked,
                Auto::NotWatching {
                    why: Blind::NotEnrolled { email },
                } if email == "stranger@example.com"
            )
        };

        let looked = auto(&m).expect("no failure").value;
        assert!(not_enrolled(&looked), "{looked:?}");
        assert_eq!(m.api.calls(), 1);
        let looked = auto(&m).expect("no failure").value;
        assert!(not_enrolled(&looked), "{looked:?}");
        assert_eq!(m.api.calls(), 1, "from the record");
        assert!(logged(&m).is_empty());
        assert_eq!(live_refresh(&m).as_deref(), Some("stranger-refresh"));
    }

    /// Claude Code has no login stored, as its `/logout` leaves it: nothing is watched.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn with_no_login_stored_nothing_is_watched() {
        let (m, _) = nearly_out("auto-signed-out", 96.0);
        m.sign_in(&serde_json::json!({"mcpOAuth": {"some-server": {"token": "unrelated"}}}));
        let path = m.ctx_home().join(".claude.json");
        let mut config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("a config")).expect("JSON");
        config
            .as_object_mut()
            .expect("an object")
            .remove("oauthAccount");
        std::fs::write(&path, config.to_string()).expect("written");

        for _ in 0..2 {
            let looked = auto(&m).expect("no failure").value;
            assert!(
                matches!(
                    looked,
                    Auto::NotWatching {
                        why: Blind::NothingSignedIn
                    }
                ),
                "{looked:?}"
            );
        }
        assert_eq!(m.api.calls(), 0);
        assert!(logged(&m).is_empty());
    }

    /// Claude Code authenticated some other way uses none of the logins Pitboard moves, so
    /// a switch would change nothing its sessions see.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
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
        for _ in 0..2 {
            let Auto::Skipped {
                why: Skip::Overridden(names),
                ..
            } = auto(&m).expect("a look").value
            else {
                panic!("skipped");
            };
            assert!(names[0].starts_with("apiKeyHelper in "), "{names:?}");
        }
        assert_eq!(m.mem.live().peek(&m.service), before);
        assert_eq!(
            stays(&m),
            [("here".to_string(), "auth_overridden".to_string())],
            "said once"
        );
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

    /// A sign-in to the account the look found a switch due to, landing before the lock is
    /// held, leaves Claude Code's config naming the account it would have left. Only a switch
    /// and `use` write the config, so the switch there is no longer any need for leaves the
    /// config as it is, and the login too.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_sign_in_to_the_account_it_would_switch_to_writes_no_config() {
        let (m, _) = nearly_out("auto-signed-in-to-target", 96.0);
        m.api.owned_by("access-there-again-refresh", owner("there"));
        m.sign_in(&document("there-again-refresh"));
        let config =
            || std::fs::read_to_string(m.ctx_home().join(".claude.json")).expect("a config");
        let before = config();

        let done = auto(&m).expect("nothing to do");

        assert_eq!(watching(&done.value), Some("there"), "{:?}", done.value);
        assert_eq!(config(), before);
        assert_eq!(live_refresh(&m).as_deref(), Some("there-again-refresh"));
    }
}
