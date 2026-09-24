//! What a session here would authenticate as, read from files rather than guessed from
//! three environment variables.
//!
//! pitboard decided whether a switch would mean anything by reading `ANTHROPIC_API_KEY`,
//! `ANTHROPIC_AUTH_TOKEN` and `CLAUDE_CODE_OAUTH_TOKEN` out of its own process
//! environment. Two holes followed. Claude Code resolves authentication from a layered
//! settings system, and any layer can set an `env` block, an `apiKeyHelper`, or one of the
//! third-party provider switches; under any of those a session ignores the login pitboard
//! moves, and pitboard reported a clean switch. And the app has no shell environment at
//! all: launched from Finder it saw none of the three, so the one surface that could not
//! warn was the one most likely to be used on a machine that needed the warning.
//!
//! Files are the one thing an app launched from Finder and a shell agree about. Measured in
//! 2.1.278: managed settings live in `/Library/Application Support/ClaudeCode` on macOS and
//! `/etc/claude-code` elsewhere, as `managed-settings.json` and a `managed-settings.d`
//! drop-in directory beside it; a person's own are `<config dir>/settings.json`.
//!
//! Project settings are deliberately not read. `.claude/settings.json` is a fact about one
//! directory, not about the machine, and an answer true in the directory pitboard happened
//! to be run from is worse than no answer at all.

use crate::context::Context;
use crate::provider::claude::paths as claude;
use serde_json::Value;
use std::path::PathBuf;

/// Set, any of these makes Claude Code authenticate with something other than the login in
/// the credential store, so moving that login changes nothing a session would notice.
pub(crate) const OVERRIDING_ENV: [&str; 9] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
    "CLAUDE_CODE_USE_GATEWAY",
    "CLAUDE_CODE_USE_ANTHROPIC_AWS",
    "CLAUDE_CODE_USE_ANTHROPIC_GOOGLE_CLOUD",
];

/// One reason a session would not use the login pitboard moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Override {
    /// What said so: a path, or "the environment".
    pub layer: String,
    /// The setting or variable that did it.
    pub key: String,
}

impl std::fmt::Display for Override {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} in {}", self.key, self.layer)
    }
}

/// Where managed settings are, which is a fact about the machine and not about the person.
fn managed_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Application Support/ClaudeCode")
    } else {
        PathBuf::from("/etc/claude-code")
    }
}

fn managed_files() -> Vec<PathBuf> {
    let dir = managed_dir();
    let mut files = vec![dir.join("managed-settings.json")];
    if let Ok(entries) = std::fs::read_dir(dir.join("managed-settings.d")) {
        let mut drop_ins: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        drop_ins.sort();
        files.extend(drop_ins);
    }
    files
}

/// A value Claude Code would read as set. An empty string, `0` and `false` are not.
fn is_set(value: &str) -> bool {
    !matches!(value.trim(), "" | "0" | "false")
}

fn overrides_in(path: &std::path::Path) -> Vec<Override> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<Value>(&raw) else {
        return Vec::new();
    };
    let layer = path.display().to_string();
    let mut found = Vec::new();
    // A helper that prints an API key is an override whatever it prints.
    if json
        .get("apiKeyHelper")
        .and_then(Value::as_str)
        .is_some_and(|h| !h.trim().is_empty())
    {
        found.push(Override {
            layer: layer.clone(),
            key: "apiKeyHelper".into(),
        });
    }
    if let Some(env) = json.get("env").and_then(Value::as_object) {
        for name in OVERRIDING_ENV {
            let set = match env.get(name) {
                Some(Value::String(v)) => is_set(v),
                Some(Value::Bool(b)) => *b,
                Some(Value::Number(n)) => n.as_i64() != Some(0),
                _ => false,
            };
            if set {
                found.push(Override {
                    layer: layer.clone(),
                    key: format!("env.{name}"),
                });
            }
        }
    }
    found
}

/// Whether a custom OAuth endpoint is configured. Set, it renames both the keychain item
/// and the config file Claude Code uses, so pitboard would be reading and writing the wrong
/// ones: a refusal rather than a warning.
///
/// Read from files as well as from this process's environment, because a `CLAUDE_CODE_CUSTOM_OAUTH_URL`
/// in a settings file is just as real and the app could not see the environment one.
pub fn custom_oauth(ctx: &Context) -> bool {
    if ctx.custom_oauth() {
        return true;
    }
    managed_files()
        .iter()
        .chain(std::iter::once(
            &claude::config_dir(ctx).join("settings.json"),
        ))
        .any(|path| {
            std::fs::read_to_string(path)
                .ok()
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                .and_then(|json| {
                    json.get("env")?
                        .get("CLAUDE_CODE_CUSTOM_OAUTH_URL")?
                        .as_str()
                        .map(is_set)
                })
                .unwrap_or(false)
        })
}

