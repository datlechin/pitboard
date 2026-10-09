//! The fixtures, each made on the real core: the macOS app's FixtureTests.swift, ported to
//! the worlds that replace its FixtureCore, and what each UI test reads in its world, asked
//! of the model an app launches into it. Where the real core answers otherwise than the
//! Swift fixture did, the test says so and holds what the core does.

use super::apps::CHATGPT;
use super::pages;
use super::tools::{CLAUDE_ADDRESS, CLAUDE_REFUSES, CODEX_ADDRESS};
use super::worlds::{Folder, Launched, World, make, out_of_reach};
use crate::model::{Intent, ModelListener, Pane, PitboardModel, PlatformError, Sheet, Snapshot};
use crate::present::testing::Utc;
use crate::present::{AccountsShown, Footing};
use crate::{
    Adoption, AppCore, Enrolled, EnrolledAs, FoundCommandLine, PitboardError, Schedule, Status,
    Switch,
};
use pitboard_core::provider::{self, ProviderId};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// As long as a test waits for another thread, however slow the machine running it.
const PATIENCE: Duration = Duration::from_secs(30);

/// A code as Claude Code's browser page shows one, `<code>#<state>`, which its sign-in takes:
/// what a UI test types.
const CODE: &str = "fixture-code#state";

/// `world`, made in a folder of this test's own.
fn made(world: World) -> Launched {
    make(world, Folder::own(world.name()).expect("a folder"))
        .unwrap_or_else(|unmade| panic!("{}: {unmade}", world.name()))
}

/// Each account the way a test reads it: its name with its tool, or the email of a login
/// with no name, then whether it is the one in use, or why it cannot be switched to.
fn described(status: &Status) -> Vec<String> {
    status
        .accounts
        .iter()
        .map(|account| {
            let name = account
                .qualified
                .clone()
                .unwrap_or_else(|| account.email.clone());
            if account.signed_in {
                format!("{name}, in use")
            } else if account.switchable {
                name
            } else {
                format!(
                    "{name}, {}",
                    account.stale.as_deref().unwrap_or("not switchable")
                )
            }
        })
        .collect()
}

/// The accounts enrolled that a read gives, each by its name with its tool and whether it is
/// in use: what an offline read and a fresh one agree on, whatever each knows of the
/// numbers. A login nobody has named is the fresh read's alone, since it asks its service
/// whose it is.
fn who(status: &Status) -> Vec<(String, bool)> {
    status
        .accounts
        .iter()
        .filter_map(|account| Some((account.qualified.clone()?, account.signed_in)))
        .collect()
}

/// The accounts in use, one per tool at most, by their names with their tools.
fn in_use(status: &Status) -> Vec<String> {
    status
        .accounts
        .iter()
        .filter(|account| account.signed_in)
        .filter_map(|account| account.qualified.clone())
        .collect()
}

/// The code a call was refused with, or `None` when it was not refused.
fn refusal<T>(answer: Result<T, PitboardError>) -> Option<String> {
    match answer {
        Ok(_) => None,
        Err(PitboardError::Failed { code, .. }) => Some(code),
    }
}

fn read(core: &AppCore) -> Status {
    core.status(true).expect("a read")
}

/// Runs a sign-in to the end the way the sheet does: reads everything the tool says, types a
/// code back once it asks for one, and finishes.
fn sign_in_to_the_end(core: &AppCore, label: &str) -> Result<Enrolled, PitboardError> {
    let session = core.sign_in(label.into())?;
    while let Some(line) = session.next_line() {
        if line.contains("Paste code") {
            session.paste(CODE.into())?;
        }
    }
    session.finish()
}

// Where each fixture starts.

/// The UI tests launch the app into these fixtures and assert on what each starts with, so
/// a change here is a change to what they test: who a read shows, or the code it fails
/// with, the codes it warns with, who the offline read shows, and which tools were found.
/// readFailure's reads both fail, as the core's do where its account index cannot be read,
/// so there is nothing known to fall back on. stuck's read warns that an interrupted switch
/// is waiting, as the Swift fixture's has since the core's read said so, and its offline
/// read says nothing of it, since only Anthropic could say whose Claude Code's login is.
///
/// FixtureTests.swift's eachFixtureStartsWhereItsTestsExpect.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn each_fixture_starts_where_its_tests_expect() {
    let work = "claude/work, in use";
    for world in World::ALL {
        let expected: Vec<&str> = match world {
            World::TwoTools | World::ChatGptOpen => vec![
                work,
                "claude/old, parked_access_expired",
                "claude/personal",
                "codex/main, in use",
                "codex/spare",
            ],
            World::OneTool | World::Stuck => vec![work, "claude/personal"],
            World::OnlyOne => vec![work],
            World::Unnamed => vec!["dana@work.example, in use"],
            World::Empty | World::FirstLaunch | World::NoClaudeCode => Vec::new(),
            // No read answers, below.
            World::ReadFailure => Vec::new(),
        };
        let launched = made(world);
        let core = &launched.core;
        if world == World::ReadFailure {
            assert_eq!(
                refusal(core.status(true)).as_deref(),
                Some("state_unreadable")
            );
            assert_eq!(
                refusal(core.status_offline()).as_deref(),
                Some("state_unreadable")
            );
        } else {
            let fresh = read(core);
            assert_eq!(described(&fresh), expected, "{}", world.name());
            let warned: Vec<&str> = fresh
                .warnings
                .iter()
                .map(|warning| warning.code.as_str())
                .collect();
            let expected: &[&str] = if world == World::Stuck {
                &["recovery_undetermined"]
            } else {
                &[]
            };
            assert_eq!(warned, expected, "{}", world.name());
            assert!(
                fresh
                    .accounts
                    .iter()
                    .all(|account| account.stale.as_deref() != Some("unreachable")),
                "{}",
                world.name()
            );
            let offline = core.status_offline().expect("what is known");
            assert_eq!(who(&offline), who(&fresh), "{}", world.name());
            assert_eq!(offline.warnings, Vec::new(), "{}", world.name());
        }
        assert_eq!(
            crate::tools()
                .iter()
                .map(|tool| tool.code.as_str())
                .collect::<Vec<_>>(),
            ["claude", "codex"]
        );
        let installed: Vec<String> = core.installed().into_iter().map(|tool| tool.code).collect();
        let expected: &[&str] = if world == World::NoClaudeCode {
            &[]
        } else {
            &["claude", "codex"]
        };
        assert_eq!(installed, expected, "{}", world.name());
    }
}

/// The UI tests launch a fixture by its name, from a list of their own, so a name changed
/// here has to change there.
///
/// FixtureTests.swift's theUITestsNameEveryFixtureAsTheAppDoes, but for the variable, which
/// is the app's to read.
#[test]
fn the_ui_tests_name_every_fixture_as_the_app_does() {
    assert_eq!(
        super::fixture_names(),
        [
            "twoTools",
            "oneTool",
            "empty",
            "firstLaunch",
            "noClaudeCode",
            "unnamed",
            "onlyOne",
            "readFailure",
            "stuck",
            "chatGPTOpen",
        ]
    );
}

// Changing accounts.

/// A switch moves who is in use within the tool it is for and leaves the other tool alone,
/// and the account it left can be switched back to. It says what running sessions do:
/// Claude Code's follow within the 33 seconds the core measured, where the Swift fixture
/// said 45, and Codex's keep the old account until they are started again. The account
/// already in use is not switched to again, and an account nobody enrolled is refused.
///
/// FixtureTests.swift's aSwitchMovesWhoIsInUseWithinItsOwnTool.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_switch_moves_who_is_in_use_within_its_own_tool() {
    let launched = made(World::TwoTools);
    let core = &launched.core;

    let claude = core.switch_to("claude/personal".into()).expect("a switch");
    assert!(
        matches!(
            claude.outcome,
            Switch::Switched {
                ref provider,
                adoption: Adoption::Follows { within_seconds: 33 },
                ..
            } if provider == "claude"
        ),
        "{:?}",
        claude.outcome
    );
    let after_claude = read(core);
    assert_eq!(in_use(&after_claude), ["claude/personal", "codex/main"]);
    assert!(
        after_claude
            .accounts
            .iter()
            .any(
                |account| account.qualified.as_deref() == Some("claude/work") && account.switchable
            )
    );

    let codex = core.switch_to("codex/spare".into()).expect("a switch");
    assert_eq!(
        codex.outcome,
        Switch::Switched {
            provider: "codex".into(),
            from: "codex/main".into(),
            to: "codex/spare".into(),
            adoption: Adoption::Restart {
                program: "codex".into()
            },
        }
    );
    let after_codex = read(core);
    assert_eq!(in_use(&after_codex), ["claude/personal", "codex/spare"]);
    assert!(
        after_codex
            .accounts
            .iter()
            .any(|account| account.qualified.as_deref() == Some("codex/main")
                && account.switchable)
    );

    assert_eq!(
        core.switch_to("claude/personal".into())
            .expect("already in use")
            .outcome,
        Switch::AlreadyActive {
            label: "personal".into()
        }
    );
    assert!(refusal(core.switch_to("claude/nobody".into())).is_some());
    assert_eq!(in_use(&read(core)), ["claude/personal", "codex/spare"]);
}

