//! The accounts as the menu bar, the menu and the window describe them, from Accounts.swift:
//! which account the bar is about and what it says, the sections per tool, and each account's
//! row, with its limits.

use super::words;
use super::{
    AccountAction, AccountDetail, AccountItem, AccountMenuShown, AccountNote, AccountSection,
    AccountStanding, ActionKind, Choice, ItemAction, ItemOffer, LimitKind, LimitRow, MenuBarText,
    MenuNotices, NoteKind, Question, RowEnd, Seen, WarmDetail, WindowOffer,
};
use crate::account_windows::{forget_message_on, windows_of_account};
use crate::model::advice::window;
use crate::model::{Intent, Place, SettingsSection, Sheet};
use crate::{Account, Limit, Tool};
use pitboard_core::duty::warm;
use pitboard_core::host::Os;
use pitboard_core::status::{self, Stale};
use pitboard_core::time::{HOUR, WEEK};
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
        .map(|limit| said::figure(limit.percent));
    if usage_only {
        return percent.unwrap_or_default();
    }
    let name = words::bar_name(account.label.as_deref().unwrap_or(words::UNNAMED));
    match percent {
        Some(percent) => words::in_bar(&name, &percent),
        None => name,
    }
}

pub(crate) fn menu_bar(seen: &Seen, notices: &MenuNotices) -> MenuBarText {
    let accounts = seen.accounts();
    let name_and_usage = bar_title(accounts, &seen.tools, false);
    let titled = titled(accounts, &seen.tools);
    let attention = notices.install.is_some()
        || !notices.switches.is_empty()
        || !notices.sign_ins.is_empty()
        || notices.others.is_some();
    // The bar follows the account closest to running out, and two tools can each have a `work`.
    let tool = titled
        .filter(|_| seen.shows_tools())
        .and_then(|account| seen.tool(&account.provider));
    let spoken = words::bar_spoken(
        (!name_and_usage.is_empty()).then_some(name_and_usage.as_str()),
        tool.map(|tool| tool.name.as_str()),
        attention,
    );
    let tip = words::bar_tip(
        attention,
        accounts
            .iter()
            .filter(|account| account.signed_in)
            .map(|account| (seen.name_of(account), summary(seen, account))),
    );
    MenuBarText {
        usage: bar_title(accounts, &seen.tools, true),
        name_and_usage,
        spoken,
        level: titled
            .and_then(|account| headline(account.usage.as_ref().map_or(&[], |u| &u.windows)))
            .map(|limit| crate::usage_level(limit.percent)),
        attention,
        tip,
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
    let needs_sign_in = needs_sign_in(account);
    let switching = account.qualified.is_some() && account.qualified.as_deref() == switching_for;
    let title = if account.unplaced {
        // Any other was read, and not one Anthropic or OpenAI named, whether or not it was asked.
        words::nameless_login(matches!(
            account.stale,
            Some(Stale::LoginUnreadable | Stale::LoginUnusable)
        ))
        .to_owned()
    } else {
        account
            .label
            .clone()
            .unwrap_or_else(|| account.email.clone())
    };
    let spoken_name = seen.name_of(account);
    let action = action(
        seen.os,
        account,
        switching_for.is_some(),
        busy,
        &spoken_name,
    );
    let problem = if account.unplaced || (!account.standing.switchable() && !account.signed_in) {
        account.stale_explanation.clone()
    } else {
        None
    };
    // No usage is read for an app's login: the menu's line says so once.
    let in_its_app = crate::model::state::in_its_app(&account.provider);
    let read_ago = account
        .read_at
        .filter(|_| in_its_app)
        .map(|read_at| words::read_ago(seen.now - read_at, seen.program_of(&account.provider)));
    let stale = if problem.is_some() || account.stale == Some(Stale::NoUsage) {
        None
    } else {
        account.stale_explanation.clone()
    };
    let stale_note = read_ago
        .clone()
        .filter(|_| account.signed_in)
        .or_else(|| stale.clone());
    let parked = account.parked.as_ref().filter(|_| !account.signed_in);
    // Without a refresh token's date, a Claude Desktop session's, which may lapse sooner.
    let (until, lapses) = match parked.map(|p| (p.refresh_expires_at, p.session_expires_at)) {
        Some((Some(at), _)) => (Some(at), false),
        Some((None, at)) => (at, true),
        None => (None, false),
    };
    let life = until.map(|at| words::saved_until(&seen.date_of(at), lapses));
    let expiring = until
        .filter(|_| account.standing == status::Standing::Expiring)
        .map(|at| words::sign_in_expires(at - seen.now, lapses));
    let sign_in_standing = needs_sign_in.then(|| words::sign_in_standing(account.standing));
    let in_place_of_limits = in_place_of_limits(seen, account);
    let figure = headline(account.usage.as_ref().map_or(&[], |usage| &usage.windows))
        .filter(|_| in_place_of_limits.is_none())
        .map(|limit| said::figure(limit.percent));
    let unnamed = account.label.is_none() && account.signed_in && !account.unplaced;
    let why_not = if switching {
        Some(words::SWITCHING_NOW)
    } else if sign_in_standing.is_some() {
        sign_in_standing
    } else if unnamed {
        Some(words::NOT_ADDED_YET)
    } else if account.standing == status::Standing::LoginUnread && !account.unplaced {
        Some(words::CANNOT_READ_OR_SWITCH)
    } else {
        None
    };
    let menu_title = words::named_in_menu(seen.os, &title, why_not.or(figure.as_deref()));
    let spoken = words::row_spoken(
        &spoken_name,
        account.plan.as_deref(),
        in_use,
        sign_in_standing,
    );
    let end = if switching {
        Some(RowEnd::Switching {
            spoken: words::SWITCHING.into(),
        })
    } else if in_use && action.is_none() {
        Some(RowEnd::InUse {
            title: words::in_use(seen.os),
        })
    } else {
        // The login in use has an action only while it has no name: Add.
        action.clone().map(|action| RowEnd::Button { action })
    };
    let warm_note = warm_note(seen, account);
    // A row says a saved sign-in has expired, as it says one is about to.
    let ended = expiring.clone().or_else(|| {
        until
            .filter(|_| account.standing == status::Standing::LoginExpired)
            .map(|at| words::sign_in_expires(at - seen.now, lapses))
    });
    let lines = [&problem, &stale_note, &ended, &warm_note]
        .into_iter()
        .flatten()
        .cloned()
        .collect();
    let actions = actions(seen, account, action.as_ref(), busy, &spoken_name);
    let acting = |kind| actions.iter().find(|action| action.kind == kind).cloned();
    // A note nothing in the app does anything about says where the docs explain it.
    let note = |kind, text, action: Option<AccountAction>| AccountNote {
        kind,
        text,
        learn: action.is_none().then(|| super::next::of_stale(seen.os)),
        action,
    };
    let problem_note = if needs_sign_in {
        let why = (problem.clone()).unwrap_or_else(|| sign_in_standing.unwrap_or_default().into());
        Some(note(
            NoteKind::Problem,
            why,
            acting(ActionKind::SignInAgain),
        ))
    } else if account.unplaced || account.standing == status::Standing::LoginUnread {
        let why = (account.stale_explanation.clone())
            .unwrap_or_else(|| words::CANNOT_READ_OR_SWITCH.into());
        Some(note(NoteKind::Problem, why, None))
    } else {
        problem
            .clone()
            .map(|why| note(NoteKind::Problem, why, None))
    };
    let notes: Vec<AccountNote> = [
        problem_note,
        unnamed.then(|| {
            let add = acting(ActionKind::Add);
            note(NoteKind::Unfinished, words::NOT_ADDED_YET.into(), add)
        }),
        expiring.map(|life| note(NoteKind::Expiring, life, acting(ActionKind::SignInAgain))),
        stale.map(|why| note(NoteKind::Stale, why, None)),
    ]
    .into_iter()
    .flatten()
    .collect();
    let read_at = account
        .read_at
        .or_else(|| account.usage.as_ref().and_then(|usage| usage.observed_at));
    let detail = AccountDetail {
        email: Some(account.email.clone()).filter(|email| !email.is_empty()),
        plan: account.plan.clone(),
        signed_in_until: parked.and_then(|p| p.refresh_expires_at.or(p.session_expires_at)),
        sign_in: if unnamed {
            Some(words::NOT_ADDED_YET.into())
        } else if in_use && account.label.is_some() {
            Some(words::IN_USE.into())
        } else if let Some(standing) = sign_in_standing {
            Some(standing.into())
        } else {
            life
        },
        warm: warm_choice(seen, account).map(|choice| warm_detail(seen.os, choice, warm_note)),
        read_at,
        read: read_ago,
        notes: (notes.iter())
            .filter(|note| match note.kind {
                NoteKind::Stale => true,
                NoteKind::Problem => !needs_sign_in,
                NoteKind::Expiring | NoteKind::Unfinished => false,
            })
            .cloned()
            .collect(),
    };
    AccountItem {
        id: account.id.clone(),
        qualified: account.qualified.clone(),
        summary: summary(seen, account),
        email_line: (!account.email.is_empty() && account.email != title)
            .then(|| account.email.clone()),
        title,
        email: account.email.clone(),
        plan: account.plan.clone(),
        menu_title,
        spoken,
        in_use,
        needs_sign_in,
        standing: standing_of(account, switching),
        unplaced: account.unplaced,
        end,
        offers: actions
            .iter()
            .filter(|action| !matches!(action.kind, ActionKind::OpenWindow | ActionKind::Remove))
            .filter_map(offer_of)
            .collect(),
        windows: actions
            .iter()
            .filter_map(|action| {
                Some(WindowOffer {
                    title: action.title.clone(),
                    window: action.window.clone()?,
                })
            })
            .collect(),
        forget: actions
            .iter()
            .find(|action| action.kind == ActionKind::Remove)
            .and_then(offer_of),
        actions,
        action,
        help: problem.or(stale_note),
        notes,
        lines,
        detail,
        limits: account
            .usage
            .as_ref()
            .map(|usage| usage.windows.iter().map(|l| limit_row(seen, l)).collect())
            .unwrap_or_default(),
    }
}

pub(crate) fn standing_of(account: &Account, switching: bool) -> AccountStanding {
    use status::Standing;
    match account.standing {
        _ if switching => AccountStanding::Switching,
        _ if account.unplaced => AccountStanding::Unusable,
        Standing::Unplaced | Standing::LoginUnread => AccountStanding::Unusable,
        _ if account.label.is_none() && account.signed_in => AccountStanding::NotAdded,
        _ if account.signed_in => AccountStanding::InUse,
        Standing::Expiring => AccountStanding::Expiring,
        Standing::Ready => AccountStanding::Ready,
        Standing::InUse => AccountStanding::InUse,
        Standing::NotEnrolled => AccountStanding::NotAdded,
        Standing::LoginReplaced
        | Standing::NothingParked
        | Standing::LoginRefused
        | Standing::LoginExpired => AccountStanding::NeedsSignIn,
    }
}

pub(crate) fn offer_of(action: &AccountAction) -> Option<ItemOffer> {
    Some(ItemOffer {
        title: action.title.clone(),
        intent: action.intent.clone()?,
        enabled: action.enabled,
        confirm: action.confirm.clone(),
    })
}

// In its menu's order: adding it or switching, signing in again and renaming; then the rest.
fn actions(
    seen: &Seen,
    account: &Account,
    action: Option<&ItemAction>,
    busy: bool,
    spoken_name: &str,
) -> Vec<AccountAction> {
    let does = |kind: ActionKind, title: String, intent: Intent, enabled: bool| AccountAction {
        kind,
        title,
        intent: Some(intent),
        window: None,
        enabled,
        confirm: None,
    };
    let add = action.and_then(|action| match &action.intent {
        Intent::Enrol { .. } => Some(does(
            ActionKind::Add,
            action.title.clone(),
            action.intent.clone(),
            true,
        )),
        _ => None,
    });
    // Signing in again is offered here for every named account, so never as its own action too.
    let switches = action.is_some_and(|action| matches!(action.intent, Intent::SwitchTo { .. }));
    let named = account
        .label
        .as_deref()
        .zip(account.qualified.as_ref())
        .filter(|_| !account.unplaced)
        .map(|(label, qualified)| {
            let provider = account.provider.clone();
            let label = label.to_owned();
            [
                does(
                    ActionKind::Switch,
                    words::switch_account(Some(&label)),
                    Intent::SwitchTo {
                        qualified: qualified.clone(),
                    },
                    switches,
                ),
                does(
                    ActionKind::SignInAgain,
                    words::sign_in_again(seen.os, true),
                    Intent::PresentSheet {
                        sheet: Sheet::SignInAgain {
                            provider: provider.clone(),
                            label: label.clone(),
                        },
                    },
                    !busy,
                ),
                does(
                    ActionKind::Rename,
                    words::rename(true).into(),
                    Intent::PresentSheet {
                        sheet: Sheet::Rename { provider, label },
                    },
                    true,
                ),
            ]
        });
    let windows = windows_of_account(account, seen.accounts())
        .into_iter()
        .map(|window| AccountAction {
            kind: ActionKind::OpenWindow,
            title: words::open_site(&window.site.name),
            intent: None,
            window: Some(window),
            enabled: true,
            confirm: None,
        });
    add.into_iter()
        .chain(named.into_iter().flatten())
        .chain(warm_up_now(seen, account))
        .chain(windows)
        .chain(forget(seen, account, spoken_name))
        .collect()
}

/// What warming up an account is, as the core's last word on it says, where it can be warmed
/// up: a Claude Code account enrolled under a name.
fn warm_choice<'a>(seen: &'a Seen, account: &Account) -> Option<&'a warm::Choice> {
    let qualified = account.qualified.as_deref()?;
    seen.state
        .warm
        .overview
        .as_ref()?
        .accounts
        .iter()
        .find(|choice| choice.qualified == qualified)
}

