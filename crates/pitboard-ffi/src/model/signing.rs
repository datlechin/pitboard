//! Signing in, and the sheet a sign-in is started from, as the Swift model did them: its tests
//! on those, each under its own name in snake case, driven through `State::apply` by hand.
//! A sign-in's thread answers once its tool has started, then with each piece of what the
//! tool says, then once it has stopped saying anything, and a test hands those answers on in
//! whatever order the Swift tests reached with gates. Where a Swift test also checked what the
//! window says of it, which comes with the model's wording, the rest of it is kept here.

use super::changing::a_look_landing_after;
use super::lanes::Lane;
use super::state::{After, Answer, Job};
use super::switching::a_read_that_started_before;
use super::testing::machine::{
    Hand, Stub, a_look_or_a_read, any_read, desktop_machine, installed_ask,
};
use super::testing::platform::StandInApps;
use super::testing::records::{
    CLAUDE, MONITOR, awaiting, claude, claude_code, claude_desktop, codex, codex_account,
    desktop_account, desktop_warning, enrolled_as, refusal, status, still_running, switched,
    warning,
};
use super::{Intent, Pane, QuitQuestion, RestartNeeded, RunningSignIn, Sheet, WindowRequest};
use crate::EnrolledAs;
use pitboard_core::switch::WaitStep;

/// What `claude auth login` 2.1.289 writes once it reads a code typed back, as the register's
/// `sign_in_output` holds it.
const PROMPT: &str = "Paste code here if prompted > ";

/// What `claude auth login` 2.1.289 writes on refusing a code typed back, as the register's
/// `sign_in_takes_another_code` holds it.
const REFUSED: &str = "Invalid code. Please make sure the full code was copied.\n";

/// An address Codex prints for a person to open.
const CODEX_ADDRESS: &str = "https://auth.openai.com/oauth/authorize?state=x\n";

fn sign_in(provider: &str, name: &str) -> Intent {
    Intent::SignIn {
        provider: provider.into(),
        name: name.into(),
    }
}

fn paste(code: &str) -> Intent {
    Intent::PasteCode { code: code.into() }
}

fn present(sheet: Sheet) -> Intent {
    Intent::PresentSheet { sheet }
}

pub(super) fn add(provider: Option<&str>) -> Sheet {
    Sheet::Add {
        provider: provider.map(str::to_owned),
    }
}

/// Puts `sheet` up and answers what that asks.
fn put_up(model: &mut Hand, machine: &mut Stub, sheet: Sheet) {
    model.send(present(sheet));
    model.run(machine);
}

/// Asks for a sign-in of `provider`'s tool as `name`, and answers its start: the id it was
/// given.
fn starts(model: &mut Hand, machine: &mut Stub, provider: &str, name: &str) -> u64 {
    let jobs = model.send(sign_in(provider, name));
    let [Job::SignIn { id, .. }] = &jobs[..] else {
        panic!("one sign-in asked for: {jobs:?}");
    };
    model.run(machine);
    *id
}

/// The sign-in under way, as the snapshot shows it.
fn running(model: &Hand) -> RunningSignIn {
    model.shown().signing_in.expect("a sign-in under way")
}

fn code_refused(model: &Hand) -> bool {
    model.state.signing().is_some_and(|s| s.code_refused())
}

/// The tool of the sign-in `id` says `text`, as its thread hands it on.
fn says(model: &mut Hand, id: u64, text: &str) {
    model.give(Answer::SignInSaid {
        id,
        text: text.into(),
        step: None,
    });
}

/// The tool of the sign-in `id` has stopped saying anything, and everything that leads to.
fn ends(model: &mut Hand, machine: &mut Stub, id: u64) {
    model.give(Answer::SignInQuiet { id });
    model.run(machine);
}

/// A sign-in from start to finish, its tool saying `said` on the way, as the Swift tests
/// awaited `signIn`.
pub(super) fn signs_in(
    model: &mut Hand,
    machine: &mut Stub,
    provider: &str,
    name: &str,
    said: &[&str],
) {
    let id = starts(model, machine, provider, name);
    for text in said {
        says(model, id, text);
    }
    ends(model, machine, id);
}

fn sign_in_over(job: &Job) -> bool {
    matches!(job, Job::SignInOver { .. })
}

/// A sign-in that cannot start says why in the sheet that started it, which stays up to say it,
/// and leaves nothing half-shown.
#[test]
fn a_sign_in_that_cannot_start_is_reported() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.starting = Err(refusal(
        "sign_in_not_isolated",
        "a sign-in here would write over the login in use",
        Vec::new(),
    ));
    put_up(&mut model, &mut machine, add(None));
    starts(&mut model, &mut machine, "claude", "work");

    let shown = model.shown();
    assert_eq!(machine.signed_in, ["claude/work"]);
    assert_eq!(shown.signing_in, None);
    assert_eq!(shown.sheet, Some(add(None)));
    let failure = shown.sheet_failure.expect("said in the sheet");
    assert_eq!(failure.title, "Couldn’t sign in to work");
    assert_eq!(
        failure.message,
        "a sign-in here would write over the login in use"
    );
    assert_eq!(model.sheet_failure_code(), Some("sign_in_not_isolated"));
    assert_eq!(model.state.presented, None, "not in an alert over it");
    assert_eq!(model.state.failure, None);
}

// The core found no program where the last look did: the sheet says so under the tool, as it
// would have before Sign In, and Pitboard looks again.
#[test]
fn a_sign_in_whose_program_has_gone_says_so_under_its_tool() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.found = vec![claude_code(), codex()];
    put_up(&mut model, &mut machine, add(Some("codex")));
    let recovered = warning(
        "interrupted_switch_undone",
        "an earlier switch was interrupted",
    );
    machine.starting = Err(refusal(
        "codex_program_missing",
        "`codex` is not on this machine",
        vec![recovered.clone()],
    ));
    machine.found = vec![claude_code()];
    let asked = model.count(installed_ask);
    starts(&mut model, &mut machine, "codex", "phu");

    let shown = model.shown();
    assert_eq!(shown.sheet_failure, None, "said under the tool instead");
    assert_eq!(
        model.state.warnings,
        [recovered],
        "and what it found on the way"
    );
    let sheet = shown.sheet_text.expect("the sheet");
    let tool = sheet
        .tools
        .iter()
        .find(|t| t.code == "codex")
        .expect("codex");
    assert!(tool.missing.is_some(), "{tool:?}");
    assert_eq!(model.count(installed_ask), asked + 1, "and looks again");
}

// A look under way when the core says a program has gone was asked before it went: its answer
// leaves the tool missing, and Pitboard looks once more.
#[test]
fn a_look_under_way_when_a_program_goes_leaves_it_missing() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.found = vec![claude_code(), codex()];
    put_up(&mut model, &mut machine, add(Some("codex")));
    model.send(Intent::LookForTools);
    let look = model.take(installed_ask);
    let before = machine.answer(look);
    machine.starting = Err(refusal(
        "codex_program_missing",
        "`codex` is not on this machine",
        Vec::new(),
    ));
    machine.found = vec![claude_code()];
    model.send(sign_in("codex", "phu"));
    model.run_but(&mut machine, installed_ask);

    model.give(before);
    let codex_missing = |model: &Hand| {
        model
            .shown()
            .sheet_text
            .and_then(|text| text.tools.into_iter().find(|tool| tool.code == "codex"))
            .is_some_and(|tool| tool.missing.is_some())
    };
    assert!(
        codex_missing(&model),
        "not brought back by an answer from before"
    );
    let again = model.take(installed_ask);
    model.give(machine.answer(again));
    assert!(codex_missing(&model));
}

// A failure is about what was asked, so it goes once the name or the tool changes.
#[test]
fn a_failure_goes_once_the_form_changes() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.starting = Err(refusal("sign_in_not_isolated", "not here", Vec::new()));
    put_up(&mut model, &mut machine, add(None));
    starts(&mut model, &mut machine, "claude", "work");
    assert!(model.shown().sheet_failure.is_some());

    model.send(Intent::DismissSheetFailure);
    assert_eq!(model.shown().sheet_failure, None);
    assert_eq!(model.shown().sheet, Some(add(None)), "the sheet stays");
}

// Someone told a tool is missing installs it elsewhere and comes back: the sheet looks again,
// and only a sheet that signs in does.
#[test]
fn a_sheet_that_signs_in_looks_for_tools_again_when_asked() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.found = vec![claude_code()];
    model.refresh(&mut machine);
    assert_eq!(model.send(Intent::LookForTools), [], "no sheet");

    put_up(&mut model, &mut machine, add(Some("codex")));
    machine.found = vec![claude_code(), codex()];
    assert_eq!(model.send(Intent::LookForTools), [Job::AskInstalled]);
    model.run(&mut machine);
    assert_eq!(model.state.installed, Some(vec![claude_code(), codex()]));

    model.send(Intent::CloseSheet);
    put_up(
        &mut model,
        &mut machine,
        Sheet::Rename {
            provider: "claude".into(),
            label: "work".into(),
        },
    );
    assert_eq!(
        model.send(Intent::LookForTools),
        [],
        "a rename signs nothing in"
    );
}

/// Codex's sign-in prints an address and reads nothing, so the tool is named in the label the
/// core is given, and the sign-in finishes and enrols.
#[test]
fn a_codex_sign_in_is_for_codex() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    put_up(&mut model, &mut machine, add(Some("codex")));
    signs_in(&mut model, &mut machine, "codex", "work", &[CODEX_ADDRESS]);
    assert_eq!(machine.signed_in, ["codex/work"]);
    assert_eq!(machine.over, [(1, true)], "finished and enrolled");
    let shown = model.shown();
    assert_eq!(shown.signing_in, None);
    assert_eq!(shown.sheet, None, "and the sheet that started it is done");
}

