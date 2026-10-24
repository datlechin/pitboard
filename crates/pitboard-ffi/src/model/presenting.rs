//! What the menu bar, the menu and the window say, as the Swift app said it. The state is
//! driven through `State::apply` by hand and read as `present` makes it, its clock times in
//! UTC.

use super::signing::{add, adding_in_claude, signs_in, travel};
use super::state::{Answer, Job};
use super::switching::switch;
use super::testing::machine::{Hand, Stub, desktop_machine};
use super::testing::platform::StandInApps;
use super::testing::records::{
    CHATGPT, awaiting, chatgpt_holding, claude_code, codex, desktop_account, desktop_warning,
    enrolled_as, refusal, status, switched, titled, warned, warning,
};
use super::{Intent, Pane, Sheet, Snapshot, Tip, WindowRequest};
use crate::account_windows::forget_message_on;
use crate::present::testing::{LimitExt, Unreadable, account, unplaced, window};
use crate::present::{
    AccountItem, AccountMenuShown, AccountsShown, Choice, Footing, ItemOffer, MenuEntry,
    NoticeAction, PaceStanding, PanelNotice, Question, SetupLink, SetupStep, Severity, WindowOffer,
    present,
};
use crate::{Abandoned, Account, EnrolledAs, GaveUp, Limit, Warning};
use pitboard_core::host::OS;
use std::time::Duration;

/// 12:00 UTC on Wednesday 14 January 2026, a day far from any change of clocks.
const NOON: i64 = 1_768_392_000;

fn at_noon() -> Hand {
    let mut model = Hand::new();
    model.now.epoch_ms = NOON * 1000;
    model
}

/// The model at noon, having read `accounts`.
fn reading(accounts: Vec<Account>) -> (Hand, Stub) {
    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(status(accounts)));
    model.refresh(&mut machine);
    (model, machine)
}

/// Every row the model shows, section by section.
fn rows(model: &Hand) -> Vec<AccountItem> {
    model
        .shown()
        .sections
        .into_iter()
        .flat_map(|section| section.accounts)
        .collect()
}

/// `account` as its row describes it at noon, with nothing else under way.
fn described(account: Account) -> AccountItem {
    rows(&reading(vec![account]).0).remove(0)
}

/// `account` as its row describes it at noon, while `under_way` is.
fn described_while(account: Account, under_way: impl FnOnce(&mut Hand)) -> AccountItem {
    let (mut model, _) = reading(vec![account]);
    under_way(&mut model);
    rows(&model).remove(0)
}

/// A switch to `qualified` under way, waiting on what holds its tool's login.
fn switching(qualified: &str) -> impl FnOnce(&mut Hand) + '_ {
    move |model| {
        model.send(Intent::SwitchTo {
            qualified: qualified.into(),
        });
    }
}

/// A sign-in under way, waiting on its tool to start.
fn signing_in(model: &mut Hand) {
    model.send(Intent::SignIn {
        provider: "claude".into(),
        name: "travel".into(),
    });
}

/// What the action of a row sends, if it has one.
fn sends(row: &AccountItem) -> Option<Intent> {
    row.action.as_ref().map(|action| action.intent.clone())
}

/// What an account's own menu offers above its windows, by title, and whether each can be
/// chosen now.
fn offered(row: &AccountItem) -> Vec<(&str, bool)> {
    row.offers
        .iter()
        .map(|offer| (offer.title.as_str(), offer.enabled))
        .collect()
}

fn present_sheet(sheet: Sheet) -> Intent {
    Intent::PresentSheet { sheet }
}

fn interrupted() -> Warning {
    warning(
        "recovery_undetermined",
        "A switch from personal to work was interrupted and cannot be finished yet.",
    )
}

fn overridden() -> Warning {
    warning("auth_overridden", "ANTHROPIC_API_KEY is set")
}

fn ids(shown: &Snapshot) -> Vec<String> {
    shown
        .notices
        .iter()
        .map(|notice| notice.id.clone())
        .collect()
}

fn give_up() -> NoticeAction {
    NoticeAction {
        title: titled("Give Up…", "Give up…"),
        intent: Intent::AbandonStuckSwitch,
        dismisses: false,
        switches: false,
        enabled: true,
        confirm: Some(Question {
            title: "Give up on the interrupted switch?".into(),
            message: "Every login is kept, and nothing is deleted. Pitboard stops trying to \
                      finish it."
                .into(),
            confirm: titled("Give Up", "Give up"),
        }),
    }
}

fn dismiss(intent: Intent) -> NoticeAction {
    NoticeAction {
        title: "Dismiss".into(),
        intent,
        dismisses: true,
        switches: false,
        enabled: true,
        confirm: None,
    }
}

fn notice(
    id: &str,
    severity: Severity,
    title: &str,
    lines: &[&str],
    actions: Vec<NoticeAction>,
) -> PanelNotice {
    PanelNotice {
        id: id.into(),
        severity,
        spoken_severity: match severity {
            Severity::Info => "Note",
            Severity::Warning => "Warning",
            Severity::Error => "Problem",
        }
        .into(),
        title: title.into(),
        lines: lines.iter().map(|&line| line.to_owned()).collect(),
        until: None,
        until_label: None,
        actions,
    }
}

// One account, as the menu and the window describe it.

/// A row is called what a person called the account. One with no name yet is called by its
/// address, the one thing about it they will recognise, and a login Pitboard cannot use by
/// what is wrong with it, since it has neither.
#[test]
fn an_account_is_called_by_its_name_or_its_address_or_what_is_wrong_with_it() {
    assert_eq!(described(account(Some("work")).build()).title, "work");
    assert_eq!(
        described(account(None).signed_in().uuid("u").build()).title,
        "u@example.com"
    );
    assert_eq!(
        described(unplaced("codex", false)).title,
        "Login Pitboard can’t use"
    );
}

/// The menu and VoiceOver say the plan after the name, and an account with none is its name.
#[test]
fn an_account_carries_the_plan_its_login_says() {
    let mut team = account(Some("work")).signed_in().build();
    team.plan = Some("Team 5x".into());
    let team = described(team);
    assert_eq!(team.plan.as_deref(), Some("Team 5x"));
    assert_eq!(team.menu_title, "work · Team 5x");
    assert_eq!(team.spoken, "work, Team 5x, in use");
    assert_eq!(team.spoken_name, "work");
    let spare = described(account(Some("spare")).build());
    assert_eq!((spare.plan, spare.menu_title.as_str()), (None, "spare"));
}

/// The window's row draws every limit and says whose address it is and whether it is in
/// use, from the row alone.
#[test]
fn a_row_carries_the_address_whether_it_is_in_use_and_every_limit() {
    let in_use = described(
        account(Some("work"))
            .signed_in()
            .limits(vec![window("session", 12.0), window("weekly_all", 64.0)])
            .build(),
    );
    assert_eq!(in_use.email, "work@example.com");
    assert!(in_use.in_use);
    let names: Vec<&str> = in_use.limits.iter().map(|l| l.short.as_str()).collect();
    assert_eq!(names, ["5h", "week"]);
    assert_eq!(in_use.spoken, "work, in use");
    assert!(!described(account(Some("spare")).build()).in_use);
    assert!(
        described(unplaced("codex", false)).limits.is_empty(),
        "nothing measured"
    );
}

/// Pressing an account in the menu or the window does the one thing its state allows,
/// decided in one place so the two cannot disagree: switch to it, sign in to it again, name
/// it, or nothing at all.
#[test]
fn pressing_an_account_does_the_one_thing_its_state_allows() {
    let spare = described(account(Some("spare")).build());
    assert_eq!(
        sends(&spare),
        Some(Intent::SwitchTo {
            qualified: "claude/spare".into()
        })
    );
    let action = spare.action.as_ref().expect("an action");
    assert_eq!(
        (action.title.as_str(), action.spoken.as_str()),
        ("Use", "Use spare")
    );
    assert_eq!(
        spare.offers.first().map(|offer| offer.title.as_str()),
        Some("Use spare"),
        "as the account's own menu says it"
    );
    assert_eq!(
        sends(&described(account(Some("spare")).of("codex").build())),
        Some(Intent::SwitchTo {
            qualified: "codex/spare".into()
        }),
        "by its label with its tool, since two tools can each have a spare"
    );
    assert_eq!(
        sends(&described(account(Some("work")).signed_in().build())),
        None,
        "already in use"
    );
    let stale = described(account(Some("stale")).switchable(false).build());
    assert_eq!(
        sends(&stale),
        Some(present_sheet(Sheet::SignInAgain {
            provider: "claude".into(),
            label: "stale".into()
        }))
    );
    assert_eq!(
        stale.action.map(|action| action.spoken),
        Some(titled("Sign In to stale Again…", "Sign in to stale again…"))
    );
    let unnamed = described(account(None).of("codex").signed_in().uuid("u").build());
    assert_eq!(
        sends(&unnamed),
        Some(present_sheet(Sheet::Name {
            provider: "codex".into(),
            email: "u@example.com".into()
        }))
    );
    assert_eq!(
        unnamed.action.map(|action| (action.title, action.spoken)),
        Some(("Name…".into(), "Name u@example.com…".into()))
    );
    assert_eq!(
        sends(&described(account(None).uuid("u").build())),
        None,
        "only the login signed in now can be named"
    );
    assert_eq!(sends(&described(unplaced("codex", false))), None);
    assert_eq!(sends(&described(unplaced("codex", true))), None);
}

/// Each change waits for the one before, so while a switch runs nothing can be pressed, and
/// only the account it is for says it is switching.
#[test]
fn a_switch_running_holds_back_every_account_and_marks_only_its_own() {
    let running = "claude/spare";
    let spare = described_while(
        account(Some("spare"))
            .limits(vec![window("session", 5.0)])
            .build(),
        switching(running),
    );
    assert!(spare.switching);
    assert_eq!(spare.summary, "Switching…");
    assert_eq!(spare.action, None);

    let other = described_while(
        account(Some("other"))
            .limits(vec![window("session", 5.0)])
            .build(),
        switching(running),
    );
    assert!(!other.switching);
    assert_eq!(other.summary, "5-hour 5%");
    assert_eq!(other.action, None);
    // Its own menu holds its switch back too, and what else it offers it offers as the Swift
    // menu did, which held nothing else back for a switch.
    let again = titled("Sign In Again…", "Sign in again…");
    assert_eq!(
        offered(&other),
        [
            ("Use other", false),
            (again.as_str(), true),
            ("Rename…", true)
        ]
    );
    assert!(other.forget.as_ref().is_some_and(|forget| forget.enabled));
    let unnamed = || account(None).signed_in().uuid("u").build();
    assert_eq!(described_while(unnamed(), switching(running)).action, None);
    assert!(
        !described_while(
            account(Some("spare")).of("codex").build(),
            switching(running)
        )
        .switching,
        "Codex's spare is another account"
    );
    assert!(!described_while(unnamed(), switching(running)).switching);
}

/// A sign-in waits on somebody in a browser. It holds back another sign-in and nothing else:
/// switching meanwhile, or naming the login signed in now, is theirs to do.
#[test]
fn a_sign_in_running_holds_back_only_another_sign_in() {
    let spare = described_while(account(Some("spare")).build(), signing_in);
    assert_eq!(
        sends(&spare),
        Some(Intent::SwitchTo {
            qualified: "claude/spare".into()
        })
    );
    // Its own menu shows signing in again, held back, beside what can still be done.
    let again = titled("Sign In Again…", "Sign in again…");
    assert_eq!(
        offered(&spare),
        [
            ("Use spare", true),
            (again.as_str(), false),
            ("Rename…", true)
        ]
    );
    assert!(spare.forget.as_ref().is_some_and(|forget| forget.enabled));
    assert_eq!(
        sends(&described_while(
            account(None).signed_in().uuid("u").build(),
            signing_in
        )),
        Some(present_sheet(Sheet::Name {
            provider: "claude".into(),
            email: "u@example.com".into()
        }))
    );
    let stale = described_while(account(Some("stale")).switchable(false).build(), signing_in);
    assert_eq!(sends(&stale), None);
    assert_eq!(
        offered(&stale),
        [
            ("Use stale", false),
            (again.as_str(), false),
            ("Rename…", true)
        ]
    );
}

/// What an account's own menu offers, each with its words, what it sends, and whether it can
/// be chosen now: naming it, where pressing the account does that, or, for an account
/// Pitboard has a name for, switching to it, signing in to it again and renaming it; each of
/// its windows; and forgetting it, asked first. Every enrolled account has the same items,
/// dimmed where they cannot be chosen now, as the menu bar's menu keeps them. Signing in
/// again is offered once, not again as the account's own action, and a login Pitboard cannot
/// use offers nothing.
///
/// AccountsPane.swift's shortcut menu, as it was at c1be7c3, which the Swift tested nowhere,
/// and which AccountsWindowTests.swift's testRenamingAnAccount, testForgettingAsksFirst and
/// testTheAccountInUseOffersNoForget read.
#[test]
fn an_accounts_own_menu_offers_what_can_be_done_to_it() {
    let again = |label: &str| {
        present_sheet(Sheet::SignInAgain {
            provider: "claude".into(),
            label: label.into(),
        })
    };
    let rename = |label: &str| {
        present_sheet(Sheet::Rename {
            provider: "claude".into(),
            label: label.into(),
        })
    };
    let sign_in_again = titled("Sign In Again…", "Sign in again…");
    let offer = |title: &str, intent: Intent| ItemOffer {
        title: title.into(),
        intent,
        enabled: true,
        confirm: None,
    };
    let dimmed = |title: &str, intent: Intent| ItemOffer {
        enabled: false,
        ..offer(title, intent)
    };
    let switch = |label: &str| Intent::SwitchTo {
        qualified: format!("claude/{label}"),
    };
    let spare = account(Some("spare")).build();
    let accounts = vec![
        account(Some("work")).signed_in().build(),
        spare.clone(),
        account(Some("stale")).switchable(false).build(),
    ];
    let (model, _) = reading(accounts.clone());
    let [work, spare_row, stale] = &rows(&model)[..] else {
        panic!("three rows");
    };

    assert_eq!(
        work.offers,
        [
            dimmed("Use work", switch("work")),
            offer(&sign_in_again, again("work")),
            offer("Rename…", rename("work"))
        ],
        "nothing to switch to: it is the one in use"
    );
    assert_eq!(
        work.forget.as_ref().map(|forget| forget.enabled),
        Some(false),
        "the core refuses to forget the one in use"
    );

    assert_eq!(
        spare_row.offers,
        [
            offer(
                "Use spare",
                Intent::SwitchTo {
                    qualified: "claude/spare".into()
                }
            ),
            offer(&sign_in_again, again("spare")),
            offer("Rename…", rename("spare")),
        ]
    );
    assert_eq!(
        spare_row.forget,
        Some(ItemOffer {
            title: "Forget…".into(),
            intent: Intent::Forget {
                qualified: "claude/spare".into()
            },
            enabled: true,
            confirm: Some(Question {
                title: "Forget “spare”?".into(),
                message: forget_message_on(OS, &spare, &accounts),
                confirm: "Forget".into(),
            }),
        })
    );

    assert_eq!(
        stale.offers,
        [
            dimmed("Use stale", switch("stale")),
            offer(&sign_in_again, again("stale")),
            offer("Rename…", rename("stale"))
        ],
        "its own action, offered once"
    );
    assert!(stale.forget.is_some());

    let unnamed = described(account(None).of("codex").signed_in().uuid("u").build());
    assert_eq!(
        unnamed.offers,
        [offer(
            "Name…",
            present_sheet(Sheet::Name {
                provider: "codex".into(),
                email: "u@example.com".into()
            })
        )]
    );
    assert_eq!(unnamed.forget, None);
    assert!(
        described(account(None).uuid("u").build()).offers.is_empty(),
        "a login not signed in has no name and cannot be named from here"
    );
    for signed_in in [false, true] {
        let cannot = described(unplaced("codex", signed_in));
        assert!(cannot.offers.is_empty());
        assert!(cannot.windows.is_empty());
        assert_eq!(cannot.forget, None);
    }
}

