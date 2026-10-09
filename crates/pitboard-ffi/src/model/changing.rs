//! Enrolling the login signed in now, renaming and forgetting, as the Swift model did them:
//! its tests on those, each under its own name in snake case, driven through `State::apply`
//! by hand. Each closes only its own sheet, says what went wrong where it was asked, and
//! reads the accounts afterwards.
//!
//! Writing the account in use into Claude Code's config, which the Swift model never did, is
//! tested here too: a change with no sheet, which says what went wrong in an alert and reads
//! the accounts afterwards, as a rename does.

use super::advice::Told;
use super::state::{Answer, Job};
use super::stowing::{left, parked_now};
use super::switching::{a_read_that_started_before, switch};
use super::testing::{
    Hand, Machine, a_look, a_look_or_a_read, any_read, claude, codex_account, enrolled_as,
    offline_read, refusal, status, still_running, switched, warned, warning,
};
use super::{Intent, Sheet};
use crate::present::testing::account;
use crate::{Account, EnrolledAs};
use pitboard_core::switch::Foreseen;
use std::time::Duration;

fn enrol(provider: &str, name: &str) -> Intent {
    Intent::Enrol {
        provider: provider.into(),
        name: name.into(),
    }
}

fn rename(provider: &str, label: &str, to: &str) -> Intent {
    Intent::Rename {
        provider: provider.into(),
        label: label.into(),
        to: to.into(),
    }
}

fn forget(qualified: &str) -> Intent {
    Intent::Forget {
        qualified: qualified.into(),
    }
}

fn update_config(qualified: &str) -> Intent {
    Intent::UpdateConfig {
        qualified: qualified.into(),
    }
}

fn naming(provider: &str, email: &str) -> Sheet {
    Sheet::Name {
        provider: provider.into(),
        email: email.into(),
    }
}

fn renaming(provider: &str, label: &str) -> Sheet {
    Sheet::Rename {
        provider: provider.into(),
        label: label.into(),
    }
}

/// Puts `sheet` up and answers what that asks.
fn put_up(model: &mut Hand, machine: &mut Machine, sheet: Sheet) {
    model.send(Intent::PresentSheet { sheet });
    model.run(machine);
}

fn tos(model: &Hand) -> Vec<String> {
    model
        .shown()
        .last_switches
        .into_iter()
        .map(|last| last.to)
        .collect()
}

/// The account signed in but not enrolled is the one the app can record by itself: no
/// browser, no terminal. The sheet closes once the name is taken, and stays open with the
/// name in it when it is refused, so it can be corrected rather than typed again.
///
/// AppModelTests.swift's onlyAnUnenrolledSignedInAccountCanBeNamedHere.
#[test]
fn only_an_unenrolled_signed_in_account_can_be_named_here() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(vec![
        account(None).signed_in().uuid("a").build(),
    ])));
    model.refresh(&mut machine);
    assert!(matches!(
        model.shown().footing,
        crate::Footing::Unnamed { .. }
    ));

    let name = naming("claude", "a@example.com");
    put_up(&mut model, &mut machine, name.clone());
    machine.enrolling_current = Err(refusal(
        "label_taken",
        "There is already an account called work.",
        vec![warning("auth_overridden", "ANTHROPIC_API_KEY is set")],
    ));
    model.send(enrol("claude", "work"));
    model.run(&mut machine);
    let shown = model.shown();
    let refused = shown.sheet_failure.expect("said in the sheet");
    assert_eq!(refused.title, "Couldn’t name this account");
    assert_eq!(refused.message, "There is already an account called work.");
    assert_eq!(shown.sheet, Some(name), "the sheet stays open to say why");
    assert!(
        shown.warnings.is_empty(),
        "said in the sheet, not in the panel"
    );
    assert_eq!(shown.failure, None);

    machine.enrolling_current = enrolled_as(EnrolledAs::Current, Vec::new());
    model.send(enrol("claude", "work"));
    model.run(&mut machine);
    assert_eq!(machine.enrolled, ["claude/work", "claude/work"]);
    assert_eq!(
        model.shown().sheet,
        None,
        "the sheet closes once it has been used"
    );
}