/// Signing in again to the account in use puts its new login in use at once. It says so, and
/// what the core warned about sessions still on the old login stays beside it until the tool
/// has another account in use, the way a switch's warning does, rather than among the read's
/// warnings, which the next read replaces. The notice comes with what the window says.
#[test]
fn a_sign_in_to_the_account_in_use_says_its_new_login_is_in_use() {
    let old_login = warning(
        "sessions_keep_old_login",
        "2 `codex` sessions started before this sign-in are still running and still using \
         `codex/work`'s old login.",
    );
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![codex_account("work", true)])));
    machine.enrolling = enrolled_as(EnrolledAs::InUse { again: true }, vec![old_login.clone()]);
    signs_in(&mut model, &mut machine, "codex", "work", &[CODEX_ADDRESS]);

    let [last] = &model.state.last_switches[..] else {
        panic!("what the sign-in said: {:#?}", model.state.last_switches);
    };
    assert_eq!(
        (last.provider.as_str(), last.to.as_str()),
        ("codex", "codex/work")
    );
    assert_eq!(
        last.said.as_deref(),
        Some("Signed in to work again. Its new login is the one in use now.")
    );
    assert_eq!(last.restart, None, "nothing switched away from anything");
    assert_eq!(last.follows_at, None);
    assert_eq!(last.warnings, std::slice::from_ref(&old_login));
    assert!(
        !model.state.warnings.contains(&old_login),
        "said once, with the sign-in"
    );
}

/// A browser often signs in to the session it already has, so the account signed in now can be
/// enrolled by a sign-in under a new name. It says it was enrolled, not signed in to again.
#[test]
fn a_first_sign_in_to_the_account_in_use_says_it_was_enrolled() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![codex_account("work", true)])));
    machine.enrolling = enrolled_as(EnrolledAs::InUse { again: false }, Vec::new());
    signs_in(&mut model, &mut machine, "codex", "work", &[]);
    assert_eq!(
        model.state.last_switches[0].said.as_deref(),
        Some("Enrolled work, the account signed in now. Its new login is the one in use.")
    );
}

/// The tool did not switch, so what its last switch said about sessions still using the account
/// it left is still true, and stays: the restart it asked for, and the warning not to sign out
/// inside one. The sign-in's own count of the same sessions, naming this account's old login,
/// would contradict it and is not added.
#[test]
fn a_sign_in_to_the_account_in_use_keeps_what_the_last_switch_said() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![
        codex_account("work", true),
        claude("personal", false, 0.0),
    ])));
    machine.switched = switched(
        "codex",
        "codex/personal",
        "codex/work",
        vec![still_running()],
    );
    model.send(Intent::SwitchTo {
        qualified: "codex/work".into(),
    });
    model.run(&mut machine);

    let old_login = warning("sessions_keep_old_login", "2 sessions");
    let parked = warning("written_on_the_command_line", "on the argument line");
    machine.enrolling = enrolled_as(
        EnrolledAs::InUse { again: true },
        vec![old_login, parked.clone()],
    );
    signs_in(&mut model, &mut machine, "codex", "work", &[]);

    let [last] = &model.state.last_switches[..] else {
        panic!("one tool's: {:#?}", model.state.last_switches);
    };
    assert_eq!(
        last.restart,
        Some(RestartNeeded {
            program: "codex".into(),
            from: "personal".into(),
        })
    );
    assert_eq!(
        last.said.as_deref(),
        Some("Signed in to work again. Its new login is the one in use now.")
    );
    assert_eq!(last.warnings, [still_running(), parked]);
}

/// A sign-in to the account in use that could not tell what still runs Codex says so beside
/// the sign-in, as a count of sessions is said. Where Codex's last switch could not tell
/// either, the sign-in says nothing more: the switch's warning is about the same sessions,
/// and keeps what not to do in them, so the notice says it once.
#[test]
fn a_sign_in_that_could_not_tell_what_runs_says_it_once() {
    let switch_untold = warning(
        "sessions_unknown",
        "Pitboard could not tell whether Codex sessions started before this switch are \
         still running.",
    );
    let sign_in_untold = warning(
        "sessions_unknown",
        "Pitboard could not tell whether Codex sessions started before this sign-in are \
         still running.",
    );

    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![codex_account("work", true)])));
    machine.enrolling = enrolled_as(
        EnrolledAs::InUse { again: true },
        vec![sign_in_untold.clone()],
    );
    signs_in(&mut model, &mut machine, "codex", "work", &[]);
    let [last] = &model.state.last_switches[..] else {
        panic!("what the sign-in said: {:#?}", model.state.last_switches);
    };
    assert_eq!(last.warnings, std::slice::from_ref(&sign_in_untold));

    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![
        codex_account("work", true),
        claude("personal", false, 0.0),
    ])));
    machine.switched = switched(
        "codex",
        "codex/personal",
        "codex/work",
        vec![switch_untold.clone()],
    );
    model.send(Intent::SwitchTo {
        qualified: "codex/work".into(),
    });
    model.run(&mut machine);
    machine.enrolling = enrolled_as(EnrolledAs::InUse { again: true }, vec![sign_in_untold]);
    signs_in(&mut model, &mut machine, "codex", "work", &[]);

    let shown = model.shown();
    let [last] = &model.state.last_switches[..] else {
        panic!("one tool's: {:#?}", model.state.last_switches);
    };
    assert_eq!(last.warnings, std::slice::from_ref(&switch_untold));
    let notice = shown
        .notices
        .iter()
        .find(|notice| notice.id == "switch/codex")
        .unwrap_or_else(|| panic!("the switch's notice: {:#?}", shown.notices));
    assert_eq!(
        notice.lines,
        [
            "Signed in to work again. Its new login is the one in use now.".to_owned(),
            "Any codex session started before this switch keeps using personal until it is \
             quit and started again."
                .to_owned(),
            switch_untold.message,
        ]
    );
}

/// A sign-in that parked its login rather than put it in use says why, after the read that
/// follows it, which would otherwise put the warning away.
#[test]
fn a_sign_in_that_was_parked_says_what_it_warned_about() {
    let untold = warning(
        "sign_in_parked_not_in_use",
        "Codex goes on with the login it has",
    );
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![codex_account("work", true)])));
    machine.enrolling = enrolled_as(EnrolledAs::Renewed, vec![untold.clone()]);
    let id = starts(&mut model, &mut machine, "codex", "work");
    model.give(Answer::SignInQuiet { id });
    model.run_but(&mut machine, any_read);
    assert!(model.state.warnings.is_empty(), "not before the read lands");
    let read = model.take(any_read);
    model.give(machine.answer(read));

    assert_eq!(model.state.warnings, [untold]);
    assert!(model.state.last_switches.is_empty());
}

/// From the sheet a sign-in is started from, a sign-in refused before it started says what that
/// refusal found on the way, such as a switch interrupted earlier and finished now, not only
/// why it was refused. The panel says it too, since what was found is about the machine and
/// outlives the sheet.
#[test]
fn a_refused_sign_in_says_what_it_found_on_the_way() {
    let recovered = warning(
        "interrupted_switch_undone",
        "an earlier switch was interrupted",
    );
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.starting = Err(refusal(
        "sign_in_not_isolated",
        "a sign-in here would write over the login in use",
        vec![recovered.clone()],
    ));
    put_up(&mut model, &mut machine, add(None));
    starts(&mut model, &mut machine, "claude", "work");

    let shown = model.shown();
    let failure = shown.sheet_failure.expect("said in the sheet");
    assert_eq!(
        failure.message,
        "a sign-in here would write over the login in use"
    );
    assert_eq!(failure.warnings, std::slice::from_ref(&recovered.message));
    assert_eq!(model.state.warnings, [recovered]);
}

/// A sign-in of another account adds a row and says nothing more.
#[test]
fn a_sign_in_of_another_account_says_nothing_more() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![codex_account("work", true)])));
    signs_in(
        &mut model,
        &mut machine,
        "codex",
        "personal",
        &[CODEX_ADDRESS],
    );
    let shown = model.shown();
    assert!(model.state.last_switches.is_empty());
    assert!(model.state.warnings.is_empty());
    assert_eq!((&model.state.presented, shown.sheet_failure), (&None, None));
}

/// Whether a code is asked for is the core's to read, in each tool's own words, and its cases
/// are the core's tests. The sheet asks it about the tool being signed in to, and a code typed
/// back is not asked for again. What is typed goes to the tool on the lane of sign-in calls,
/// apart from the actor and from the sign-in's own thread, which waits on the browser.
#[test]
fn a_code_is_asked_for_only_where_the_tool_takes_one() {
    for (provider, takes) in [("claude", true), ("codex", false)] {
        let mut model = Hand::new();
        let mut machine = Stub::reading(Ok(status(Vec::new())));
        let id = starts(&mut model, &mut machine, provider, "work");
        says(&mut model, id, PROMPT);
        assert_eq!(running(&model).wants_code, takes, "{provider}");

        let typed = model.send(paste("abc"));
        if takes {
            assert_eq!(
                typed,
                [Job::PasteCode {
                    id,
                    code: "abc".into()
                }]
            );
            assert_eq!(typed[0].lane(), Lane::SignInCalls);
            assert!(!running(&model).wants_code, "asked once");
        } else {
            assert_eq!(typed, [], "nothing is typed to a tool that reads nothing");
        }
        ends(&mut model, &mut machine, id);
        assert_eq!(model.shown().signing_in, None);
    }
}

/// Cancel stops the tool on the lane of sign-in calls: stopping waits for the tool, and a Codex
/// sign-in waiting on the browser held the whole app while it did. What the stopped tool leaves
/// behind is not a failure to report, and nothing is enrolled.
#[test]
fn a_cancelled_sign_in_stops_the_tool_and_reports_nothing() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    let id = starts(&mut model, &mut machine, "codex", "work");
    says(&mut model, id, CODEX_ADDRESS);
    assert!(running(&model).url.is_some());

    let stop = model.send(Intent::CancelSignIn);
    assert_eq!(stop, [Job::StopSignIn { id }]);
    assert_eq!(stop[0].lane(), Lane::SignInCalls);
    assert_eq!(model.shown().signing_in, None);
    model.run(&mut machine);
    ends(&mut model, &mut machine, id);

    assert_eq!(machine.stopped, [id], "the tool is stopped");
    assert_eq!(machine.over, [(id, false)], "and nothing is enrolled");
    let shown = model.shown();
    assert!(model.state.warnings.is_empty());
    assert_eq!((&model.state.presented, shown.sheet_failure), (&None, None));
    assert_eq!(
        model.count(any_read),
        0,
        "nothing changed, so nothing is read"
    );
}

