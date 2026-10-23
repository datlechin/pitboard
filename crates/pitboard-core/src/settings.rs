//! What a session here would authenticate as, read from files rather than guessed from
//! three environment variables.
//!
//! Pitboard decided whether a switch would mean anything by reading `ANTHROPIC_API_KEY`,
//! `ANTHROPIC_AUTH_TOKEN` and `CLAUDE_CODE_OAUTH_TOKEN` out of its own process
//! environment. Two holes followed. Claude Code resolves authentication from a layered
//! settings system, and any layer can set an `env` block, an `apiKeyHelper`, or one of the
//! third-party provider switches; under any of those a session ignores the login Pitboard
//! moves, and Pitboard reported a clean switch. And the app has no shell environment at
//! all: launched from Finder it saw none of the three, so the one surface that could not
//! warn was the one most likely to be used on a machine that needed the warning.
//!
//! Files are the one thing an app launched from Finder and a shell agree about. Measured in
//! 2.1.278: managed settings live in `/Library/Application Support/ClaudeCode` on macOS and
//! `/etc/claude-code` on Linux, as `managed-settings.json` and a `managed-settings.d`
//! drop-in directory beside it; a person's own are `<config dir>/settings.json`. Read from
//! 2.1.300, Windows keeps them in `C:\Program Files\ClaudeCode` and in the registry's
//! policy value `Settings` (`windows_managed_settings` in the register).
//!
//! Project settings are deliberately not read. `.claude/settings.json` is a fact about one
//! directory, not about the machine, and an answer true in the directory Pitboard happened
//! to be run from is worse than no answer at all.

use crate::context::{Context, Environment};
use crate::host::{Administered, Hive, Os};
use crate::provider::claude::paths as claude;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Set, any of these makes Claude Code authenticate with something other than the login in
/// the credential store, so moving that login changes nothing a session would notice.
pub(crate) const OVERRIDING_ENV: [&str; 10] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
    "CLAUDE_CODE_USE_GATEWAY",
    "CLAUDE_CODE_USE_ANTHROPIC_AWS",
    "CLAUDE_CODE_USE_ANTHROPIC_GOOGLE_CLOUD",
    "CLAUDE_CODE_USE_MANTLE",
];

/// One reason a session would not use the login Pitboard moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Override {
    /// What said so: a path, or "the environment".
    pub(crate) layer: String,
    /// The setting or variable that did it.
    pub(crate) key: String,
}

impl std::fmt::Display for Override {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} in {}", self.key, self.layer)
    }
}

pub(crate) fn managed_dir(os: Os) -> PathBuf {
    match os {
        Os::MacOs => "/Library/Application Support/ClaudeCode".into(),
        Os::Linux => "/etc/claude-code".into(),
        Os::Windows => r"C:\Program Files\ClaudeCode".into(),
    }
}

// Under `HKLM` and `HKCU` (`windows_managed_settings`).
const POLICIES: &str = r"SOFTWARE\Policies\ClaudeCode";
const POLICY: &str = "Settings";

struct Layer {
    name: String,
    text: String,
}

fn managed_files(ctx: &Context) -> Vec<PathBuf> {
    let dir = managed_dir(ctx.os);
    let mut files = vec![dir.join("managed-settings.json")];
    let mut drop_ins: Vec<PathBuf> = ctx
        .host()
        .administered_folder(&dir.join("managed-settings.d"))
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    drop_ins.sort();
    files.extend(drop_ins);
    files
}

// `HKLM` wins over the folder unless they merge, so reading both finds more, never less.
fn managed(ctx: &Context) -> Vec<Layer> {
    let host = ctx.host();
    let mut there = false;
    let mut layers: Vec<Layer> = managed_files(ctx)
        .into_iter()
        .filter_map(|path| match host.administered_file(&path) {
            Administered::Set(text) => {
                there |= holds_policy(&text);
                Some(administered(path.display().to_string(), &text))
            }
            Administered::Unreadable(_) => {
                there = true;
                None
            }
            Administered::Unset => None,
        })
        .collect();
    if ctx.os == Os::Windows {
        for hive in [Hive::Machine, Hive::User] {
            if hive == Hive::User && there {
                break;
            }
            match host.policy(hive, POLICIES, POLICY) {
                Administered::Set(text) => {
                    there |= holds_policy(&text);
                    layers.push(administered(
                        format!(r"{}\{POLICIES}\{POLICY}", hive.name()),
                        &text,
                    ));
                }
                // Claude Code treats a failed HKLM query as absent and still reads HKCU (its
                // `vYn` nulls hklmStdout on a non-zero `reg.exe` exit), so an unreadable
                // machine policy does not keep HKCU out.
                Administered::Unreadable(_) => there |= hive != Hive::Machine,
                Administered::Unset => {}
            }
        }
    }
    layers
}

