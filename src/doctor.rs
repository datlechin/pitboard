//! `pitboard doctor`: check, on this machine, that what pitboard relies on about Claude Code
//! still holds, and say which assumption broke when one has. Gathering is kept apart from
//! judging so every judgement can be tested.

use crate::error::Error;
use crate::state::{Park, State};
use crate::ui::{self, BAD, DIM, GOOD, WARN, pad, paint};
use crate::{claude, home, park, slot, store, switch, time, usage};
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
    pub name: String,
    pub level: Level,
    pub detail: String,
    /// What the user should do. Empty when there is nothing to do.
    pub advice: String,
}

/// Everything read from the machine, so judging it touches nothing.
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
    pub machine_id_known: bool,
    /// `CLAUDE_CODE_HOVER_REST`, which switches on the successor credential backend.
    pub hover_rest_env: bool,
    pub state: Result<State, Error>,
    /// Each enrolled account's parked login, read back from the vault.
    pub parks: Vec<ParkFact>,
    pub interrupted: bool,
    pub now: i64,
}

pub struct ParkFact {
    pub label: String,
    pub active: bool,
    pub park: Option<Park>,
    /// Why it cannot be read back, if it cannot.
    pub unreadable: Option<String>,
}

fn park_facts(state: &State) -> Vec<ParkFact> {
    state
        .accounts
        .iter()
        .map(|a| ParkFact {
            label: a.label.clone(),
            active: state.active.as_deref() == Some(a.label.as_str()),
            park: a.parked.clone(),
            unreadable: a.parked.as_ref().and_then(|p| {
                park::load(&a.label, p).err().map(|e| match e {
                    Error::ParkedCredentialMissing { .. } => "missing from the vault".into(),
                    Error::ParkedCredentialCorrupt { detail, .. } => detail,
                    other => other.to_string(),
                })
            }),
        })
        .collect()
}

pub fn gather() -> Facts {
    let config = claude::load_config();
    let state = crate::state::load();
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
        machine_id_known: crate::state::machine_id() != "unknown",
        hover_rest_env: std::env::var("CLAUDE_CODE_HOVER_REST")
            .is_ok_and(|v| v == "1" || v == "true"),
        parks: state.as_ref().map(park_facts).unwrap_or_default(),
        state,
        interrupted: switch::interrupted(),
        service,
        now: crate::time::now(),
    }
}

fn mode_of(path: &std::path::Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(std::fs::metadata(path).ok()?.permissions().mode() & 0o777)
}

fn ok(code: &'static str, name: impl Into<String>, detail: impl Into<String>) -> Check {
    Check {
        code,
        name: name.into(),
        level: Level::Ok,
        detail: detail.into(),
        advice: String::new(),
    }
}
fn warn(
    code: &'static str,
    name: impl Into<String>,
    detail: impl Into<String>,
    advice: impl Into<String>,
) -> Check {
    Check {
        code,
        name: name.into(),
        level: Level::Warn,
        detail: detail.into(),
        advice: advice.into(),
    }
}
fn fail(
    code: &'static str,
    name: impl Into<String>,
    detail: impl Into<String>,
    advice: impl Into<String>,
) -> Check {
    Check {
        code,
        name: name.into(),
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
            (None, _) => ok(
                "usage_cache",
                "usage cache",
                "absent; Claude Code writes it after a call that reports usage",
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
            format!(
                "Park names contain account identifiers, so only you should read it: \
                 `chmod 700 {}`.",
                facts.home.display()
            ),
        ),
    });

    checks.push(match &facts.state {
        Ok(state) => ok(
            "state",
            "accounts",
            match state.accounts.len() {
                0 => "none enrolled yet".to_string(),
                1 => "1 enrolled".to_string(),
                n => format!("{n} enrolled"),
            },
        ),
        Err(e) => fail(
            "state",
            "accounts",
            e.to_string(),
            "pitboard will not switch until its account list can be read.",
        ),
    });
    checks.extend(facts.parks.iter().map(|p| judge_park(p, facts.now)));
    if facts.interrupted {
        checks.push(warn(
            "interrupted_switch",
            "interrupted switch",
            "a switch did not finish",
            "The next `pitboard use`, `enroll` or `forget` finishes it before anything else.",
        ));
    }
    if let Ok(state) = &facts.state
        && !state.discarded.is_empty()
    {
        checks.push(warn(
            "discarded",
            "old parked logins",
            format!("{} waiting to be deleted", state.discarded.len()),
            "pitboard deletes them on its next change; if they stay, check the keychain is unlocked.",
        ));
    }

    if !facts.machine_id_known {
        checks.push(warn(
            "machine_id",
            "machine id",
            "this machine has no stable identifier",
            "pitboard cannot tell this machine from another that also lacks one, so it cannot \
             refuse state copied between them. Never copy ~/.pitboard between machines.",
        ));
    }

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