/// Naming a Codex login enrols it as Codex's. A bare name means Claude Code to the core, and
/// would have enrolled nothing or the wrong tool's login.
///
/// AppModelTests.swift's aCodexLoginIsNamedAsCodexs.
#[test]
fn a_codex_login_is_named_as_codexs() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(vec![
        claude("work", true, 0.0),
        account(None).of("codex").signed_in().uuid("c").build(),
    ])));
    model.refresh(&mut machine);
    assert_eq!(
        model.shown().footing,
        crate::Footing::Unnamed {
            provider: "codex".into(),
            email: "c@example.com".into()
        }
    );
    model.send(enrol("codex", "job"));
    model.run(&mut machine);
    assert_eq!(machine.enrolled, ["codex/job"]);
}

/// Forgetting is destructive, so what the model does with a refusal matters. The refusal is
/// said to whoever asked, in the core's own words, and one that went through says nothing.
///
/// AppModelTests.swift's aRefusedForgetIsReported.
#[test]
fn a_refused_forget_is_reported() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(Vec::new())));
    model.send(forget("claude/alpha"));
    model.run(&mut machine);
    assert_eq!(model.shown().failure, None);

    machine.forgetting = Err(refusal(
        "cannot_forget_active_account",
        "beta is the account in use, so it cannot be forgotten.",
        vec![warning("auth_overridden", "ANTHROPIC_API_KEY is set")],
    ));
    model.send(forget("claude/beta"));
    model.run(&mut machine);
    assert_eq!(machine.forgot, ["claude/alpha", "claude/beta"]);
    let shown = model.shown();
    let refused = shown.failure.expect("said");
    assert_eq!(refused.title, "Couldn’t forget beta");
    assert_eq!(
        refused.message,
        "beta is the account in use, so it cannot be forgotten."
    );
    assert_eq!(
        refused.code.as_deref(),
        Some("cannot_forget_active_account")
    );
    let codes: Vec<&str> = refused.warnings.iter().map(|w| w.code.as_str()).collect();
    assert_eq!(codes, ["auth_overridden"]);
    assert_eq!(shown.read_failure, None, "a refusal is not a failed read");
    assert!(
        shown.warnings.is_empty(),
        "its warnings are said with it, not in the panel"
    );
}

/// Two tools can each have a `work`. Every call names the one meant, with its tool, and the
/// rows are told apart by their id rather than by a label they share.
///
/// AppModelTests.swift's twoToolsWorkAccountsAreSwitchedAndForgottenByTheirOwnName, the
/// forgetting and the row's name: the switch is switching.rs's.
#[test]
fn two_tools_work_accounts_are_forgotten_by_their_own_name() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(vec![
        account(Some("work")).signed_in().uuid("same").build(),
        account(Some("personal")).build(),
        account(Some("work")).of("codex").uuid("same").build(),
        account(Some("spare")).of("codex").signed_in().build(),
    ])));
    model.refresh(&mut machine);
    let rows: Vec<crate::AccountItem> = model
        .shown()
        .sections
        .into_iter()
        .flat_map(|section| section.accounts)
        .collect();
    let ids: std::collections::BTreeSet<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids.len(), rows.len(), "one uuid, two tools, two rows");
    let codex_work = rows
        .iter()
        .find(|row| row.provider == "codex" && row.title == "work")
        .expect("Codex's work");
    let claude_work = rows
        .iter()
        .find(|row| row.provider == "claude" && row.title == "work")
        .expect("Claude Code's work");
    assert_ne!(codex_work.id, claude_work.id);

    for row in [codex_work, claude_work] {
        model.send(forget(row.qualified.as_deref().expect("enrolled")));
        model.run(&mut machine);
    }
    assert_eq!(machine.forgot, ["codex/work", "claude/work"]);
    assert_eq!(codex_work.spoken_name, "work (Codex)");
}

