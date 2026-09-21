//! `pitboard status`: who is signed in, what each account has left, and which accounts can
//! be switched to.
//!
//! Numbers are asked of Anthropic rather than read from Claude Code's cache, which only moves
//! when Claude Code itself asks. Every account is asked at once, so the command costs one
//! round trip, not one per account.

use crate::api::{self, ApiError, Owner};
use crate::context::Context;
use crate::state::{Park, State};
use crate::ui::{self, BAD, BOLD, DIM, GOOD, WARN, pad, paint};
use crate::usage::{Snapshot, Source, Window};
use crate::{claude, park, readings, store, time};
use serde_json::{Value, json};

/// A parked login this close to expiring is worth renewing now.
const RENEW_WITHIN: i64 = 3 * 86_400;

/// Why a reading is not live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stale {
    NothingSignedIn,
    /// Claude Code's own session has expired; its next call renews it.
    SessionExpired,
    /// A parked login's access token has expired and could not be renewed this time.
    ParkedAccessExpired,
    NothingParked,
    ParkUnreadable,
    RateLimited,
    Unreachable,
    Unexpected,
}

impl Stale {
    fn of(error: &ApiError, signed_in: bool) -> Stale {
        match error {
            ApiError::Unauthorized if signed_in => Stale::SessionExpired,
            ApiError::Unauthorized => Stale::ParkedAccessExpired,
            ApiError::RateLimited => Stale::RateLimited,
            ApiError::Network(_) => Stale::Unreachable,
            ApiError::Unexpected { .. } | ApiError::Malformed(_) | ApiError::InvalidGrant => {
                Stale::Unexpected
            }
        }
    }

    /// Only what is worth a word: a parked login going quiet is how parking works.
    fn explanation(self) -> Option<&'static str> {
        match self {
            Stale::NothingSignedIn => Some("nothing is signed in"),
            Stale::SessionExpired => Some("Claude Code's session has expired; `claude` renews it"),
            Stale::ParkedAccessExpired | Stale::NothingParked => None,
            Stale::ParkUnreadable => Some("its parked login cannot be read; run `pitboard doctor`"),
            Stale::RateLimited => Some("Anthropic is rate limiting usage checks"),
            Stale::Unreachable => Some("Anthropic could not be reached"),
            Stale::Unexpected => Some("Anthropic's answer was not understood"),
        }
    }
}

pub struct Row {
    /// `None` for an account that is signed in but not enrolled.
    pub label: Option<String>,
    pub email: String,
    pub account_uuid: String,
    pub signed_in: bool,
    pub parked: Option<Park>,
    pub usage: Option<Snapshot>,
    pub stale: Option<Stale>,
}

impl Row {
    /// Whether `pitboard use` would switch to it now.
    pub fn switchable(&self, now: i64) -> bool {
        !self.signed_in && self.parked.as_ref().is_some_and(|p| p.restorable_at(now))
    }
}

pub struct Report {
    pub now: i64,
    pub rows: Vec<Row>,
    /// Who the live login belongs to, as Anthropic says: Claude Code's config can be a day
    /// behind.
    pub signed_in: Result<Owner, String>,
}

/// Everything gathered from the machine and the network, so assembling it touches neither.
struct Facts {
    signed_in: Option<Result<Owner, ApiError>>,
    live_usage: Result<Snapshot, Stale>,
    /// One per enrolled account, in order.
    parked_usage: Vec<Result<Snapshot, Stale>>,
    claude_code_cache: Option<Snapshot>,
}