/// An account whose parked login can no longer be used is signed in to again through the
/// sign-in a new account gets, so the address Codex prints shows the same way. The sheet has
/// the label alone, and the core is given it with its tool once, as for a new account. The
/// row's action comes with what the window says.
#[test]
fn an_account_that_cannot_be_switched_to_is_signed_in_to_again() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![
        codex_account("personal", true),
        codex_account("work", false),
        claude("spare", false, 0.0),
    ])));
    model.refresh(&mut machine);
    let again = Sheet::SignInAgain {
        provider: "codex".into(),
        label: "work".into(),
    };
    put_up(&mut model, &mut machine, again);
    let id = starts(&mut model, &mut machine, "codex", "work");
    says(&mut model, id, CODEX_ADDRESS);
    let shown = running(&model);
    assert_eq!(machine.signed_in, ["codex/work"]);
    let signing = model.state.signing().expect("a sign-in");
    assert_eq!(
        (signing.provider.as_str(), signing.name.as_str()),
        ("codex", "work")
    );
    assert!(shown.url.is_some());
    ends(&mut model, &mut machine, id);
    assert_eq!(machine.over, [(id, true)]);
    assert_eq!(model.shown().signing_in, None);
    assert_eq!(model.shown().sheet, None);

    let spare = Sheet::SignInAgain {
        provider: "claude".into(),
        label: "spare".into(),
    };
    put_up(&mut model, &mut machine, spare);
    signs_in(&mut model, &mut machine, "claude", "spare", &[]);
    assert_eq!(machine.signed_in, ["codex/work", "claude/spare"]);
}

/// One sign-in at a time. A second asked for while one runs would take the place of the one on
/// screen and leave the first tool waiting on a browser with nothing to stop it.
#[test]
fn a_second_sign_in_while_one_runs_starts_nothing() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    let first = starts(&mut model, &mut machine, "codex", "work");
    says(&mut model, first, CODEX_ADDRESS);

    assert_eq!(model.send(sign_in("codex", "home")), []);
    let shown = running(&model);
    assert_eq!(
        (shown.id, model.state.signing().map(|s| s.name.as_str())),
        (first, Some("work"))
    );
    ends(&mut model, &mut machine, first);
    assert_eq!(machine.signed_in, ["codex/work"]);
    assert_eq!(model.shown().signing_in, None);
}

/// Cancel can be pressed before the tool has started. What then starts is stopped rather than
/// watched, and nothing is enrolled or said.
#[test]
fn a_sign_in_cancelled_while_it_starts_stops_what_started() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    model.send(sign_in("codex", "work"));
    let start = model.next();
    let Job::SignIn { id, .. } = start else {
        panic!("the sign-in asked for: {start:?}");
    };
    assert!(model.shown().signing_in.is_some());

    assert_eq!(model.send(Intent::CancelSignIn), [], "nothing has started");
    assert_eq!(model.shown().signing_in, None);
    let stop = model.give(machine.answer(start));
    assert_eq!(stop, [Job::StopSignIn { id }], "what started is stopped");
    model.run(&mut machine);
    says(&mut model, id, CODEX_ADDRESS);
    ends(&mut model, &mut machine, id);

    assert_eq!(machine.over, [(id, false)]);
    let shown = model.shown();
    assert_eq!(shown.signing_in, None);
    assert!(model.state.warnings.is_empty());
    assert!(model.state.last_switches.is_empty());
    assert_eq!((&model.state.presented, shown.sheet_failure), (&None, None));
}

fn a_sign_in(job: &Job) -> bool {
    matches!(job, Job::SignIn { .. })
}

/// Cancel and then Sign In at once, as a person can press one after the other. The sign-in
/// cancelled holds the core's one sign-in at a time until its tool has stopped, so the new one
/// is shown as starting meanwhile, in the sheet it was started from, and asked for only once
/// the cancelled one has let go: as the stop on the lane of sign-in calls answers, where it
/// stopped the tool, or else once its own thread has answered last. Asked for at once, it
/// started on a thread of its own before the stop had run, and was refused as a sign-in
/// already waiting. Asked for only once that thread had answered, it waited as long as a
/// program the tool started held the tool's output open, as `codex login` does once Codex's
/// npm launcher is killed.
#[test]
fn a_sign_in_asked_for_after_a_cancel_starts_once_the_cancelled_one_has_stopped() {
    for stops_its_tool in [true, false] {
        let case = if stops_its_tool {
            "the stop stopped its tool"
        } else {
            "it had gone quiet before the stop"
        };
        let mut model = Hand::new();
        let mut machine = Stub::reading(Ok(status(Vec::new())));
        put_up(&mut model, &mut machine, add(None));
        let first = starts(&mut model, &mut machine, "claude", "work");
        says(&mut model, first, PROMPT);

        assert_eq!(
            model.send(Intent::CancelSignIn),
            [Job::StopSignIn { id: first }],
            "{case}"
        );
        assert_eq!(
            model.send(sign_in("claude", "work")),
            [],
            "{case}: nothing asked for while the first is stopping"
        );
        let waiting = running(&model);
        assert_ne!(waiting.id, first);
        assert_eq!(
            (waiting.url, waiting.wants_code),
            (None, false),
            "{case}: shown as starting"
        );
        assert_eq!(model.shown().sheet, Some(add(None)), "{case}");
        assert_eq!(
            model.send(paste("early#code")),
            [],
            "{case}: nothing to type to yet"
        );

        let stop = model.next();
        assert_eq!(stop, Job::StopSignIn { id: first }, "{case}");
        let asked_for = Job::SignIn {
            id: waiting.id,
            qualified: "claude/work".into(),
        };
        if stops_its_tool {
            // As the lane answers once it has stopped the tool and waited for it. The
            // thread may go on reading what the tool said for as long as its output is open.
            assert_eq!(
                model.give(Answer::SignInStopped { id: first }),
                [asked_for],
                "{case}: asked for as the stop answers"
            );
            model.run(&mut machine);
            says(&mut model, waiting.id, PROMPT);
            assert!(running(&model).wants_code, "{case}");
            assert_eq!(
                model.give(Answer::SignInQuiet { id: first }),
                [Job::SignInOver {
                    id: first,
                    enrol: false
                }],
                "{case}"
            );
            model.run(&mut machine);
            assert_eq!(model.count(a_sign_in), 2, "{case}: nothing asked for again");
            assert_eq!(running(&model).id, waiting.id, "{case}");
            assert!(
                running(&model).wants_code,
                "{case}: the new one left as it was"
            );
        } else {
            // As the lane answers where the tool had stopped saying anything first: the
            // thread sees to the tool, and its last answer is what lets the next start.
            assert_eq!(
                model.give(machine.answer(stop)),
                [],
                "{case}: a stop that stopped nothing is not over"
            );
            assert_eq!(
                model.give(Answer::SignInQuiet { id: first }),
                [Job::SignInOver {
                    id: first,
                    enrol: false
                }],
                "{case}"
            );
            let over = model.next();
            assert_eq!(
                model.give(machine.answer(over)),
                [asked_for],
                "{case}: asked for once the first's thread is over"
            );
            model.run(&mut machine);
            says(&mut model, waiting.id, PROMPT);
            assert!(running(&model).wants_code, "{case}");
        }
        assert_eq!(machine.signed_in, ["claude/work", "claude/work"], "{case}");
        assert_eq!(machine.over, [(first, false)], "{case}");
        let shown = model.shown();
        assert_eq!(
            (model.state.presented, shown.sheet_failure),
            (None, None),
            "{case}"
        );
    }
}

/// However a sign-in cancelled before it lets go of the core's one sign-in at a time, that is
/// what lets the one asked for since start: its tool failing to start, or starting and then
/// being stopped as it starts, said by the stop where it stopped the tool and by its thread
/// otherwise, or its start or its finish lost to a panic, or what it signed in to enrolled
/// too late to stop. Its thread answering last after the stop said so asks for nothing more.
/// A sign-in cancelled while it waits is never asked for, and once nothing is left of the one
/// before it the next starts at once.
#[test]
fn a_sign_in_waiting_on_a_cancelled_one_starts_once_that_one_has_let_go() {
    type Ends = fn(&mut Hand, &mut Stub, u64);
    let cases: [(&str, Ends); 6] = [
        ("could not start", |model, _, id| {
            model.give(Answer::SignInStarted {
                id,
                started: Err(refusal("sign_in_incomplete", "no", Vec::new()).error()),
                put_back: None,
            });
        }),
        (
            "stopped as it started, by its thread",
            |model, machine, id| {
                model.give(Answer::SignInStarted {
                    id,
                    started: Ok(()),
                    put_back: None,
                });
                model.run(machine);
                assert_eq!(machine.stopped, [id]);
                model.give(Answer::SignInQuiet { id });
                model.run(machine);
            },
        ),
        (
            "stopped as it started, by the stop",
            |model, machine, id| {
                model.give(Answer::SignInStarted {
                    id,
                    started: Ok(()),
                    put_back: None,
                });
                assert_eq!(
                    model.take(|job| matches!(job, Job::StopSignIn { .. })),
                    Job::StopSignIn { id }
                );
                model.give(Answer::SignInStopped { id });
                model.run_but(machine, a_sign_in);
                model.give(Answer::SignInQuiet { id });
                model.run_but(machine, a_sign_in);
            },
        ),
        ("its start lost", |model, _, id| {
            model.give(Answer::Lost(Job::SignIn {
                id,
                qualified: "claude/one".into(),
            }));
        }),
        ("its finish lost", |model, _, id| {
            model.give(Answer::SignInStarted {
                id,
                started: Ok(()),
                put_back: None,
            });
            model.give(Answer::Lost(Job::SignInOver { id, enrol: true }));
        }),
        ("enrolled too late to stop", |model, machine, id| {
            model.give(Answer::SignInStarted {
                id,
                started: Ok(()),
                put_back: None,
            });
            model.give(Answer::SignInFinished {
                id,
                done: machine.enrolling.clone().map_err(|refused| refused.error()),
            });
            model.run_but(machine, a_sign_in);
        }),
    ];
    for (case, ends) in cases {
        let mut model = Hand::new();
        let mut machine = Stub::reading(Ok(status(Vec::new())));
        model.send(sign_in("claude", "one"));
        let Job::SignIn { id: first, .. } = model.next() else {
            panic!("{case}: the first sign-in asked for");
        };
        model.send(Intent::CancelSignIn);
        assert_eq!(model.send(sign_in("claude", "two")), [], "{case}");
        let second = running(&model).id;
        ends(&mut model, &mut machine, first);
        let asked: Vec<&Job> = model.asked.iter().filter(|job| a_sign_in(job)).collect();
        assert_eq!(
            asked,
            [
                &Job::SignIn {
                    id: first,
                    qualified: "claude/one".into()
                },
                &Job::SignIn {
                    id: second,
                    qualified: "claude/two".into()
                }
            ],
            "{case}"
        );
        assert_eq!(running(&model).id, second, "{case}");
        assert_eq!(
            model.state.presented, None,
            "{case}: nothing said of the first"
        );
    }

    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    let first = starts(&mut model, &mut machine, "claude", "one");
    model.send(Intent::CancelSignIn);
    model.send(sign_in("claude", "two"));
    model.send(Intent::CancelSignIn);
    assert_eq!(model.shown().signing_in, None);
    model.run(&mut machine);
    ends(&mut model, &mut machine, first);
    assert_eq!(model.count(a_sign_in), 1, "the second was never asked for");
    assert_eq!(model.send(sign_in("claude", "three")).len(), 1, "at once");
}