// Turning warm-ups on and off is Settings' alone, where the hours and every account show.
fn warm_up_now(seen: &Seen, account: &Account) -> Option<AccountAction> {
    let choice = warm_choice(seen, account)?;
    let warm = &seen.state.warm;
    let halted = warm
        .overview
        .as_ref()
        .is_some_and(|overview| overview.halted.is_some());
    Some(AccountAction {
        kind: ActionKind::WarmUpNow,
        title: words::warm_up_now(seen.os),
        intent: Some(Intent::WarmNow {
            qualified: choice.qualified.clone(),
        }),
        window: None,
        enabled: !halted && warm.running.is_none() && choice.now == warm::Standing::Due,
        confirm: None,
    })
}

/// Where an account turned on for warm-ups stands, as the line under it says: "Warms up again
/// at 22:50", or that one is under way.
fn warm_note(seen: &Seen, account: &Account) -> Option<String> {
    let choice = warm_choice(seen, account)?;
    let warming = match seen.state.warm.running.as_ref() {
        Some(Some(asked)) => *asked == choice.qualified,
        Some(None) => choice.standing == Some(warm::Standing::Due),
        None => false,
    };
    if warming {
        return Some(words::WARMING_UP.into());
    }
    let standing = choice.standing.as_ref()?;
    let clock = |at| seen.clock(at);
    Some(words::typeset(&said::capitalised(&words::warm_standing(
        standing, &clock,
    ))))
}

