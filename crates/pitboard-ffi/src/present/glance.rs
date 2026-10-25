//! What a widget, a control or an intent reads of the accounts, in view of others and synced.

use super::accounts::{limit_row, limits_said, standing_of};
use super::{AccountStanding, LimitKind, Seen, words};
use crate::account_windows::stores::derive;
use crate::model::state::{Answered, State};
use crate::model::{LocalTime, OutsideAnswer};
use crate::{Account, UsageLevel};
use pitboard_core::host::{OS, Os};
use pitboard_core::state::Key;
use pitboard_core::status::Stale;
use serde::Serialize;

// A widget reads any schema it knows and leaves another alone.
const SCHEMA: u32 = 1;

#[derive(Serialize)]
struct Glance {
    schema: u32,
    // when the accounts were last read, in epoch seconds
    read_at: Option<i64>,
    accounts: Vec<GlanceAccount>,
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
    limits: Vec<GlanceLimit>,
}

#[derive(Serialize)]
struct GlanceLimit {
    kind: &'static str,
    // the model a weekly limit of one model's is for
    model: Option<String>,
    name: String,
    used: f64,
    level: &'static str,
    resets_at: Option<i64>,
}

// Neither may change: a widget or a shortcut keeps an account by it, across renames.
pub(crate) fn id(account: &str) -> String {
    derive("glance", account)
}

pub(crate) fn glance(state: &State, now: i64, local: &dyn LocalTime) -> String {
    glance_on(OS, state, now, local)
}

pub(crate) fn glance_on(os: Os, state: &State, now: i64, local: &dyn LocalTime) -> String {
    let seen = Seen::new(state, now, local, os);
    let glance = Glance {
        schema: SCHEMA,
        read_at: state.updated_ms.map(|ms| ms.div_euclid(1000)),
        accounts: (seen.accounts().iter())
            .filter_map(|account| glanced(&seen, account))
            .collect(),
    };
    serde_json::to_string(&glance).unwrap_or_default()
}

// Only an account added under a name, which a switch from outside can name.
fn glanced(seen: &Seen, account: &Account) -> Option<GlanceAccount> {
    let label = Key::parse(account.qualified.as_deref()?).label;
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
    Some(GlanceAccount {
        id: id(&account.id),
        tool: account.provider.clone(),
        tool_name: seen.tool_name(&account.provider),
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
                    used: row.percent,
                    level: match row.level {
                        UsageLevel::Plenty => "plenty",
                        UsageLevel::Low => "low",
                        UsageLevel::Out => "out",
                    },
                    resets_at: row.resets_at,
                }
            })
            .collect(),
    })
}

pub(crate) fn answers(seen: &Seen) -> Vec<OutsideAnswer> {
    (seen.state.answered.iter())
        .map(|(request, answered)| {
            let (done, said) = match answered {
                Answered::Switched { tool, label } => (
                    true,
                    format!(
                        "{}.",
                        words::switched_to(Some(&seen.tool_name(tool)), label)
                    ),
                ),
                Answered::InUse { label } => (true, words::already_in_use(label)),
                Answered::Updated { at } => (true, words::updated(&seen.clock(*at))),
                Answered::Unknown => (false, words::NO_SUCH_ACCOUNT.into()),
                Answered::NeedsSignIn { label } => (false, words::sign_in_first(label)),
                Answered::Unusable { label } => (false, words::cannot_switch_to(label)),
                Answered::Busy => (false, words::SWITCH_UNDER_WAY.into()),
                Answered::AppOpen { app } => (false, words::open_pitboard_to_switch(app)),
                Answered::Replace { label } => (false, words::open_pitboard_to_replace(label)),
                Answered::Failed { message } => (false, message.clone()),
            };
            OutsideAnswer {
                request: *request,
                done,
                said,
            }
        })
        .collect()
}