/// Naming or renaming closes its own sheet once it is done, and only that. The name is saved
/// even when somebody has since put up another sheet, and closing that one threw away
/// whatever was in it.
///
/// AppModelTests.swift's namingAndRenamingCloseOnlyTheirOwnSheet.
#[test]
fn naming_and_renaming_close_only_their_own_sheet() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(vec![
        account(None).signed_in().uuid("a").build(),
        account(Some("personal")).build(),
    ])));
    model.refresh(&mut machine);

    let others = [
        Sheet::Add { provider: None },
        naming("codex", "c@example.com"),
        renaming("claude", "other"),
    ];
    for other in &others {
        put_up(&mut model, &mut machine, other.clone());
        model.send(enrol("claude", "work"));
        model.run(&mut machine);
        let shown = model.shown();
        assert_eq!(shown.sheet_failure, None);
        assert_eq!(shown.sheet.as_ref(), Some(other));
    }
    put_up(&mut model, &mut machine, naming("claude", "a@example.com"));
    model.send(enrol("claude", "work"));
    model.run(&mut machine);
    assert_eq!(model.shown().sheet, None);

    for other in others.iter().chain([&renaming("codex", "personal")]) {
        put_up(&mut model, &mut machine, other.clone());
        model.send(rename("claude", "personal", "home"));
        model.run(&mut machine);
        assert_eq!(model.shown().sheet.as_ref(), Some(other));
    }
    put_up(&mut model, &mut machine, renaming("claude", "personal"));
    model.send(rename("claude", "personal", "home"));
    model.run(&mut machine);
    assert_eq!(model.shown().sheet, None);
    assert_eq!(
        machine.renamed.last(),
        Some(&("claude/personal".to_owned(), "home".to_owned()))
    );
}

/// AppModelTests.swift's aReadThatStartedBeforeAChangeIsDroppedWhenItLands, for an
/// enrolment.
#[test]
fn a_read_that_started_before_naming_an_account_is_dropped_when_it_lands() {
    a_read_that_started_before(|model, machine| {
        model.send(enrol("codex", "job"));
        model.run(machine);
    });
}

/// The same for a rename.
#[test]
fn a_read_that_started_before_a_rename_is_dropped_when_it_lands() {
    a_read_that_started_before(|model, machine| {
        model.send(rename("codex", "spare", "home"));
        model.run(machine);
    });
}

/// The same for forgetting.
#[test]
fn a_read_that_started_before_forgetting_is_dropped_when_it_lands() {
    a_read_that_started_before(|model, machine| {
        model.send(forget("codex/spare"));
        model.run(machine);
    });
}

/// What `change` does while a look for changes made elsewhere is under way: the look finds
/// the account index as the change wrote it, and lands once the change has answered, before
/// the read the change asked for. `change` makes the change and answers it, leaving every
/// look and read waiting; the accounts are `after` once it has.
///
/// The look cannot tell this app's change from one made elsewhere, and took it for one. The
/// read after the change was then dropped as a read that started before a change, and what
/// the change did was shown only from what is already known, with no time it was read,
/// until something asked for another: the menu opening, the accounts pane shown, or the
/// timer's read minutes later. The fixture's sign-in of a Codex account met it in 2 of 140
/// runs of the whole suite. The poll leaves the account index alone until the read after the
/// change has landed, as it does while a switch runs, and a change made elsewhere after that
/// is noticed as before.
pub(super) fn a_look_landing_after(
    after: Vec<Account>,
    change: impl FnOnce(&mut Hand, &mut Machine),
) {
    let after = status(after);
    let elsewhere = status(vec![codex_account("personal", false)]);
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(vec![
        codex_account("personal", true),
        codex_account("spare", false),
    ])));
    model.refresh(&mut machine);
    model.notice(&mut machine);
    let read_at = model.shown().updated_at;

    model.later(Duration::from_secs(3));
    model.look();
    machine.changed += 1;
    machine.answer = Ok(after.clone());
    machine.offline = Ok(after.clone());
    let (reads, known) = (model.count(any_read), model.count(offline_read));
    change(&mut model, &mut machine);
    assert_eq!(
        model.count(any_read),
        reads + 1,
        "the read after the change"
    );
    let look = model.take(a_look);
    model.give(machine.answer(look));
    model.run(&mut machine);

    let shown = model.shown();
    assert_eq!(shown.status, Some(after));
    assert_ne!(shown.updated_at, read_at);
    assert_eq!(
        shown.updated_at,
        Some(model.now.epoch()),
        "the read after the change shown"
    );
    assert!(!shown.reading);
    assert_eq!(
        model.count(offline_read),
        known,
        "the change taken for its own"
    );

    model.later(Duration::from_secs(3));
    machine.changed += 1;
    machine.offline = Ok(elsewhere.clone());
    model.notice(&mut machine);
    assert_eq!(
        model.shown().status,
        Some(elsewhere),
        "a change made elsewhere since is noticed"
    );
}

