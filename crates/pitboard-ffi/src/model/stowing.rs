//! Putting away the login Claude Code left in a file behind the keychain, from the notice that
//! says it is there: a sheet that looks first and says whose the login is once that is known,
//! then puts it away once somebody confirms what it says, as `pitboard stow` asks first.

use super::state::{Answer, Job};
use super::testing::{Hand, Machine, claude, refusal, warned, warning};
use super::{Intent, Sheet};
use crate::Held;
use crate::present::Severity;
use pitboard_core::api::Owner;
use pitboard_core::switch::{Foreseen, Kept, Left, Stowed};
use std::path::PathBuf;

const FILE: &str = "/Users/dana/.claude/.credentials.json";

/// The warning every read gives while the file is behind the keychain, holding `held`.
fn fallback_login(held: Held) -> crate::Warning {
    crate::Warning {
        held: Some(held),
        ..warning(
            "fallback_login",
            &format!(
                "{FILE} holds another Claude Code login, which a session that cannot read the \
                 keychain, such as one started over SSH, signs in with."
            ),
        )
    }
}

/// The file, holding a login the look found `login` of, with Claude Code's MCP tokens beside it.
pub(super) fn left(login: Foreseen) -> Left {
    Left {
        path: PathBuf::from(FILE),
        seen: "0123456789abcdef".into(),
        login,
        dropped: vec!["mcpOAuth".into()],
    }
}

pub(super) fn parked_now(label: &str) -> Kept {
    Kept::ParkedNow {
        label: label.into(),
    }
}

/// A model that has read `work` in use, with the file behind the keychain holding a login.
fn with_a_file_left(machine: &mut Machine) -> Hand {
    with_a_file_holding(machine, Held::Login)
}

/// A model that has read `work` in use, with the file behind the keychain holding `held`.
fn with_a_file_holding(machine: &mut Machine, held: Held) -> Hand {
    machine.answer = Ok(warned(
        vec![claude("work", true, 10.0)],
        vec![fallback_login(held)],
    ));
    let mut model = Hand::new();
    model.refresh(machine);
    model
}

fn stow_sheet() -> Intent {
    Intent::PresentSheet { sheet: Sheet::Stow }
}

fn a_look_left(job: &Job) -> bool {
    matches!(job, Job::LookLeft)
}

fn a_stow(job: &Job) -> bool {
    matches!(job, Job::Stow { .. })
}

/// The notice that a login is left in a file offers to put it away, with a sheet of its own,
/// rather than sending somebody to a terminal, and so does the notice of a file with no login
/// in it. One Pitboard cannot read it cannot put away, so its notice offers nothing, and its
/// words send somebody to `pitboard doctor`.
#[test]
fn the_notice_of_a_login_left_in_a_file_offers_to_put_it_away() {
    for held in [Held::Login, Held::NoLogin, Held::Unreadable] {
        let mut machine = Machine::reading(Ok(warned(Vec::new(), Vec::new())));
        let model = with_a_file_holding(&mut machine, held);

        let shown = model.shown();
        let notice = shown
            .notices
            .iter()
            .find(|notice| notice.id.starts_with("warning/fallback_login/"))
            .expect("the notice");
        if held == Held::Unreadable {
            assert!(notice.actions.is_empty(), "{:?}", notice.actions);
            continue;
        }
        assert_eq!(notice.actions.len(), 1, "{held:?}: {:?}", notice.actions);
        assert_eq!(notice.actions[0].title, "Put Away…");
        assert_eq!(notice.actions[0].intent, stow_sheet());
        assert!(notice.actions[0].enabled);
        assert_eq!(notice.actions[0].confirm, None, "the sheet asks");
    }
}