/// A sign-in that finishes closes the sheet it was started from. A sheet up by then would be
/// somebody else's, such as a name half typed, and closing it threw that away. Here no other
/// sheet can be up while a sign-in runs, since it keeps its own and nothing puts one up any
/// other way, as the Swift test's direct assignment did: so one closed meanwhile stays closed,
/// and the next is put up once the sign-in is over.
#[test]
fn a_finished_sign_in_closes_only_the_sheet_it_started_from() {
    let rename = Sheet::Rename {
        provider: "claude".into(),
        label: "home".into(),
    };
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    put_up(&mut model, &mut machine, add(Some("codex")));
    let id = starts(&mut model, &mut machine, "codex", "work");
    says(&mut model, id, CODEX_ADDRESS);

    put_up(&mut model, &mut machine, rename.clone());
    assert_eq!(model.shown().sheet, Some(add(Some("codex"))));
    model.send(Intent::CloseSheet);
    put_up(&mut model, &mut machine, rename.clone());
    assert_eq!(model.shown().sheet, None);

    ends(&mut model, &mut machine, id);
    assert_eq!(machine.over, [(id, true)]);
    assert_eq!(model.shown().signing_in, None);
    assert_eq!(model.shown().sheet, None);
    put_up(&mut model, &mut machine, rename.clone());
    assert_eq!(model.shown().sheet, Some(rename));
}

/// Cancel pressed while what the tool signed in to is being enrolled comes too late to stop it.
/// It is enrolled all the same, so the accounts are read again, and nothing else is touched:
/// the sign-in and the sheet on screen by then are somebody else's, a sign-in started since
/// among them. Where the cancel reached the tool first and finishing fails as stopped, that was
/// somebody's answer and says nothing.
#[test]
fn a_sign_in_cancelled_while_it_finishes_leaves_what_came_after_it() {
    for stopped_first in [false, true] {
        let travel = codex_account("travel", false);
        let mut model = Hand::new();
        let mut machine = Stub::reading(Ok(status(vec![codex_account("work", true)])));
        put_up(&mut model, &mut machine, add(Some("codex")));
        let late = starts(&mut model, &mut machine, "codex", "travel");
        model.give(Answer::SignInQuiet { id: late });
        let finishing = model.take(sign_in_over);
        assert_eq!(
            finishing,
            Job::SignInOver {
                id: late,
                enrol: true
            }
        );
        assert_eq!(
            model.send(Intent::CancelSignIn),
            [Job::StopSignIn { id: late }]
        );
        model.run(&mut machine);

        put_up(&mut model, &mut machine, add(Some("claude")));
        assert_eq!(
            model.send(sign_in("claude", "other")),
            [],
            "not while the one finishing holds the core's one sign-in at a time"
        );
        let next = running(&model).id;
        let reads = model.count(any_read);
        machine.answer = Ok(status(vec![codex_account("work", true), travel.clone()]));
        let done = if stopped_first {
            Err(refusal(
                "sign_in_gone",
                "The sign-in is no longer running.",
                vec![warning("interrupted_switch_undone", "A switch was undone.")],
            ))
        } else {
            enrolled_as(EnrolledAs::SignedIn, Vec::new())
        };
        model.give(Answer::SignInFinished {
            id: late,
            done: done.map_err(|refused| refused.error()),
        });
        model.run(&mut machine);
        assert_eq!(
            machine.signed_in,
            ["codex/travel", "claude/other"],
            "asked for once the one finishing is over"
        );

        let shown = model.shown();
        let still = shown
            .signing_in
            .as_ref()
            .expect("the sign-in started since");
        assert_eq!(
            (still.id, model.state.signing().map(|s| s.name.as_str())),
            (next, Some("other"))
        );
        assert_eq!(shown.sheet, Some(add(Some("claude"))), "{stopped_first}");
        assert!(model.state.warnings.is_empty(), "{stopped_first}");
        assert_eq!((&model.state.presented, shown.sheet_failure), (&None, None));
        if stopped_first {
            assert_eq!(
                model.count(any_read),
                reads,
                "nothing was enrolled, so nothing is read"
            );
        } else {
            assert_eq!(model.count(any_read), reads + 1);
            assert!(
                model
                    .state
                    .status
                    .clone()
                    .expect("accounts")
                    .accounts
                    .contains(&travel),
                "enrolled all the same"
            );
        }
    }
}

/// A read that started before this app's sign-in finished lands after it with who was signed in
/// before, and is dropped, as `switching.rs` has it for the other changes.
#[test]
fn a_read_that_started_before_a_sign_in_is_dropped_when_it_lands() {
    a_read_that_started_before(|model, machine| {
        signs_in(model, machine, "codex", "travel", &[]);
        assert_eq!(machine.signed_in, ["codex/travel"]);
    });
}

/// `changing.rs`'s look landing after a change, for what a sign-in enrols: the look finds the
/// account index as the enrolment wrote it, and lands after the sign-in has finished and
/// before the read after it, which it dropped. The Codex half of the fixture's
/// `accounts_are_added_through_each_tools_sign_in` met it, cancelling nothing.
#[test]
fn a_look_landing_after_a_sign_in_enrols_leaves_the_read_after_it() {
    let after = vec![
        codex_account("personal", true),
        codex_account("spare", false),
        codex_account("travel", false),
    ];
    a_look_landing_after(after, |model, machine| {
        model.send(sign_in("codex", "travel"));
        model.run_but(machine, a_look_or_a_read);
        let id = running(model).id;
        says(model, id, CODEX_ADDRESS);
        model.give(Answer::SignInQuiet { id });
        model.run_but(machine, a_look_or_a_read);
        assert_eq!(machine.over, [(id, true)]);
        assert_eq!(model.shown().signing_in, None, "finished");
    });
}

/// The menu stays usable while the window has a sheet up, and its Add Account… or an item that
/// names an account puts up a sheet. Over a running sign-in that replaced the sheet that could
/// finish or stop it, and left the tool waiting on a browser with nothing on screen. The
/// sign-in keeps its sheet, and the window is still brought forward on the accounts pane.
#[test]
fn a_running_sign_in_keeps_its_sheet() {
    let rename = Sheet::Rename {
        provider: "claude".into(),
        label: "personal".into(),
    };
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![claude("work", true, 0.0)])));
    model.refresh(&mut machine);
    put_up(&mut model, &mut machine, add(None));
    let id = starts(&mut model, &mut machine, "claude", "travel");
    says(&mut model, id, PROMPT);
    assert!(running(&model).wants_code);

    let requests = model.shown().window_request.serial;
    put_up(
        &mut model,
        &mut machine,
        Sheet::Name {
            provider: "codex".into(),
            email: "someone@example.com".into(),
        },
    );
    put_up(&mut model, &mut machine, rename.clone());
    let shown = model.shown();
    assert_eq!(shown.sheet, Some(add(None)));
    assert_eq!(
        shown.window_request,
        WindowRequest {
            serial: requests + 2,
            pane: Some(Pane::Accounts),
            settings: false,
        }
    );

    model.send(Intent::CancelSignIn);
    model.run(&mut machine);
    ends(&mut model, &mut machine, id);
    put_up(&mut model, &mut machine, rename.clone());
    assert_eq!(model.shown().sheet, Some(rename), "once it is over");
}

/// The sheet over the window, the tool's sign-in with the code it asks for typed back, and the
/// new account parked beside the one in use once the sheet has closed by itself, on the model's
/// own state. `threaded.rs` adds one through the real core and a stand-in for `claude`, and the
/// tool the sheet starts on comes with what the window says.
#[test]
fn an_account_is_added_through_the_sheet_from_start_to_finish() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![claude("work", true, 0.0)])));
    model.refresh(&mut machine);
    put_up(&mut model, &mut machine, add(None));
    assert_eq!(model.shown().window_request.serial, 1);

    let id = starts(&mut model, &mut machine, "claude", "travel");
    says(
        &mut model,
        id,
        "Opening browser to sign in\u{2026}\nIf the browser didn't open, visit: \
         https://claude.ai/oauth/authorize?fixture=1\n",
    );
    says(&mut model, id, PROMPT);
    let shown = running(&model);
    assert!(shown.wants_code);
    assert_eq!(
        shown.url.as_deref(),
        Some("https://claude.ai/oauth/authorize?fixture=1")
    );
    assert_eq!(model.shown().sheet, Some(add(None)));

    model.send(paste("fixture-code"));
    machine.answer = Ok(status(vec![
        claude("work", true, 0.0),
        claude("travel", false, 0.0),
    ]));
    model.run(&mut machine);
    ends(&mut model, &mut machine, id);

    assert_eq!(machine.pasted, [(id, "fixture-code".to_owned())]);
    let shown = model.shown();
    assert_eq!(shown.signing_in, None);
    assert_eq!(shown.sheet, None);
    let accounts = model.state.status.clone().expect("accounts").accounts;
    let travel = accounts
        .iter()
        .find(|account| account.qualified.as_deref() == Some("claude/travel"))
        .expect("travel enrolled");
    assert!(!travel.signed_in);
    assert!(travel.switchable);
    assert!(model.state.warnings.is_empty());
}