/// A parked login this close to expiring is worth renewing now.
const RENEW_WITHIN: i64 = 3 * 86_400;

fn judge_park(fact: &ParkFact, now: i64) -> Check {
    let name = format!("account {}", fact.label);
    let renew = format!("Run `pitboard enroll {} --sign-in`.", fact.label);
    let Some(park) = &fact.park else {
        return if fact.active {
            ok(
                "parked_login",
                name,
                "signed in; parked when you switch away",
            )
        } else {
            warn("parked_login", name, "nothing parked to switch to", renew)
        };
    };
    if let Some(why) = &fact.unreadable {
        return fail(
            "parked_login",
            name,
            format!("its parked login is unusable: {why}"),
            renew,
        );
    }
    match park.refresh_expires_at {
        Some(at) if at <= now => warn(
            "parked_login",
            name,
            format!("its parked login expired {}", ui::moment(at, now)),
            renew,
        ),
        Some(at) if at - now < RENEW_WITHIN => warn(
            "parked_login",
            name,
            format!("its parked login expires in {}", time::span(at - now)),
            renew,
        ),
        Some(at) => ok(
            "parked_login",
            name,
            format!("parked, good for {}", time::span(at - now)),
        ),
        None => ok("parked_login", name, "parked"),
    }
}

/// Claude Code's successor credential backend: a stub in every build so far, but compiled
/// in and switched on from the server.
fn judge_storage_v5(facts: &Facts) -> Check {
    let flag_on = facts
        .config
        .as_ref()
        .ok()
        .and_then(|c| c.get("cachedGrowthBookFeatures"))
        .and_then(|f| f.get("tengu_hover_rest"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if facts.hover_rest_env || flag_on {
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

/// The checks, and where Claude Code's files were found, for a program to read rather than
/// parse out of the checks' wording.
pub struct Diagnosis {
    pub checks: Vec<Check>,
    pub environment: Value,
}

pub fn run() -> Diagnosis {
    let facts = gather();
    Diagnosis {
        checks: evaluate(&facts),
        environment: json!({
            "config_file": facts.config_path,
            "storage_dir": facts.storage_dir,
            "credential_service": facts.service,
            "credential_store": facts.backend.as_ref().map_or("unreadable", |b| b.name()),
            "home": facts.home,
        }),
    }
}

/// No check failed. Warnings are advice; a failure means an assumption broke.
pub fn healthy(checks: &[Check]) -> bool {
    checks.iter().all(|c| c.level != Level::Fail)
}

pub fn render_human(checks: &[Check]) -> String {
    let width = checks
        .iter()
        .map(|c| c.name.chars().count())
        .max()
        .unwrap_or(0);
    let mut out = String::new();
    for c in checks {
        let mark = match c.level {
            Level::Ok => paint(GOOD, "✓"),
            Level::Warn => paint(WARN, "!"),
            Level::Fail => paint(BAD, "✗"),
        };
        out.push_str(&format!("{mark} {}  {}\n", pad(&c.name, width), c.detail));
        if !c.advice.is_empty() {
            out.push_str(&format!(
                "  {}  {}\n",
                pad("", width),
                paint(DIM, &c.advice)
            ));
        }
    }
    let count = |level| checks.iter().filter(|c| c.level == level).count();
    let summary = match (count(Level::Fail), count(Level::Warn)) {
        (0, 0) => paint(GOOD, "Everything pitboard relies on holds."),
        (0, w) => paint(WARN, format!("{w} to look at; nothing is broken.")),
        (f, _) => paint(
            BAD,
            format!("{f} broken: do not switch accounts until fixed."),
        ),
    };
    out.push_str(&format!("\n{summary}\n"));
    out
}

pub fn render_json(diagnosis: &Diagnosis) -> Value {
    json!({
        "environment": diagnosis.environment,
        "checks": diagnosis.checks.iter().map(|c| json!({
            "code": c.code,
            "name": c.name,
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
            machine_id_known: true,
            hover_rest_env: false,
            state: Ok(State::default()),
            parks: Vec::new(),
            interrupted: false,
            now: NOW,
        }
    }

    const NOW: i64 = 1_789_935_600;

    fn parked(label: &str, refresh_expires_at: Option<i64>) -> ParkFact {
        ParkFact {
            label: label.into(),
            active: false,
            park: Some(Park {
                service: format!("pitboard-park-{label}-1"),
                parked_at: NOW - 86_400,
                refresh_fingerprint: "f".into(),
                access_expires_at: None,
                refresh_expires_at,
            }),
            unreadable: None,
        }
    }

    fn named<'a>(checks: &'a [Check], name: &str) -> &'a Check {
        checks.iter().find(|c| c.name == name).expect(name)
    }

    fn check<'a>(checks: &'a [Check], code: &str) -> &'a Check {
        checks.iter().find(|c| c.code == code).expect(code)
    }

    #[test]
    fn a_healthy_machine_reports_no_failures() {
        let checks = evaluate(&facts());
        assert!(checks.iter().all(|c| c.level != Level::Fail));
        assert!(healthy(&checks));
    }

    #[test]
    fn a_credential_without_claude_ai_oauth_is_a_failure_not_a_warning() {
        let mut f = facts();
        f.credential = Ok(Some(json!({"slackTag": {}})));
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "credential").level, Level::Fail);
        assert!(!healthy(&checks));
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
    fn the_successor_backend_is_flagged_when_the_environment_turns_it_on() {
        let mut f = facts();
        assert_eq!(check(&evaluate(&f), "storage_v5").level, Level::Ok);
        f.hover_rest_env = true;
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
    fn a_machine_without_a_stable_identifier_is_flagged() {
        let mut f = facts();
        f.machine_id_known = false;
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "machine_id").level, Level::Warn);
        assert!(evaluate(&facts()).iter().all(|c| c.code != "machine_id"));
    }

    #[test]
    fn each_check_is_told_apart_by_code_and_name() {
        let mut f = facts();
        f.parks = vec![parked("work", None), parked("personal", None)];
        let checks = evaluate(&f);
        let mut keys: Vec<(&str, &str)> =
            checks.iter().map(|c| (c.code, c.name.as_str())).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), before);
    }

    #[test]
    fn every_parked_login_is_judged_and_a_way_back_is_offered() {
        let mut f = facts();
        let mut unusable = parked("broken", Some(NOW + 30 * 86_400));
        unusable.unreadable = Some("missing from the vault".into());
        f.parks = vec![
            parked("fine", Some(NOW + 20 * 86_400)),
            parked("soon", Some(NOW + 86_400)),
            parked("gone", Some(NOW - 1)),
            unusable,
            ParkFact {
                label: "empty".into(),
                active: false,
                park: None,
                unreadable: None,
            },
            ParkFact {
                label: "live".into(),
                active: true,
                park: None,
                unreadable: None,
            },
        ];
        let checks = evaluate(&f);
        for (name, level) in [
            ("account fine", Level::Ok),
            ("account soon", Level::Warn),
            ("account gone", Level::Warn),
            ("account broken", Level::Fail),
            ("account empty", Level::Warn),
            ("account live", Level::Ok),
        ] {
            let c = named(&checks, name);
            assert_eq!(c.level, level, "{name}: {}", c.detail);
            if level != Level::Ok {
                let label = name.trim_start_matches("account ");
                assert!(
                    c.advice
                        .contains(&format!("pitboard enroll {label} --sign-in"))
                );
            }
        }
    }

    #[test]
    fn an_interrupted_switch_and_leftover_parks_are_reported() {
        let mut f = facts();
        f.interrupted = true;
        f.state = Ok(State {
            discarded: vec!["pitboard-park-x-1".into()],
            ..State::default()
        });
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "interrupted_switch").level, Level::Warn);
        assert_eq!(check(&checks, "discarded").level, Level::Warn);
        assert!(healthy(&checks), "neither stops pitboard working");
    }

    #[test]
    fn the_summary_says_whether_anything_is_broken() {
        let plain =
            |checks: &[Check]| anstream::adapter::strip_str(&render_human(checks)).to_string();
        assert!(plain(&evaluate(&facts())).ends_with("Everything pitboard relies on holds.\n"));
        let mut f = facts();
        f.credential = Ok(Some(json!({"slackTag": {}})));
        assert!(plain(&evaluate(&f)).contains("1 broken"));
    }
}