// `note` is what its line says, where it says anything.
fn warm_detail(os: Os, choice: &warm::Choice, note: Option<String>) -> WarmDetail {
    let (next_at, running_until) = match &choice.standing {
        Some(warm::Standing::Running { until, next }) => (*next, Some(*until)),
        Some(warm::Standing::Resting { at }) => (Some(*at), None),
        Some(warm::Standing::Waiting { until }) => (Some(*until), None),
        Some(warm::Standing::Held(hold)) => (hold.until(), None),
        Some(warm::Standing::Due) | None => (None, None),
    };
    WarmDetail {
        on: choice.on,
        said: note.unwrap_or_else(|| words::WARM_UP_OFF.into()),
        next_at,
        running_until,
        settings: Choice {
            title: words::warm_up_settings(os),
            intent: Intent::Show {
                place: Some(Place::Settings {
                    section: SettingsSection::WarmUp,
                }),
            },
            enabled: true,
        },
    }
}

// The question names it, and says what forgetting it deletes in the account windows' words.
fn forget(seen: &Seen, account: &Account, spoken_name: &str) -> Option<AccountAction> {
    let qualified = account
        .qualified
        .as_ref()
        .filter(|_| account.label.is_some())?;
    Some(AccountAction {
        kind: ActionKind::Remove,
        title: words::remove_account(seen.os),
        intent: Some(Intent::Forget {
            qualified: qualified.clone(),
        }),
        window: None,
        // The core refuses it.
        enabled: !account.signed_in,
        confirm: Some(Question {
            title: words::remove_title(spoken_name),
            message: forget_message_on(seen.os, account, seen.accounts()),
            confirm: words::REMOVE.into(),
            destructive: true,
            confirm_is_default: true,
        }),
    })
}