/// Signing in again to the account in use, from its row, keeps it the account in use and says
/// its new login is the one in use now, as a sign-in through the core does.
#[test]
fn signing_in_again_to_the_account_in_use_says_it_has_a_new_login() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![claude("work", true, 0.0)])));
    model.refresh(&mut machine);
    let again = Sheet::SignInAgain {
        provider: "claude".into(),
        label: "work".into(),
    };
    put_up(&mut model, &mut machine, again);
    machine.enrolling = enrolled_as(EnrolledAs::InUse { again: true }, Vec::new());
    let id = starts(&mut model, &mut machine, "claude", "work");
    says(&mut model, id, PROMPT);
    model.send(paste("fixture-code"));
    model.run(&mut machine);
    ends(&mut model, &mut machine, id);

    let shown = model.shown();
    assert_eq!(shown.sheet, None);
    let last = model
        .state
        .last_switches
        .first()
        .expect("what the sign-in said");
    assert_eq!(
        (last.provider.as_str(), last.to.as_str()),
        ("claude", "work")
    );
    assert_eq!(
        last.said.as_deref(),
        Some("Signed in to work again. Its new login is the one in use now.")
    );
    let work = &model.state.status.clone().expect("accounts").accounts[0];
    assert!(work.signed_in);
    assert!(!work.switchable);
}

/// Opening the form for another account asks again what is installed: the first answer may have
/// come while the person's login shell was too slow to say, and the core asks it once more when
/// that was so. Naming the account in use asks nothing, nor does a read once something is
/// found. What the sheet offers comes with what the window says.
#[test]
fn what_is_installed_is_asked_again_when_the_form_for_another_account_opens() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.found = vec![claude_code()];
    model.refresh(&mut machine);
    assert_eq!(model.state.installed, Some(vec![claude_code()]));

    machine.found = vec![claude_code(), codex()];
    assert_eq!(model.send(present(add(None))), [Job::AskInstalled]);
    model.run(&mut machine);
    assert_eq!(model.state.installed, Some(vec![claude_code(), codex()]));
    assert_eq!(model.count(installed_ask), 2);
    assert_eq!(
        model.send(present(add(Some("codex")))),
        [],
        "not for one put up in its place"
    );

    model.send(Intent::CloseSheet);
    put_up(
        &mut model,
        &mut machine,
        Sheet::Name {
            provider: "claude".into(),
            email: "a@example.com".into(),
        },
    );
    model.refresh(&mut machine);
    assert_eq!(
        model.count(installed_ask),
        2,
        "not for naming the account in use, nor for a read"
    );

    model.send(Intent::CloseSheet);
    let again = Sheet::SignInAgain {
        provider: "codex".into(),
        label: "work".into(),
    };
    assert_eq!(
        model.send(present(again)),
        [Job::AskInstalled],
        "signing in again needs the tool as much"
    );
}

/// A sheet is over the main window, so putting one up opens the window too, on the accounts the
/// sheet is about. Opening the window by itself comes with what the window says.
#[test]
fn a_sheet_is_put_up_over_the_window() {
    let rename = Sheet::Rename {
        provider: "claude".into(),
        label: "personal".into(),
    };
    let mut model = Hand::new();
    assert_eq!(model.shown().window_request.serial, 0);
    model.send(present(rename.clone()));
    let shown = model.shown();
    assert_eq!(shown.sheet, Some(rename));
    assert_eq!(
        shown.window_request,
        WindowRequest {
            serial: 1,
            pane: Some(Pane::Accounts),
            settings: false,
        }
    );
}

// What the Swift model did not do.

/// After Claude Code refuses a code typed back, the person may paste one again: the owner's
/// decision. Claude Code 2.1.289 refuses a line that is not the whole code with `Invalid
/// code.` and goes on reading in the same sign-in, as the register's
/// `sign_in_takes_another_code` holds, so the field is offered again for the same sign-in,
/// saying the code was refused, and the next code goes to the same tool. The Swift model took
/// any code typed back as the one, and never asked again, leaving the person a sign-in that
/// could only be cancelled.
#[test]
fn a_code_claude_code_refused_may_be_pasted_again() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    let id = starts(&mut model, &mut machine, "claude", "travel");
    says(&mut model, id, PROMPT);
    assert_eq!(
        model.send(paste("half")),
        [Job::PasteCode {
            id,
            code: "half".into()
        }]
    );
    let shown = running(&model);
    assert!(!shown.wants_code && !code_refused(&model));

    says(&mut model, id, REFUSED);
    let shown = running(&model);
    assert!(shown.wants_code, "asked again");
    assert!(code_refused(&model));

    let typed = model.send(paste("the-code#the-state"));
    assert_eq!(
        typed,
        [Job::PasteCode {
            id,
            code: "the-code#the-state".into()
        }],
        "to the same sign-in"
    );
    let shown = running(&model);
    assert!(
        !shown.wants_code && !code_refused(&model),
        "nothing said of it yet"
    );
    model.run(&mut machine);
    ends(&mut model, &mut machine, id);
    assert_eq!(
        machine.signed_in,
        ["claude/travel"],
        "one sign-in throughout"
    );
    assert_eq!(machine.over, [(id, true)]);
    assert_eq!(model.shown().signing_in, None);
}

/// What the tool said before a code was typed back says nothing of that code: a refusal of
/// the one before is not a refusal of the next.
#[test]
fn only_what_the_tool_says_after_a_code_refuses_it() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    let id = starts(&mut model, &mut machine, "claude", "travel");
    says(&mut model, id, PROMPT);
    model.send(paste("one"));
    says(&mut model, id, "Invalid ");
    assert!(
        !running(&model).wants_code,
        "not until the whole of it has come"
    );
    says(&mut model, id, REFUSED.trim_start_matches("Invalid "));
    model.send(paste("two"));
    says(&mut model, id, "Login successful.\n");
    let shown = running(&model);
    assert!(!shown.wants_code && !code_refused(&model));
}

/// A name and a code are taken without the white space around them, as the Swift sheets took
/// it away with Foundation's `whitespacesAndNewlines`, which holds U+200B ZERO WIDTH SPACE as
/// well as Unicode's White_Space, as measured on macOS 27.0; one with nothing else in it
/// starts or types nothing.
#[test]
fn a_name_and_a_code_are_trimmed_as_the_sheet_trimmed_them() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    assert_eq!(model.send(sign_in("claude", " \u{200b}\n")), []);
    assert_eq!(
        model.send(sign_in("claude", "\u{3000}travel\u{200b} ")),
        [Job::SignIn {
            id: 1,
            qualified: "claude/travel".into()
        }]
    );
    assert_eq!(
        model.state.signing().map(|s| s.name.as_str()),
        Some("travel")
    );
    model.run(&mut machine);
    says(&mut model, 1, PROMPT);
    assert_eq!(model.send(paste(" \t\u{200b}")), []);
    assert_eq!(
        model.send(paste("\u{200b}abc#def\n")),
        [Job::PasteCode {
            id: 1,
            code: "abc#def".into()
        }]
    );
}

/// A code is typed only to a tool that has started and asks for one: before then there is
/// nothing to type to, and a code typed again before the tool has said anything of the last
/// one is the same code twice, which the sheet's field, gone once it is typed, never sent.
#[test]
fn a_code_is_typed_only_where_the_tool_asks_for_one() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    model.send(sign_in("claude", "travel"));
    assert_eq!(model.send(paste("early")), [], "not started");
    model.run(&mut machine);
    assert_eq!(model.send(paste("early")), [], "not asked for yet");
    says(&mut model, 1, PROMPT);
    assert_eq!(model.send(paste("abc")).len(), 1);
    assert_eq!(model.send(paste("abc")), [], "typed once");
    model.run(&mut machine);
    assert_eq!(machine.pasted, [(1, "abc".to_owned())]);
}

/// A sign-in that fails once the sheet it was started from has gone, or one started where no
/// sheet was up, says so in the window, which is opened for it: something asked for that did
/// not happen is said somewhere. The Swift model handed it back to the sheet, which had gone,
/// so it was said nowhere.
#[test]
fn a_sign_in_that_fails_once_its_sheet_has_gone_says_so_in_the_window() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    put_up(&mut model, &mut machine, add(Some("codex")));
    let id = starts(&mut model, &mut machine, "codex", "work");
    says(&mut model, id, CODEX_ADDRESS);
    model.send(Intent::CloseSheet);
    machine.enrolling = Err(refusal(
        "sign_in_incomplete",
        "The sign-in did not finish.",
        Vec::new(),
    ));
    let requests = model.shown().window_request.serial;
    ends(&mut model, &mut machine, id);

    let shown = model.shown();
    let failure = model.state.presented.clone().expect("said in the window");
    assert_eq!(failure.title, "Couldn’t sign in to work");
    assert_eq!(failure.message, "The sign-in did not finish.");
    assert_eq!(
        shown.window_request,
        WindowRequest {
            serial: requests + 1,
            pane: None,
            settings: false,
        }
    );
    assert_eq!((shown.sheet, shown.sheet_failure), (None, None));

    machine.starting = Err(refusal("claude_program_missing", "no claude", Vec::new()));
    starts(&mut model, &mut machine, "claude", "other");
    let failure = model.state.presented.clone().expect("said in the window");
    assert_eq!(
        (failure.id, failure.title.as_str()),
        (2, "Couldn’t sign in to other")
    );
}

/// A sign-in whose tool stopped before it finished says so in the sheet it was started
/// from, with the name still in it, and the panel says what it warned of. Nothing was
/// enrolled, so nothing is read.
#[test]
fn a_sign_in_that_does_not_finish_says_so_in_its_sheet() {
    let undone = warning("interrupted_switch_undone", "A switch was undone.");
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.enrolling = Err(refusal(
        "sign_in_incomplete",
        "The sign-in did not finish.",
        vec![undone.clone()],
    ));
    put_up(&mut model, &mut machine, add(Some("codex")));
    signs_in(&mut model, &mut machine, "codex", "work", &[CODEX_ADDRESS]);

    let shown = model.shown();
    assert_eq!(shown.signing_in, None);
    assert_eq!(shown.sheet, Some(add(Some("codex"))));
    let failure = shown.sheet_failure.expect("said in the sheet");
    assert_eq!(failure.message, "The sign-in did not finish.");
    assert_eq!(model.state.warnings, [undone]);
    assert_eq!(model.state.presented, None);
    assert_eq!(model.count(any_read), 0);
}