/// With no account selected, the menu bar's Account menu keeps the items an enrolled account
/// has, dimmed, in the system's case.
#[test]
fn the_account_menu_with_nothing_selected_keeps_its_items() {
    let (model, _) = reading(vec![account(Some("work")).signed_in().build()]);
    assert_eq!(
        model.on_a_mac().account_menu,
        AccountMenuShown {
            offers: vec!["Use".into(), "Sign In Again…".into(), "Rename…".into()],
            forget: "Forget…".into(),
        }
    );
    assert_eq!(model.on_windows().account_menu.offers[1], "Sign in again…");
}

/// An account's own menu opens each of its windows, one per site of its tool, named by the
/// site, and the window is the one the menus and the picker list for it: not that of another
/// tool's account with the same account id. A login with no name, or one Pitboard cannot
/// use, has none.
///
/// AccountsPane.swift's Open items, which asked `windowsOf` and worded each in Swift, and
/// which AccountWindowTests.swift's testAnAccountsShortcutMenuOpensItsWindow reads.
#[test]
fn an_accounts_own_menu_opens_its_windows() {
    let accounts = vec![
        account(Some("work")).signed_in().uuid("same").build(),
        account(Some("main"))
            .of("codex")
            .signed_in()
            .uuid("same")
            .build(),
    ];
    let (model, _) = reading(accounts.clone());
    let listed = crate::window_accounts(accounts);
    let [work, main] = &rows(&model)[..] else {
        panic!("two rows");
    };
    assert_eq!(
        work.windows,
        [WindowOffer {
            title: "Open claude.ai".into(),
            window: listed[0].clone(),
        }]
    );
    assert_eq!(
        main.windows,
        [WindowOffer {
            title: "Open chatgpt.com".into(),
            window: listed[1].clone(),
        }]
    );
    assert_eq!(main.windows[0].window.site.host, "chatgpt.com");
    assert!(
        described(account(None).signed_in().uuid("u").build())
            .windows
            .is_empty()
    );
}

/// Whether an account needs signing in again is a fact about the account, and a sign-in or a
/// switch running elsewhere does not change it. Its item cannot be chosen meanwhile, and
/// without the line saying why it reads as an account that simply does not work.
#[test]
fn an_account_that_needs_signing_in_again_says_so_while_something_else_runs() {
    let stale = || {
        account(Some("stale"))
            .switchable(false)
            .limits(vec![window("session", 12.0)])
            .build()
    };
    assert_eq!(described(stale()).summary, "Needs signing in again");
    assert_eq!(
        described_while(stale(), signing_in).summary,
        "Needs signing in again"
    );
    let switch_elsewhere = described_while(stale(), switching("claude/spare"));
    assert_eq!(switch_elsewhere.summary, "Needs signing in again");
    assert_eq!(switch_elsewhere.spoken, "stale, needs signing in again");
}

/// An account whose login may be the one in use that could not be read is not said to need
/// signing in again: the core says its login could not be read, and that is the line.
#[test]
fn an_account_whose_login_could_not_be_read_is_not_said_to_need_signing_in() {
    let why = "Claude Code's login could not be read; run `pitboard doctor`";
    let unread = Account {
        stale: Some("login_unreadable".into()),
        ..account(Some("beta"))
            .switchable(false)
            .unmeasured()
            .explained(why)
            .build()
    };
    let said = described(unread);
    assert!(!said.needs_sign_in);
    assert_eq!(said.summary, why);
    assert_eq!(said.spoken, "beta");
}

/// The line under an account's name in the menu says what stands in its way before
/// anything about its limits, since that is what decides whether it can be chosen.
#[test]
fn the_menu_says_what_stands_in_an_accounts_way_before_its_limits() {
    let limits = || vec![window("session", 12.0)];
    let login = unplaced("codex", false);
    assert_eq!(
        Some(described(login.clone()).summary),
        login.stale_explanation
    );
    let unexplained = Account {
        stale_explanation: None,
        ..login
    };
    assert_eq!(described(unexplained).summary, "Can’t be read or switched");
    assert_eq!(
        described(account(None).signed_in().uuid("u").limits(limits()).build()).summary,
        "Not named yet"
    );
    assert_eq!(
        described(
            account(Some("stale"))
                .switchable(false)
                .limits(limits())
                .build()
        )
        .summary,
        "Needs signing in again"
    );
}

/// An account with nothing measured yet still has a line under its name, and its address is
/// the one true thing to put there.
#[test]
fn an_account_with_nothing_measured_is_described_by_its_address() {
    assert_eq!(
        described(account(Some("spare")).build()).summary,
        "spare@example.com"
    );
    assert_eq!(
        described(account(Some("spare")).unmeasured().build()).summary,
        "spare@example.com"
    );
    assert_eq!(
        described(account(Some("work")).signed_in().build()).summary,
        "work@example.com"
    );
}

/// Every limit in one line, in the order the service gave them, the way a sentence starts: a
/// capital on its first word and on nothing after it. A limit scoped to one model is named
/// with its model, so it does not read as the account's own.
#[test]
fn limits_are_said_in_one_line_with_only_its_first_letter_capitalised() {
    let summary =
        |limits: Vec<Limit>| described(account(Some("work")).limits(limits).build()).summary;
    assert_eq!(
        summary(vec![window("weekly_all", 64.4), window("session", 12.0)]),
        "Weekly 64%, 5-hour 12%"
    );
    assert_eq!(
        summary(vec![
            window("session", 12.5),
            window("weekly_all", 30.0),
            window("weekly_scoped", 98.0).scope("Fable"),
        ]),
        "5-hour 13%, weekly 30%, weekly Fable 98%"
    );
    assert_eq!(
        summary(vec![window("weekly_scoped", 7.0).scope("Fable")]),
        "Weekly Fable 7%"
    );
}

/// A limit that has run out is worth knowing with when it comes back. Later today that is a
/// clock time, and on another day its weekday too, so a menu left open past midnight is
/// still right. Used up is the core's: at the service's 100 as read, until its reset, after
/// which the line gives the last figure, as `pitboard status` does, until a reading moves it.
///
/// The clock is read in UTC.
#[test]
fn a_used_up_limit_says_when_it_comes_back() {
    let today = NOON + 2 * 3600;
    let later = NOON + 2 * 86_400;
    let spent = |percent: f64, resets: Option<i64>| {
        described(
            account(Some("work"))
                .signed_in()
                .limits(vec![window("session", percent).resets(resets)])
                .build(),
        )
        .summary
    };
    assert_eq!(spent(100.0, Some(today)), "5-hour used up until 14:00");
    assert_eq!(spent(100.0, Some(later)), "5-hour used up until Fri 12:00");
    assert_eq!(spent(100.0, None), "5-hour used up");
    assert_eq!(spent(100.0, Some(NOON - 60)), "5-hour 100%", "it has reset");
    assert_eq!(spent(100.0, Some(NOON)), "5-hour 100%", "it resets now");
    assert_eq!(
        spent(99.6, Some(today)),
        "5-hour 100%",
        "drawn at 100%, and still answered"
    );
    assert_eq!(
        described(
            account(Some("work"))
                .limits(vec![
                    window("weekly_all", 100.0).resets(Some(today)),
                    window("session", 40.0),
                ])
                .build()
        )
        .summary,
        "Weekly used up until 14:00, 5-hour 40%"
    );
}

/// A clock the app's own code cannot read is said in UTC, and named so, rather than not at
/// all.
#[test]
fn a_clock_the_app_cannot_read_is_said_in_utc_rather_than_not_at_all() {
    let today = NOON + 2 * 3600;
    let (model, _) = reading(vec![
        account(Some("work"))
            .signed_in()
            .limits(vec![window("session", 100.0).resets(Some(today))])
            .build(),
    ]);
    let shown = present(&model.state, NOON, &Unreadable);
    assert_eq!(
        shown.sections[0].accounts[0].summary,
        "5-hour used up until 14:00 UTC"
    );
}

/// Why an account cannot be used is said in full only where it cannot be: a login Pitboard
/// cannot use, or an account that cannot be switched to. Numbers that are merely not live do
/// not stop anybody choosing an account, and the account in use needs no switch.
#[test]
fn only_an_account_that_cannot_be_used_says_why() {
    let refused_login = "Its parked login was refused. Sign in to it again.";
    let old_numbers = "Anthropic did not answer, so these are the last numbers measured.";
    let login = unplaced("codex", false);
    assert_eq!(described(login.clone()).problem, login.stale_explanation);
    assert_eq!(
        described(
            account(Some("stale"))
                .switchable(false)
                .explained(refused_login)
                .build()
        )
        .problem
        .as_deref(),
        Some(refused_login)
    );
    assert_eq!(
        described(account(Some("stale")).switchable(false).build()).problem,
        None,
        "nothing to say"
    );
    assert_eq!(
        described(account(Some("spare")).explained(old_numbers).build()).problem,
        None,
        "it can still be switched to"
    );
    assert_eq!(
        described(
            account(Some("work"))
                .signed_in()
                .switchable(false)
                .explained(old_numbers)
                .build()
        )
        .problem,
        None,
        "it is the one in use"
    );
}

/// Numbers that are not new are worth knowing about where the account can still be used:
/// its service could not be reached or is rate limiting, or its session has expired. That is
/// said beside the numbers of the account in use and of one that can be switched to, and not
/// a second time where the account cannot be used and why is said already. The menu's help
/// says whichever there is.
#[test]
fn why_a_usable_accounts_numbers_are_old_is_said_beside_them() {
    let old_numbers = "Anthropic did not answer, so these are the last numbers measured.";
    let refused_login = "Its parked login was refused. Sign in to it again.";
    let spare = described(account(Some("spare")).explained(old_numbers).build());
    assert_eq!(spare.stale_note.as_deref(), Some(old_numbers));
    assert_eq!(spare.problem, None);
    assert_eq!(spare.help.as_deref(), Some(old_numbers));
    let in_use = described(
        account(Some("work"))
            .signed_in()
            .switchable(false)
            .explained(old_numbers)
            .build(),
    );
    assert_eq!(in_use.stale_note.as_deref(), Some(old_numbers));
    assert_eq!(in_use.problem, None);

    let expired = described(
        account(Some("stale"))
            .switchable(false)
            .explained(refused_login)
            .build(),
    );
    assert_eq!(expired.problem.as_deref(), Some(refused_login));
    assert_eq!(
        expired.stale_note, None,
        "said once, as why it cannot be used"
    );
    assert_eq!(expired.help.as_deref(), Some(refused_login));
    assert_eq!(described(unplaced("codex", false)).stale_note, None);
    assert_eq!(
        described(account(Some("spare")).build()).stale_note,
        None,
        "numbers that are new"
    );
    assert_eq!(described(account(Some("spare")).build()).help, None);
}

/// How long a parked login stays usable is about a switch to it, so it is said of every
/// account but the one in use, whose login is not parked.
#[test]
fn a_parked_logins_life_is_said_only_of_an_account_not_in_use() {
    let parking = NOON + 3 * 86_400 + 60;
    assert_eq!(
        described(account(Some("spare")).parked_until(parking).build())
            .parked_note
            .as_deref(),
        Some("Parked login good for 3 more days")
    );
    assert_eq!(
        described(
            account(Some("work"))
                .signed_in()
                .parked_until(parking)
                .build()
        )
        .parked_note,
        None
    );
    assert_eq!(described(account(Some("spare")).build()).parked_note, None);
}

const HOUR: i64 = 3_600;
const WEEK: i64 = 7 * 86_400;

/// A five-hour limit four hours in at 30%, and a week ten hours in at 33%, as of `NOON`.
fn busy_limits() -> Vec<Limit> {
    vec![
        window("session", 30.0)
            .length(5 * HOUR)
            .resets(Some(NOON + HOUR)),
        window("weekly_all", 33.0)
            .length(WEEK)
            .resets(Some(NOON + WEEK - 10 * HOUR)),
    ]
}

/// Each bar marks where an even use of its limit would be by now, in the words beside it
/// and the help over it, as `pitboard status` says it; a limit whose pace means nothing
/// marks nothing.
#[test]
fn a_bar_marks_where_an_even_pace_would_be() {
    let rows = described(
        account(Some("work"))
            .signed_in()
            .limits(busy_limits())
            .read_at(NOON)
            .build(),
    )
    .limits;
    let session = rows[0].pace.as_ref().expect("a pace");
    assert_eq!(session.standing, PaceStanding::Under);
    assert!((session.expected - 80.0).abs() < 1e-9);
    assert_eq!(session.said, "50% under pace");
    let week = rows[1].pace.as_ref().expect("a pace");
    assert_eq!(week.standing, PaceStanding::Over);
    assert_eq!(week.said, "27% over pace");
    assert!(week.help.contains("runs out in 20h 18m"), "{}", week.help);
    assert!(
        rows[1].spoken.contains("27 percent over an even pace"),
        "{}",
        rows[1].spoken
    );
    let unknown = described(
        account(Some("work"))
            .signed_in()
            .limits(vec![window("session", 30.0)])
            .read_at(NOON)
            .build(),
    );
    assert_eq!(unknown.limits[0].pace, None, "no length, no pace");
}

/// The account in use says which of its limits runs out first at its pace, as a sentence
/// in the window and beside that limit in the menu. A parked account is not being used, so
/// nothing of it runs out, and an account with no limit over pace says nothing.
#[test]
fn the_account_in_use_says_which_limit_runs_out_first() {
    let in_use = described(
        account(Some("work"))
            .signed_in()
            .limits(busy_limits())
            .read_at(NOON)
            .build(),
    );
    assert_eq!(
        in_use.pace.as_deref(),
        Some("Weekly limit runs out in 20h 18m at this pace")
    );
    assert_eq!(
        in_use.summary,
        "5-hour 30%, weekly 33% (runs out in 20h 18m)"
    );
    let parked = described(
        account(Some("spare"))
            .limits(busy_limits())
            .read_at(NOON)
            .build(),
    );
    assert_eq!(parked.pace, None);
    assert_eq!(parked.summary, "5-hour 30%, weekly 33%");
    let calm = described(
        account(Some("work"))
            .signed_in()
            .limits(vec![busy_limits().remove(0)])
            .read_at(NOON)
            .build(),
    );
    assert_eq!(calm.pace, None);
}

