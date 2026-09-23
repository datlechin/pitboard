//! `pitboard doctor`: check, on this machine, that what pitboard relies on about Claude Code
//! still holds, and say which assumption broke when one has. Gathering is kept apart from
//! judging so every judgement can be tested.

use crate::context::Context;
use crate::error::Error;
use crate::provider::claude::daemon;
use crate::provider::claude::paths as claude;
use crate::provider::claude::slot;
use crate::state::{Park, State};
use crate::{home, park, store, switch, time, usage};
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
    /// What the live login costs against the store's ceiling, where there is one.
    pub credential_cost: Option<store::Cost>,
    /// What is taking up the room, largest first: (what it is, bytes).
    pub credential_parts: Vec<(String, usize)>,
    pub home: PathBuf,
    pub home_mode: Option<u32>,
    /// Anything on this disk holding a login that somebody other than the owner can read:
    /// (path, mode). Empty on a machine with a keychain and a tidy plaintext fallback,
    /// and the whole security story on a machine without one.
    pub readable_by_others: Vec<(String, u32)>,
    pub machine_id_known: bool,
    /// `CLAUDE_CODE_HOVER_REST`, which switches on the successor credential backend.
    pub hover_rest_env: bool,
    /// Claude Code's supervisor daemon, where one has ever run for this slot.
    pub daemon: Option<daemon::Daemon>,
    /// Names pitboard wrote down before creating a park and has not resolved yet.
    pub pending_parks: Vec<String>,
    /// Which Claude Code is installed here, read off disk.
    pub claude_version: Option<String>,
    /// Every reason a session here would authenticate as something other than the stored
    /// login, read from settings files as well as from this process's environment.
    pub auth_overrides: Vec<crate::settings::Override>,
    /// Accounts pitboard is not asking Anthropic about yet, and for how long: (uuid, seconds).
    pub asking_held: Vec<(String, i64)>,
    pub state: Result<State, Error>,
    /// Each enrolled account's parked login, read back from the vault.
    pub parks: Vec<ParkFact>,
    pub interrupted: bool,
    pub now: i64,
}

pub struct ParkFact {
    pub label: String,
    pub active: bool,
    /// When this account was last switched to, where that is recorded.
    pub last_used_at: Option<i64>,
    pub park: Option<Park>,
    /// Why it cannot be read back, if it cannot.
    pub unreadable: Option<String>,
}

fn park_facts(ctx: &Context, state: &State, live_uuid: Option<&str>) -> Vec<ParkFact> {
    state
        .accounts
        .iter()
        .map(|a| ParkFact {
            label: a.label.clone(),
            last_used_at: a.last_used_at,
            // What Claude Code's config says, when it says anything. pitboard's own
            // record of its last switch says nothing about a sign-in made elsewhere.
            active: match live_uuid {
                Some(uuid) => a.account_uuid == uuid,
                None => state.active.as_deref() == Some(a.label.as_str()),
            },
            park: a.parked.clone(),
            unreadable: a.parked.as_ref().and_then(|p| {
                park::load(ctx, &a.label, p).err().map(|e| match e {
                    Error::ParkedCredentialMissing { .. } => "missing from the vault".into(),
                    Error::ParkedCredentialCorrupt { detail, .. } => detail,
                    other => other.to_string(),
                })
            }),
        })
        .collect()
}