/// What went wrong in a sheet goes with it: closed, replaced by another, or signed in from
/// again. It is numbered with what is said in the window, so an app tells each failure from
/// one it has shown.
#[test]
fn what_went_wrong_in_a_sheet_goes_with_it() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.switched = Err(refusal("nothing_parked", "nothing parked", Vec::new()));
    model.send(Intent::SwitchTo {
        qualified: "claude/spare".into(),
    });
    model.run(&mut machine);
    assert_eq!(model.state.presented.clone().map(|f| f.id), Some(1));

    machine.starting = Err(refusal("sign_in_not_isolated", "not here", Vec::new()));
    put_up(&mut model, &mut machine, add(None));
    starts(&mut model, &mut machine, "claude", "work");
    assert_eq!(model.shown().sheet_failure.map(|f| f.id), Some(2));
    put_up(&mut model, &mut machine, add(None));
    assert!(model.shown().sheet_failure.is_some(), "the same sheet");

    starts(&mut model, &mut machine, "claude", "work");
    assert_eq!(model.shown().sheet_failure.map(|f| f.id), Some(3));
    machine.starting = Ok(());
    model.send(sign_in("claude", "work"));
    assert_eq!(model.shown().sheet_failure, None, "signed in from again");
    model.run(&mut machine);
    let id = running(&model).id;
    model.send(Intent::CancelSignIn);
    model.run(&mut machine);
    ends(&mut model, &mut machine, id);

    machine.starting = Err(refusal("sign_in_not_isolated", "not here", Vec::new()));
    starts(&mut model, &mut machine, "claude", "work");
    put_up(&mut model, &mut machine, add(Some("codex")));
    assert_eq!(model.shown().sheet_failure, None, "another sheet");
    starts(&mut model, &mut machine, "codex", "work");
    model.send(Intent::CloseSheet);
    let shown = model.shown();
    assert_eq!((shown.sheet, shown.sheet_failure), (None, None));
}

/// A sign-in whose start or finish stopped with a panic is over, said as a failure of its
/// own in its sheet, since what its tool did is not known.
#[test]
fn a_sign_in_lost_to_a_panic_is_said_and_over() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    put_up(&mut model, &mut machine, add(None));
    model.send(sign_in("claude", "work"));
    let start = model.next();
    assert_eq!(model.give(Answer::Lost(start)), []);
    let shown = model.shown();
    assert_eq!(shown.signing_in, None);
    let failure = shown.sheet_failure.expect("said in the sheet");
    assert_eq!(failure.title, "Couldn’t sign in to work");
    assert_eq!(model.sheet_failure_code(), None);

    let id = starts(&mut model, &mut machine, "claude", "work");
    model.give(Answer::SignInQuiet { id });
    let over = model.take(sign_in_over);
    assert_eq!(model.give(Answer::Lost(over)), []);
    let shown = model.shown();
    assert_eq!(shown.signing_in, None);
    assert_eq!(shown.sheet_failure.map(|f| f.id), Some(2));
}

/// Every sign-in's thread is told once whether to enrol, once its tool has stopped saying
/// anything, and only the sign-in still under way is: one cancelled, or that has already
/// failed to start, is never enrolled.
#[test]
fn each_sign_in_is_told_once_whether_to_enrol() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    let first = starts(&mut model, &mut machine, "claude", "one");
    ends(&mut model, &mut machine, first);
    let second = starts(&mut model, &mut machine, "claude", "two");
    model.send(Intent::CancelSignIn);
    model.run(&mut machine);
    ends(&mut model, &mut machine, second);
    assert_eq!(machine.over, [(first, true), (second, false)]);
    assert_eq!(
        model.give(Answer::SignInSaid {
            id: second,
            text: PROMPT.into(),
            step: None,
        }),
        [],
        "a sign-in no longer under way says nothing more"
    );
}

// Signing in inside an app: Claude Desktop's.

pub(super) fn adding_in_claude(open: bool) -> (Hand, Stub) {
    let mut model = Hand::new();
    let mut machine = desktop_machine(open);
    model.refresh(&mut machine);
    put_up(&mut model, &mut machine, add(Some("desktop")));
    (model, machine)
}

pub(super) fn travel() -> Intent {
    sign_in("desktop", "travel")
}

fn a_quit(job: &Job) -> bool {
    matches!(job, Job::Quit { .. })
}

// The quit question asked for an add, answered as its confirm button answers it.
fn quit_for_it(model: &mut Hand, machine: &mut Stub) {
    model.send(travel());
    model.run(machine);
    let question = model.shown().quit_question.expect("asked first");
    model.send(Intent::QuitAndSwitch {
        qualified: question.qualified,
    });
    model.run(machine);
}

fn page(model: &Hand) -> crate::present::SigningInText {
    model.on_a_mac().signing_in_text.expect("the waiting page")
}

#[test]
fn an_add_for_a_tool_that_signs_in_inside_its_app_asks_before_it_quits_the_app() {
    let (mut model, mut machine) = adding_in_claude(true);
    let jobs = model.send(travel());
    assert_eq!(
        jobs,
        [Job::Holding {
            qualified: "desktop/travel".into(),
            then: After::SignIn {
                provider: "desktop".into(),
                name: "travel".into(),
                from: Some(add(Some("desktop"))),
            },
        }]
    );
    assert!(model.on_a_mac().sheet_text.expect("the form").saving);
    model.run(&mut machine);
    let shown = model.on_a_mac();
    assert_eq!(
        shown.quit_question,
        Some(QuitQuestion {
            qualified: "desktop/travel".into(),
            app_id: CLAUDE.into(),
            name: "Claude".into(),
        })
    );
    assert_eq!(
        shown.quit_confirmation.expect("asked").title,
        "Quit Claude to add an account?"
    );
    assert_eq!(shown.signing_in, None, "nothing parked before the answer");
    assert_eq!(model.state.switch_under_way(), None);
    assert!(machine.signed_in.is_empty());
    assert!(machine.apps.asked().is_empty());
}

#[test]
fn the_app_is_opened_in_front_once_the_account_in_use_is_parked() {
    let (mut model, mut machine) = adding_in_claude(true);
    machine.put_back = Some("desktop/work".into());
    quit_for_it(&mut model, &mut machine);
    assert_eq!(machine.signed_in, ["desktop/travel"]);
    assert_eq!(
        machine.apps.asked(),
        [format!("quit {CLAUDE}"), format!("open-front {CLAUDE}")]
    );
    assert!(model.asked.iter().any(|job| *job
        == Job::Open {
            location: StandInApps::copy_of(CLAUDE),
            tool: "desktop".into(),
            front: true,
        }));
    let page = page(&model);
    assert_eq!(page.waiting, "Waiting for Claude to quit");
    assert!(!page.offers_link, "Claude has no page to offer");
    assert!(
        page.message
            .starts_with("In Claude, sign in to the account you’re adding.")
    );
}

#[test]
fn a_park_refused_opens_the_app_again_behind() {
    let (mut model, mut machine) = adding_in_claude(true);
    machine.starting = Err(refusal(
        "login_in_doubt",
        "Pitboard recorded this Claude login as work’s, and Claude names an account you haven’t \
         added for it.",
        Vec::new(),
    ));
    quit_for_it(&mut model, &mut machine);
    assert_eq!(
        machine.apps.asked(),
        [format!("quit {CLAUDE}"), format!("open {CLAUDE}")]
    );
    let shown = model.on_a_mac();
    assert_eq!(shown.signing_in, None);
    assert_eq!(
        shown.sheet_failure.expect("said in the sheet").title,
        "Couldn’t sign in to travel"
    );
    assert_eq!(
        shown.sheet_text.expect("the form").confirm,
        "Add the Account Claude Shows"
    );
    machine.changed += 1;
    model.notice(&mut machine);
    assert_eq!(
        model.count(super::testing::machine::offline_read),
        1,
        "a refused park lets the index go"
    );
}

#[test]
fn cancelling_while_parking_stops_what_started_and_opens_claude_behind() {
    let (mut model, mut machine) = adding_in_claude(true);
    machine.put_back = Some("desktop/work".into());
    model.send(travel());
    model.run(&mut machine);
    let question = model.shown().quit_question.expect("asked");
    model.send(Intent::QuitAndSwitch {
        qualified: question.qualified,
    });
    let quit = model.next();
    let answer = machine.answer(quit);
    model.give(answer);
    let Job::SignIn { id, .. } = model.take(|job| matches!(job, Job::SignIn { .. })) else {
        unreachable!("taken as a sign-in");
    };
    model.send(Intent::CancelSignIn);
    let jobs = model.give(Answer::SignInStarted {
        id,
        started: Ok(()),
        put_back: Some("desktop/work".into()),
    });
    assert!(jobs.contains(&Job::StopSignIn { id }), "{jobs:?}");
    assert!(
        jobs.iter().any(|job| matches!(
            job,
            Job::Open { tool, front: false, .. } if tool == "desktop"
        )),
        "{jobs:?}"
    );
    assert_eq!(model.shown().signing_in, None);
    model.run(&mut machine);
    assert_eq!(
        machine.apps.asked(),
        [format!("quit {CLAUDE}"), format!("open {CLAUDE}")]
    );
    machine.changed += 1;
    model.notice(&mut machine);
    assert_eq!(model.count(super::testing::machine::offline_read), 1);
}

#[test]
fn an_add_cancelled_before_its_park_starts_lets_the_index_go_and_opens_claude_behind() {
    let (mut model, mut machine) = adding_in_claude(true);
    let first = starts(&mut model, &mut machine, "claude", "one");
    model.send(Intent::CancelSignIn);
    // Its stop unanswered, it holds the core's one sign-in at a time.
    model.take(|job| matches!(job, Job::StopSignIn { .. }));
    quit_for_it(&mut model, &mut machine);
    model.send(Intent::CancelSignIn);
    model.run(&mut machine);
    model.give(Answer::SignInStopped { id: first });
    model.run(&mut machine);
    assert_eq!(
        machine.signed_in,
        ["claude/one"],
        "travel's park never started"
    );
    assert_eq!(
        machine.apps.asked(),
        [format!("quit {CLAUDE}"), format!("open {CLAUDE}")]
    );
    machine.changed += 1;
    model.notice(&mut machine);
    assert_eq!(model.count(super::testing::machine::offline_read), 1);
}

