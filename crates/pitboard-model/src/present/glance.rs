//! The widget feed, the glance a widget, a control or an intent reads of the accounts, in view
//! of others and synced.

use super::accounts::{headline, limit_row, limits_said, standing_of};
use super::sheets::quit_question;
use super::{AccountStanding, LimitKind, Seen, words};
use crate::account_windows::stores::derive;
use crate::model::state::{After, Answered, State};
use crate::model::{LocalTime, OutsideAnswer};
use crate::{Account, Front, UsageLevel};
use pitboard_core::status::Stale;
use pitboard_core::words::figure;
use serde::Serialize;

// A widget reads any schema it knows and leaves another alone.
const SCHEMA: u32 = 1;

#[derive(Serialize)]
struct Glance {
    schema: u32,
    // when the accounts were last read, in epoch seconds
    read_at: Option<i64>,
    words: GlanceWords,
    accounts: Vec<GlanceAccount>,
}

// What a widget or a control says beside the accounts, which links no Rust to say it.
#[derive(Serialize)]
struct GlanceWords {
    in_use: String,
    // before the countdown a widget draws to a reset
    resets_in: &'static str,
    resetting_now: &'static str,
    // "Updated 8:00 AM"
    updated: Option<String>,
    no_accounts: String,
    none_in_use: &'static str,
    // a widget's Switch, on macOS 27
    switch_button: &'static str,
    open_to_switch: &'static str,
}

#[derive(Serialize)]
struct GlanceAccount {
    id: String,
    tool: String,
    tool_name: String,
    name: String,
    in_use: bool,
    standing: &'static str,
    plan: Option<String>,
    // the menu's line: "5-hour 42% used, Weekly 12%"
    said: String,
    // beside its name in a control: "In Use", its tightest limit's "64%", or why it has none
    value: String,
    limits: Vec<GlanceLimit>,
}

#[derive(Serialize)]
struct GlanceLimit {
    kind: &'static str,
    // the model a weekly limit of one model's is for
    model: Option<String>,
    name: String,
    // beside its bar: "5-hour"
    short: String,
    used: f64,
    figure: String,
    level: &'static str,
    resets_at: Option<i64>,
    // as the app's row says it: "Resets 3:45 PM", or empty where nothing says
    resets: String,
    spoken: String,
}

// The "glance" name and the stores' namespace never change: a widget keeps an account by this
// id across renames.
pub(crate) fn id(account: &str) -> String {
    derive("glance", account)
}

// The file, and what a widget shows of it: everything but when the accounts were read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GlanceText {
    pub(crate) file: String,
    pub(crate) shown: String,
}

pub(crate) fn glance(state: &State, now: i64, local: &dyn LocalTime) -> GlanceText {
    glance_on(state.front, state, now, local)
}

pub(crate) fn glance_on(
    front: Front,
    state: &State,
    now: i64,
    local: &dyn LocalTime,
) -> GlanceText {
    let seen = Seen::new(state, now, local, front);
    let accounts: Vec<GlanceAccount> = (seen.accounts().iter())
        .filter_map(|account| glanced(&seen, account))
        .collect();
    let shown = serde_json::to_string(&accounts).unwrap_or_default();
    let read_at = state.updated_ms.map(|ms| ms.div_euclid(1000));
    let glance = Glance {
        schema: SCHEMA,
        read_at,
        words: GlanceWords {
            in_use: words::in_use(front),
            resets_in: words::RESETS_IN,
            resetting_now: words::RESETTING_NOW,
            updated: read_at.map(|at| words::updated(&seen.clock(at))),
            no_accounts: words::no_accounts(front).0,
            none_in_use: words::NONE_IN_USE,
            switch_button: words::SWITCH,
            open_to_switch: words::OPEN_TO_SWITCH,
        },
        accounts,
    };
    GlanceText {
        file: serde_json::to_string(&glance).unwrap_or_default(),
        shown,
    }
}