/// Only an enrolled account that is not the one in use may be forgotten: forgetting that one
/// would throw away the only record of who is signed in, and the core refuses it. The
/// question asked first names it, and says what forgetting it deletes in the words the
/// account windows say it in. Its menu offers it, and the window's Delete key does it.
///
/// AccountsPane.swift's canForget and its alert, which the Swift tested nowhere.
#[test]
fn only_an_enrolled_account_not_in_use_may_be_forgotten() {
    let spare = account(Some("spare")).build();
    let accounts = vec![account(Some("work")).signed_in().build(), spare.clone()];
    let (model, _) = reading(accounts.clone());
    let shown = rows(&model);
    assert!(
        shown[0]
            .forget
            .as_ref()
            .is_some_and(|forget| !forget.enabled)
    );
    assert!(
        shown[1]
            .forget
            .as_ref()
            .is_some_and(|forget| forget.enabled)
    );
    let forget = shown[1].forget.as_ref().expect("spare may be forgotten");
    assert_eq!(
        forget.intent,
        Intent::Forget {
            qualified: "claude/spare".into()
        }
    );
    assert_eq!(
        forget.confirm,
        Some(Question {
            title: "Forget “spare”?".into(),
            message: forget_message_on(OS, &spare, &accounts),
            confirm: "Forget".into(),
        })
    );
    assert_eq!(
        described(account(None).signed_in().uuid("u").build()).forget,
        None
    );
    assert_eq!(described(unplaced("codex", false)).forget, None);
}

/// An enrolled account that cannot be switched to is signed in to again from its row, in its
/// own tool. The sign-in itself is signing.rs's.
#[test]
fn an_account_that_cannot_be_switched_to_is_signed_in_to_again_from_the_panel() {
    let (model, _) = reading(vec![
        account(Some("personal")).of("codex").signed_in().build(),
        account(Some("work")).of("codex").switchable(false).build(),
        account(Some("spare")).switchable(false).build(),
    ]);
    let shown = rows(&model);
    let again = |provider: &str, label: &str| {
        Some(present_sheet(Sheet::SignInAgain {
            provider: provider.into(),
            label: label.into(),
        }))
    };
    // A section per tool, Claude Code's first.
    let actions: Vec<Option<Intent>> = shown.iter().map(sends).collect();
    assert_eq!(
        actions,
        [again("claude", "spare"), None, again("codex", "work")]
    );
    assert_eq!(
        shown[2]
            .action
            .as_ref()
            .map(|action| action.spoken.as_str()),
        Some(
            titled(
                "Sign In to work (Codex) Again…",
                "Sign in to work (Codex) again…"
            )
            .as_str()
        ),
        "named with its tool beside another tool's accounts"
    );
}

// Sections.

/// A tool Pitboard does not list yet still gets a section, after the ones it lists, headed by
/// its code, rather than its accounts going missing.
#[test]
fn a_tool_nobody_lists_is_grouped_last_under_its_code() {
    let (model, _) = reading(vec![
        account(Some("g")).of("gemini").build(),
        account(Some("job")).of("codex").build(),
        account(Some("work")).build(),
    ]);
    let sections = model.shown().sections;
    let ids: Vec<&str> = sections.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["claude", "codex", "gemini"]);
    let headings: Vec<Option<&str>> = sections.iter().map(|s| s.heading.as_deref()).collect();
    assert_eq!(
        headings,
        [Some("Claude Code"), Some("Codex"), Some("gemini")]
    );
    let titles: Vec<Vec<&str>> = sections
        .iter()
        .map(|s| s.accounts.iter().map(|a| a.title.as_str()).collect())
        .collect();
    assert_eq!(titles, [vec!["work"], vec!["job"], vec!["g"]]);
}

/// One tool's accounts are one section with no heading, known by the tool's code, with the
/// rows in the order they came.
#[test]
fn one_tools_accounts_are_one_section_known_by_its_code() {
    let (model, _) = reading(vec![
        account(Some("spare")).of("codex").build(),
        account(Some("job")).of("codex").signed_in().build(),
    ]);
    let sections = model.shown().sections;
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].id, "codex");
    assert_eq!(sections[0].heading, None);
    let ids: Vec<&str> = sections[0].accounts.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids, ["codex:spare", "codex:job"]);
}

/// One tool, whichever it is: no headings, the rows in the order the core gave them, and the
/// bar naming the first account signed in, as it always has. A login signed in with no name
/// yet is called unnamed there, which says what it is and fits where an address would not.
#[test]
fn one_tool_looks_as_it_always_did() {
    for tool in ["claude", "codex"] {
        let (model, _) = reading(vec![
            account(None)
                .of(tool)
                .signed_in()
                .uuid("u")
                .limits(vec![window("session", 80.0)])
                .build(),
            account(Some("work"))
                .of(tool)
                .limits(vec![window("session", 10.0)])
                .build(),
        ]);
        let shown = model.shown();
        assert_eq!(shown.sections.len(), 1, "{tool}");
        assert_eq!(shown.sections[0].heading, None);
        assert_eq!(shown.sections[0].accounts.len(), 2);
        assert_eq!(shown.menu_bar.name_and_usage, "unnamed 80%");
        assert!(!shown.shows_tools);
    }
    assert!(reading(Vec::new()).0.shown().sections.is_empty());
}

/// More than one tool: a section per tool, headed by its name, in the order the tools are
/// listed whatever order the rows came in.
#[test]
fn accounts_of_two_tools_are_grouped_by_tool() {
    let (model, _) = reading(vec![
        account(Some("job")).of("codex").build(),
        account(Some("work")).signed_in().build(),
        account(Some("side")).of("codex").signed_in().build(),
    ]);
    let shown = model.shown();
    assert!(shown.shows_tools);
    let headings: Vec<Option<&str>> = shown
        .sections
        .iter()
        .map(|s| s.heading.as_deref())
        .collect();
    assert_eq!(headings, [Some("Claude Code"), Some("Codex")]);
    let titles: Vec<Vec<&str>> = shown
        .sections
        .iter()
        .map(|s| s.accounts.iter().map(|a| a.title.as_str()).collect())
        .collect();
    assert_eq!(titles, [vec!["work"], vec!["job", "side"]]);
}

// The menu bar.

/// A read fills in the menu bar's title.
#[test]
fn a_read_fills_in_the_title() {
    let (model, _) = reading(vec![
        account(Some("work"))
            .signed_in()
            .limits(vec![window("session", 42.0).resets(None)])
            .build(),
    ]);
    let bar = model.shown().menu_bar;
    assert_eq!(bar.name_and_usage, "work 42%");
    assert_eq!(bar.usage, "42%");
}

/// The menu bar follows whichever account is closest to running out, and two tools can each
/// have a `work`, so once there is more than one tool VoiceOver says which.
#[test]
fn the_menu_bar_says_which_tool_it_is_about_once_there_are_two() {
    let mut model = at_noon();
    assert_eq!(model.shown().menu_bar.spoken, "Pitboard");
    let mut machine = Stub::reading(Ok(status(vec![
        account(Some("work"))
            .signed_in()
            .limits(vec![window("session", 42.0)])
            .build(),
    ])));
    model.refresh(&mut machine);
    assert_eq!(model.shown().menu_bar.spoken, "Pitboard, work 42%");

    machine.answer = Ok(status(vec![
        account(Some("work"))
            .signed_in()
            .limits(vec![window("session", 60.0)])
            .build(),
        account(Some("work"))
            .of("codex")
            .signed_in()
            .limits(vec![window("five_hour", 80.0).resets(None)])
            .build(),
    ]));
    model.refresh(&mut machine);
    assert_eq!(model.shown().menu_bar.spoken, "Pitboard, work 80%, Codex");
}

// What the window and the menu have to tell somebody.

/// A machine with nothing wrong has nothing to say, before the first read and after it.
#[test]
fn a_machine_with_nothing_wrong_has_no_notices() {
    let mut model = at_noon();
    assert!(model.shown().notices.is_empty());
    let mut machine = Stub::reading(Ok(status(vec![
        account(Some("work"))
            .signed_in()
            .limits(vec![window("session", 10.0)])
            .build(),
        account(Some("spare")).build(),
    ])));
    model.refresh(&mut machine);
    assert!(model.shown().notices.is_empty());
}

/// An interrupted switch nothing can finish stops Pitboard working, so it is said first,
/// with the way out, which asks first. The warning the read carries about it is the same
/// fact and is not said a second time; anything else the read warned about still is.
#[test]
fn an_interrupted_switch_is_said_first_with_the_way_out_and_only_once() {
    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(warned(
        vec![
            account(Some("work")).signed_in().build(),
            account(Some("job")).of("codex").signed_in().build(),
        ],
        vec![interrupted(), overridden()],
    )));
    model.refresh(&mut machine);

    let notices = model.shown().notices;
    assert_eq!(notices.len(), 2);
    assert_eq!(
        notices[0],
        notice(
            "stuck",
            Severity::Error,
            "An interrupted switch is waiting",
            &[
                "An interrupted switch can’t be finished until Anthropic or OpenAI answers.",
                "Giving up on it keeps every login. Nothing is deleted.",
            ],
            vec![give_up()],
        )
    );
    assert_eq!(
        notices[1].title,
        "An environment variable overrides the login"
    );
    assert!(
        !notices
            .iter()
            .any(|notice| notice.lines.contains(&interrupted().message))
    );
}

/// When the read itself failed because of the interrupted switch, what stopped it is the
/// reason given, and it is not said again as a read that failed.
#[test]
fn an_interrupted_switch_that_stopped_the_read_gives_its_reason() {
    let reason = "Anthropic could not be reached to finish the switch to work.";
    let mut model = at_noon();
    let mut machine = Stub::reading(Err(refusal(
        "recovery_undetermined",
        reason,
        vec![interrupted()],
    )));
    machine.offline = Ok(status(vec![account(Some("work")).signed_in().build()]));
    model.refresh(&mut machine);
    assert_eq!(
        model.shown().notices,
        [notice(
            "stuck",
            Severity::Error,
            "An interrupted switch is waiting",
            &[
                reason,
                "Giving up on it keeps every login. Nothing is deleted."
            ],
            vec![give_up()],
        )]
    );
}

/// A read that failed says why, and that the numbers shown are the last ones measured rather
/// than now's. A warning that only repeats why is not said twice; any other it carried is.
#[test]
fn a_failed_read_says_why_and_that_the_numbers_are_old() {
    let reason = "Anthropic could not be reached";
    let mut model = at_noon();
    let mut machine = Stub::reading(Err(refusal(
        "unreachable",
        reason,
        vec![warning("unreachable", reason), overridden()],
    )));
    machine.offline = Ok(status(vec![
        account(Some("work"))
            .signed_in()
            .limits(vec![window("session", 10.0)])
            .build(),
    ]));
    model.refresh(&mut machine);

    let notices = model.shown().notices;
    let titles: Vec<&str> = notices.iter().map(|n| n.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "Couldn’t read usage",
            "An environment variable overrides the login"
        ]
    );
    assert_eq!(
        notices[0],
        notice(
            "read",
            Severity::Error,
            "Couldn’t read usage",
            &[reason, "The numbers shown are the last ones measured."],
            Vec::new(),
        )
    );
    assert_eq!(notices[1].lines, [overridden().message]);
}

/// The line saying the numbers shown are the last ones measured is said only where there are
/// numbers shown. With nothing measured, or nothing known at all, it pointed at numbers that
/// were not there.
#[test]
fn a_failed_read_says_the_numbers_are_old_only_where_there_are_some() {
    let reason = "Anthropic could not be reached";
    for known in [
        Ok(status(vec![
            account(Some("work")).signed_in().unmeasured().build(),
        ])),
        Ok(status(Vec::new())),
        Err(refusal(
            "state_unreadable",
            "/Users/dana/.pitboard/state.json could not be read",
            Vec::new(),
        )),
    ] {
        let mut model = at_noon();
        let mut machine = Stub::reading(Err(refusal("unreachable", reason, Vec::new())));
        machine.offline = known;
        model.refresh(&mut machine);
        assert_eq!(model.shown().notices[0].lines, [reason]);
    }
}

/// A failed read with nothing to list is the window's whole content, with why and a way to
/// try again, and not a spinner that never stops or a list with nothing in it, whether
/// nothing is known at all or what is known is empty. A machine without Claude Code says that
/// instead. With accounts to show, the list stays, with the failure above it as a notice.
#[test]
fn a_failed_read_with_nothing_to_list_is_what_the_window_shows() {
    let reason = "/Users/dana/.pitboard/state.json was written on another Mac.";
    let failed = || Err(refusal("state_wrong_machine", reason, Vec::new()));
    let read_failed = AccountsShown::ReadFailed {
        title: titled("Couldn’t Read Accounts", "Couldn’t read accounts"),
        detail: reason.into(),
        retry: Choice {
            title: titled("Try Again", "Try again"),
            intent: Intent::Refresh { asked: true },
            enabled: true,
        },
    };

    let mut unknown = at_noon();
    let mut nothing = Stub::reading(failed());
    nothing.offline = failed();
    unknown.refresh(&mut nothing);
    let shown = unknown.shown();
    assert_eq!(shown.status, None);
    assert_eq!(
        shown.footing,
        Footing::Ready,
        "not a machine without Claude Code"
    );
    assert_eq!(shown.accounts_shown, read_failed);

    let mut empty = at_noon();
    empty.refresh(&mut Stub::reading(failed()));
    let shown = empty.shown();
    assert_eq!(shown.status.map(|s| s.accounts.len()), Some(0));
    assert_eq!(
        shown.footing,
        Footing::NoOneSignedIn,
        "the read's problem is said ahead of it"
    );
    assert_eq!(shown.accounts_shown, read_failed);

    let mut some = at_noon();
    let mut listed = Stub::reading(failed());
    listed.offline = Ok(status(vec![
        account(Some("work"))
            .signed_in()
            .limits(vec![window("session", 5.0)])
            .build(),
    ]));
    some.refresh(&mut listed);
    let shown = some.shown();
    assert_eq!(ids(&shown), ["read"]);
    assert_eq!(shown.accounts_shown, AccountsShown::List);

    let mut missing = at_noon();
    let mut bare = Stub::reading(failed());
    bare.found = Vec::new();
    missing.refresh(&mut bare);
    let shown = missing.shown();
    assert_eq!(shown.footing, Footing::NoClaudeCode);
    assert!(matches!(shown.accounts_shown, AccountsShown::NoTool { .. }));
}