/// A parked login that expired stays unusable until somebody signs in to it again. A switch
/// to it is refused and moves nothing, and a switch between two other accounts of its tool
/// leaves it as it was, still offering a sign-in rather than a switch.
///
/// FixtureTests.swift's anExpiredParkedLoginStaysUnusableAcrossSwitches.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn an_expired_parked_login_stays_unusable_across_switches() {
    let launched = made(World::TwoTools);
    let core = &launched.core;
    assert_eq!(
        refusal(core.switch_to("claude/old".into())).as_deref(),
        Some("parked_login_expired")
    );
    assert_eq!(in_use(&read(core)), ["claude/work", "codex/main"]);

    core.switch_to("claude/personal".into()).expect("a switch");
    assert_eq!(
        described(&read(core)),
        [
            "claude/personal, in use",
            "claude/old, parked_access_expired",
            "claude/work",
            "codex/main, in use",
            "codex/spare",
        ]
    );
}

/// The core names the accounts of a switch as the command line types them: bare for Claude
/// Code, with the tool for any other. The model matches what a switch said against that, so
/// what a Claude Code switch said is kept once the read after it has landed.
///
/// FixtureTests.swift's aSwitchNamesTheAccountsAsTheCoreTypesThem, its model half asked of
/// the model an app launches into the world.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_switch_names_the_accounts_as_the_core_types_them() {
    {
        let launched = made(World::TwoTools);
        let core = &launched.core;
        assert_eq!(
            core.switch_to("codex/main".into())
                .expect("already in use")
                .outcome,
            Switch::AlreadyActive {
                label: "codex/main".into()
            }
        );
        assert_eq!(
            core.switch_to("claude/personal".into())
                .expect("a switch")
                .outcome,
            Switch::Switched {
                provider: "claude".into(),
                from: "work".into(),
                to: "personal".into(),
                adoption: Adoption::Follows { within_seconds: 33 },
            }
        );
    }

    let launched = made(World::TwoTools);
    let (model, told) = started(&launched);
    model.send(Intent::SwitchTo {
        qualified: "claude/personal".into(),
    });
    let last = told.until("the switch, read back", |snapshot| {
        snapshot.switch_under_way.is_none()
            && !snapshot.reading
            && snapshot
                .status
                .as_ref()
                .is_some_and(|status| in_use(status).contains(&"claude/personal".into()))
    });
    assert_eq!(
        last.last_switches
            .iter()
            .map(|said| said.provider.as_str())
            .collect::<Vec<_>>(),
        ["claude"]
    );
    model.shutdown();
}

/// The login signed in with no name is named in place, and stays the account in use. A name
/// its tool already has is refused and leaves it unnamed, and once it has a name there is
/// nobody left to name.
///
/// FixtureTests.swift's theLoginSignedInNowIsNamedWithANameNotTaken.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn the_login_signed_in_now_is_named_with_a_name_not_taken() {
    let launched = made(World::Unnamed);
    let core = &launched.core;
    sign_in_to_the_end(core, "claude/personal").expect("signed in");
    assert_eq!(
        refusal(core.enroll_current("personal".into())).as_deref(),
        Some("label_taken")
    );
    assert_eq!(
        described(&read(core)),
        ["dana@work.example, in use", "claude/personal"]
    );

    assert_eq!(
        core.enroll_current("work".into()).expect("named"),
        Enrolled {
            email: "dana@work.example".into(),
            outcome: EnrolledAs::Current,
            warnings: Vec::new(),
        }
    );
    assert_eq!(
        described(&read(core)),
        ["claude/work, in use", "claude/personal"]
    );
    assert!(refusal(core.enroll_current("home".into())).is_some());
}

/// The account in use cannot be forgotten, since its login is the one the tool is using.
/// Any other can, and goes from the list.
///
/// FixtureTests.swift's onlyAnAccountNotInUseIsForgotten.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn only_an_account_not_in_use_is_forgotten() {
    let launched = made(World::TwoTools);
    let core = &launched.core;
    assert_eq!(
        refusal(core.forget("claude/work".into())).as_deref(),
        Some("cannot_forget_active_account")
    );
    assert!(refusal(core.forget("claude/nobody".into())).is_some());

    core.forget("codex/spare".into()).expect("forgotten");
    assert_eq!(
        described(&read(core)),
        [
            "claude/work, in use",
            "claude/old, parked_access_expired",
            "claude/personal",
            "codex/main, in use",
        ]
    );
}

/// A rename needs a name its tool has not given another account. Another tool's names do
/// not count, since a name is only ever used with its tool.
///
/// FixtureTests.swift's aRenameNeedsANameItsToolHasNotGiven.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_rename_needs_a_name_its_tool_has_not_given() {
    let launched = made(World::TwoTools);
    let core = &launched.core;
    assert_eq!(
        refusal(core.rename("claude/personal".into(), "old".into())).as_deref(),
        Some("label_taken")
    );

    core.rename("claude/personal".into(), "main".into())
        .expect("renamed");
    assert_eq!(
        described(&read(core)),
        [
            "claude/work, in use",
            "claude/old, parked_access_expired",
            "claude/main",
            "codex/main, in use",
            "codex/spare",
        ]
    );
}

// Signing in.

/// Claude Code's sign-in prints the address to open and asks for the code from the browser,
/// which the sheet shows as a link and a field, and goes no further until a code is typed
/// back. Finishing then parks the new account beside the one in use.
///
/// FixtureTests.swift's aClaudeCodeSignInWaitsForTheCodeAndThenParksTheAccount.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_claude_code_sign_in_waits_for_the_code_and_then_parks_the_account() {
    let launched = made(World::OneTool);
    let core = &launched.core;
    let session = core.sign_in("claude/travel".into()).expect("started");
    let mut said = String::new();
    while let Some(line) = session.next_line() {
        said.push_str(&line);
        if line.contains("Paste code") {
            break;
        }
    }
    let shown = provider::sign_in_view(ProviderId::Claude, &said, false);
    assert_eq!(shown.url.as_deref(), Some(CLAUDE_ADDRESS));
    assert!(shown.wants_code);

    let asked = Instant::now();
    let pasting = {
        let session = Arc::clone(&session);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            session.paste(CODE.into())
        })
    };
    assert_eq!(session.next_line(), None);
    assert!(
        asked.elapsed() >= Duration::from_millis(300),
        "nothing more until the code is typed"
    );
    pasting.join().expect("the paste").expect("typed back");

    assert_eq!(
        session.finish().expect("enrolled"),
        Enrolled {
            email: "travel@example.com".into(),
            outcome: EnrolledAs::SignedIn,
            warnings: Vec::new(),
        }
    );
    assert_eq!(
        described(&read(core)),
        ["claude/work, in use", "claude/personal", "claude/travel"]
    );
}

/// Claude Code's sign-in refuses a line typed back that is not `<code>#<state>` with both
/// halves, with its own `Invalid code.` line, and reads on in the same sign-in, asking
/// nothing again; the first line with both halves it takes, whatever its state says, as the
/// register's `sign_in_takes_another_code` holds of 2.1.289. A line typed before it asks is
/// read once it does.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn claude_code_refuses_a_code_without_both_halves_and_reads_another() {
    let launched = made(World::OneTool);
    let core = &launched.core;
    let session = core.sign_in("claude/travel".into()).expect("started");
    session.paste("fixture-code".into()).expect("typed back");
    let mut said = String::new();
    while let Some(line) = session.next_line() {
        said.push_str(&line);
        if line == CLAUDE_REFUSES {
            break;
        }
    }
    assert!(
        said.ends_with(&format!("Paste code here if prompted > {CLAUDE_REFUSES}")),
        "{said:?}"
    );
    for refused in ["#state", "fixture-code#", "  fixture-code  "] {
        session.paste(refused.into()).expect("typed back");
        assert_eq!(
            session.next_line().as_deref(),
            Some(CLAUDE_REFUSES),
            "{refused:?}"
        );
    }
    session
        .paste(" fixture-code#any state at all ".into())
        .expect("typed back");
    assert_eq!(session.next_line(), None);
    assert_eq!(
        session.finish().expect("enrolled").email,
        "travel@example.com"
    );
}

/// A code Claude Code refused is said in the sheet, in the sentence the owner approved on 6
/// October 2026, and the field is offered again for the same sign-in, which takes the next
/// code: what AccountsWindowTests.swift's tests that add a Claude Code account meet in this
/// world, where they type `fixture-code` with no state, until they type a code with its `#`.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_refused_code_is_said_in_the_sheet_and_another_is_asked_for() {
    let launched = made(World::OneTool);
    let (model, told) = started(&launched);
    model.send(Intent::PresentSheet {
        sheet: Sheet::Add { provider: None },
    });
    model.send(Intent::SignIn {
        provider: "claude".into(),
        name: "third".into(),
    });
    told.until("the code asked for", |snapshot| {
        snapshot
            .signing_in
            .as_ref()
            .is_some_and(|signing| signing.wants_code)
    });
    model.send(Intent::PasteCode {
        code: "fixture-code".into(),
    });
    let refused = told.until("the code refused", |snapshot| {
        snapshot
            .signing_in
            .as_ref()
            .is_some_and(|signing| signing.wants_code && signing.code_refused)
    });
    assert_eq!(
        refused
            .signing_in_text
            .and_then(|text| text.refused)
            .as_deref(),
        Some(
            "Claude Code didn’t take that code. Copy the whole code your browser shows, and \
             paste it again."
        )
    );
    model.send(Intent::PasteCode { code: CODE.into() });
    told.until("the account listed", |snapshot| {
        snapshot.signing_in.is_none()
            && read_in(snapshot)
            && snapshot.status.as_ref().is_some_and(|status| {
                status
                    .accounts
                    .iter()
                    .any(|account| account.qualified.as_deref() == Some("claude/third"))
            })
    });
    model.shutdown();
}

