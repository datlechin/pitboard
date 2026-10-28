use super::{Check, Facts, fail, ok, read_from, warn, words};
use crate::context::Context;
use crate::error::Error;
use crate::host::{Os, SecretStore};
use crate::in_use::Stored;
use crate::provider::claude::live as claude_live;
use crate::provider::{LoginKind, ProviderId};
use crate::state::{State, renewal_due};
use crate::store;
use crate::words as said;
use serde_json::Value;
use std::path::PathBuf;

// Claude Code's plaintext file, there while the keychain holds the login in use.
pub(crate) struct FallbackLogin {
    pub(crate) path: PathBuf,
    // The store it is behind.
    pub(crate) store: SecretStore,
    // `None` where it holds no login, and empty where that login has no refresh token.
    pub(crate) fingerprint: Result<Option<String>, store::Error>,
}

pub(super) fn login_pins(ctx: &Context, state: Option<&State>) -> (Vec<String>, Vec<String>) {
    let said = crate::settings::organisation_pins(ctx)
        .into_iter()
        .map(|(layer, ids)| words::org_pinned(&said::listed_values(&ids), layer))
        .chain(crate::settings::console_pin(ctx).map(words::console_pinned))
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

// Nothing where that cannot be told: an unreadable keychain is `credential_store`'s to say.
fn fallback_login(
    ctx: &Context,
    chain: &store::Live,
    holder: &dyn store::RawStore,
    service: &str,
) -> Option<FallbackLogin> {
    let store = holder.kind().secret_store()?;
    let login = claude_live::behind_holder(chain, holder, service)?;
    Some(FallbackLogin {
        path: claude_live::credential_file(ctx),
        store,
        fingerprint: login.map(|held| {
            claude_live::login_in(&held)
                .map(|document| crate::provider::claude::document::fingerprint_of(&document))
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
    pub(super) fallback: Option<FallbackLogin>,
}

pub(super) fn found(ctx: &Context, service: &str) -> Found {
    let chain = claude_live::chain(ctx);
    let holder = store::holder(&chain, service);
    let raw = holder
        .clone()
        .and_then(|held| held.map_or(Ok(None), |backend| backend.read(service)));
    let credential = raw.clone().and_then(store::parsed);
    Found {
        fallback: match &holder {
            Ok(Some(held)) => fallback_login(ctx, &chain, *held, service),
            _ => None,
        },
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

// MCP server tokens, not the login itself, are what usually fill a store's ceiling.
pub(super) fn parts_of(document: &Value) -> Vec<(String, usize)> {
    let weigh = |value: &Value| serde_json::to_string(value).map(|s| s.len()).unwrap_or(0);
    let Some(root) = document.as_object() else {
        return Vec::new();
    };
    let mut parts: Vec<(String, usize)> = root
        .iter()
        .flat_map(|(key, value)| match (key.as_str(), value.as_object()) {
            // The usual culprit, which a person signs out of server by server.
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

pub(super) fn run_claude_once(facts: &Facts) -> String {
    let found = facts.claude_program.as_deref();
    let program = said::spelling(facts.os, ProviderId::Claude, found).of(ProviderId::Claude);
    match found {
        Some(_) => words::run_once(program),
        None => words::install_then_run_once(program),
    }
}

pub(super) fn judge_config(facts: &Facts) -> Check {
    match &facts.config {
        Ok(v) => ok(
            "config_file",
            "config file",
            words::config_read(
                facts.config_path.display(),
                v.as_object().map_or(0, serde_json::Map::len),
            ),
        ),
        // Claude Code writes it on its first run, and a switch does without it.
        Err(Error::ClaudeConfigMissing { path, .. }) if facts.claude_program.is_none() => warn(
            "config_file",
            "config file",
            words::not_installed_and_no_config(path.display()),
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
            words::identity(&id.email, &id.organization_uuid),
        ),
        None => warn(
            "identity",
            "identity",
            words::NO_ACCOUNT_RECORDED,
            words::written_after_the_first_call(&run_claude_once(facts)),
        ),
    }
}

pub(super) fn judge_slot(facts: &Facts) -> Check {
    ok(
        "slot",
        "slot",
        if facts.default_slot {
            words::default_slot(&facts.service, &facts.account)
        } else {
            words::slot_selected_by(&facts.service, &facts.account, &facts.storage_dir)
        },
    )
}

// Said before the day a login grown past the ceiling cannot be switched at all.
pub(super) fn judge_size(facts: &Facts) -> Option<Check> {
    let price = facts.credential_cost?;
    if let Ok(store::Backend::CredentialManager) = &facts.backend {
        return Some(judge_pieces(price, &facts.credential_parts));
    }
    let (bytes, limit) = (price.needs, price.limit);
    let biggest = words::biggest(&facts.credential_parts);
    Some(if price.refused() {
        fail(
            "credential_size",
            "login size",
            words::size_refused(bytes, limit, &biggest),
            words::NO_THIRD_WAY,
        )
    } else if price.over() {
        // No fault: Claude Code writes this same login on the argument line on every refresh.
        warn(
            "credential_size",
            "login size",
            words::size_on_the_argument_line(bytes, limit, &biggest),
            words::ONLY_AS_AN_ARGUMENT,
        )
    } else {
        ok("credential_size", "login size", words::size(bytes, limit))
    })
}

pub(super) fn judge_no_argv(facts: &Facts) -> Option<Check> {
    (facts.no_argv && facts.os == Os::Windows).then(|| {
        warn(
            "no_argv",
            "PITBOARD_NO_ARGV",
            words::NO_ARGV_CHANGES_NOTHING,
            words::UNSET_NO_ARGV,
        )
    })
}

pub(super) fn judge_store(facts: &Facts) -> Check {
    match &facts.backend {
        Ok(store::Backend::Keychain) => ok("credential_store", "credential store", words::KEYCHAIN),
        // Enterprise persistence (`credman_attributes`).
        Ok(store::Backend::CredentialManager) => {
            let detail = words::in_credential_manager(&facts.service, &facts.account);
            if facts.windows.as_ref().is_some_and(|homes| homes.roams) {
                warn(
                    "credential_store",
                    "credential store",
                    detail,
                    words::ROAMS_WITH_THE_PROFILE,
                )
            } else {
                ok("credential_store", "credential store", detail)
            }
        }
        Ok(store::Backend::File) => {
            let detail = words::plaintext_file(facts.credential_file.display());
            match facts.os {
                Os::MacOs => warn(
                    "credential_store",
                    "credential store",
                    detail,
                    words::FELL_BACK_TO_A_FILE,
                ),
                // The chain is the file on Windows only where Credential Manager holds no login.
                Os::Linux | Os::Windows => ok("credential_store", "credential store", detail),
            }
        }
        // No tool keeps its login in Pitboard's sealed files or its Secret Service items.
        Ok(
            store::Backend::Unknown | store::Backend::SealedFile | store::Backend::SecretService,
        ) => fail(
            "credential_store",
            "credential store",
            words::NOT_READ_YET,
            words::UNKNOWN_NOT_EMPTY,
        ),
        // As a switch refuses with `live_credential_elsewhere`.
        Ok(store::Backend::Absent) => match &facts.config_account {
            Some(named) => fail(
                "credential_store",
                "credential store",
                said::named_but_not_found(facts.os, &named.email),
                words::NOWHERE_ANYBODY_READS,
            ),
            None => warn(
                "credential_store",
                "credential store",
                words::NO_CREDENTIAL_ANYWHERE,
                words::NOTHING_SIGNED_IN,
            ),
        },
        Err(e) => fail(
            "credential_store",
            "credential store",
            e.to_string(),
            match facts.os {
                Os::MacOs | Os::Linux => words::UNKNOWN_NOT_EMPTY,
                Os::Windows => words::UNKNOWN_AND_SIGNED_OUT,
            },
        ),
    }
}

// A login renewed since is no warning; a config naming another account is, as only asking tells.
pub(super) fn judge_in_use(facts: &Facts, moment: &dyn Fn(i64, i64) -> String) -> Option<Check> {
    let (code, name) = ("in_use", "in use");
    let known = facts.in_use.as_ref()?;
    let state = facts.state.as_ref().ok()?;
    let enrolled = facts.parks.iter().any(|p| p.provider == ProviderId::Claude);
    if known.last.is_none() && !enrolled {
        return None;
    }
    let ask = words::ASK_WITH_STATUS;
    let Some(last) = &known.last else {
        return Some(warn(code, name, words::NEVER_ASKED, ask));
    };
    let account = Stored::of(state, ProviderId::Claude, last.owner.as_ref());
    let in_use = account.said("no account");
    let config =
        Stored::of(state, ProviderId::Claude, known.own_record.as_ref()).said("no account");
    let when = moment(last.known_at, facts.now);
    let then = match &last.owner {
        Some(_) => words::anthropic_named(&in_use, &when),
        None => words::found_none_stored(&when),
    };
    match known.unconfirmed() {
        Some(crate::in_use::Doubt::NeverEstablished) => {
            return Some(warn(
                code,
                name,
                words::not_asked_since_an_update(&in_use),
                ask,
            ));
        }
        Some(crate::in_use::Doubt::NamedMoved) => {
            return Some(warn(
                code,
                name,
                words::config_named_since(&config, &then),
                ask,
            ));
        }
        Some(crate::in_use::Doubt::LoginUnreadable) | None => {}
    }
    if known.named_another(ProviderId::Claude).is_some() {
        return Some(warn(
            code,
            name,
            words::config_names_another(&in_use, &when, &config),
            words::status_shows(&config, &account.how_to_name()),
        ));
    }
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
    Some(ok(
        code,
        name,
        match (&last.owner, stored) {
            (Some(_), Some(Some(now))) if now.is_empty() || now != last.login => {
                words::renewed_since(&in_use, &when)
            }
            (Some(_), Some(None)) => words::none_stored_since(&in_use, &when),
            (Some(_), _) => words::as_anthropic_said(&in_use, &when),
            (None, Some(Some(_))) => words::one_stored_since(&when),
            (None, _) => words::none_stored(&when),
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
                    words::signed_out_keeps(&keys),
                    words::NOTHING_SIGNED_IN,
                );
            }
            let changed =
                |detail: String| fail("credential", "credential", detail, words::SHAPE_CHANGED);
            if !doc.is_object() {
                return changed(words::NOT_A_JSON_OBJECT.into());
            }
            let Some(oauth) = doc.get("claudeAiOauth").and_then(Value::as_object) else {
                return changed(words::oauth_not_an_object(&keys));
            };
            let fingerprint = oauth
                .get("refreshToken")
                .and_then(Value::as_str)
                .map_or_else(|| words::NO_TOKEN.into(), store::fingerprint);
            let Some(at) = crate::provider::claude_code()
                .expiry(doc)
                .refresh_expires_at
            else {
                return ok(
                    "credential",
                    "credential",
                    words::refresh_and_keys(&fingerprint, &keys),
                );
            };
            let life = said::parked_life_column(at, facts.now);
            let detail = words::refresh_life_and_keys(&fingerprint, &life, &keys);
            if at <= facts.now {
                warn("credential", "credential", detail, words::EXPIRED)
            } else if renewal_due(at, facts.now) {
                warn("credential", "credential", detail, words::EXPIRES_SOON)
            } else {
                ok("credential", "credential", detail)
            }
        }
        Ok(None) => warn(
            "credential",
            "credential",
            words::NOTHING_STORED,
            words::NOTHING_SIGNED_IN,
        ),
        // `locked_keychain_keeps_last_login`: a session keeps the login it last read.
        Err(store::Error::Locked) if facts.os == Os::Windows => fail(
            "credential",
            "credential",
            words::CREDENTIAL_MANAGER_OUT_OF_REACH,
            words::from_a_desktop_session(facts.credential_file.display()),
        ),
        Err(store::Error::Locked) => fail(
            "credential",
            "credential",
            words::KEYCHAIN_LOCKED,
            words::unlock_the_keychain(&match facts
                .fallback_login
                .as_ref()
                .map(|left| (left.path.display(), &left.fingerprint))
            {
                Some((path, Ok(Some(_)))) => words::then_signs_in_with(path),
                Some((path, Err(_))) => words::then_cannot_tell(path),
                Some((_, Ok(None))) | None => words::THEN_SIGNED_OUT.into(),
            }),
        ),
        // `claude auth login` writes nothing over it (`credman_move_lost_a_piece`).
        Err(store::Error::Broken { why, spelling }) => fail(
            "credential",
            "credential",
            words::unreadable_to_claude_code_too(why),
            words::log_out_to_delete_it(spelling.of(ProviderId::Claude)),
        ),
        Err(e) => fail(
            "credential",
            "credential",
            e.to_string(),
            words::DO_NOT_WRITE,
        ),
    }
}

// The register's `locked_sign_in_writes_fallback` and `fallback_outlives_keychain_writes`.
pub(super) fn judge_fallback_login(facts: &Facts) -> Option<Check> {
    let left = facts.fallback_login.as_ref()?;
    let path = left.path.display();
    let (detail, advice) = match &left.fingerprint {
        Ok(Some(fingerprint)) => (
            words::file_refresh(
                path,
                if fingerprint.is_empty() {
                    words::NO_TOKEN
                } else {
                    fingerprint
                },
            ),
            words::the_file_signs_sessions_in(left.store),
        ),
        Ok(None) => (words::file_without_a_login(path), words::stow_deletes_it()),
        Err(e) => (
            words::file_not_read(path, e),
            words::delete_it_yourself(left.store, &facts.os.remove_file_command(&left.path)),
        ),
    };
    Some(warn("fallback_login", "fallback login", detail, advice))
}

// The flag moves no login out of the keychain or Credential Manager (`live_chain_order`,
// `credman_migration`): only the file's may differ.
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
        return ok("storage_v5", "storage v5", words::INACTIVE);
    }
    match facts.backend {
        Ok(store::Backend::File) => warn(
            "storage_v5",
            "storage v5",
            words::ON_IN_THE_FALLBACK_STORE,
            words::MAYBE_NOT_THE_SAME_FILE,
        ),
        _ => ok(
            "storage_v5",
            "storage v5",
            facts
                .backend
                .as_ref()
                .ok()
                .and_then(|backend| backend.secret_store())
                .map_or_else(|| words::SWITCHED_ON.to_owned(), words::on_in),
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
                said::past_credential_manager(bytes, limit),
                words::biggest(parts)
            ),
            words::NEITHER_CAN_WRITE_IT,
        ),
        Some(0) => ok("credential_size", "login size", words::in_one_item(bytes)),
        Some(pieces) => ok(
            "credential_size",
            "login size",
            words::in_pieces(bytes, pieces, crate::provider::claude::pieces::PIECE),
        ),
    }
}

pub(super) fn judge_login_restrictions(facts: &Facts) -> Option<Check> {
    if facts.login_pins.is_empty() {
        return None;
    }
    let name = "login restrictions";
    let pins = facts.login_pins.join("; ");
    Some(if facts.pinned_out.is_empty() {
        ok("login_restrictions", name, pins)
    } else {
        warn(
            "login_restrictions",
            name,
            pins,
            words::refused_as_a_session_starts(&said::listed(facts.pinned_out.clone())),
        )
    })
}

pub(super) fn judge_auth(facts: &Facts) -> Check {
    if facts.auth_overrides.is_empty() {
        let read = if facts.fallback_login.is_some() {
            words::THE_STORED_LOGIN_ONCE_READ
        } else {
            words::THE_STORED_LOGIN
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
        words::something_else(&named.join("; ")),
        words::A_LOGIN_NOTHING_READS,
    )
}

// Stated, never warned about: Claude Code ships several times a week.
pub(super) fn judge_claude_version(facts: &Facts) -> Check {
    let verified = read_from(ProviderId::Claude, facts.os);
    ok(
        "claude_version",
        "Claude Code build",
        match facts.claude_version.as_deref() {
            None => words::build_not_found(verified),
            Some(installed) if installed == verified => words::build_as_read(installed),
            Some(installed) => words::build_installed(installed, verified),
        },
    )
}

// A second writer of the login, which takes the same write lock as a switch.
pub(super) fn judge_daemon(facts: &Facts) -> Check {
    let Some(d) = &facts.daemon else {
        return ok("claude_daemon", "Claude Code daemon", words::NO_DAEMON);
    };
    let version = words::daemon_version(d.version.as_deref());
    ok(
        "claude_daemon",
        "Claude Code daemon",
        if d.running {
            words::daemon_running(&version, d.pid)
        } else {
            words::daemon_stopped(&version)
        },
    )
}