#[test]
fn a_look_landing_after_naming_an_account_leaves_the_read_after_it() {
    let after = vec![
        codex_account("personal", true),
        codex_account("spare", false),
        codex_account("job", false),
    ];
    a_look_landing_after(after, |model, machine| {
        model.send(enrol("codex", "job"));
        model.run_but(machine, a_look_or_a_read);
    });
}

#[test]
fn a_look_landing_after_a_rename_leaves_the_read_after_it() {
    let after = vec![
        codex_account("personal", true),
        codex_account("home", false),
    ];
    a_look_landing_after(after, |model, machine| {
        model.send(rename("codex", "spare", "home"));
        model.run_but(machine, a_look_or_a_read);
    });
}

#[test]
fn a_look_landing_after_forgetting_leaves_the_read_after_it() {
    a_look_landing_after(vec![codex_account("personal", true)], |model, machine| {
        model.send(forget("codex/spare"));
        model.run_but(machine, a_look_or_a_read);
    });
}

#[test]
fn a_look_landing_after_updating_the_config_leaves_the_read_after_it() {
    let after = vec![
        codex_account("personal", true),
        codex_account("spare", false),
    ];
    a_look_landing_after(after, |model, machine| {
        model.send(update_config("claude/work"));
        model.run_but(machine, a_look_or_a_read);
    });
}