#[test]
fn a_park_cancelled_in_flight_and_asked_again_keeps_each_hold_its_own() {
    for open in [false, true] {
        let (mut model, mut machine) = adding_in_claude(open);
        machine.put_back = Some("desktop/work".into());
        model.send(travel());
        if open {
            model.run(&mut machine);
            let question = model.shown().quit_question.expect("asked");
            model.send(Intent::QuitAndSwitch {
                qualified: question.qualified,
            });
        }
        let held = model.next();
        let answer = machine.answer(held);
        model.give(answer);
        let first = model.take(|job| matches!(job, Job::SignIn { .. }));
        let Job::SignIn { id, .. } = first.clone() else {
            unreachable!("taken as a sign-in");
        };
        model.send(Intent::CancelSignIn);
        model.send(travel());
        model.run(&mut machine);
        let started = machine.answer(first);
        model.give(started);
        model.run(&mut machine);
        model.give(Answer::SignInQuiet { id });
        model.run(&mut machine);
        assert_eq!(
            machine.signed_in,
            ["desktop/travel", "desktop/travel"],
            "{open}"
        );
        assert_eq!(running(&model).id, id + 1, "{open}: the second started");
        let quit_and_opened: &[String] = if open {
            &[format!("quit {CLAUDE}"), format!("open-front {CLAUDE}")]
        } else {
            &[]
        };
        assert_eq!(
            machine.apps.asked(),
            quit_and_opened,
            "{open}: opened once the second has parked"
        );
        machine.changed += 1;
        model.notice(&mut machine);
        assert_eq!(
            model.count(super::testing::machine::offline_read),
            1,
            "{open}"
        );
    }
}

// Claude's quit failing each way: (quits, lingers, lost, what is said, what the apps were asked).
pub(super) fn quits_failed() -> [(bool, bool, bool, String, Vec<String>); 3] {
    let still_open = "Claude is still open, so nothing has changed. Quit it, then try again.";
    let lingering = format!(
        "Part of Claude is still running after it quit, so nothing has changed. Wait until it \
         ends, or quit it in {MONITOR}, then try again."
    );
    let (quit, open) = (format!("quit {CLAUDE}"), format!("open {CLAUDE}"));
    [
        (false, false, false, still_open.into(), vec![quit.clone()]),
        (true, true, false, lingering, vec![quit, open]),
        (true, false, true, still_open.into(), Vec::new()),
    ]
}

#[test]
fn an_add_whose_app_does_not_quit_parks_nothing_and_lets_its_sheet_go() {
    for (quits, lingers, lost, message, asked) in quits_failed() {
        let (mut model, mut machine) = adding_in_claude(true);
        machine.apps = StandInApps::new(&[CLAUDE], quits);
        machine.lingers = lingers;
        machine.put_back = Some("desktop/work".into());
        model.send(travel());
        model.run(&mut machine);
        let question = model.shown().quit_question.expect("asked");
        model.send(Intent::QuitAndSwitch {
            qualified: question.qualified,
        });
        if lost {
            let quit = model.take(a_quit);
            model.give(Answer::Lost(quit));
        }
        model.run(&mut machine);
        assert!(machine.signed_in.is_empty(), "{message}");
        let shown = model.on_a_mac();
        assert_eq!(shown.signing_in, None);
        assert_eq!(shown.sheet, Some(add(Some("desktop"))));
        let failure = shown.sheet_failure.expect("said in the sheet");
        assert_eq!(failure.title, "Couldn’t sign in to travel");
        assert_eq!(failure.message, message);
        assert!(!shown.sheet_text.expect("the form").saving, "{message}");
        assert_eq!(machine.apps.asked(), asked);
        machine.changed += 1;
        model.notice(&mut machine);
        assert_eq!(
            model.count(super::testing::machine::offline_read),
            1,
            "the index is not held"
        );
        model.send(Intent::CloseSheet);
        assert_eq!(model.shown().sheet, None, "{message}");
    }
}

#[test]
fn no_park_starts_beside_a_switch_of_its_tool() {
    let (mut model, mut machine) = adding_in_claude(true);
    model.send(Intent::SwitchTo {
        qualified: "desktop/home".into(),
    });
    assert_eq!(model.send(travel()), [], "asked while the switch runs");
    model.run(&mut machine);
    assert!(machine.signed_in.is_empty());

    let (mut model, mut machine) = adding_in_claude(false);
    model.send(travel());
    model.send(Intent::SwitchTo {
        qualified: "desktop/home".into(),
    });
    model.run(&mut machine);
    assert!(
        machine.signed_in.is_empty(),
        "asked before the switch, it would park once the switch is claimed"
    );
    assert_eq!(machine.switched_to, ["desktop/home"]);
    assert!(!model.on_a_mac().sheet_text.expect("the form").saving);
}

#[test]
fn keeping_the_app_open_starts_no_sign_in_and_lets_the_sheet_go() {
    let (mut model, mut machine) = adding_in_claude(true);
    model.send(travel());
    model.run(&mut machine);
    assert!(model.on_a_mac().sheet_text.expect("the form").saving);
    assert_eq!(model.send(Intent::KeepAppOpen), []);
    let shown = model.on_a_mac();
    assert_eq!(shown.quit_question, None);
    assert_eq!(shown.signing_in, None);
    assert!(!shown.sheet_text.expect("the form").saving);
    assert_eq!(model.count(a_quit), 0);
    assert!(machine.apps.asked().is_empty());
}

#[test]
fn a_first_account_of_such_a_tool_is_recorded_as_the_one_signed_in() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![claude("work", true, 10.0)])));
    machine.found = vec![claude_code(), claude_desktop()];
    model.refresh(&mut machine);
    put_up(&mut model, &mut machine, add(Some("desktop")));
    assert_eq!(
        model.on_a_mac().sheet_text.expect("the form").confirm,
        "Add Account"
    );
    let jobs = model.send(sign_in("desktop", "work"));
    assert_eq!(
        jobs,
        [Job::Holding {
            qualified: "desktop/work".into(),
            then: After::Enrol {
                provider: "desktop".into(),
                name: "work".into(),
                from: Some(add(Some("desktop"))),
            },
        }]
    );
    model.run(&mut machine);
    assert_eq!(machine.enrolled, ["desktop/work"]);
    assert!(machine.signed_in.is_empty());
    assert_eq!(model.shown().sheet, None, "its sheet closes");
}

#[test]
fn a_first_account_refused_as_signed_out_offers_the_sign_in_next() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![claude("work", true, 10.0)])));
    machine.found = vec![claude_code(), claude_desktop()];
    machine.enrolling_current = Err(refusal(
        "live_credential_absent",
        "Claude isn’t signed in.",
        Vec::new(),
    ));
    model.refresh(&mut machine);
    put_up(&mut model, &mut machine, add(Some("desktop")));
    model.send(sign_in("desktop", "work"));
    model.run(&mut machine);
    let shown = model.on_a_mac();
    assert_eq!(
        shown.sheet_failure.expect("said in the sheet").title,
        "Couldn’t add work"
    );
    assert_eq!(shown.sheet_text.expect("the form").confirm, "Start Sign-In");
    let jobs = model.send(sign_in("desktop", "work"));
    assert!(
        matches!(
            &jobs[..],
            [Job::Holding {
                then: After::SignIn { .. },
                ..
            }]
        ),
        "{jobs:?}"
    );
}

#[test]
fn an_add_that_met_a_login_pitboard_cannot_place_records_the_account_claude_shows() {
    let (mut model, mut machine) = adding_in_claude(false);
    machine.starting = Err(refusal("login_in_doubt", "in doubt", Vec::new()));
    model.send(travel());
    model.run(&mut machine);
    assert_eq!(machine.signed_in, ["desktop/travel"]);
    assert_eq!(
        model.on_a_mac().sheet_text.expect("the form").confirm,
        "Add the Account Claude Shows"
    );
    let jobs = model.send(travel());
    assert!(
        matches!(
            &jobs[..],
            [Job::Holding {
                then: After::Enrol { .. },
                ..
            }]
        ),
        "{jobs:?}"
    );
}

#[test]
fn the_wait_says_each_step_it_meets() {
    let (mut model, mut machine) = adding_in_claude(false);
    machine.put_back = Some("desktop/work".into());
    model.send(travel());
    model.run(&mut machine);
    let id = running(&model).id;
    let step = |model: &mut Hand, text: &str, step| {
        model.give(Answer::SignInSaid {
            id,
            text: text.into(),
            step: Some(step),
        });
        page(model)
    };
    let open = step(&mut model, "Claude is open.\n", WaitStep::Open);
    assert!(
        open.message
            .starts_with("Open Claude and sign in to the account you’re adding."),
        "Pitboard did not open it: {}",
        open.message
    );
    assert_eq!(open.waiting, "Waiting for Claude to quit");
    assert_eq!(open.announce, None);
    let nobody = step(
        &mut model,
        "Claude quit with nobody signed in. Still waiting.\n",
        WaitStep::QuitSignedOut,
    );
    assert!(
        nobody
            .message
            .starts_with("Claude quit with nobody signed in.")
    );
    assert_eq!(
        nobody.announce.as_ref(),
        Some(&nobody.message),
        "asks again"
    );
    let unfinished = step(
        &mut model,
        "Claude quit before it finished signing in.\n",
        WaitStep::QuitUnfinished,
    );
    assert!(
        unfinished
            .message
            .starts_with("Claude quit before it finished signing in.")
    );
    assert_eq!(unfinished.announce.as_ref(), Some(&unfinished.message));
}

#[test]
fn later_stops_the_wait_and_reads_again() {
    let (mut model, mut machine) = adding_in_claude(false);
    machine.put_back = Some("desktop/work".into());
    model.send(travel());
    model.run(&mut machine);
    let id = running(&model).id;
    assert_eq!(page(&model).cancel.title, "Later");
    let reads = model.count(any_read);
    let jobs = model.send(Intent::CancelSignIn);
    assert!(jobs.contains(&Job::StopSignIn { id }), "{jobs:?}");
    model.run(&mut machine);
    assert!(
        model.count(any_read) > reads,
        "the notice of an add left halfway is read"
    );
    assert_eq!(model.shown().signing_in, None);
}