/// The pane's Try Again is held back while the accounts are read again, and offered once the
/// read has answered, as the pane's toolbar Refresh is. AccountsPane.swift held it back
/// itself, from the snapshot's `reading`, which the Swift tested nowhere.
#[test]
fn the_panes_try_again_is_held_back_while_the_accounts_are_read() {
    let failed = || {
        Err(refusal(
            "state_wrong_machine",
            "/Users/dana/.pitboard/state.json was written on another Mac.",
            Vec::new(),
        ))
    };
    let mut model = at_noon();
    let mut machine = Stub::reading(failed());
    machine.offline = failed();
    model.refresh(&mut machine);
    let retry = |model: &Hand| match model.shown().accounts_shown {
        AccountsShown::ReadFailed { retry, .. } => retry,
        other => panic!("why the read failed: {other:?}"),
    };
    assert!(retry(&model).enabled);
    model.send(Intent::Refresh { asked: true });
    assert!(model.shown().reading);
    assert_eq!(
        retry(&model),
        Choice {
            title: titled("Try Again", "Try again"),
            intent: Intent::Refresh { asked: true },
            enabled: false,
        }
    );
    model.run(&mut machine);
    assert!(retry(&model).enabled, "the read answered, and failed again");
}

/// Without a tool there is nothing to read, and the menu says how to install one instead.
/// A failed read beside that would be the same fact said as a fault. Once another tool has
/// an account here the machine is not without a tool, and a failed read is one.
#[test]
fn a_machine_without_claude_code_is_not_told_its_read_failed() {
    let missing = || {
        let mut machine = Stub::reading(Err(refusal(
            "unreachable",
            "Anthropic could not be reached",
            Vec::new(),
        )));
        machine.found = Vec::new();
        machine
    };
    let mut bare = at_noon();
    bare.refresh(&mut missing());
    let shown = bare.shown();
    assert_eq!(shown.footing, Footing::NoClaudeCode);
    assert!(shown.notices.is_empty());

    let mut with_codex = at_noon();
    let mut machine = missing();
    machine.offline = Ok(status(vec![
        account(Some("job")).of("codex").signed_in().build(),
    ]));
    with_codex.refresh(&mut machine);
    assert_eq!(ids(&with_codex.shown()), ["read"]);
}

/// Sessions of a tool that follows a switch by itself pick it up within a moment, and the
/// notice counts down to it. Once that moment has passed, the notice has nothing left to say
/// that the row's In Use does not, so it goes. Dismissing it puts it away before then.
#[test]
fn a_switch_counts_down_only_until_open_sessions_have_followed() {
    let (mut model, mut machine) = reading(vec![
        account(Some("work")).signed_in().build(),
        account(Some("personal")).build(),
    ]);
    machine.switched = switched("claude", "personal", "work", Vec::new());
    switch(&mut model, &mut machine, "claude/work");
    let follows = NOON + 33;

    let counting = PanelNotice {
        until: Some(follows),
        until_label: Some("Sessions already open follow in".into()),
        ..notice(
            "switch/claude",
            Severity::Info,
            "Switched to work",
            &[],
            vec![dismiss(Intent::DismissSwitch {
                provider: "claude".into(),
            })],
        )
    };
    model.later(Duration::from_secs(23));
    assert_eq!(model.shown().notices, std::slice::from_ref(&counting));
    model.later(Duration::from_secs(10));
    assert!(
        model.shown().notices.is_empty(),
        "{:?}",
        model.shown().notices
    );
    assert_eq!(
        model.shown().last_switches.len(),
        1,
        "the switch is still kept"
    );

    switch(&mut model, &mut machine, "claude/work");
    assert_eq!(ids(&model.shown()), ["switch/claude"]);
    model.send(Intent::DismissSwitch {
        provider: "claude".into(),
    });
    assert!(model.shown().notices.is_empty());
}

/// With two tools each switch is said on its own, so it names its tool, and so does its
/// countdown: one beside a Codex notice would otherwise read as contradicting it.
#[test]
fn a_switch_names_its_tool_only_beside_another() {
    let (mut model, mut machine) = reading(vec![
        account(Some("work")).signed_in().build(),
        account(Some("job")).of("codex").signed_in().build(),
    ]);
    machine.switched = switched("claude", "personal", "work", Vec::new());
    switch(&mut model, &mut machine, "claude/work");
    let notice = model.shown().notices.remove(0);
    assert_eq!(notice.title, "Switched Claude Code to work");
    assert_eq!(
        notice.until_label.as_deref(),
        Some("Claude Code sessions already open follow in")
    );
}

/// While a file sits behind the keychain, Claude Code sessions already running take a switch
/// at their login's next renewal, which is theirs to know and no moment to count down to. The
/// switch's notice counts down to nothing, and the warning every read carries while the file
/// is there says when they follow, once, in a notice of its own. The notification of a
/// switch made by itself says it in place of a countdown.
#[test]
fn a_switch_that_sessions_take_at_renewal_counts_down_nothing() {
    let path = "/Users/x/.claude/.credentials.json";
    let left = warning(
        "fallback_login",
        &format!("{path} is there with no Claude Code login in it."),
    );
    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(warned(
        vec![
            account(Some("work")).signed_in().build(),
            account(Some("personal")).build(),
        ],
        vec![left.clone()],
    )));
    model.refresh(&mut machine);
    machine.switched = Ok(crate::Switched {
        outcome: crate::Switch::Switched {
            provider: "claude".into(),
            from: "personal".into(),
            to: "work".into(),
            adoption: crate::Adoption::AtRenewal { file: path.into() },
        },
        warnings: vec![left.clone()],
    });
    switch(&mut model, &mut machine, "claude/work");

    let shown = model.shown();
    assert_eq!(shown.last_switches[0].follows_at, None);
    assert_eq!(shown.last_switches[0].restart, None);
    let [left_behind] = shown.notices.as_slice() else {
        panic!("no bare switch notice beside it: {:?}", shown.notices);
    };
    assert_eq!(left_behind.severity, Severity::Warning);
    assert_eq!(
        [model.on_a_mac(), model.on_windows()].map(|shown| shown.notices[0].title.clone()),
        [
            "A login file is left behind the keychain",
            "A login file is left behind Credential Manager"
        ]
    );
    assert_eq!(left_behind.lines, [left.message]);
    assert_eq!(left_behind.until, None);
    assert_eq!(
        crate::present::auto_switched_notice(
            "personal",
            "work",
            "96% of its 5-hour limit",
            &crate::Adoption::AtRenewal { file: path.into() },
            NOON,
        )
        .body,
        format!(
            "personal has used 96% of its 5-hour limit. Sessions already running keep the \
             account they are on until their login is next renewed, or until they are started \
             again: {path} is there."
        )
    );
}

/// A running `codex` never picks a switch up, so a countdown would promise what does not
/// happen. The notice is a warning that says to start its sessions again, or, once the core
/// has counted them, the core's own warning, which says the same with the count.
#[test]
fn a_switch_that_needs_a_restart_is_a_warning_without_a_countdown() {
    let (mut model, mut machine) = reading(vec![
        account(Some("work")).of("codex").signed_in().build(),
        account(Some("personal")).of("codex").build(),
    ]);
    machine.switched = switched("codex", "codex/personal", "codex/work", Vec::new());
    switch(&mut model, &mut machine, "codex/work");
    assert_eq!(
        model.shown().notices,
        [notice(
            "switch/codex",
            Severity::Warning,
            "Switched to work",
            &[
                "Any codex session started before this switch keeps using personal until it \
               is quit and started again."
            ],
            vec![dismiss(Intent::DismissSwitch {
                provider: "codex".into()
            })],
        )]
    );

    let counted = warning(
        "sessions_still_running",
        "2 `codex` sessions started before this switch are still running.",
    );
    machine.switched = switched(
        "codex",
        "codex/personal",
        "codex/work",
        vec![counted.clone()],
    );
    switch(&mut model, &mut machine, "codex/work");
    let notices = model.shown().notices;
    let severities: Vec<Severity> = notices.iter().map(|n| n.severity).collect();
    assert_eq!(severities, [Severity::Warning]);
    assert_eq!(notices[0].lines, [counted.message]);
    assert_eq!(notices[0].until, None);
}

/// When the core could not count any sessions, the app's sentence is all there is, and it is
/// said of any session rather than of ones that may not exist.
#[test]
fn a_restart_is_explained_when_no_sessions_were_counted() {
    let (mut model, mut machine) =
        reading(vec![account(Some("work")).of("codex").signed_in().build()]);
    machine.switched = switched("codex", "codex/personal", "codex/work", Vec::new());
    switch(&mut model, &mut machine, "codex/work");
    assert_eq!(
        model.shown().notices[0].lines,
        [
            "Any codex session started before this switch keeps using personal until it is quit \
          and started again."
        ]
    );
}

/// A warning the read after a switch also carries is shown once, not twice: the switch's
/// notice leaves it to the read.
///
/// So the read has to say it: a warning both carry is said once, and not dropped from both.
#[test]
fn a_switch_warning_the_read_repeats_is_said_once() {
    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(warned(
        vec![account(Some("b")).signed_in().build()],
        vec![overridden()],
    )));
    machine.switched = switched("claude", "a", "b", vec![overridden()]);
    switch(&mut model, &mut machine, "claude/b");
    let shown = model.shown();
    assert_eq!(shown.warnings, [overridden()]);
    let switched_notice = shown
        .notices
        .iter()
        .find(|notice| notice.id == "switch/claude")
        .expect("the switch's notice");
    assert!(switched_notice.lines.is_empty(), "{switched_notice:?}");
    let said = shown
        .notices
        .iter()
        .flat_map(|notice| &notice.lines)
        .filter(|line| **line == overridden().message)
        .count();
    assert_eq!(said, 1);
}

/// A warning a switch carried is said with the switch, where it belongs, and nowhere else.
#[test]
fn a_warning_the_last_switch_carried_is_said_with_it_and_nowhere_else() {
    let (mut model, mut machine) = reading(vec![account(Some("work")).signed_in().build()]);
    machine.switched = switched("claude", "personal", "work", vec![overridden()]);
    switch(&mut model, &mut machine, "claude/work");
    let shown = model.shown();
    assert_eq!(ids(&shown), ["switch/claude"]);
    assert_eq!(shown.notices[0].severity, Severity::Warning);
    assert_eq!(shown.notices[0].lines, [overridden().message]);
}

/// Signing in to the account in use puts its new login in use at once, and sessions already
/// running keep the old one, exactly as a switch leaves them on the old account. So it is
/// said the way a switch is: the account has a new login, and what that means for them.
#[test]
fn a_sign_in_that_put_a_new_login_in_use_says_the_account_has_one() {
    let (mut model, mut machine) = reading(vec![account(Some("work")).signed_in().build()]);
    machine.enrolling = enrolled_as(EnrolledAs::InUse { again: true }, Vec::new());
    signs_in(&mut model, &mut machine, "claude", "work", &[]);
    assert_eq!(
        model.shown().notices,
        [notice(
            "switch/claude",
            Severity::Info,
            "work has a new login",
            &["Signed in to work again. Its new login is the one in use now."],
            vec![dismiss(Intent::DismissSwitch {
                provider: "claude".into()
            })],
        )]
    );

    let old_login = warning(
        "sessions_keep_old_login",
        "2 `codex` sessions are still using `codex/work`'s old login.",
    );
    let (mut model, mut machine) =
        reading(vec![account(Some("work")).of("codex").signed_in().build()]);
    machine.enrolling = enrolled_as(EnrolledAs::InUse { again: false }, vec![old_login.clone()]);
    signs_in(&mut model, &mut machine, "codex", "work", &[]);
    assert_eq!(
        model.shown().notices,
        [notice(
            "switch/codex",
            Severity::Warning,
            "work has a new login",
            &[
                "Enrolled work, the account signed in now. Its new login is the one in use.",
                &old_login.message,
            ],
            vec![dismiss(Intent::DismissSwitch {
                provider: "codex".into()
            })],
        )]
    );
}

/// Two tools can each have a `work`, so once both are shown a notice about one account says
/// which tool it is for, as a switch's notice and advice both do.
#[test]
fn a_sign_in_notice_names_its_tool_beside_another() {
    let (mut model, mut machine) = reading(vec![
        account(Some("work")).signed_in().build(),
        account(Some("work")).of("codex").signed_in().build(),
    ]);
    machine.enrolling = enrolled_as(EnrolledAs::InUse { again: true }, Vec::new());
    signs_in(&mut model, &mut machine, "codex", "work", &[]);
    let notice = model.shown().notices.remove(0);
    assert_eq!(notice.id, "switch/codex");
    assert_eq!(notice.title, "work in Codex has a new login");
}

/// Every warning a read carries is a notice of its own, headed from its code so the menu can
/// name it in a line, with the core's message under it saying what to do. Two warnings of
/// one code are two notices, each with an identity of its own.
#[test]
fn each_warning_a_read_carries_is_a_notice_headed_from_its_code() {
    let second = warning("auth_overridden", "CLAUDE_CODE_OAUTH_TOKEN is set");
    let novel = warning("something_new", "Something Pitboard does not know yet");
    let warnings = vec![overridden(), novel, second];
    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(warned(
        vec![account(Some("work")).signed_in().build()],
        warnings.clone(),
    )));
    model.refresh(&mut machine);

    let notices = model.shown().notices;
    let titles: Vec<&str> = notices.iter().map(|n| n.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "An environment variable overrides the login",
            "Pitboard has a warning",
            "An environment variable overrides the login",
        ]
    );
    for (notice, warning) in notices.iter().zip(&warnings) {
        assert_eq!(notice.lines, std::slice::from_ref(&warning.message));
        assert_eq!(notice.severity, Severity::Warning);
        assert!(notice.actions.is_empty());
        assert!(
            notice.id.starts_with(&format!("warning/{}/", warning.code)),
            "{}",
            notice.id
        );
    }
    let distinct: std::collections::BTreeSet<&str> =
        notices.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(distinct.len(), warnings.len());
}

/// Two warnings alike are two notices still, each with an identity of its own, which the
/// Swift's `hashValue` gave both alike.
#[test]
fn two_warnings_alike_are_two_notices() {
    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(warned(
        vec![account(Some("work")).signed_in().build()],
        vec![overridden(), overridden()],
    )));
    model.refresh(&mut machine);
    let shown = model.shown();
    assert_eq!(shown.notices.len(), 2);
    assert_ne!(shown.notices[0].id, shown.notices[1].id);
}