/// However a change of this app's own ends, it gives the account index back to the poll, and
/// a change made elsewhere after it is noticed: one that is refused or comes to nothing reads
/// nothing after it, a read after one can fail, or be dropped behind a second change, and a
/// sign-in's enrolment can fail. A change still holding the index once it was over would have
/// left the menu bar blind to every switch typed in a terminal until the app quit.
#[test]
fn however_a_change_ends_it_gives_the_index_back_to_the_poll() {
    let refused = || Err(refusal("refused", "it was refused", Vec::new()));
    type Ending = Box<dyn Fn(&mut Hand, &mut Machine)>;
    let cases: Vec<(&str, Ending)> = vec![
        (
            "a refused rename",
            Box::new(move |model, machine| {
                machine.renaming = refused();
                model.send(rename("codex", "spare", "home"));
                model.run(machine);
            }),
        ),
        (
            "a forget that came to nothing",
            Box::new(|model, _| {
                model.send(forget("codex/spare"));
                let forgetting = model.next();
                model.give(Answer::Lost(forgetting));
            }),
        ),
        (
            "a refused config update",
            Box::new(move |model, machine| {
                machine.updating_config = Err(refusal("refused", "it was refused", Vec::new()));
                model.send(update_config("claude/work"));
                model.run(machine);
            }),
        ),
        (
            "a config update that came to nothing",
            Box::new(|model, _| {
                model.send(update_config("claude/work"));
                let updating = model.next();
                model.give(Answer::Lost(updating));
            }),
        ),
        (
            "a refused name",
            Box::new(move |model, machine| {
                machine.enrolling_current = Err(refusal("refused", "refused", Vec::new()));
                model.send(enrol("codex", "job"));
                model.run(machine);
            }),
        ),
        (
            "a refused put away",
            Box::new(move |model, machine| {
                machine.leftover = Ok(Some(left(Foreseen::Kept(parked_now("work")))));
                machine.stowing = Err(refusal("refused", "refused", Vec::new()));
                model.send(Intent::PresentSheet { sheet: Sheet::Stow });
                model.run(machine);
                model.send(Intent::Stow);
                model.run(machine);
            }),
        ),
        (
            "a put away that came to nothing",
            Box::new(|model, machine| {
                machine.leftover = Ok(Some(left(Foreseen::Kept(parked_now("work")))));
                model.send(Intent::PresentSheet { sheet: Sheet::Stow });
                model.run(machine);
                model.send(Intent::Stow);
                let stowing = model.next();
                model.give(Answer::Lost(stowing));
                // The sheet looks at the file again.
                model.run(machine);
            }),
        ),
        (
            "giving up refused",
            Box::new(move |model, machine| {
                machine.abandoned = Err(refusal("refused", "refused", Vec::new()));
                model.send(Intent::AbandonStuckSwitch);
                model.run(machine);
            }),
        ),
        (
            "a rename whose read fails",
            Box::new(move |model, machine| {
                machine.answer = Err(refusal("unreachable", "could not be reached", Vec::new()));
                model.send(rename("codex", "spare", "home"));
                model.run(machine);
            }),
        ),
        (
            "two changes, the first one's read dropped",
            Box::new(|model, machine| {
                model.send(rename("codex", "spare", "home"));
                model.send(forget("codex/home"));
                model.run(machine);
            }),
        ),
        (
            "a sign-in whose enrolment fails",
            Box::new(move |model, machine| {
                machine.enrolling = Err(refusal("refused", "refused", Vec::new()));
                model.send(Intent::SignIn {
                    provider: "codex".into(),
                    name: "travel".into(),
                });
                model.run(machine);
                let id = model.shown().signing_in.expect("a sign-in").id;
                model.give(Answer::SignInQuiet { id });
                model.run(machine);
            }),
        ),
        (
            "a sign-in whose enrolment came to nothing",
            Box::new(|model, machine| {
                model.send(Intent::SignIn {
                    provider: "codex".into(),
                    name: "travel".into(),
                });
                model.run(machine);
                let id = model.shown().signing_in.expect("a sign-in").id;
                model.give(Answer::SignInQuiet { id });
                let over = model.next();
                model.give(Answer::Lost(over));
            }),
        ),
        (
            "a renewal whose read fails",
            Box::new(move |model, machine| {
                machine.answer = Err(refusal("unreachable", "could not be reached", Vec::new()));
                model.send(Intent::RenewNow);
                model.run(machine);
            }),
        ),
    ];
    for (case, change) in cases {
        let mut model = Hand::new();
        let mut machine = Machine::reading(Ok(status(vec![
            codex_account("personal", true),
            codex_account("spare", false),
        ])));
        model.refresh(&mut machine);
        model.notice(&mut machine);
        change(&mut model, &mut machine);
        assert_eq!(model.pending(), 0, "{case}: over");

        let elsewhere = status(vec![codex_account("personal", false)]);
        machine.changed += 1;
        machine.offline = Ok(elsewhere.clone());
        model.notice(&mut machine);
        assert_eq!(model.shown().status, Some(elsewhere), "{case}");
    }
}

/// A look that lands while a rename is being made finds the account under its new name before
/// the rename has answered. Taken for a rename made elsewhere, it put away what the switch to
/// that account had said, since no account of the old name was in use, and told again that
/// the account had run out, as advice about an account it had not told of. The rename then
/// carried nothing. The poll leaves the account index alone from the moment a change is asked
/// for, as it does for a switch.
#[test]
fn a_look_while_a_rename_is_made_leaves_what_was_said_about_the_account() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(vec![
        claude("work", true, 100.0),
        claude("personal", false, 10.0),
    ])));
    machine.switched = switched("claude", "personal", "work", vec![still_running()]);
    switch(&mut model, &mut machine, "claude/work");
    model.notice(&mut machine);
    assert_eq!(tos(&model), ["work"]);
    assert_eq!(machine.posted.len(), 1, "told that work ran out");

    let renamed = status(vec![
        claude("office", true, 100.0),
        claude("personal", false, 10.0),
    ]);
    model.send(rename("claude", "work", "office"));
    let renaming = model.take(|job| matches!(job, Job::Rename { .. }));
    machine.changed += 1;
    machine.answer = Ok(renamed.clone());
    machine.offline = Ok(renamed.clone());
    model.notice(&mut machine);
    model.give(machine.answer(renaming));
    model.run(&mut machine);

    let shown = model.shown();
    assert_eq!(shown.status, Some(renamed));
    assert_eq!(
        tos(&model),
        ["office"],
        "what the switch said, under the new name"
    );
    assert_eq!(shown.last_switches[0].warnings, [still_running()]);
    assert_eq!(machine.posted.len(), 1, "and not told again");
    assert_eq!(
        model.state.told,
        Told::from([("claude/office/session/".to_owned(), 100)])
    );
}

