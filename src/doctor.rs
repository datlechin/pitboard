//! `pitboard doctor` — prove, on this machine right now, that what this tool believes
//! about Claude Code is still true.
//!
//! This exists because the tool depends on behaviour Anthropic never documented. The
//! honest response to that is not to hope, but to check every assumption on every run
//! and say plainly which one broke.

use crate::{claude, slot, store, usage};
use serde_json::{Value, json};

#[derive(PartialEq, Clone, Copy)]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

pub struct Check {
    pub name: &'static str,
    pub level: Level,
    pub detail: String,
    /// What the user should do. Empty when there is nothing to do.
    pub advice: String,
}

fn ok(name: &'static str, detail: impl Into<String>) -> Check {
    Check {
        name,
        level: Level::Ok,
        detail: detail.into(),
        advice: String::new(),
    }
}
fn warn(name: &'static str, detail: impl Into<String>, advice: impl Into<String>) -> Check {
    Check {
        name,
        level: Level::Warn,
        detail: detail.into(),
        advice: advice.into(),
    }
}
fn fail(name: &'static str, detail: impl Into<String>, advice: impl Into<String>) -> Check {
    Check {
        name,
        level: Level::Fail,
        detail: detail.into(),
        advice: advice.into(),
    }
}

pub fn run() -> Vec<Check> {
    let mut checks = Vec::new();

    // The one external binary we depend on. Everything else is downstream of it.
    checks.push(match std::fs::metadata("/usr/bin/security") {
        Ok(_) => ok("security tool", "/usr/bin/security"),
        Err(e) => fail(
            "security tool",
            format!("/usr/bin/security is missing: {e}"),
            "This tool reads the keychain the same way Claude Code does. Without it, nothing works.",
        ),
    });

    let config = claude::load_config();
    let path = claude::config_file().display().to_string();
    checks.push(match &config {
        Ok(v) => ok(
            "config file",
            format!("{path}  ({} keys)", v.as_object().map_or(0, |o| o.len())),
        ),
        Err(e) => fail(
            "config file",
            e.to_string(),
            "Claude Code may not have run on this machine yet.",
        ),
    });

    let identity = config.as_ref().ok().and_then(claude::identity);
    checks.push(match &identity {
        Some(id) => ok(
            "identity",
            format!("{}  ·  org {}", id.email, id.organization_uuid),
        ),
        None => warn(
            "identity",
            "no oauthAccount in the config",
            "Claude Code writes this after its first successful call. Run `claude` once.",
        ),
    });

    // Which slot this process would read, and whether the derivation still holds.
    let service = claude::live_service();
    // The account name is half the lookup: a $USER that fails Claude Code's filter falls
    // back to a literal, and without showing it a wrong derivation looks like an empty store.
    let account = slot::account_name();
    checks.push(if claude::is_default_slot() {
        ok(
            "slot",
            format!("default  ·  {service}  ·  account {account}"),
        )
    } else {
        ok(
            "slot",
            format!(
                "{service}  ·  account {account}  (selected by {})",
                claude::storage_dir()
            ),
        )
    });

    checks.push(match store::resolve(&service) {
        Ok(store::Backend::Keychain) => ok("credential store", "keychain"),
        Ok(store::Backend::File) => warn(
            "credential store",
            format!("plaintext file  ·  {}", store::credential_file().display()),
            "Claude Code fell back to a file, which means a keychain write failed at some point.",
        ),
        Ok(store::Backend::Absent) => warn(
            "credential store",
            "no credential in either backend",
            "Nothing is signed in for this slot.",
        ),
        Err(e) => fail(
            "credential store",
            e.to_string(),
            "The store could not be read. Treat this as unknown, never as empty.",
        ),
    });

    checks.push(match store::read(&service) {
        Ok(Some(doc)) => {
            let keys: Vec<&str> = doc
                .as_object()
                .map(|o| o.keys().map(String::as_str).collect())
                .unwrap_or_default();
            match doc.get("claudeAiOauth").and_then(Value::as_object) {
                Some(o) => {
                    let fp = o
                        .get("refreshToken")
                        .and_then(Value::as_str)
                        .map(store::fingerprint)
                        .unwrap_or_else(|| "none".into());
                    let expires = o.get("refreshTokenExpiresAt").and_then(Value::as_i64);
                    let left = expires
                        .map(|ms| (ms / 1000 - crate::time::now()) / 86_400)
                        .unwrap_or(-1);
                    let detail =
                        format!("refresh {fp}  ·  {left} days left  ·  document keys {keys:?}");
                    if left < 3 {
                        warn(
                            "credential",
                            detail,
                            "This login expires soon and will need signing in again.",
                        )
                    } else {
                        ok("credential", detail)
                    }
                }
                None => fail(
                    "credential",
                    format!("document has no claudeAiOauth, keys are {keys:?}"),
                    "The credential shape changed.",
                ),
            }
        }
        Ok(None) => warn(
            "credential",
            "nothing stored",
            "Nothing is signed in for this slot.",
        ),
        Err(e) => fail(
            "credential",
            e.to_string(),
            "Do not write to the store while this is failing.",
        ),
    });

    checks.push(match (config.as_ref().ok().and_then(usage::from_config_cache), &identity) {
        (Some(s), Some(id)) if s.account_uuid.as_deref() == Some(id.account_uuid.as_str()) => ok(
            "usage cache",
            format!("{} windows, measured for this account", s.windows.len()),
        ),
        (Some(_), Some(_)) => warn(
            "usage cache",
            "cached for a different account",
            "Numbers from it are ignored rather than shown, which is why status may look empty.",
        ),
        (Some(s), None) => ok("usage cache", format!("{} windows", s.windows.len())),
        (None, _) => warn("usage cache", "absent", "Claude Code writes it after a call that reports usage."),
    });

    // The successor credential backend. It is a stub in every shipped build so far,
    // but it is compiled in and server-gated, so it is worth watching for.
    checks.push({
        let env_on = std::env::var("CLAUDE_CODE_HOVER_REST").is_ok_and(|v| v == "1" || v == "true");
        let flag_on = config
            .as_ref()
            .ok()
            .and_then(|c| c.get("cachedGrowthBookFeatures").cloned())
            .and_then(|f| f.get("tengu_hover_rest").and_then(Value::as_bool))
            .unwrap_or(false);
        if env_on || flag_on {
            warn(
                "storage v5",
                "the successor credential backend is switched on",
                "This tool has not been verified against it. Check for an update before switching.",
            )
        } else {
            ok("storage v5", "inactive")
        }
    });

    checks
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
            "name": c.name,
            "level": match c.level { Level::Ok => "ok", Level::Warn => "warn", Level::Fail => "fail" },
            "detail": c.detail,
            "advice": c.advice,
        })).collect::<Vec<_>>(),
    })
}