/// Codex's sign-in prints the address to open beside the loopback address the browser comes
/// back to, reads nothing typed, and finishes by itself once the browser is done.
///
/// FixtureTests.swift's aCodexSignInFinishesByItself.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_codex_sign_in_finishes_by_itself() {
    let launched = made(World::TwoTools);
    let core = &launched.core;
    let session = core.sign_in("codex/travel".into()).expect("started");
    let mut said = String::new();
    while let Some(line) = session.next_line() {
        said.push_str(&line);
    }
    let shown = provider::sign_in_view(ProviderId::Codex, &said, false);
    assert_eq!(shown.url.as_deref(), Some(CODEX_ADDRESS));
    assert!(!shown.wants_code);

    assert_eq!(
        session.finish().expect("enrolled"),
        Enrolled {
            email: "travel@example.com".into(),
            outcome: EnrolledAs::SignedIn,
            warnings: Vec::new(),
        }
    );
    let accounts = described(&read(core));
    assert_eq!(
        accounts[accounts.len() - 3..],
        ["codex/main, in use", "codex/spare", "codex/travel"]
    );
}

/// A sign-in stopped part way enrols nothing: it stops waiting for a code, and finishing it
/// fails as stopped, the way the tool's own does.
///
/// FixtureTests.swift's aStoppedSignInEnrolsNothing.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_stopped_sign_in_enrols_nothing() {
    let launched = made(World::OneTool);
    let core = &launched.core;
    let session = core.sign_in("claude/travel".into()).expect("started");
    let reading = {
        let session = Arc::clone(&session);
        std::thread::spawn(move || while session.next_line().is_some() {})
    };
    session.cancel();
    reading.join().expect("the reading ends");

    assert!(refusal(session.finish()).is_some());
    assert_eq!(
        described(&read(core)),
        ["claude/work, in use", "claude/personal"]
    );
}

/// Signing in again to an account whose parked login expired renews that account rather
/// than adding a second one, and it can be switched to again, with nothing wrong with it any
/// more.
///
/// FixtureTests.swift's signingInAgainToAnExpiredAccountMakesItSwitchable and
/// signingInAgainPutsAwayWhatWasWrongWithTheParkedLogin.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn signing_in_again_to_an_expired_account_makes_it_switchable() {
    let launched = made(World::TwoTools);
    let core = &launched.core;
    assert_eq!(
        sign_in_to_the_end(core, "claude/old").expect("signed in"),
        Enrolled {
            email: "dana@old.example".into(),
            outcome: EnrolledAs::Renewed,
            warnings: Vec::new(),
        }
    );
    let after = read(core);
    assert_eq!(
        described(&after),
        [
            "claude/work, in use",
            "claude/old",
            "claude/personal",
            "codex/main, in use",
            "codex/spare",
        ]
    );
    let old = after
        .accounts
        .iter()
        .find(|account| account.qualified.as_deref() == Some("claude/old"))
        .expect("old");
    assert_eq!((&old.stale, &old.stale_explanation), (&None, &None));
}

/// Signing in again to the account in use puts its new login in use at once, as the core
/// does, and parks nothing: it stays the account in use and is not one to switch to.
///
/// FixtureTests.swift's signingInAgainToTheAccountInUseKeepsItInUse.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn signing_in_again_to_the_account_in_use_keeps_it_in_use() {
    let launched = made(World::OneTool);
    let core = &launched.core;
    assert_eq!(
        sign_in_to_the_end(core, "claude/work").expect("signed in"),
        Enrolled {
            email: "dana@work.example".into(),
            outcome: EnrolledAs::InUse { again: true },
            warnings: Vec::new(),
        }
    );
    let after = read(core);
    assert_eq!(
        described(&after),
        ["claude/work, in use", "claude/personal"]
    );
    assert!(
        after.accounts.iter().any(
            |account| account.qualified.as_deref() == Some("claude/work") && !account.switchable
        )
    );
}

// The rest of the machine.

/// An interrupted switch nothing can finish is given up on once, and asking again has
/// nothing to give up on. Until then a read warns of it, with the words a change is refused
/// with, and every change is refused, since each finishes an interrupted switch before
/// anything else; after, neither. Claude Code's session has expired meanwhile, which giving
/// up does not change.
///
/// FixtureTests.swift's givingUpOnTheInterruptedSwitchHappensOnce.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn giving_up_on_the_interrupted_switch_happens_once() {
    let launched = made(World::Stuck);
    let core = &launched.core;
    let warned = read(core).warnings;
    assert_eq!(
        warned
            .iter()
            .map(|warning| warning.code.as_str())
            .collect::<Vec<_>>(),
        ["recovery_undetermined"]
    );
    match core.switch_to("claude/personal".into()) {
        Err(PitboardError::Failed { code, message, .. }) => {
            assert_eq!(code, "recovery_undetermined");
            assert_eq!(
                message, warned[0].message,
                "the read says what a change is told"
            );
        }
        other => panic!("{:?}", other.map(|_| ())),
    }

    assert_eq!(
        core.abandon_recovery().expect("given up"),
        Some(crate::Abandoned {
            from: "work".into(),
            to: "personal".into(),
            logins_kept: 2,
        })
    );
    assert_eq!(core.abandon_recovery().expect("nothing to give up"), None);
    let after = read(core);
    assert_eq!(
        described(&after),
        ["claude/work, in use", "claude/personal"]
    );
    assert_eq!(after.warnings, Vec::new(), "nothing is waiting any more");
    assert_ne!(
        refusal(core.switch_to("claude/personal".into())).as_deref(),
        Some("recovery_undetermined"),
        "nothing is waiting any more"
    );
}

/// The log keeps what changed newest last, and names Claude Code's accounts bare and any
/// other tool's with the tool, as the command line types them. The core logs a switch as
/// `use`, where the Swift fixture wrote `switch`. Work signed in to Claude Code after old was
/// enrolled privately with nothing signed in, and enrolling work found that, which the log
/// keeps as `in-use` once work is enrolled, under its label.
///
/// FixtureTests.swift's theLogRecordsChangesNewestLastAsTheyAreTyped, but for when the
/// account index last changed: that a change made elsewhere is noticed is threaded.rs's
/// `a_change_another_front_end_makes_is_told_without_asking_anyone`.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn the_log_records_changes_newest_last_as_they_are_typed() {
    let launched = made(World::TwoTools);
    let core = &launched.core;
    let history = core.log(500);
    assert_eq!(
        history
            .iter()
            .map(|change| format!("{} {} {}", change.caller, change.verb, change.subject))
            .collect::<Vec<_>>(),
        [
            "cli enroll old",
            "cli enroll codex/main",
            "cli in-use work",
            "cli enroll work",
            "app enroll personal",
            "app enroll codex/spare",
            "cli use personal",
            "app use work",
        ]
    );

    core.switch_to("claude/personal".into()).expect("a switch");
    core.switch_to("codex/spare".into()).expect("a switch");
    core.rename("claude/work".into(), "office".into())
        .expect("renamed");
    let log = core.log(500);
    assert_eq!(
        log[history.len()..]
            .iter()
            .map(|change| format!("{} {}", change.verb, change.subject))
            .collect::<Vec<_>>(),
        ["use personal", "use codex/spare", "rename work -> office"]
    );
    assert!(
        log[log.len() - 3..]
            .iter()
            .all(|change| change.caller == "app" && change.outcome == "ok")
    );
    let dates: Vec<i64> = log
        .iter()
        .filter_map(|change| pitboard_core::time::parse(&change.at))
        .collect();
    assert_eq!(dates.len(), log.len());
    assert!(dates.windows(2).all(|pair| pair[0] <= pair[1]), "{dates:?}");
    assert_eq!(core.log(2), log[log.len() - 2..]);
}

/// Daily renewal can be turned on and off, and taking away a schedule that is not there
/// says there was nothing to take away. Nothing is ever repaired. The schedule is a file in
/// the fixture's own folder, which no scheduler is asked to start.
///
/// FixtureTests.swift's theScheduleTurnsOnAndOff.
#[test]
#[cfg_attr(windows, ignore = "W25: Task Scheduler")]
fn the_schedule_turns_on_and_off() {
    let launched = made(World::OneTool);
    let core = &launched.core;
    assert_eq!(core.schedule(), Some(Schedule::Absent));
    assert_eq!(core.schedule_uninstall().ok(), Some(false));

    let path = core.schedule_install().expect("installed");
    assert!(
        std::path::Path::new(&path).starts_with(launched.machine.root()),
        "{path}"
    );
    assert_eq!(
        core.schedule(),
        Some(Schedule::Installed {
            path,
            every_seconds: 86_400
        })
    );
    assert_eq!(core.schedule_uninstall().ok(), Some(true));
    assert_eq!(core.schedule(), Some(Schedule::Absent));
    assert_eq!(core.schedule_repair().ok(), Some(false));
}

