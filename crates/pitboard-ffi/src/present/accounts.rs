//! The accounts as the menu bar, the menu and the window describe them, from Accounts.swift:
//! which account the bar is about and what it says, the sections per tool, and each account's
//! row, with its limits.

use super::words;
use super::{
    AccountItem, AccountSection, ItemAction, ItemOffer, LimitPace, LimitRow, MenuBarText,
    PaceStanding, Question, Seen, WindowOffer,
};
use crate::account_windows::{forget_message_on, windows_of_account};
use crate::model::{Intent, Sheet};
use crate::{Account, Limit, Tool};
use pitboard_core::pace::{self, Pace, Standing};
use pitboard_core::words as said;
use std::cmp::Ordering;

/// The limit worth putting in the menu bar: the account's own, not one scoped to a single
/// model, and one the account is working against when the service says which. A scoped row
/// at 98% would otherwise read as though everything had stopped. The fullest of those, and
/// the first of them where two are as full, as Swift's `max(by:)` keeps the first.
pub(crate) fn headline(windows: &[Limit]) -> Option<&Limit> {
    let own: Vec<&Limit> = windows.iter().filter(|w| w.scope.is_none()).collect();
    let candidates: Vec<&Limit> = if own.is_empty() {
        windows.iter().collect()
    } else {
        own
    };
    let active: Vec<&Limit> = candidates.iter().copied().filter(|w| w.is_active).collect();
    let pool = if active.is_empty() {
        candidates
    } else {
        active
    };
    // A later limit takes the place only of one less full than it, as `max(by:)` keeps its
    // result unless the next is in increasing order after it.
    pool.into_iter()
        .fold(None, |best: Option<&Limit>, limit| match best {
            Some(best) if best.percent.partial_cmp(&limit.percent) != Some(Ordering::Less) => {
                Some(best)
            }
            _ => Some(limit),
        })
}

/// Tool codes without repeats, in the order `tools` lists them, and any it does not list
/// after those in the order they came. The core already lists rows this way; this keeps a
/// view from depending on it.
pub(crate) fn in_order(codes: &[&str], tools: &[Tool]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let listed = tools.iter().map(|tool| tool.code.as_str());
    for code in listed.chain(codes.iter().copied()) {
        if codes.contains(&code) && !seen.iter().any(|kept| kept == code) {
            seen.push(code.to_owned());
        }
    }
    seen
}

fn providers(accounts: &[Account]) -> Vec<&str> {
    accounts
        .iter()
        .map(|account| account.provider.as_str())
        .collect()
}

/// The account the menu bar is about. With more than one tool there is an account in use in
/// each, and one bar: it follows the enrolled one whose headline limit is most used, so what
/// it shows is the account closest to running out, and a tie goes to the tool `tools` lists
/// first. Nobody enrolled signed in anywhere, it says what is signed in, as one tool would.
pub(crate) fn titled<'a>(accounts: &'a [Account], tools: &[Tool]) -> Option<&'a Account> {
    let providers = in_order(&providers(accounts), tools);
    if providers.len() <= 1 {
        return accounts.iter().find(|account| account.signed_in);
    }
    let used = |account: &Account| {
        headline(account.usage.as_ref().map_or(&[], |usage| &usage.windows))
            .map_or(-1.0, |limit| limit.percent)
    };
    let mut closest: Option<&Account> = None;
    for provider in &providers {
        let in_use = accounts.iter().find(|account| {
            &account.provider == provider && account.signed_in && account.label.is_some()
        });
        if let Some(in_use) = in_use
            && closest.is_none_or(|closest| used(in_use) > used(closest))
        {
            closest = Some(in_use);
        }
    }
    closest.or_else(|| accounts.iter().find(|account| account.signed_in))
}

