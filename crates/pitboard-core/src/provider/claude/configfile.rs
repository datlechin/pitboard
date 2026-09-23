//! Changing the identity recorded in Claude Code's config file.
//!
//! `oauthAccount` is replaced, never removed: Claude Code refuses to write its config when
//! the key is missing on disk while a running process still has one cached. Caches are
//! dropped by shape rather than by name, because the account-derived keys change between
//! releases.

use crate::context::Context;
use crate::error::{Error, Result};
use crate::{atomic, home};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

const BACKUPS_KEPT: usize = 10;

/// An object keyed by identifiers already partitions its contents per account or
/// organisation, so it stays across a switch. Claude Code uses bare UUIDs, `acct:<uuid>`
/// and `bi1-<16 hex>` as such keys.
fn is_partitioned(value: &Value) -> bool {
    let Some(map) = value.as_object() else {
        return false;
    };
    !map.is_empty() && map.keys().all(|k| is_identifier(k))
}

fn is_identifier(key: &str) -> bool {
    let k = key.strip_prefix("acct:").unwrap_or(key);
    if let Some(rest) = k.strip_prefix("bi1-") {
        return rest.len() == 16 && rest.bytes().all(|b| b.is_ascii_hexdigit());
    }
    k.len() == 36
        && k.split('-').map(str::len).eq([8, 4, 4, 4, 12])
        && k.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

/// The value itself, or one of its direct fields, equals one of these ids exactly. Claude
/// Code's per-account caches carry the id at that depth; looking deeper would claim
/// `projects` for whichever account one project happens to mention.
fn mentions(value: &Value, identifiers: &[&str]) -> bool {
    let is_one = |v: &Value| {
        v.as_str()
            .is_some_and(|s| identifiers.iter().any(|id| !id.is_empty() && s == *id))
    };
    match value {
        Value::Object(fields) => fields.values().any(is_one),
        other => is_one(other),
    }
}

fn belongs_to(value: &Value, outgoing: &[&str]) -> bool {
    !is_partitioned(value) && mentions(value, outgoing)
}

/// Replace the recorded identity and drop what was derived from the previous one. Leaving
/// out `profileFetchedAt` makes Claude Code refetch its profile rather than trust ours.
///
/// Returns the keys it dropped. pitboard is editing the file that holds a person's whole
/// Claude Code life and deciding what to remove from it by a shape rule, so what it
/// actually removed is worth writing down rather than inferring later from a backup.
pub fn splice_identity(
    config: &mut Value,
    oauth_account: &Value,
    outgoing: &[&str],
) -> Vec<String> {
    let Some(root) = config.as_object_mut() else {
        return Vec::new();
    };

    let stale: Vec<String> = root
        .iter()
        .filter(|(k, v)| k.as_str() != "oauthAccount" && belongs_to(v, outgoing))
        .map(|(k, _)| k.clone())
        .collect();
    for key in &stale {
        root.remove(key);
    }

    let mut incoming = oauth_account.as_object().cloned().unwrap_or_else(Map::new);
    incoming.remove("profileFetchedAt");
    root.insert("oauthAccount".into(), Value::Object(incoming));
    stale
}

fn backups_dir(ctx: &Context) -> PathBuf {
    home::dir(ctx).join("backups")
}

/// Claude Code keeps a backup ring of its own, but churns through it in minutes, so it
/// cannot be relied on to still hold a pre-switch copy.
pub fn backup(ctx: &Context, path: &Path) -> Result<PathBuf> {
    let fail = |source| Error::ConfigBackupFailed {
        path: path.to_path_buf(),
        source,
    };
    let dir = backups_dir(ctx);
    home::create_private(&dir).map_err(fail)?;
    let target = dir.join(format!("claude.json.{}", ctx.now()));
    std::fs::copy(path, &target).map_err(fail)?;

    let mut existing: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(fail)?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("claude.json."))
        })
        .collect();
    existing.sort();
    for old in existing.iter().rev().skip(BACKUPS_KEPT) {
        let _ = std::fs::remove_file(old);
    }
    Ok(target)
}