// Launching into a fixture.

/// A UI test launches the app into a fixture many times, and each launch starts where the
/// last one did, whatever the last one left: the folder emptied and made again, seen before
/// unless it is the first launch, what it has told empty, and the same accounts, or, in
/// readFailure, the same failure, its account index that nobody may read emptied with the
/// rest. The first launch is kept while the second is made in its folder, as the folder an
/// app launches into is kept from one launch to the next, so what the first left is there
/// to be emptied.
///
/// FixtureTests.swift's everyLaunchStartsWhereTheLastOneDid, but for the login item and
/// notifications' permission, which are the app's.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn every_launch_starts_where_the_last_one_did() {
    use pitboard_core::app::AppFile;
    let known = |core: &AppCore| match core.status_offline() {
        Ok(status) => Ok(who(&status)),
        Err(PitboardError::Failed { code, .. }) => Err(code),
    };
    for world in World::ALL {
        let first = made(world);
        let core = &first.core;
        let accounts = known(core);
        assert_eq!(
            accounts.is_err(),
            world == World::ReadFailure,
            "{}",
            world.name()
        );
        let preferences = core.app_file(AppFile::Preferences).expect("readable");
        assert_eq!(
            preferences
                .as_deref()
                .map(|text| text.contains(r#""has_been_seen":true"#)),
            (world != World::FirstLaunch).then_some(true),
            "{}",
            world.name()
        );
        // What a launch leaves behind: a switch, what it told, preferences of its own, and
        // anything else in its folder.
        core.switch_to("claude/personal".into()).ok();
        core.keep_app_file(AppFile::Told, r#"{"claude/work/session/":7200}"#)
            .expect("told");
        core.keep_app_file(
            AppFile::Preferences,
            r#"{"has_been_seen":true,"second_account_declined":["claude"]}"#,
        )
        .expect("kept");
        let left = first.machine.root().join("left behind");
        std::fs::write(&left, "").expect("left behind");

        let again = made(world);
        assert_eq!(
            again.machine.root(),
            first.machine.root(),
            "the same folder"
        );
        assert!(!left.exists(), "{}: the folder emptied", world.name());
        let core = &again.core;
        assert_eq!(known(core), accounts, "{}", world.name());
        assert_eq!(
            core.app_file(AppFile::Preferences).expect("readable"),
            preferences,
            "{}",
            world.name()
        );
        assert_eq!(
            core.app_file(AppFile::Told).expect("readable"),
            None,
            "{}",
            world.name()
        );
        assert!(again.machine.root().starts_with(std::env::temp_dir()));
    }
}

/// The command line is inside a stand-in app in the fixture's own folder, one a schedule
/// would keep reaching and a link could reach, and a terminal finds none until one is linked
/// in the folder's `bin`, where the macOS app's fixture links it: linking is the app's own.
///
/// FixtureTests.swift's aLaunchLinksItsCommandLineInATemporaryDirectory, but for linking.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_launch_keeps_its_command_line_in_its_own_folder() {
    let launched = made(World::OneTool);
    let core = &launched.core;
    let own = core.own_command_line();
    assert!(own.lasting(), "{own:?}");
    assert!(
        launched
            .machine
            .helper()
            .starts_with(launched.machine.root())
    );
    assert_eq!(core.command_line(), FoundCommandLine::Nowhere);

    // A link as the macOS app makes one, which only a Unix system has.
    #[cfg(unix)]
    {
        let link = launched.machine.bin().join("pitboard");
        pitboard_core::testing::fs::link(&launched.machine.helper(), &link).expect("linked");
        assert_eq!(
            core.command_line(),
            FoundCommandLine::Bundled {
                path: link.to_string_lossy().into_owned()
            }
        );
    }
}

// The model an app launches into a fixture.

/// Keeps every snapshot it is told of.
#[derive(Default)]
struct Told {
    snapshots: Mutex<Vec<Snapshot>>,
    arrived: Condvar,
}

impl ModelListener for Told {
    fn changed(&self, snapshot: Snapshot) -> Result<(), PlatformError> {
        self.snapshots
            .lock()
            .expect("a test's own lock")
            .push(snapshot);
        self.arrived.notify_all();
        Ok(())
    }
}

impl Told {
    /// Waits until the last snapshot told satisfies `done`, and gives it.
    fn until(&self, what: &str, done: impl Fn(&Snapshot) -> bool) -> Snapshot {
        let started = Instant::now();
        let mut snapshots = self.snapshots.lock().expect("a test's own lock");
        loop {
            if let Some(last) = snapshots.last()
                && done(last)
            {
                return last.clone();
            }
            let left = PATIENCE
                .checked_sub(started.elapsed())
                .unwrap_or_else(|| panic!("never told {what}: {:#?}", snapshots.last()));
            snapshots = self
                .arrived
                .wait_timeout(snapshots, left)
                .expect("a test's own lock")
                .0;
        }
    }
}

/// Whether the accounts have been read, and nothing is being read.
fn read_in(snapshot: &Snapshot) -> bool {
    snapshot.status.is_some() && !snapshot.reading && snapshot.updated_at.is_some()
}

/// The app's model over `launched`, started, once the accounts have been read.
fn started(launched: &Launched) -> (Arc<PitboardModel>, Arc<Told>) {
    let told = Arc::new(Told::default());
    let model = launched.model(Arc::clone(&told) as Arc<dyn ModelListener>, Arc::new(Utc));
    model.send(Intent::Start);
    told.until("the accounts read", read_in);
    (model, told)
}

/// The UI tests open the menu before the window, and the menu shows only what a read has
/// found. The app launched into a fixture reads by itself, as it does on a real machine,
/// with nothing opened and nothing pressed.
///
/// FixtureTests.swift's aLaunchReadsWithoutBeingAsked.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_launch_reads_without_being_asked() {
    let launched = made(World::TwoTools);
    let (model, told) = started(&launched);
    let last = told.until("the accounts read", read_in);
    assert_eq!(
        described(last.status.as_ref().expect("read")),
        described(&read(&launched.core))
    );
    model.shutdown();
}

/// The fixture's ChatGPT runs Codex's login while it is open, as the process list shows it,
/// and its app control quits and opens it: a switch through the fixture quits it, switches
/// and opens it again, which is what the UI test drives.
///
/// FixtureTests.swift's theChatGPTFixtureIsQuitForACodexSwitchAndOpenedAgain.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn the_chatgpt_fixture_is_quit_for_a_codex_switch_and_opened_again() {
    let launched = made(World::ChatGptOpen);
    let core = &launched.core;
    assert_eq!(
        core.holding("codex".into())
            .iter()
            .map(|held| held.kind.as_str())
            .collect::<Vec<_>>(),
        ["chatgpt_app"]
    );
    assert!(core.holding("claude".into()).is_empty());

    let (model, told) = started(&launched);
    model.send(Intent::SwitchTo {
        qualified: "codex/spare".into(),
    });
    let asked = told.until("the quit question", |snapshot| {
        snapshot.quit_question.is_some()
    });
    let question = asked.quit_question.expect("asked");
    assert_eq!(question.name, "ChatGPT");
    model.send(Intent::QuitAndSwitch {
        qualified: question.qualified,
    });
    told.until("the switch, read back", |snapshot| {
        snapshot.switch_under_way.is_none()
            && snapshot
                .status
                .as_ref()
                .is_some_and(|status| in_use(status).contains(&"codex/spare".into()))
    });
    assert_eq!(
        launched.apps.asked(),
        [format!("quit {CHATGPT}"), format!("open {CHATGPT}")]
    );
    assert!(launched.apps.is_running(CHATGPT));
    model.shutdown();
}

/// The fixture's ChatGPT leaves the process list as it quits and comes back as it is
/// opened again, so the core sees it go and come back as it would on a real machine.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn chatgpt_leaves_the_process_list_as_it_quits() {
    use crate::model::AppControl;
    let launched = made(World::ChatGptOpen);
    let (apps, core) = (&launched.apps, &launched.core);
    let copy = apps
        .running(CHATGPT.into())
        .expect("asked")
        .expect("running");
    apps.request_quit(CHATGPT.into()).expect("asked to quit");
    assert_eq!(apps.running(CHATGPT.into()).expect("asked"), None);
    assert!(core.holding("codex".into()).is_empty());
    apps.reopen(copy).expect("opened");
    assert_eq!(core.holding("codex".into()).len(), 1);
    assert!(
        !made(World::TwoTools).apps.is_running(CHATGPT),
        "only chatGPTOpen has it open"
    );
}

// What each UI test reads in its world.

