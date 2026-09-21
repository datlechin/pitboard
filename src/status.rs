//! `pitboard status` — who is signed in, and how much each account has left.
//!
//! Numbers are asked of Anthropic each time, not copied from Claude Code's cache: that cache
//! only moves when Claude Code itself asks, so reading it showed whatever `/usage` last saw.
//! Every account is asked at once, so the command costs one round trip, not one per account.

use crate::api::{self, ApiError, Owner};
use crate::state::State;
use crate::usage::{self, Snapshot, Source};
use crate::{claude, park, readings, store, time};
use serde_json::{Value, json};

pub struct Row {
    /// `None` for an account that is signed in but not enrolled.
    pub label: Option<String>,
    pub email: String,
    pub account_uuid: String,
    pub signed_in: bool,
    pub restorable: bool,
    pub usage: Option<Snapshot>,
    /// Why the reading is not live, when it is not.
    pub stale_because: Option<String>,
}

pub struct Report {
    pub rows: Vec<Row>,
    /// Who the live credential belongs to, as Anthropic says — not as Claude Code's config
    /// remembers, which can be a day out of date.
    pub signed_in: Result<Owner, String>,
    pub backend: Result<store::Backend, store::Error>,
    pub service: String,
    pub config_file: String,
}

/// Everything gathered from the machine and the network, so assembling it touches neither.
struct Facts {
    signed_in: Option<Result<Owner, ApiError>>,
    live_usage: Option<Result<Snapshot, ApiError>>,
    parked_usage: Vec<Option<Result<Snapshot, ApiError>>>,
    claude_code_cache: Option<Snapshot>,
}