// The managed sources a session takes, as Claude Code picks them: `HKLM` over the folder unless
// it merges them, `HKCU` where neither holds a policy. Err names one Pitboard cannot read that
// may be taken.
fn managed_taken(ctx: &Context) -> Result<Vec<Layer>, String> {
    let host = ctx.host();
    let mut folder = Vec::new();
    let mut unread = None;
    for path in managed_files(ctx) {
        match host.administered_file(&path) {
            Administered::Set(text) => {
                folder.push(administered(path.display().to_string(), &text));
            }
            Administered::Unreadable(why) => {
                unread.get_or_insert(format!("{}: {why}", path.display()));
            }
            Administered::Unset => {}
        }
    }
    let folder = || unread.map_or(Ok(folder), Err);
    if ctx.os != Os::Windows {
        return folder();
    }
    let policy = |hive: Hive| {
        let name = format!(r"{}\{POLICIES}\{POLICY}", hive.name());
        match host.policy(hive, POLICIES, POLICY) {
            Administered::Set(text) if holds_policy(&text) => Ok(Some(administered(name, &text))),
            Administered::Set(_) | Administered::Unset => Ok(None),
            Administered::Unreadable(why) => Err(format!("{name}: {why}")),
        }
    };
    if let Some(machine) = policy(Hive::Machine)? {
        let merges = serde_json::from_str::<Value>(&machine.text)
            .ok()
            .and_then(|json| Some(json.get("managedSourcesBehavior")?.as_str()? == "merge"))
            .unwrap_or(false);
        let mut taken = vec![machine];
        if merges {
            taken.extend(folder()?);
        }
        return Ok(taken);
    }
    let folder = folder()?;
    if folder.iter().any(|layer| holds_policy(&layer.text)) {
        return Ok(folder);
    }
    Ok(policy(Hive::User)?.into_iter().collect())
}

// What keeps `HKCU` out: one Claude Code cannot read, or one that sets more than composition.
fn holds_policy(text: &str) -> bool {
    let text = claude::without_byte_order_mark(text);
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(keys)) => keys.keys().any(|key| {
            !matches!(
                key.as_str(),
                "managedSourcesBehavior" | "wslInheritsWindowsSettings"
            )
        }),
        Ok(_) | Err(_) => !text.trim().is_empty(),
    }
}

/// A value Claude Code would read as set. An empty string, `0` and `false` are not.
fn is_set(value: &str) -> bool {
    !matches!(value.trim(), "" | "0" | "false")
}

// As Claude Code reads a settings file: UTF-16LE after FF FE, UTF-8 otherwise, and one byte
// order mark dropped (`settings_byte_order_mark`).
fn own(path: &Path) -> Option<Layer> {
    let bytes = std::fs::read(path).ok()?;
    let text = match bytes.strip_prefix(&[0xFF, 0xFE]) {
        Some(wide) => String::from_utf16(
            &wide
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes(*pair))
                .collect::<Vec<_>>(),
        )
        .ok()?,
        None => String::from_utf8(bytes).ok()?,
    };
    Some(Layer {
        name: path.display().to_string(),
        text: claude::without_byte_order_mark(&text).to_owned(),
    })
}

fn administered(name: String, text: &str) -> Layer {
    Layer {
        name,
        text: claude::without_byte_order_mark(text).to_owned(),
    }
}

fn overrides_in(layer: &Layer) -> Vec<Override> {
    let Ok(json) = serde_json::from_str::<Value>(&layer.text) else {
        return Vec::new();
    };
    let layer = layer.name.clone();
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

// Set in managed settings, these send every session through the Cloud gateway (auth_precedence).
fn gateway_in(layer: &Layer) -> Vec<Override> {
    let Ok(json) = serde_json::from_str::<Value>(&layer.text) else {
        return Vec::new();
    };
    let required = [
        json.get("forceLoginMethod").and_then(Value::as_str) == Some("gateway"),
        json.get("forceLoginGatewayUrl")
            .and_then(Value::as_str)
            .is_some_and(|url| !url.is_empty()),
    ];
    ["forceLoginMethod", "forceLoginGatewayUrl"]
        .into_iter()
        .zip(required)
        .filter(|(_, set)| *set)
        .map(|(key, _)| Override {
            layer: layer.name.clone(),
            key: key.into(),
        })
        .collect()
}

// Managed settings, then the `settings.json` given.
fn layered(ctx: &Context, settings: &Path) -> Vec<Override> {
    managed(ctx)
        .iter()
        .flat_map(|layer| overrides_in(layer).into_iter().chain(gateway_in(layer)))
        .chain(own(settings).iter().flat_map(overrides_in))
        .collect()
}

const FEDERATION: &str = "oidc_federation";
const OAUTH: &str = "user_oauth";

// Where Claude Code finds its Anthropic profiles, and which one the environment names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Profiles {
    folder: Option<PathBuf>,
    named: Option<String>,
    federated: bool,
}