/// Every account is an item under its tool, as MenuBarTests.swift's
/// testTheMenuListsEveryAccountUnderItsTool reads them.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn the_menu_lists_every_account_under_its_tool() {
    let launched = made(World::TwoTools);
    let (model, told) = started(&launched);
    let shown = told.until("the accounts read", read_in);
    let sections: Vec<(Option<String>, Vec<String>)> = shown
        .sections
        .iter()
        .map(|section| {
            (
                section.heading.clone(),
                section
                    .accounts
                    .iter()
                    .map(|item| item.title.clone())
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        sections,
        [
            (
                Some("Claude Code".into()),
                vec!["work".into(), "old".into(), "personal".into()]
            ),
            (Some("Codex".into()), vec!["main".into(), "spare".into()]),
        ]
    );
    model.shutdown();
}

/// A machine without Claude Code says so first, as MenuBarTests.swift's
/// testAMachineWithoutClaudeCodeSaysSo reads it; one nobody is signed in on offers an
/// account, as AccountsWindowTests.swift's testAnEmptyMachineOffersAnAccount reads it; and
/// only the first launch ever opens the window by itself, as its
/// testTheFirstLaunchOpensTheWindow reads it.
#[test]
#[cfg_attr(
    windows,
    ignore = "W16: Pitboard writing, replacing and removing files on Windows"
)]
fn a_machine_with_nothing_on_it_says_what_to_do_first() {
    for world in [World::NoClaudeCode, World::Empty, World::FirstLaunch] {
        let launched = made(world);
        let (model, told) = started(&launched);
        let shown = told.until("the accounts read", read_in);
        match world {
            World::NoClaudeCode => {
                assert_eq!(shown.footing, Footing::NoClaudeCode);
                assert_eq!(
                    shown
                        .menu_notices
                        .install
                        .as_ref()
                        .map(|item| item.title.as_str()),
                    Some("Claude Code isn’t installed")
                );
            }
            _ => assert!(
                matches!(&shown.accounts_shown, AccountsShown::NoAccounts { title, .. } if title == "No Accounts"),
                "{}: {:?}",
                world.name(),
                shown.accounts_shown
            ),
        }
        // The window is asked for once the preferences say the app has never been seen, and
        // they are read on a lane of their own, so the accounts can be read first: in 2 of
        // 800 copies of this test run 16 at a time on Linux, firstLaunch's had not yet asked.
        // A machine whose app has been seen never asks, which `keeping.rs` holds.
        if world == World::FirstLaunch {
            told.until("the window asked for", |snapshot| {
                read_in(snapshot) && snapshot.window_request.serial > 0
            });
        } else {
            assert_eq!(shown.window_request.serial, 0, "{}", world.name());
        }
        model.shutdown();
    }
}

/// The login in use with no name is offered one, as AccountsWindowTests.swift's
/// testNamingTheAccountInUse reads it, and naming it lists it.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn the_login_in_use_is_offered_a_name() {
    let launched = made(World::Unnamed);
    let (model, told) = started(&launched);
    let shown = told.until("the accounts read", read_in);
    assert_eq!(
        shown.setup.as_ref().map(|step| step.title.as_str()),
        Some("Give this account a name")
    );
    model.send(Intent::Enrol {
        provider: "claude".into(),
        name: "work".into(),
    });
    told.until("the account named", |snapshot| {
        read_in(snapshot)
            && snapshot.status.as_ref().is_some_and(|status| {
                status
                    .accounts
                    .iter()
                    .any(|account| account.qualified.as_deref() == Some("claude/work"))
            })
    });
    model.shutdown();
}

/// One account is offered a second until Not Now, as AccountsWindowTests.swift's
/// testOneAccountIsOfferedASecondUntilNotNow reads it, which the fixture's own folder keeps.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn one_account_is_offered_a_second_until_not_now() {
    let launched = made(World::OnlyOne);
    let (model, told) = started(&launched);
    let shown = told.until("the accounts read", read_in);
    assert_eq!(
        shown.setup.as_ref().map(|step| step.title.as_str()),
        Some("Add a second account")
    );
    model.send(Intent::DeclineSecondAccount {
        provider: "claude".into(),
    });
    told.until("the nudge put away", |snapshot| snapshot.setup.is_none());
    let started = Instant::now();
    while !launched
        .core
        .app_file(pitboard_core::app::AppFile::Preferences)
        .expect("readable")
        .is_some_and(|kept| kept.contains(r#""second_account_declined":["claude"]"#))
    {
        assert!(started.elapsed() < PATIENCE, "never kept");
        std::thread::sleep(Duration::from_millis(10));
    }
    model.shutdown();
}

/// Activity lists what Pitboard changed, newest first, every switch above every enrolment,
/// as PanesAndSettingsTests.swift's testActivityListsChanges and AccountsWindowTests.swift's
/// testCommandNAddsAnAccountFromAnyPane read it. A switch is named "Switch" from the verb
/// the core logs it under, `use`.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn activity_lists_every_switch_above_every_enrolment() {
    let launched = made(World::OneTool);
    let (model, told) = started(&launched);
    model.send(Intent::PaneShown {
        pane: Pane::Activity,
    });
    let shown = told.until("the log read", |snapshot| {
        !snapshot.machine.activity.lines.is_empty()
    });
    assert_eq!(
        shown
            .machine
            .activity
            .lines
            .iter()
            .map(|line| line.change.as_str())
            .collect::<Vec<_>>(),
        ["Switch", "Switch", "Enrol", "Enrol"]
    );
    model.shutdown();
}

/// Daily renewal turns on, says how often it runs, and Renew Now renews the one parked login
/// that is due, as PanesAndSettingsTests.swift's testDailyRenewal reads it.
#[test]
#[cfg_attr(windows, ignore = "W25: Task Scheduler")]
fn daily_renewal_turns_on_and_renew_now_says_what_it_did() {
    let launched = made(World::OneTool);
    let (model, told) = started(&launched);
    model.send(Intent::SetSchedule { on: true });
    let on = told.until("the schedule on", |snapshot| {
        snapshot.machine.schedule.on && !snapshot.machine.schedule.changing
    });
    assert_eq!(on.machine.schedule.runs.as_deref(), Some("Every day"));
    model.send(Intent::RenewNow);
    told.until("the renewal said", |snapshot| {
        snapshot.machine.renewal.note == "Renewed one." && !snapshot.machine.renewal.renewing
    });
    model.shutdown();
}

/// A Codex switch says running sessions keep the old account until they are restarted, as
/// AccountsWindowTests.swift's testUsingACodexAccountSaysSessionsKeepTheOldOne reads it.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_codex_switch_says_sessions_keep_the_old_account() {
    let launched = made(World::TwoTools);
    let (model, told) = started(&launched);
    model.send(Intent::SwitchTo {
        qualified: "codex/spare".into(),
    });
    let shown = told.until("what the switch said", |snapshot| {
        snapshot
            .notices
            .iter()
            .any(|notice| notice.id == "switch/codex")
    });
    let notice = shown
        .notices
        .iter()
        .find(|notice| notice.id == "switch/codex")
        .expect("said");
    assert!(
        notice.lines.iter().any(|line| line
            .starts_with("Any codex session started before this switch keeps using main")),
        "{notice:?}"
    );
    model.shutdown();
}

/// Adding accounts as AccountsWindowTests.swift's tests do: Claude Code's sign-in asks for
/// the code and lists the account once it is typed back, Codex's lists it by itself, and one
/// cancelled adds nothing and lets another start.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn accounts_are_added_through_each_tools_sign_in() {
    let launched = made(World::OneTool);
    let (model, told) = started(&launched);
    model.send(Intent::PresentSheet {
        sheet: Sheet::Add { provider: None },
    });
    model.send(Intent::SignIn {
        provider: "claude".into(),
        name: "third".into(),
    });
    let asking = told.until("the code asked for", |snapshot| {
        snapshot
            .signing_in
            .as_ref()
            .is_some_and(|signing| signing.wants_code)
    });
    assert_eq!(
        asking.signing_in.and_then(|signing| signing.url).as_deref(),
        Some(CLAUDE_ADDRESS)
    );
    model.send(Intent::CancelSignIn);
    told.until("the sign-in over", |snapshot| snapshot.signing_in.is_none());
    model.send(Intent::SignIn {
        provider: "claude".into(),
        name: "third".into(),
    });
    told.until("the code asked for", |snapshot| {
        snapshot
            .signing_in
            .as_ref()
            .is_some_and(|signing| signing.wants_code)
    });
    model.send(Intent::PasteCode { code: CODE.into() });
    told.until("the account listed", |snapshot| {
        snapshot.signing_in.is_none()
            && read_in(snapshot)
            && snapshot.status.as_ref().is_some_and(|status| {
                status
                    .accounts
                    .iter()
                    .any(|account| account.qualified.as_deref() == Some("claude/third"))
            })
    });
    model.shutdown();

    let launched = made(World::TwoTools);
    let (model, told) = started(&launched);
    model.send(Intent::SignIn {
        provider: "codex".into(),
        name: "third".into(),
    });
    told.until("the account listed", |snapshot| {
        snapshot.signing_in.is_none()
            && read_in(snapshot)
            && snapshot.status.as_ref().is_some_and(|status| {
                status
                    .accounts
                    .iter()
                    .any(|account| account.qualified.as_deref() == Some("codex/third"))
            })
    });
    model.shutdown();
}