/// What the menu bar says: the account in use and the limit closest to its end, or that
/// limit's figure alone. Nothing is known until the first read, and an account signed in
/// but not enrolled has no name, so it is called unnamed, which says what it is and fits
/// where an address would not. Long labels are cut, because the bar is shared with
/// everything else running.
pub(crate) fn bar_title(accounts: &[Account], tools: &[Tool], usage_only: bool) -> String {
    let Some(account) = titled(accounts, tools) else {
        return String::new();
    };
    let percent = headline(account.usage.as_ref().map_or(&[], |usage| &usage.windows))
        .map(|limit| words::figure(limit.percent));
    if usage_only {
        return percent.unwrap_or_default();
    }
    let name = words::bar_name(account.label.as_deref().unwrap_or("unnamed"));
    match percent {
        Some(percent) => format!("{name} {percent}"),
        None => name,
    }
}

pub(crate) fn menu_bar(seen: &Seen) -> MenuBarText {
    let accounts = seen.accounts();
    let name_and_usage = bar_title(accounts, &seen.tools, false);
    // Once more than one tool is shown it names the tool too: the bar follows whichever
    // account is closest to running out, and two tools can each have a `work`.
    let spoken = if name_and_usage.is_empty() {
        "Pitboard".to_owned()
    } else {
        match titled(accounts, &seen.tools)
            .filter(|_| seen.shows_tools())
            .and_then(|account| seen.tool(&account.provider))
        {
            Some(tool) => format!("Pitboard, {name_and_usage}, {}", tool.name),
            None => format!("Pitboard, {name_and_usage}"),
        }
    };
    MenuBarText {
        usage: bar_title(accounts, &seen.tools, true),
        name_and_usage,
        spoken,
    }
}

/// The accounts a section per tool, in the order the tools are listed. One section, unheaded
/// and in the order given, when they are all one tool's, so a machine with one tool looks
/// exactly as it always did. A tool Pitboard does not list still gets a section, after the
/// ones it lists, headed by its code, rather than its accounts going missing.
pub(crate) fn sections(seen: &Seen) -> Vec<AccountSection> {
    let accounts = seen.accounts();
    let providers = in_order(&providers(accounts), &seen.tools);
    if providers.len() <= 1 {
        return providers
            .into_iter()
            .map(|id| AccountSection {
                id,
                heading: None,
                accounts: accounts.iter().map(|account| item(seen, account)).collect(),
            })
            .collect();
    }
    providers
        .into_iter()
        .map(|code| AccountSection {
            heading: Some(seen.tool_name(&code)),
            accounts: accounts
                .iter()
                .filter(|account| account.provider == code)
                .map(|account| item(seen, account))
                .collect(),
            id: code,
        })
        .collect()
}

/// One account as the menu and the window describe it, from the account alone and what is
/// under way: the account a switch is running for, and whether a sign-in is.
pub(crate) fn item(seen: &Seen, account: &Account) -> AccountItem {
    let state = seen.state;
    let switching_for = state.switch_under_way();
    let busy = state.sign_in_under_way();
    let in_use = account.signed_in;
    let needs_sign_in = !account.unplaced
        && account.label.is_some()
        && !account.signed_in
        && !account.switchable
        && !login_unread(account);
    let switching = account.qualified.is_some() && account.qualified.as_deref() == switching_for;
    let title = if account.unplaced {
        "Login Pitboard can’t use".to_owned()
    } else {
        account
            .label
            .clone()
            .unwrap_or_else(|| account.email.clone())
    };
    let spoken_name = seen.name_of(account);
    let action = action(account, switching_for.is_some(), busy, &spoken_name);
    let problem = if account.unplaced || (!account.switchable && !account.signed_in) {
        account.stale_explanation.clone()
    } else {
        None
    };
    let stale_note = if problem.is_none() {
        account.stale_explanation.clone()
    } else {
        None
    };
    let parked_note = if account.signed_in {
        None
    } else {
        said::parked_life(
            account.parked.as_ref().and_then(|p| p.refresh_expires_at),
            seen.now,
        )
    };
    let pace = runs_out_first(account, seen.now).map(|(limit, in_seconds)| {
        let windows = account.usage.as_ref().map_or(&[][..], |u| &u.windows);
        words::capitalised(&said::runs_out(&scoped_name(&windows[limit]), in_seconds))
    });
    let mut spoken = vec![spoken_name.clone()];
    if in_use {
        spoken.push("in use".into());
    }
    if needs_sign_in {
        spoken.push("needs signing in again".into());
    }
    AccountItem {
        id: account.id.clone(),
        provider: account.provider.clone(),
        qualified: account.qualified.clone(),
        summary: summary(seen, account, switching, needs_sign_in),
        title,
        email: account.email.clone(),
        plan: account.plan.clone(),
        spoken: spoken.join(", "),
        in_use,
        needs_sign_in,
        unplaced: account.unplaced,
        switching,
        offers: offers(account, action.as_ref(), busy),
        windows: windows_of_account(account, seen.accounts())
            .into_iter()
            .map(|window| WindowOffer {
                title: format!("Open {}", window.site.name),
                window,
            })
            .collect(),
        forget: forget(seen, account, &spoken_name),
        action,
        help: problem.clone().or_else(|| stale_note.clone()),
        problem,
        stale_note,
        pace,
        parked_note,
        limits: account
            .usage
            .as_ref()
            .map(|usage| {
                let read_at = usage.observed_at.unwrap_or(seen.now);
                usage
                    .windows
                    .iter()
                    .map(|l| limit_row(l, read_at, seen.now))
                    .collect()
            })
            .unwrap_or_default(),
        spoken_name,
    }
}