pub(crate) fn account_menu(os: Os) -> AccountMenuShown {
    AccountMenuShown {
        offers: vec![
            words::switch_account(None),
            words::sign_in_again(os, true),
            words::rename(true).into(),
        ],
        forget: words::remove_account(os),
    }
}

/// What pressing an account does, decided once so the menu and the window cannot disagree.
///
/// A switch under way holds everything back, since each change waits for the one before. A
/// sign-in under way holds back only another sign-in: it waits on a person in a browser, and
/// switching meanwhile is theirs to do.
fn action(
    os: Os,
    account: &Account,
    switching: bool,
    busy: bool,
    spoken: &str,
) -> Option<ItemAction> {
    if account.unplaced || switching {
        return None;
    }
    let Some(label) = account.label.as_deref() else {
        return account.signed_in.then(|| ItemAction {
            title: words::add_account_signed_in(None),
            spoken: words::add_account_signed_in(Some(spoken)),
            intent: Intent::Enrol {
                provider: account.provider.clone(),
            },
        });
    };
    if account.signed_in {
        return None;
    }
    if account.standing.switchable()
        && let Some(qualified) = &account.qualified
    {
        return Some(ItemAction {
            title: words::switch_account(None),
            spoken: words::switch_account(Some(spoken)),
            intent: Intent::SwitchTo {
                qualified: qualified.clone(),
            },
        });
    }
    (!busy).then(|| ItemAction {
        title: words::sign_in_asking(os),
        spoken: words::sign_in_to_again(os, spoken, true),
        intent: Intent::PresentSheet {
            sheet: Sheet::SignInAgain {
                provider: account.provider.clone(),
                label: label.to_owned(),
            },
        },
    })
}