/// A rename changes what an account is called and nothing else about it. What its tool's
/// last switch said is still true of it, and so is advice about it running out, so both are
/// said under the new name, and one tool's rename says nothing about another tool's accounts.
/// Keyed by the old name, the read after a rename took the switch for undone and the advice
/// for new, and told it again.
///
/// AppModelTests.swift's aRenameCarriesWhatWasSaidAboutTheAccount.
#[test]
fn a_rename_carries_what_was_said_about_the_account() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(vec![
        claude("work", true, 100.0),
        claude("personal", false, 10.0),
        codex_account("work", true),
        codex_account("personal", false),
    ])));
    machine.switched = switched("claude", "personal", "work", Vec::new());
    switch(&mut model, &mut machine, "claude/work");
    machine.switched = switched("codex", "codex/personal", "codex/work", Vec::new());
    switch(&mut model, &mut machine, "codex/work");
    assert_eq!(tos(&model), ["work", "codex/work"]);
    let from = |model: &Hand| {
        model.shown().last_switches[1]
            .restart
            .as_ref()
            .map(|restart| restart.from.clone())
    };
    let advice = |model: &Hand| model.state.advice.first().cloned().expect("advice");
    assert_eq!(from(&model).as_deref(), Some("personal"));
    assert_eq!(advice(&model).switch_to, "claude/personal");

    // Each read after a rename fails here, so what is said is what the rename carried.
    machine.answer = Err(refusal("unreachable", "could not be reached", Vec::new()));

    model.send(rename("claude", "personal", "spare"));
    model.run(&mut machine);
    assert_eq!(
        (advice(&model).instead, advice(&model).switch_to),
        ("spare".into(), "claude/spare".into())
    );
    assert_eq!(
        from(&model).as_deref(),
        Some("personal"),
        "Codex's is another account"
    );
    model.send(rename("codex", "personal", "home"));
    model.run(&mut machine);
    assert_eq!(from(&model).as_deref(), Some("home"));
    assert_eq!(
        advice(&model).instead,
        "spare",
        "Claude Code's is another account"
    );
    model.send(rename("codex", "work", "job"));
    model.run(&mut machine);
    assert_eq!(tos(&model), ["work", "codex/job"]);
    assert_eq!(advice(&model).ran, "work");
    model.send(rename("claude", "work", "office"));
    model.run(&mut machine);
    assert_eq!(tos(&model), ["office", "codex/job"]);
    assert_eq!(advice(&model).ran, "office");
    let told = Told::from([("claude/office/session/".to_owned(), 100)]);
    assert_eq!(model.state.told_this_launch, told);
    assert_eq!(model.state.told, told);
    assert_eq!(machine.kept.last(), Some(&told), "and kept so");

    machine.answer = Ok(status(vec![
        claude("office", true, 100.0),
        claude("spare", false, 10.0),
        codex_account("job", true),
        codex_account("home", false),
    ]));
    model.refresh(&mut machine);
    assert_eq!(tos(&model), ["office", "codex/job"], "still in use");
    let notice = model
        .shown()
        .notices
        .into_iter()
        .find(|notice| notice.id == "switch/codex")
        .expect("Codex's switch");
    assert!(
        notice
            .lines
            .iter()
            .any(|line| line.contains("keeps using home")),
        "{notice:?}"
    );
    let offered: Vec<String> = model
        .state
        .advice
        .iter()
        .map(|advice| advice.switch_to.clone())
        .collect();
    assert_eq!(offered, ["claude/spare"]);
    assert_eq!(model.state.told, told, "and not told again");
    assert_eq!(machine.posted.len(), 1);
}

// What the Swift model did not do.