/// Claude Code's file, not ours, so it keeps the permissions its owner gave it.
fn write(path: &Path, config: &Value) -> Result<()> {
    let body = serde_json::to_string(config).expect("a loaded config is always serialisable");
    atomic::write(path, body.as_bytes(), atomic::Perms::MatchExisting).map_err(|e| {
        Error::ConfigWriteFailed {
            path: path.to_path_buf(),
            detail: e.to_string(),
        }
    })
}

/// How many times to start again from what is on disk before giving up.
const ATTEMPTS: usize = 4;

/// Change Claude Code's config without losing what Claude Code wrote meanwhile.
///
/// This used to read the file, edit one key in memory, and rename a whole new file over it.
/// Anything Claude Code wrote in between was silently gone, from the file that holds a
/// person's project history, their MCP configuration and everything else they have set.
///
/// Measured on 22 September 2026 against a running session: the file is rewritten about
/// every forty seconds and every rewrite changes something, so the window is real. Claude
/// Code takes no lock on this file, so pitboard cannot take the same one, and inventing one
/// would only make pitboard's runs block a session's writes. What it can do is check that
/// the bytes it parsed are still the bytes on disk and start again from the new ones when
/// they are not. That narrows the window from a whole edit to a read and a rename; it does
/// not close it, and nothing here claims otherwise.
pub fn update(
    ctx: &Context,
    path: &Path,
    change: impl Fn(&mut Value) -> Vec<String>,
) -> Result<Vec<String>> {
    let read = || -> Result<(String, Value)> {
        let raw = std::fs::read_to_string(path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                Error::ClaudeConfigMissing {
                    path: path.to_path_buf(),
                }
            } else {
                Error::ClaudeConfigUnreadable {
                    path: path.to_path_buf(),
                    source,
                }
            }
        })?;
        let parsed = serde_json::from_str(&raw).map_err(|source| Error::ClaudeConfigNotJson {
            path: path.to_path_buf(),
            source,
        })?;
        Ok((raw, parsed))
    };

    for attempt in 1..=ATTEMPTS {
        let (before, mut config) = read()?;
        let dropped = change(&mut config);
        // Whatever Claude Code wrote between the read above and here starts this again.
        let (now, _) = read()?;
        if now != before {
            if attempt == ATTEMPTS {
                return Err(Error::ConfigWriteFailed {
                    path: path.to_path_buf(),
                    detail: format!(
                        "Claude Code rewrote it {ATTEMPTS} times while pitboard was changing \
                         it, so pitboard did not write rather than write over what Claude \
                         Code had just put there"
                    ),
                });
            }
            continue;
        }
        write(path, &config)?;
        // What was removed from a person's own file, written down rather than inferred
        // later from a backup.
        if !dropped.is_empty() {
            crate::audit::record(ctx, "config", &dropped.join(" "), "dropped");
        }
        return Ok(dropped);
    }
    unreachable!("the loop returns or fails on its last attempt")
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::time::{Clock, FixedClock};
    use std::sync::Arc;

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn scratch(name: &str) -> (Context, PathBuf, Scratch) {
        let root = std::env::temp_dir().join(format!(
            "pitboard-config-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a scratch home");
        let ctx = Context::new(root.clone())
            .with_pitboard_home(root.join(".pitboard"))
            .with_clock(Arc::new(FixedClock::at(1_760_000_000)) as Arc<dyn Clock>);
        home::ensure(&ctx).expect("a pitboard home, which is where the record goes");
        let path = root.join(".claude.json");
        std::fs::write(&path, serde_json::json!({"numStartups": 1}).to_string()).expect("a config");
        (ctx, path, Scratch(root))
    }

    /// Measured against a running session on 22 September 2026: this file is rewritten
    /// about every forty seconds and every rewrite changes something. Before this, anything
    /// Claude Code wrote between pitboard reading the file and renaming a new one over it
    /// was silently gone.
    #[test]
    fn a_write_that_would_lose_what_claude_code_just_wrote_does_not_happen() {
        let (ctx, path, _s) = scratch("lost-update");
        let interfering = std::cell::Cell::new(0);

        let outcome = update(&ctx, &path, |config| {
            // Claude Code writes the file while pitboard is deciding what to change.
            interfering.set(interfering.get() + 1);
            std::fs::write(
                &path,
                serde_json::json!({"numStartups": interfering.get() + 1}).to_string(),
            )
            .expect("claude code writes");
            config["pitboardWasHere"] = serde_json::json!(true);
            Vec::new()
        });

        assert!(
            matches!(outcome, Err(Error::ConfigWriteFailed { .. })),
            "got {outcome:?}"
        );
        assert_eq!(interfering.get(), ATTEMPTS, "it tried, then stopped");
        let on_disk: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert!(
            on_disk.get("pitboardWasHere").is_none(),
            "nothing of pitboard's was written over what Claude Code put there"
        );
        assert_eq!(on_disk["numStartups"], ATTEMPTS as i64 + 1);
    }

    #[test]
    fn a_quiet_file_is_written_once_and_what_was_dropped_is_recorded() {
        let (ctx, path, _s) = scratch("quiet");
        std::fs::write(
            &path,
            serde_json::json!({
                "oauthAccount": {"accountUuid": OLD_ACCOUNT},
                "cachedArtifactRoster": {"org": OLD_ORG},
                "numStartups": 7,
            })
            .to_string(),
        )
        .expect("a config");

        let dropped = update(&ctx, &path, |config| {
            splice_identity(
                config,
                &serde_json::json!({"accountUuid": NEW_ACCOUNT}),
                &[OLD_ACCOUNT, OLD_ORG],
            )
        })
        .expect("written");

        assert_eq!(dropped, vec!["cachedArtifactRoster".to_string()]);
        let on_disk: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(on_disk["oauthAccount"]["accountUuid"], NEW_ACCOUNT);
        assert_eq!(on_disk["numStartups"], 7);

        // And it is in the record, so a person can see what pitboard took out of their file.
        let said = crate::audit::read(&ctx, 10);
        assert!(
            said.iter()
                .any(|e| e.verb == "config" && e.subject.contains("cachedArtifactRoster")),
            "{said:?}"
        );
    }

    const OLD_ACCOUNT: &str = "7c6b5a49-3827-4165-a4b3-c2d1e0f9a8b7";
    const OLD_ORG: &str = "3e2d1c0b-5a49-4837-9261-f0e1d2c3b4a5";
    const NEW_ACCOUNT: &str = "1f0e2d3c-4b5a-4968-8776-a5b4c3d2e1f0";

    fn config() -> Value {
        serde_json::json!({
            "oauthAccount": {"accountUuid": OLD_ACCOUNT, "emailAddress": "old@x.com",
                             "profileFetchedAt": 1789871209282i64},
            "groveConfigCache": {OLD_ACCOUNT: {"a": 1}, NEW_ACCOUNT: {"a": 2}},
            "passesEligibilityCache": {OLD_ORG: {"eligible": true}},
            "clientDataCacheSlots": {"bi1-0123456789abcdef": {"x": 1}},
            "cachedArtifactRoster": {"org": OLD_ORG, "items": []},
            "cachedUsageUtilization": {"accountUuid": OLD_ACCOUNT, "utilization": {}},
            "numStartups": 412,
            "autoUpdates": true,
            "projects": {"/Users/x/code": {"allowedTools": []}}
        })
    }

    #[test]
    fn partitioned_caches_survive_a_switch() {
        let mut c = config();
        splice_identity(
            &mut c,
            &serde_json::json!({"accountUuid": NEW_ACCOUNT}),
            &[OLD_ACCOUNT, OLD_ORG],
        );
        // Keyed by account or organisation, so both accounts' entries coexist safely.
        assert!(c.get("groveConfigCache").is_some());
        assert!(c.get("passesEligibilityCache").is_some());
        assert!(c.get("clientDataCacheSlots").is_some());
    }

    #[test]
    fn unpartitioned_caches_of_the_outgoing_account_are_dropped() {
        let mut c = config();
        splice_identity(
            &mut c,
            &serde_json::json!({"accountUuid": NEW_ACCOUNT}),
            &[OLD_ACCOUNT, OLD_ORG],
        );
        assert!(c.get("cachedArtifactRoster").is_none());
        assert!(c.get("cachedUsageUtilization").is_none());
    }

    #[test]
    fn machine_and_preference_state_is_left_alone() {
        let mut c = config();
        splice_identity(
            &mut c,
            &serde_json::json!({"accountUuid": NEW_ACCOUNT}),
            &[OLD_ACCOUNT, OLD_ORG],
        );
        assert_eq!(c["numStartups"], 412);
        assert_eq!(c["autoUpdates"], true);
        assert!(c["projects"]["/Users/x/code"].is_object());
    }

    #[test]
    fn oauth_account_is_replaced_never_removed() {
        let mut c = config();
        splice_identity(
            &mut c,
            &serde_json::json!({"accountUuid": NEW_ACCOUNT, "emailAddress": "new@x.com"}),
            &[OLD_ACCOUNT, OLD_ORG],
        );
        assert_eq!(c["oauthAccount"]["accountUuid"], NEW_ACCOUNT);
        assert_eq!(c["oauthAccount"]["emailAddress"], "new@x.com");
    }

    #[test]
    fn profile_fetched_at_is_stripped_so_claude_code_refetches() {
        let mut c = config();
        let incoming = serde_json::json!({"accountUuid": NEW_ACCOUNT, "profileFetchedAt": 999});
        splice_identity(&mut c, &incoming, &[OLD_ACCOUNT, OLD_ORG]);
        assert!(c["oauthAccount"].get("profileFetchedAt").is_none());
    }

    #[test]
    fn identifier_shapes_match_what_claude_code_actually_uses() {
        assert!(is_identifier(OLD_ACCOUNT));
        assert!(is_identifier(&format!("acct:{OLD_ACCOUNT}")));
        assert!(is_identifier("bi1-0123456789abcdef"));
        for not in [
            "numStartups",
            "bi1-short",
            "bi1-0123456789abcdeZ",
            "",
            "a-b-c-d-e",
        ] {
            assert!(
                !is_identifier(not),
                "{not:?} should not read as an identifier"
            );
        }
    }

    #[test]
    fn an_empty_object_is_not_treated_as_partitioned() {
        assert!(!is_partitioned(&serde_json::json!({})));
    }

    #[test]
    fn a_container_with_the_outgoing_id_buried_deep_inside_is_kept() {
        let mut c = config();
        c["projects"]["/Users/x/code"]["lastSession"] =
            serde_json::json!({"org": OLD_ORG, "at": 1});
        splice_identity(&mut c, &serde_json::json!({}), &[OLD_ACCOUNT, OLD_ORG]);
        assert!(
            c["projects"]["/Users/x/code"].is_object(),
            "every project's settings would have been dropped"
        );
    }

    #[test]
    fn an_id_that_merely_contains_the_outgoing_one_is_not_a_match() {
        let mut c = config();
        c["unrelated"] = serde_json::json!({"note": format!("prefix-{OLD_ACCOUNT}-suffix")});
        splice_identity(&mut c, &serde_json::json!({}), &[OLD_ACCOUNT, OLD_ORG]);
        assert!(c.get("unrelated").is_some());
    }

    #[test]
    fn a_cache_naming_neither_uuid_is_kept() {
        let mut c = config();
        c["somethingNew"] = serde_json::json!({"unrelated": "value"});
        splice_identity(&mut c, &serde_json::json!({}), &[OLD_ACCOUNT, OLD_ORG]);
        assert!(c.get("somethingNew").is_some());
    }
}
