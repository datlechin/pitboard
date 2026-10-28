//! The fixtures, each made on the real core: where each starts, the tools and apps it stands
//! in for, and what each UI test reads in its world, asked of the model an app launches into
//! it. Where the real core answers otherwise than the Swift fixture did, the test says so and
//! holds what the core does.

use super::apps::{CHATGPT, CLAUDE, chatgpt};
use super::pages;
use super::tools::{CLAUDE_ADDRESS, CLAUDE_REFUSES, CODEX_ADDRESS};
use super::worlds::{Folder, Launched, World, make, out_of_reach};
use crate::AccountKey;
use crate::Front;
use crate::Tool;
use crate::model::testing::platform::{PATIENCE, Posted, StandInApps, Told, eventually};
use crate::model::testing::records::{mac_app, read_in};
use crate::model::{
    Doing, Intent, Listener, LocalTime, Notice, NoticeButton, NoticeButtonKind, Notifier,
    OtherApps, Pane, PitboardModel, Place, PlatformError, Sheet, Snapshot, Tip,
};
use crate::present::testing::Utc;
use crate::present::words;
use crate::present::{AccountItem, AccountsShown, ActionKind, Footing, RowEnd};
use crate::{AppCore, Enrolled, EnrolledAs, FoundCommandLine, PitboardError, Schedule, Status};
use pitboard_core::host::{OS, Os};
use pitboard_core::provider::{self, ProviderId};
use pitboard_core::state::Key;
use pitboard_core::status::Stale;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A code as Claude Code's browser page shows one, `<code>#<state>`, which its sign-in takes:
/// what a UI test types.
const CODE: &str = "fixture-code#state";

// The macOS app's, whichever system runs the tests.
const FRONT: Front = Front::MacMenuBar;

/// `world`, made in a folder of this test's own.
fn made(world: World) -> Launched {
    make(world, Folder::own(world.name()).expect("a folder"), FRONT)
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
                .key
                .as_ref()
                .map_or_else(|| account.email.clone(), Key::qualified);
            if account.signed_in {
                format!("{name}, in use")
            } else if account.standing.switchable() {
                name
            } else {
                format!(
                    "{name}, {}",
                    account.stale.map_or("not switchable", Stale::code)
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
        .filter_map(|account| Some((account.key.as_ref()?.qualified(), account.signed_in)))
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

// Where each fixture starts.

/// The UI tests launch the app into these fixtures and assert on what each starts with, so
/// a change here is a change to what they test: who a read shows, or the code it fails
/// with, the codes it warns with, who the offline read shows, and which tools were found.
/// readFailure's reads both fail, as the core's do where its account index cannot be read,
/// so there is nothing known to fall back on. stuck's read warns that an interrupted switch
/// is waiting, as the Swift fixture's has since the core's read said so, and its offline
/// read says nothing of it, since only Anthropic could say whose Claude Code's login is.
/// leftInFile's read warns of the login left in a file, which a read that asks nobody does
/// not look at.
#[test]
fn each_fixture_starts_where_its_tests_expect() {
    let work = "claude/work, in use";
    for world in World::ALL {
        let desktop = matches!(world, World::ClaudeDesktop | World::ClaudeDesktopOpen);
        if desktop && !ProviderId::Desktop.supported_on(OS) {
            continue;
        }
        let expected: Vec<&str> = match world {
            World::ClaudeDesktop | World::ClaudeDesktopOpen => vec![
                work,
                "claude/personal",
                "desktop/work, in use",
                "desktop/home",
            ],
            World::TwoTools | World::ChatGptOpen => vec![
                work,
                "claude/old, parked_access_expired",
                "claude/personal",
                "codex/main, in use",
                "codex/spare",
            ],
            World::OneTool
            | World::Stuck
            | World::WarmUp
            | World::LeftInFile
            | World::SwitchedByItself => {
                vec![work, "claude/personal"]
            }
            World::LoginReplaced => vec!["claude/personal, in use", "claude/work, login_replaced"],
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
            let expected: &[&str] = match world {
                World::Stuck => &["recovery_undetermined"],
                World::LeftInFile => &["fallback_login"],
                World::LoginReplaced => &["login_replaced"],
                _ => &[],
            };
            assert_eq!(warned, expected, "{}", world.name());
            assert!(
                fresh
                    .accounts
                    .iter()
                    .all(|account| account.stale != Some(Stale::Unreachable)),
                "{}",
                world.name()
            );
            let offline = core.status_offline().expect("what is known");
            assert_eq!(who(&offline), who(&fresh), "{}", world.name());
            let standing: Vec<&str> = offline
                .warnings
                .iter()
                .map(|warning| warning.code.as_str())
                .collect();
            // What Pitboard recorded is said by a read that asks nobody too.
            let expected: &[&str] = match world {
                World::LoginReplaced => &["login_replaced"],
                _ => &[],
            };
            assert_eq!(standing, expected, "{}", world.name());
        }
        let listed: &[&str] = match OS {
            Os::MacOs => &["claude", "codex", "desktop"],
            Os::Linux | Os::Windows => &["claude", "codex"],
        };
        assert_eq!(
            crate::tools_on(OS)
                .into_iter()
                .map(ProviderId::code)
                .collect::<Vec<_>>(),
            listed
        );
        let installed: Vec<&str> = core.installed().into_iter().map(ProviderId::code).collect();
        let expected: &[&str] = match world {
            World::NoClaudeCode => &[],
            World::ClaudeDesktop | World::ClaudeDesktopOpen => &["claude", "desktop"],
            _ => &["claude", "codex"],
        };
        assert_eq!(installed, expected, "{}", world.name());
        assert_eq!(
            launched.apps.is_running(CLAUDE),
            world == World::ClaudeDesktopOpen,
            "{}",
            world.name()
        );
    }
}

/// The UI tests launch a fixture by its name, from a list of their own, so a name changed
/// here has to change there.
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
            "claudeDesktop",
            "claudeDesktopOpen",
            "warmUp",
            "leftInFile",
            "loginReplaced",
            "switchedByItself",
        ]
    );
}

/// Work is at 97% of its five-hour limit and personal has room, so the app switches Claude
/// Code to personal by itself as it starts, and its notice offers Switch Back.
#[test]
fn an_account_at_its_limit_is_switched_away_from_as_the_app_starts() {
    let launched = made(World::SwitchedByItself);
    let (model, told) = started(&launched);
    let switched = told.last_where("personal in use, with Switch Back", |snapshot| {
        uses(snapshot, "claude/personal")
            && snapshot.notices.iter().any(|notice| {
                notice
                    .actions
                    .iter()
                    .any(|action| matches!(action.intent, Intent::SwitchBack { .. }))
            })
    });
    let notice = switched
        .notices
        .iter()
        .find(|notice| notice.id == "switch/claude")
        .expect("the switch's notice");
    assert_eq!(notice.title, words::switched_to(None, "personal"));
    assert_eq!(notice.actions[0].title, words::switch_back(FRONT));
    model.shutdown();
}

/// An account turned on for warm-ups whose five-hour period is not running is warmed up as
/// the app starts, through the core's own warm-up with the run played: its row then says when
/// it warms up again, and the activity log has the warm-up.
#[test]
fn an_account_turned_on_is_warmed_up_as_the_app_starts() {
    let launched = made(World::WarmUp);
    let (model, told) = started(&launched);
    let personal = |shown: &Snapshot| {
        shown
            .sections
            .iter()
            .flat_map(|section| &section.accounts)
            .find(|item| item.title == "personal")
            .and_then(|item| item.detail.warm.as_ref().map(|warm| warm.said.clone()))
    };
    // What the row says while a period runs, up to the clock time it says.
    let running = words::typeset(&pitboard_core::words::capitalised(
        &pitboard_core::words::warm_standing(
            &pitboard_core::duty::warm::Standing::Running {
                until: 0,
                next: Some(0),
            },
            &|_| String::new(),
        ),
    ));
    let shown = told.last_where("personal warmed up", |shown| {
        personal(shown).is_some_and(|note| note.starts_with(&running))
    });
    let warm = &shown.machine.warm;
    assert_eq!(
        Some(warm.range_label.as_str()),
        words::warm_hours(0, 24 * 60, |_| None).as_deref()
    );
    assert!(
        warm.accounts
            .iter()
            .any(|account| account.title == "personal" && account.on)
    );
    // The row says it runs as soon as the core's files do, which a look the warm-up's own
    // readings wake can show before the warm-up has ended and been logged.
    eventually("the warm-up logged", || {
        launched.core.log(20).iter().any(|change| {
            change.verb == "warm-up" && change.subject == "personal" && change.outcome == "ok"
        })
    });
    model.shutdown();
}

// Signing in.