pub fn gather(ctx: &Context) -> Facts {
    let config = claude::load_config(ctx);
    let state = crate::state::load(ctx);
    let service = claude::live_service(ctx);
    let home = home::dir(ctx);
    let identity = config.as_ref().ok().and_then(claude::identity);
    Facts {
        security_tool: cfg!(target_os = "macos")
            .then(|| store::SECURITY.to_string())
            .filter(|p| std::fs::metadata(p).is_ok()),
        config_path: claude::config_file(ctx),
        identity: identity.clone(),
        config,
        account: slot::account_name(ctx),
        default_slot: claude::is_default_slot(ctx),
        storage_dir: claude::storage_dir(ctx),
        backend: store::resolve(ctx, &service),
        credential_file: store::credential_file(ctx),
        credential_cost: store::read_raw(ctx, &service)
            .ok()
            .flatten()
            .and_then(|raw| store::cost(ctx, &service, &raw)),
        credential_parts: store::read(ctx, &service)
            .ok()
            .flatten()
            .map(|doc| parts_of(&doc))
            .unwrap_or_default(),
        credential: store::read(ctx, &service),
        home_mode: mode_of(&home),
        readable_by_others: loose_logins(ctx),
        home,
        machine_id_known: crate::state::machine_id() != "unknown",
        hover_rest_env: ctx.hover_rest,
        daemon: daemon::read(ctx),
        pending_parks: crate::pending::outstanding(ctx),
        claude_version: claude::installed_version(ctx),
        auth_overrides: crate::settings::overrides(ctx),
        asking_held: crate::budget::holds(ctx),
        parks: state
            .as_ref()
            .map(|s| park_facts(ctx, s, identity.as_ref().map(|i| i.account_uuid.as_str())))
            .unwrap_or_default(),
        state,
        interrupted: switch::interrupted(ctx),
        service,
        now: ctx.now(),
    }
}

/// What is taking up the room in a credential document, largest first.
///
/// A login that will not fit is almost never the login: on one real machine the OAuth block
/// was 506 bytes and eleven MCP server tokens were 3679. Saying "8503 of 4032 bytes" leaves
/// a person to guess which of those to do something about, and the answer is in the
/// document pitboard has already read.
fn parts_of(document: &Value) -> Vec<(String, usize)> {
    let weigh = |value: &Value| serde_json::to_string(value).map(|s| s.len()).unwrap_or(0);
    let Some(root) = document.as_object() else {
        return Vec::new();
    };
    let mut parts: Vec<(String, usize)> = root
        .iter()
        .flat_map(|(key, value)| match (key.as_str(), value.as_object()) {
            // The usual culprit, and the one a person can act on server by server.
            ("mcpOAuth", Some(servers)) if servers.len() > 1 => servers
                .iter()
                .map(|(server, held)| (format!("mcpOAuth {server}"), weigh(held)))
                .collect(),
            _ => vec![(key.clone(), weigh(value))],
        })
        .collect();
    parts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    parts
}

fn mode_of(path: &std::path::Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(std::fs::metadata(path).ok()?.permissions().mode() & 0o777)
}