/// Cancel and then Sign In at once, as a person can press one after the other: the new
/// sign-in waits, shown as starting, until the one cancelled before it has stopped and let go
/// of the core's one sign-in at a time, then asks for the code, and nothing on the way is
/// refused as a sign-in already waiting.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_sign_in_asked_for_at_once_after_a_cancel_asks_for_the_code() {
    let launched = made(World::OneTool);
    let (model, told) = started(&launched);
    let sign_in = || Intent::SignIn {
        provider: "claude".into(),
        name: "third".into(),
    };
    let refused =
        |snapshot: &Snapshot| snapshot.failure.is_some() || snapshot.sheet_failure.is_some();
    model.send(Intent::PresentSheet {
        sheet: Sheet::Add { provider: None },
    });
    model.send(sign_in());
    let first = told.until("the code asked for", |snapshot| {
        snapshot
            .signing_in
            .as_ref()
            .is_some_and(|signing| signing.wants_code)
    });
    let first = first.signing_in.expect("the first sign-in").id;
    model.send(Intent::CancelSignIn);
    model.send(sign_in());
    let asking = told.until("the code asked for again, or a refusal", |snapshot| {
        refused(snapshot)
            || snapshot
                .signing_in
                .as_ref()
                .is_some_and(|signing| signing.id != first && signing.wants_code)
    });
    assert_eq!(
        (asking.failure, asking.sheet_failure),
        (None, None),
        "the second sign-in refused"
    );
    model.send(Intent::PasteCode { code: CODE.into() });
    told.until("the account listed", |snapshot| {
        refused(snapshot)
            || (snapshot.signing_in.is_none()
                && read_in(snapshot)
                && snapshot.status.as_ref().is_some_and(|status| {
                    status
                        .accounts
                        .iter()
                        .any(|account| account.qualified.as_deref() == Some("claude/third"))
                }))
    });
    model.shutdown();
}

/// A rename to a name another account has is refused in its sheet, and a free name is
/// taken, as AccountsWindowTests.swift's testRenamingAnAccount reads it; the account in use
/// offers no Forget, and any other is forgotten, as its testTheAccountInUseOffersNoForget
/// and testForgettingAsksFirst read it.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn accounts_are_renamed_and_forgotten_as_the_window_offers() {
    let launched = made(World::OneTool);
    let (model, told) = started(&launched);
    let shown = told.until("the accounts read", read_in);
    // Each row's own menu, by its items' titles, with Forget… last where it is offered.
    let menus: Vec<(String, Vec<String>)> = shown
        .sections
        .iter()
        .flat_map(|section| &section.accounts)
        .map(|item| {
            let titles = item
                .offers
                .iter()
                .chain(&item.forget)
                .map(|offer| offer.title.clone())
                .collect();
            (item.title.clone(), titles)
        })
        .collect();
    let titles = |titles: &[&str]| titles.iter().map(|&title| title.to_owned()).collect();
    assert_eq!(
        menus,
        [
            ("work".into(), titles(&["Sign In Again…", "Rename…"])),
            (
                "personal".into(),
                titles(&["Use personal", "Sign In Again…", "Rename…", "Forget…"])
            ),
        ]
    );

    model.send(Intent::PresentSheet {
        sheet: Sheet::Rename {
            provider: "claude".into(),
            label: "personal".into(),
        },
    });
    model.send(Intent::Rename {
        provider: "claude".into(),
        label: "personal".into(),
        to: "work".into(),
    });
    let refused = told.until("the rename refused", |snapshot| {
        snapshot.sheet_failure.is_some()
    });
    assert_eq!(
        refused
            .sheet_failure
            .map(|failure| failure.title)
            .as_deref(),
        Some("Couldn’t rename personal")
    );
    model.send(Intent::Rename {
        provider: "claude".into(),
        label: "personal".into(),
        to: "home".into(),
    });
    told.until("the account renamed", |snapshot| {
        read_in(snapshot)
            && snapshot.status.as_ref().is_some_and(|status| {
                in_use(status) == ["claude/work"]
                    && status
                        .accounts
                        .iter()
                        .any(|account| account.qualified.as_deref() == Some("claude/home"))
            })
    });
    model.send(Intent::Forget {
        qualified: "claude/home".into(),
    });
    told.until("the account forgotten", |snapshot| {
        read_in(snapshot)
            && snapshot
                .status
                .as_ref()
                .is_some_and(|status| status.accounts.len() == 1)
    });
    model.shutdown();
}

/// A read that fails is said in the menu, as one item that opens the window on the
/// accounts, and in the window, as MenuBarTests.swift's
/// testAFailedReadIsSaidInTheMenuAndInTheWindow reads it in readFailure: the item and the
/// notice are both "Couldn’t read usage". On the real core a read fails where the account
/// index cannot be read, and then so does the read of what is known, so two things that test
/// reads are not there: the notice says the core's own words, that it could not read
/// Pitboard's account list, where the Swift fixture said "Anthropic could not be reached",
/// and no account is listed, where the Swift fixture listed claude/work. The window says it
/// could not read the accounts instead, with Try Again.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_failed_read_is_said_in_the_menu_and_in_the_window() {
    let launched = made(World::ReadFailure);
    let told = Arc::new(Told::default());
    let model = launched.model(Arc::clone(&told) as Arc<dyn ModelListener>, Arc::new(Utc));
    model.send(Intent::Start);
    let shown = told.until("the read failed", |snapshot| {
        snapshot.read_failure.is_some() && !snapshot.reading
    });
    let failure = shown.read_failure.expect("failed");
    assert_eq!(failure.code, "state_unreadable");
    assert!(
        failure
            .message
            .starts_with("could not read Pitboard's account list at "),
        "{failure:?}"
    );

    let item = shown.menu_notices.others.expect("an item");
    assert_eq!(item.title, "Couldn’t read usage");
    assert_eq!(
        item.intent,
        Some(Intent::ShowWindow {
            pane: Some(Pane::Accounts)
        })
    );
    assert_eq!(
        shown
            .notices
            .iter()
            .map(|notice| (notice.id.as_str(), notice.title.as_str()))
            .collect::<Vec<_>>(),
        [("read", "Couldn’t read usage")]
    );
    assert_eq!(
        shown.notices[0].lines,
        std::slice::from_ref(&failure.message)
    );

    assert_eq!(shown.status, None, "nothing is known");
    assert!(shown.sections.is_empty());
    assert_eq!(
        shown.menu_accounts_note.as_deref(),
        Some("No accounts to show")
    );
    assert_eq!(
        shown.accounts_shown,
        AccountsShown::ReadFailed {
            title: "Couldn’t Read Accounts".into(),
            detail: failure.message,
            retry: crate::present::Choice {
                title: "Try Again".into(),
                intent: Intent::Refresh { asked: true },
                enabled: true,
            },
        }
    );
    model.shutdown();
}

/// With Anthropic out of reach, the real core still reads: each account says why its
/// numbers are the last measured, and nothing says a read failed. readFailure was made so
/// before it was given an account index the core cannot read, and the Swift fixture's
/// readFailure failed the read, `unreachable`, which the core's never does; so oneTool is
/// made so here, once its accounts have been read.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn a_service_out_of_reach_leaves_the_last_numbers_and_says_why_on_each_account() {
    let launched = made(World::OneTool);
    out_of_reach(&launched.machine).expect("out of reach");
    let fresh = read(&launched.core);
    assert_eq!(
        fresh
            .accounts
            .iter()
            .map(|account| (account.stale.as_deref(), account.usage.is_some()))
            .collect::<Vec<_>>(),
        [(Some("unreachable"), true), (Some("unreachable"), true)]
    );

    let (model, told) = started(&launched);
    let shown = told.until("the accounts read", read_in);
    let items: Vec<_> = shown
        .sections
        .iter()
        .flat_map(|section| &section.accounts)
        .collect();
    assert_eq!(items.len(), 2);
    for item in items {
        assert_eq!(
            item.stale_note.as_deref(),
            Some("Anthropic could not be reached"),
            "{item:?}"
        );
        assert!(!item.limits.is_empty(), "the last numbers measured");
    }
    assert!(shown.read_failure.is_none());
    assert!(shown.notices.iter().all(|notice| notice.id != "read"));
    model.shutdown();
}

