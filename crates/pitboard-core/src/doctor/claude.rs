use super::{Check, Facts, fail, ok, read_from, warn};
use crate::context::Context;
use crate::error::Error;
use crate::host::Os;
use crate::in_use::Stored;
use crate::provider::claude::live as claude_live;
use crate::provider::{LoginKind, ProviderId};
use crate::state::{State, renewal_due};
use crate::{store, words};
use serde_json::Value;
use std::path::PathBuf;

/// Claude Code's plaintext fallback file, there while the keychain holds the login in use.
pub(crate) struct FallbackLogin {
    pub(crate) path: PathBuf,
    /// A handle on the refresh token of the login it holds, never the token: `None` where
    /// it holds no login, empty where that login has no refresh token, and why where what
    /// it holds cannot be read.
    pub(crate) fingerprint: Result<Option<String>, store::Error>,
}

pub(super) fn login_pins(ctx: &Context, state: Option<&State>) -> (Vec<String>, Vec<String>) {
    let said = crate::settings::organisation_pins(ctx)
        .into_iter()
        .map(|(layer, ids)| {
            format!(
                "`forceLoginOrgUUID` set to {} in {layer}",
                words::listed_values(&ids)
            )
        })
        .collect();
    let LoginKind::Document(claude) = crate::provider::of(ProviderId::Claude) else {
        return (said, Vec::new());
    };
    let ruled_out = state.map_or_else(Vec::new, |s| {
        s.accounts
            .iter()
            .filter(|a| a.provider() == ProviderId::Claude)
            .filter(|a| claude.refuses(ctx, &Value::Null, &a.owner()).is_some())
            .map(|a| format!("`{}`", s.typed(&a.key())))
            .collect()
    });
    (said, ruled_out)
}

// Nothing where whether it is there cannot be told: an unreadable keychain is the
// `credential_store` check's to say.
pub(super) fn fallback_login(ctx: &Context) -> Option<FallbackLogin> {
    let login = claude_live::login_behind(ctx)?;
    Some(FallbackLogin {
        path: claude_live::credential_file(ctx),
        fingerprint: login.map(|login| {
            login.map(|document| crate::provider::claude::document::fingerprint_of(&document))
        }),
    })
}

// Claude Code's login in use from one look down its chain and one read.
pub(super) struct Found {
    pub(super) backend: Result<store::Backend, store::Error>,
    pub(super) credential: Result<Option<Value>, store::Error>,
    pub(super) cost: Option<store::Cost>,
    pub(super) parts: Vec<(String, usize)>,
    pub(super) stored: bool,
}

pub(super) fn found(ctx: &Context, service: &str) -> Found {
    let chain = claude_live::chain(ctx);
    let holder = store::holder(&chain, service);
    let raw = holder
        .clone()
        .and_then(|held| held.map_or(Ok(None), |backend| backend.read(service)));
    let credential = raw.clone().and_then(store::parsed);
    Found {
        backend: holder
            .as_ref()
            .map(|held| held.map_or(store::Backend::Absent, |backend| backend.kind()))
            .map_err(Clone::clone),
        cost: match (&holder, &raw) {
            (Ok(held), Ok(Some(raw))) => store::cost_at(&chain, *held, service, raw),
            _ => None,
        },
        parts: match &credential {
            Ok(Some(document)) => parts_of(document),
            _ => Vec::new(),
        },
        stored: matches!(raw, Ok(Some(_))),
        credential,
    }
}