/// A name is saved without the white space around it, as the sheet's Save offers it, and a
/// name with nothing in it, or a rename to the name an account has, saves nothing.
#[test]
fn a_name_is_saved_as_its_sheet_offers_it() {
    let mut model = Hand::new();
    assert_eq!(model.send(enrol("claude", " \n")), []);
    assert_eq!(model.send(rename("claude", "work", " work ")), []);
    assert_eq!(
        model.send(enrol("codex", " job\t")),
        [Job::Enrol {
            provider: "codex".into(),
            name: "job".into(),
            from: None,
        }]
    );
    assert_eq!(
        model.send(rename("claude", "work", " office ")),
        [Job::Rename {
            provider: "claude".into(),
            label: "work".into(),
            to: "office".into(),
            from: None,
        }]
    );
}

/// A name being saved cannot be withdrawn, so its sheet says it is saving, which holds its
/// buttons back, and a second Save from it saves nothing until the first has answered, as
/// NameSheets.swift's `saving` held them back.
#[test]
fn a_sheet_saving_a_name_holds_back_until_it_answers() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(Vec::new())));
    put_up(&mut model, &mut machine, naming("claude", "a@example.com"));
    let jobs = model.send(enrol("claude", "work"));
    assert_eq!(jobs.len(), 1);
    assert!(model.shown().sheet_text.expect("the sheet").saving);
    assert_eq!(model.send(enrol("claude", "work")), [], "once");

    machine.enrolling_current = Err(refusal("label_taken", "Taken.", Vec::new()));
    let job = model.next();
    let answer = machine.answer(job);
    model.give(answer);
    let shown = model.shown();
    assert!(!shown.sheet_text.expect("the sheet").saving);
    assert!(shown.sheet_failure.is_some());
    assert_eq!(model.send(enrol("claude", "work")).len(), 1, "and again");
}

/// What a sheet could not save once it has gone, closed or replaced while the save ran, is
/// said in the window: something asked for that did not happen is said somewhere. The Swift
/// handed it back to the sheet that asked, which had gone, so it was said nowhere.
///
/// Reaches people in PR 10.
#[test]
fn a_name_that_cannot_be_saved_once_its_sheet_has_gone_is_said_in_the_window() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(Vec::new())));
    machine.enrolling_current = Err(refusal("label_taken", "Taken.", Vec::new()));
    machine.renaming = Err(refusal("label_taken", "Taken too.", Vec::new()));

    put_up(&mut model, &mut machine, naming("claude", "a@example.com"));
    model.send(enrol("claude", "work"));
    model.send(Intent::CloseSheet);
    let requests = model.shown().window_request.serial;
    model.run(&mut machine);
    let shown = model.shown();
    assert_eq!(shown.sheet_failure, None);
    let failure = shown.failure.expect("said in the window");
    assert_eq!(failure.title, "Couldn’t name this account");
    assert_eq!(shown.window_request.serial, requests + 1);

    put_up(&mut model, &mut machine, renaming("claude", "work"));
    model.send(rename("claude", "work", "office"));
    put_up(&mut model, &mut machine, Sheet::Add { provider: None });
    let shown = model.shown();
    assert_eq!(shown.sheet_failure, None, "not in another sheet");
    assert_eq!(
        shown.failure.map(|failure| failure.title),
        Some("Couldn’t rename work".into())
    );
}

/// A save whose job came to nothing is said as a failure of its own, where it was asked.
#[test]
fn a_save_that_came_to_nothing_says_so() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(Vec::new())));
    put_up(&mut model, &mut machine, renaming("claude", "work"));
    let jobs = model.send(rename("claude", "work", "office"));
    let job = jobs.into_iter().next().expect("the rename");
    model.give(Answer::Lost(job));
    let shown = model.shown();
    let failure = shown.sheet_failure.expect("said in the sheet");
    assert_eq!(failure.title, "Couldn’t rename work");
    assert!(failure.message.starts_with("Pitboard stopped before"));
    assert!(!shown.sheet_text.expect("the sheet").saving);
}

