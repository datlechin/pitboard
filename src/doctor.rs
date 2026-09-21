//! `pitboard doctor` — prove, on this machine right now, that what this tool believes
//! about Claude Code is still true.
//!
//! This exists because the tool depends on behaviour Anthropic never documented. The
//! honest response to that is not to hope, but to check every assumption on every run and
//! say plainly which one broke.
//!
//! Gathering is separated from judging so the judging can be tested. A check that is a
//! pure function of constants proves nothing about the machine it runs on; there used to
//! be one here, and it could never fail.

use crate::error::Error;
use crate::{claude, home, slot, store, usage};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

pub struct Check {
    /// Stable, snake_case, safe for a program to branch on.
    pub code: &'static str,
    pub name: &'static str,
    pub level: Level,
    pub detail: String,
    /// What the user should do. Empty when there is nothing to do.
    pub advice: String,
}

/// Everything read from the machine, so that judging it touches nothing.
pub struct Facts {
    pub security_tool: Option<String>,
    pub config_path: PathBuf,
    pub config: Result<Value, Error>,
    pub identity: Option<claude::Identity>,
    pub service: String,
    pub account: String,
    pub default_slot: bool,
    pub storage_dir: String,
    pub backend: Result<store::Backend, store::Error>,
    pub credential_file: PathBuf,
    pub credential: Result<Option<Value>, store::Error>,
    pub home: PathBuf,
    pub home_mode: Option<u32>,
    pub now: i64,
}

pub fn gather() -> Facts {
    let config = claude::load_config();
    let service = claude::live_service();
    Facts {
        security_tool: cfg!(target_os = "macos")
            .then(|| "/usr/bin/security".to_string())
            .filter(|p| std::fs::metadata(p).is_ok()),
        config_path: claude::config_file(),
        identity: config.as_ref().ok().and_then(claude::identity),
        config,
        account: slot::account_name(),
        default_slot: claude::is_default_slot(),
        storage_dir: claude::storage_dir(),
        backend: store::resolve(&service),
        credential_file: store::credential_file(),
        credential: store::read(&service),
        home: home::dir(),
        home_mode: mode_of(&home::dir()),
        service,
        now: crate::time::now(),
    }
}

fn mode_of(path: &std::path::Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(std::fs::metadata(path).ok()?.permissions().mode() & 0o777)
}

fn ok(code: &'static str, name: &'static str, detail: impl Into<String>) -> Check {
    Check {
        code,
        name,
        level: Level::Ok,
        detail: detail.into(),
        advice: String::new(),
    }
}
fn warn(
    code: &'static str,
    name: &'static str,
    detail: impl Into<String>,
    advice: impl Into<String>,
) -> Check {
    Check {
        code,
        name,
        level: Level::Warn,
        detail: detail.into(),
        advice: advice.into(),
    }
}
fn fail(
    code: &'static str,
    name: &'static str,
    detail: impl Into<String>,
    advice: impl Into<String>,
) -> Check {
    Check {
        code,
        name,
        level: Level::Fail,
        detail: detail.into(),
        advice: advice.into(),
    }
}