/// What is taking up the room in a credential document, largest first.
///
/// A login that will not fit is almost never the login: on one real machine the OAuth block
/// was 506 bytes and eleven MCP server tokens were 3679. Saying "8503 of 4032 bytes" leaves
/// a person to guess which of those to do something about, and the answer is in the
/// document Pitboard has already read.
pub(super) fn parts_of(document: &Value) -> Vec<(String, usize)> {
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

// Claude Code writes its config and its account once it has run.
pub(super) fn run_claude_once(facts: &Facts) -> String {
    let found = facts.claude_program.as_deref();
    let program = words::spelling(facts.os, ProviderId::Claude, found).of(ProviderId::Claude);
    match found {
        Some(_) => format!("Run `{program}` once."),
        None => format!("Install Claude Code, then run `{program}` once."),
    }
}

pub(super) fn judge_config(facts: &Facts) -> Check {
    match &facts.config {
        Ok(v) => ok(
            "config_file",
            "config file",
            format!(
                "{}  ({} keys)",
                facts.config_path.display(),
                v.as_object().map_or(0, serde_json::Map::len)
            ),
        ),
        // Claude Code writes it on its first run, and a switch does without it.
        Err(Error::ClaudeConfigMissing { path, .. }) if facts.claude_program.is_none() => warn(
            "config_file",
            "config file",
            format!(
                "Claude Code is not installed where Pitboard looks, and {} does not exist",
                path.display()
            ),
            run_claude_once(facts),
        ),
        Err(missing @ Error::ClaudeConfigMissing { .. }) => warn(
            "config_file",
            "config file",
            missing.to_string(),
            run_claude_once(facts),
        ),
        Err(e) => fail(
            "config_file",
            "config file",
            e.to_string(),
            run_claude_once(facts),
        ),
    }
}

pub(super) fn judge_identity(facts: &Facts) -> Check {
    match &facts.config_account {
        Some(id) => ok(
            "identity",
            "identity",
            format!("{}  ·  org {}", id.email, id.organization_uuid),
        ),
        None => warn(
            "identity",
            "identity",
            "Claude Code has not recorded a signed-in account",
            format!(
                "It writes this after its first successful call. {}",
                run_claude_once(facts)
            ),
        ),
    }
}

pub(super) fn judge_slot(facts: &Facts) -> Check {
    ok(
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
    )
}

// A login that grows past the ceiling cannot be switched at all, and it grows by things
// done elsewhere, so it is worth saying before the day it refuses.
pub(super) fn judge_size(facts: &Facts) -> Option<Check> {
    let price = facts.credential_cost?;
    if let Ok(store::Backend::CredentialManager) = &facts.backend {
        return Some(judge_pieces(price, &facts.credential_parts));
    }
    let (bytes, limit) = (price.needs, price.limit);
    Some(if price.refused() {
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
    })
}

pub(super) fn judge_no_argv(facts: &Facts) -> Option<Check> {
    (facts.no_argv && facts.os == Os::Windows).then(|| {
        warn(
            "no_argv",
            "PITBOARD_NO_ARGV",
            "set, and it changes nothing on Windows",
            "Claude Code's Credential Manager store takes a login of any size it writes in \
             pieces, never on an argument line. Unset it.",
        )
    })
}

pub(super) fn judge_store(facts: &Facts) -> Check {
    match &facts.backend {
        Ok(store::Backend::Keychain) => ok("credential_store", "credential store", "keychain"),
        // Enterprise persistence (`credman_attributes`).
        Ok(store::Backend::CredentialManager) => {
            let detail = format!(
                "Credential Manager  ·  {}/{}  ·  every logon of yours on this PC reads it",
                facts.service, facts.account
            );
            if facts.windows.as_ref().is_some_and(|homes| homes.roams) {
                warn(
                    "credential_store",
                    "credential store",
                    detail,
                    "Your profile roams, and Credential Manager takes Claude Code's login with it \
                     to every PC you sign in to, so a switch here changes the login there too.",
                )
            } else {
                ok("credential_store", "credential store", detail)
            }
        }
        Ok(store::Backend::File) => {
            let detail = format!("plaintext file  ·  {}", facts.credential_file.display());
            match facts.os {
                Os::MacOs => warn(
                    "credential_store",
                    "credential store",
                    detail,
                    "Claude Code fell back to a file, which means a keychain write failed at some point.",
                ),
                // The chain is the file on Windows only where Credential Manager holds no login.
                Os::Linux | Os::Windows => ok("credential_store", "credential store", detail),
            }
        }
        // No tool keeps its login in Pitboard's sealed files.
        Ok(store::Backend::Unknown | store::Backend::SealedFile) => fail(
            "credential_store",
            "credential store",
            "a store Pitboard does not read yet",
            "Treat this as unknown, never as empty.",
        ),
        // As a switch refuses with `live_credential_elsewhere`: what would follow Claude Code
        // moving where it keeps a login.
        Ok(store::Backend::Absent) => match &facts.config_account {
            Some(named) => fail(
                "credential_store",
                "credential store",
                words::named_but_not_found(facts.os, &named.email),
                "Pitboard will not write a login where nobody reads it. Check for a Pitboard \
                 update; if there is none, this is worth reporting.",
            ),
            None => warn(
                "credential_store",
                "credential store",
                "no credential in any backend",
                "Nothing is signed in for this slot.",
            ),
        },
        Err(e) => fail(
            "credential_store",
            "credential store",
            e.to_string(),
            match facts.os {
                Os::MacOs | Os::Linux => "Treat this as unknown, never as empty.",
                Os::Windows => {
                    "Treat this as unknown, never as empty. While nothing turns Claude Code's \
                     Credential Manager store on, its sessions read only .credentials.json, so a \
                     login left only in Credential Manager is signed out to them."
                }
            },
        ),
    }
}

/// Whose login Claude Code has stored, as Anthropic last said, which every reader that does
/// not ask takes as the account in use, and whether something may have signed in since.
/// Doctor asks nobody, so a login renewed since, which Claude Code does every few hours, is
/// said and is no warning; Claude Code's config naming another account is, since only a
/// read that asks can say whether another login came with it. Nothing is said where nothing
/// is recorded and no Claude Code account is enrolled.
pub(super) fn judge_in_use(facts: &Facts, moment: &dyn Fn(i64, i64) -> String) -> Option<Check> {
    let (code, name) = ("in_use", "in use");
    let known = facts.in_use.as_ref()?;
    let state = facts.state.as_ref().ok()?;
    let enrolled = facts.parks.iter().any(|p| p.provider == ProviderId::Claude);
    if known.last.is_none() && !enrolled {
        return None;
    }
    let ask = "Run `pitboard status`, which asks Anthropic whose login Claude Code has stored.";
    let Some(last) = &known.last else {
        return Some(warn(
            code,
            name,
            "Anthropic has not been asked whose login Claude Code has stored",
            ask,
        ));
    };
    let account = Stored::of(state, ProviderId::Claude, last.owner.as_ref());
    let in_use = account.said("no account");
    let config =
        Stored::of(state, ProviderId::Claude, known.own_record.as_ref()).said("no account");
    let when = moment(last.known_at, facts.now);
    let then = match &last.owner {
        Some(_) => format!("Anthropic named {in_use} {when}"),
        None => format!("Pitboard found no login stored {when}"),
    };
    match known.unconfirmed() {
        Some(crate::in_use::Doubt::NeverEstablished) => {
            return Some(warn(
                code,
                name,
                format!(
                    "Anthropic has not been asked whose login Claude Code has stored since \
                     Pitboard was updated; {in_use} is the account it last switched to"
                ),
                ask,
            ));
        }
        Some(crate::in_use::Doubt::NamedMoved) => {
            return Some(warn(
                code,
                name,
                format!(
                    "Claude Code's config has named {config} since {then}, so another login \
                     may be stored"
                ),
                ask,
            ));
        }
        Some(crate::in_use::Doubt::LoginUnreadable) | None => {}
    }
    if known.named_another(ProviderId::Claude).is_some() {
        return Some(warn(
            code,
            name,
            format!("{in_use}, as Anthropic said {when}, and Claude Code's config names {config}"),
            format!(
                "`/status` in Claude Code shows {config}. {}",
                account.how_to_name()
            ),
        ));
    }
    // The login stored now by its fingerprint, empty for a login with no refresh token:
    // `Some(None)` where none is stored, and `None` where the store or its shape cannot be read.
    let claude_code = crate::provider::claude_code();
    let stored = match &facts.credential {
        Ok(Some(document)) if crate::provider::claude::document::signed_out(document) => Some(None),
        Ok(Some(document)) => claude_code
            .slice(document)
            .ok()
            .map(|slice| Some(claude_code.fingerprint(&slice))),
        Ok(None) => Some(None),
        Err(_) => None,
    };
    let renewed = "the next `pitboard status` asks whose it is";
    Some(ok(
        code,
        name,
        match (&last.owner, stored) {
            (Some(_), Some(Some(now))) if now.is_empty() || now != last.login => format!(
                "{in_use}, as Anthropic said {when} of a login renewed or replaced since; \
                 {renewed}"
            ),
            (Some(_), Some(None)) => {
                format!("{in_use}, as Anthropic said {when}; no login is stored since")
            }
            (Some(_), _) => format!("{in_use}, as Anthropic said {when}"),
            (None, Some(Some(_))) => {
                format!("no login stored, as Pitboard found {when}; one is since, and {renewed}")
            }
            (None, _) => format!("no login stored, as Pitboard found {when}"),
        },
    ))
}

pub(super) fn judge_credential(facts: &Facts) -> Check {
    match &facts.credential {
        Ok(Some(doc)) => {
            let keys: Vec<&str> = doc
                .as_object()
                .map(|o| o.keys().map(String::as_str).collect())
                .unwrap_or_default();
            if crate::provider::claude::document::signed_out(doc) {
                return warn(
                    "credential",
                    "credential",
                    format!("signed out; the document keeps only {keys:?}"),
                    "Nothing is signed in for this slot.",
                );
            }
            let changed = |detail: String| {
                fail(
                    "credential",
                    "credential",
                    detail,
                    "The credential's shape changed. Do not switch accounts until this is understood.",
                )
            };
            if !doc.is_object() {
                return changed("the stored login is not a JSON object".into());
            }
            let Some(oauth) = doc.get("claudeAiOauth").and_then(Value::as_object) else {
                return changed(format!(
                    "claudeAiOauth is not an object; the keys are {keys:?}"
                ));
            };
            let fingerprint = oauth
                .get("refreshToken")
                .and_then(Value::as_str)
                .map(store::fingerprint)
                .unwrap_or_else(|| "none".into());
            let Some(at) = crate::provider::claude_code()
                .expiry(doc)
                .refresh_expires_at
            else {
                return ok(
                    "credential",
                    "credential",
                    format!("refresh {fingerprint}  ·  keys {keys:?}"),
                );
            };
            let life = words::parked_life_column(at, facts.now);
            let detail = format!("refresh {fingerprint}  ·  {life}  ·  keys {keys:?}");
            if at <= facts.now {
                warn(
                    "credential",
                    "credential",
                    detail,
                    "This login has expired: sign in to Claude Code again.",
                )
            } else if renewal_due(at, facts.now) {
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
        // Read in 2.1.294 (the register's `locked_keychain_keeps_last_login`): a session
        // keeps serving the login it last read while the keychain is locked, and one that
        // read none reads the keychain as empty and falls through to the file.
        Err(store::Error::Locked) if facts.os == Os::Windows => fail(
            "credential",
            "credential",
            "Credential Manager cannot be reached from this logon",
            format!(
                "A network logon, as {}, has no Credential Manager. Run Pitboard from a desktop \
                 session. A Claude Code session started from this logon reads only {}.",
                words::SSH_SIGN_IN,
                facts.credential_file.display()
            ),
        ),
        Err(store::Error::Locked) => fail(
            "credential",
            "credential",
            "the keychain is locked, and cannot ask to be unlocked from here",
            format!(
                "Unlock it with `security unlock-keychain`, or run Pitboard from a desktop \
                 session. Until then a Claude Code session here keeps the login it last read \
                 and follows no switch{}",
                match facts
                    .fallback_login
                    .as_ref()
                    .map(|left| (left.path.display(), &left.fingerprint))
                {
                    Some((path, Ok(Some(_)))) => {
                        format!(", and one started here signs in with the login in {path}.")
                    }
                    Some((path, Err(_))) => format!(
                        ", and whether one started here signs in with what {path} holds cannot \
                         be told, since Pitboard could not read it."
                    ),
                    Some((_, Ok(None))) | None => ", and one started here is signed out.".into(),
                }
            ),
        ),
        // `claude auth login` writes nothing over it (`credman_move_lost_a_piece`).
        Err(store::Error::Broken { why, spelling }) => fail(
            "credential",
            "credential",
            format!("{why}, which Claude Code cannot read either"),
            format!(
                "Run `{} auth logout`, which deletes it, then sign in to Claude Code again. \
                 Signing in before that leaves it as it is.",
                spelling.of(ProviderId::Claude)
            ),
        ),
        Err(e) => fail(
            "credential",
            "credential",
            e.to_string(),
            "Do not write to the store while this is failing.",
        ),
    }
}

// What leaves the file and keeps it there: `locked_sign_in_writes_fallback` and
// `fallback_outlives_keychain_writes` on macOS, `credman_unreachable_writes_file` and
// `credman_migration` on Windows.
pub(super) fn judge_fallback_login(facts: &Facts) -> Option<Check> {
    let left = facts.fallback_login.as_ref()?;
    let path = left.path.display();
    let running = format!(
        "While it is there, Claude Code sessions already running at a switch {}.",
        words::kept_until_renewed()
    );
    let cannot = words::out_of_reach(facts.os);
    let (could_not, such_as, as_login) = match facts.os {
        Os::MacOs | Os::Linux => (
            "the keychain could not be read",
            "in a session started over SSH".to_owned(),
            "`/login` over SSH does".to_owned(),
        ),
        Os::Windows => (
            "Credential Manager could not be reached",
            format!("in {}", words::SSH_SIGN_IN),
            format!("`/login` does in {}", words::SSH_SIGN_IN),
        ),
    };
    let (detail, advice) = match &left.fingerprint {
        Ok(Some(fingerprint)) => (
            format!(
                "{path}  ·  refresh {}",
                if fingerprint.is_empty() {
                    "none"
                } else {
                    fingerprint.as_str()
                }
            ),
            format!(
                "Claude Code signs in with this wherever it {cannot}, such as {such_as}, and no \
                 switch reaches it. A sign-in made where {could_not} leaves one, as {as_login}. \
                 {running} `pitboard stow` keeps this login for its account, where Pitboard \
                 holds no other it can switch to, then deletes the file, which leaves one login \
                 for every session."
            ),
        ),
        Ok(None) => (
            format!("{path}  ·  no login in it"),
            format!(
                "It holds no Claude Code login. {running} `pitboard stow` deletes it, which \
                 lets them follow a switch."
            ),
        ),
        Err(e) => (
            format!("{path}  ·  not read: {e}"),
            format!(
                "Pitboard could not read it, so whether it holds a login, which a session that \
                 {cannot} signs in with, cannot be told, and `pitboard stow` cannot put it away. \
                 {running} Deleting it lets them follow a switch, and takes whatever it holds \
                 with it: `{}`.",
                facts.os.remove_file_command(&left.path)
            ),
        ),
    };
    Some(warn("fallback_login", "fallback login", detail, advice))
}

/// Claude Code's successor credential backend.
///
/// Measured in 2.1.278: the flag does not move the login out of the keychain. The live
/// chain is built as keychain-with-plaintext-fallback either way, and the flag only decides
/// what backs the fallback half, and only for a caller that hands a backend in. An ordinary
/// `claude` hands none in, so the fallback stays `<storage dir>/.credentials.json`. The
/// combination worth saying something about is the flag on *and* the login living in the
/// fallback, because that is the one case where what Pitboard reads may not be what a
/// session reads.
pub(super) fn judge_storage_v5(facts: &Facts) -> Check {
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
            "Pitboard reads the plaintext file. If Claude Code was given a backend of its \
             own, that is not the same file. Check for an update before switching.",
        ),
        _ => ok(
            "storage_v5",
            "storage v5",
            "switched on; the keychain is still where the login is",
        ),
    }
}

// On Windows: one item to 2400 bytes, 2400-character pieces of base64 to 256 (`credman_chunking`).
pub(super) fn judge_pieces(price: store::Cost, parts: &[(String, usize)]) -> Check {
    let (bytes, limit) = (price.needs, price.limit);
    match crate::provider::claude::pieces::pieces_for(bytes) {
        None => fail(
            "credential_size",
            "login size",
            format!(
                "{}{}",
                words::past_credential_manager(bytes, limit),
                biggest(parts)
            ),
            "Neither Pitboard nor Claude Code can write a login this size there. Sign out of \
             MCP servers you no longer use to make it smaller.",
        ),
        Some(0) => ok(
            "credential_size",
            "login size",
            format!("{bytes} bytes, in one item"),
        ),
        Some(pieces) => ok(
            "credential_size",
            "login size",
            format!(
                "{bytes} bytes, in {pieces} pieces of {} characters",
                crate::provider::claude::pieces::PIECE
            ),
        ),
    }
}

/// The three things taking up the most room, for a check that would otherwise leave a
/// person guessing which of them to do something about.
pub(super) fn biggest(parts: &[(String, usize)]) -> String {
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

pub(super) fn judge_login_restrictions(facts: &Facts) -> Option<Check> {
    if facts.organisation_pins.is_empty() {
        return None;
    }
    let name = "login restrictions";
    let said = facts.organisation_pins.join("; ");
    Some(if facts.pinned_out.is_empty() {
        ok("login_restrictions", name, said)
    } else {
        warn(
            "login_restrictions",
            name,
            said,
            format!(
                "Claude Code refuses the logins of {} as a session starts, so Pitboard does not \
                 put them in use.",
                words::listed(facts.pinned_out.clone())
            ),
        )
    })
}

// From files as well, since an app launched from Finder has no environment to read.
pub(super) fn judge_auth(facts: &Facts) -> Check {
    if facts.auth_overrides.is_empty() {
        let read = if facts.fallback_login.is_some() {
            "the stored login, which is what Pitboard moves, once a session reads it again"
        } else {
            "the stored login, which is what Pitboard moves"
        };
        return ok("auth_source", "what a session authenticates with", read);
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
         that Pitboard is moving a login nothing reads.",
    )
}

/// Which Claude Code is installed, against which one Pitboard's facts were read.
///
/// Stated rather than warned about. Claude Code ships several times a week, so a mismatch
/// is the ordinary state of the world within days of a release and warning about it would
/// be noise on every machine. What is worth a warning is an assumption that has actually
/// stopped holding, which is a probe's job and not a version number's.
pub(super) fn judge_claude_version(facts: &Facts) -> Check {
    let verified = read_from(ProviderId::Claude, facts.os);
    match facts.claude_version.as_deref() {
        None => ok(
            "claude_version",
            "Claude Code build",
            format!("not found here; Pitboard's facts were read from {verified}"),
        ),
        Some(installed) if installed == verified => ok(
            "claude_version",
            "Claude Code build",
            format!("{installed}, which is what Pitboard's facts were read from"),
        ),
        Some(installed) => ok(
            "claude_version",
            "Claude Code build",
            format!("{installed} installed; Pitboard's facts were read from {verified}"),
        ),
    }
}

/// Claude Code's supervisor daemon is a second writer of the login, on a schedule nobody
/// typed. It takes the same write lock, so it cannot write underneath a switch, but a
/// person reading a diagnosis should be able to see that it is there.
pub(super) fn judge_daemon(facts: &Facts) -> Check {
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