pub(crate) fn needs_sign_in(account: &Account) -> bool {
    account.label.is_some() && account.standing.needs_sign_in()
}

// The line under an account in the menu: its limits, or what stands in their way.
fn summary(seen: &Seen, account: &Account) -> String {
    in_place_of_limits(seen, account).unwrap_or_else(|| limits_said(seen, account))
}

fn in_place_of_limits(seen: &Seen, account: &Account) -> Option<String> {
    if account.qualified.is_some() && account.qualified.as_deref() == seen.state.switch_under_way()
    {
        return Some(words::SWITCHING_NOW.into());
    }
    if account.unplaced || account.standing == status::Standing::LoginUnread {
        return Some(
            account
                .stale_explanation
                .clone()
                .unwrap_or_else(|| words::CANNOT_READ_OR_SWITCH.into()),
        );
    }
    if account.label.is_none() {
        return Some(words::NOT_ADDED_YET.into());
    }
    if needs_sign_in(account) {
        return Some(words::sign_in_standing(account.standing).into());
    }
    let windows = account.usage.as_ref().map_or(&[][..], |u| &u.windows);
    if windows.is_empty() && account.stale == Some(Stale::NoUsage) {
        return Some(words::NO_USAGE_READ.into());
    }
    windows.is_empty().then(|| account.email.clone())
}

pub(crate) fn limits_said(seen: &Seen, account: &Account) -> String {
    let windows = account.usage.as_ref().map_or(&[][..], |u| &u.windows);
    // Nothing else of the account matters until the last limit it reached resets.
    let reached = (windows.iter())
        .filter(|limit| window(limit).used_up(seen.now))
        .max_by_key(|limit| limit.resets_at.unwrap_or(i64::MAX));
    if let Some(limit) = reached {
        let title = words::limit_title(&said::limit_name(&window(limit)), limit.scope.as_deref());
        let clock = limit.resets_at.map(|at| seen.clock(at));
        return words::limit_reached(&title, clock.as_deref());
    }
    let limits: Vec<(String, String)> = windows
        .iter()
        .map(|limit| {
            let name = said::limit_name(&window(limit));
            let short = words::limit_short(&name, limit.scope.as_deref());
            (short, said::figure(limit.percent))
        })
        .collect();
    words::limits_line(seen.os, &limits)
}

fn kind_of(limit: &Limit) -> LimitKind {
    const FIVE_HOURS: i64 = 5 * HOUR;
    match (window(limit).length(), &limit.scope) {
        (Some(FIVE_HOURS), None) => LimitKind::FiveHour,
        (Some(WEEK), None) => LimitKind::Weekly,
        (Some(WEEK), Some(model)) => LimitKind::WeeklyModel {
            model: model.clone(),
        },
        (length_seconds, _) => LimitKind::Other { length_seconds },
    }
}