pub fn evaluate(facts: &Facts) -> Vec<Check> {
    let mut checks = Vec::new();

    if cfg!(target_os = "macos") {
        checks.push(match &facts.security_tool {
            Some(path) => ok("security_tool", "security tool", path.clone()),
            None => fail(
                "security_tool",
                "security tool",
                "/usr/bin/security is missing",
                "pitboard reads the keychain the same way Claude Code does. Without it, nothing works.",
            ),
        });
    }

    checks.push(match &facts.config {
        Ok(v) => ok(
            "config_file",
            "config file",
            format!(
                "{}  ({} keys)",
                facts.config_path.display(),
                v.as_object().map_or(0, serde_json::Map::len)
            ),
        ),
        Err(e) => fail(
            "config_file",
            "config file",
            e.to_string(),
            "Run `claude` once.",
        ),
    });

    checks.push(match &facts.identity {
        Some(id) => ok(
            "identity",
            "identity",
            format!("{}  ·  org {}", id.email, id.organization_uuid),
        ),
        None => warn(
            "identity",
            "identity",
            "Claude Code has not recorded a signed-in account",
            "It writes this after its first successful call. Run `claude` once.",
        ),
    });

    checks.push(if facts.default_slot {
        ok(
            "slot",
            "slot",
            format!(
                "default  ·  {}  ·  account {}",
                facts.service, facts.account
            ),
        )
    } else {
        ok(
            "slot",
            "slot",
            format!(
                "{}  ·  account {}  (selected by {})",
                facts.service, facts.account, facts.storage_dir
            ),
        )
    });

    checks.push(match &facts.backend {
        Ok(store::Backend::Keychain) => ok("credential_store", "credential store", "keychain"),
        Ok(store::Backend::File) => {
            let detail = format!("plaintext file  ·  {}", facts.credential_file.display());
            if cfg!(target_os = "macos") {
                warn(
                    "credential_store",
                    "credential store",
                    detail,
                    "Claude Code fell back to a file, which means a keychain write failed at some point.",
                )
            } else {
                ok("credential_store", "credential store", detail)
            }
        }
        Ok(store::Backend::Absent) => warn(
            "credential_store",
            "credential store",
            "no credential in any backend",
            "Nothing is signed in for this slot.",
        ),
        Err(e) => fail(
            "credential_store",
            "credential store",
            e.to_string(),
            "Treat this as unknown, never as empty.",
        ),
    });

    checks.push(judge_credential(facts));

    checks.push(
        match (
            facts
                .config
                .as_ref()
                .ok()
                .and_then(usage::from_config_cache),
            &facts.identity,
        ) {
            (Some(s), Some(id)) if s.account_uuid.as_deref() == Some(id.account_uuid.as_str()) => {
                ok(
                    "usage_cache",
                    "usage cache",
                    format!("{} windows, measured for this account", s.windows.len()),
                )
            }
            (Some(_), Some(_)) => warn(
                "usage_cache",
                "usage cache",
                "cached for a different account",
                "Its numbers are ignored rather than shown, which is why status may look empty.",
            ),
            (Some(s), None) => ok(
                "usage_cache",
                "usage cache",
                format!("{} windows", s.windows.len()),
            ),
            (None, _) => warn(
                "usage_cache",
                "usage cache",
                "absent",
                "Claude Code writes it after a call that reports usage.",
            ),
        },
    );

    checks.push(match facts.home_mode {
        None => ok(
            "home",
            "pitboard home",
            format!("{} (not created yet)", facts.home.display()),
        ),
        Some(0o700) => ok("home", "pitboard home", facts.home.display().to_string()),
        Some(mode) => warn(
            "home",
            "pitboard home",
            format!("{} is mode {mode:o}", facts.home.display()),
            "Park names contain account identifiers, so the directory should be 0700. \
             pitboard tightens it on its next write.",
        ),
    });

    checks.push(judge_storage_v5(facts));
    checks
}

fn judge_credential(facts: &Facts) -> Check {
    match &facts.credential {
        Ok(Some(doc)) => {
            let keys: Vec<&str> = doc
                .as_object()
                .map(|o| o.keys().map(String::as_str).collect())
                .unwrap_or_default();
            let Some(oauth) = doc.get("claudeAiOauth").and_then(Value::as_object) else {
                return fail(
                    "credential",
                    "credential",
                    format!("the document has no claudeAiOauth; its keys are {keys:?}"),
                    "The credential's shape changed. Do not switch accounts until this is understood.",
                );
            };
            let fingerprint = oauth
                .get("refreshToken")
                .and_then(Value::as_str)
                .map(store::fingerprint)
                .unwrap_or_else(|| "none".into());
            let days = oauth
                .get("refreshTokenExpiresAt")
                .and_then(Value::as_i64)
                .map_or(-1, |ms| (ms / 1000 - facts.now) / 86_400);
            let detail = format!("refresh {fingerprint}  ·  {days} days left  ·  keys {keys:?}");
            if days < 3 {
                warn(
                    "credential",
                    "credential",
                    detail,
                    "This login expires soon and will need signing in again.",
                )
            } else {
                ok("credential", "credential", detail)
            }
        }
        Ok(None) => warn(
            "credential",
            "credential",
            "nothing stored",
            "Nothing is signed in for this slot.",
        ),
        Err(e) => fail(
            "credential",
            "credential",
            e.to_string(),
            "Do not write to the store while this is failing.",
        ),
    }
}