/// Every reason a session started here would authenticate as something other than the login
/// pitboard moves. Empty means a switch changes what a session sees, which is the answer
/// pitboard needs before it moves anything.
pub fn overrides(ctx: &Context) -> Vec<Override> {
    let mut found: Vec<Override> = managed_files()
        .iter()
        .chain(std::iter::once(
            &claude::config_dir(ctx).join("settings.json"),
        ))
        .flat_map(|path| overrides_in(path))
        .collect();
    // What this process was started with, which a command line sees and an app does not.
    found.extend(ctx.overriding_auth().iter().map(|key| Override {
        layer: "the environment".into(),
        key: key.clone(),
    }));
    found
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;

    pub(crate) struct Scratch(pub(crate) PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    pub(crate) fn home(name: &str) -> (Context, PathBuf, Scratch) {
        let root = std::env::temp_dir().join(format!(
            "pitboard-settings-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let config = root.join(".claude");
        std::fs::create_dir_all(&config).expect("a config dir");
        let ctx = Context::new(root.clone())
            .with_claude_config_dir(config.to_string_lossy().into_owned());
        (ctx, config, Scratch(root))
    }

    pub(crate) fn write(path: &std::path::Path, value: Value) {
        std::fs::write(path, value.to_string()).expect("written");
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::*;
    use super::*;
    use serde_json::json;

    #[test]
    fn an_ordinary_machine_has_nothing_in_the_way() {
        let (ctx, config, _s) = home("clean");
        write(&config.join("settings.json"), json!({"theme": "dark"}));
        assert_eq!(overrides(&ctx), Vec::new());
    }

    #[test]
    fn a_settings_file_that_sets_a_key_is_found_where_the_environment_would_not_be() {
        let (ctx, config, _s) = home("env-block");
        write(
            &config.join("settings.json"),
            json!({"env": {"ANTHROPIC_API_KEY": "sk-ant-something"}}),
        );
        let found = overrides(&ctx);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].key, "env.ANTHROPIC_API_KEY");
        assert!(found[0].layer.ends_with("settings.json"));
    }

    #[test]
    fn a_helper_that_prints_a_key_is_an_override_whatever_it_prints() {
        let (ctx, config, _s) = home("helper");
        write(
            &config.join("settings.json"),
            json!({"apiKeyHelper": "/usr/local/bin/get-key"}),
        );
        assert_eq!(overrides(&ctx)[0].key, "apiKeyHelper");
    }

    #[test]
    fn a_third_party_provider_is_an_override_too() {
        let (ctx, config, _s) = home("bedrock");
        write(
            &config.join("settings.json"),
            json!({"env": {"CLAUDE_CODE_USE_BEDROCK": "1"}}),
        );
        assert_eq!(overrides(&ctx)[0].key, "env.CLAUDE_CODE_USE_BEDROCK");
    }

    /// Claude Code reads an empty string, `0` and `false` as not set, and so does this.
    #[test]
    fn a_key_that_is_present_but_empty_sets_nothing() {
        let (ctx, config, _s) = home("empty");
        write(
            &config.join("settings.json"),
            json!({
                "apiKeyHelper": "  ",
                "env": {
                    "ANTHROPIC_API_KEY": "",
                    "CLAUDE_CODE_USE_BEDROCK": "0",
                    "CLAUDE_CODE_USE_VERTEX": "false",
                },
            }),
        );
        assert_eq!(overrides(&ctx), Vec::new());
    }

    #[test]
    fn a_settings_file_that_is_not_json_is_not_a_reason_to_refuse_anything() {
        let (ctx, config, _s) = home("broken");
        std::fs::write(config.join("settings.json"), "{ not json").expect("written");
        assert_eq!(overrides(&ctx), Vec::new());
    }
}

#[cfg(test)]
mod custom_oauth_tests {
    use super::tests_support::*;
    use super::*;
    use serde_json::json;

    #[test]
    fn a_custom_endpoint_in_a_settings_file_is_found_the_way_the_environment_one_is() {
        let (ctx, config, _s) = home("custom-oauth");
        assert!(!custom_oauth(&ctx));
        write(
            &config.join("settings.json"),
            json!({"env": {"CLAUDE_CODE_CUSTOM_OAUTH_URL": "https://oauth.example"}}),
        );
        assert!(custom_oauth(&ctx));
    }
}