/// The sheet looks first, and says whose the login is once that is known: until then it
/// says it is finding out, and Put Away waits.
#[test]
fn the_sheet_says_whose_the_login_is_once_it_is_known() {
    let mut machine = Machine::reading(Ok(warned(Vec::new(), Vec::new())));
    let mut model = with_a_file_left(&mut machine);
    machine.leftover = Ok(Some(left(Foreseen::Kept(parked_now("work")))));

    model.send(stow_sheet());

    assert_eq!(model.count(a_look_left), 1);
    let looking = model.shown().stow_text.expect("the sheet's words");
    assert_eq!(
        looking.looking.as_deref(),
        Some("Finding out whose login it is…")
    );
    assert!(looking.lines.is_empty());
    assert!(!looking.can_confirm);

    model.run(&mut machine);

    let text = model.shown().stow_text.expect("the sheet's words");
    assert_eq!(text.title, "Put Away the Login Left in a File");
    assert_eq!(text.confirm, "Put Away");
    assert_eq!(text.looking, None);
    assert!(text.can_confirm);
    assert!(!text.saving);
    assert_eq!(
        text.lines,
        [
            format!(
                "{FILE} holds `work`'s login, and Pitboard holds no parked login of `work` to \
                 keep in its place. Putting it away parks this one for `work`, then deletes \
                 the file."
            ),
            "It also holds 1 other key, `mcpOAuth`, which goes with the file: Pitboard does not \
             move it, and Claude Code's document in the keychain keeps its own."
                .to_string(),
            "Once the file is gone, Claude Code sessions already running follow a switch \
             within 33 seconds again, and one that signed in with its login, such as one over \
             SSH, is signed out."
                .to_string(),
        ]
    );
}

/// Put Away sends what the sheet showed, which is put away only while the file still holds
/// it. The sheet closes, what was done is said until somebody puts it away, and the accounts
/// are read again.
#[test]
fn putting_away_sends_what_the_sheet_showed_and_says_what_it_did() {
    let mut machine = Machine::reading(Ok(warned(Vec::new(), Vec::new())));
    let mut model = with_a_file_left(&mut machine);
    machine.leftover = Ok(Some(left(Foreseen::Kept(parked_now("work")))));
    model.send(stow_sheet());
    model.run(&mut machine);
    machine.answer = Ok(warned(vec![claude("work", true, 10.0)], Vec::new()));
    machine.stowing = Ok(Stowed {
        path: PathBuf::from(FILE),
        kept: parked_now("work"),
        dropped: vec!["mcpOAuth".into()],
    });

    model.send(Intent::Stow);
    assert!(model.shown().stow_text.expect("still up").saving);
    model.run(&mut machine);

    assert_eq!(machine.stowed_seen, ["0123456789abcdef"]);
    let shown = model.shown();
    assert_eq!(shown.sheet, None);
    assert_eq!(shown.stow_text, None);
    assert_eq!(shown.sheet_failure, None);
    let notice = shown
        .notices
        .iter()
        .find(|notice| notice.id == "stowed")
        .expect("what it did");
    assert_eq!(notice.severity, Severity::Info);
    assert_eq!(notice.title, "Put away the login left in a file");
    assert_eq!(
        notice.lines[0],
        format!("Parked the login {FILE} held for `work`, then deleted the file.")
    );
    assert!(
        shown
            .notices
            .iter()
            .all(|notice| !notice.id.starts_with("warning/fallback_login/")),
        "the read after it no longer finds the file"
    );
    assert_eq!(notice.actions.len(), 1);
    model.send(notice.actions[0].intent.clone());
    assert!(
        model
            .shown()
            .notices
            .iter()
            .all(|notice| notice.id != "stowed")
    );
}

/// The login of an account nobody enrolled cannot be put away: the sheet says whose it is and
/// what to do first, holds Put Away back, and sends nothing if it is pressed.
#[test]
fn a_login_of_an_account_not_enrolled_is_not_offered_to_put_away() {
    let mut machine = Machine::reading(Ok(warned(Vec::new(), Vec::new())));
    let mut model = with_a_file_left(&mut machine);
    machine.leftover = Ok(Some(left(Foreseen::NotEnrolled(Owner {
        account_uuid: "stranger".into(),
        email: "stranger@example.com".into(),
        organization_uuid: "org-stranger".into(),
    }))));
    model.send(stow_sheet());
    model.run(&mut machine);

    let text = model.shown().stow_text.expect("the sheet's words");
    assert!(!text.can_confirm);
    assert!(
        text.lines[0].contains("stranger@example.com"),
        "{:?}",
        text.lines
    );
    assert!(model.send(Intent::Stow).is_empty());
}