#[test]
fn cancelling_while_claude_quits_parks_nothing_and_opens_it_again() {
    let (mut model, mut machine) = adding_in_claude(true);
    model.send(travel());
    model.run(&mut machine);
    let question = model.shown().quit_question.expect("asked");
    model.send(Intent::QuitAndSwitch {
        qualified: question.qualified,
    });
    assert!(!page(&model).cancel.enabled, "held back while Claude quits");
    assert_eq!(model.send(Intent::CancelSignIn), []);
    model.run(&mut machine);
    assert!(machine.signed_in.is_empty(), "nothing parked");
    assert_eq!(
        machine.apps.asked(),
        [format!("quit {CLAUDE}"), format!("open {CLAUDE}")]
    );
}

#[test]
fn a_sheet_that_is_saving_does_not_close() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    let name = Sheet::Name {
        provider: "claude".into(),
        email: "a@example.com".into(),
    };
    put_up(&mut model, &mut machine, name.clone());
    model.send(Intent::Enrol {
        provider: "claude".into(),
        name: "work".into(),
    });
    model.send(Intent::CloseSheet);
    assert_eq!(
        model.shown().sheet,
        Some(name),
        "a save cannot be withdrawn"
    );
    model.run(&mut machine);
    assert_eq!(model.shown().sheet, None, "closed once enrolled");

    let (mut model, mut machine) = adding_in_claude(true);
    model.send(travel());
    model.send(Intent::CloseSheet);
    assert_eq!(model.shown().sheet, Some(add(Some("desktop"))));
    model.run(&mut machine);
    model.send(Intent::KeepAppOpen);
    model.send(Intent::CloseSheet);
    assert_eq!(model.shown().sheet, None);
}

#[test]
fn a_sheet_held_for_the_quit_question_stays_up_until_it_is_answered() {
    let name = Sheet::Name {
        provider: "claude".into(),
        email: "a@example.com".into(),
    };
    let held = [
        (add(Some("desktop")), travel()),
        (
            Sheet::SignInAgain {
                provider: "desktop".into(),
                label: "work".into(),
            },
            sign_in("desktop", "work"),
        ),
        (
            Sheet::Name {
                provider: "desktop".into(),
                email: String::new(),
            },
            Intent::Enrol {
                provider: "desktop".into(),
                name: "travel".into(),
            },
        ),
    ];
    let up = |sheet: &Sheet, starts_it: &Intent| {
        let mut model = Hand::new();
        let mut machine = desktop_machine(true);
        model.refresh(&mut machine);
        put_up(&mut model, &mut machine, sheet.clone());
        model.send(starts_it.clone());
        (model, machine)
    };
    for (sheet, starts_it) in &held {
        // Put up while Pitboard asks whether Claude runs, and while the question is asked.
        for asked in [false, true] {
            let (mut model, mut machine) = up(sheet, starts_it);
            if asked {
                model.run(&mut machine);
                assert!(model.shown().quit_question.is_some(), "{sheet:?}");
            }
            model.send(present(name.clone()));
            assert_eq!(model.shown().sheet.as_ref(), Some(sheet), "{asked}");
            model.run(&mut machine);
            let question = model.shown().quit_question.expect("asked");
            model.send(Intent::QuitAndSwitch {
                qualified: question.qualified,
            });
            model.send(present(name.clone()));
            assert_eq!(
                model.shown().sheet.as_ref(),
                Some(sheet),
                "{asked}: while Claude quits"
            );
        }
        let (mut model, mut machine) = up(sheet, starts_it);
        model.run(&mut machine);
        model.send(Intent::KeepAppOpen);
        model.send(present(name.clone()));
        assert_eq!(
            model.shown().sheet.as_ref(),
            Some(&name),
            "{sheet:?}: let go once the question is closed"
        );
    }
    let (mut model, mut machine) = adding_in_claude(true);
    model.send(travel());
    model.run(&mut machine);
    let question = model.shown().quit_question.expect("asked");
    model.send(Intent::QuitAndSwitch {
        qualified: question.qualified,
    });
    model.run(&mut machine);
    let shown = model.on_a_mac();
    assert_eq!(shown.sheet, Some(add(Some("desktop"))));
    assert!(shown.signing_in_text.is_some(), "the wait is on screen");
}

#[test]
fn a_late_answer_beside_another_sign_in_quits_nothing() {
    let (mut model, mut machine) = adding_in_claude(true);
    model.send(travel());
    model.run(&mut machine);
    let question = model.shown().quit_question.expect("asked");
    // Closed before the app sends its answer, which lets the sheet be replaced meanwhile.
    model.send(Intent::KeepAppOpen);
    put_up(&mut model, &mut machine, add(None));
    starts(&mut model, &mut machine, "claude", "one");
    model.send(Intent::QuitAndSwitch {
        qualified: question.qualified,
    });
    model.send(Intent::CancelSignIn);
    model.run(&mut machine);
    assert_eq!(model.count(a_quit), 0);
    put_up(&mut model, &mut machine, add(Some("desktop")));
    assert!(!model.on_a_mac().sheet_text.expect("the form").saving);
    model.send(Intent::CloseSheet);
    assert_eq!(model.shown().sheet, None);
}

#[test]
fn parking_for_a_sign_in_holds_the_index_until_its_read_is_over() {
    let (mut model, mut machine) = adding_in_claude(false);
    machine.put_back = Some("desktop/work".into());
    model.send(travel());
    let held = model.next();
    let answer = machine.answer(held);
    model.give(answer);
    let sign_in = model.take(|job| matches!(job, Job::SignIn { .. }));
    machine.changed += 1;
    model.notice(&mut machine);
    assert_eq!(
        model.count(super::testing::machine::offline_read),
        0,
        "the park is this app's own change"
    );
    let started = machine.answer(sign_in);
    model.give(started);
    model.run(&mut machine);
    machine.changed += 1;
    model.notice(&mut machine);
    assert_eq!(
        model.count(super::testing::machine::offline_read),
        1,
        "let go once the read after it is over"
    );
}

#[test]
fn a_sign_in_again_of_the_account_in_use_is_said_as_a_new_login() {
    let mut model = Hand::new();
    let mut machine = desktop_machine(false);
    machine.put_back = Some("desktop/work".into());
    machine.enrolling = enrolled_as(EnrolledAs::InUse { again: true }, Vec::new());
    model.refresh(&mut machine);
    put_up(
        &mut model,
        &mut machine,
        Sheet::SignInAgain {
            provider: "desktop".into(),
            label: "work".into(),
        },
    );
    assert_eq!(
        model.on_a_mac().sheet_text.expect("the form").confirm,
        "Park work"
    );
    model.send(sign_in("desktop", "work"));
    model.run(&mut machine);
    let id = running(&model).id;
    ends(&mut model, &mut machine, id);
    let notice = model
        .on_a_mac()
        .notices
        .into_iter()
        .find(|notice| notice.id == "switch/desktop")
        .expect("what the sign-in did");
    assert_eq!(notice.title, "work has a new login");
    assert_eq!(
        notice.lines,
        ["Signed in to work again. Claude is signed in to it now."]
    );
}

#[test]
fn a_resumed_add_offers_to_put_its_account_back() {
    let mut model = Hand::new();
    let read = crate::Status {
        awaiting: vec![awaiting("travel", "work", false)],
        ..status(vec![
            desktop_account("work", false),
            desktop_account("home", false),
        ])
    };
    let mut machine = desktop_machine(false);
    machine.answer = Ok(read);
    machine.enrolling_current = Err(refusal(
        "live_credential_absent",
        "Claude isn’t signed in.",
        Vec::new(),
    ));
    machine.put_back = Some("desktop/work".into());
    model.refresh(&mut machine);
    put_up(&mut model, &mut machine, add(Some("desktop")));
    let form = model.on_a_mac().sheet_text.expect("the form");
    assert_eq!(form.name, "travel");
    assert_eq!(form.confirm, "Add the Account Claude Shows");
    model.send(travel());
    model.run(&mut machine);
    assert_eq!(
        model.on_a_mac().sheet_text.expect("the form").confirm,
        "Start Sign-In"
    );
    model.send(travel());
    model.run(&mut machine);
    let page = page(&model);
    assert_eq!(
        page.undo.map(|undo| (undo.title, undo.intent)),
        Some((
            "Put work Back".into(),
            Intent::SwitchTo {
                qualified: "desktop/work".into()
            }
        ))
    );
    assert_eq!(page.cancel.title, "Later");
    assert!(!page.escape_cancels);
}

#[test]
fn nothing_parked_from_a_logged_out_claude_offers_no_put_back() {
    let mut model = Hand::new();
    let mut machine = desktop_machine(false);
    machine.answer = Ok(super::testing::records::warned(
        vec![
            desktop_account("work", true),
            desktop_account("home", false),
        ],
        vec![desktop_warning(
            "looks_signed_out",
            "Claude looks signed out since Pitboard last read work in use.",
        )],
    ));
    model.refresh(&mut machine);
    put_up(&mut model, &mut machine, add(Some("desktop")));
    assert_eq!(
        model.on_a_mac().sheet_text.expect("the form").confirm,
        "Start Sign-In"
    );
    model.send(travel());
    model.run(&mut machine);
    assert_eq!(machine.signed_in, ["desktop/travel"]);
    let page = page(&model);
    assert_eq!(page.cancel.title, "Cancel");
    assert!(page.escape_cancels);
    assert_eq!(page.undo, None);
}

#[test]
fn no_undo_is_offered_before_the_core_says_what_it_parked() {
    let (mut model, mut machine) = adding_in_claude(false);
    machine.put_back = Some("desktop/work".into());
    model.send(travel());
    let held = model.next();
    let answer = machine.answer(held);
    model.give(answer);
    let page = page(&model);
    assert_eq!(page.undo, None);
    assert!(!page.escape_cancels);
    assert_eq!(page.cancel.title, "Cancel");
    assert_eq!(page.waiting, "Parking work");
}

#[test]
fn the_page_says_pitboard_opened_claude_only_where_it_did() {
    for (moving, said) in [
        (true, "Open Claude and sign in"),
        (false, "In Claude, sign in"),
    ] {
        let (mut model, mut machine) = adding_in_claude(true);
        machine.put_back = Some("desktop/work".into());
        machine.moving = moving;
        quit_for_it(&mut model, &mut machine);
        let page = page(&model);
        assert!(page.message.starts_with(said), "{moving}: {}", page.message);
    }
}
