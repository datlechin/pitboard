//! `pitboard status`: who is signed in, what each account has left, and which accounts can
//! be switched to.
//!
//! Numbers are asked of Anthropic rather than read from Claude Code's cache, which only moves
//! when Claude Code itself asks. Every account is asked at once, so the command costs one
//! round trip, not one per account.

use crate::api::{self, ApiError, Owner};
use crate::context::Context;
use crate::state::{Park, State};
use crate::usage::{Snapshot, Source};
use crate::{claude, park, readings, store, time};
use serde_json::Value;

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

    /// Stable, for a program to branch on; the same as its JSON form.
    pub fn code(self) -> &'static str {
        match self {
            Stale::NothingSignedIn => "nothing_signed_in",
            Stale::SessionExpired => "session_expired",
            Stale::ParkedAccessExpired => "parked_access_expired",
            Stale::NothingParked => "nothing_parked",
            Stale::ParkUnreadable => "park_unreadable",
            Stale::RateLimited => "rate_limited",
            Stale::Unreachable => "unreachable",
            Stale::Unexpected => "unexpected",
        }
    }

    /// Only what is worth a word: a parked login going quiet is how parking works.
    pub fn explanation(self) -> Option<&'static str> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Account;
    use crate::usage::Window;
    use serde_json::json;

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

    #[test]
    fn every_stale_code_is_its_json_form() {
        for stale in [
            Stale::NothingSignedIn,
            Stale::SessionExpired,
            Stale::ParkedAccessExpired,
            Stale::NothingParked,
            Stale::ParkUnreadable,
            Stale::RateLimited,
            Stale::Unreachable,
            Stale::Unexpected,
        ] {
            assert_eq!(serde_json::to_value(stale).unwrap(), stale.code());
        }
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
    fn what_cannot_be_asked_falls_back_to_what_was_remembered() {
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
        assert_eq!(personal.stale, Some(Stale::ParkedAccessExpired));
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
}