/// What stops the look, or putting away, is said in the sheet, which stays up: whose the login
/// is cannot be told, or the file changed since the sheet said what it holds.
#[test]
fn what_stops_it_is_said_in_the_sheet() {
    let mut machine = Machine::reading(Ok(warned(Vec::new(), Vec::new())));
    let mut model = with_a_file_left(&mut machine);
    machine.leftover = Err(refusal(
        "left_login_unidentified",
        "Pitboard could not ask Anthropic whose the login is.",
        Vec::new(),
    ));
    model.send(stow_sheet());
    model.run(&mut machine);

    let shown = model.shown();
    let failure = shown.sheet_failure.expect("said in the sheet");
    assert_eq!(failure.title, "Couldn’t tell whose login it is");
    assert!(!shown.stow_text.expect("still up").can_confirm);

    machine.leftover = Ok(Some(left(Foreseen::Kept(parked_now("work")))));
    model.send(Intent::CloseSheet);
    model.send(stow_sheet());
    model.run(&mut machine);
    assert_eq!(model.shown().sheet_failure, None, "a look anew");
    machine.stowing = Err(refusal(
        "left_login_changed",
        "The file has changed since putting it away was confirmed.",
        Vec::new(),
    ));
    model.send(Intent::Stow);
    model.run(&mut machine);

    let shown = model.shown();
    assert_eq!(shown.sheet, Some(Sheet::Stow));
    let failure = shown.sheet_failure.expect("said in the sheet");
    assert_eq!(failure.title, "Couldn’t put the login away");
    assert_eq!(failure.code.as_deref(), Some("left_login_changed"));
    assert!(!shown.stow_text.expect("still up").saving);
}

/// Refused, the sheet looks at the file again, since what it showed is out of date: the file
/// changed, or was renewed and written back before putting it away stopped. Put Away waits
/// for that look, and then sends what it found, not what was refused.
#[test]
fn a_refused_put_away_looks_again() {
    let mut machine = Machine::reading(Ok(warned(Vec::new(), Vec::new())));
    let mut model = with_a_file_left(&mut machine);
    machine.leftover = Ok(Some(left(Foreseen::Kept(parked_now("work")))));
    model.send(stow_sheet());
    model.run(&mut machine);
    machine.stowing = Err(refusal(
        "left_login_changed",
        "The file has changed since putting it away was confirmed.",
        Vec::new(),
    ));

    model.send(Intent::Stow);
    model.run_but(&mut machine, a_look_left);

    assert_eq!(model.count(a_look_left), 2);
    let shown = model.shown();
    assert_eq!(
        shown
            .sheet_failure
            .expect("said in the sheet")
            .code
            .as_deref(),
        Some("left_login_changed")
    );
    let looking = shown.stow_text.expect("still up");
    assert!(looking.looking.is_some());
    assert!(!looking.can_confirm);
    assert!(model.send(Intent::Stow).is_empty(), "nothing to send yet");

    machine.leftover = Ok(Some(Left {
        seen: "fedcba9876543210".into(),
        ..left(Foreseen::Kept(parked_now("work")))
    }));
    model.run(&mut machine);
    let text = model.shown().stow_text.expect("still up");
    assert!(text.can_confirm);
    assert!(
        model.shown().sheet_failure.is_some(),
        "what stopped it stays said"
    );
    model.send(Intent::Stow);
    model.run(&mut machine);
    assert_eq!(
        machine.stowed_seen,
        ["0123456789abcdef", "fedcba9876543210"]
    );
}

/// Putting away that came to nothing is said in its sheet: what it did is not known.
#[test]
fn putting_away_that_came_to_nothing_is_said_in_its_sheet() {
    let mut machine = Machine::reading(Ok(warned(Vec::new(), Vec::new())));
    let mut model = with_a_file_left(&mut machine);
    machine.leftover = Ok(Some(left(Foreseen::Kept(parked_now("work")))));
    model.send(stow_sheet());
    model.run(&mut machine);

    model.send(Intent::Stow);
    let stowing = model.take(a_stow);
    model.give(Answer::Lost(stowing));

    let shown = model.shown();
    let failure = shown.sheet_failure.expect("said in the sheet");
    assert_eq!(failure.title, "Couldn’t put the login away");
    assert!(!shown.stow_text.expect("still up").saving);
}