/// Every file on this machine that holds a usable login and is not private to its owner.
///
/// Claude Code has no keyring backend outside macOS and Windows, so on Linux its own login
/// is a plaintext file it chmods to 0600, and pitboard's parked logins are plaintext files
/// beside it. That is not pitboard weakening anything, but it does mean the only thing
/// between a parked OAuth token and everyone else with an account on the machine is a mode
/// bit, and a mode bit is something a backup restore, a `cp`, an rsync or a careless umask
/// quietly changes. So it is looked at rather than assumed.
fn loose_logins(ctx: &Context) -> Vec<(String, u32)> {
    // Group and other, read or write. Anything there is somebody who is not the owner.
    const SHARED: u32 = 0o077;
    let mut loose = Vec::new();
    let mut look = |path: std::path::PathBuf| {
        if let Some(mode) = mode_of(&path)
            && mode & SHARED != 0
        {
            loose.push((path.display().to_string(), mode));
        }
    };
    look(store::credential_file(ctx));
    let vault = store::vault_dir(ctx);
    look(vault.clone());
    if let Ok(entries) = std::fs::read_dir(&vault) {
        let mut parks: Vec<std::path::PathBuf> = entries.flatten().map(|e| e.path()).collect();
        parks.sort();
        for park in parks {
            look(park);
        }
    }
    loose
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
                format!("{} is missing", store::SECURITY),
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

    checks.push(ok(
        "slot",
        "slot",
        if facts.default_slot {
            format!(
                "default  ·  {}  ·  account {}",
                facts.service, facts.account
            )
        } else {
            format!(
                "{}  ·  account {}  (selected by {})",
                facts.service, facts.account, facts.storage_dir
            )
        },
    ));

    // A login that grows past the ceiling cannot be switched at all, and it grows by things
    // done elsewhere, so it is worth saying before the day it refuses.
    if let Some(price) = facts.credential_cost {
        let (bytes, limit) = (price.needs, price.limit);
        checks.push(if price.refused() {
            // The refusal was asked for. Say what it will cost when the day comes rather
            // than only on the day itself.
            fail(
                "credential_size",
                "login size",
                format!(
                    "{bytes} of {limit} bytes, and PITBOARD_NO_ARGV refuses that{}",
                    biggest(&facts.credential_parts)
                ),
                "There is no third way to write a login this size. Sign out of MCP servers \
                 you no longer use to make it smaller, or unset PITBOARD_NO_ARGV and accept \
                 the argument-line write.",
            )
        } else if price.over() {
            // Not a fault: `security` takes this much of a command from stdin and no more,
            // and the argument line is the only other way it offers. Claude Code writes
            // this same login that way itself on every refresh.
            warn(
                "credential_size",
                "login size",
                format!(
                    "{bytes} of {limit} bytes: written on the argument line{}",
                    biggest(&facts.credential_parts)
                ),
                "A login this size can only be written by passing it as an argument, where \
                 a process running as you could read it while the call lasts. Signing out \
                 of MCP servers you no longer use makes it smaller; PITBOARD_NO_ARGV=1 \
                 refuses the switch instead.",
            )
        } else {
            ok(
                "credential_size",
                "login size",
                format!("{bytes} of {limit} bytes"),
            )
        });
    }

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
        // The one wrong diagnosis in this file. If Claude Code's config names somebody as
        // signed in, "nothing is signed in" is not an observation, it is pitboard looking
        // in the wrong place, and it is the failure that would follow Claude Code moving
        // where it keeps a login.
        Ok(store::Backend::Absent) if facts.identity.is_some() => fail(
            "credential_store",
            "credential store",
            format!(
                "Claude Code's config says {} is signed in, and no store pitboard reads \
                 holds that login",
                facts
                    .identity
                    .as_ref()
                    .map(|i| i.email.as_str())
                    .unwrap_or("somebody")
            ),
            "pitboard will not write a login where nobody reads it. Check for a pitboard \
             update; if there is none, this is worth reporting.",
        ),
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

    checks.push(if facts.readable_by_others.is_empty() {
        ok(
            "private_on_disk",
            "logins on disk",
            "nothing holding a login is readable by anyone else",
        )
    } else {
        let names: Vec<String> = facts
            .readable_by_others
            .iter()
            .map(|(path, mode)| format!("{path} is mode {mode:o}"))
            .collect();
        fail(
            "private_on_disk",
            "logins on disk",
            names.join("; "),
            format!(
                "These hold usable OAuth tokens in plain text, which is how Claude Code                  stores them where there is no keychain. Anyone else on this machine can                  read them: `chmod go-rwx {}`.",
                facts
                    .readable_by_others
                    .iter()
                    .map(|(path, _)| path.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        )
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
            match e {
                // The one unreadable state that has a command of its own.
                Error::StateWrongMachine { .. } => {
                    "Run `pitboard adopt` to keep these accounts on this computer. The \
                     logins they came with are dropped, because a login belongs to the \
                     computer that signed in."
                }
                _ => "pitboard will not switch until its account list can be read.",
            },
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
    checks.push(judge_daemon(facts));
    checks.push(judge_pending(facts));
    checks.push(judge_claude_version(facts));
    checks.push(judge_auth(facts));
    checks.push(judge_asking(facts));
    checks.extend(
        facts
            .parks
            .iter()
            .filter_map(|p| judge_dormant(p, facts.now)),
    );
    checks
}

/// A refresh token's own life, from a real renewal answer: thirty days. An account that has
/// been parked for longer than that without being switched to has had its login kept alive
/// purely by pitboard, through at least one whole token lifetime, for nobody.
const A_TOKEN_LIFETIME: i64 = 30 * 86_400;

/// An account nobody has come back to.
///
/// pitboard renews a parked login for as long as the account is enrolled, so one enrolled
/// once and never used again keeps a live, continuously rotated refresh token on this
/// machine indefinitely. That is a defensible thing to do and an indefensible thing to do
/// silently. Nothing is dropped on a timer pitboard chose: the threshold here is the
/// token's own lifetime, and all it does is say so.
fn judge_dormant(park: &ParkFact, now: i64) -> Option<Check> {
    if park.active || park.park.is_none() {
        return None;
    }
    let dormant_for = now - park.last_used_at?;
    if dormant_for < A_TOKEN_LIFETIME {
        return None;
    }
    Some(warn(
        "dormant_account",
        format!("account {}", park.label),
        format!(
            "not switched to for {}; pitboard has kept its login alive that whole time",
            time::span(dormant_for)
        ),
        format!(
            "Every `pitboard` renews it, so its refresh token is rotated and kept live on \
             this machine for as long as it stays enrolled. If you are not coming back to \
             it, `pitboard forget {}` deletes the login and the record.",
            park.label
        ),
    ))
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
pub const RENEW_WITHIN: i64 = 3 * 86_400;

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
            format!("its parked login expired {}", time::moment(at, now)),
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

/// Claude Code's successor credential backend.
///
/// Measured in 2.1.278: the flag does not move the login out of the keychain. The live
/// chain is built as keychain-with-plaintext-fallback either way, and the flag only decides
/// what backs the fallback half, and only for a caller that hands a backend in. An ordinary
/// `claude` hands none in, so the fallback stays `<storage dir>/.credentials.json`. The
/// combination worth saying something about is the flag on *and* the login living in the
/// fallback, because that is the one case where what pitboard reads may not be what a
/// session reads.
fn judge_storage_v5(facts: &Facts) -> Check {
    let flag_on = facts
        .config
        .as_ref()
        .ok()
        .and_then(|c| c.get("cachedGrowthBookFeatures"))
        .and_then(|f| f.get("tengu_hover_rest"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !(facts.hover_rest_env || flag_on) {
        return ok("storage_v5", "storage v5", "inactive");
    }
    match facts.backend {
        Ok(store::Backend::File) => warn(
            "storage_v5",
            "storage v5",
            "switched on, and this login is in the fallback store",
            "pitboard reads the plaintext file. If Claude Code was given a backend of its \
             own, that is not the same file. Check for an update before switching.",
        ),
        _ => ok(
            "storage_v5",
            "storage v5",
            "switched on; the keychain is still where the login is",
        ),
    }
}

/// The three things taking up the most room, for a check that would otherwise leave a
/// person guessing which of them to do something about.
fn biggest(parts: &[(String, usize)]) -> String {
    let named: Vec<String> = parts
        .iter()
        .take(3)
        .map(|(what, bytes)| format!("{what} {bytes}"))
        .collect();
    if named.is_empty() {
        String::new()
    } else {
        format!(". Mostly: {}", named.join(", "))
    }
}

/// Whether pitboard is waiting before asking Anthropic about anything.
///
/// Ordinarily nothing is waiting: an account is asked about whenever its tightest limit
/// could have moved by a percentage point, and that is the floor rather than a wait. A wait
/// means Anthropic asked for less traffic or could not be reached, and a person watching a
/// number not move deserves to know which.
fn judge_asking(facts: &Facts) -> Check {
    match facts.asking_held.len() {
        0 => ok("asking", "asking Anthropic", "nothing is being held back"),
        n => {
            let longest = facts
                .asking_held
                .iter()
                .map(|(_, seconds)| *seconds)
                .max()
                .unwrap_or_default();
            warn(
                "asking",
                "asking Anthropic",
                format!(
                    "{n} account(s) not being asked about for up to {}",
                    time::span(longest)
                ),
                "Anthropic asked for less traffic, or could not be reached. The numbers \
                 shown are the last ones measured until then; `pitboard status --fresh` \
                 does not override a wait Anthropic asked for.",
            )
        }
    }
}

/// Whether moving the stored login would change anything a session sees.
///
/// Claude Code resolves this from layered settings, so a managed policy or a line in a
/// person's own `settings.json` can make every switch pitboard performs a no-op. Read from
/// files rather than from this process's environment, because an app launched from Finder
/// has no environment to read and is the surface most likely to be used on a machine that
/// needs the answer.
fn judge_auth(facts: &Facts) -> Check {
    if facts.auth_overrides.is_empty() {
        return ok(
            "auth_source",
            "what a session authenticates with",
            "the stored login, which is what pitboard moves",
        );
    }
    let named: Vec<String> = facts
        .auth_overrides
        .iter()
        .map(ToString::to_string)
        .collect();
    warn(
        "auth_source",
        "what a session authenticates with",
        format!("something else: {}", named.join("; ")),
        "Claude Code here authenticates with that rather than with the stored login, so \
         switching accounts changes nothing a session would notice. Remove it, or accept \
         that pitboard is moving a login nothing reads.",
    )
}

/// Which Claude Code is installed, against which one pitboard's facts were read.
///
/// Stated rather than warned about. Claude Code ships several times a week, so a mismatch
/// is the ordinary state of the world within days of a release and warning about it would
/// be noise on every machine. What is worth a warning is an assumption that has actually
/// stopped holding, which is a probe's job and not a version number's.
fn judge_claude_version(facts: &Facts) -> Check {
    let verified = crate::assumptions::VERIFIED_AGAINST;
    match facts.claude_version.as_deref() {
        None => ok(
            "claude_version",
            "Claude Code build",
            format!("not found here; pitboard's facts were read from {verified}"),
        ),
        Some(installed) if installed == verified => ok(
            "claude_version",
            "Claude Code build",
            format!("{installed}, which is what pitboard's facts were read from"),
        ),
        Some(installed) => ok(
            "claude_version",
            "Claude Code build",
            format!("{installed} installed; pitboard's facts were read from {verified}"),
        ),
    }
}

/// A name written down before a park was created, still unresolved. Ordinarily there is
/// nothing here: the next change resolves every one of them. What is left is an item that
/// could not be read, which on macOS is a locked keychain and nothing worse.
fn judge_pending(facts: &Facts) -> Check {
    match facts.pending_parks.len() {
        0 => ok("pending_parks", "parks being reclaimed", "none outstanding"),
        n => warn(
            "pending_parks",
            "parks being reclaimed",
            format!("{n} could not be read this time"),
            "pitboard wrote these names down before creating a login in them and cannot \
             read them back to find out what is there. Unlock the keychain and run any \
             pitboard command; it resolves them before doing anything else.",
        ),
    }
}

/// Claude Code's supervisor daemon is a second writer of the login, on a schedule nobody
/// typed. It takes the same write lock, so it cannot write underneath a switch, but a
/// person reading a diagnosis should be able to see that it is there.
fn judge_daemon(facts: &Facts) -> Check {
    let Some(d) = &facts.daemon else {
        return ok("claude_daemon", "Claude Code daemon", "none has run here");
    };
    let version = d
        .version
        .as_deref()
        .map(|v| format!("Claude Code {v}"))
        .unwrap_or_else(|| "an unrecorded version".into());
    if d.running {
        ok(
            "claude_daemon",
            "Claude Code daemon",
            format!("running, {version}, pid {}", d.pid),
        )
    } else {
        ok(
            "claude_daemon",
            "Claude Code daemon",
            format!("not running; {version} ran here last"),
        )
    }
}

/// The checks, and where Claude Code's files were found, for a program to read rather than
/// parse out of the checks' wording.
pub struct Diagnosis {
    pub checks: Vec<Check>,
    pub environment: Value,
    /// What must not leave this machine in a report. A person reading their own diagnosis
    /// should see their own email; the thing they paste somewhere else should not carry it.
    pub redaction: crate::redact::Sheet,
}

pub fn run(ctx: &Context) -> Diagnosis {
    let facts = gather(ctx);
    Diagnosis {
        redaction: redaction_for(ctx, &facts),
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

/// Everything in a diagnosis that names a person or an account.
///
/// The salt is this machine and this moment, so identifiers line up inside one report and
/// two reports from one machine do not line up with each other. It is never printed.
fn redaction_for(ctx: &Context, facts: &Facts) -> crate::redact::Sheet {
    let mut sheet = crate::redact::Sheet::new(
        format!("{}:{}", crate::state::machine_id(), ctx.now_millis()),
        ctx.home().to_string_lossy(),
    )
    .hide(facts.account.clone(), "user");

    if let Some(id) = &facts.identity {
        sheet = sheet
            .hide(id.email.clone(), "email")
            .hide(id.account_uuid.clone(), "account")
            .hide(id.organization_uuid.clone(), "org");
        if let Some(name) = &id.organization_name {
            sheet = sheet.hide(name.clone(), "org");
        }
    }
    if let Ok(state) = &facts.state {
        for account in &state.accounts {
            sheet = sheet
                .hide(account.email.clone(), "email")
                .hide(account.account_uuid.clone(), "account")
                .hide(account.organization_uuid.clone(), "org");
        }
    }
    // A fingerprint is not a token, and it still identifies one login across reports.
    if let Ok(Some(doc)) = &facts.credential
        && let Some(fingerprint) = doc
            .get("claudeAiOauth")
            .map(crate::park::fingerprint_of)
            .filter(|f| !f.is_empty())
    {
        sheet = sheet.hide(fingerprint, "login");
    }
    for park in &facts.parks {
        if let Some(held) = &park.park {
            sheet = sheet
                .hide(held.refresh_fingerprint.clone(), "login")
                .hide(held.service.clone(), "park");
        }
    }
    sheet
}

/// No check failed. Warnings are advice; a failure means an assumption broke.
pub fn healthy(checks: &[Check]) -> bool {
    checks.iter().all(|c| c.level != Level::Fail)
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
            credential_parts: Vec::new(),
            credential_cost: Some(store::Cost {
                needs: 900,
                limit: 4032,
                second_route: true,
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
            readable_by_others: Vec::new(),
            machine_id_known: true,
            hover_rest_env: false,
            daemon: None,
            pending_parks: Vec::new(),
            claude_version: Some(crate::assumptions::VERIFIED_AGAINST.into()),
            auth_overrides: Vec::new(),
            asking_held: Vec::new(),
            state: Ok(State::default()),
            parks: Vec::new(),
            interrupted: false,
            now: NOW,
        }
    }

    const NOW: i64 = 1_789_935_600;

    fn parked(label: &str, refresh_expires_at: Option<i64>) -> ParkFact {
        ParkFact {
            last_used_at: None,
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

    /// What the check above is given, read off a real disk.
    ///
    /// Everything else here hands `evaluate` its facts, so nothing until now had ever
    /// looked at a file's mode. A gatherer that returns an empty list whatever the disk
    /// says would pass every one of those tests.
    #[test]
    fn a_world_readable_park_is_found_on_the_disk() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "pitboard-doctor-modes-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        struct Scratch(std::path::PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _guard = Scratch(root.clone());

        let ctx = Context::new(root.clone()).with_pitboard_home(root.join(".pitboard"));
        let vault = store::vault_dir(&ctx);
        std::fs::create_dir_all(&vault).expect("a vault");
        std::fs::set_permissions(&vault, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(
            loose_logins(&ctx).is_empty(),
            "a private vault is not loose"
        );

        let park = vault.join("pitboard-park-x.json");
        std::fs::write(&park, "{}").expect("a park");
        std::fs::set_permissions(&park, std::fs::Permissions::from_mode(0o644)).unwrap();
        let found = loose_logins(&ctx);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].1, 0o644);
        assert!(found[0].0.ends_with("pitboard-park-x.json"), "{found:?}");

        std::fs::set_permissions(&park, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(loose_logins(&ctx).is_empty(), "0600 is private");
    }

    /// On a machine with no keychain every parked login is a plaintext OAuth token in a
    /// file, and the only thing between it and everyone else with an account here is a
    /// mode bit. A backup restore, a `cp -r`, an rsync or a careless umask changes one
    /// quietly, and nothing else in pitboard would ever mention it.
    #[test]
    fn a_login_anyone_on_this_machine_can_read_is_a_failure() {
        let mut f = facts();
        assert_eq!(check(&evaluate(&f), "private_on_disk").level, Level::Ok);

        f.readable_by_others = vec![
            ("/home/a/.pitboard/vault/pitboard-park-x.json".into(), 0o644),
            ("/home/a/.claude/.credentials.json".into(), 0o640),
        ];
        let checks = evaluate(&f);
        let found = check(&checks, "private_on_disk");
        assert_eq!(found.level, Level::Fail);
        assert!(found.detail.contains("mode 644"), "{}", found.detail);
        assert!(found.detail.contains("mode 640"), "{}", found.detail);
        // The advice has to be something a person can run, not a description of a problem.
        assert!(found.advice.contains("chmod go-rwx"), "{}", found.advice);
        assert!(!healthy(&checks));
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
    fn the_successor_backend_alone_does_not_move_the_login() {
        let server_on = || {
            let mut f = facts();
            f.config = Ok(json!({"cachedGrowthBookFeatures": {"tengu_hover_rest": true}}));
            f
        };
        let env_on = || {
            let mut f = facts();
            f.hover_rest_env = true;
            f
        };
        for mut f in [server_on(), env_on()] {
            // On the keychain, the flag changes nothing pitboard reads.
            let checks = evaluate(&f);
            assert_eq!(check(&checks, "storage_v5").level, Level::Ok);
            // In the fallback, it is the one case worth saying something about.
            f.backend = Ok(store::Backend::File);
            let checks = evaluate(&f);
            assert_eq!(check(&checks, "storage_v5").level, Level::Warn);
        }
    }

    #[test]
    fn the_successor_backend_is_quiet_when_it_is_off() {
        let mut f = facts();
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "storage_v5").level, Level::Ok);
        // The fallback store on its own is not the successor backend.
        f.backend = Ok(store::Backend::File);
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "storage_v5").level, Level::Ok);
    }

    /// The failure that would follow Claude Code moving where it keeps a login: not an
    /// absence, a mismatch, and the difference is what decides whether the advice is "sign
    /// in" or "pitboard is looking in the wrong place".
    #[test]
    fn a_config_that_names_somebody_signed_in_with_no_login_anywhere_is_a_failure() {
        let mut f = facts();
        f.backend = Ok(store::Backend::Absent);
        let checks = evaluate(&f);
        let store = check(&checks, "credential_store");
        assert_eq!(store.level, Level::Fail);
        assert!(store.detail.contains("a@b.c"));

        // Nobody signed in at all is an ordinary state with an ordinary answer.
        f.identity = None;
        let checks = evaluate(&f);
        let store = check(&checks, "credential_store");
        assert_eq!(store.level, Level::Warn);
        assert!(store.advice.contains("Nothing is signed in"));
    }

    #[test]
    fn a_login_past_the_ceiling_reads_differently_when_the_way_past_it_is_refused() {
        let mut f = facts();
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "credential_size").level, Level::Ok);

        f.credential_cost = Some(store::Cost {
            needs: 8503,
            limit: 4032,
            second_route: true,
        });
        let checks = evaluate(&f);
        let written = check(&checks, "credential_size");
        assert_eq!(written.level, Level::Warn);
        assert!(written.detail.contains("argument line"));

        f.credential_cost = Some(store::Cost {
            needs: 8503,
            limit: 4032,
            second_route: false,
        });
        let checks = evaluate(&f);
        let refused = check(&checks, "credential_size");
        assert_eq!(
            refused.level,
            Level::Fail,
            "there is no third way to write it"
        );
        assert!(refused.detail.contains("PITBOARD_NO_ARGV"));
    }

    /// An account nobody has come back to keeps a live, continuously rotated refresh token
    /// on this machine for as long as it stays enrolled. Nothing is dropped on a timer
    /// pitboard chose; the threshold is the token's own lifetime and all it does is say so.
    /// A login that will not fit is almost never the login. On one real machine the OAuth
    /// block was 506 bytes and eleven MCP server tokens were 3679, and the check said
    /// "8503 of 4032 bytes" and left the person to guess which of those to act on.
    #[test]
    fn a_login_too_big_says_what_is_taking_up_the_room() {
        let mut f = facts();
        f.credential_cost = Some(store::Cost {
            needs: 8503,
            limit: 4032,
            second_route: true,
        });
        f.credential_parts = parts_of(&json!({
            "claudeAiOauth": {"refreshToken": "r"},
            "mcpOAuth": {
                "one": {"token": "x".repeat(300)},
                "two": {"token": "y".repeat(200)},
                "three": {"token": "z".repeat(100)},
            },
        }));

        let checks = evaluate(&f);
        let size = check(&checks, "credential_size");
        assert!(size.detail.contains("mcpOAuth one"), "{}", size.detail);
        assert!(size.detail.contains("mcpOAuth two"), "{}", size.detail);
        assert!(
            !size.detail.contains("claudeAiOauth"),
            "three is enough to act on, and the login itself is never the problem: {}",
            size.detail
        );
    }

    #[test]
    fn what_takes_up_the_room_is_listed_largest_first_and_named_per_server() {
        let parts = parts_of(&json!({
            "claudeAiOauth": {"refreshToken": "r"},
            "mcpOAuth": {"small": {"t": "x"}, "large": {"t": "y".repeat(500)}},
        }));
        let names: Vec<&str> = parts.iter().map(|(what, _)| what.as_str()).collect();
        assert_eq!(names[0], "mcpOAuth large", "largest first: {names:?}");
        assert!(names.contains(&"claudeAiOauth"));
        assert!(
            names.contains(&"mcpOAuth small"),
            "each server is named, because that is what a person signs out of"
        );

        // One server is not worth breaking apart; the key says it already.
        let single = parts_of(&json!({"mcpOAuth": {"only": {"t": "x"}}}));
        assert_eq!(single.len(), 1);
        assert_eq!(single[0].0, "mcpOAuth");
    }

    #[test]
    fn an_account_nobody_has_come_back_to_is_said_out_loud() {
        let mut f = facts();
        f.parks = vec![ParkFact {
            label: "work".into(),
            active: false,
            last_used_at: Some(f.now - 31 * 86_400),
            park: Some(Park {
                service: "pitboard-park-acc-1".into(),
                parked_at: f.now - 31 * 86_400,
                refresh_fingerprint: "f".into(),
                access_expires_at: Some(f.now + 3600),
                refresh_expires_at: Some(f.now + 30 * 86_400),
            }),
            unreadable: None,
        }];
        let checks = evaluate(&f);
        let dormant = check(&checks, "dormant_account");
        assert_eq!(dormant.level, Level::Warn);
        assert!(dormant.advice.contains("pitboard forget work"));

        // A month is the token's own life. Inside it, there is nothing to say.
        f.parks[0].last_used_at = Some(f.now - 29 * 86_400);
        assert!(
            !evaluate(&f).iter().any(|c| c.code == "dormant_account"),
            "an account used within a token's lifetime is not dormant"
        );

        // Neither is one that is signed in, nor one holding nothing.
        f.parks[0].last_used_at = Some(f.now - 400 * 86_400);
        f.parks[0].active = true;
        assert!(!evaluate(&f).iter().any(|c| c.code == "dormant_account"));
        f.parks[0].active = false;
        f.parks[0].park = None;
        assert!(!evaluate(&f).iter().any(|c| c.code == "dormant_account"));
    }

    #[test]
    fn the_daemon_is_reported_without_being_a_problem() {
        let mut f = facts();
        let checks = evaluate(&f);
        assert!(check(&checks, "claude_daemon").detail.contains("none"));

        f.daemon = Some(daemon::Daemon {
            pid: 4321,
            version: Some("2.1.278".into()),
            started_at: Some(1_790_079_766_317),
            origin: Some("transient".into()),
            launch_target: None,
            running: true,
        });
        let checks = evaluate(&f);
        let running = check(&checks, "claude_daemon");
        assert_eq!(running.level, Level::Ok);
        assert!(running.detail.contains("running"));
        assert!(running.detail.contains("2.1.278"));
        assert!(running.detail.contains("4321"));

        f.daemon.as_mut().expect("set above").running = false;
        let checks = evaluate(&f);
        let stopped = check(&checks, "claude_daemon");
        assert_eq!(stopped.level, Level::Ok);
        assert!(stopped.detail.contains("not running"));
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
                last_used_at: None,
                label: "empty".into(),
                active: false,
                park: None,
                unreadable: None,
            },
            ParkFact {
                last_used_at: None,
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
}