/// What an account's own menu offers to do to it, above its windows: its own action where
/// that switches to it or names it, and then, for an account Pitboard has a name for and can
/// place, signing in to it again and renaming it. Signing in again is offered here for every
/// such account, so not a second time as the account's own action, and is held back while a
/// sign-in runs, as the action is; nothing else is held back, here or by a switch under way.
///
/// AccountsPane.swift's shortcut menu as it was at c1be7c3: its `offeredInItsMenu`, and the
/// items it worded and sent from `renamable` and `busy`.
fn offers(account: &Account, action: Option<&ItemAction>, busy: bool) -> Vec<ItemOffer> {
    let offer = |title: String, intent: Intent, enabled: bool| ItemOffer {
        title,
        intent,
        enabled,
        confirm: None,
    };
    let own = action.and_then(|action| {
        let title = match &action.intent {
            Intent::SwitchTo { .. } => format!("Use {}", account.label.as_deref()?),
            Intent::PresentSheet {
                sheet: Sheet::Name { .. },
            } => action.title.clone(),
            _ => return None,
        };
        Some(offer(title, action.intent.clone(), true))
    });
    let named = account
        .label
        .as_deref()
        .filter(|_| !account.unplaced)
        .map(|label| {
            let provider = account.provider.clone();
            let label = label.to_owned();
            [
                offer(
                    "Sign In Again…".into(),
                    Intent::PresentSheet {
                        sheet: Sheet::SignInAgain {
                            provider: provider.clone(),
                            label: label.clone(),
                        },
                    },
                    !busy,
                ),
                offer(
                    "Rename…".into(),
                    Intent::PresentSheet {
                        sheet: Sheet::Rename { provider, label },
                    },
                    true,
                ),
            ]
        });
    own.into_iter().chain(named.into_iter().flatten()).collect()
}

/// Forgetting an account, with the question asked first, where it may be forgotten: one
/// enrolled, and not the one in use, whose record is the only one of who is signed in, and
/// which the core refuses. The question names it, and says what forgetting it deletes in the
/// words the account windows say it in.
fn forget(seen: &Seen, account: &Account, spoken_name: &str) -> Option<ItemOffer> {
    let qualified = account
        .qualified
        .as_ref()
        .filter(|_| account.label.is_some() && !account.signed_in)?;
    Some(ItemOffer {
        title: "Forget…".into(),
        intent: Intent::Forget {
            qualified: qualified.clone(),
        },
        enabled: true,
        confirm: Some(Question {
            title: format!("Forget “{spoken_name}”?"),
            message: forget_message_on(seen.os, account, seen.accounts()),
            confirm: "Forget".into(),
        }),
    })
}