fn access_token(oauth: &Value) -> Option<String> {
    oauth
        .get("accessToken")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

pub fn gather(state: &State) -> Report {
    let service = claude::live_service();
    let live_token = store::read(&service)
        .ok()
        .flatten()
        .and_then(|doc| access_token(&doc["claudeAiOauth"]));
    let parked_tokens: Vec<Option<String>> = state
        .accounts
        .iter()
        .map(|a| {
            a.restorable()
                .and_then(|g| park::load(&a.label, g).ok())
                .and_then(|oauth| access_token(&oauth))
        })
        .collect();

    let (signed_in, live_usage, parked_usage) = std::thread::scope(|scope| {
        let owner = scope.spawn(|| live_token.as_deref().map(api::owner));
        let live = scope.spawn(|| live_token.as_deref().map(api::usage));
        let parked: Vec<_> = parked_tokens
            .iter()
            .map(|token| scope.spawn(move || token.as_deref().map(api::usage)))
            .collect();
        (
            owner.join().ok().flatten(),
            live.join().ok().flatten(),
            parked
                .into_iter()
                .map(|h| h.join().ok().flatten())
                .collect(),
        )
    });

    let facts = Facts {
        signed_in,
        live_usage,
        parked_usage,
        claude_code_cache: claude::load_config()
            .ok()
            .as_ref()
            .and_then(usage::from_config_cache),
    };
    let rows = assemble(state, &facts, readings::recall);
    readings::remember(
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
        rows,
        signed_in: match facts.signed_in {
            Some(Ok(owner)) => Ok(owner),
            Some(Err(e)) => Err(e.to_string()),
            None => Err("nothing is signed in".into()),
        },
        backend: store::resolve(&service),
        service,
        config_file: claude::config_file().display().to_string(),
    }
}

fn reason(error: &ApiError, signed_in: bool) -> String {
    match error {
        ApiError::Unauthorized if signed_in => {
            "Claude Code's session has expired; run `claude` and it will refresh".into()
        }
        ApiError::Unauthorized => "its parked login is more than about twelve hours old".into(),
        ApiError::RateLimited => "Anthropic is rate limiting usage checks right now".into(),
        ApiError::Network(_) => "Anthropic could not be reached".into(),
        other => other.to_string(),
    }
}

/// Pick the best reading available, and say why it is not live when it is not.
fn settle(
    asked: Option<&Result<Snapshot, ApiError>>,
    fallback: impl FnOnce() -> Option<Snapshot>,
    signed_in: bool,
) -> (Option<Snapshot>, Option<String>) {
    match asked {
        Some(Ok(live)) => (Some(live.clone()), None),
        Some(Err(e)) => (fallback(), Some(reason(e, signed_in))),
        None if signed_in => (fallback(), Some("nothing is signed in".into())),
        None => (fallback(), Some("no parked login to ask with".into())),
    }
}

fn assemble(state: &State, facts: &Facts, recall: impl Fn(&str) -> Option<Snapshot>) -> Vec<Row> {
    let live_uuid = match &facts.signed_in {
        Some(Ok(owner)) => Some(owner.account_uuid.as_str()),
        _ => None,
    };

    let mut rows: Vec<Row> = state
        .accounts
        .iter()
        .zip(&facts.parked_usage)
        .map(|(account, parked)| {
            let signed_in = live_uuid == Some(account.account_uuid.as_str());
            let (usage, stale_because) = if signed_in {
                settle(
                    facts.live_usage.as_ref(),
                    || {
                        // Claude Code's cache only counts when it was measured for this account.
                        facts
                            .claude_code_cache
                            .clone()
                            .filter(|c| c.account_uuid.as_deref() == live_uuid)
                            .or_else(|| recall(&account.account_uuid))
                    },
                    true,
                )
            } else {
                settle(parked.as_ref(), || recall(&account.account_uuid), false)
            };
            Row {
                label: Some(account.label.clone()),
                email: account.email.clone(),
                account_uuid: account.account_uuid.clone(),
                signed_in,
                restorable: account.restorable().is_some(),
                usage,
                stale_because,
            }
        })
        .collect();

    if let Some(Ok(owner)) = &facts.signed_in
        && !rows.iter().any(|r| r.signed_in)
    {
        let (usage, stale_because) = settle(
            facts.live_usage.as_ref(),
            || {
                facts
                    .claude_code_cache
                    .clone()
                    .filter(|c| c.account_uuid.as_deref() == Some(owner.account_uuid.as_str()))
            },
            true,
        );
        rows.push(Row {
            label: None,
            email: owner.email.clone(),
            account_uuid: owner.account_uuid.clone(),
            signed_in: true,
            restorable: false,
            usage,
            stale_because,
        });
    }

    rows.sort_by_key(|r| !r.signed_in);
    rows
}

fn label_of(kind: &str, scope: Option<&str>) -> String {
    let base = match kind {
        "session" | "five_hour" => "5h",
        "weekly_all" | "seven_day" | "weekly_scoped" => "weekly",
        other => other,
    };
    match scope {
        Some(s) => format!("{base} ({s})"),
        None => base.to_string(),
    }
}

fn provenance(usage: &Snapshot, now: i64) -> String {
    let at = usage
        .observed_at
        .map(|t| time::format_local(t, "%H:%M"))
        .unwrap_or_else(|| "an unknown time".into());
    match usage.source {
        Source::Live => "live".into(),
        Source::ClaudeCodeCache => format!("from Claude Code's cache, measured {at}"),
        Source::Remembered => {
            let age = usage
                .observed_at
                .map(|t| time::humanise_until(now, t).replacen("in ", "", 1))
                .unwrap_or_default();
            format!("measured {at} ({age} ago)")
        }
    }
}

pub fn render_human(report: &Report) -> String {
    let now = time::now();
    let mut out = String::new();

    if report.rows.is_empty() {
        out.push_str("  nothing is signed in and no account is enrolled\n");
    }
    for row in &report.rows {
        let name = row.label.as_deref().unwrap_or("(not enrolled)");
        let marker = if row.signed_in { "signed in" } else { "" };
        out.push_str(&format!("  {name:<10} {:<36} {marker}\n", row.email));

        match &row.usage {
            Some(usage) if !usage.windows.is_empty() => {
                for w in &usage.windows {
                    if w.percent == 0.0 && w.scope.is_some() {
                        continue;
                    }
                    let resets = w
                        .resets_at
                        .map(|t| format!("resets {}", time::humanise_until(t, now)))
                        .unwrap_or_default();
                    out.push_str(&format!(
                        "    {:<14} {}  {:>3.0}%   {resets}\n",
                        label_of(&w.kind, w.scope.as_deref()),
                        usage::bar(w.percent, 10),
                        w.percent
                    ));
                }
                let mut note = provenance(usage, now);
                if let Some(why) = &row.stale_because {
                    note.push_str(" — ");
                    note.push_str(why);
                }
                out.push_str(&format!("    {:<14} {note}\n", ""));
            }
            _ => {
                let why = row.stale_because.as_deref().unwrap_or("no reading yet");
                out.push_str(&format!("    {:<14} no usage known: {why}\n", ""));
            }
        }
        if row.label.is_none() {
            out.push_str(&format!(
                "    {:<14} run `pitboard enroll <label>` so this account can be parked\n",
                ""
            ));
        }
        out.push('\n');
    }

    let backend = match &report.backend {
        Ok(store::Backend::Keychain) => format!("keychain  ·  {}", report.service),
        Ok(store::Backend::File) => format!("file  ·  {}", store::credential_file().display()),
        Ok(store::Backend::Absent) => "no credential found".into(),
        Err(e) => format!("unreadable  ·  {e}"),
    };
    out.push_str(&format!("  store      {backend}\n"));
    out.push_str(&format!("  config     {}\n", report.config_file));
    out
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
            "restorable": r.restorable,
            "usage": r.usage.as_ref().map(|u| json!({
                "source": u.source,
                "observed_at": u.observed_at,
                "windows": u.windows,
            })),
            "stale_because": r.stale_because,
        })).collect::<Vec<_>>(),
        "store": {
            "backend": match &report.backend {
                Ok(store::Backend::Keychain) => "keychain",
                Ok(store::Backend::File) => "file",
                Ok(store::Backend::Absent) => "absent",
                Err(_) => "unreadable",
            },
            "service": report.service,
            "error": report.backend.as_ref().err().map(|e| json!({
                "code": e.code(),
                "message": e.to_string(),
            })),
        },
        "config_file": report.config_file,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Account, Generation};
    use crate::usage::Window;

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
                resets_at: Some(1_789_942_800),
                is_active: true,
            }],
            observed_at: Some(1_789_933_772),
            account_uuid: account.map(str::to_owned),
            source,
        }
    }

    fn account(label: &str) -> Account {
        Account {
            label: label.into(),
            account_uuid: format!("{label}-uuid"),
            email: format!("{label}@example.com"),
            organization_uuid: "org".into(),
            oauth_account: json!({}),
            generations: vec![Generation {
                service: format!("pitboard-park-{label}-1"),
                parked_at: 1_789_900_000,
                refresh_fingerprint: "f".into(),
                installed_at: None,
            }],
        }
    }

    fn state(labels: &[&str]) -> State {
        State {
            accounts: labels.iter().map(|l| account(l)).collect(),
            ..State::default()
        }
    }

    fn nothing_remembered(_: &str) -> Option<Snapshot> {
        None
    }

    #[test]
    fn a_live_reading_is_used_and_marked_live() {
        let s = state(&["work"]);
        let facts = Facts {
            signed_in: Some(Ok(owner("work-uuid"))),
            live_usage: Some(Ok(reading(30.0, Source::Live, None))),
            parked_usage: vec![None],
            claude_code_cache: Some(reading(2.0, Source::ClaudeCodeCache, Some("work-uuid"))),
        };
        let rows = assemble(&s, &facts, nothing_remembered);
        let usage = rows[0].usage.as_ref().unwrap();
        assert_eq!(usage.source, Source::Live);
        assert_eq!(
            usage.windows[0].percent, 30.0,
            "the live number must win over Claude Code's stale cache"
        );
        assert_eq!(rows[0].stale_because, None);
    }

    #[test]
    fn an_expired_session_falls_back_to_claude_codes_cache_and_says_why() {
        let s = state(&["work"]);
        let facts = Facts {
            signed_in: Some(Ok(owner("work-uuid"))),
            live_usage: Some(Err(ApiError::Unauthorized)),
            parked_usage: vec![None],
            claude_code_cache: Some(reading(2.0, Source::ClaudeCodeCache, Some("work-uuid"))),
        };
        let rows = assemble(&s, &facts, nothing_remembered);
        assert_eq!(
            rows[0].usage.as_ref().unwrap().source,
            Source::ClaudeCodeCache
        );
        assert!(
            rows[0]
                .stale_because
                .as_deref()
                .unwrap()
                .contains("expired")
        );
    }

    #[test]
    fn claude_codes_cache_for_another_account_is_never_shown_as_this_one() {
        let s = state(&["work"]);
        let facts = Facts {
            signed_in: Some(Ok(owner("work-uuid"))),
            live_usage: Some(Err(ApiError::Network("offline".into()))),
            parked_usage: vec![None],
            claude_code_cache: Some(reading(99.0, Source::ClaudeCodeCache, Some("someone-else"))),
        };
        let rows = assemble(&s, &facts, nothing_remembered);
        assert!(
            rows[0].usage.is_none(),
            "a foreign measurement must not be rendered"
        );
    }

    #[test]
    fn a_parked_account_is_asked_with_its_own_token() {
        let s = state(&["work", "personal"]);
        let facts = Facts {
            signed_in: Some(Ok(owner("work-uuid"))),
            live_usage: Some(Ok(reading(30.0, Source::Live, None))),
            parked_usage: vec![None, Some(Ok(reading(12.0, Source::Live, None)))],
            claude_code_cache: None,
        };
        let rows = assemble(&s, &facts, nothing_remembered);
        let personal = rows
            .iter()
            .find(|r| r.label.as_deref() == Some("personal"))
            .unwrap();
        assert_eq!(personal.usage.as_ref().unwrap().windows[0].percent, 12.0);
        assert!(!personal.signed_in);
    }

    #[test]
    fn a_parked_account_whose_token_expired_shows_what_was_remembered() {
        let s = state(&["work", "personal"]);
        let facts = Facts {
            signed_in: Some(Ok(owner("work-uuid"))),
            live_usage: Some(Ok(reading(30.0, Source::Live, None))),
            parked_usage: vec![None, Some(Err(ApiError::Unauthorized))],
            claude_code_cache: None,
        };
        let remembered = |uuid: &str| {
            (uuid == "personal-uuid").then(|| reading(44.0, Source::Remembered, Some(uuid)))
        };
        let rows = assemble(&s, &facts, remembered);
        let personal = rows
            .iter()
            .find(|r| r.label.as_deref() == Some("personal"))
            .unwrap();
        assert_eq!(personal.usage.as_ref().unwrap().source, Source::Remembered);
        assert!(
            personal
                .stale_because
                .as_deref()
                .unwrap()
                .contains("twelve hours")
        );
    }

    #[test]
    fn the_signed_in_account_comes_first_and_is_taken_from_the_server_not_the_config() {
        let s = state(&["alpha", "beta"]);
        let facts = Facts {
            signed_in: Some(Ok(owner("beta-uuid"))),
            live_usage: Some(Ok(reading(5.0, Source::Live, None))),
            parked_usage: vec![None, None],
            claude_code_cache: None,
        };
        let rows = assemble(&s, &facts, nothing_remembered);
        assert_eq!(rows[0].label.as_deref(), Some("beta"));
        assert!(rows[0].signed_in);
        assert!(!rows[1].signed_in);
    }

    #[test]
    fn a_signed_in_account_that_is_not_enrolled_still_appears() {
        let s = state(&["alpha"]);
        let facts = Facts {
            signed_in: Some(Ok(owner("stranger"))),
            live_usage: Some(Ok(reading(5.0, Source::Live, None))),
            parked_usage: vec![None],
            claude_code_cache: None,
        };
        let rows = assemble(&s, &facts, nothing_remembered);
        assert_eq!(rows[0].label, None);
        assert!(rows[0].signed_in);
        assert!(
            render_human(&Report {
                rows,
                signed_in: Ok(owner("stranger")),
                backend: Ok(store::Backend::Keychain),
                service: "Claude Code-credentials".into(),
                config_file: "/x".into(),
            })
            .contains("pitboard enroll")
        );
    }

    #[test]
    fn the_human_view_never_prints_a_keychain_item_name() {
        let s = state(&["work"]);
        let facts = Facts {
            signed_in: Some(Ok(owner("work-uuid"))),
            live_usage: Some(Ok(reading(30.0, Source::Live, None))),
            parked_usage: vec![None],
            claude_code_cache: None,
        };
        let text = render_human(&Report {
            rows: assemble(&s, &facts, nothing_remembered),
            signed_in: Ok(owner("work-uuid")),
            backend: Ok(store::Backend::Keychain),
            service: "Claude Code-credentials".into(),
            config_file: "/x".into(),
        });
        assert!(!text.contains("pitboard-park-"), "{text}");
    }

    #[test]
    fn the_json_says_where_every_number_came_from() {
        let s = state(&["work"]);
        let facts = Facts {
            signed_in: Some(Ok(owner("work-uuid"))),
            live_usage: Some(Ok(reading(30.0, Source::Live, None))),
            parked_usage: vec![None],
            claude_code_cache: None,
        };
        let value = render_json(&Report {
            rows: assemble(&s, &facts, nothing_remembered),
            signed_in: Ok(owner("work-uuid")),
            backend: Ok(store::Backend::Keychain),
            service: "Claude Code-credentials".into(),
            config_file: "/x".into(),
        });
        assert_eq!(value["accounts"][0]["usage"]["source"], "live");
        assert_eq!(value["signed_in"]["account_uuid"], "work-uuid");
    }
}