impl Profiles {
    // Each trimmed, as Claude Code reads them, and an empty one unset.
    pub(crate) fn read(env: &Environment) -> Profiles {
        let set = |name| env.text(name).map(str::trim).filter(|v| !v.is_empty());
        let under = |name, path: &[&str]| {
            set(name).map(|dir| path.iter().fold(PathBuf::from(dir), |p, part| p.join(part)))
        };
        let folder = set("ANTHROPIC_CONFIG_DIR")
            .map(PathBuf::from)
            .or_else(|| match env.os() {
                Os::Windows => under("APPDATA", &["Anthropic"])
                    .or_else(|| under("USERPROFILE", &["AppData", "Roaming", "Anthropic"])),
                Os::MacOs | Os::Linux => under("XDG_CONFIG_HOME", &["anthropic"])
                    .or_else(|| under("HOME", &[".config", "anthropic"])),
            });
        Profiles {
            folder,
            named: set("ANTHROPIC_PROFILE").map(str::to_owned),
            federated: set("ANTHROPIC_FEDERATION_RULE_ID").is_some()
                && set("ANTHROPIC_ORGANIZATION_ID").is_some(),
        }
    }
}

fn profile_file(folder: &Path, name: &str) -> PathBuf {
    folder.join("configs").join(format!("{name}.json"))
}

// A profile's `authentication.type`. Claude Code takes a `user_oauth` one only with a login in
// its credentials file, which a file that is there and not empty stands for: it holds a token.
fn mode(folder: &Path, name: &str) -> Option<String> {
    let profile: Value =
        serde_json::from_str(&std::fs::read_to_string(profile_file(folder, name)).ok()?).ok()?;
    let authentication = profile.get("authentication")?;
    let mode = authentication.get("type")?.as_str()?;
    if mode == OAUTH {
        let credentials = authentication
            .get("credentials_path")
            .and_then(Value::as_str)
            .map_or_else(
                || folder.join("credentials").join(format!("{name}.json")),
                PathBuf::from,
            );
        std::fs::metadata(credentials)
            .ok()
            .filter(|file| file.is_file() && file.len() > 0)?;
    }
    Some(mode.to_owned())
}

// The profile a session takes before the stored login: the one `ANTHROPIC_PROFILE` names, else
// the federation both variables set, else the active profile where it federates. Claude Code
// takes a claude.ai login before an active profile that signs in with OAuth.
fn profile(ctx: &Context) -> Option<Override> {
    let Profiles {
        folder,
        named,
        federated,
    } = &ctx.anthropic;
    let environment = |key: &str| Override {
        layer: "the environment".into(),
        key: key.into(),
    };
    if let Some(name) = named {
        let mode = mode(folder.as_deref()?, name)?;
        return [FEDERATION, OAUTH]
            .contains(&mode.as_str())
            .then(|| environment("ANTHROPIC_PROFILE"));
    }
    if *federated {
        return Some(environment(
            "ANTHROPIC_FEDERATION_RULE_ID and ANTHROPIC_ORGANIZATION_ID",
        ));
    }
    let folder = folder.as_deref()?;
    let active = std::fs::read_to_string(folder.join("active_config"))
        .ok()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "default".into());
    (mode(folder, &active)? == FEDERATION).then(|| Override {
        layer: profile_file(folder, &active).display().to_string(),
        key: format!("the `{FEDERATION}` profile `{active}`"),
    })
}

// Each managed source a session takes that sets `forceLoginOrgUUID`, with the organisations it
// names (organisation_pin). None where one cannot be read: Claude Code then refuses every login.
pub(crate) fn organisation_pins(ctx: &Context) -> Vec<(String, Vec<String>)> {
    managed_taken(ctx)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|layer| {
            let json = serde_json::from_str::<Value>(&layer.text).ok()?;
            let ids = match json.get("forceLoginOrgUUID")? {
                Value::String(id) => vec![id.clone()],
                Value::Array(ids) => ids
                    .iter()
                    .map(|id| id.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()?,
                _ => return None,
            };
            Some((layer.name, ids))
        })
        .collect()
}

// It renames Claude Code's keychain item and config file, so it is a refusal, not a warning.
pub(crate) fn custom_oauth(ctx: &Context) -> bool {
    if ctx.custom_oauth() {
        return true;
    }
    managed(ctx)
        .into_iter()
        .chain(own(&claude::config_dir(ctx).join("settings.json")))
        .any(|layer| {
            serde_json::from_str::<Value>(&layer.text)
                .ok()
                .and_then(|json| {
                    json.get("env")?
                        .get("CLAUDE_CODE_CUSTOM_OAUTH_URL")?
                        .as_str()
                        .map(is_set)
                })
                .unwrap_or(false)
        })
}

// Each read where no descriptor is named for it, and taken before `apiKeyHelper` and the stored
// login (`remote_token_files`).
const OAUTH_TOKEN: (&str, &str) = (".oauth_token", "the OAuth token");
const API_KEY: (&str, &str) = (".api_key", "the API key");
// Claude Code's, past which it takes neither.
const REMOTE_TOKEN_LIMIT: u64 = 65_536;