/// Claude Code's sign-in prints the address to open and asks for the code from the browser,
/// which the sheet shows as a link and a field, and goes no further until a code is typed
/// back. Finishing then parks the new account beside the one in use.
#[test]
fn a_claude_code_sign_in_waits_for_the_code_and_then_parks_the_account() {
    let launched = made(World::OneTool);
    let core = &launched.core;
    let session = core.sign_in(ProviderId::Claude, None).expect("started");
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

    let (line, said) = std::sync::mpsc::channel();
    let reading = {
        let session = Arc::clone(&session);
        std::thread::spawn(move || {
            let _ = line.send(session.next_line());
        })
    };
    assert_eq!(
        said.recv_timeout(Duration::from_millis(300)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout),
        "nothing more until the code is typed"
    );
    session.paste(CODE).expect("typed back");
    assert_eq!(said.recv_timeout(PATIENCE), Ok(None));
    reading.join().expect("the read");

    assert_eq!(
        session.finish().expect("enrolled"),
        Enrolled {
            email: "dana@travel.example".into(),
            label: "travel".into(),
            added: true,
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
fn claude_code_refuses_a_code_without_both_halves_and_reads_another() {
    let launched = made(World::OneTool);
    let core = &launched.core;
    let session = core.sign_in(ProviderId::Claude, None).expect("started");
    session.paste("fixture-code").expect("typed back");
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
        session.paste(refused).expect("typed back");
        assert_eq!(
            session.next_line().as_deref(),
            Some(CLAUDE_REFUSES),
            "{refused:?}"
        );
    }
    session
        .paste(" fixture-code#any state at all ")
        .expect("typed back");
    assert_eq!(session.next_line(), None);
    assert_eq!(
        session.finish().expect("enrolled").email,
        "dana@travel.example"
    );
}

/// A code Claude Code refused is said in the sheet, in the sentence the owner approved on 6
/// October 2026, and the field is offered again for the same sign-in, which takes the next
/// code: what AccountsWindowTests.swift's tests that add a Claude Code account meet in this
/// world, where they type `fixture-code` with no state, until they type a code with its `#`.
#[test]
fn a_refused_code_is_said_in_the_sheet_and_another_is_asked_for() {
    let launched = made(World::OneTool);
    let (model, told) = started(&launched);
    model.send(Intent::PresentSheet {
        sheet: Sheet::Add { tool: None },
    });
    model.send(Intent::SignIn {
        tool: Tool::Claude,
        again: None,
    });
    told.last_where("the code asked for", |snapshot| {
        snapshot
            .signing_in
            .as_ref()
            .is_some_and(|signing| signing.wants_code)
    });
    model.send(Intent::PasteCode {
        code: "fixture-code".into(),
    });
    let refused = told.last_where("the code refused", |snapshot| {
        snapshot
            .signing_in
            .as_ref()
            .is_some_and(|signing| signing.wants_code)
            && snapshot
                .signing_in_text
                .as_ref()
                .is_some_and(|text| text.refused.is_some())
    });
    assert_eq!(
        refused.signing_in_text.and_then(|text| text.refused),
        Some(words::code_refused("Claude Code"))
    );
    model.send(Intent::PasteCode { code: CODE.into() });
    told.last_where("the account listed", |snapshot| {
        snapshot.signing_in.is_none() && read_in(snapshot) && lists(snapshot, "claude/travel")
    });
    model.shutdown();
}

/// Codex's sign-in prints the address to open beside the loopback address the browser comes
/// back to, reads nothing typed, and finishes by itself once the browser is done.
#[test]
fn a_codex_sign_in_finishes_by_itself() {
    let launched = made(World::TwoTools);
    let core = &launched.core;
    let session = core.sign_in(ProviderId::Codex, None).expect("started");
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
            email: "dana@travel.example".into(),
            label: "travel".into(),
            added: true,
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

// The rest of the machine.

/// Daily renewal can be turned on and off, and taking away a schedule that is not there
/// says there was nothing to take away. Nothing is ever repaired. The schedule is a file in
/// the fixture's own folder, which no scheduler is asked to start: on Windows, the task's
/// definition as the pretend Task Scheduler keeps it.
#[test]
fn the_schedule_turns_on_and_off() {
    let launched = made(World::OneTool);
    let core = &launched.core;
    assert_eq!(core.schedule(), Some(Schedule::Absent));
    assert_eq!(core.schedule_uninstall().ok(), Some(false));

    let path = core.schedule_install().expect("installed");
    let kept = match OS {
        Os::MacOs | Os::Linux => std::path::PathBuf::from(&path),
        Os::Windows => {
            let task = path
                .strip_prefix(r"\Pitboard\")
                .expect("in Pitboard's folder");
            assert!(task.starts_with("Renew S-1-5-"), "{path}");
            launched
                .machine
                .root()
                .join("home")
                .join("Tasks")
                .join("Pitboard")
                .join(task)
        }
    };
    assert!(
        kept.starts_with(launched.machine.root()) && kept.is_file(),
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
/// The login item and notifications' permission are the app's.
#[test]
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
        core.switch_to(&Key::new(ProviderId::Claude, "personal"))
            .ok();
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
#[test]
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

fn rows(snapshot: &Snapshot) -> impl Iterator<Item = &AccountItem> {
    snapshot
        .sections
        .iter()
        .flat_map(|section| &section.accounts)
}

/// The accounts a snapshot lists by their names with their tools, and whether each is in
/// use, as `who` has a read's.
fn listed(snapshot: &Snapshot) -> Vec<(String, bool)> {
    rows(snapshot)
        .filter_map(|row| Some((row.qualified.clone()?, row.in_use)))
        .collect()
}

fn lists(snapshot: &Snapshot, qualified: &str) -> bool {
    rows(snapshot).any(|row| row.qualified.as_deref() == Some(qualified))
}

/// The accounts in use a snapshot shows, with no switch under way.
fn shows_in_use(snapshot: &Snapshot) -> Vec<String> {
    if rows(snapshot).any(AccountItem::switching) {
        return Vec::new();
    }
    listed(snapshot)
        .into_iter()
        .filter_map(|(qualified, in_use)| in_use.then_some(qualified))
        .collect()
}

/// The app's model over `launched`, started, once the accounts have been read.
fn started(launched: &Launched) -> (Arc<PitboardModel>, Arc<Told>) {
    let told = Arc::new(Told::default());
    let model = launched.model(Arc::clone(&told) as Arc<dyn Listener>, Arc::new(Utc));
    model.send(Intent::Start);
    told.last_where("the accounts read", read_in);
    (model, told)
}

/// The UI tests open the menu before the window, and the menu shows only what a read has
/// found. The app launched into a fixture reads by itself, as it does on a real machine,
/// with nothing opened and nothing pressed.
#[test]
fn a_launch_reads_without_being_asked() {
    let launched = made(World::TwoTools);
    let (model, told) = started(&launched);
    let last = told.last_where("the accounts read", read_in);
    assert_eq!(listed(&last), who(&read(&launched.core)));
    model.shutdown();
}

// On Windows by its package, so the quit question names the AUMID the Windows app's
// OtherApps takes.
#[test]
fn the_chatgpt_fixture_is_the_app_the_core_finds_on_each_system() {
    use crate::launch::Remedy;
    let launched = made(World::TwoTools);
    for os in [Os::MacOs, Os::Windows] {
        let chatgpt = chatgpt(os);
        chatgpt.opens(&launched.machine.host);
        let held: Vec<(String, Remedy)> = launched
            .core
            .holding(ProviderId::Codex)
            .into_iter()
            .map(|held| (held.kind, held.remedy))
            .collect();
        assert_eq!(
            held,
            [(
                "chatgpt_app".to_owned(),
                Remedy::ReopenApp {
                    app: chatgpt.app_id(),
                    name: "ChatGPT".into(),
                }
            )],
            "{os:?}"
        );
    }
    assert_eq!(CHATGPT, chatgpt(OS).app);
    assert_eq!(chatgpt(Os::Windows).app, "OpenAI.Codex_2p2nqsd0c76g0!App");
}

/// The fixture's ChatGPT runs Codex's login while it is open, as the process list shows it,
/// and its app control quits and opens it: a switch through the fixture quits it, switches
/// and opens it again, which is what the UI test drives.
#[test]
fn the_chatgpt_fixture_is_quit_for_a_codex_switch_and_opened_again() {
    let launched = made(World::ChatGptOpen);
    let core = &launched.core;
    assert_eq!(
        core.holding(ProviderId::Codex)
            .iter()
            .map(|held| held.kind.as_str())
            .collect::<Vec<_>>(),
        ["chatgpt_app"]
    );
    assert!(core.holding(ProviderId::Claude).is_empty());

    let (model, told) = started(&launched);
    model.send(Intent::SwitchTo {
        account: AccountKey::new(Tool::Codex, "spare"),
    });
    let asked = told.last_where("the quit question", |snapshot| {
        snapshot.quit_question.is_some()
    });
    let question = asked.quit_question.expect("asked");
    assert_eq!(question.question.title, words::quit_to_switch("ChatGPT"));
    model.send(question.confirmed);
    told.last_where("the switch, read back", |snapshot| {
        shows_in_use(snapshot).contains(&"codex/spare".into())
    });
    assert_eq!(
        launched.apps.asked(),
        [format!("quit {CHATGPT}"), format!("open {CHATGPT}")]
    );
    assert!(launched.apps.is_running(CHATGPT));
    model.shutdown();
}

#[test]
fn claude_is_quit_for_a_switch_and_opened_again_behind() {
    // windows: Claude Desktop is macOS only
    if !ProviderId::Desktop.supported_on(OS) {
        return;
    }
    let launched = made(World::ClaudeDesktopOpen);
    let core = &launched.core;
    assert_eq!(
        core.holding(ProviderId::Desktop)
            .iter()
            .map(|held| held.kind.as_str())
            .collect::<Vec<_>>(),
        ["claude_desktop"]
    );
    let (model, told) = started(&launched);
    model.send(Intent::SwitchTo {
        account: AccountKey::new(Tool::Desktop, "home"),
    });
    let asked = told.last_where("the quit question", |snapshot| {
        snapshot.quit_question.is_some()
    });
    let question = asked.quit_question.expect("asked");
    assert_eq!(question.question.title, words::quit_to_switch("Claude"));
    model.send(question.confirmed);
    told.last_where("the switch, read back", |snapshot| {
        shows_in_use(snapshot).contains(&"desktop/home".into())
    });
    model.shutdown();
    assert_eq!(
        launched.apps.asked(),
        [format!("quit {CLAUDE}"), format!("open {CLAUDE}")]
    );
    assert!(launched.apps.is_running(CLAUDE));
}

#[test]
fn an_add_in_claude_desktop_parks_waits_and_enrols_with_claude_in_front() {
    use pitboard_core::switch::WaitStep;
    // windows: Claude Desktop is macOS only
    if !ProviderId::Desktop.supported_on(OS) {
        return;
    }
    // The steps, through the core's own wait, with the fixture's Claude played by hand.
    let launched = made(World::ClaudeDesktop);
    let (core, claude) = (&launched.core, &launched.machine.claude);
    let session = core
        .sign_in(ProviderId::Desktop, None)
        .expect("work parked");
    assert_eq!(
        session.put_back,
        Some(Key::new(ProviderId::Desktop, "work"))
    );
    let reading = {
        let session = Arc::clone(&session);
        std::thread::spawn(move || {
            let mut said = Vec::new();
            while let Some(line) = session.next_line() {
                said.push(line);
            }
            said
        })
    };
    let until = |step: WaitStep| {
        let started = Instant::now();
        while session.step() != Some(step) {
            assert!(started.elapsed() < PATIENCE, "never {step:?}");
            std::thread::sleep(Duration::from_millis(2));
        }
    };
    until(WaitStep::Waiting);
    claude.opens();
    until(WaitStep::Open);
    claude.quits();
    until(WaitStep::QuitSignedOut);
    claude.opens();
    until(WaitStep::Open);
    claude.signs_in(&pitboard_core::testing::desktop::TRAVEL);
    claude.reports();
    claude.quits();
    let said = reading.join().expect("everything the wait said");
    assert!(
        said.iter()
            .any(|line| line.starts_with("Claude quit with nobody signed in")),
        "{said:?}"
    );
    let enrolled = session.finish().expect("travel enrolled");
    assert_eq!(enrolled.outcome, EnrolledAs::InUse { again: false });
    assert_eq!(
        enrolled.label, "desktop",
        "Claude Desktop's accounts have no email"
    );

    // The same add from the app, over a Claude that is open.
    let launched = made(World::ClaudeDesktopOpen);
    let (model, told) = started(&launched);
    model.send(Intent::PresentSheet {
        sheet: Sheet::Add {
            tool: Some(Tool::Desktop),
        },
    });
    model.send(Intent::SignIn {
        tool: Tool::Desktop,
        again: None,
    });
    let asked = told.last_where("the quit question", |snapshot| {
        snapshot.quit_question.is_some()
    });
    let question = asked.quit_question.expect("asked");
    assert_eq!(
        question.question.confirm,
        words::quit_and_continue(FRONT, "Claude")
    );
    model.send(question.confirmed);
    let waiting = told.last_where("the wait, with Claude opened for it", |snapshot| {
        snapshot
            .signing_in_text
            .as_ref()
            .is_some_and(|text| text.steps == words::in_app_steps("Claude", None, true))
    });
    let text = waiting.signing_in_text.expect("the page");
    assert_eq!(text.waiting, words::waiting_to_quit("Claude"));
    assert_eq!(
        text.undo.map(|undo| undo.title),
        Some(words::switch_back_to(FRONT, "work"))
    );
    told.last_where("travel added, in use and named next", |snapshot| {
        snapshot.signing_in.is_none()
            && matches!(snapshot.sheet, Some(Sheet::Name { .. }))
            && shows_in_use(snapshot).contains(&"desktop/desktop".into())
    });
    model.shutdown();
    assert_eq!(
        launched.apps.asked(),
        [format!("quit {CLAUDE}"), format!("open-front {CLAUDE}")]
    );
    assert!(!launched.apps.is_running(CLAUDE), "the person quit it");
}

#[test]
fn an_add_left_halfway_is_put_back_by_use() {
    // windows: Claude Desktop is macOS only
    if !ProviderId::Desktop.supported_on(OS) {
        return;
    }
    let launched = made(World::ClaudeDesktop);
    let (model, told) = started(&launched);
    model.send(Intent::PresentSheet {
        sheet: Sheet::Add {
            tool: Some(Tool::Desktop),
        },
    });
    model.send(Intent::SignIn {
        tool: Tool::Desktop,
        again: None,
    });
    let waiting = told.last_where("work parked for the sign-in", |snapshot| {
        snapshot
            .signing_in_text
            .as_ref()
            .is_some_and(|text| text.undo.is_some())
    });
    let text = waiting.signing_in_text.expect("the page");
    assert_eq!(text.message, words::in_app_note("Claude", None));
    assert_eq!(text.steps, words::in_app_steps("Claude", None, false));
    assert_eq!(text.cancel.title, words::LATER);
    assert!(!text.escape_cancels);
    model.send(Intent::CancelSignIn);
    let left = told.last_where("the add left halfway", |snapshot| {
        snapshot.signing_in.is_none()
            && snapshot
                .notices
                .iter()
                .any(|notice| notice.id == "awaiting/desktop")
    });
    let notice = left
        .notices
        .iter()
        .find(|notice| notice.id == "awaiting/desktop")
        .expect("its notice");
    assert_eq!(
        notice
            .actions
            .iter()
            .map(|action| action.title.clone())
            .collect::<Vec<_>>(),
        [
            words::finish(FRONT, false),
            words::switch_back_to(FRONT, "work")
        ]
    );
    model.send(notice.actions[1].intent.clone());
    told.last_where("work back in use", |snapshot| {
        !snapshot
            .notices
            .iter()
            .any(|notice| notice.id == "awaiting/desktop")
            && shows_in_use(snapshot).contains(&"desktop/work".into())
    });
    model.shutdown();
    assert_eq!(launched.apps.asked(), Vec::<String>::new());
}

#[test]
fn a_claude_pitboard_quits_before_anybody_signs_in_writes_nothing_after() {
    use crate::model::OtherApps;
    // windows: Claude Desktop is macOS only
    if !ProviderId::Desktop.supported_on(OS) {
        return;
    }
    let launched = made(World::ClaudeDesktopOpen);
    let apps = &launched.apps;
    let copy = apps.running(mac_app(CLAUDE)).expect("asked").expect("open");
    apps.request_quit(mac_app(CLAUDE)).expect("asked to quit");
    let session = launched
        .core
        .sign_in(ProviderId::Desktop, None)
        .expect("work parked");
    apps.reopen(copy, true).expect("opened for the sign-in");
    apps.request_quit(mac_app(CLAUDE)).expect("asked to quit");
    apps.played();
    assert!(!launched.machine.claude.holds_a_session());
    assert!(!launched.apps.is_running(CLAUDE));
    session.cancel();
}

// The actions of the notice whose id starts `id`, by title.
fn actions_of(snapshot: &Snapshot, id: &str) -> Vec<String> {
    snapshot
        .notices
        .iter()
        .find(|notice| notice.id.starts_with(id))
        .map(|notice| {
            (notice.actions.iter())
                .map(|action| action.title.clone())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_stopped_move_is_given_up_from_the_app() {
    // windows: Claude Desktop is macOS only
    if !ProviderId::Desktop.supported_on(OS) {
        return;
    }
    let launched = made(World::ClaudeDesktop);
    let machine = &launched.machine;
    pitboard_core::testing::desktop::leave_a_move_waiting(
        &machine.terminal(),
        &machine.host,
        &Key::new(ProviderId::Desktop, "work"),
    );
    // Opened, somebody signed in to a third account, quit: recovery cannot tell.
    machine.claude.opens();
    machine
        .claude
        .signs_in(&pitboard_core::testing::desktop::TRAVEL);
    machine.claude.reports();
    machine.claude.quits();
    let (model, told) = started(&launched);
    let waiting = told.last_where("the move said waiting", |snapshot| {
        !actions_of(snapshot, "warning/move_waiting/").is_empty()
    });
    assert_eq!(
        actions_of(&waiting, "warning/move_waiting/"),
        [words::add_account_shown(FRONT, "Claude", true)],
        "no change has tried to settle it yet"
    );
    model.send(Intent::SwitchTo {
        account: AccountKey::new(Tool::Desktop, "home"),
    });
    let refused = told.last_where("the switch refused", |snapshot| {
        actions_of(snapshot, "warning/move_waiting/").len() == 2
    });
    let undetermined = pitboard_core::error::Error::RecoveryUndetermined {
        tool: ProviderId::Desktop,
        from: "desktop/work".into(),
        to: "desktop/work".into(),
        detail: "Claude's folder holds a session that is not the one moving".into(),
        spelling: pitboard_core::words::Spelling::Bare,
    };
    assert_eq!(
        refused.failure_alert,
        Some(crate::account_windows::AlertText {
            title: words::could_not(&Doing::Switch {
                account: Key::new(ProviderId::Desktop, "home")
            }),
            message: words::in_app_refusal(FRONT, &undetermined).expect("the app's words"),
            next: Some(crate::present::next::of_failure(
                FRONT,
                Some("recovery_undetermined"),
                Some(&Key::new(ProviderId::Desktop, "work"))
            )),
        })
    );
    let notice = (refused.notices.iter())
        .find(|notice| notice.id.starts_with("warning/move_waiting/"))
        .expect("its notice");
    assert_eq!(
        actions_of(&refused, "warning/move_waiting/"),
        [
            words::add_account_shown(FRONT, "Claude", true),
            words::cancel_switch(FRONT, true)
        ]
    );
    model.send(notice.actions[1].intent.clone());
    let gave_up = told.last_where("the move given up", |snapshot| {
        snapshot
            .notices
            .iter()
            .any(|notice| notice.id == "moves_given_up")
    });
    model.shutdown();
    let notice = (gave_up.notices.iter())
        .find(|notice| notice.id == "moves_given_up")
        .expect("its notice");
    assert_eq!(notice.title, words::MOVE_CANCELLED);
    assert_eq!(
        notice.lines,
        [words::in_app_given_up(&pitboard_core::switch::GivenUp {
            tool: ProviderId::Desktop,
            label: "desktop/work".into(),
            parked: 1,
            set_aside: 0,
            left: Vec::new(),
            slot: None,
        })]
    );
    assert!(!launched.core.moving(ProviderId::Desktop), "nothing waits");
}

#[test]
fn a_move_the_give_up_settles_is_said_as_finished() {
    // windows: Claude Desktop is macOS only
    if !ProviderId::Desktop.supported_on(OS) {
        return;
    }
    let launched = made(World::ClaudeDesktop);
    let machine = &launched.machine;
    pitboard_core::testing::desktop::leave_a_move_waiting(
        &machine.terminal(),
        &machine.host,
        &Key::new(ProviderId::Desktop, "work"),
    );
    let (model, told) = started(&launched);
    told.last_where("the move said waiting", |snapshot| {
        !actions_of(snapshot, "warning/move_waiting/").is_empty()
    });
    // As the stuck notice's Give Up… asks it: recovery finishes this move.
    model.send(Intent::AbandonStuckSwitch);
    let settled = told.last_where("the move said finished", |snapshot| {
        (snapshot.notices.iter())
            .any(|notice| notice.id.starts_with("warning/interrupted_move_finished/"))
    });
    model.shutdown();
    assert!(
        !settled
            .notices
            .iter()
            .any(|notice| notice.id == "moves_given_up"),
        "nothing given up"
    );
    assert!(!launched.core.moving(ProviderId::Desktop), "nothing waits");
}

/// The fixture's ChatGPT leaves the process list as it quits and comes back as it is
/// opened again, so the core sees it go and come back as it would on a real machine.
#[test]
fn chatgpt_leaves_the_process_list_as_it_quits() {
    use crate::model::OtherApps;
    let launched = made(World::ChatGptOpen);
    let (apps, core) = (&launched.apps, &launched.core);
    let copy = apps
        .running(chatgpt(OS).app_id())
        .expect("asked")
        .expect("running");
    apps.request_quit(chatgpt(OS).app_id())
        .expect("asked to quit");
    assert_eq!(apps.running(chatgpt(OS).app_id()).expect("asked"), None);
    assert!(core.holding(ProviderId::Codex).is_empty());
    apps.reopen(copy, false).expect("opened");
    assert_eq!(core.holding(ProviderId::Codex).len(), 1);
    assert!(
        !made(World::TwoTools).apps.is_running(CHATGPT),
        "only chatGPTOpen has it open"
    );
}

// What each UI test reads in its world.

/// Every account is an item under its tool, as MenuBarTests.swift's
/// testTheMenuListsEveryAccountUnderItsTool reads them.
#[test]
fn the_menu_lists_every_account_under_its_tool() {
    let launched = made(World::TwoTools);
    let (model, told) = started(&launched);
    let shown = told.last_where("the accounts read", read_in);
    let sections: Vec<(Option<String>, Vec<String>)> = shown
        .sections
        .iter()
        .map(|section| {
            (
                section.heading.clone(),
                section
                    .accounts
                    .iter()
                    .map(|item| item.menu_title.clone())
                    .collect(),
            )
        })
        .collect();
    let named = |title, aside| words::named_in_menu(FRONT, title, aside);
    assert_eq!(
        sections,
        [
            (
                Some("Claude Code".into()),
                vec![
                    named("work", Some("42%")),
                    named("old", Some("sign-in expired")),
                    named("personal", Some("100%"))
                ]
            ),
            (
                Some("Codex".into()),
                vec![named("main", Some("7%")), named("spare", Some("0%"))]
            ),
        ]
    );
    let codex: Vec<&str> = (shown.sections[1].accounts.iter())
        .map(|item| item.summary.as_str())
        .collect();
    let weekly = |figure: &str| {
        words::limits_line(
            FRONT,
            &[(words::limit_short("weekly", None), figure.to_owned())],
        )
    };
    assert_eq!(codex, [weekly("7%"), weekly("0%")], "a Pro plan's limits");
    model.shutdown();
}

/// A machine without Claude Code says so first, as MenuBarTests.swift's
/// testAMachineWithoutClaudeCodeSaysSo reads it, and opens no window by itself.
#[test]
fn a_machine_without_claude_code_says_so_first() {
    let launched = made(World::NoClaudeCode);
    let (model, told) = started(&launched);
    let shown = told.last_where("the accounts read", read_in);
    assert_eq!(shown.footing, Footing::NoClaudeCode);
    assert_eq!(
        shown
            .menu_notices
            .install
            .as_ref()
            .map(|item| item.title.as_str()),
        Some(words::claude_code_not_installed(FRONT).as_str())
    );
    assert_eq!(shown.window_request.serial, 0);
    model.shutdown();
}

// As testAnEmptyMachineOffersAnAccount and testTheFirstLaunchOpensTheWindow read it on macOS.
#[test]
fn a_machine_nobody_is_signed_in_on_offers_an_account() {
    for world in [World::Empty, World::FirstLaunch] {
        let launched = made(world);
        let (model, told) = started(&launched);
        let shown = told.last_where("the Codex tip", |snapshot| {
            read_in(snapshot) && snapshot.footing == Footing::CodexUnused
        });
        assert!(
            matches!(&shown.accounts_shown, AccountsShown::NoAccounts { title, .. } if *title == words::no_accounts(FRONT).0),
            "{}: {:?}",
            world.name(),
            shown.accounts_shown
        );
        model.send(Intent::DeclineTip { tip: Tip::UseCodex });
        let declined = told.last_where("the Codex tip put away", |snapshot| {
            snapshot.footing == Footing::NoOneSignedIn
        });
        assert!(
            matches!(&declined.accounts_shown, AccountsShown::NoAccounts { title, .. } if *title == words::no_accounts(FRONT).0),
            "{}: {:?}",
            world.name(),
            declined.accounts_shown
        );
        // The window is asked for once the preferences say the app has never been seen, and
        // they are read on a lane of their own, so the accounts can be read first: in 2 of
        // 800 copies of this test run 16 at a time on Linux, firstLaunch's had not yet asked.
        // A machine whose app has been seen never asks, which `keeping.rs` holds.
        if world == World::FirstLaunch {
            told.last_where("the window asked for", |snapshot| {
                read_in(snapshot) && snapshot.window_request.serial > 0
            });
        } else {
            assert_eq!(shown.window_request.serial, 0, "{}", world.name());
        }
        model.shutdown();
    }
}

/// The login in use with no name is added from its row, as AccountsWindowTests.swift's
/// testNamingTheAccountInUse reads it, and adding it lists it.
#[test]
fn the_login_in_use_is_offered_a_name() {
    let launched = made(World::Unnamed);
    let (model, told) = started(&launched);
    let shown = told.last_where("the accounts read", read_in);
    let Some(RowEnd::Button { action }) = &shown.sections[0].accounts[0].end else {
        panic!("no button on the row");
    };
    assert_eq!(action.intent, Intent::Enrol { tool: Tool::Claude });
    model.send(Intent::Enrol { tool: Tool::Claude });
    told.last_where("the account added", |snapshot| {
        read_in(snapshot)
            && lists(snapshot, "claude/work")
            && matches!(snapshot.sheet, Some(Sheet::Name { .. }))
    });
    model.shutdown();
}

/// The login left in a file behind Claude Code's store is put away from the sheet its notice
/// opens, as the Windows app's stow UI test drives it: the sheet says whose login it is once
/// it has looked, and Put Away keeps it for personal and deletes the file, which a notice then
/// says.
#[test]
fn the_login_left_in_a_file_is_put_away_from_its_sheet() {
    let launched = made(World::LeftInFile);
    let (model, told) = started(&launched);
    let put_away = words::clean_up(FRONT, true);
    told.last_where("the file said, with Put Away…", |snapshot| {
        actions_of(snapshot, "warning/fallback_login/") == [put_away.as_str()]
    });

    model.send(Intent::PresentSheet { sheet: Sheet::Stow });
    let looked = told.last_where("the look", |snapshot| {
        snapshot
            .stow_text
            .as_ref()
            .is_some_and(|text| text.can_confirm)
    });
    let said = looked
        .stow_text
        .map(|text| text.message)
        .unwrap_or_default();
    assert!(said.contains("personal"), "{said:?}");
    model.send(Intent::Stow);

    told.last_where("put away, and said", |snapshot| {
        snapshot.sheet.is_none() && snapshot.notices.iter().any(|notice| notice.id == "stowed")
    });
    assert!(
        read(&launched.core)
            .warnings
            .iter()
            .all(|warning| warning.code != "fallback_login")
    );
    model.shutdown();
}

/// One account is offered a second until Not Now, as AccountsWindowTests.swift's
/// testOneAccountIsOfferedASecondUntilNotNow reads it, which the fixture's own folder keeps.
#[test]
fn one_account_is_offered_a_second_until_not_now() {
    let launched = made(World::OnlyOne);
    let (model, told) = started(&launched);
    let shown = told.last_where("the accounts read", read_in);
    assert_eq!(
        shown.setup.as_ref().map(|step| step.title.as_str()),
        Some(words::add_a_second(None).as_str())
    );
    model.send(Intent::DeclineSecondAccount { tool: Tool::Claude });
    told.last_where("the next tip, for Codex", |snapshot| {
        snapshot.setup.as_ref().map(|step| step.title.as_str())
            == Some(words::use_with_codex(FRONT).as_str())
    });
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
fn activity_lists_every_switch_above_every_enrolment() {
    let launched = made(World::OneTool);
    let (model, told) = started(&launched);
    model.send(Intent::PaneShown {
        pane: Pane::Activity,
    });
    let shown = told.last_where("the log read", |snapshot| {
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
        ["switch", "switch", "enroll", "enroll"].map(words::change_verb)
    );
    model.shutdown();
}

// As PanesAndSettingsTests.swift's testDailyRenewal reads it.
#[test]
fn daily_renewal_turns_on() {
    let launched = made(World::OneTool);
    let (model, told) = started(&launched);
    model.send(Intent::SetSchedule { on: true });
    told.last_where("the schedule on", |snapshot| {
        snapshot.machine.schedule.on && snapshot.machine.schedule.enabled
    });
    model.shutdown();
}

/// A Codex switch says running sessions keep the old account until they are restarted, as
/// AccountsWindowTests.swift's testUsingACodexAccountSaysSessionsKeepTheOldOne reads it.
#[test]
fn a_codex_switch_says_sessions_keep_the_old_account() {
    let launched = made(World::TwoTools);
    let (model, told) = started(&launched);
    model.send(Intent::SwitchTo {
        account: AccountKey::new(Tool::Codex, "spare"),
    });
    let shown = told.last_where("what the switch said", |snapshot| {
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
        notice
            .lines
            .contains(&words::restart_notice("codex", "main")),
        "{notice:?}"
    );
    model.shutdown();
}

/// Adding accounts as AccountsWindowTests.swift's tests do: Claude Code's sign-in asks for
/// the code and lists the account once it is typed back, Codex's lists it by itself, and one
/// cancelled adds nothing and lets another start.
#[test]
fn accounts_are_added_through_each_tools_sign_in() {
    let launched = made(World::OneTool);
    let (model, told) = started(&launched);
    model.send(Intent::PresentSheet {
        sheet: Sheet::Add { tool: None },
    });
    model.send(Intent::SignIn {
        tool: Tool::Claude,
        again: None,
    });
    let asking = told.last_where("the code asked for", |snapshot| {
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
    told.last_where("the sign-in over", |snapshot| snapshot.signing_in.is_none());
    model.send(Intent::SignIn {
        tool: Tool::Claude,
        again: None,
    });
    told.last_where("the code asked for", |snapshot| {
        snapshot
            .signing_in
            .as_ref()
            .is_some_and(|signing| signing.wants_code)
    });
    model.send(Intent::PasteCode { code: CODE.into() });
    told.last_where("the account listed", |snapshot| {
        snapshot.signing_in.is_none() && read_in(snapshot) && lists(snapshot, "claude/travel")
    });
    model.shutdown();

    let launched = made(World::TwoTools);
    let (model, told) = started(&launched);
    model.send(Intent::SignIn {
        tool: Tool::Codex,
        again: None,
    });
    told.last_where("the account listed", |snapshot| {
        snapshot.signing_in.is_none() && read_in(snapshot) && lists(snapshot, "codex/travel")
    });
    model.shutdown();
}

// Every world named a CODEX_HOME it never made, which Codex on Windows refuses, so adding a
// Codex account there was refused.
#[test]
fn adding_codex_in_first_launch_reaches_its_sign_in() {
    let launched = made(World::FirstLaunch);
    assert!(
        launched.machine.home().join(".codex").is_dir(),
        "the world names a Codex home it did not make"
    );
    let (model, told) = started(&launched);
    model.send(Intent::PresentSheet {
        sheet: Sheet::Add { tool: None },
    });
    model.send(Intent::SignIn {
        tool: Tool::Codex,
        again: None,
    });
    let ended = told.last_where("the account listed or refused", |snapshot| {
        snapshot.sheet_failure.is_some()
            || (snapshot.signing_in.is_none()
                && read_in(snapshot)
                && lists(snapshot, "codex/travel"))
    });
    assert_eq!(ended.sheet_failure, None);
    model.shutdown();
}

/// Cancel and then Sign In at once, as a person can press one after the other: the new
/// sign-in waits, shown as starting, until the one cancelled before it has stopped and let go
/// of the core's one sign-in at a time, then asks for the code, and nothing on the way is
/// refused as a sign-in already waiting.
#[test]
fn a_sign_in_asked_for_at_once_after_a_cancel_asks_for_the_code() {
    let launched = made(World::OneTool);
    let (model, told) = started(&launched);
    let sign_in = || Intent::SignIn {
        tool: Tool::Claude,
        again: None,
    };
    let refused =
        |snapshot: &Snapshot| snapshot.failure_alert.is_some() || snapshot.sheet_failure.is_some();
    model.send(Intent::PresentSheet {
        sheet: Sheet::Add { tool: None },
    });
    model.send(sign_in());
    let first = told.last_where("the code asked for", |snapshot| {
        snapshot
            .signing_in
            .as_ref()
            .is_some_and(|signing| signing.wants_code)
    });
    let first = first.signing_in.expect("the first sign-in").id;
    model.send(Intent::CancelSignIn);
    model.send(sign_in());
    let asking = told.last_where("the code asked for again, or a refusal", |snapshot| {
        refused(snapshot)
            || snapshot
                .signing_in
                .as_ref()
                .is_some_and(|signing| signing.id != first && signing.wants_code)
    });
    assert_eq!(
        (asking.failure_alert, asking.sheet_failure),
        (None, None),
        "the second sign-in refused"
    );
    model.send(Intent::PasteCode { code: CODE.into() });
    told.last_where("the account listed", |snapshot| {
        refused(snapshot)
            || (snapshot.signing_in.is_none()
                && read_in(snapshot)
                && lists(snapshot, "claude/travel"))
    });
    model.shutdown();
}

/// A rename to a name another account has is refused in its sheet, and a free name is
/// taken, as AccountsWindowTests.swift's testRenamingAnAccount reads it; the account in use
/// offers Forget dimmed, which its shortcut menu leaves out, and any other is forgotten, as
/// its testTheAccountInUseOffersNoForget and testForgettingAsksFirst read it.
#[test]
fn accounts_are_renamed_and_forgotten_as_the_window_offers() {
    let launched = made(World::OneTool);
    let (model, told) = started(&launched);
    let shown = told.last_where("the accounts read", read_in);
    // Each row's own menu but its windows, by its items' titles, with Remove Account… last.
    let menus: Vec<(String, Vec<String>)> = shown
        .sections
        .iter()
        .flat_map(|section| &section.accounts)
        .map(|item| {
            let titles = (item.actions.iter())
                .filter(|action| action.kind != ActionKind::OpenWindow)
                .map(|action| action.title.clone())
                .collect();
            (item.title.clone(), titles)
        })
        .collect();
    let titles = |label| {
        vec![
            words::switch_account(Some(label)),
            words::sign_in_again(FRONT, true),
            words::rename(true).into(),
            words::warm_up_now(FRONT),
            words::remove_account(FRONT),
        ]
    };
    assert_eq!(
        menus,
        [
            ("work".into(), titles("work")),
            ("personal".into(), titles("personal")),
        ]
    );
    let work = &shown.sections[0].accounts[0];
    let enabled = |kind| (work.actions.iter()).any(|action| action.kind == kind && action.enabled);
    assert!(!enabled(ActionKind::Switch) && !enabled(ActionKind::Remove));

    model.send(Intent::PresentSheet {
        sheet: Sheet::Rename {
            account: AccountKey::new(Tool::Claude, "personal"),
        },
    });
    model.send(Intent::Rename {
        account: AccountKey::new(Tool::Claude, "personal"),
        to: "work".into(),
    });
    let refused = told.last_where("the rename refused", |snapshot| {
        snapshot.sheet_failure.is_some()
    });
    assert_eq!(
        refused.sheet_failure.map(|failure| failure.title),
        Some(words::could_not(&Doing::Rename {
            account: Key::new(ProviderId::Claude, "personal")
        }))
    );
    model.send(Intent::Rename {
        account: AccountKey::new(Tool::Claude, "personal"),
        to: "home".into(),
    });
    told.last_where("the account renamed", |snapshot| {
        read_in(snapshot)
            && shows_in_use(snapshot) == ["claude/work"]
            && lists(snapshot, "claude/home")
    });
    model.send(Intent::Forget {
        account: AccountKey::new(Tool::Claude, "home"),
    });
    told.last_where("the account forgotten", |snapshot| {
        read_in(snapshot) && rows(snapshot).count() == 1
    });
    model.shutdown();
}

// As MenuBarTests.swift's testAFailedReadIsSaidInTheMenuAndInTheWindow reads it in readFailure.
#[test]
fn a_failed_read_is_said_in_the_menu_and_in_the_window() {
    let launched = made(World::ReadFailure);
    let told = Arc::new(Told::default());
    let model = launched.model(Arc::clone(&told) as Arc<dyn Listener>, Arc::new(Utc));
    model.send(Intent::Start);
    let shown = told.last_where("the read failed", |snapshot| {
        !snapshot.reading && matches!(snapshot.accounts_shown, AccountsShown::ReadFailed { .. })
    });
    let AccountsShown::ReadFailed {
        detail: failure, ..
    } = shown.accounts_shown.clone()
    else {
        unreachable!("waited for")
    };
    let Err(PitboardError::Failed { code, message, .. }) = launched.core.status(true) else {
        panic!("the core's read fails too");
    };
    assert_eq!((code.as_str(), &failure), ("state_unreadable", &message));

    let item = shown.menu_notices.others.expect("an item");
    assert_eq!(
        (item.title.as_str(), item.subtitle.as_deref()),
        (
            words::issues(FRONT, 1).as_str(),
            Some(words::could_not(&Doing::ReadUsage).as_str())
        )
    );
    assert_eq!(
        item.intent,
        Some(Intent::Show {
            place: Some(Place::Accounts)
        })
    );
    assert_eq!(item.help.as_ref(), Some(&failure));
    assert!(shown.notices.is_empty(), "the window says it once, below");

    assert!(shown.sections.is_empty(), "nothing is known");
    assert_eq!(
        shown.menu_accounts_note,
        Some(words::no_accounts_to_show(FRONT))
    );
    assert_eq!(
        shown.accounts_shown,
        AccountsShown::ReadFailed {
            title: words::could_not_read_accounts(FRONT),
            detail: failure,
            retry: crate::present::Choice {
                title: words::try_again(FRONT),
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
fn a_service_out_of_reach_leaves_the_last_numbers_and_says_why_on_each_account() {
    let launched = made(World::OneTool);
    out_of_reach(&launched.machine).expect("out of reach");
    let fresh = read(&launched.core);
    assert_eq!(
        fresh
            .accounts
            .iter()
            .map(|account| (account.stale.map(Stale::code), account.usage.is_some()))
            .collect::<Vec<_>>(),
        [(Some("unreachable"), true), (Some("unreachable"), true)]
    );

    let (model, told) = started(&launched);
    let shown = told.last_where("the accounts read", read_in);
    let items: Vec<_> = shown
        .sections
        .iter()
        .flat_map(|section| &section.accounts)
        .collect();
    assert_eq!(items.len(), 2);
    for item in items {
        let stale = item
            .notes
            .iter()
            .find(|note| note.kind == crate::present::NoteKind::Stale);
        assert_eq!(
            stale.map(|note| &note.text),
            fresh.accounts[0].stale_explanation.as_ref(),
            "{item:?}"
        );
        assert!(!item.limits.is_empty(), "the last numbers measured");
    }
    assert!(!matches!(
        shown.accounts_shown,
        AccountsShown::ReadFailed { .. }
    ));
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
/// has "Interrupted switch", worth looking at, beside personal's parked login.
#[test]
fn an_interrupted_switch_is_offered_a_way_out_as_the_app_starts() {
    let launched = made(World::Stuck);
    let (model, told) = started(&launched);
    let shown = told.last_where("the accounts read", read_in);
    let notice = shown.notices.first().expect("a notice");
    assert_eq!(
        (notice.id.as_str(), notice.title.as_str()),
        ("stuck", words::SWITCH_WAITING)
    );
    let give_up = notice.actions.first().expect("an action");
    assert_eq!(give_up.title, words::cancel_switch(FRONT, true));
    assert_eq!(give_up.intent, Intent::AbandonStuckSwitch);
    assert_eq!(
        give_up
            .confirm
            .as_ref()
            .map(|question| question.confirm.clone()),
        Some(words::cancel_switch(FRONT, false))
    );
    let item = shown.menu_notices.others.expect("an item");
    assert_eq!(
        (item.title.as_str(), item.subtitle.as_deref()),
        (
            words::issues(FRONT, 1).as_str(),
            Some(words::SWITCH_WAITING)
        )
    );
    assert_eq!(
        item.intent,
        Some(Intent::Show {
            place: Some(Place::Accounts)
        })
    );

    model.send(Intent::PaneShown { pane: Pane::Checks });
    let checked = told.last_where("the checks made", |snapshot| {
        !snapshot.machine.checks.lines().is_empty() && !snapshot.machine.checks.checking
    });
    let lines = checked.machine.checks.lines();
    let interrupted = (lines.iter())
        .find(|line| line.name == words::check_name(FRONT, "interrupted_switch", ""))
        .expect("doctor's check");
    assert_eq!(interrupted.level, crate::Level::Warn);
    assert_eq!(
        interrupted.action.as_ref().map(|cancel| &cancel.intent),
        Some(&Intent::AbandonStuckSwitch)
    );
    assert_eq!(
        checked.machine.checks.summary,
        Some(words::check_issues(FRONT, 2)),
        "personal's saved sign-in, and the switch"
    );

    model.send(Intent::AbandonStuckSwitch);
    let given_up = told.last_where("what giving up kept", |snapshot| {
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
    assert_eq!(notice.title, words::SWITCH_CANCELLED);
    assert!(
        notice
            .lines
            .iter()
            .any(|line| line.contains(&words::sign_ins(2))),
        "{notice:?}"
    );
    told.last_where("nothing waiting", |snapshot| {
        snapshot.notices.iter().all(|notice| notice.id != "stuck")
    });
    model.shutdown();
}

/// This Mac shows the core's checks, as PanesAndSettingsTests.swift's
/// testThisMacShowsTheChecks reads them, and they are not the Swift fixture's. That passed a
/// check called "Keychain" and warned that daily renewal was off, which made "One thing is
/// worth looking at.". The core has no check called "Keychain", and says nothing of a
/// schedule that is not there. oneTool's one thing is personal's parked login instead, which
/// is the world's own doing: it lasts two days more, where the Swift fixture's lasted eleven,
/// and doctor warns of a parked login whose refresh token lapses within three days.
///
/// So in PR 10 that test reads what the core's doctor says: the sentence is the same, of
/// personal's parked login rather than of daily renewal, and it waits for one of the core's
/// checks rather than one called "Keychain". work's parked login and personal's are two
/// checks of one code, so the pane lists them by their ids.
#[test]
fn this_mac_shows_the_cores_checks() {
    let launched = made(World::OneTool);
    let (model, told) = started(&launched);
    model.send(Intent::PaneShown { pane: Pane::Checks });
    let shown = told.last_where("the checks made", |snapshot| {
        !snapshot.machine.checks.lines().is_empty() && !snapshot.machine.checks.checking
    });
    let checks = shown.machine.checks;
    let lines = checks.lines();
    let worth_looking_at: Vec<&str> = (lines.iter())
        .filter(|line| line.level != crate::Level::Ok)
        .map(|line| line.name.as_str())
        .collect();
    assert_eq!(worth_looking_at, ["personal"], "{lines:#?}");
    // Every account's saved sign-in is a check of its own under one code, and each is a line
    // with its own id, which the pane lists them by.
    let claude = (checks.groups.iter())
        .find(|group| group.id == ProviderId::Claude.code())
        .expect("Claude Code's checks");
    let accounts: Vec<&str> = (claude.lines.iter())
        .map(|line| line.name.as_str())
        .filter(|name| ["work", "personal"].contains(name))
        .collect();
    assert_eq!(accounts, ["work", "personal"], "{lines:#?}");
    let ids: std::collections::BTreeSet<u64> = lines.iter().map(|line| line.id).collect();
    assert_eq!(ids.len(), lines.len(), "{lines:#?}");
    let personal = (lines.iter())
        .find(|line| line.name == "personal")
        .expect("personal's");
    assert!(
        personal.detail.starts_with("Saved sign-in valid until "),
        "{personal:?}"
    );
    assert_eq!(
        personal.action.as_ref().map(|again| again.title.clone()),
        Some(words::sign_in_again(FRONT, true))
    );
    assert_eq!(checks.summary, Some(words::check_issues(FRONT, 1)));
    model.shutdown();
}

/// A person's clock that is not the command line's: 12 hours, in UTC.
struct Twelve;

impl LocalTime for Twelve {
    fn clock(&self, epoch: i64, with_weekday: bool) -> Result<String, PlatformError> {
        let of_day = epoch.rem_euclid(86_400);
        let (hours, minutes) = (of_day / 3600, of_day % 3600 / 60);
        let half = if hours < 12 { "AM" } else { "PM" };
        let time = format!("{}:{minutes:02} {half}", (hours + 11) % 12 + 1);
        Ok(if with_weekday {
            format!("{} {time}", &Utc.clock(epoch, true)?[..3])
        } else {
            time
        })
    }

    fn same_day(&self, first: i64, second: i64) -> Result<bool, PlatformError> {
        Utc.same_day(first, second)
    }

    fn date_and_time(&self, epoch: i64) -> Result<String, PlatformError> {
        self.clock(epoch, true)
    }

    fn time_of_day(&self, minutes: u16) -> Result<String, PlatformError> {
        Utc.time_of_day(minutes)
    }

    fn date(&self, epoch: i64) -> Result<String, PlatformError> {
        Utc.date(epoch)
    }
}

// Doctor's moments, which Copy Report copies, read on the person's 12-hour clock.
#[test]
fn this_pc_says_when_a_check_was_made_on_the_persons_clock() {
    let launched = made(World::OneTool);
    let told = Arc::new(Told::default());
    let model = launched.model(Arc::clone(&told) as Arc<dyn Listener>, Arc::new(Twelve));
    model.send(Intent::Start);
    told.last_where("the accounts read", read_in);
    model.send(Intent::PaneShown { pane: Pane::Checks });
    let shown = told.last_where("the checks made", |snapshot| {
        !snapshot.machine.checks.lines().is_empty() && !snapshot.machine.checks.checking
    });
    let report = shown.machine.checks.report.expect("doctor's report");
    let report: serde_json::Value = serde_json::from_str(&report).expect("JSON");
    let in_use = (report["checks"].as_array().into_iter().flatten())
        .find(|check| check["code"] == "in_use")
        .expect("the account in use");
    let said = in_use["detail"]
        .as_str()
        .unwrap_or_default()
        .rsplit_once("as Anthropic said ")
        .map(|(_, when)| when)
        .unwrap_or_else(|| panic!("{in_use:?}"));
    assert!(
        said.ends_with(" AM") || said.ends_with(" PM"),
        "{said} is not on the person's clock"
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
fn the_account_windows_are_what_their_ui_tests_read() {
    use crate::SiteMenu;
    use crate::account_windows::WindowNoteKind;
    use crate::present::PickerShown;

    let launched = made(World::TwoTools);
    let (model, told) = started(&launched);
    let read = told.last_where("the accounts read", read_in);
    let spare_row = read
        .sections
        .iter()
        .flat_map(|section| &section.accounts)
        .find(|item| item.qualified.as_deref() == Some("codex/spare"))
        .expect("spare's row");
    let opens: Vec<(&str, &str)> = (spare_row.actions.iter())
        .filter_map(|action| {
            Some((
                action.title.as_str(),
                action.window.as_ref()?.label.as_str(),
            ))
        })
        .collect();
    assert_eq!(opens, [(words::open_site("chatgpt.com").as_str(), "spare")]);
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
                words::open_site("claude.ai"),
                vec!["work".to_owned(), "old".into(), "personal".into()]
            ),
            (
                words::open_site("chatgpt.com"),
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
        .last_where("Open answering", |last| {
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
    assert_eq!(title, &words::open_link_as("chatgpt.com"));
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
        .last_where("spare's window", |last| {
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
    let refused = told.last_where("the refusal", |last| {
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
    assert_eq!(title, words::cannot_open_link(FRONT));
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
    let refused = PitboardModel::fixture(
        "twoTool".into(),
        FRONT,
        Arc::new(Told::default()),
        Arc::new(Utc),
    );
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

// The folder a fixture made in `made` keeps Pitboard's files in, as each system names it.
fn pitboard_in(made: &std::path::Path) -> std::path::PathBuf {
    let home = made.join("home");
    match OS {
        Os::MacOs | Os::Linux => home.join(".pitboard"),
        Os::Windows => home.join("AppData").join("Local").join("Pitboard"),
    }
}

/// The test itself, in a process whose temporary directory is its own.
fn an_exported_fixture_is_made_in_the_temporary_directory() {
    let _forgets = ForgetsTheLaunches;
    let shared = std::env::temp_dir().join("pitboard-fixture");
    let told = pitboard_in(&shared).join("told.json");
    std::fs::create_dir_all(told.parent().expect("a folder")).expect("a folder");
    std::fs::write(&told, r#"{"claude/work/session/":7200}"#).expect("left behind");

    let listener = Arc::new(Told::default());
    let model = PitboardModel::fixture(
        "oneTool".into(),
        FRONT,
        Arc::clone(&listener) as Arc<dyn Listener>,
        Arc::new(Utc),
    )
    .expect("a fixture");
    assert!(pitboard_in(&shared).is_dir());
    assert!(!told.exists(), "what the last launch told is gone");
    model.send(Intent::Start);
    let shown = listener.last_where("the accounts read", read_in);
    assert_eq!(
        listed(&shown),
        [
            ("claude/work".into(), true),
            ("claude/personal".into(), false)
        ]
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
fn an_exported_fixture_in_a_directory_of_its_own_is_made_there() {
    let own = Folder::own("exported-in").expect("a folder");
    let made = own.path().join("pitboard-fixture");
    let told = pitboard_in(&made).join("told.json");
    std::fs::create_dir_all(told.parent().expect("a folder")).expect("a folder");
    std::fs::write(&told, r#"{"claude/work/session/":7200}"#).expect("left behind");

    let listener = Arc::new(Told::default());
    let model = PitboardModel::fixture_in(
        "oneTool".into(),
        FRONT,
        own.path().to_string_lossy().into_owned(),
        Arc::clone(&listener) as Arc<dyn Listener>,
        Some(StandInApps::new(&[], true)),
        Arc::new(Posted::default()),
        Arc::new(Utc),
    )
    .expect("a fixture");
    assert!(pitboard_in(&made).is_dir());
    assert!(!told.exists(), "what the last launch told is gone");
    let epoch = || {
        let since = std::time::UNIX_EPOCH.elapsed().expect("after 1970");
        i64::try_from(since.as_secs()).expect("in range")
    };
    let asked = epoch();
    model.send(Intent::Start);
    let shown = listener.last_where("the accounts read", read_in);
    let answered = epoch();
    assert_eq!(
        listed(&shown),
        [
            ("claude/work".into(), true),
            ("claude/personal".into(), false)
        ]
    );
    // The read's time, on the person's clock.
    let read_then: Vec<String> = (asked..=answered)
        .map(|at| words::updated(&Utc.clock(at, false).expect("a time")))
        .collect();
    assert!(
        read_then.contains(&shown.updated_menu),
        "{} is none of {read_then:?}",
        shown.updated_menu
    );
    model.shutdown();
    assert!(made.is_dir(), "left there, as an app's is");
}

// The platform a test of an app's bindings hands `fixture_in`. ModelTests.cs crosses the same
// flows from C#: one failing there and not here points at the bindings.

fn started_in(
    own: &Folder,
    world: World,
    apps: &Arc<StandInApps>,
    posted: &Arc<Posted>,
) -> (Arc<PitboardModel>, Arc<Told>) {
    let told = Arc::new(Told::default());
    let model = PitboardModel::fixture_in(
        world.name().into(),
        FRONT,
        own.path().to_string_lossy().into_owned(),
        Arc::clone(&told) as Arc<dyn Listener>,
        Some(Arc::clone(apps) as Arc<dyn OtherApps>),
        Arc::clone(posted) as Arc<dyn Notifier>,
        Arc::new(Utc),
    )
    .expect("a fixture");
    model.send(Intent::Start);
    told.last_where("the accounts read", read_in);
    (model, told)
}

fn uses(snapshot: &Snapshot, qualified: &str) -> bool {
    shows_in_use(snapshot).iter().any(|used| used == qualified)
}

#[test]
fn a_codex_switch_quits_and_reopens_chatgpt_through_the_apps_fixture_in_is_given() {
    let own = Folder::own("given-apps").expect("a folder");
    let apps = StandInApps::new(&[CHATGPT], true);
    let (model, told) = started_in(
        &own,
        World::ChatGptOpen,
        &apps,
        &Arc::new(Posted::default()),
    );

    model.send(Intent::SwitchTo {
        account: AccountKey::new(Tool::Codex, "spare"),
    });
    let asked = told.last_where("the quit question", |snapshot| {
        snapshot.quit_question.is_some()
    });
    let question = asked.quit_question.expect("asked");
    assert_eq!(
        (question.question.title, &question.confirmed),
        (
            words::quit_to_switch("ChatGPT"),
            &Intent::QuitAndSwitch {
                account: AccountKey::new(Tool::Codex, "spare")
            }
        )
    );
    assert_eq!(apps.running_calls(), 1);
    model.send(question.confirmed);
    told.last_where("the switch, read back", |snapshot| {
        uses(snapshot, "codex/spare")
    });
    eventually("opened ChatGPT again", || apps.asked().len() == 2);
    assert_eq!(
        apps.asked(),
        [format!("quit {CHATGPT}"), format!("open {CHATGPT}")]
    );
    assert_eq!(apps.running_calls(), 3);
    model.shutdown();
}

#[test]
fn a_refusal_to_quit_from_the_apps_fixture_in_is_given_stops_the_switch() {
    let own = Folder::own("refusing-apps").expect("a folder");
    let apps = StandInApps::new(&[CHATGPT], true);
    apps.fail_quit(true);
    let (model, told) = started_in(
        &own,
        World::ChatGptOpen,
        &apps,
        &Arc::new(Posted::default()),
    );

    model.send(Intent::SwitchTo {
        account: AccountKey::new(Tool::Codex, "spare"),
    });
    told.last_where("the quit question", |snapshot| {
        snapshot.quit_question.is_some()
    });
    model.send(Intent::QuitAndSwitch {
        account: AccountKey::new(Tool::Codex, "spare"),
    });
    let refused = told.last_where("the switch stopped", |snapshot| {
        snapshot.failure_alert.is_some()
    });
    let failure = refused.failure_alert.as_ref().expect("stopped");
    assert_eq!(
        failure.title,
        words::could_not(&Doing::Switch {
            account: Key::new(ProviderId::Codex, "spare")
        })
    );
    assert_eq!(
        failure.message,
        words::still_open("ChatGPT", "switch again")
    );
    assert!(uses(&refused, "codex/main"));
    assert!(apps.asked().is_empty(), "{:?}", apps.asked());
    assert_eq!(apps.running_calls(), 2);
    model.shutdown();
}

#[test]
fn a_run_out_is_posted_through_the_notifications_fixture_in_is_given() {
    let own = Folder::own("given-notifications").expect("a folder");
    let posted = Arc::new(Posted::default());
    let (model, told) = started_in(&own, World::OneTool, &StandInApps::new(&[], true), &posted);

    model.send(Intent::SwitchTo {
        account: AccountKey::new(Tool::Claude, "personal"),
    });
    told.last_where("the switch, read back", |snapshot| {
        uses(snapshot, "claude/personal")
    });
    eventually("posted the run-out", || !posted.posted().is_empty());
    let posted = posted.posted();
    // When the limit resets is the read's.
    let resets_at = posted[0]
        .id
        .strip_prefix("claude/personal/session/-")
        .expect("personal's 5-hour limit");
    assert!(resets_at.parse::<i64>().is_ok(), "{resets_at}");
    assert_eq!(
        posted,
        [Notice {
            id: format!("claude/personal/session/-{resets_at}"),
            title: words::limit_notification(FRONT, "5-hour", None, true),
            subtitle: Some("Claude Code".into()),
            body: format!(
                "{} {}",
                words::has_used_share("personal", "100%"),
                words::other_used("work", Some(58))
            ),
            clicked: "accounts".into(),
            button: Some(NoticeButton {
                kind: NoticeButtonKind::SwitchTo,
                title: words::switch_button(FRONT, "work"),
                clicked: "switch/claude/work".into(),
            }),
        }]
    );
    model.shutdown();
}

#[test]
fn a_login_replaced_outside_is_posted_through_the_notifications_fixture_in_is_given() {
    let own = Folder::own("given-replaced").expect("a folder");
    let posted = Arc::new(Posted::default());
    let (model, _) = started_in(
        &own,
        World::LoginReplaced,
        &StandInApps::new(&[], true),
        &posted,
    );

    eventually("posted the login replaced", || !posted.posted().is_empty());
    model.shutdown();

    // The account in use has run out too, which is posted after it.
    let said: Vec<(String, String, Option<NoticeButton>)> = posted
        .posted()
        .into_iter()
        .filter(|notice| notice.id.starts_with("warning/login_replaced/"))
        .map(|notice| (notice.title, notice.body, notice.button))
        .collect();
    let replaced = pitboard_core::service::Warning::LoginReplaced {
        tool: ProviderId::Claude,
        id: String::new(),
        label: "claude/work".into(),
        now: pitboard_core::service::Stored::Account("claude/personal".into()),
    };
    assert_eq!(
        said,
        [(
            words::sign_in_replaced(FRONT),
            words::in_app_warning(FRONT, &replaced).expect("the app's words"),
            Some(NoticeButton {
                kind: NoticeButtonKind::SignInAgain,
                title: words::sign_in_again_button(FRONT),
                clicked: "sign-in-again/claude/work".into(),
            })
        )]
    );
}

#[test]
fn a_notification_the_system_refused_is_said_after_doctors_checks() {
    let own = Folder::own("refused-notifications").expect("a folder");
    let posted = Arc::new(Posted::refusing("COMException 0x80070490"));
    let (model, told) = started_in(
        &own,
        World::LoginReplaced,
        &StandInApps::new(&[], true),
        &posted,
    );
    // The login replaced, then the run-out of the account in use, the last refused.
    eventually("posted both", || posted.posted().len() == 2);

    model.send(Intent::PaneShown { pane: Pane::Checks });
    let refused = |snapshot: &Snapshot| {
        (snapshot.machine.checks.lines().into_iter()).find(|line| line.name == words::NOTIFICATIONS)
    };
    let checked = told.last_where("the checks with the refusal", |snapshot| {
        refused(snapshot).is_some()
    });
    model.shutdown();

    let line = refused(&checked).expect("a line");
    assert_eq!(line.level, crate::Level::Warn);
    // Said at the clock time it was refused, which the test cannot know to the minute.
    let said = words::check_text(&words::not_posted(
        &words::limit_notification(FRONT, "5-hour", None, true),
        "\n",
        "COMException 0x80070490",
    ));
    let (before, after) = said.split_once('\n').expect("a clock time");
    assert!(
        line.detail.starts_with(before) && line.detail.ends_with(after),
        "{}",
        line.detail
    );
}

// Plan.md section 3, in every world; doctor's report alone, which Copy Report copies, says them.
#[test]
fn no_word_on_screen_is_park_put_away_give_up_enrol_record_or_settle() {
    use crate::present::testing::{mechanism_words, strings_of};
    let mut said = Vec::new();
    for world in World::ALL {
        let launched = made(world);
        let told = Arc::new(Told::default());
        let model = launched.model(Arc::clone(&told) as Arc<dyn Listener>, Arc::new(Utc));
        model.send(Intent::Start);
        told.last_where("the accounts read, or not", |snapshot| {
            read_in(snapshot) || matches!(snapshot.accounts_shown, AccountsShown::ReadFailed { .. })
        });
        for pane in [Pane::Checks, Pane::Activity] {
            model.send(Intent::PaneShown { pane });
        }
        model.send(Intent::PresentSheet {
            sheet: Sheet::Add { tool: None },
        });
        let shown = told.last_where("the checks, the activity and the sheet", |snapshot| {
            let machine = &snapshot.machine;
            !machine.checks.checking
                && !machine.checks.lines().is_empty()
                && (!machine.activity.lines.is_empty() || machine.activity.empty.is_some())
                && snapshot.sheet_text.is_some()
        });
        model.shutdown();
        let mut shown = shown;
        shown.machine.checks.report = None;
        for text in strings_of(&shown) {
            let found = mechanism_words(&text);
            if !found.is_empty() && !said.iter().any(|(_, kept, _)| *kept == text) {
                said.push((world.name(), text, found));
            }
        }
    }
    assert!(said.is_empty(), "{said:#?}");
}