/// The successor credential backend. A stub in every shipped build so far, but compiled in
/// and switched on from the server, so it is worth watching for.
fn judge_storage_v5(facts: &Facts) -> Check {
    let env_on = std::env::var("CLAUDE_CODE_HOVER_REST").is_ok_and(|v| v == "1" || v == "true");
    let flag_on = facts
        .config
        .as_ref()
        .ok()
        .and_then(|c| c.get("cachedGrowthBookFeatures"))
        .and_then(|f| f.get("tengu_hover_rest"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if env_on || flag_on {
        warn(
            "storage_v5",
            "storage v5",
            "the successor credential backend is switched on",
            "pitboard has not been verified against it. Check for an update before switching.",
        )
    } else {
        ok("storage_v5", "storage v5", "inactive")
    }
}

pub fn run() -> Vec<Check> {
    evaluate(&gather())
}

pub fn render_human(checks: &[Check]) -> String {
    let mut out = String::new();
    for c in checks {
        let mark = match c.level {
            Level::Ok => "ok  ",
            Level::Warn => "warn",
            Level::Fail => "FAIL",
        };
        out.push_str(&format!("  {mark}  {:<18}{}\n", c.name, c.detail));
        if !c.advice.is_empty() {
            out.push_str(&format!("        {:<18}{}\n", "", c.advice));
        }
    }
    out
}

pub fn render_json(checks: &[Check]) -> Value {
    json!({
        "schema": 1,
        "ok": checks.iter().all(|c| c.level != Level::Fail),
        "checks": checks.iter().map(|c| json!({
            "code": c.code,
            "level": match c.level { Level::Ok => "ok", Level::Warn => "warn", Level::Fail => "fail" },
            "detail": c.detail,
            "advice": c.advice,
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts {
            security_tool: Some("/usr/bin/security".into()),
            config_path: PathBuf::from("/home/x/.claude.json"),
            config: Ok(json!({"oauthAccount": {}})),
            identity: Some(claude::Identity {
                email: "a@b.c".into(),
                account_uuid: "acc".into(),
                organization_uuid: "org".into(),
                organization_name: None,
                subscription: None,
                rate_limit_tier: None,
            }),
            service: "Claude Code-credentials".into(),
            account: "someone".into(),
            default_slot: true,
            storage_dir: "/home/x/.claude".into(),
            backend: Ok(store::Backend::Keychain),
            credential_file: PathBuf::from("/home/x/.claude/.credentials.json"),
            credential: Ok(Some(json!({"claudeAiOauth": {
                "refreshToken": "r",
                "refreshTokenExpiresAt": 2_000_000_000_000i64
            }}))),
            home: PathBuf::from("/home/x/.pitboard"),
            home_mode: Some(0o700),
            now: 1_789_935_600,
        }
    }

    fn check<'a>(checks: &'a [Check], code: &str) -> &'a Check {
        checks.iter().find(|c| c.code == code).expect(code)
    }

    #[test]
    fn a_healthy_machine_reports_no_failures() {
        let checks = evaluate(&facts());
        assert!(checks.iter().all(|c| c.level != Level::Fail));
        assert!(render_json(&checks)["ok"].as_bool().unwrap());
    }

    #[test]
    fn a_credential_without_claude_ai_oauth_is_a_failure_not_a_warning() {
        let mut f = facts();
        f.credential = Ok(Some(json!({"slackTag": {}})));
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "credential").level, Level::Fail);
        assert!(!render_json(&checks)["ok"].as_bool().unwrap());
    }

    #[test]
    fn an_unreadable_store_fails_rather_than_reporting_nothing_stored() {
        let mut f = facts();
        f.credential = Err(store::Error::Unreadable("security exited 1".into()));
        f.backend = Err(store::Error::Unreadable("security exited 1".into()));
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "credential").level, Level::Fail);
        assert_eq!(check(&checks, "credential_store").level, Level::Fail);
    }

    #[test]
    fn a_login_about_to_expire_is_flagged() {
        let mut f = facts();
        f.credential = Ok(Some(json!({"claudeAiOauth": {
            "refreshToken": "r",
            "refreshTokenExpiresAt": (f.now + 86_400) * 1000
        }})));
        assert_eq!(check(&evaluate(&f), "credential").level, Level::Warn);
    }

    #[test]
    fn a_loose_home_directory_is_flagged() {
        let mut f = facts();
        f.home_mode = Some(0o755);
        let checks = evaluate(&f);
        let home = check(&checks, "home");
        assert_eq!(home.level, Level::Warn);
        assert!(home.detail.contains("755"));
    }

    #[test]
    fn the_successor_backend_is_flagged_when_the_server_turns_it_on() {
        let mut f = facts();
        f.config = Ok(json!({"cachedGrowthBookFeatures": {"tengu_hover_rest": true}}));
        assert_eq!(check(&evaluate(&f), "storage_v5").level, Level::Warn);
    }

    #[test]
    fn every_failure_and_warning_tells_the_user_something() {
        let mut f = facts();
        f.credential = Ok(Some(json!({"slackTag": {}})));
        f.home_mode = Some(0o755);
        for c in evaluate(&f) {
            if c.level != Level::Ok {
                assert!(!c.advice.is_empty(), "{} has no advice", c.code);
            }
        }
    }

    #[test]
    fn check_codes_are_unique() {
        let checks = evaluate(&facts());
        let mut codes: Vec<&str> = checks.iter().map(|c| c.code).collect();
        codes.sort_unstable();
        let before = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), before);
    }
}