pub(crate) fn limit_row(seen: &Seen, limit: &Limit) -> LimitRow {
    let name = said::limit_name(&window(limit));
    let model = limit.scope.as_deref();
    let title = words::limit_title(&name, model);
    let figure = said::figure(limit.percent);
    let resets = match limit.resets_at {
        Some(at) if at <= seen.now => words::RESETTING_NOW.into(),
        Some(at) => words::resets(&seen.clock(at)),
        None => String::new(),
    };
    LimitRow {
        kind: kind_of(limit),
        short: words::limit_short(&name, model),
        spoken: words::spoken_limit(&title, limit.percent, &resets),
        name: title,
        percent: limit.percent,
        used: words::used(&figure),
        figure,
        level: crate::usage_level(limit.percent),
        resets_at: limit.resets_at,
        resets_in: (limit.resets_at)
            .filter(|at| *at > seen.now)
            .map(|at| words::resets_in(at - seen.now))
            .unwrap_or_default(),
        resets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::present::testing::{LimitExt, account, both_tools, claude_code, codex, window};

    fn title(accounts: &[Account]) -> String {
        bar_title(accounts, &[], false)
    }

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
        assert_eq!(title(&read), words::in_bar("work", "64%"));
    }

    /// macOS hides the widest menu bar items first when there is no room, and a notched
    /// display has little. The settings can drop the name, and then the figure too, leaving
    /// the mark.
    ///
    /// The mark alone is the app's setting, which shows neither.
    #[test]
    fn the_menu_bar_shows_no_more_than_the_settings_ask_for() {
        let read = [account(Some("work"))
            .signed_in()
            .limits(vec![window("session", 12.0), window("weekly_all", 64.4)])
            .build()];
        assert_eq!(bar_title(&read, &[], false), words::in_bar("work", "64%"));
        assert_eq!(bar_title(&read, &[], true), "64%");
        assert_eq!(
            bar_title(&[account(Some("work")).signed_in().build()], &[], true),
            "",
            "no figure to show until one is measured"
        );
    }

    /// Before the first read there is nothing true to say, and the icon is already there, so
    /// the item shows the mark alone rather than a name it has not checked.
    #[test]
    fn the_menu_bar_says_nothing_before_the_first_read() {
        assert_eq!(title(&[]), "");
        assert_eq!(bar_title(&[], &both_tools(), true), "");
    }

    /// The bar is shared with everything else running, so a name longer than twelve
    /// characters is cut to eleven and an ellipsis, and a name that fits is left whole.
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
        assert_eq!(
            bar("abcdefghijkl", false),
            words::in_bar("abcdefghijkl", "5%")
        );
        assert_eq!(
            bar("abcdefghijklm", false),
            words::in_bar("abcdefghijk…", "5%")
        );
        assert_eq!(
            bar("a-very-long-account-label", false),
            words::in_bar("a-very-long…", "5%")
        );
        assert!(bar("a-very-long-account-label", false).chars().count() <= 17);
        assert_eq!(bar("a-very-long-account-label", true), "5%");
    }

    /// Before anything is measured the bar still says whose account is in use, and a setting
    /// that asks for the figure alone shows the mark until there is one.
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
            words::UNNAMED
        );
        assert_eq!(bar_title(&unmeasured, &[], true), "");
    }

    /// With nobody signed in there is no account in use to name, and naming one that is not
    /// would be wrong.
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
    #[test]
    fn a_scoped_limit_does_not_take_the_menu_bar() {
        let read = [account(Some("work"))
            .signed_in()
            .limits(vec![
                window("session", 30.0),
                window("weekly_scoped", 98.0).scope("Fable"),
            ])
            .build()];
        assert_eq!(title(&read), words::in_bar("work", "30%"));
    }

    /// Among the account's own limits, the one it is working against wins, and failing that
    /// the fullest; two as full give the first.
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
        assert_eq!(
            bar_title(&read, &both_tools(), false),
            words::in_bar("job", "71%")
        );
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
        assert_eq!(
            bar_title(&scoped, &both_tools(), false),
            words::in_bar("job", "50%")
        );
    }

    /// A tie goes to the tool the listing puts first, whatever order the rows came in, so the
    /// bar does not move between two accounts at the same figure from one read to the next,
    /// and a login nobody has named does not take the bar from one that has a name.
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
        assert_eq!(
            bar_title(&tied, &both_tools(), false),
            words::in_bar("work", "50%")
        );

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
        assert_eq!(
            bar_title(&unnamed, &both_tools(), false),
            words::in_bar("personal", "10%")
        );
    }

    /// An account with nothing measured is not closer to running out than one at nought.
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
        assert_eq!(
            bar_title(&login_only, &both_tools(), false),
            words::in_bar(words::UNNAMED, "3%")
        );
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

    fn bar(accounts: Vec<Account>, warnings: Vec<crate::Warning>) -> MenuBarText {
        let state = read(status(accounts, warnings), both_tools());
        present_on(Os::Windows, &state, 0, &Utc).menu_bar
    }

    /// The step of the limit the bar names, which an icon can show where the bar has no
    /// words: none before anything is measured, and none with nobody signed in.
    #[test]
    fn the_menu_bar_says_the_step_of_the_limit_it_names() {
        let level = |percent: f64| {
            bar(
                vec![
                    account(Some("work"))
                        .signed_in()
                        .limits(vec![window("session", percent)])
                        .build(),
                ],
                Vec::new(),
            )
            .level
        };
        assert_eq!(level(12.0), Some(crate::UsageLevel::Plenty));
        assert_eq!(level(75.0), Some(crate::UsageLevel::Low));
        assert_eq!(level(96.0), Some(crate::UsageLevel::Out));
        assert_eq!(
            bar(vec![account(Some("work")).signed_in().build()], Vec::new()).level,
            None
        );
        assert_eq!(
            bar(
                vec![
                    account(Some("work"))
                        .limits(vec![window("session", 96.0)])
                        .build()
                ],
                Vec::new()
            )
            .level,
            None
        );
    }

    /// The bar is flagged while the menu opens on something to look at, and says so aloud.
    #[test]
    fn the_menu_bar_is_flagged_while_the_menu_has_something_to_look_at() {
        let work = || {
            account(Some("work"))
                .signed_in()
                .limits(vec![window("session", 42.0)])
                .build()
        };
        let calm = bar(vec![work()], Vec::new());
        let work_42 = words::in_bar("work", "42%");
        assert!(!calm.attention);
        assert_eq!(calm.spoken, words::bar_spoken(Some(&work_42), None, false));
        let warned = bar(
            vec![work()],
            vec![crate::Warning {
                code: "keychain_locked".into(),
                message: "The keychain is locked.".into(),
                account: None,
                held: None,
                provider: None,
            }],
        );
        assert!(warned.attention);
        assert_eq!(warned.spoken, words::bar_spoken(Some(&work_42), None, true));

        let state = read(status(Vec::new(), Vec::new()), Vec::new());
        let bare = present_on(Os::Windows, &state, 0, &Utc).menu_bar;
        assert!(bare.attention, "no tool here");
        assert_eq!(bare.spoken, words::bar_spoken(None, None, true));
    }

    /// Windows' notification area has a tooltip and no title: it lists each account in use,
    /// with its tool beside another tool's, and its limits as the menu says them, after whether
    /// the menu needs attention, which Narrator reads with the icon's name.
    #[test]
    fn the_notification_areas_tip_lists_each_account_in_use() {
        let (work, job) = (
            account(Some("work"))
                .signed_in()
                .limits(vec![window("session", 12.0), window("weekly_all", 64.4)])
                .build(),
            account(Some("job"))
                .of("codex")
                .signed_in()
                .limits(vec![window("five_hour", 71.0).length(18_000)])
                .build(),
        );
        let state = read(
            status(
                vec![
                    work,
                    account(Some("spare"))
                        .limits(vec![window("session", 3.0)])
                        .build(),
                    job,
                ],
                Vec::new(),
            ),
            both_tools(),
        );
        let shown = present_on(Os::Windows, &state, 0, &Utc);
        let line = |section: usize| shown.sections[section].accounts[0].summary.clone();
        assert_eq!(
            shown.menu_bar.tip,
            words::bar_tip(
                false,
                [
                    (words::named_with_tool("work", "Claude Code"), line(0)),
                    (words::named_with_tool("job", "Codex"), line(1))
                ]
            ),
            "each account in use, with the menu's line about it"
        );
        let work = || {
            account(Some("work"))
                .signed_in()
                .limits(vec![window("session", 12.0)])
                .build()
        };
        let one = bar(vec![work()], Vec::new());
        let work_line = || {
            words::limits_line(
                Os::Windows,
                &[(words::limit_short("5-hour", None), "12%".into())],
            )
        };
        assert_eq!(
            one.tip,
            words::bar_tip(false, [("work".into(), work_line())])
        );
        assert_eq!(bar(Vec::new(), Vec::new()).tip, words::bar_tip(false, []));
        let warned = bar(
            vec![work()],
            vec![crate::Warning {
                code: "keychain_locked".into(),
                message: "The keychain is locked.".into(),
                account: None,
                held: None,
                provider: None,
            }],
        );
        assert_eq!(
            warned.tip,
            words::bar_tip(true, [("work".into(), work_line())])
        );
        let state = read(status(Vec::new(), Vec::new()), Vec::new());
        let bare = present_on(Os::Windows, &state, 0, &Utc).menu_bar;
        assert_eq!(bare.tip, words::bar_tip(true, []), "no tool here");
    }

    /// Sections come in the order the tools are listed, each once, whatever order the rows
    /// came in, and a code the listing does not know follows in the order it first appeared.
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

    #[test]
    fn a_limits_row_says_its_names_figure_step_and_reset() {
        use crate::model::state::{Cadence, State};
        let now = 1_800_000_000;
        let state = State::new(Cadence::APP);
        let seen = Seen::new(&state, now, &Utc, Os::MacOs);
        let limit = window("weekly_scoped", 72.4)
            .scope("Fable")
            .length(604_800)
            .resets(Some(now + 3 * 3600 + 5 * 60));
        let row = limit_row(&seen, &limit);
        assert_eq!(
            (row.short.as_str(), row.name.as_str()),
            (
                words::limit_short("weekly", Some("Fable")).as_str(),
                words::limit_title("weekly", Some("Fable")).as_str()
            )
        );
        assert_eq!(
            (row.figure.as_str(), row.used.as_str()),
            ("72%", words::used("72%").as_str())
        );
        assert_eq!(row.level, crate::UsageLevel::Low);
        assert_eq!(row.resets, words::resets("11:05"));
        assert_eq!(
            row.spoken,
            words::spoken_limit(&row.name, 72.4, &row.resets)
        );
        let due = limit_row(&seen, &window("session", 100.0).resets(Some(now)));
        assert_eq!(due.resets, words::RESETTING_NOW);
        assert_eq!(due.level, crate::UsageLevel::Out);
    }

    use crate::present::present_on;
    use crate::present::testing::{Utc, claude_desktop, desktop_account, read, status};
    use pitboard_core::host::Os;

    const NOW: i64 = 1_800_000_000;

    fn desktop_rows(accounts: Vec<Account>) -> Vec<AccountItem> {
        let state = read(status(accounts, Vec::new()), vec![claude_desktop()]);
        present_on(Os::MacOs, &state, NOW, &Utc)
            .sections
            .into_iter()
            .flat_map(|section| section.accounts)
            .collect()
    }

    #[test]
    fn a_claude_desktop_row_says_no_usage_once_and_when_its_sign_in_may_lapse() {
        let home = Account {
            parked: Some(crate::Parked {
                refresh_expires_at: None,
                session_expires_at: Some(NOW + 25 * 86_400 + 3_600),
            }),
            ..desktop_account("home", false)
        };
        let rows = desktop_rows(vec![desktop_account("work", true), home]);
        assert_eq!(rows[0].summary, words::NO_USAGE_READ);
        assert_eq!(rows[1].summary, words::NO_USAGE_READ);
        assert!(
            rows[1].lines.is_empty(),
            "no usage said once, in the menu's line, and its life in the detail"
        );
        let lapsed = Account {
            parked: Some(crate::Parked {
                refresh_expires_at: None,
                session_expires_at: Some(NOW - 1),
            }),
            standing: status::Standing::Expiring,
            ..desktop_account("home", false)
        };
        assert_eq!(
            desktop_rows(vec![lapsed])[0].lines,
            [words::sign_in_expires(-1, true)],
            "as `pitboard status` hints a sign-in for it"
        );
    }

    // As `pitboard status` hints a sign-in for it; it still switches until it expires.
    #[test]
    fn a_row_says_to_sign_in_again_to_a_parked_login_due_to_expire() {
        let row = |account: Account| {
            let state = read(status(vec![account], Vec::new()), vec![claude_code()]);
            present_on(Os::MacOs, &state, NOW, &Utc)
                .sections
                .into_iter()
                .flat_map(|section| section.accounts)
                .next()
                .expect("a row")
        };
        let due = NOW + 2 * 86_400;
        let expiring = row(account(Some("work"))
            .parked_until(due)
            .standing(status::Standing::Expiring)
            .build());
        assert_eq!(expiring.lines, [words::sign_in_expires(2 * 86_400, false)]);
        assert!(!expiring.needs_sign_in);
        assert!(matches!(
            expiring.action.map(|action| action.intent),
            Some(Intent::SwitchTo { .. })
        ));

        let lasting = NOW + 20 * 86_400;
        let ready = row(account(Some("work")).parked_until(lasting).build());
        assert!(ready.lines.is_empty(), "nothing until three days are left");
    }

    fn menu_titles(os: Os, accounts: Vec<Account>) -> Vec<String> {
        let state = read(status(accounts, Vec::new()), vec![claude_code()]);
        present_on(os, &state, NOW, &Utc)
            .sections
            .into_iter()
            .flat_map(|section| section.accounts)
            .map(|item| item.menu_title)
            .collect()
    }

    // Plan 4.8: a Windows menu item has one line, where macOS puts the rest on a second.
    #[test]
    fn a_windows_menu_item_says_its_figure_or_why_it_cannot_be_switched_to() {
        let mut work = account(Some("work"))
            .signed_in()
            .limits(vec![window("session", 12.0), window("weekly_all", 64.4)])
            .build();
        work.plan = Some("Team 5x".into());
        let accounts = vec![
            work,
            account(Some("personal"))
                .limits(vec![window("session", 100.0)])
                .build(),
            account(Some("spare")).build(),
            account(Some("old"))
                .standing(status::Standing::LoginExpired)
                .limits(vec![window("session", 3.0)])
                .build(),
        ];
        let named = |name, aside| words::named_in_menu(Os::Windows, name, aside);
        assert_eq!(
            menu_titles(Os::Windows, accounts.clone()),
            [
                named("work", Some("64%")),
                named("personal", Some("100%")),
                named("spare", None),
                named("old", Some("sign-in expired"))
            ],
            "the reason in place of the figure"
        );
        assert_eq!(
            menu_titles(Os::MacOs, accounts),
            ["work", "personal", "spare", "old"]
        );
    }

    #[test]
    fn a_claude_desktop_row_in_use_says_when_it_was_read() {
        let work = Account {
            read_at: Some(NOW - 2 * 3_600),
            ..desktop_account("work", true)
        };
        let rows = desktop_rows(vec![work]);
        let read = words::read_ago(2 * 3_600, "Claude");
        assert_eq!(rows[0].lines, [read.as_str()]);
        assert!(rows[0].notes.is_empty(), "nothing to do");
        assert_eq!(
            (rows[0].detail.read_at, rows[0].detail.read.as_ref()),
            (Some(NOW - 2 * 3_600), Some(&read))
        );
        assert_eq!(
            rows[0].detail.email, None,
            "nobody tells Pitboard its address"
        );
    }
}