/// A warning about one account keeps its notice by that account while its words change, and
/// one account's id beginning another's does not make the second a copy of the first.
#[test]
fn a_warning_about_an_account_is_its_notice_by_that_account() {
    let replaced = |account: &str, now: &str| Warning {
        account: Some(account.into()),
        held: None,
        ..warning(
            "login_replaced",
            &format!("Claude Code now has `{now}`'s login stored"),
        )
    };
    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(warned(
        vec![account(Some("work")).signed_in().build()],
        vec![
            replaced("claude:here2", "work"),
            replaced("claude:here", "work"),
        ],
    )));
    model.refresh(&mut machine);
    let expected = [
        "warning/login_replaced/claude:here2",
        "warning/login_replaced/claude:here",
    ];
    assert_eq!(ids(&model.shown()), expected);

    machine.answer = Ok(warned(
        vec![account(Some("work")).signed_in().build()],
        vec![
            replaced("claude:here2", "spare"),
            replaced("claude:here", "spare"),
        ],
    ));
    model.send(Intent::Refresh { asked: true });
    model.run(&mut machine);
    assert_eq!(ids(&model.shown()), expected);
}

/// Giving up on an interrupted switch deletes nothing, and says so, with how many logins were
/// kept, until somebody puts it away.
#[test]
fn giving_up_on_a_switch_says_what_was_kept_until_put_away() {
    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(warned(
        vec![account(Some("work")).signed_in().build()],
        vec![interrupted()],
    )));
    model.refresh(&mut machine);
    assert_eq!(ids(&model.shown()), ["stuck"]);

    machine.answer = Ok(status(vec![account(Some("work")).signed_in().build()]));
    machine.abandoned = Ok(GaveUp {
        switch: Some(Abandoned {
            from: "personal".into(),
            to: "work".into(),
            logins_kept: 2,
        }),
        ..GaveUp::default()
    });
    model.send(Intent::AbandonStuckSwitch);
    model.run(&mut machine);
    assert_eq!(
        model.shown().notices,
        [notice(
            "abandoned",
            Severity::Info,
            "Gave up on the interrupted switch",
            &[
                "The switch from personal to work was given up. 2 logins kept, and nothing \
               was deleted."
            ],
            vec![dismiss(Intent::DismissAbandoned)],
        )]
    );

    machine.abandoned = Ok(GaveUp {
        switch: Some(Abandoned {
            from: "personal".into(),
            to: "work".into(),
            logins_kept: 1,
        }),
        ..GaveUp::default()
    });
    model.send(Intent::AbandonStuckSwitch);
    model.run(&mut machine);
    assert_eq!(
        model.shown().notices[0].lines,
        [
            "The switch from personal to work was given up. 1 login kept, and nothing was \
          deleted."
        ]
    );

    model.send(Intent::DismissAbandoned);
    assert!(model.shown().notices.is_empty());
}

/// The most pressing first: what stops Pitboard working, then an account that ran out, then
/// what each tool's last switch said, then warnings, then what is only worth knowing.
#[test]
fn notices_come_the_most_pressing_first() {
    let accounts = || {
        vec![
            account(Some("work"))
                .signed_in()
                .limits(vec![window("session", 10.0)])
                .build(),
            account(Some("spare"))
                .limits(vec![window("session", 20.0)])
                .build(),
            account(Some("job")).of("codex").signed_in().build(),
        ]
    };
    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(warned(accounts(), vec![interrupted()])));
    model.refresh(&mut machine);

    machine.answer = Ok(status(accounts()));
    machine.abandoned = Ok(GaveUp {
        switch: Some(Abandoned {
            from: "personal".into(),
            to: "work".into(),
            logins_kept: 2,
        }),
        ..GaveUp::default()
    });
    model.send(Intent::AbandonStuckSwitch);
    model.run(&mut machine);

    machine.switched = switched("codex", "codex/side", "codex/job", Vec::new());
    switch(&mut model, &mut machine, "codex/job");

    machine.answer = Ok(status(vec![
        account(Some("work"))
            .signed_in()
            .limits(vec![window("session", 100.0)])
            .build(),
        account(Some("spare"))
            .limits(vec![window("session", 20.0)])
            .build(),
        account(Some("job")).of("codex").signed_in().build(),
    ]));
    model.refresh(&mut machine);

    machine.answer = Err(refusal(
        "unreachable",
        "Anthropic could not be reached",
        vec![overridden()],
    ));
    model.refresh(&mut machine);

    let notices = model.shown().notices;
    let kinds: Vec<&str> = notices
        .iter()
        .map(|n| n.id.split('/').next().unwrap_or_default())
        .collect();
    assert_eq!(kinds, ["read", "advice", "switch", "warning", "abandoned"]);
    let severities: Vec<Severity> = notices.iter().map(|n| n.severity).collect();
    assert_eq!(
        severities,
        [
            Severity::Error,
            Severity::Warning,
            Severity::Warning,
            Severity::Warning,
            Severity::Info
        ]
    );
}

/// Advice offers an account of the same tool with room, as a button that switches to it by
/// its label with its tool. It names the tool only once two are shown, so a machine
/// with one tool reads as it always did.
#[test]
fn advice_offers_the_account_with_room_and_names_its_tool_only_beside_another() {
    let spent = || {
        vec![
            account(Some("work"))
                .signed_in()
                .limits(vec![window("session", 100.0)])
                .build(),
            account(Some("spare"))
                .limits(vec![window("session", 20.0)])
                .build(),
        ]
    };
    let (alone, _) = reading(spent());
    assert_eq!(
        alone.shown().notices,
        [notice(
            "advice/claude/work/session/",
            Severity::Warning,
            "work has no 5-hour limit left",
            &["spare has 80% of its own left."],
            vec![NoticeAction {
                title: "Switch to spare".into(),
                intent: Intent::SwitchTo {
                    qualified: "claude/spare".into()
                },
                dismisses: false,
                switches: true,
                enabled: true,
                confirm: None,
            }],
        )]
    );

    let mut beside = spent();
    beside.push(account(Some("job")).of("codex").signed_in().build());
    let (beside, _) = reading(beside);
    let titles: Vec<String> = beside
        .shown()
        .notices
        .into_iter()
        .map(|n| n.title)
        .collect();
    assert_eq!(titles, ["Claude Code: work has no 5-hour limit left"]);
}

/// An account offered on a plan without the limit that ran out is said to have no such
/// limit, not all of it left.
#[test]
fn an_account_offered_without_the_limit_that_ran_out_is_said_to_have_no_such_limit() {
    let (model, _) = reading(vec![
        account(Some("work"))
            .signed_in()
            .limits(vec![window("session", 20.0), window("weekly_all", 100.0)])
            .build(),
        account(Some("seat"))
            .limits(vec![window("session", 10.0)])
            .build(),
    ]);
    let notices = model.shown().notices;
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].title, "work has no weekly limit left");
    assert_eq!(notices[0].lines, ["seat has no such limit."]);
}

/// A switch of one tool leaves another tool's accounts as they were. Advice about a Claude
/// Code account still out, beside another that still has room, is as true after a Codex
/// switch as before it, and it is never told again, so putting it away loses it.
#[test]
fn advice_about_one_tool_outlives_a_switch_of_another() {
    let spent = || {
        account(Some("work"))
            .signed_in()
            .limits(vec![window("session", 100.0)])
            .build()
    };
    let spare = || {
        account(Some("spare"))
            .limits(vec![window("session", 20.0)])
            .build()
    };
    let (mut model, mut machine) = reading(vec![
        spent(),
        spare(),
        account(Some("side")).of("codex").signed_in().build(),
        account(Some("job")).of("codex").build(),
    ]);
    assert_eq!(ids(&model.shown()), ["advice/claude/work/session/"]);

    machine.answer = Ok(status(vec![
        spent(),
        spare(),
        account(Some("side")).of("codex").build(),
        account(Some("job")).of("codex").signed_in().build(),
    ]));
    machine.switched = switched("codex", "codex/side", "codex/job", Vec::new());
    switch(&mut model, &mut machine, "codex/job");
    assert_eq!(
        ids(&model.shown()),
        ["advice/claude/work/session/", "switch/codex"]
    );
}

/// The menu turns advice into an item that switches, held back while a switch is under way,
/// and only advice: a notice offers an account when one of its actions is a switch to it.
#[test]
fn a_notice_offers_an_account_only_when_it_can_switch_to_one() {
    let (mut model, _) = reading(vec![
        account(Some("work"))
            .signed_in()
            .limits(vec![window("session", 100.0)])
            .build(),
        account(Some("spare"))
            .limits(vec![window("session", 20.0)])
            .build(),
    ]);
    let menu = model.shown().menu_notices;
    assert_eq!(
        menu.switches,
        [MenuEntry {
            title: "Switch to spare".into(),
            subtitle: Some("work has no 5-hour limit left".into()),
            help: None,
            severity: None,
            intent: Some(Intent::SwitchTo {
                qualified: "claude/spare".into()
            }),
            link: None,
            enabled: true,
        }]
    );
    assert_eq!(
        menu.others, None,
        "advice is not something else to know about"
    );

    model.send(Intent::SwitchTo {
        qualified: "claude/spare".into(),
    });
    let shown = model.shown();
    assert!(!shown.menu_notices.switches[0].enabled);
    assert!(!shown.notices[0].actions[0].enabled);
}

/// Claude Code's config naming another account than the one in use is put right by writing
/// the account in use there, so its notice offers that. It moves no account, so the menu
/// does not offer it as a switch. An account in use that is not enrolled has no name to
/// write, and is offered nothing.
#[test]
fn the_config_names_another_notice_offers_to_update_it() {
    let names_another = warning(
        "config_names_another",
        "Claude Code's config names `spare`, and the login Claude Code has stored is `work`'s.",
    );
    let accounts = |in_use: Account| {
        vec![
            in_use,
            account(Some("spare")).build(),
            account(Some("main")).of("codex").signed_in().build(),
        ]
    };
    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(warned(
        accounts(account(Some("work")).signed_in().build()),
        vec![names_another.clone()],
    )));
    model.refresh(&mut machine);

    let shown = model.shown();
    assert_eq!(
        shown.notices[0].actions,
        [NoticeAction {
            title: titled("Update Claude Code’s Config", "Update Claude Code’s config"),
            intent: Intent::UpdateConfig {
                qualified: "claude/work".into()
            },
            dismisses: false,
            switches: false,
            enabled: true,
            confirm: None,
        }]
    );
    assert!(shown.menu_notices.switches.is_empty());

    machine.answer = Ok(warned(
        accounts(account(None).signed_in().uuid("u").build()),
        vec![names_another],
    ));
    model.refresh(&mut machine);
    assert!(model.shown().notices[0].actions.is_empty());
}

/// What can be done about a notice is a button under it, and putting it away is the icon at
/// its end: never both, and never neither.
#[test]
fn a_notices_action_is_either_a_button_or_what_puts_it_away() {
    let accounts = |percent: f64| {
        vec![
            account(Some("work"))
                .of("codex")
                .signed_in()
                .limits(vec![window("five_hour", percent)])
                .build(),
            account(Some("spare"))
                .of("codex")
                .limits(vec![window("five_hour", 20.0)])
                .build(),
        ]
    };
    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(warned(accounts(10.0), vec![interrupted()])));
    model.refresh(&mut machine);
    machine.switched = switched("codex", "codex/side", "codex/work", Vec::new());
    switch(&mut model, &mut machine, "codex/work");
    machine.abandoned = Ok(GaveUp {
        switch: Some(Abandoned {
            from: "personal".into(),
            to: "work".into(),
            logins_kept: 2,
        }),
        ..GaveUp::default()
    });
    model.send(Intent::AbandonStuckSwitch);
    model.run(&mut machine);
    machine.answer = Ok(warned(accounts(100.0), vec![interrupted()]));
    model.refresh(&mut machine);

    let actions: Vec<NoticeAction> = model
        .shown()
        .notices
        .into_iter()
        .flat_map(|notice| notice.actions)
        .collect();
    let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
    let give_up = titled("Give Up…", "Give up…");
    assert_eq!(
        titles,
        [give_up.as_str(), "Switch to spare", "Dismiss", "Dismiss"]
    );
    let dismisses: Vec<bool> = actions.iter().map(|a| a.dismisses).collect();
    assert_eq!(dismisses, [false, false, true, true]);
    let switches: Vec<bool> = actions.iter().map(|a| a.switches).collect();
    assert_eq!(switches, [false, true, false, false]);
}

/// The menu has no room for paragraphs: everything to know about that is more than a note is
/// one item that opens the window where it is said in full, named as the one notice it is
/// or counting them, and a machine without a tool is told how to install Claude Code.
///
/// MenuBarContent.swift's attention section, which the Swift tested nowhere.
#[test]
fn the_menu_says_what_needs_attention_in_one_item() {
    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(warned(
        vec![account(Some("work")).signed_in().build()],
        vec![overridden()],
    )));
    model.refresh(&mut machine);
    let menu = model.shown().menu_notices;
    assert_eq!(menu.install, None);
    assert!(menu.switches.is_empty());
    assert_eq!(
        menu.others,
        Some(MenuEntry {
            title: "An environment variable overrides the login".into(),
            subtitle: Some("Show in Pitboard".into()),
            help: Some("ANTHROPIC_API_KEY is set".into()),
            severity: Some(Severity::Warning),
            intent: Some(Intent::ShowWindow {
                pane: Some(Pane::Accounts)
            }),
            link: None,
            enabled: true,
        })
    );

    machine.answer = Ok(warned(
        vec![account(Some("work")).signed_in().build()],
        vec![interrupted(), overridden()],
    ));
    model.refresh(&mut machine);
    let others = model.shown().menu_notices.others.expect("one item");
    assert_eq!(others.title, "2 things to look at");
    assert_eq!(
        others.subtitle.as_deref(),
        Some("An interrupted switch is waiting")
    );
    assert_eq!(others.help, None);
    assert_eq!(others.severity, Some(Severity::Error));

    // A note is not pressing enough for the menu.
    machine.answer = Ok(status(vec![account(Some("work")).signed_in().build()]));
    machine.abandoned = Ok(GaveUp {
        switch: Some(Abandoned {
            from: "personal".into(),
            to: "work".into(),
            logins_kept: 1,
        }),
        ..GaveUp::default()
    });
    model.send(Intent::AbandonStuckSwitch);
    model.run(&mut machine);
    assert_eq!(ids(&model.shown()), ["abandoned"]);
    assert_eq!(model.shown().menu_notices.others, None);

    let mut bare = at_noon();
    let mut nothing = Stub::reading(Ok(status(Vec::new())));
    nothing.found = Vec::new();
    bare.refresh(&mut nothing);
    let install = bare.shown().menu_notices.install.expect("how to install");
    assert_eq!(install.title, "Claude Code isn’t installed");
    assert_eq!(install.subtitle.as_deref(), Some("Learn how to install it"));
    assert_eq!(
        install.link.as_deref(),
        Some("https://docs.claude.com/en/docs/claude-code/setup")
    );
}