fn access_token(oauth: &Value) -> Option<String> {
    oauth
        .get("accessToken")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// The token to ask about a parked account with, or why there is none.
fn parked_token(
    ctx: &Context,
    label: &str,
    parked: Option<&Park>,
    now: i64,
) -> Result<String, Stale> {
    match parked {
        None => Err(Stale::NothingParked),
        Some(p) if !p.askable_at(now) => Err(Stale::ParkedAccessExpired),
        Some(p) => park::load(ctx, label, p)
            .ok()
            .and_then(|oauth| access_token(&oauth))
            .ok_or(Stale::ParkUnreadable),
    }
}

pub fn gather(ctx: &Context, state: &State) -> Report {
    let now = time::now();
    let live_token = store::read(ctx, &claude::live_service(ctx))
        .ok()
        .flatten()
        .and_then(|doc| access_token(&doc["claudeAiOauth"]));
    let parked_tokens: Vec<Result<String, Stale>> = state
        .accounts
        .iter()
        .map(|a| parked_token(ctx, &a.label, a.parked.as_ref(), now))
        .collect();

    let (signed_in, live_usage, parked_usage) = std::thread::scope(|scope| {
        let owner = scope.spawn(|| live_token.as_deref().map(|t| api::owner(ctx, t)));
        let live = scope.spawn(|| match live_token.as_deref() {
            Some(token) => api::usage(ctx, token).map_err(|e| Stale::of(&e, true)),
            None => Err(Stale::NothingSignedIn),
        });
        let parked: Vec<_> = parked_tokens
            .iter()
            .map(|token| {
                scope.spawn(move || {
                    let token = token.as_deref().map_err(|stale| *stale)?;
                    api::usage(ctx, token).map_err(|e| Stale::of(&e, false))
                })
            })
            .collect();
        (
            owner.join().ok().flatten(),
            live.join().unwrap_or(Err(Stale::Unexpected)),
            parked
                .into_iter()
                .map(|h| h.join().unwrap_or(Err(Stale::Unexpected)))
                .collect(),
        )
    });

    let facts = Facts {
        signed_in,
        live_usage,
        parked_usage,
        claude_code_cache: claude::load_config(ctx)
            .ok()
            .as_ref()
            .and_then(crate::usage::from_config_cache),
    };
    let remembered = readings::load(ctx);
    let rows = assemble(state, &facts, |uuid| remembered.get(uuid).cloned());
    readings::remember(
        ctx,
        &rows
            .iter()
            .filter_map(|r| {
                r.usage
                    .as_ref()
                    .filter(|u| u.source == Source::Live)
                    .map(|u| (r.account_uuid.clone(), u.clone()))
            })
            .collect::<Vec<_>>(),
    );

    Report {
        now,
        rows,
        signed_in: match facts.signed_in {
            Some(Ok(owner)) => Ok(owner),
            Some(Err(e)) => Err(e.to_string()),
            None => Err("nothing is signed in".into()),
        },
    }
}

fn assemble(state: &State, facts: &Facts, recall: impl Fn(&str) -> Option<Snapshot>) -> Vec<Row> {
    let live_uuid = match &facts.signed_in {
        Some(Ok(owner)) => Some(owner.account_uuid.as_str()),
        _ => None,
    };
    // Claude Code's cache counts only when it was measured for the account in question.
    let cached_for = |uuid: &str| {
        facts
            .claude_code_cache
            .clone()
            .filter(|c| c.account_uuid.as_deref() == Some(uuid))
    };
    let reading = |asked: &Result<Snapshot, Stale>, fallback: Option<Snapshot>| match asked {
        Ok(live) => (Some(live.clone()), None),
        Err(stale) => (fallback, Some(*stale)),
    };

    let mut rows: Vec<Row> = state
        .accounts
        .iter()
        .zip(&facts.parked_usage)
        .map(|(account, parked)| {
            let uuid = account.account_uuid.as_str();
            let signed_in = live_uuid == Some(uuid);
            let (usage, stale) = if signed_in {
                reading(&facts.live_usage, cached_for(uuid).or_else(|| recall(uuid)))
            } else {
                reading(parked, recall(uuid))
            };
            Row {
                label: Some(account.label.clone()),
                email: account.email.clone(),
                account_uuid: account.account_uuid.clone(),
                signed_in,
                parked: account.parked.clone(),
                usage,
                stale,
            }
        })
        .collect();

    if let Some(Ok(owner)) = &facts.signed_in
        && !rows.iter().any(|r| r.signed_in)
    {
        let (usage, stale) = reading(&facts.live_usage, cached_for(&owner.account_uuid));
        rows.push(Row {
            label: None,
            email: owner.email.clone(),
            account_uuid: owner.account_uuid.clone(),
            signed_in: true,
            parked: None,
            usage,
            stale,
        });
    }

    rows.sort_by_key(|r| !r.signed_in);
    rows
}

fn window_name(w: &Window) -> String {
    let base = match w.kind.as_str() {
        "session" | "five_hour" => "5h",
        "weekly_all" | "seven_day" | "weekly_scoped" => "week",
        other => other,
    };
    match &w.scope {
        Some(scope) => format!("{base} · {scope}"),
        None => base.to_string(),
    }
}

/// Whether the account can be switched to, and what to do when it cannot.
fn standing(row: &Row, now: i64) -> String {
    let label = row.label.as_deref().unwrap_or("<label>");
    let sign_in_again = format!("pitboard enroll {label} --sign-in");
    if row.signed_in {
        return match row.label {
            Some(_) => paint(GOOD, "signed in"),
            None => format!(
                "{} {}",
                paint(GOOD, "signed in"),
                paint(WARN, "· not enrolled: pitboard enroll <label>")
            ),
        };
    }
    match &row.parked {
        None => paint(WARN, format!("nothing parked · {sign_in_again}")),
        Some(p) if !p.restorable_at(now) => paint(BAD, format!("login expired · {sign_in_again}")),
        Some(p) => match p.refresh_expires_at {
            Some(at) if at - now < RENEW_WITHIN => paint(
                WARN,
                format!(
                    "ready · expires in {} · {sign_in_again}",
                    time::span(at - now)
                ),
            ),
            Some(at) => format!(
                "{} {}",
                paint(GOOD, "ready"),
                paint(DIM, format!("· good for {}", time::span(at - now)))
            ),
            None => paint(GOOD, "ready"),
        },
    }
}

fn provenance(usage: &Snapshot, now: i64) -> Option<String> {
    let at = usage.observed_at?;
    match usage.source {
        Source::Live => None,
        Source::ClaudeCodeCache => Some(format!(
            "from Claude Code, measured {}",
            ui::moment(at, now)
        )),
        Source::Remembered => Some(format!(
            "measured {}, {} ago",
            ui::moment(at, now),
            time::span(now - at)
        )),
    }
}

pub fn render_human(report: &Report) -> String {
    let now = report.now;
    if report.rows.is_empty() {
        return "Nothing is signed in and no account is enrolled.\n\
                Run `claude` and sign in, then `pitboard enroll <label>`.\n"
            .into();
    }
    let label_width = report
        .rows
        .iter()
        .filter_map(|r| r.label.as_deref().map(str::len))
        .max()
        .unwrap_or(0);
    let email_width = report.rows.iter().map(|r| r.email.len()).max().unwrap_or(0);
    let name_width = report
        .rows
        .iter()
        .flat_map(|r| r.usage.iter().flat_map(|u| u.windows.iter()))
        .map(|w| window_name(w).chars().count())
        .max()
        .unwrap_or(0);

    let mut blocks = Vec::new();
    for row in &report.rows {
        let marker = if row.signed_in {
            paint(GOOD, "●")
        } else {
            paint(DIM, "○")
        };
        let label = match label_width {
            0 => String::new(),
            width => format!(
                "{}  ",
                paint(BOLD, pad(row.label.as_deref().unwrap_or_default(), width))
            ),
        };
        let mut block = format!(
            "{marker} {label}{}  {}\n",
            paint(DIM, pad(&row.email, email_width)),
            standing(row, now)
        );

        let windows: Vec<&Window> = row.usage.iter().flat_map(|u| u.windows.iter()).collect();
        if windows.is_empty() {
            let why = row.stale.and_then(Stale::explanation);
            block.push_str(&format!(
                "    {}\n",
                paint(
                    DIM,
                    match why {
                        Some(why) => format!("no usage known · {why}"),
                        None => "no usage known yet".into(),
                    }
                )
            ));
        }
        for w in &windows {
            let resets = w.resets_at.map_or_else(String::new, |at| {
                if at <= now {
                    "resetting now".into()
                } else {
                    format!("resets in {}", time::span(at - now))
                }
            });
            block.push_str(&format!(
                "    {}  {}  {}  {}\n",
                pad(&window_name(w), name_width),
                ui::bar(w.percent, 10),
                paint(ui::level(w.percent), format!("{:>3.0}%", w.percent)),
                paint(DIM, resets)
            ));
        }
        if !windows.is_empty() {
            let note = row.usage.as_ref().and_then(|u| provenance(u, now));
            let why = row.stale.and_then(Stale::explanation);
            let line = match (note, why) {
                (Some(note), Some(why)) => {
                    format!("{} {}", paint(DIM, note), paint(WARN, format!("· {why}")))
                }
                (Some(note), None) => paint(DIM, note),
                (None, Some(why)) => paint(WARN, why),
                (None, None) => String::new(),
            };
            if !line.is_empty() {
                block.push_str(&format!("    {}  {line}\n", pad("", name_width)));
            }
        }
        blocks.push(block);
    }
    blocks.join("\n")
}

pub fn render_json(report: &Report) -> Value {
    json!({
        "signed_in": report.signed_in.as_ref().ok().map(|o| json!({
            "account_uuid": o.account_uuid,
            "email": o.email,
            "organization_uuid": o.organization_uuid,
        })),
        "signed_in_error": report.signed_in.as_ref().err(),
        "accounts": report.rows.iter().map(|r| json!({
            "label": r.label,
            "email": r.email,
            "account_uuid": r.account_uuid,
            "signed_in": r.signed_in,
            "switchable": r.switchable(report.now),
            "parked": r.parked.as_ref().map(|p| json!({
                "parked_at": p.parked_at,
                "access_expires_at": p.access_expires_at,
                "refresh_expires_at": p.refresh_expires_at,
            })),
            "usage": r.usage.as_ref().map(|u| json!({
                "source": u.source,
                "observed_at": u.observed_at,
                "windows": u.windows,
            })),
            "stale": r.stale,
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Account;

    const NOW: i64 = 1_789_935_000;

    fn owner(uuid: &str) -> Owner {
        Owner {
            account_uuid: uuid.into(),
            email: format!("{uuid}@example.com"),
            organization_uuid: "org".into(),
        }
    }

    fn reading(percent: f64, source: Source, account: Option<&str>) -> Snapshot {
        Snapshot {
            windows: vec![Window {
                kind: "session".into(),
                scope: None,
                percent,
                resets_at: Some(NOW + 3_600),
                is_active: true,
            }],
            observed_at: Some(NOW - 7_200),
            account_uuid: account.map(str::to_owned),
            source,
        }
    }

    fn parked(refresh_expires_at: i64) -> Park {
        Park {
            service: "pitboard-park-x-1".into(),
            parked_at: NOW - 86_400,
            refresh_fingerprint: "f".into(),
            access_expires_at: Some(NOW - 3_600),
            refresh_expires_at: Some(refresh_expires_at),
        }
    }

    fn account(label: &str) -> Account {
        Account {
            label: label.into(),
            account_uuid: format!("{label}-uuid"),
            email: format!("{label}@example.com"),
            organization_uuid: "org".into(),
            oauth_account: json!({}),
            parked: Some(parked(NOW + 20 * 86_400)),
        }
    }

    fn state(labels: &[&str]) -> State {
        State {
            accounts: labels.iter().map(|l| account(l)).collect(),
            ..State::default()
        }
    }

    fn facts(
        signed_in: &str,
        live: Result<Snapshot, Stale>,
        parked: Vec<Result<Snapshot, Stale>>,
    ) -> Facts {
        Facts {
            signed_in: Some(Ok(owner(signed_in))),
            live_usage: live,
            parked_usage: parked,
            claude_code_cache: None,
        }
    }

    fn nothing_remembered(_: &str) -> Option<Snapshot> {
        None
    }

    fn report(rows: Vec<Row>) -> Report {
        Report {
            now: NOW,
            rows,
            signed_in: Ok(owner("work-uuid")),
        }
    }

    fn plain(styled: &str) -> String {
        anstream::adapter::strip_str(styled).to_string()
    }

    #[test]
    fn a_live_reading_wins_over_claude_codes_cache() {
        let s = state(&["work"]);
        let mut f = facts(
            "work-uuid",
            Ok(reading(30.0, Source::Live, None)),
            vec![Err(Stale::NothingParked)],
        );
        f.claude_code_cache = Some(reading(2.0, Source::ClaudeCodeCache, Some("work-uuid")));
        let rows = assemble(&s, &f, nothing_remembered);
        let usage = rows[0].usage.as_ref().unwrap();
        assert_eq!(usage.source, Source::Live);
        assert_eq!(usage.windows[0].percent, 30.0);
        assert_eq!(rows[0].stale, None);
    }

    #[test]
    fn an_expired_session_falls_back_to_claude_codes_cache_and_says_why() {
        let s = state(&["work"]);
        let mut f = facts(
            "work-uuid",
            Err(Stale::SessionExpired),
            vec![Err(Stale::NothingParked)],
        );
        f.claude_code_cache = Some(reading(2.0, Source::ClaudeCodeCache, Some("work-uuid")));
        let rows = assemble(&s, &f, nothing_remembered);
        assert_eq!(
            rows[0].usage.as_ref().unwrap().source,
            Source::ClaudeCodeCache
        );
        assert_eq!(rows[0].stale, Some(Stale::SessionExpired));
    }

    #[test]
    fn claude_codes_cache_for_another_account_is_never_shown_as_this_one() {
        let s = state(&["work"]);
        let mut f = facts(
            "work-uuid",
            Err(Stale::Unreachable),
            vec![Err(Stale::NothingParked)],
        );
        f.claude_code_cache = Some(reading(99.0, Source::ClaudeCodeCache, Some("someone-else")));
        assert!(assemble(&s, &f, nothing_remembered)[0].usage.is_none());
    }

    #[test]
    fn a_parked_account_is_asked_with_its_own_login() {
        let s = state(&["work", "personal"]);
        let f = facts(
            "work-uuid",
            Ok(reading(30.0, Source::Live, None)),
            vec![
                Err(Stale::NothingParked),
                Ok(reading(12.0, Source::Live, None)),
            ],
        );
        let rows = assemble(&s, &f, nothing_remembered);
        let personal = rows
            .iter()
            .find(|r| r.label.as_deref() == Some("personal"))
            .unwrap();
        assert_eq!(personal.usage.as_ref().unwrap().windows[0].percent, 12.0);
        assert!(personal.switchable(NOW));
    }

    #[test]
    fn a_parked_login_past_its_access_expiry_is_not_asked() {
        let ctx = Context::from_env();
        let token = parked_token(&ctx, "personal", Some(&parked(NOW + 86_400)), NOW);
        assert_eq!(token, Err(Stale::ParkedAccessExpired));
        assert_eq!(
            parked_token(&ctx, "personal", None, NOW),
            Err(Stale::NothingParked)
        );
    }

    #[test]
    fn what_cannot_be_asked_shows_what_was_remembered() {
        let s = state(&["work", "personal"]);
        let f = facts(
            "work-uuid",
            Ok(reading(30.0, Source::Live, None)),
            vec![Err(Stale::NothingParked), Err(Stale::ParkedAccessExpired)],
        );
        let remembered = |uuid: &str| {
            (uuid == "personal-uuid").then(|| reading(44.0, Source::Remembered, Some(uuid)))
        };
        let rows = assemble(&s, &f, remembered);
        let personal = rows
            .iter()
            .find(|r| r.label.as_deref() == Some("personal"))
            .unwrap();
        assert_eq!(personal.usage.as_ref().unwrap().source, Source::Remembered);
        let text = plain(&render_human(&report(rows)));
        assert!(
            text.contains("measured") && text.contains("2h 00m ago"),
            "{text}"
        );
    }

    #[test]
    fn the_signed_in_account_comes_first_and_is_taken_from_the_server_not_the_config() {
        let s = state(&["alpha", "beta"]);
        let f = facts(
            "beta-uuid",
            Ok(reading(5.0, Source::Live, None)),
            vec![Err(Stale::NothingParked), Err(Stale::NothingParked)],
        );
        let rows = assemble(&s, &f, nothing_remembered);
        assert_eq!(rows[0].label.as_deref(), Some("beta"));
        assert!(rows[0].signed_in && !rows[0].switchable(NOW));
        assert!(!rows[1].signed_in);
    }

    #[test]
    fn a_signed_in_account_that_is_not_enrolled_says_how_to_enroll_it() {
        let s = state(&["alpha"]);
        let f = facts(
            "stranger",
            Ok(reading(5.0, Source::Live, None)),
            vec![Err(Stale::NothingParked)],
        );
        let rows = assemble(&s, &f, nothing_remembered);
        assert_eq!(rows[0].label, None);
        assert!(plain(&render_human(&report(rows))).contains("pitboard enroll <label>"));
    }

    #[test]
    fn an_account_that_cannot_be_switched_to_says_how_to_fix_it() {
        let mut s = state(&["work", "expired", "empty", "soon"]);
        s.accounts[1].parked = Some(parked(NOW - 1));
        s.accounts[2].parked = None;
        s.accounts[3].parked = Some(parked(NOW + 86_400));
        let f = facts(
            "work-uuid",
            Ok(reading(1.0, Source::Live, None)),
            vec![Err(Stale::NothingParked); 4],
        );
        let rows = assemble(&s, &f, nothing_remembered);
        assert!(
            rows.iter()
                .all(|r| !r.switchable(NOW) || r.label.as_deref() == Some("soon"))
        );
        let text = plain(&render_human(&report(rows)));
        for (label, says) in [
            (
                "expired",
                "login expired · pitboard enroll expired --sign-in",
            ),
            ("empty", "nothing parked · pitboard enroll empty --sign-in"),
            ("soon", "expires in 1d 0h · pitboard enroll soon --sign-in"),
        ] {
            let line = text
                .lines()
                .find(|l| l.contains(&format!(" {label} ")))
                .unwrap();
            assert!(line.contains(says), "{line}");
        }
    }

    #[test]
    fn columns_line_up_whatever_the_label_and_email_lengths() {
        let s = state(&["a", "much-longer-label"]);
        let f = facts(
            "a-uuid",
            Ok(reading(5.0, Source::Live, None)),
            vec![Err(Stale::NothingParked), Err(Stale::NothingParked)],
        );
        let text = plain(&render_human(&report(assemble(&s, &f, nothing_remembered))));
        let column = |email: &str| {
            let line = text.lines().find(|l| l.contains(email)).unwrap();
            line[..line.find(email).unwrap()].chars().count()
        };
        assert_eq!(
            column("a@example.com"),
            column("much-longer-label@example.com"),
            "{text}"
        );
    }

    /// An unused model limit is still a limit: at 0% it says there is room.
    #[test]
    fn every_limit_is_shown_including_one_not_yet_used() {
        let s = state(&["work"]);
        let mut live = reading(30.0, Source::Live, None);
        live.windows.push(Window {
            kind: "weekly_scoped".into(),
            scope: Some("Fable".into()),
            percent: 0.0,
            resets_at: Some(NOW + 86_400),
            is_active: false,
        });
        let f = facts("work-uuid", Ok(live), vec![Err(Stale::NothingParked)]);
        let text = plain(&render_human(&report(assemble(&s, &f, nothing_remembered))));
        let fable = text
            .lines()
            .find(|l| l.contains("week · Fable"))
            .expect(&text);
        assert!(fable.contains("0%"), "{fable}");
    }

    #[test]
    fn the_human_view_never_prints_a_keychain_item_name() {
        let s = state(&["work"]);
        let f = facts(
            "work-uuid",
            Ok(reading(30.0, Source::Live, None)),
            vec![Err(Stale::NothingParked)],
        );
        let text = render_human(&report(assemble(&s, &f, nothing_remembered)));
        assert!(!text.contains("pitboard-park-"), "{text}");
    }

    #[test]
    fn the_json_says_where_every_number_came_from_and_what_can_be_switched_to() {
        let s = state(&["work", "personal"]);
        let f = facts(
            "work-uuid",
            Ok(reading(30.0, Source::Live, None)),
            vec![Err(Stale::NothingParked), Err(Stale::ParkedAccessExpired)],
        );
        let value = render_json(&report(assemble(&s, &f, nothing_remembered)));
        assert_eq!(value["accounts"][0]["usage"]["source"], "live");
        assert_eq!(value["accounts"][0]["switchable"], false);
        assert_eq!(value["accounts"][1]["switchable"], true);
        assert_eq!(value["accounts"][1]["stale"], "parked_access_expired");
        assert!(value["accounts"][1]["parked"]["refresh_expires_at"].is_i64());
        assert!(value["accounts"][1]["parked"].get("service").is_none());
        assert_eq!(value["signed_in"]["account_uuid"], "work-uuid");
    }
}