/// An interrupted switch nothing can finish is offered a way out as the app starts, since
/// the read says it is waiting: the window's first notice, with Give Up… and its question,
/// and the menu's one item, which opens the window on the accounts. Giving up says what it
/// kept, and nothing is waiting any more. This is what AccountsWindowTests.swift's
/// testGivingUpOnAnInterruptedSwitch and MenuBarTests.swift's
/// testShowingANoticeOpensTheWindowOnTheAccounts read in stuck. That second test first waits,
/// on This Mac, for a check called "Keychain", which the core's doctor has none of; there it
/// has "interrupted switch", worth looking at, beside personal's parked login.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn an_interrupted_switch_is_offered_a_way_out_as_the_app_starts() {
    let launched = made(World::Stuck);
    let (model, told) = started(&launched);
    let shown = told.until("the accounts read", read_in);
    assert!(shown.stuck);
    let notice = shown.notices.first().expect("a notice");
    assert_eq!(
        (notice.id.as_str(), notice.title.as_str()),
        ("stuck", "An interrupted switch is waiting")
    );
    let give_up = notice.actions.first().expect("an action");
    assert_eq!(give_up.title, "Give Up…");
    assert_eq!(give_up.intent, Intent::AbandonStuckSwitch);
    assert_eq!(
        give_up
            .confirm
            .as_ref()
            .map(|question| question.confirm.as_str()),
        Some("Give Up")
    );
    let item = shown.menu_notices.others.expect("an item");
    assert_eq!(item.title, "An interrupted switch is waiting");
    assert_eq!(
        item.intent,
        Some(Intent::ShowWindow {
            pane: Some(Pane::Accounts)
        })
    );

    model.send(Intent::PaneShown {
        pane: Pane::Machine,
    });
    let checked = told.until("the checks made", |snapshot| {
        !snapshot.machine.checks.lines.is_empty() && !snapshot.machine.checks.checking
    });
    let interrupted = checked
        .machine
        .checks
        .lines
        .iter()
        .find(|line| line.name == "interrupted switch")
        .expect("doctor's check");
    assert_eq!(interrupted.level, crate::Level::Warn);
    assert!(
        checked
            .machine
            .checks
            .lines
            .iter()
            .all(|line| line.name != "Keychain")
    );
    assert_eq!(
        checked.machine.checks.summary.as_deref(),
        Some("2 things are worth looking at."),
        "personal's parked login, and the switch"
    );

    model.send(Intent::AbandonStuckSwitch);
    let given_up = told.until("what giving up kept", |snapshot| {
        snapshot
            .notices
            .iter()
            .any(|notice| notice.id == "abandoned")
    });
    let notice = given_up
        .notices
        .iter()
        .find(|notice| notice.id == "abandoned")
        .expect("said");
    assert_eq!(notice.title, "Gave up on the interrupted switch");
    assert!(
        notice
            .lines
            .iter()
            .any(|line| line.contains("2 logins kept")),
        "{notice:?}"
    );
    told.until("nothing waiting", |snapshot| {
        !snapshot.stuck && snapshot.notices.iter().all(|notice| notice.id != "stuck")
    });
    model.shutdown();
}

/// This Mac shows the core's checks, as PanesAndSettingsTests.swift's
/// testThisMacShowsTheChecks reads them, and they are not the Swift fixture's. That passed a
/// check called "Keychain" and warned that daily renewal was off, which made "One thing is
/// worth looking at.". The core has no check called "Keychain", and says nothing of a
/// schedule that is not there. oneTool's one thing is personal's parked login instead, which
/// is the world's own doing: it lasts two days more, so that Renew Now renews it, where the
/// Swift fixture's lasted eleven. Renew Now finds a parked login due that the read before it
/// left alone only while its refresh token lapses within three days, which is when doctor
/// warns of it, since both ask `doctor::renewal_due`.
///
/// So in PR 10 that test reads what the core's doctor says: the sentence is the same, of
/// personal's parked login rather than of daily renewal, and it waits for one of the core's
/// checks rather than one called "Keychain". work's parked login and personal's are two
/// checks of one code, so the pane lists them by their ids.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn this_mac_shows_the_cores_checks() {
    let launched = made(World::OneTool);
    let (model, told) = started(&launched);
    model.send(Intent::PaneShown {
        pane: Pane::Machine,
    });
    let shown = told.until("the checks made", |snapshot| {
        !snapshot.machine.checks.lines.is_empty() && !snapshot.machine.checks.checking
    });
    let checks = shown.machine.checks;
    assert!(
        checks.lines.iter().all(|line| line.name != "Keychain"),
        "{:#?}",
        checks.lines
    );
    let worth_looking_at: Vec<(&str, &str)> = checks
        .lines
        .iter()
        .filter(|line| line.level != crate::Level::Ok)
        .map(|line| (line.code.as_str(), line.name.as_str()))
        .collect();
    assert_eq!(
        worth_looking_at,
        [("parked_login", "account personal")],
        "{:#?}",
        checks.lines
    );
    // Every account's parked login is a check of its own under one code, and each is a line
    // with its own id, which the pane lists them by.
    let parked: Vec<&str> = checks
        .lines
        .iter()
        .filter(|line| line.code == "parked_login")
        .map(|line| line.name.as_str())
        .collect();
    assert!(parked.len() > 1, "{:#?}", checks.lines);
    let ids: std::collections::BTreeSet<u64> = checks.lines.iter().map(|line| line.id).collect();
    assert_eq!(ids.len(), checks.lines.len(), "{:#?}", checks.lines);
    let personal = checks
        .lines
        .iter()
        .find(|line| line.name == "account personal")
        .expect("personal's");
    assert!(
        personal.detail.starts_with("its parked login expires in "),
        "{personal:?}"
    );
    assert_eq!(
        checks.summary.as_deref(),
        Some("One thing is worth looking at.")
    );
    model.shutdown();
}

// The account windows.

/// The account windows in twoTools, as AccountWindowTests.swift and AccountPickerTests.swift
/// in the UI tests read them: each site's menu offers its own tool's accounts, and spare's
/// own menu in the window opens its chatgpt.com window; a chatgpt.com link shared with the
/// debug build's scheme asks which Codex account opens it, and Open answers once the link has
/// waited; choosing spare opens its window on the link's stand-in, saying how to sign in,
/// with its store recorded in the fixture's own folder; and a link to another site is
/// refused, saying where it is.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn the_account_windows_are_what_their_ui_tests_read() {
    use crate::SiteMenu;
    use crate::account_windows::WindowNoteKind;
    use crate::present::PickerShown;

    let launched = made(World::TwoTools);
    let (model, told) = started(&launched);
    let read = told.until("the accounts read", read_in);
    let spare_row = read
        .sections
        .iter()
        .flat_map(|section| &section.accounts)
        .find(|item| item.qualified.as_deref() == Some("codex/spare"))
        .expect("spare's row");
    let opens: Vec<(&str, &str)> = spare_row
        .windows
        .iter()
        .map(|offer| (offer.title.as_str(), offer.window.label.as_str()))
        .collect();
    assert_eq!(opens, [("Open chatgpt.com", "spare")]);
    let shown = read.account_windows;
    let menus: Vec<(String, Vec<String>)> = shown
        .menus
        .iter()
        .map(|menu| match menu {
            SiteMenu::One { title, window } => (title.clone(), vec![window.label.clone()]),
            SiteMenu::Several { title, windows, .. } => (
                title.clone(),
                windows.iter().map(|window| window.label.clone()).collect(),
            ),
        })
        .collect();
    assert_eq!(
        menus,
        [
            (
                "Open claude.ai".to_owned(),
                vec!["work".to_owned(), "old".into(), "personal".into()]
            ),
            (
                "Open chatgpt.com".to_owned(),
                vec!["main".to_owned(), "spare".into()]
            ),
        ]
    );

    let shared = |link: &str| Intent::LinkArrived {
        text: pitboard_sites::pitboard_link(
            &pitboard_sites::SiteLink::parse(link).expect("a link"),
            super::worlds::LINK_SCHEME,
        ),
    };
    model.send(shared("https://chatgpt.com/c/shared"));
    let picker = told
        .until("Open answering", |last| {
            last.account_windows
                .picker
                .as_ref()
                .is_some_and(|picker| picker.armed)
        })
        .account_windows
        .picker
        .expect("the link");
    let PickerShown::Choose {
        title, accounts, ..
    } = &picker.shown
    else {
        panic!("accounts to choose from: {picker:?}");
    };
    assert_eq!(title, "Open this chatgpt.com link as:");
    let labels: Vec<&str> = accounts.iter().map(|a| a.window.label.as_str()).collect();
    assert_eq!(labels, ["main", "spare"]);
    let spare = accounts[1].window.store.clone();
    model.send(Intent::OpenLink {
        arrival: picker.arrival,
        store: spare.clone(),
    });
    model.send(Intent::WindowOpened {
        store: spare.clone(),
    });
    let opened = told
        .until("spare's window", |last| {
            last.account_windows.picker.is_none()
                && last.account_windows.open.iter().any(|w| w.store == spare)
        })
        .account_windows
        .open
        .into_iter()
        .find(|window| window.store == spare)
        .expect("spare's window");
    assert_eq!(opened.load.url, "pitboard-fixture://chatgpt.com/c/shared");
    assert_eq!(opened.note, Some(WindowNoteKind::SignIn));
    let records = std::fs::read_to_string(launched.machine.root().join("windows.json"))
        .expect("kept in the fixture's own folder");
    assert!(records.contains(&spare), "{records}");

    model.send(Intent::LinkArrived {
        text: "pitboard-debug://open?url=https%3A%2F%2Fexample.com%2Fpage".into(),
    });
    let refused = told.until("the refusal", |last| {
        last.account_windows
            .picker
            .as_ref()
            .is_some_and(|picker| matches!(picker.shown, PickerShown::Refused { .. }))
    });
    let Some(PickerShown::Refused { title, reason }) =
        refused.account_windows.picker.map(|picker| picker.shown)
    else {
        unreachable!()
    };
    assert_eq!(title, "Can’t Open This Link");
    assert!(reason.contains("This link is on example.com."), "{reason}");
    model.shutdown();
}