/// When the numbers were read is said as a time rather than an age, since a menu can stay
/// open, and "just now" would still say so ten minutes later; while a read runs, that it
/// does; before one has answered, that nothing is read yet or that the last numbers measured
/// are shown. The window's subtitle says nothing before the first read.
///
/// MenuBarContent.swift's and AccountsPane.swift's `updated`, which the Swift tested nowhere.
#[test]
fn when_the_numbers_were_read_is_said_as_a_time() {
    let mut model = at_noon();
    let shown = model.shown();
    assert_eq!(
        (shown.updated_menu.as_str(), shown.updated_window.as_str()),
        ("Not read yet", "")
    );
    let mut machine = Stub::reading(Ok(status(vec![account(Some("work")).signed_in().build()])));
    model.send(Intent::Refresh { asked: false });
    model.run_but(&mut machine, super::testing::machine::any_read);
    let shown = model.shown();
    assert_eq!(
        (shown.updated_menu.as_str(), shown.updated_window.as_str()),
        ("Reading…", "Reading…")
    );
    model.run(&mut machine);
    let shown = model.shown();
    assert_eq!(
        (shown.updated_menu.as_str(), shown.updated_window.as_str()),
        ("Updated 12:00", "Updated 12:00")
    );

    let mut failed = at_noon();
    failed.refresh(&mut Stub::reading(Err(refusal(
        "unreachable",
        "Anthropic could not be reached",
        Vec::new(),
    ))));
    let shown = failed.shown();
    assert_eq!(shown.updated_menu, "Showing the last numbers measured");
    assert_eq!(shown.updated_window, "");
}

/// Where the menu has no accounts to list it says why, and the window says it in full: still
/// reading, nothing to show after a read that could not say, or nobody signed in, with how to
/// add an account. A machine without Claude Code says that alone.
///
/// MenuBarContent.swift's accounts section and AccountsPane.swift's content, which the
/// Swift tested nowhere.
#[test]
fn where_there_are_no_accounts_the_menu_and_the_window_say_why() {
    let model = at_noon();
    let shown = model.shown();
    assert_eq!(
        shown.menu_accounts_note.as_deref(),
        Some("Reading accounts…")
    );
    assert_eq!(
        shown.accounts_shown,
        AccountsShown::Reading {
            title: "Reading accounts…".into()
        }
    );

    let mut unknown = at_noon();
    let mut machine = Stub::reading(Err(refusal("unreachable", "x", Vec::new())));
    machine.offline = Err(refusal("state_unreadable", "y", Vec::new()));
    unknown.refresh(&mut machine);
    assert_eq!(
        unknown.shown().menu_accounts_note.as_deref(),
        Some("No accounts to show")
    );

    let (empty, _) = reading(Vec::new());
    let shown = empty.shown();
    assert_eq!(shown.menu_accounts_note.as_deref(), Some("No accounts yet"));
    assert_eq!(
        shown.accounts_shown,
        AccountsShown::NoAccounts {
            title: titled("No Accounts", "No accounts"),
            detail: "Sign in once here and Pitboard parks that login, so signing in to another \
                     account doesn’t cost you the first."
                .into(),
            add: Choice {
                title: titled("Add Account…", "Add account…"),
                intent: present_sheet(Sheet::Add { provider: None }),
                enabled: true,
            },
        }
    );

    let mut bare = at_noon();
    let mut nothing = Stub::reading(Ok(status(Vec::new())));
    nothing.found = Vec::new();
    bare.refresh(&mut nothing);
    let shown = bare.on_a_mac();
    assert_eq!(shown.menu_accounts_note, None);
    assert_eq!(
        shown.accounts_shown,
        AccountsShown::NoTool {
            title: "Claude Code Isn’t Installed".into(),
            detail: "Pitboard switches the logins of Claude Code, Codex and Claude Desktop, so \
                     there is nothing for it to do until one of them is installed and signed in \
                     once."
                .into(),
            link_title: "How to Install Claude Code".into(),
            link: "https://docs.claude.com/en/docs/claude-code/setup".into(),
        }
    );

    let (listed, _) = reading(vec![account(Some("work")).signed_in().build()]);
    assert_eq!(listed.shown().menu_accounts_note, None);
    assert_eq!(listed.shown().accounts_shown, AccountsShown::List);
}

// What a machine that is not set up yet is told to do.

fn footing(accounts: Vec<Account>) -> Footing {
    reading(accounts).0.shown().footing
}

/// A new install of the app. Before the first read there is nothing true to say, and a setup
/// step shown to somebody who finished it years ago is worse than silence.
#[test]
fn nothing_is_asked_of_anyone_before_the_first_read() {
    let shown = Hand::new().shown();
    assert_eq!(shown.footing, Footing::Ready);
    assert_eq!(shown.setup, None);
}

/// The one state Pitboard cannot do anything about. It has to say so rather than show an
/// empty panel, which reads as an app that does not work. The core's read succeeds with
/// nothing to list on such a machine, so what says it is that no tool was found.
#[test]
fn a_machine_without_claude_code_is_told_that_first() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.found = Vec::new();
    assert_eq!(
        model.shown().footing,
        Footing::Ready,
        "nothing is said before the first read"
    );
    model.refresh(&mut machine);
    assert_eq!(model.shown().footing, Footing::NoClaudeCode);
}

/// A tool that was found, or a login or an account of any tool, means a tool is here, and
/// the machine is one to set up rather than one to install on.
#[test]
fn a_tool_found_or_signed_in_is_not_a_machine_without_one() {
    let mut found = Hand::new();
    let mut codex_only = Stub::reading(Ok(status(Vec::new())));
    codex_only.found = vec![codex()];
    found.refresh(&mut codex_only);
    assert_eq!(found.shown().footing, Footing::CodexUnused);

    let mut unfound = Hand::new();
    let mut signed_in = Stub::reading(Ok(status(vec![
        account(Some("job")).of("codex").signed_in().build(),
    ])));
    signed_in.found = Vec::new();
    unfound.refresh(&mut signed_in);
    assert_ne!(unfound.shown().footing, Footing::NoClaudeCode);
}

/// Once a tool is found the machine is one to sign in on. The asking is reading.rs's.
#[test]
fn what_is_installed_is_asked_again_while_nothing_is_found() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.found = Vec::new();
    model.refresh(&mut machine);
    assert_eq!(model.shown().footing, Footing::NoClaudeCode);
    machine.found = vec![claude_code()];
    model.refresh(&mut machine);
    assert_eq!(model.shown().footing, Footing::NoOneSignedIn);
}

#[test]
fn an_empty_machine_is_asked_to_sign_in_once() {
    assert_eq!(footing(Vec::new()), Footing::NoOneSignedIn);
}

/// A login with no name cannot be parked, so this is the step between signing in and
/// Pitboard being able to do anything at all. Until it has one it is called by its email
/// address, which is how the person knows it.
#[test]
fn an_account_signed_in_without_a_name_is_asked_for_one() {
    let (model, _) = reading(vec![account(None).signed_in().uuid("a").build()]);
    let shown = model.shown();
    assert_eq!(
        shown.footing,
        Footing::Unnamed {
            provider: "claude".into(),
            email: "a@example.com".into()
        }
    );
    assert_eq!(shown.sections[0].accounts[0].spoken_name, "a@example.com");
    assert_eq!(
        shown.setup,
        Some(SetupStep {
            title: "Give this account a name".into(),
            detail: "a@example.com is signed in. Pitboard parks logins under a name you \
                     choose, and can’t park this one until it has one."
                .into(),
            actions: vec![Choice {
                title: "Name…".into(),
                intent: present_sheet(Sheet::Name {
                    provider: "claude".into(),
                    email: "a@example.com".into()
                }),
                enabled: true,
            }],
            link: None,
        })
    );
}

/// One enrolled account is told there is nothing to switch to, by the step above the accounts,
/// which can be declined for its tool.
#[test]
fn one_enrolled_account_is_told_there_is_nothing_to_switch_to() {
    let (model, _) = reading(vec![
        account(Some("work"))
            .signed_in()
            .limits(vec![window("session", 10.0)])
            .build(),
    ]);
    let shown = model.shown();
    assert_eq!(
        shown.footing,
        Footing::OnlyOne {
            provider: "claude".into(),
            label: "work".into()
        }
    );
    assert_eq!(
        shown.setup,
        Some(SetupStep {
            title: "Add a second account".into(),
            detail: "work is the only account Pitboard knows, so there’s nothing to switch to. \
                     Adding another signs in to it and parks its login beside this one."
                .into(),
            actions: vec![
                Choice {
                    title: titled("Add Account…", "Add account…"),
                    intent: present_sheet(Sheet::Add {
                        provider: Some("claude".into())
                    }),
                    enabled: true,
                },
                Choice {
                    title: titled("Not Now", "Not now"),
                    intent: Intent::DeclineSecondAccount {
                        provider: "claude".into()
                    },
                    enabled: true,
                },
            ],
            link: None,
        })
    );
}

/// Two Claude Code accounts are asked what happens at a limit, until somebody chooses in the
/// settings, where the sentence on the plans' terms sits beside switching by itself.
#[test]
fn two_accounts_are_asked_what_happens_at_a_limit() {
    let (model, _) = reading(vec![
        account(Some("work")).signed_in().build(),
        account(Some("personal")).build(),
    ]);
    assert_eq!(model.shown().footing, Footing::AtLimitUnchosen);
    let [mac, windows] = [model.on_a_mac(), model.on_windows()].map(|shown| shown.setup);
    assert_eq!(
        mac,
        Some(SetupStep {
            title: "Switch before a limit runs out".into(),
            detail: "Pitboard notifies you once a limit of the account in use reaches 95%. It \
                     can switch Claude Code to another of your accounts instead."
                .into(),
            actions: vec![
                Choice {
                    title: "Open Settings…".into(),
                    intent: Intent::ShowLimits,
                    enabled: true,
                },
                Choice {
                    title: "Not Now".into(),
                    intent: Intent::DeclineTip { tip: Tip::AtLimit },
                    enabled: true,
                },
            ],
            link: None,
        })
    );
    let windows = windows.expect("a step");
    assert_eq!(
        windows
            .actions
            .iter()
            .map(|choice| choice.title.as_str())
            .collect::<Vec<_>>(),
        ["Open settings", "Not now"]
    );

    let (mut chose, _) = reading(vec![
        account(Some("work")).signed_in().build(),
        account(Some("personal")).build(),
    ]);
    chose.state.preferences.choose(super::AtLimit::Switch);
    assert_eq!(chose.shown().footing, Footing::Ready);
    let codex_pair = footing(vec![
        account(Some("job")).of("codex").signed_in().build(),
        account(Some("side")).of("codex").build(),
    ]);
    assert_eq!(
        codex_pair,
        Footing::Ready,
        "only Claude Code switches by itself"
    );
}

/// The model at noon, having read `accounts` on a machine where Claude Code and Codex were
/// found.
fn reading_with_codex(accounts: Vec<Account>) -> (Hand, Stub) {
    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(status(accounts)));
    machine.found = vec![claude_code(), codex()];
    model.refresh(&mut machine);
    (model, machine)
}

/// Codex found with nothing of it enrolled is a tool Pitboard reads nothing of, so it cannot
/// say whether Codex is signed in. It offers to use Pitboard with it after Claude Code's own
/// steps, and in place of No Accounts where Claude Code is signed in to nothing, which said
/// nobody was signed in while Codex was.
#[test]
fn codex_found_with_nothing_of_it_enrolled_is_offered() {
    let (model, _) = reading_with_codex(vec![
        account(Some("work")).signed_in().build(),
        account(Some("personal")).build(),
    ]);
    let shown = model.on_a_mac();
    assert_eq!(shown.footing, Footing::CodexUnused);
    assert_eq!(
        shown.setup,
        Some(SetupStep {
            title: "Use Pitboard with Codex".into(),
            detail: "Codex is installed. Pitboard reads nothing of it until you add the account \
                     it’s signed in to."
                .into(),
            actions: vec![
                Choice {
                    title: "Add Codex Account…".into(),
                    intent: Intent::NameSignedIn {
                        provider: "codex".into()
                    },
                    enabled: true,
                },
                Choice {
                    title: "Not Now".into(),
                    intent: Intent::DeclineTip { tip: Tip::UseCodex },
                    enabled: true,
                },
            ],
            link: None,
        })
    );

    let (alone, _) = {
        let mut model = at_noon();
        let mut machine = Stub::reading(Ok(status(Vec::new())));
        machine.found = vec![codex()];
        model.refresh(&mut machine);
        (model, machine)
    };
    assert_eq!(alone.shown().footing, Footing::CodexUnused);
    assert_eq!(alone.shown().accounts_shown, AccountsShown::List);
    let (both, _) = reading_with_codex(Vec::new());
    assert_eq!(both.shown().footing, Footing::CodexUnused);
    assert_eq!(both.shown().accounts_shown, AccountsShown::List);

    let (enrolled, _) = reading_with_codex(vec![
        account(Some("work")).signed_in().build(),
        account(Some("personal")).build(),
        account(Some("job")).of("codex").signed_in().build(),
        account(Some("side")).of("codex").build(),
    ]);
    assert_eq!(enrolled.shown().footing, Footing::AtLimitUnchosen);
}

/// Asked to use Pitboard with Codex, the model reads whose login Codex stores, and not before:
/// Name This Account for it, or Add Account set to Codex where nobody is signed in to it.
#[test]
fn using_codex_names_the_login_it_stores_read_only_once_asked() {
    let (mut model, _) = reading_with_codex(vec![account(Some("work")).signed_in().build()]);
    assert!(
        !model
            .asked
            .iter()
            .any(|job| matches!(job, Job::ReadSignedIn { .. })),
        "nothing read of Codex before the click"
    );
    let codex = || Intent::NameSignedIn {
        provider: "codex".into(),
    };
    assert_eq!(
        model.send(codex()),
        [Job::ReadSignedIn {
            provider: "codex".into()
        }]
    );
    model.give(Answer::SignedIn {
        provider: "codex".into(),
        email: Some("dana@acme.io".into()),
    });
    let shown = model.shown();
    assert_eq!(
        shown.sheet,
        Some(Sheet::Name {
            provider: "codex".into(),
            email: "dana@acme.io".into()
        })
    );
    assert_eq!(shown.sheet_text.expect("the sheet").name, "acme");

    model.send(Intent::CloseSheet);
    model.send(codex());
    model.give(Answer::SignedIn {
        provider: "codex".into(),
        email: None,
    });
    assert_eq!(
        model.shown().sheet,
        Some(Sheet::Add {
            provider: Some("codex".into())
        })
    );
}

