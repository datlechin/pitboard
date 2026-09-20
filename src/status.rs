//! `pitboard status` — what account is signed in and how much of it is left.
//!
//! Every number shown here comes from something Claude Code already wrote to disk.
//! No network, no secret read, nothing written.

use crate::{claude, store, time, usage};
use serde_json::{Value, json};

pub struct Report {
    pub identity: Option<claude::Identity>,
    pub snapshot: Option<usage::Snapshot>,
    /// The cache was measured for a different account than the one signed in now.
    pub snapshot_is_foreign: bool,
    pub backend: Result<store::Backend, String>,
    pub service: String,
    pub config_file: String,
}

pub fn gather() -> Report {
    let config = claude::load_config().ok();
    let identity = config.as_ref().and_then(claude::identity);
    let snapshot = config.as_ref().and_then(usage::from_config_cache);
    let snapshot_is_foreign = match (&identity, &snapshot) {
        (Some(id), Some(s)) => s
            .account_uuid
            .as_ref()
            .is_some_and(|u| *u != id.account_uuid),
        _ => false,
    };
    let service = claude::live_service();
    Report {
        identity,
        snapshot,
        snapshot_is_foreign,
        backend: store::resolve(&service).map_err(|e| e.to_string()),
        service,
        config_file: claude::config_file().display().to_string(),
    }
}

fn label(kind: &str, scope: Option<&str>) -> String {
    let base = match kind {
        "session" | "five_hour" => "5h",
        "weekly_all" | "seven_day" => "weekly",
        "weekly_scoped" => "weekly",
        other => other,
    };
    match scope {
        Some(s) => format!("{base} ({s})"),
        None => base.to_string(),
    }
}

pub fn render_human(
    r: &Report,
    accounts: &[crate::state::Account],
    active: Option<&str>,
) -> String {
    let mut out = String::new();
    match &r.identity {
        Some(id) => {
            out.push_str(&format!("  account   {}\n", id.email));
            let tier = id
                .rate_limit_tier
                .as_deref()
                .unwrap_or("unknown tier")
                .replace("default_claude_", "")
                .replace('_', " ");
            out.push_str(&format!(
                "            {}  ·  org {}\n",
                tier,
                id.organization_uuid.get(..8).unwrap_or("?")
            ));
        }
        None => out.push_str("  account   not signed in\n"),
    }

    match &r.snapshot {
        Some(s) if r.snapshot_is_foreign => {
            out.push_str(
                "\n  usage     the cached numbers belong to a different account, so they are not shown\n",
            );
            let _ = s;
        }
        Some(s) if s.windows.is_empty() => {
            out.push_str("\n  usage     no window has been measured yet in this session\n");
        }
        Some(s) => {
            out.push('\n');
            let now = time::now();
            for w in &s.windows {
                if w.percent == 0.0 && w.scope.is_some() {
                    continue; // an untouched per-model window is noise
                }
                let resets = w
                    .resets_at
                    .map(|t| format!("resets {}", time::humanise_until(t, now)))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "  {:<9} {}  {:>3.0}%   {}\n",
                    label(&w.kind, w.scope.as_deref()),
                    usage::bar(w.percent, 10),
                    w.percent,
                    resets
                ));
            }
            if let Some(at) = s.observed_at {
                out.push_str(&format!(
                    "\n  measured  {}, from Claude Code's own cache\n",
                    time::format_local(at, "%H:%M")
                ));
            }
        }
        None => out.push_str("\n  usage     nothing measured yet\n"),
    }

    let backend = match &r.backend {
        Ok(store::Backend::Keychain) => format!("keychain  ·  {}", r.service),
        Ok(store::Backend::File) => format!("file  ·  {}", store::credential_file().display()),
        Ok(store::Backend::Absent) => "no credential found".to_string(),
        Err(e) => format!("unreadable  ·  {e}"),
    };
    out.push_str(&format!("\n  store     {backend}\n"));
    out.push_str(&format!("  config    {}\n", r.config_file));

    if !accounts.is_empty() {
        out.push('\n');
        for a in accounts {
            let mark = if Some(a.label.as_str()) == active {
                "*"
            } else {
                " "
            };
            let parked = match a.newest() {
                Some(g) => format!("parked {}", time::format_local(g.parked_at, "%d %b %H:%M")),
                None => "not parked".to_string(),
            };
            out.push_str(&format!(
                "{mark} {:<10} {:<34} {}\n",
                a.label, a.email, parked
            ));
        }
    }
    out
}

pub fn render_json(r: &Report, accounts: &[crate::state::Account], active: Option<&str>) -> Value {
    json!({
        "schema": 1,
        "account": r.identity.as_ref().map(|id| json!({
            "email": id.email,
            "account_uuid": id.account_uuid,
            "organization_uuid": id.organization_uuid,
            "organization_name": id.organization_name,
            "rate_limit_tier": id.rate_limit_tier,
        })),
        "usage": r.snapshot.as_ref().map(|s| json!({
            "observed_at": s.observed_at,
            "source": "config_cache",
            "belongs_to_active_account": !r.snapshot_is_foreign,
            "windows": s.windows.iter().map(|w| json!({
                "kind": w.kind, "scope": w.scope, "percent": w.percent,
                "resets_at": w.resets_at, "is_active": w.is_active,
            })).collect::<Vec<_>>(),
        })),
        "store": {
            "backend": match &r.backend {
                Ok(store::Backend::Keychain) => "keychain",
                Ok(store::Backend::File) => "file",
                Ok(store::Backend::Absent) => "absent",
                Err(_) => "unreadable",
            },
            "service": r.service,
            "error": r.backend.as_ref().err(),
        },
        "config_file": r.config_file,
        "accounts": accounts.iter().map(|a| json!({
            "label": a.label,
            "email": a.email,
            "account_uuid": a.account_uuid,
            "active": Some(a.label.as_str()) == active,
            "parked_at": a.newest().map(|g| g.parked_at),
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_collapse_claude_codes_vocabulary_to_two_words() {
        assert_eq!(label("session", None), "5h");
        assert_eq!(label("five_hour", None), "5h");
        assert_eq!(label("weekly_all", None), "weekly");
        assert_eq!(label("weekly_scoped", Some("Fable")), "weekly (Fable)");
        assert_eq!(label("something_new", None), "something_new");
    }
}