/// What pressing an account does, decided once so the menu and the window cannot disagree.
///
/// A switch under way holds everything back, since each change waits for the one before. A
/// sign-in under way holds back only another sign-in: it waits on a person in a browser, and
/// switching meanwhile is theirs to do.
fn action(account: &Account, switching: bool, busy: bool, spoken: &str) -> Option<ItemAction> {
    if account.unplaced || switching {
        return None;
    }
    let Some(label) = account.label.as_deref() else {
        return account.signed_in.then(|| ItemAction {
            title: "Name…".into(),
            spoken: format!("Name {spoken}…"),
            intent: Intent::PresentSheet {
                sheet: Sheet::Name {
                    provider: account.provider.clone(),
                    email: account.email.clone(),
                },
            },
        });
    };
    if account.signed_in {
        return None;
    }
    if account.switchable
        && let Some(qualified) = &account.qualified
    {
        return Some(ItemAction {
            title: "Use".into(),
            spoken: format!("Use {spoken}"),
            intent: Intent::SwitchTo {
                qualified: qualified.clone(),
            },
        });
    }
    (!busy).then(|| ItemAction {
        title: "Sign In Again…".into(),
        spoken: format!("Sign In to {spoken} Again…"),
        intent: Intent::PresentSheet {
            sheet: Sheet::SignInAgain {
                provider: account.provider.clone(),
                label: label.to_owned(),
            },
        },
    })
}

/// A limit's name as a sentence says it, with the model it is scoped to: "weekly Fable".
fn scoped_name(limit: &Limit) -> String {
    said::scoped_limit_name(&limit.kind, limit.length_seconds, limit.scope.as_deref())
}

/// A limit's pace as read at `read_at` and told at `now`, as `pitboard status` works it out.
fn pace_of(limit: &Limit, read_at: i64, now: i64) -> Option<Pace> {
    pace::of(
        limit.percent,
        limit.resets_at,
        limit.length_seconds,
        read_at,
        now,
    )
}

/// Which limit of the account in use runs out first at its pace, by its place among the
/// account's limits, and in how many seconds. Nothing for an account not in use: it is
/// parked, and nothing of it runs out.
fn runs_out_first(account: &Account, now: i64) -> Option<(usize, i64)> {
    if !account.signed_in {
        return None;
    }
    let usage = account.usage.as_ref()?;
    let read_at = usage.observed_at.unwrap_or(now);
    pace::first_to_run_out(
        usage
            .windows
            .iter()
            .enumerate()
            .map(|(place, limit)| (place, pace_of(limit, read_at, now))),
    )
}

/// An account not in use whose login may be the one in use that could not be read, which no
/// sign-in would fix.
fn login_unread(account: &Account) -> bool {
    !account.signed_in && account.stale.as_deref() == Some("login_unreadable")
}

/// Under the name in the menu, one line: what the account's limits stand at, and when one
/// that has run out comes back. What stands in the way instead, when something does.
fn summary(seen: &Seen, account: &Account, switching: bool, needs_sign_in: bool) -> String {
    if switching {
        return "Switching…".into();
    }
    if account.unplaced || login_unread(account) {
        return account
            .stale_explanation
            .clone()
            .unwrap_or_else(|| "Can’t be read or switched".into());
    }
    if account.label.is_none() {
        return "Not named yet".into();
    }
    if needs_sign_in {
        return "Needs signing in again".into();
    }
    let windows = account.usage.as_ref().map_or(&[][..], |u| &u.windows);
    if windows.is_empty() {
        return account.email.clone();
    }
    // The limit that runs out first, said beside its own figure, where there is one.
    let first = runs_out_first(account, seen.now);
    let said: Vec<String> = windows
        .iter()
        .enumerate()
        .map(|(at, limit)| {
            let scoped = scoped_name(limit);
            match limit.resets_at {
                Some(back) if limit.percent >= 100.0 => {
                    if back > seen.now {
                        format!("{scoped} used up until {}", seen.clock(back))
                    } else {
                        format!("{scoped} used up")
                    }
                }
                _ => match first {
                    Some((first, in_seconds)) if first == at => format!(
                        "{scoped} {} ({})",
                        words::figure(limit.percent),
                        words::runs_out_in(in_seconds)
                    ),
                    _ => format!("{scoped} {}", words::figure(limit.percent)),
                },
            }
        })
        .collect();
    words::capitalised(&said.join(", "))
}