/// The app opens at login only where somebody turned that on, and switches, notifies and
/// warms up only while it is open, so the step after a second account asks, in each system's
/// words. Windows puts a new icon behind the taskbar's arrow and lets nothing bring it out but
/// its own settings, so its step says so and links them.
#[test]
fn an_app_that_does_not_open_at_login_is_asked_to() {
    let (mut model, _) = reading(vec![
        account(Some("work")).signed_in().build(),
        account(Some("personal")).build(),
    ]);
    model.send(Intent::OpensAtLogin { on: false });
    assert_eq!(model.shown().footing, Footing::NotOpenedAtLogin);
    let not_now = |title: &str| Choice {
        title: title.into(),
        intent: Intent::DeclineTip {
            tip: Tip::OpenAtLogin,
        },
        enabled: true,
    };
    assert_eq!(
        model.on_a_mac().setup,
        Some(SetupStep {
            title: "Open Pitboard at login".into(),
            detail: "Pitboard watches your limits only while it’s open.".into(),
            actions: vec![
                Choice {
                    title: "Open at Login".into(),
                    intent: Intent::OpensAtLogin { on: true },
                    enabled: true,
                },
                not_now("Not Now"),
            ],
            link: None,
        })
    );
    assert_eq!(
        model.on_windows().setup,
        Some(SetupStep {
            title: "Start when you sign in to Windows".into(),
            detail: "Pitboard watches your limits only while it’s running. Its icon starts \
                     behind the arrow at the end of the taskbar, and Taskbar settings can keep \
                     it in view."
                .into(),
            actions: vec![
                Choice {
                    title: "Turn on".into(),
                    intent: Intent::OpensAtLogin { on: true },
                    enabled: true,
                },
                not_now("Not now"),
            ],
            link: Some(SetupLink {
                title: "Taskbar settings".into(),
                link: "ms-settings:taskbar".into(),
            }),
        })
    );
}

/// Mid-switch, and a login signed out from somewhere else, both leave accounts enrolled with
/// nobody signed in. Neither is a machine that needs setting up, and asking somebody to sign
/// in again there would have them sign in over an account Pitboard already holds.
#[test]
fn enrolled_accounts_with_nobody_signed_in_are_not_asked_to_start_over() {
    assert_eq!(footing(vec![account(Some("work")).build()]), Footing::Ready);
}

/// Somebody signed in to Codex is not somebody nobody is signed in to.
#[test]
fn a_codex_login_is_not_an_empty_machine() {
    assert_eq!(
        footing(vec![account(Some("work")).of("codex").signed_in().build()]),
        Footing::OnlyOne {
            provider: "codex".into(),
            label: "work".into()
        }
    );
}

/// A Claude Code account and a Codex account are two accounts and nothing to switch to: an
/// account is only ever switched to another of its own tool. The step names the tool, once
/// there are two.
#[test]
fn only_one_account_is_counted_per_tool() {
    let (model, _) = reading(vec![
        account(Some("work")).signed_in().build(),
        account(Some("spare")).build(),
        account(Some("job")).of("codex").signed_in().build(),
    ]);
    let shown = model.shown();
    assert_eq!(
        shown.footing,
        Footing::OnlyOne {
            provider: "codex".into(),
            label: "job".into()
        }
    );
    let step = shown.setup.expect("a step");
    assert_eq!(step.title, "Add a second Codex account");
    assert!(
        step.detail
            .starts_with("job is the only Codex account Pitboard knows"),
        "{}",
        step.detail
    );
}

/// A login Pitboard could not read, or cannot switch, has no account behind it. Offering to
/// name it would enrol something that can never be switched to.
#[test]
fn an_unplaced_login_is_neither_unenrolled_nor_offered_a_name() {
    for row in [unplaced("codex", false), unplaced("codex", true)] {
        let (model, _) = reading(vec![
            account(Some("work")).signed_in().build(),
            account(Some("spare")).build(),
            account(Some("job")).of("codex").build(),
            account(Some("side")).of("codex").build(),
            row,
        ]);
        let shown = model.shown();
        assert_eq!(shown.footing, Footing::AtLimitUnchosen, "nothing to name");
        assert!(
            rows(&model).iter().all(|row| !matches!(
                row.action.as_ref().map(|a| &a.intent),
                Some(Intent::PresentSheet {
                    sheet: Sheet::Name { .. }
                })
            )),
            "nothing to name"
        );
    }
}

/// The report of 16 October 2026, first run: Claude Code signed in to a login Anthropic would
/// not name. The window said "No accounts"; it shows the login and why.
#[test]
fn a_login_whose_account_cannot_be_told_is_shown_from_the_first_run() {
    let mut untold = unplaced("claude", false);
    untold.stale = Some("refused".into());
    untold.stale_explanation = Some(
        "Pitboard could not tell which account Claude Code's login is (Anthropic answered 400)"
            .into(),
    );
    let (model, _) = reading(vec![untold]);
    let shown = model.shown();
    assert_eq!(shown.footing, Footing::Ready);
    assert_eq!(shown.setup, None);
    let [row] = &rows(&model)[..] else {
        panic!("one row");
    };
    assert!(row.unplaced && row.action.is_none(), "{row:?}");
    assert_eq!(row.title, "Login Pitboard can’t identify");
    assert_eq!(
        row.summary,
        "Pitboard could not tell which account Claude Code's login is (Anthropic answered 400)"
    );
}

// The sheets.

/// The model with the sheet for a new account up, on a machine where `found` were found and
/// `accounts` are shown, as a Mac's app has it.
fn adding(found: Vec<crate::Tool>, accounts: Vec<Account>) -> Snapshot {
    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(status(accounts)));
    machine.found = found;
    model.refresh(&mut machine);
    model.send(present_sheet(Sheet::Add { provider: None }));
    model.run(&mut machine);
    model.on_a_mac()
}

// The codes offered, and which of them Pitboard did not find.
fn tools_and_missing(sheet: &crate::present::SheetText) -> (Vec<&str>, Vec<&str>) {
    let codes = sheet.tools.iter().map(|t| t.code.as_str()).collect();
    let missing = sheet
        .tools
        .iter()
        .filter(|t| t.missing.is_some())
        .map(|t| t.code.as_str())
        .collect();
    (codes, missing)
}

// Offered and refused once Sign In was pressed, a tool Pitboard could not find failed late.
#[test]
fn every_tool_is_offered_and_one_not_found_says_how_to_get_it() {
    let codex_known = adding(
        vec![claude_code()],
        vec![account(Some("work")).of("codex").signed_in().build()],
    )
    .sheet_text
    .expect("the sheet");
    assert_eq!(
        tools_and_missing(&codex_known),
        (vec!["claude", "codex", "desktop"], vec!["codex", "desktop"])
    );
    assert_eq!(codex_known.tool, "claude");
    let missing = codex_known.tools[1].missing.as_ref().expect("codex");
    assert_eq!(
        missing.title,
        "Pitboard can’t find the Codex CLI on this Mac."
    );
    assert_eq!(
        missing.detail,
        "Adding a Codex account runs the Codex CLI’s own sign-in. Pitboard doesn’t use the copy \
         inside the ChatGPT app. Install the CLI, then come back to this window."
    );
    assert_eq!(missing.link_title, "How to Install the Codex CLI");
    assert_eq!(missing.link, "https://developers.openai.com/codex/cli");
    assert!(codex_known.tools[1].message.contains("Codex’s own sign-in"));

    let codex_only = adding(vec![codex()], Vec::new())
        .sheet_text
        .expect("the sheet");
    assert_eq!(codex_only.tool, "codex", "the first tool found");
    assert_eq!(codex_only.message, codex_only.tools[1].message);
}

// Nothing found leaves nothing to sign in with, which each tool says.
#[test]
fn nothing_found_starts_on_claude_code_and_says_none_is_here() {
    let sheet = adding(Vec::new(), Vec::new())
        .sheet_text
        .expect("the sheet");
    assert_eq!(sheet.tool, "claude");
    assert_eq!(
        tools_and_missing(&sheet),
        (
            vec!["claude", "codex", "desktop"],
            vec!["claude", "codex", "desktop"]
        )
    );
    assert_eq!(
        sheet.tools[0].missing.as_ref().map(|m| m.link.as_str()),
        Some("https://docs.claude.com/en/docs/claude-code/setup")
    );

    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(warned(Vec::new(), vec![interrupted()])));
    machine.found = Vec::new();
    model.refresh(&mut machine);
    assert_eq!(
        model.shown().notices[0].lines[0],
        "An interrupted switch can’t be finished until Anthropic answers."
    );
}

/// Nothing is known to be here until a read asks. The asking is reading.rs's.
#[test]
fn what_is_installed_is_asked_once() {
    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.found = vec![claude_code(), codex()];
    model.refresh(&mut machine);
    model.send(present_sheet(Sheet::Add { provider: None }));
    model.run(&mut machine);
    let sheet = model.on_a_mac().sheet_text.expect("the sheet");
    assert_eq!(tools_and_missing(&sheet).1, ["desktop"]);
}

/// The sheet offers what the answer found once it is in. The asking is signing.rs's.
#[test]
fn what_is_installed_is_asked_again_when_the_form_for_another_account_opens() {
    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    model.refresh(&mut machine);
    machine.found = vec![claude_code(), codex()];
    model.send(present_sheet(Sheet::Add { provider: None }));
    assert_eq!(
        tools_and_missing(&model.on_a_mac().sheet_text.expect("the sheet")).1,
        ["codex", "desktop"],
        "what was found before"
    );
    model.run(&mut machine);
    assert_eq!(
        tools_and_missing(&model.on_a_mac().sheet_text.expect("the sheet")).1,
        ["desktop"]
    );
}

/// The form starts on the tool it was asked about, and otherwise on the first it offers.
#[test]
fn the_form_starts_on_the_tool_it_is_about() {
    let starts = |sheet: Sheet| {
        let mut model = Hand::new();
        model.send(present_sheet(sheet));
        model.shown().sheet_text.expect("the sheet").tool
    };
    assert_eq!(starts(Sheet::Add { provider: None }), "claude");
    assert_eq!(
        starts(Sheet::Add {
            provider: Some("codex".into())
        }),
        "codex"
    );
    assert_eq!(
        starts(Sheet::Name {
            provider: "codex".into(),
            email: "c@example.com".into()
        }),
        "codex"
    );
    assert_eq!(
        starts(Sheet::SignInAgain {
            provider: "codex".into(),
            label: "work".into()
        }),
        "codex"
    );
    assert_eq!(
        starts(Sheet::Rename {
            provider: "codex".into(),
            label: "work".into()
        }),
        "codex"
    );
}

/// Each sheet says what it is for, at the top: what signing in does and to whom, which login
/// is being named, what a rename changes. A rename starts with the name the account has.
///
/// NameSheets.swift's and SignInSheet.swift's titles and messages, which the Swift tested
/// nowhere.
#[test]
fn each_sheet_says_what_it_is_for() {
    let shown = |sheet: Sheet| {
        let mut model = Hand::new();
        model.send(present_sheet(sheet));
        model.shown().sheet_text.expect("the sheet")
    };
    let add = shown(Sheet::Add { provider: None });
    assert_eq!(add.title, titled("Add Account", "Add account"));
    assert_eq!(
        add.message,
        "Pitboard opens Claude Code’s own sign-in in your browser. Sign in as the account \
         you’re adding, and Pitboard parks its login beside the one in use."
    );
    assert_eq!((add.name.as_str(), add.prompt.as_str()), ("", "work"));
    assert_eq!(add.account, None);

    let again = shown(Sheet::SignInAgain {
        provider: "codex".into(),
        label: "work".into(),
    });
    assert_eq!(
        again.title,
        titled("Sign In to work Again", "Sign in to work again")
    );
    assert_eq!(
        again.message,
        "Pitboard opens Codex’s own sign-in in your browser. Sign in as work to give Pitboard \
         a new login for it."
    );
    assert_eq!(again.account.as_deref(), Some("work"));

    let name = shown(Sheet::Name {
        provider: "codex".into(),
        email: "c@example.com".into(),
    });
    assert_eq!(name.title, titled("Name This Account", "Name this account"));
    assert_eq!(
        name.message,
        "c@example.com is signed in to Codex. Pitboard parks its login under this name \
         whenever you switch to another account."
    );

    let rename = shown(Sheet::Rename {
        provider: "claude".into(),
        label: "work".into(),
    });
    assert_eq!(rename.title, "Rename “work”");
    assert_eq!(
        rename.message,
        "The account keeps its parked login. Only the name you switch to it by changes, here \
         and in the command line."
    );
    assert_eq!(
        (rename.name.as_str(), rename.prompt.as_str()),
        ("work", "work")
    );
    assert!(!rename.saving);
}

/// Name This Account starts on a name from the email address: the domain's first label for a
/// work address, and the part before any `+` for one at a mail service. A name another
/// account of the tool already has is passed over for the other, and with both taken the
/// field starts empty. An account of another tool takes no name from this one's.
#[test]
fn name_this_account_starts_on_a_name_from_the_email() {
    let named = |taken: Vec<Account>, provider: &str, email: &str| {
        let (mut model, _) = reading(taken);
        model.send(present_sheet(Sheet::Name {
            provider: provider.into(),
            email: email.into(),
        }));
        model.shown().sheet_text.expect("the sheet").name
    };
    assert_eq!(named(Vec::new(), "claude", "dana@acme.io"), "acme");
    assert_eq!(named(Vec::new(), "claude", "dana@eng.acme.io"), "eng");
    assert_eq!(
        named(Vec::new(), "codex", "Dana.Lee+pitboard@Gmail.com"),
        "dana.lee"
    );
    assert_eq!(named(Vec::new(), "claude", "dana@icloud.com"), "dana");
    for abroad in ["dana@yahoo.co.uk", "dana@hotmail.fr", "dana@web.de"] {
        assert_eq!(named(Vec::new(), "claude", abroad), "dana", "{abroad}");
    }
    let acme = || account(Some("acme")).signed_in().build();
    assert_eq!(named(vec![acme()], "claude", "dana@acme.io"), "dana");
    assert_eq!(
        named(
            vec![acme(), account(Some("dana")).build()],
            "claude",
            "dana@acme.io"
        ),
        ""
    );
    assert_eq!(
        named(
            vec![account(Some("acme")).of("codex").signed_in().build()],
            "claude",
            "dana@acme.io"
        ),
        "acme"
    );
    assert_eq!(named(Vec::new(), "claude", ""), "");
}

/// Each sheet's default button says what pressing it does: a sheet that signs in says Sign
/// In, one that names a login signed in now says Save, and a rename says Rename.
///
/// SignInSheet.swift's Sign In and NameSheets.swift's `saveTitle`, which chose between Save
/// and Rename by the sheet, which the Swift tested nowhere, and which
/// AccountsWindowTests.swift's testRenamingAnAccount presses.
#[test]
fn each_sheets_default_button_says_what_it_does() {
    let confirm = |sheet: Sheet| {
        let mut model = Hand::new();
        model.send(present_sheet(sheet));
        model.shown().sheet_text.expect("the sheet").confirm
    };
    let sign_in = titled("Sign In", "Sign in");
    assert_eq!(confirm(Sheet::Add { provider: None }), sign_in);
    assert_eq!(
        confirm(Sheet::SignInAgain {
            provider: "codex".into(),
            label: "work".into(),
        }),
        sign_in
    );
    assert_eq!(
        confirm(Sheet::Name {
            provider: "codex".into(),
            email: "c@example.com".into(),
        }),
        "Save"
    );
    assert_eq!(
        confirm(Sheet::Rename {
            provider: "claude".into(),
            label: "work".into(),
        }),
        "Rename"
    );
}