// On Windows the path names no drive, so it is at the root of whichever drive a session runs
// from.
fn remote_tokens(ctx: &Context, files: &[(&str, &str)]) -> Vec<Override> {
    let folders: Vec<String> = match ctx.os {
        Os::Windows => ctx
            .host()
            .fixed_drives()
            .into_iter()
            .map(|drive| format!(r"{drive}:\home\claude\.claude\remote\"))
            .collect(),
        Os::MacOs | Os::Linux => vec!["/home/claude/.claude/remote/".into()],
    };
    folders
        .iter()
        .flat_map(|folder| {
            files
                .iter()
                .map(move |(file, what)| (format!("{folder}{file}"), *what))
        })
        .filter(|(path, _)| {
            matches!(
                ctx.host()
                    .administered_file_up_to(Path::new(path), REMOTE_TOKEN_LIMIT),
                Administered::Set(text) if !text.trim().is_empty()
            )
        })
        .map(|(layer, what)| Override {
            layer,
            key: what.into(),
        })
        .collect()
}

pub(crate) fn overrides(ctx: &Context) -> Vec<Override> {
    let mut found = layered(ctx, &claude::config_dir(ctx).join("settings.json"));
    found.extend(profile(ctx));
    found.extend(remote_tokens(ctx, &[OAUTH_TOKEN, API_KEY]));
    // What this process was started with, which a command line sees and an app does not.
    found.extend(ctx.overriding_auth().iter().map(|key| Override {
        layer: "the environment".into(),
        key: key.clone(),
    }));
    found
}

// One settings file's or policy's `env` value for a variable.
pub(crate) struct EnvSetting {
    pub(crate) layer: String,
    pub(crate) value: Value,
    pub(crate) managed: bool,
}

// Highest first: the managed sources a session takes, then the person's own settings.json, which
// Claude Code assigns before them (`windows_backend_choice`). A name in any case, as Windows
// takes one. Err names a managed source Pitboard cannot read that may be taken.
pub(crate) fn env_settings(ctx: &Context, name: &str) -> Result<Vec<EnvSetting>, String> {
    let managed = managed_taken(ctx)?.into_iter().map(|layer| (layer, true));
    let own = own(&claude::config_dir(ctx).join("settings.json")).map(|layer| (layer, false));
    Ok(managed
        .chain(own)
        .filter_map(|(layer, managed)| {
            let json = serde_json::from_str::<Value>(&layer.text).ok()?;
            let value = json
                .get("env")?
                .as_object()?
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))?
                .1
                .clone();
            Some(EnvSetting {
                layer: layer.name,
                value,
                managed,
            })
        })
        .collect())
}

// Managed settings' over the person's own, as each wins over the one below it.
pub(crate) fn status_line_command(ctx: &Context) -> Option<String> {
    managed(ctx)
        .into_iter()
        .chain(own(&claude::config_dir(ctx).join("settings.json")))
        .find_map(|layer| {
            serde_json::from_str::<Value>(&layer.text)
                .ok()?
                .get("statusLine")?
                .get("command")?
                .as_str()
                .map(str::to_owned)
        })
}