/// One limit as a row of its account's bars, from a reading taken at `read_at`, as of `now`.
pub(crate) fn limit_row(limit: &Limit, read_at: i64, now: i64) -> LimitRow {
    let name = scoped_name(limit);
    let pace = pace_of(limit, read_at, now);
    LimitRow {
        short: said::limit_column(&limit.kind, limit.length_seconds, limit.scope.as_deref()),
        percent: limit.percent,
        figure: words::figure(limit.percent),
        level: crate::usage_level(limit.percent),
        resets: limit
            .resets_at
            .map(|at| said::resets(at, now))
            .unwrap_or_default(),
        spoken: words::spoken_limit(
            &name,
            limit.percent,
            limit.resets_at.map(|at| at - now),
            pace.as_ref(),
        ),
        pace: pace.as_ref().map(|pace| LimitPace {
            expected: pace.expected,
            standing: match pace.standing {
                Standing::Under => PaceStanding::Under,
                Standing::Even => PaceStanding::Even,
                Standing::Over { .. } => PaceStanding::Over,
            },
            said: said::pace_column(pace),
            help: words::pace_help(pace),
        }),
        name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::present::testing::{LimitExt, account, both_tools, claude_code, codex, window};

    fn title(accounts: &[Account]) -> String {
        bar_title(accounts, &[], false)
    }

    /// MenuTests.swift's theMenuBarNamesTheAccountInUseAndItsTightestLimit.
    #[test]
    fn the_menu_bar_names_the_account_in_use_and_its_tightest_limit() {
        let read = [
            account(Some("work"))
                .signed_in()
                .limits(vec![window("session", 12.0), window("weekly_all", 64.4)])
                .build(),
            account(Some("personal"))
                .limits(vec![window("session", 99.0)])
                .build(),
        ];
        assert_eq!(title(&read), "work 64%");
    }

    /// macOS hides the widest menu bar items first when there is no room, and a notched
    /// display has little. The settings can drop the name, and then the figure too, leaving
    /// the mark.
    ///
    /// MenuTests.swift's theMenuBarShowsNoMoreThanTheSettingsAskFor, and
    /// PresentationTests.swift's theMenuBarSaysAsMuchAsTheSettingsAskAndTheMarkAloneSaysNothing:
    /// the mark alone is the app's setting, which shows neither.
    #[test]
    fn the_menu_bar_shows_no_more_than_the_settings_ask_for() {
        let read = [account(Some("work"))
            .signed_in()
            .limits(vec![window("session", 12.0), window("weekly_all", 64.4)])
            .build()];
        assert_eq!(bar_title(&read, &[], false), "work 64%");
        assert_eq!(bar_title(&read, &[], true), "64%");
        assert_eq!(
            bar_title(&[account(Some("work")).signed_in().build()], &[], true),
            "",
            "no figure to show until one is measured"
        );
    }

    /// Before the first read there is nothing true to say, and the icon is already there, so
    /// the item shows the mark alone rather than a name it has not checked.
    ///
    /// MenuTests.swift's theMenuBarSaysNothingBeforeTheFirstRead.
    #[test]
    fn the_menu_bar_says_nothing_before_the_first_read() {
        assert_eq!(title(&[]), "");
        assert_eq!(bar_title(&[], &both_tools(), true), "");
    }

    /// The bar is shared with everything else running, so a name longer than twelve
    /// characters is cut to eleven and an ellipsis, and a name that fits is left whole.
    ///
    /// MenuTests.swift's aLongLabelIsBounded and PresentationTests.swift's
    /// aNameLongerThanTwelveCharactersIsCutToElevenAndAnEllipsis.
    #[test]
    fn a_name_longer_than_twelve_characters_is_cut_to_eleven_and_an_ellipsis() {
        let bar = |label: &str, usage_only: bool| {
            bar_title(
                &[account(Some(label))
                    .signed_in()
                    .limits(vec![window("session", 5.0)])
                    .build()],
                &[],
                usage_only,
            )
        };
        assert_eq!(bar("abcdefghijkl", false), "abcdefghijkl 5%");
        assert_eq!(bar("abcdefghijklm", false), "abcdefghijk… 5%");
        assert_eq!(bar("a-very-long-account-label", false), "a-very-long… 5%");
        assert!(bar("a-very-long-account-label", false).chars().count() <= 17);
        assert_eq!(bar("a-very-long-account-label", true), "5%");
    }

    /// Before anything is measured the bar still says whose account is in use, and a setting
    /// that asks for the figure alone shows the mark until there is one.
    ///
    /// PresentationTests.swift's anAccountWithNothingMeasuredIsNamedAloneInTheBar.
    #[test]
    fn an_account_with_nothing_measured_is_named_alone_in_the_bar() {
        let unmeasured = [account(Some("work")).signed_in().build()];
        assert_eq!(title(&unmeasured), "work");
        assert_eq!(
            title(&[account(Some("work")).signed_in().unmeasured().build()]),
            "work"
        );
        assert_eq!(
            title(&[account(None).signed_in().uuid("u").build()]),
            "unnamed"
        );
        assert_eq!(bar_title(&unmeasured, &[], true), "");
    }

    /// With nobody signed in there is no account in use to name, and naming one that is not
    /// would be wrong.
    ///
    /// PresentationTests.swift's nobodySignedInLeavesTheBarToItsMark.
    #[test]
    fn nobody_signed_in_leaves_the_bar_to_its_mark() {
        let read = [
            account(Some("work"))
                .limits(vec![window("session", 5.0)])
                .build(),
            account(Some("spare")).build(),
        ];
        for usage_only in [false, true] {
            assert_eq!(bar_title(&read, &both_tools(), usage_only), "");
        }
    }

    /// The figure is a whole percentage, rounded the way a person rounds: a half goes up.
    ///
    /// PresentationTests.swift's theBarsFigureIsRoundedToTheNearestWholePercent.
    #[test]
    fn the_bars_figure_is_rounded_to_the_nearest_whole_percent() {
        let bar = |percent: f64| {
            bar_title(
                &[account(Some("work"))
                    .signed_in()
                    .limits(vec![window("session", percent)])
                    .build()],
                &[],
                true,
            )
        };
        assert_eq!(bar(64.4), "64%");
        assert_eq!(bar(64.5), "65%");
        assert_eq!(bar(0.4), "0%");
        assert_eq!(bar(100.0), "100%");
    }

    /// A limit scoped to one model is not the account's limit. Reading 98% in the menu bar
    /// while the binding limit is at 30% says the day is over when it is not.
    ///
    /// MenuTests.swift's aScopedLimitDoesNotTakeTheMenuBar.
    #[test]
    fn a_scoped_limit_does_not_take_the_menu_bar() {
        let read = [account(Some("work"))
            .signed_in()
            .limits(vec![
                window("session", 30.0),
                window("weekly_scoped", 98.0).scope("Fable"),
            ])
            .build()];
        assert_eq!(title(&read), "work 30%");
    }

    /// Among the account's own limits, the one it is working against wins, and failing that
    /// the fullest; two as full give the first.
    ///
    /// MenuTests.swift's theLimitInUseIsThePreferredHeadline.
    #[test]
    fn the_limit_in_use_is_the_preferred_headline() {
        let windows = [
            window("weekly_all", 80.0).active(false),
            window("session", 40.0),
        ];
        assert_eq!(headline(&windows).map(|l| l.kind.as_str()), Some("session"));
        assert_eq!(headline(&[]), None);
        let tied = [window("session", 40.0), window("weekly_all", 40.0)];
        assert_eq!(headline(&tied).map(|l| l.kind.as_str()), Some("session"));
    }

    /// With an account in use in each tool and one bar, the bar shows the one closest to
    /// running out, judged by its own limits and not one scoped to a single model.
    ///
    /// MenuTests.swift's theMenuBarFollowsTheMostUsedAccountAcrossTools and
    /// PresentationTests.swift's theBarIsAboutTheToolClosestToRunningOut.
    #[test]
    fn the_bar_is_about_the_tool_closest_to_running_out() {
        let read = [
            account(Some("personal"))
                .signed_in()
                .limits(vec![window("session", 40.0)])
                .build(),
            account(Some("job"))
                .of("codex")
                .signed_in()
                .limits(vec![window("five_hour", 71.0).length(18_000)])
                .build(),
            account(Some("spare"))
                .of("codex")
                .limits(vec![window("five_hour", 99.0).length(18_000)])
                .build(),
        ];
        assert_eq!(bar_title(&read, &both_tools(), false), "job 71%");
        let scoped = [
            account(Some("work"))
                .signed_in()
                .limits(vec![
                    window("session", 30.0),
                    window("weekly_scoped", 99.0).scope("Fable"),
                ])
                .build(),
            account(Some("job"))
                .of("codex")
                .signed_in()
                .limits(vec![window("five_hour", 50.0)])
                .build(),
        ];
        assert_eq!(
            titled(&scoped, &both_tools()).and_then(|a| a.label.as_deref()),
            Some("job")
        );
        assert_eq!(bar_title(&scoped, &both_tools(), false), "job 50%");
    }

    /// A tie goes to the tool the listing puts first, whatever order the rows came in, so the
    /// bar does not move between two accounts at the same figure from one read to the next,
    /// and a login nobody has named does not take the bar from one that has a name.
    ///
    /// MenuTests.swift's aTieInTheMenuBarGoesToTheFirstTool and PresentationTests.swift's
    /// aTieGoesToTheToolListedFirstAndNotTheRowThatCameFirst.
    #[test]
    fn a_tie_goes_to_the_tool_listed_first_and_not_the_row_that_came_first() {
        let tied = [
            account(Some("job"))
                .of("codex")
                .signed_in()
                .limits(vec![window("five_hour", 50.0)])
                .build(),
            account(Some("work"))
                .signed_in()
                .limits(vec![window("session", 50.0)])
                .build(),
        ];
        let label = |tools: &[Tool]| titled(&tied, tools).and_then(|a| a.label.clone());
        assert_eq!(label(&[claude_code(), codex()]).as_deref(), Some("work"));
        assert_eq!(label(&[codex(), claude_code()]).as_deref(), Some("job"));
        assert_eq!(bar_title(&tied, &both_tools(), false), "work 50%");

        let unnamed = [
            account(Some("personal"))
                .signed_in()
                .limits(vec![window("session", 10.0)])
                .build(),
            account(None)
                .of("codex")
                .signed_in()
                .uuid("c")
                .limits(vec![window("five_hour", 99.0)])
                .build(),
        ];
        assert_eq!(bar_title(&unnamed, &both_tools(), false), "personal 10%");
    }

    /// An account with nothing measured is not closer to running out than one at nought.
    ///
    /// PresentationTests.swift's anAccountWithNothingMeasuredGivesTheBarToOneThatWasMeasured.
    #[test]
    fn an_account_with_nothing_measured_gives_the_bar_to_one_that_was_measured() {
        let read = [
            account(Some("work")).signed_in().build(),
            account(Some("job"))
                .of("codex")
                .signed_in()
                .limits(vec![window("five_hour", 0.0)])
                .build(),
        ];
        assert_eq!(
            titled(&read, &both_tools()).and_then(|a| a.label.as_deref()),
            Some("job")
        );
    }

    /// With nobody enrolled signed in to any tool the bar says what is signed in, as it would
    /// with one tool, rather than nothing. With nobody signed in at all it is about nobody.
    ///
    /// PresentationTests.swift's withNobodyEnrolledSignedInTheBarIsAboutWhoeverIsSignedIn.
    #[test]
    fn with_nobody_enrolled_signed_in_the_bar_is_about_whoever_is_signed_in() {
        let login_only = [
            account(Some("work")).build(),
            account(None)
                .of("codex")
                .signed_in()
                .uuid("c")
                .limits(vec![window("five_hour", 3.0)])
                .build(),
        ];
        assert_eq!(
            titled(&login_only, &both_tools()).map(|a| a.id.as_str()),
            Some("codex:c")
        );
        assert_eq!(bar_title(&login_only, &both_tools(), false), "unnamed 3%");
        assert_eq!(
            titled(
                &[
                    account(Some("work")).build(),
                    account(Some("job")).of("codex").build()
                ],
                &both_tools()
            ),
            None
        );
        assert_eq!(titled(&[], &both_tools()), None);
    }

    /// Sections come in the order the tools are listed, each once, whatever order the rows
    /// came in, and a code the listing does not know follows in the order it first appeared.
    ///
    /// PresentationTests.swift's toolCodesFollowTheListingEachOnce.
    #[test]
    fn tool_codes_follow_the_listing_each_once() {
        assert_eq!(
            in_order(&["x", "codex", "claude", "codex", "y", "x"], &both_tools()),
            ["claude", "codex", "x", "y"]
        );
        assert_eq!(
            in_order(&["codex"], &both_tools()),
            ["codex"],
            "a listed tool with no rows is left out"
        );
        assert_eq!(in_order(&["b", "a", "b"], &[]), ["b", "a"]);
        assert!(in_order(&[], &both_tools()).is_empty());
    }

    /// A limit's row says what the column beside its bar says and what VoiceOver hears, from
    /// its record and the moment it is shown: names, figure, step, pace and reset.
    #[test]
    fn a_limits_row_says_its_names_figure_step_pace_and_reset() {
        let now = 1_800_000_000;
        let row = limit_row(
            &window("weekly_scoped", 72.4)
                .scope("Fable")
                .length(604_800)
                .resets(Some(now + 3 * 3600 + 5 * 60)),
            now,
            now,
        );
        assert_eq!(row.name, "weekly Fable");
        assert_eq!(row.short, "week · Fable");
        assert_eq!(row.figure, "72%");
        assert_eq!(row.level, crate::UsageLevel::Low);
        assert_eq!(row.resets, "resets in 3h 05m");
        assert_eq!(
            row.pace.as_ref().map(|pace| pace.said.as_str()),
            Some("26% under pace")
        );
        assert_eq!(
            row.spoken,
            "weekly Fable limit, 72 percent used, 26 percent under an even pace, resets in 3 \
             hours, 5 minutes"
        );
        let unknown = limit_row(&window("session", 42.0).resets(None), now, now);
        assert_eq!(unknown.resets, "", "nothing where no reset is known");
        assert_eq!(unknown.spoken, "5-hour limit, 42 percent used");
        let due = limit_row(&window("session", 100.0).resets(Some(now)), now, now);
        assert_eq!(due.resets, "resetting now");
        assert_eq!(due.level, crate::UsageLevel::Out);
        assert!(due.spoken.ends_with(", resetting now"), "{}", due.spoken);
    }

    /// The column beside a bar says when a limit resets as `pitboard status` does, minutes in
    /// two digits, and that it is resetting once that moment has come. A limit with no reset
    /// known has nothing there. Every span it can say is the core's to test.
    ///
    /// PresentationTests.swift's aResetIsSaidAsTheCommandLineSaysIt.
    #[test]
    fn a_reset_is_said_as_the_command_line_says_it() {
        let noon = 1_768_392_000;
        let resets =
            |at: Option<i64>| limit_row(&window("session", 42.0).resets(at), noon, noon).resets;
        assert_eq!(resets(Some(noon + 3600 + 5 * 60)), "resets in 1h 05m");
        assert_eq!(
            resets(Some(noon + 2 * 86_400 + 4 * 3600)),
            "resets in 2d 4h"
        );
        assert_eq!(resets(Some(noon)), "resetting now");
        assert_eq!(resets(Some(noon - 60)), "resetting now");
        assert_eq!(resets(None), "");
    }
}