/// `work` in use and `spare` beside it, as the reads in the config update's tests have them.
fn work_in_use() -> Vec<Account> {
    vec![
        account(Some("work")).signed_in().build(),
        account(Some("spare")).build(),
    ]
}

fn names_another() -> crate::Warning {
    warning(
        "config_names_another",
        "Claude Code’s config names `spare`, and the login Claude Code has stored is `work`’s.",
    )
}

/// The button on the notice that says Claude Code's config names another account.
fn the_update(model: &Hand) -> Intent {
    model
        .shown()
        .notices
        .into_iter()
        .flat_map(|notice| notice.actions)
        .find(|action| action.title == "Update Claude Code’s Config")
        .expect("offered")
        .intent
}

fn titles(model: &Hand) -> Vec<String> {
    model
        .shown()
        .notices
        .into_iter()
        .map(|notice| notice.title)
        .collect()
}

/// Writing the account in use into Claude Code's config moves no login, so it is no switch:
/// the row of the account in use never reads as switching while it runs, no account is
/// switched to, and no notice says "Switched to work". What it warned of that the read after
/// it says as well, such as a file behind the keychain, which every change and read carries,
/// is said once.
#[test]
fn updating_claude_codes_config_is_no_switch() {
    let behind = warning(
        "fallback_login",
        "/Users/you/.claude/.credentials.json holds another Claude Code login.",
    );
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(warned(
        work_in_use(),
        vec![names_another(), behind.clone()],
    )));
    model.refresh(&mut machine);

    model.send(the_update(&model));
    let shown = model.shown();
    assert_eq!(shown.switch_under_way, None);
    let rows: Vec<(String, bool)> = shown
        .sections
        .into_iter()
        .flat_map(|section| section.accounts)
        .map(|row| (row.summary, row.switching))
        .collect();
    assert!(
        rows.iter()
            .all(|(summary, switching)| summary != "Switching…" && !switching),
        "{rows:?}"
    );

    machine.updating_config = Ok(vec![behind.clone()]);
    machine.answer = Ok(warned(work_in_use(), vec![behind]));
    model.run(&mut machine);
    assert_eq!(machine.configs_updated, ["claude/work"]);
    assert!(machine.switched_to.is_empty());
    assert!(model.shown().last_switches.is_empty());
    assert_eq!(titles(&model), ["A login file is left behind the keychain"]);
}

/// A config that could not be written is said beside the read after the update, which still
/// says the config names another account, under a title of its own and never as a switch,
/// until the next read.
#[test]
fn a_config_that_could_not_be_updated_is_said_beside_the_read_after() {
    let not_written = warning(
        "config_write_failed",
        "Claude Code's config at /Users/you/.claude.json could not be updated (disk full).",
    );
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(warned(work_in_use(), vec![names_another()])));
    model.refresh(&mut machine);

    machine.updating_config = Ok(vec![not_written.clone()]);
    model.send(the_update(&model));
    model.run(&mut machine);
    assert_eq!(
        titles(&model),
        [
            "Claude Code’s config names another account",
            "Pitboard has a warning"
        ]
    );
    assert_eq!(model.shown().notices[1].lines, [not_written.message]);
    assert!(model.shown().last_switches.is_empty());

    model.refresh(&mut machine);
    assert_eq!(
        titles(&model),
        ["Claude Code’s config names another account"]
    );
}

/// Pressed once another account is in use, as after a switch that landed before the press,
/// the update writes nothing and says so in the window, as a refused change is said.
#[test]
fn a_config_update_refused_is_said_in_the_window() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(warned(work_in_use(), vec![names_another()])));
    model.refresh(&mut machine);

    machine.updating_config = Err(refusal(
        "account_not_in_use",
        "`work` is not signed in now, so Pitboard left Claude Code's config as it is.",
        Vec::new(),
    ));
    model.send(update_config("claude/work"));
    model.run(&mut machine);
    let shown = model.shown();
    let refused = shown.failure.expect("said");
    assert_eq!(refused.title, "Couldn’t update Claude Code’s config");
    assert_eq!(refused.code.as_deref(), Some("account_not_in_use"));
    assert!(shown.last_switches.is_empty());
    assert!(machine.switched_to.is_empty());
}