/// A sign-in under way says what to do in the browser and what the field for a code is for,
/// and once the tool has refused a code typed back, why the field is offered again.
///
/// SignInSheet.swift's progress, which the Swift tested nowhere, and the sentence for a code
/// refused, which it never had.
#[test]
fn a_sign_in_under_way_says_what_to_do_in_the_browser() {
    let mut model = Hand::new();
    let jobs = model.send(Intent::SignIn {
        provider: "claude".into(),
        name: "travel".into(),
    });
    let [Job::SignIn { id, .. }] = jobs[..] else {
        panic!("a sign-in: {jobs:?}");
    };
    model.give(Answer::SignInStarted {
        id,
        started: Ok(()),
        put_back: None,
    });
    let text = model.shown().signing_in_text.expect("what it says");
    assert_eq!(
        text.title,
        titled("Signing In to Claude Code", "Signing in to Claude Code")
    );
    assert!(text.offers_link, "a browser sign-in offers its page");
    assert_eq!(
        text.message,
        "Finish signing in as travel in your browser. This closes once Claude Code says \
         you’re in."
    );
    assert_eq!(
        text.code_note,
        "Claude Code takes the code shown after you sign in, whether or not your browser came \
         back to it."
    );
    assert_eq!(text.refused, None);

    model.give(Answer::SignInSaid {
        id,
        text: "Paste code here if prompted > ".into(),
        step: None,
    });
    model.send(Intent::PasteCode {
        code: "half".into(),
    });
    model.give(Answer::SignInSaid {
        id,
        text: "Invalid code. Please make sure the full code was copied.\n".into(),
        step: None,
    });
    assert_eq!(
        model.shown().signing_in_text.and_then(|text| text.refused),
        Some(
            "Claude Code didn’t take that code. Copy the whole code your browser shows, and \
             paste it again."
                .into()
        )
    );
}

/// The question about quitting the app that holds a tool's login is asked in the app's name,
/// while it is asked.
///
/// MainWindow.swift's alert, which the Swift tested nowhere.
#[test]
fn the_question_about_quitting_an_app_is_asked_in_its_name() {
    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.held.insert("codex".into(), vec![chatgpt_holding()]);
    machine.apps = StandInApps::new(&[CHATGPT], true);
    switch(&mut model, &mut machine, "codex/spare");
    let asked = model.shown().quit_confirmation.expect("the question");
    assert_eq!(asked.title, "Quit ChatGPT to switch?");
    assert_eq!(
        asked.confirm,
        titled("Quit ChatGPT and Switch", "Quit ChatGPT and switch")
    );
    model.send(Intent::KeepAppOpen);
    assert_eq!(model.shown().quit_confirmation, None);
}

// The window.

/// A sheet is over the main window, so putting one up opens the window too, on the accounts
/// the sheet is about. Opening the window by itself leaves whatever sheet is up, and whichever
/// pane was showing.
///
/// Putting the sheet up is signing.rs's.
#[test]
fn opening_the_window_by_itself_leaves_the_sheet_up() {
    let rename = Sheet::Rename {
        provider: "claude".into(),
        label: "personal".into(),
    };
    let mut model = Hand::new();
    model.send(present_sheet(rename.clone()));
    model.send(Intent::ShowWindow { pane: None });
    let shown = model.shown();
    assert_eq!(
        shown.window_request,
        WindowRequest {
            serial: 2,
            pane: None,
            settings: false,
        }
    );
    assert_eq!(shown.sheet, Some(rename));
}

/// A request for the window can ask for a pane: a notice is said on the accounts pane, and
/// the menu's item for one opens the window there, whichever pane it was left on. A request
/// that asks for none leaves the pane as it is, so it does not carry a pane an earlier
/// request asked for, and a failure said in the window is over every pane.
#[test]
fn a_request_for_the_window_can_ask_for_a_pane() {
    let mut model = Hand::new();
    assert_eq!(model.shown().window_request.pane, None);
    model.send(Intent::ShowWindow {
        pane: Some(Pane::Machine),
    });
    assert_eq!(
        model.shown().window_request,
        WindowRequest {
            serial: 1,
            pane: Some(Pane::Machine),
            settings: false,
        }
    );
    model.send(Intent::ShowWindow {
        pane: Some(Pane::Accounts),
    });
    assert_eq!(
        model.shown().window_request,
        WindowRequest {
            serial: 2,
            pane: Some(Pane::Accounts),
            settings: false,
        }
    );
    model.send(Intent::ShowWindow { pane: None });
    assert_eq!(model.shown().window_request.pane, None);

    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.forgetting = Err(refusal("account_unknown", "Nothing is parked.", Vec::new()));
    model.send(Intent::Forget {
        qualified: "claude/personal".into(),
    });
    model.run(&mut machine);
    assert_eq!(
        model.shown().window_request,
        WindowRequest {
            serial: 4,
            pane: None,
            settings: false,
        },
        "an alert is over every pane"
    );
}

/// A failure of something asked for away from the window is said in the window, which is
/// opened for it, as an alert with everything it warned about, until somebody has read it.
/// Nothing that went wrong opens nothing.
#[test]
fn a_failure_away_from_the_window_is_said_in_it() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    model.send(Intent::Forget {
        qualified: "claude/alpha".into(),
    });
    model.run(&mut machine);
    let shown = model.shown();
    assert_eq!(shown.failure, None);
    assert_eq!(shown.failure_alert, None);
    assert_eq!(shown.window_request.serial, 0);

    machine.forgetting = Err(refusal(
        "cannot_forget_active_account",
        "beta is the account in use, so it cannot be forgotten.",
        vec![overridden()],
    ));
    model.send(Intent::Forget {
        qualified: "claude/beta".into(),
    });
    model.run(&mut machine);
    let shown = model.shown();
    assert_eq!(shown.window_request.serial, 1);
    let alert = shown.failure_alert.expect("an alert");
    assert_eq!(alert.title, "Couldn’t forget beta");
    assert_eq!(
        alert.message,
        "beta is the account in use, so it cannot be forgotten.\n\nANTHROPIC_API_KEY is set"
    );

    model.send(Intent::DismissFailure);
    let shown = model.shown();
    assert_eq!(shown.failure, None);
    assert_eq!(shown.failure_alert, None);
    assert_eq!(
        shown.window_request.serial, 1,
        "putting it away opens nothing"
    );
}

/// A switch asked for from the menu or a notification that fails is said in the window,
/// since neither has anywhere to put a sentence. One that works opens nothing: the menu bar
/// already shows it.
#[test]
fn only_a_switch_asked_for_away_from_the_window_that_fails_opens_it() {
    let (mut model, mut machine) = reading(vec![
        account(Some("work")).signed_in().build(),
        account(Some("personal")).build(),
    ]);
    machine.switched = switched("claude", "work", "personal", Vec::new());
    switch(&mut model, &mut machine, "claude/personal");
    let shown = model.shown();
    assert_eq!(shown.failure, None);
    assert_eq!(shown.window_request.serial, 0);

    machine.switched = Err(refusal(
        "account_unknown",
        "There is no account called claude/nobody.",
        Vec::new(),
    ));
    switch(&mut model, &mut machine, "claude/nobody");
    let shown = model.shown();
    let failure = shown.failure.expect("said in the window");
    assert_eq!(failure.title, "Couldn’t switch to nobody");
    assert_eq!(failure.message, "There is no account called claude/nobody.");
    assert_eq!(shown.window_request.serial, 1);
}

/// What the window shows of the time is made again on the minute tick, once started, and on
/// nothing else: a reset that was "resets in 2h 05m" says "resets in 2h 04m" a minute later
/// with nothing read, as the Swift app's bars redrew every minute.
#[test]
fn what_depends_on_the_time_is_made_again_every_minute() {
    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(status(vec![
        account(Some("work"))
            .signed_in()
            .limits(vec![
                window("session", 42.0).resets(Some(NOON + 2 * 3600 + 5 * 60)),
            ])
            .build(),
    ])));
    model.send(Intent::Start);
    model.run(&mut machine);
    let resets = |model: &Hand| {
        model.shown().sections[0].accounts[0].limits[0]
            .resets
            .clone()
    };
    assert_eq!(resets(&model), "resets in 2h 05m");
    let due = model.state.next_due().expect("a timer");
    assert!(due <= model.now.running + Duration::from_secs(60));

    model.later(Duration::from_secs(60));
    assert!(model.tick().iter().all(|job| matches!(job, Job::Look)));
    assert_eq!(resets(&model), "resets in 2h 04m");
    assert!(
        model.state.next_due().expect("a timer") <= model.now.running + Duration::from_secs(60),
        "and again a minute later"
    );
}

/// The words for who is asked follow the tools shown, so a machine with Claude Code alone
/// reads exactly as it did: here in what an interrupted switch waits for.
#[test]
fn the_service_asked_is_named_for_the_tools_shown() {
    let mut model = at_noon();
    let mut machine = Stub::reading(Ok(warned(
        vec![account(Some("work")).signed_in().build()],
        vec![interrupted()],
    )));
    model.refresh(&mut machine);
    let shown = model.shown();
    assert_eq!(
        shown.notices[0].lines[0],
        "An interrupted switch can’t be finished until Anthropic answers."
    );
    assert!(!shown.shows_tools);

    machine.answer = Ok(warned(
        vec![
            account(Some("work")).signed_in().build(),
            account(Some("job")).of("codex").signed_in().build(),
        ],
        vec![interrupted()],
    ));
    model.refresh(&mut machine);
    let shown = model.shown();
    assert_eq!(
        shown.notices[0].lines[0],
        "An interrupted switch can’t be finished until Anthropic or OpenAI answers."
    );
    assert!(shown.shows_tools);
}

// What a sign-in or a recording inside Claude says in its sheet, driven through the model.

#[test]
fn the_quit_question_is_carried_with_the_sheet_it_was_asked_from() {
    let (mut model, mut machine) = adding_in_claude(true);
    model.send(travel());
    model.run(&mut machine);
    let shown = model.on_a_mac();
    assert_eq!(shown.sheet, Some(add(Some("desktop"))));
    assert!(shown.quit_question.is_some());
    assert_eq!(
        shown.quit_confirmation.expect("asked").confirm,
        "Quit Claude and Park work"
    );
    assert!(
        shown.sheet_text.expect("the form").saving,
        "its button holds back"
    );
}

#[test]
fn a_use_refused_in_doubt_opens_the_add_sheet_on_the_account_claude_shows() {
    let mut model = at_noon();
    let mut machine = desktop_machine(false);
    machine.switched = Err(refusal(
        "live_account_not_enrolled",
        "Claude is signed in to an account you haven’t added, so it can’t be parked.",
        Vec::new(),
    ));
    model.refresh(&mut machine);
    switch(&mut model, &mut machine, "desktop/home");
    model.send(present_sheet(add(Some("desktop"))));
    model.run(&mut machine);
    let form = model.on_a_mac().sheet_text.expect("the form");
    assert_eq!(form.confirm, "Add the Account Claude Shows");
    assert_eq!(
        form.message,
        "Pitboard can’t identify the login Claude holds. Open Claude, check which account it \
         shows, and quit it. Pitboard records that account under this name: one you added, or \
         a new one."
    );
}

#[test]
fn escape_never_leaves_an_add_with_its_account_parked() {
    let (mut model, mut machine) = adding_in_claude(true);
    machine.put_back = Some("desktop/work".into());
    model.send(travel());
    model.run(&mut machine);
    let question = model.shown().quit_question.expect("asked");
    model.send(Intent::QuitAndSwitch {
        qualified: question.qualified,
    });
    let quitting = model.on_a_mac().signing_in_text.expect("the page");
    assert_eq!(quitting.waiting, "Quitting Claude");
    assert!(!quitting.escape_cancels);
    assert!(!quitting.cancel.enabled);
    model.run(&mut machine);
    let waiting = model.on_a_mac().signing_in_text.expect("the page");
    assert!(!waiting.escape_cancels, "Escape would leave work parked");
    assert_eq!(waiting.cancel.title, "Later");
    assert_eq!(
        waiting.undo.map(|undo| undo.title).as_deref(),
        Some("Put work Back")
    );
}

#[test]
fn the_sheet_says_claude_is_quitting_and_its_account_is_read() {
    // `None` is what each Add Account in the app opens, on Claude Code here.
    for opened_on in [Some("desktop"), None] {
        let mut model = at_noon();
        let mut machine = desktop_machine(true);
        machine.answer = Ok(warned(
            vec![
                desktop_account("work", true),
                desktop_account("home", false),
            ],
            vec![desktop_warning("in_use_unconfirmed", "in doubt")],
        ));
        model.refresh(&mut machine);
        model.send(present_sheet(add(opened_on)));
        model.run(&mut machine);
        model.send(travel());
        model.run(&mut machine);
        assert_eq!(
            model.on_a_mac().sheet_text.expect("the form").progress,
            None,
            "nothing yet while asked"
        );
        let question = model.shown().quit_question.expect("asked");
        model.send(Intent::QuitAndSwitch {
            qualified: question.qualified,
        });
        assert_eq!(
            model
                .on_a_mac()
                .sheet_text
                .expect("the form")
                .progress
                .as_deref(),
            Some("Quitting Claude…"),
            "opened on {opened_on:?}"
        );
        let quit = model.next();
        let answer = machine.answer(quit);
        model.give(answer);
        assert_eq!(
            model
                .on_a_mac()
                .sheet_text
                .expect("the form")
                .progress
                .as_deref(),
            Some("Reading Claude’s account…"),
            "opened on {opened_on:?}"
        );
        model.run(&mut machine);
        assert_eq!(model.shown().sheet, None, "closed once recorded");
    }
}

#[test]
fn an_add_this_app_waits_for_is_not_a_notice_too() {
    let mut model = at_noon();
    let read = crate::Status {
        awaiting: vec![awaiting("travel", "work", false)],
        ..status(vec![
            desktop_account("work", false),
            desktop_account("home", false),
        ])
    };
    let mut machine = desktop_machine(false);
    machine.answer = Ok(read);
    machine.put_back = Some("desktop/work".into());
    machine.enrolling_current = Err(refusal(
        "live_credential_absent",
        "Claude isn’t signed in.",
        Vec::new(),
    ));
    model.refresh(&mut machine);
    assert!(ids(&model.on_a_mac()).contains(&"awaiting/desktop".into()));
    model.send(present_sheet(add(Some("desktop"))));
    model.run(&mut machine);
    model.send(travel());
    model.run(&mut machine);
    model.send(travel());
    model.run(&mut machine);
    let shown = model.on_a_mac();
    assert!(shown.signing_in.is_some(), "this app waits for it");
    assert!(
        !ids(&shown).contains(&"awaiting/desktop".into()),
        "its sheet offers the same"
    );
}