// Only an account added under a name, which a switch from outside can name.
fn glanced(seen: &Seen, account: &Account) -> Option<GlanceAccount> {
    let label = account.key.as_ref()?.label.clone();
    if account.unplaced {
        return None;
    }
    let standing = standing_of(account, false);
    let windows = account
        .usage
        .as_ref()
        .map_or(&[][..], |usage| &usage.windows);
    let said = match standing {
        AccountStanding::NeedsSignIn => words::sign_in_standing(account.standing).into(),
        AccountStanding::Unusable => words::CANNOT_READ_OR_SWITCH.into(),
        _ if windows.is_empty() && account.stale == Some(Stale::NoUsage) => {
            words::NO_USAGE_READ.into()
        }
        _ => limits_said(seen, account),
    };
    let value = match headline(windows) {
        _ if account.signed_in => words::in_use(seen.front),
        Some(tightest) => figure(tightest.percent),
        None => said.clone(),
    };
    Some(GlanceAccount {
        id: id(&account.id),
        tool: account.provider.code().into(),
        tool_name: account.provider.name().into(),
        name: label,
        in_use: account.signed_in,
        standing: match standing {
            AccountStanding::InUse => "in_use",
            AccountStanding::Ready => "ready",
            AccountStanding::Expiring => "expiring",
            AccountStanding::NeedsSignIn => "needs_sign_in",
            AccountStanding::NotAdded => "not_added",
            AccountStanding::Unusable => "unusable",
            AccountStanding::Switching => "switching",
        },
        plan: account.plan.clone(),
        said,
        value,
        limits: windows
            .iter()
            .map(|limit| {
                let row = limit_row(seen, limit);
                let (kind, model) = match row.kind {
                    LimitKind::FiveHour => ("five_hour", None),
                    LimitKind::Weekly => ("weekly", None),
                    LimitKind::WeeklyModel { model } => ("weekly_model", Some(model)),
                    LimitKind::Other { .. } => ("other", None),
                };
                GlanceLimit {
                    kind,
                    model,
                    name: row.name,
                    short: row.short,
                    used: row.percent,
                    figure: row.figure,
                    level: match row.level {
                        UsageLevel::Plenty => "plenty",
                        UsageLevel::Low => "low",
                        UsageLevel::Out => "out",
                    },
                    resets_at: row.resets_at,
                    resets: row.resets,
                    spoken: row.spoken,
                }
            })
            .collect(),
    })
}

pub(crate) fn answers(seen: &Seen) -> Vec<OutsideAnswer> {
    (seen.state.answered.iter())
        .map(|(request, answered)| {
            let question = match answered {
                Answered::AppOpen { question } => {
                    Some(quit_question(seen, question, &After::Switch).question)
                }
                _ => None,
            };
            let (done, said) = match answered {
                Answered::Switched { tool, label } => (
                    true,
                    format!("{}.", words::switched_to(Some(tool.name()), label)),
                ),
                Answered::InUse { label } => (true, words::already_in_use(label)),
                Answered::Updated { at } => (true, words::updated(&seen.clock(*at))),
                Answered::Unknown => (false, words::NO_SUCH_ACCOUNT.into()),
                Answered::NeedsSignIn { label } => (false, words::sign_in_first(label)),
                Answered::Unusable { label } => (false, words::cannot_switch_to(label)),
                Answered::Busy => (false, words::SWITCH_UNDER_WAY.into()),
                Answered::AppOpen { question } => {
                    (false, words::open_pitboard_to_switch(&question.name))
                }
                Answered::Replace { label } => (false, words::open_pitboard_to_replace(label)),
                Answered::Asking => (false, words::QUESTION_OPEN.into()),
                Answered::Uninstalling => (false, words::BEING_UNINSTALLED.into()),
                Answered::Failed { doing } => {
                    (false, words::failed_outside(&words::could_not(doing)))
                }
            };
            OutsideAnswer {
                request: *request,
                done,
                said,
                question,
            }
        })
        .collect()
}