// The stand-in pages.

/// Each site's stand-in has what its window's tests press and read: its title, the path it
/// was asked for, a link outside it, Google's sign-in, another app's link, a chat, a
/// download, a dialog each way, its sign-in three ways, and an artifact's frame.
#[test]
fn each_site_has_a_stand_in_page_with_what_its_tests_look_for() {
    let page = pages::page("pitboard-fixture://chatgpt.com/c/shared?x=1#y");
    assert!(page.starts_with("<!doctype html>"));
    for part in [
        "<title>chatgpt.com stand-in</title>",
        "<h1>chatgpt.com stand-in</h1>",
        "A Pitboard fixture page at /c/shared. Nothing here reaches the network.",
        "id=\"outside\" href=\"https://example.com/\">A link outside chatgpt.com</a>",
        "href=\"https://accounts.google.com/o/oauth2/v2/auth\">Continue with Google</a>",
        "href=\"vscode://file/x\">Open in an editor</a>",
        "href=\"pitboard-fixture://chatgpt.com/chat/fixture\">A chat</a>",
        "download=\"notes.txt\" href=\"data:text/plain,notes\">Download notes</a>",
        "onclick=\"alert('Saved.')\">Alert</button>",
        "'confirmed' : 'declined'\">Confirm</button>",
        "href=\"pitboard-fixture://appleid.apple.com/sign-in\" target=\"_blank\" rel=\"opener\">Continue with appleid.apple.com</a>",
        "window.open('pitboard-fixture://appleid.apple.com/sign-in', 'sign-in', 'width=480,height=600')",
        "const w = window.open(''); w.location = 'pitboard-fixture://appleid.apple.com/sign-in'",
        "src=\"pitboard-fixture://artifact.fixture/\"",
    ] {
        assert!(page.contains(part), "{part}\n{page}");
    }
    assert!(pages::page("pitboard-fixture://claude.ai/").contains("A Pitboard fixture page at /."));
    assert!(
        pages::page("pitboard-fixture://CLAUDE.AI/new")
            .contains("<title>claude.ai stand-in</title>")
    );
}

/// A host a site's sign-in goes to has a sign-in stand-in whose Done closes its window; the
/// artifact's host has the frame whose link a message clicks; and anything else has nothing.
#[test]
fn every_other_host_has_its_own_stand_in() {
    let sign_in = pages::page("pitboard-fixture://appleid.apple.com/sign-in");
    assert!(sign_in.contains("<title>appleid.apple.com sign-in stand-in</title>"));
    assert!(sign_in.contains("<button id=\"done\" onclick=\"window.close()\">Done</button>"));
    let artifact = pages::page("pitboard-fixture://artifact.fixture/");
    assert!(artifact.contains("<title>Artifact</title>"));
    assert!(artifact.contains("document.getElementById('artifact-download').click()"));
    for nowhere in [
        "pitboard-fixture://example.com/",
        "pitboard-fixture:",
        "not a link",
    ] {
        assert!(
            pages::page(nowhere).contains("<p>Nothing is here.</p>"),
            "{nowhere}"
        );
    }
}

// What every build exports.

/// A fixture's name that is none of them is refused, naming every one there is, before the
/// folder an app's fixture is kept in is touched.
#[test]
fn an_unknown_fixture_is_refused_naming_every_one() {
    let refused =
        PitboardModel::fixture("twoTool".into(), Arc::new(Told::default()), Arc::new(Utc));
    match refused {
        Err(super::FixtureError::Unknown { reason }) => {
            assert!(reason.contains("\"twoTool\""), "{reason}");
            assert!(reason.contains("twoTools, oneTool, empty"), "{reason}");
        }
        other => panic!("{:?}", other.map(|_| ())),
    }
}

/// Forgets the launches into the folder an app's fixture is kept in once a test is done with
/// it, however the test ends.
struct ForgetsTheLaunches;

impl Drop for ForgetsTheLaunches {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("pitboard-fixture"));
    }
}

/// Set in the environment of the child the test of the exported constructor runs itself in,
/// which is how the child knows it is the one to make the fixture.
const EXPORTED_CHILD: &str = "PITBOARD_TEST_EXPORTED_FIXTURE_CHILD";

/// The exported constructor makes a fixture in the folder an app's fixture is kept in,
/// `pitboard-fixture` in the temporary directory, emptying what the last launch left there,
/// and its model, once started, reads its accounts and tells the app's listener.
///
/// That folder is one for every process with the same temporary directory, which is how a
/// debug build launched into a fixture and its UI tests share it. Made there, this test
/// emptied the world of another copy of the suite running on the same machine, as in a
/// stress run of several at once, or of a debug build launched into a fixture meanwhile. So
/// it runs again in a child given a temporary directory of its own, `TMPDIR` as the core and
/// the macOS app both find it, and the child makes the fixture through the constructor an
/// app calls, in the folder it finds there. Setting `TMPDIR` in this process would change
/// the environment while the harness's other threads may read it.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
#[allow(
    clippy::disallowed_methods,
    reason = "the child is told it is the child through its environment"
)]
fn an_exported_fixture_is_started_and_reads_its_accounts() {
    if std::env::var_os(EXPORTED_CHILD).is_some() {
        an_exported_fixture_is_made_in_the_temporary_directory();
    } else {
        in_a_child_with_a_temporary_directory_of_its_own();
    }
}

/// Runs the test of the exported constructor again, alone, in a child whose temporary
/// directory is a folder of this test's own, which goes once the child is done.
fn in_a_child_with_a_temporary_directory_of_its_own() {
    let own = Folder::own("exported").expect("a folder");
    let out = std::process::Command::new(std::env::current_exe().expect("this test's program"))
        .args([
            "fixture::tests::an_exported_fixture_is_started_and_reads_its_accounts",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(EXPORTED_CHILD, "1")
        .env("TMPDIR", own.path())
        .env("TMP", own.path())
        .env("TEMP", own.path())
        .output()
        .expect("this test's program runs");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.status.success(),
        "the child failed, {}:\n{said}",
        out.status
    );
    assert!(said.contains("1 passed"), "the child ran no test:\n{said}");
    assert!(
        !own.path().join("pitboard-fixture").exists(),
        "the child forgot its launch:\n{said}"
    );
}

/// The test itself, in a process whose temporary directory is its own.
fn an_exported_fixture_is_made_in_the_temporary_directory() {
    let _forgets = ForgetsTheLaunches;
    let shared = std::env::temp_dir().join("pitboard-fixture");
    let told = shared.join("home/.pitboard/told.json");
    std::fs::create_dir_all(told.parent().expect("a folder")).expect("a folder");
    std::fs::write(&told, r#"{"claude/work/session/":7200}"#).expect("left behind");

    let listener = Arc::new(Told::default());
    let model = PitboardModel::fixture(
        "oneTool".into(),
        Arc::clone(&listener) as Arc<dyn ModelListener>,
        Arc::new(Utc),
    )
    .expect("a fixture");
    assert!(shared.join("home/.pitboard").is_dir());
    assert!(!told.exists(), "what the last launch told is gone");
    model.send(Intent::Start);
    let shown = listener.until("the accounts read", read_in);
    assert_eq!(
        described(shown.status.as_ref().expect("read")),
        ["claude/work, in use", "claude/personal"]
    );
    model.shutdown();
}

/// The exported constructor given a temporary directory makes its fixture in the folder
/// `pitboard-fixture` there, emptying what the last launch left there, as `fixture` does in
/// the process's own, and its model, once started, reads its accounts and tells the app's
/// listener. The C# and Swift tests of the bindings launch with it into a directory of their
/// own, so that neither empties the world of a debug build launched into a fixture, or of
/// another run of the same tests, as they did launching with `fixture`.
#[test]
#[cfg_attr(
    windows,
    ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
)]
fn an_exported_fixture_in_a_directory_of_its_own_is_made_there() {
    let own = Folder::own("exported-in").expect("a folder");
    let made = own.path().join("pitboard-fixture");
    let told = made.join("home/.pitboard/told.json");
    std::fs::create_dir_all(told.parent().expect("a folder")).expect("a folder");
    std::fs::write(&told, r#"{"claude/work/session/":7200}"#).expect("left behind");

    let listener = Arc::new(Told::default());
    let model = PitboardModel::fixture_in(
        "oneTool".into(),
        own.path().to_string_lossy().into_owned(),
        Arc::clone(&listener) as Arc<dyn ModelListener>,
        Arc::new(Utc),
    )
    .expect("a fixture");
    assert!(made.join("home/.pitboard").is_dir());
    assert!(!told.exists(), "what the last launch told is gone");
    model.send(Intent::Start);
    let shown = listener.until("the accounts read", read_in);
    assert_eq!(
        described(shown.status.as_ref().expect("read")),
        ["claude/work, in use", "claude/personal"]
    );
    model.shutdown();
    assert!(made.is_dir(), "left there, as an app's is");
}