// A warm-up's run gets none of this process's environment, and its own token comes before
// `.oauth_token` and any profile.
pub(crate) fn overrides_of_a_run_in(ctx: &Context, config: &Path) -> Vec<Override> {
    let mut found = layered(ctx, &config.join("settings.json"));
    found.extend(remote_tokens(ctx, &[API_KEY]));
    found
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
pub(crate) mod tests_support {
    use super::*;

    pub(crate) fn home() -> (Context, PathBuf, tempfile::TempDir) {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let config = scratch.path().join(".claude");
        std::fs::create_dir_all(&config).expect("a config dir");
        let ctx = Context::new(scratch.path().to_path_buf())
            .with_claude_config_dir(config.to_string_lossy().into_owned());
        (ctx, config, scratch)
    }

    pub(crate) fn write(path: &Path, value: Value) {
        std::fs::write(path, value.to_string()).expect("written");
    }
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::tests_support::*;
    use super::*;
    use serde_json::json;

    #[test]
    fn the_status_line_command_is_read_from_the_persons_settings() {
        let (ctx, config, _scratch) = home();
        assert_eq!(status_line_command(&ctx), None);
        write(
            &config.join("settings.json"),
            json!({"statusLine": {"type": "command", "command": "pitboard statusline"}}),
        );
        assert_eq!(
            status_line_command(&ctx).as_deref(),
            Some("pitboard statusline")
        );
        write(&config.join("settings.json"), json!({"statusLine": {}}));
        assert_eq!(status_line_command(&ctx), None);
    }

    #[test]
    fn a_settings_file_that_starts_with_a_byte_order_mark_is_read() {
        let (ctx, config, _s) = home();
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend(
            json!({
                "env": {"ANTHROPIC_API_KEY": "sk-ant-something"},
                "statusLine": {"type": "command", "command": "pitboard statusline"},
            })
            .to_string()
            .into_bytes(),
        );
        std::fs::write(config.join("settings.json"), bytes).expect("written");
        assert_eq!(overrides(&ctx).len(), 1);
        assert_eq!(
            status_line_command(&ctx).as_deref(),
            Some("pitboard statusline")
        );
    }

    #[test]
    fn an_ordinary_machine_has_nothing_in_the_way() {
        let (ctx, config, _s) = home();
        write(&config.join("settings.json"), json!({"theme": "dark"}));
        assert_eq!(overrides(&ctx), Vec::new());
    }

    #[test]
    fn a_settings_file_that_sets_a_key_is_found_where_the_environment_would_not_be() {
        let (ctx, config, _s) = home();
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
        let (ctx, config, _s) = home();
        write(
            &config.join("settings.json"),
            json!({"apiKeyHelper": "/usr/local/bin/get-key"}),
        );
        assert_eq!(overrides(&ctx)[0].key, "apiKeyHelper");
    }

    #[test]
    fn a_third_party_provider_is_an_override_too() {
        let (ctx, config, _s) = home();
        for name in ["CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_MANTLE"] {
            write(&config.join("settings.json"), json!({"env": {name: "1"}}));
            assert_eq!(overrides(&ctx)[0].key, format!("env.{name}"));
        }
    }

    /// Claude Code reads an empty string, `0` and `false` as not set, and so does this.
    #[test]
    fn a_key_that_is_present_but_empty_sets_nothing() {
        let (ctx, config, _s) = home();
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
        let (ctx, config, _s) = home();
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
        let (ctx, config, _s) = home();
        assert!(!custom_oauth(&ctx));
        write(
            &config.join("settings.json"),
            json!({"env": {"CLAUDE_CODE_CUSTOM_OAUTH_URL": "https://oauth.example"}}),
        );
        assert!(custom_oauth(&ctx));
    }
}

#[cfg(test)]
mod managed_tests {
    use super::*;
    use crate::context::Environment;
    use crate::host::memory::MemoryHost;
    use serde_json::json;
    use std::sync::Arc;

    fn on(os: Os, memory: &Arc<MemoryHost>) -> (Context, tempfile::TempDir) {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let home = scratch.path().to_string_lossy().into_owned();
        let config = scratch
            .path()
            .join(".claude")
            .to_string_lossy()
            .into_owned();
        let env = Environment::on(
            os,
            [
                ("HOME", home.as_str()),
                ("USERPROFILE", home.as_str()),
                ("CLAUDE_CONFIG_DIR", config.as_str()),
            ],
        );
        let ctx = Context::for_command_line(&env).with_memory_stores(Arc::clone(memory));
        (ctx, scratch)
    }

    fn said(found: &[Override]) -> Vec<(String, String)> {
        found
            .iter()
            .map(|o| (o.layer.clone(), o.key.clone()))
            .collect()
    }

    #[test]
    fn managed_settings_are_read_where_claude_code_keeps_them_on_each_system() {
        for (os, dir) in [
            (Os::MacOs, "/Library/Application Support/ClaudeCode"),
            (Os::Linux, "/etc/claude-code"),
            (Os::Windows, r"C:\Program Files\ClaudeCode"),
        ] {
            let memory = MemoryHost::new();
            let dir = PathBuf::from(dir);
            let (file, drop_ins) = (
                dir.join("managed-settings.json"),
                dir.join("managed-settings.d"),
            );
            memory.administers(
                &file,
                &json!({"env": {"ANTHROPIC_API_KEY": "sk-ant-x"}}).to_string(),
            );
            memory.administers(
                drop_ins.join("20-helper.json"),
                &json!({"apiKeyHelper": "/opt/key"}).to_string(),
            );
            memory.administers(
                drop_ins.join("10-oauth.json"),
                &json!({"env": {"CLAUDE_CODE_CUSTOM_OAUTH_URL": "https://oauth.example"}})
                    .to_string(),
            );
            memory.administers(
                drop_ins.join("30-notes.txt"),
                &json!({"apiKeyHelper": "/opt/other"}).to_string(),
            );
            let (ctx, scratch) = on(os, &memory);
            let expected = [
                (
                    file.display().to_string(),
                    "env.ANTHROPIC_API_KEY".to_string(),
                ),
                (
                    drop_ins.join("20-helper.json").display().to_string(),
                    "apiKeyHelper".to_string(),
                ),
            ];
            assert_eq!(said(&overrides(&ctx)), expected, "{os:?}");
            assert!(custom_oauth(&ctx), "{os:?}");
            let run = scratch.path().join("warm").join("config");
            assert_eq!(said(&overrides_of_a_run_in(&ctx, &run)), expected, "{os:?}");
        }
    }

    #[test]
    fn windows_policies_are_read_as_claude_code_reads_them() {
        let memory = MemoryHost::new();
        let user = format!(r"HKCU\{POLICIES}\{POLICY}");
        let machine = format!(r"HKLM\{POLICIES}\{POLICY}");
        memory.sets_policy(
            Hive::User,
            POLICIES,
            POLICY,
            &json!({"env": {"CLAUDE_CODE_USE_BEDROCK": "1"}}).to_string(),
        );
        let (ctx, _scratch) = on(Os::Windows, &memory);
        assert_eq!(
            said(&overrides(&ctx)),
            [(user.clone(), "env.CLAUDE_CODE_USE_BEDROCK".to_string())]
        );

        memory.sets_policy(
            Hive::Machine,
            POLICIES,
            POLICY,
            &json!({"managedSourcesBehavior": "merge"}).to_string(),
        );
        assert_eq!(
            said(&overrides(&ctx)),
            [(user, "env.CLAUDE_CODE_USE_BEDROCK".to_string())],
            "one that says only how the sources compose keeps nothing out"
        );

        memory.sets_policy(
            Hive::Machine,
            POLICIES,
            POLICY,
            &json!({"apiKeyHelper": "C:\\key.cmd"}).to_string(),
        );
        assert_eq!(
            said(&overrides(&ctx)),
            [(machine, "apiKeyHelper".to_string())]
        );

        let (mac, _scratch) = on(Os::MacOs, &memory);
        assert_eq!(overrides(&mac), Vec::new(), "macOS has no registry");
    }

    // Claude Code treats a failed HKLM query as absent and still reads HKCU, so Pitboard must
    // not miss an HKCU override when the machine policy cannot be read.
    #[test]
    fn an_unreadable_machine_policy_still_lets_hkcu_be_read() {
        let memory = MemoryHost::new();
        let user = format!(r"HKCU\{POLICIES}\{POLICY}");
        memory.policy_unreadable(Hive::Machine, POLICIES, POLICY, "access denied");
        memory.sets_policy(
            Hive::User,
            POLICIES,
            POLICY,
            &json!({"apiKeyHelper": "C:\\key.cmd"}).to_string(),
        );
        let (ctx, _scratch) = on(Os::Windows, &memory);
        assert_eq!(said(&overrides(&ctx)), [(user, "apiKeyHelper".to_string())]);
    }

    #[test]
    fn a_token_in_claude_codes_remote_files_comes_before_the_stored_login() {
        for os in [Os::MacOs, Os::Linux] {
            let memory = MemoryHost::new();
            memory.administers("/home/claude/.claude/remote/.oauth_token", "a token\n");
            memory.administers("/home/claude/.claude/remote/.api_key", " \n");
            let (ctx, _scratch) = on(os, &memory);
            assert_eq!(
                said(&overrides(&ctx)),
                [(
                    "/home/claude/.claude/remote/.oauth_token".to_string(),
                    "the OAuth token".to_string()
                )],
                "{os:?}"
            );
            assert_eq!(
                overrides(&ctx)[0].to_string(),
                "the OAuth token in /home/claude/.claude/remote/.oauth_token"
            );
        }
    }

    #[test]
    fn a_remote_token_file_over_claude_codes_limit_holds_no_token() {
        let key = "/home/claude/.claude/remote/.api_key";
        let memory = MemoryHost::new();
        memory.administers(key, &"k".repeat(65_536));
        let (ctx, _scratch) = on(Os::Linux, &memory);
        assert_eq!(
            said(&overrides(&ctx)),
            [(key.to_string(), "the API key".to_string())]
        );

        memory.administers(key, &"k".repeat(65_537));
        assert_eq!(overrides(&ctx), Vec::new());
    }

    // A warm-up's own token comes before `.oauth_token`, and `.api_key` before both.
    #[test]
    fn a_warm_ups_run_takes_the_api_key_in_claude_codes_remote_file() {
        for (os, folder) in [
            (Os::MacOs, "/home/claude/.claude/remote/"),
            (Os::Linux, "/home/claude/.claude/remote/"),
            (Os::Windows, r"C:\home\claude\.claude\remote\"),
        ] {
            let memory = MemoryHost::new();
            memory.administers(format!("{folder}.oauth_token"), "a token");
            let (ctx, scratch) = on(os, &memory);
            let run = scratch.path().join("warm").join("config");
            assert_eq!(overrides_of_a_run_in(&ctx, &run), Vec::new(), "{os:?}");

            memory.administers(format!("{folder}.api_key"), "sk-ant-x");
            assert_eq!(
                said(&overrides_of_a_run_in(&ctx, &run)),
                [(format!("{folder}.api_key"), "the API key".to_string())],
                "{os:?}"
            );
        }
    }

    #[test]
    fn on_windows_a_remote_token_file_is_at_a_drives_root() {
        let memory = MemoryHost::new();
        memory.administers(r"C:\home\claude\.claude\remote\.api_key", "sk-ant-x");
        memory.administers("/home/claude/.claude/remote/.oauth_token", "a token");
        let (ctx, _scratch) = on(Os::Windows, &memory);
        assert_eq!(
            said(&overrides(&ctx)),
            [(
                r"C:\home\claude\.claude\remote\.api_key".to_string(),
                "the API key".to_string()
            )]
        );

        memory.has_fixed_drives(&['C', 'D']);
        memory.administers(r"D:\home\claude\.claude\remote\.oauth_token", "a token");
        assert_eq!(
            said(&overrides(&ctx)),
            [
                (
                    r"C:\home\claude\.claude\remote\.api_key".to_string(),
                    "the API key".to_string()
                ),
                (
                    r"D:\home\claude\.claude\remote\.oauth_token".to_string(),
                    "the OAuth token".to_string()
                ),
            ]
        );
    }

    // Read in 2.1.309 (auth_precedence): managed settings that require the Cloud gateway's
    // sign-in send every session there, a warm-up's run included. The person's own only aim
    // `/login` at it.
    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "a test sets up its scratch files as a person or a tool would"
    )]
    fn managed_settings_that_require_the_gateway_sign_in_override_the_login() {
        for (os, dir) in [
            (Os::MacOs, "/Library/Application Support/ClaudeCode"),
            (Os::Linux, "/etc/claude-code"),
            (Os::Windows, r"C:\Program Files\ClaudeCode"),
        ] {
            let file = PathBuf::from(dir).join("managed-settings.json");
            let memory = MemoryHost::new();
            let (ctx, scratch) = on(os, &memory);
            let own = scratch.path().join(".claude").join("settings.json");
            std::fs::create_dir_all(own.parent().expect("its folder")).expect("a config dir");
            std::fs::write(
                &own,
                json!({"forceLoginMethod": "gateway", "forceLoginGatewayUrl": "https://gw.example"})
                    .to_string(),
            )
            .expect("written");
            memory.administers(&file, &json!({"forceLoginMethod": "claudeai"}).to_string());
            assert_eq!(overrides(&ctx), Vec::new(), "{os:?}");

            let run = scratch.path().join("warm").join("config");
            for (set, key) in [
                (json!({"forceLoginMethod": "gateway"}), "forceLoginMethod"),
                (
                    json!({"forceLoginGatewayUrl": "https://gw.example"}),
                    "forceLoginGatewayUrl",
                ),
            ] {
                memory.administers(&file, &set.to_string());
                let expected = [(file.display().to_string(), key.to_string())];
                assert_eq!(said(&overrides(&ctx)), expected, "{os:?}");
                assert_eq!(said(&overrides_of_a_run_in(&ctx, &run)), expected, "{os:?}");
            }
        }
    }

    // Read in 2.1.309 (organisation_pin): one organisation or a list of them, from the managed
    // sources a session takes and not from the person's own settings.
    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "a test sets up its scratch files as a person or a tool would"
    )]
    fn a_managed_organisation_pin_is_read_as_one_or_a_list() {
        let memory = MemoryHost::new();
        let (ctx, scratch) = on(Os::Linux, &memory);
        let own = scratch.path().join(".claude").join("settings.json");
        std::fs::create_dir_all(own.parent().expect("its folder")).expect("a config dir");
        std::fs::write(&own, json!({"forceLoginOrgUUID": "org-own"}).to_string()).expect("written");
        assert_eq!(organisation_pins(&ctx), Vec::new());

        let dir = PathBuf::from("/etc/claude-code");
        let (file, drop_in) = (
            dir.join("managed-settings.json"),
            dir.join("managed-settings.d").join("10-org.json"),
        );
        memory.administers(&file, &json!({"forceLoginOrgUUID": "org-a"}).to_string());
        memory.administers(
            &drop_in,
            &json!({"forceLoginOrgUUID": ["org-b", "org-c"]}).to_string(),
        );
        assert_eq!(
            organisation_pins(&ctx),
            [
                (file.display().to_string(), vec!["org-a".to_string()]),
                (
                    drop_in.display().to_string(),
                    vec!["org-b".to_string(), "org-c".to_string()]
                ),
            ]
        );
    }

    #[test]
    fn a_managed_file_keeps_the_persons_policy_out() {
        let memory = MemoryHost::new();
        memory.sets_policy(
            Hive::User,
            POLICIES,
            POLICY,
            &json!({"apiKeyHelper": "C:\\key.cmd"}).to_string(),
        );
        memory.administers(
            PathBuf::from(r"C:\Program Files\ClaudeCode").join("managed-settings.json"),
            &json!({"permissions": {"deny": ["WebFetch"]}}).to_string(),
        );
        let (ctx, _scratch) = on(Os::Windows, &memory);
        assert_eq!(overrides(&ctx), Vec::new());
    }
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod profile_tests {
    use super::*;
    use crate::context::Environment;
    use serde_json::json;

    fn on(os: Os, scratch: &Path, set: &[(&str, &str)]) -> Context {
        let home = scratch.to_string_lossy().into_owned();
        let config = scratch.join(".claude").to_string_lossy().into_owned();
        let mut pairs = vec![
            ("HOME", home.as_str()),
            ("USERPROFILE", home.as_str()),
            ("CLAUDE_CONFIG_DIR", config.as_str()),
        ];
        pairs.extend_from_slice(set);
        Context::for_command_line(&Environment::on(os, pairs))
    }

    fn profile(folder: &Path, name: &str, mode: &str) -> PathBuf {
        let path = folder.join("configs").join(format!("{name}.json"));
        std::fs::create_dir_all(path.parent().expect("its folder")).expect("a folder");
        std::fs::write(&path, json!({"authentication": {"type": mode}}).to_string())
            .expect("written");
        path
    }

    fn signed_in(folder: &Path, name: &str) {
        let path = folder.join("credentials").join(format!("{name}.json"));
        std::fs::create_dir_all(path.parent().expect("its folder")).expect("a folder");
        std::fs::write(path, json!({"refresh_token": "not-a-token"}).to_string()).expect("written");
    }

    fn said(ctx: &Context) -> Vec<String> {
        overrides(ctx).iter().map(ToString::to_string).collect()
    }

    // Read in 2.1.309 (anthropic_profiles): the active profile comes before the stored login
    // only where it federates; a claude.ai login comes before one signed in with OAuth.
    #[test]
    fn the_active_profile_overrides_the_login_where_it_federates() {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let folder = scratch.path().join(".config").join("anthropic");
        let ctx = on(Os::Linux, scratch.path(), &[]);
        assert_eq!(said(&ctx), Vec::<String>::new());

        let default = profile(&folder, "default", "oidc_federation");
        assert_eq!(
            said(&ctx),
            [format!(
                "the `oidc_federation` profile `default` in {}",
                default.display()
            )]
        );

        std::fs::write(folder.join("active_config"), "work\n").expect("written");
        profile(&folder, "work", "user_oauth");
        signed_in(&folder, "work");
        assert_eq!(said(&ctx), Vec::<String>::new());
    }

    // `ANTHROPIC_PROFILE` takes a profile of either mode, but only one there: a user_oauth
    // profile with no login in its credentials file is none, and then nothing else is looked at.
    #[test]
    fn a_named_profile_overrides_the_login_where_it_is_there() {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let folder = scratch.path().join(".config").join("anthropic");
        let named = [
            ("ANTHROPIC_PROFILE", " work "),
            ("ANTHROPIC_FEDERATION_RULE_ID", "rule"),
            ("ANTHROPIC_ORGANIZATION_ID", "org"),
        ];
        let ctx = on(Os::MacOs, scratch.path(), &named);
        profile(&folder, "default", "oidc_federation");
        profile(&folder, "work", "user_oauth");
        assert_eq!(said(&ctx), Vec::<String>::new());

        signed_in(&folder, "work");
        assert_eq!(said(&ctx), ["ANTHROPIC_PROFILE in the environment"]);
    }

    // The file holds a login, so it is looked at for being there and not empty, never read.
    #[cfg(unix)]
    #[test]
    fn a_profiles_credentials_file_is_never_read() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = tempfile::tempdir().expect("a scratch home");
        let folder = scratch.path().join(".config").join("anthropic");
        let ctx = on(Os::Linux, scratch.path(), &[("ANTHROPIC_PROFILE", "work")]);
        profile(&folder, "work", "user_oauth");
        signed_in(&folder, "work");
        let credentials = folder.join("credentials").join("work.json");
        std::fs::set_permissions(&credentials, std::fs::Permissions::from_mode(0o000))
            .expect("unreadable");
        assert_eq!(said(&ctx), ["ANTHROPIC_PROFILE in the environment"]);

        std::fs::set_permissions(&credentials, std::fs::Permissions::from_mode(0o600))
            .expect("readable");
        std::fs::write(&credentials, "").expect("emptied");
        assert_eq!(said(&ctx), Vec::<String>::new());
    }

    // Both variables, with no `ANTHROPIC_PROFILE`, are a federation of their own.
    #[test]
    fn a_federation_the_environment_sets_overrides_the_login() {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let pair = [
            ("ANTHROPIC_FEDERATION_RULE_ID", "rule"),
            ("ANTHROPIC_ORGANIZATION_ID", "org"),
        ];
        assert_eq!(
            said(&on(Os::Linux, scratch.path(), &pair)),
            ["ANTHROPIC_FEDERATION_RULE_ID and ANTHROPIC_ORGANIZATION_ID in the environment"]
        );
        assert_eq!(
            said(&on(Os::Linux, scratch.path(), &pair[..1])),
            Vec::<String>::new()
        );
        assert_eq!(
            said(&on(
                Os::Linux,
                scratch.path(),
                &[pair[0], ("ANTHROPIC_ORGANIZATION_ID", " ")]
            )),
            Vec::<String>::new()
        );
    }

    #[test]
    fn profiles_are_where_claude_code_looks_for_them_on_each_system() {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let elsewhere = scratch.path().join("elsewhere");
        let named = elsewhere.to_string_lossy().into_owned();
        let roaming = scratch.path().join("Roaming");
        let app_data = roaming.to_string_lossy().into_owned();
        for (ctx, folder) in [
            (
                on(Os::Linux, scratch.path(), &[("XDG_CONFIG_HOME", &named)]),
                elsewhere.join("anthropic"),
            ),
            (
                on(
                    Os::MacOs,
                    scratch.path(),
                    &[
                        ("ANTHROPIC_CONFIG_DIR", &named),
                        ("XDG_CONFIG_HOME", "/xdg"),
                    ],
                ),
                elsewhere.clone(),
            ),
            (
                on(Os::Windows, scratch.path(), &[("APPDATA", &app_data)]),
                roaming.join("Anthropic"),
            ),
            (
                on(Os::Windows, scratch.path(), &[]),
                scratch
                    .path()
                    .join("AppData")
                    .join("Roaming")
                    .join("Anthropic"),
            ),
        ] {
            let path = profile(&folder, "default", "oidc_federation");
            assert_eq!(
                said(&ctx),
                [format!(
                    "the `oidc_federation` profile `default` in {}",
                    path.display()
                )],
                "{folder:?}"
            );
            std::fs::remove_file(path).expect("removed");
        }
    }
}
