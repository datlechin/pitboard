//! Where Codex CLI keeps things.
//!
//! Read from codex-cli 0.154.0: the binary on this machine, and the matching public source
//! at tag `rust-v0.154.0`.

use crate::context::Context;
use std::path::PathBuf;

/// The file Codex keeps its login in, inside [`home`].
pub(crate) const AUTH_FILE: &str = "auth.json";

/// `CODEX_HOME`, or `~/.codex`.
///
/// Codex requires the directory to exist already and canonicalises it, so a scratch home
/// for a private sign-in has to be created before `codex login` is run.
pub(crate) fn home(ctx: &Context) -> PathBuf {
    match ctx.codex_home() {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => ctx.home().join(".codex"),
    }
}

pub(crate) fn auth_file(ctx: &Context) -> PathBuf {
    home(ctx).join(AUTH_FILE)
}

/// Which store this machine's Codex is configured to keep its login in.
///
/// The shipped default is `file`, and it is a packaged default rather than a line in
/// anybody's `config.toml`, so an absent setting means `file` and not "unknown".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Backend {
    File,
    Keyring,
    /// `auto`: the keyring, falling back to the file.
    Either,
    /// In memory for the life of one process. Nothing pitboard can park.
    Ephemeral,
    /// `[features] secret_auth_storage` with a keyring store: an encrypted file whose key
    /// is in the keychain.
    Secrets,
}

/// What `$CODEX_HOME/config.toml` says about where the login is kept.
///
/// Read from that one file, which is where a person sets it. A store pinned by
/// `/etc/codex/requirements.toml` or a managed profile is not read, and is dated in the
/// register as a known gap.
pub(crate) fn backend(ctx: &Context) -> Backend {
    std::fs::read_to_string(home(ctx).join("config.toml"))
        .map_or(Backend::File, |config| backend_in(&config))
}

/// The store a `config.toml` names.
///
/// A whole TOML parser for two keys would be a dependency for two lines. What is read is
/// the top-level `cli_auth_credentials_store`, whose value is one of four bare words, and
/// `secret_auth_storage` inside `[features]`. A line inside any other table is not the
/// setting, whatever it is called, and a trailing comment is not part of a value.
fn backend_in(config: &str) -> Backend {
    let mut table = String::new();
    let mut store = Backend::File;
    let mut secrets = false;
    for line in config.lines() {
        let line = line.trim();
        if let Some(header) = line.strip_prefix('[') {
            table = header
                .split(']')
                .next()
                .unwrap_or_default()
                .trim()
                .to_string();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), bare(value));
        match (table.as_str(), key) {
            ("", "cli_auth_credentials_store") => {
                store = match value {
                    "keyring" => Backend::Keyring,
                    "auto" => Backend::Either,
                    "ephemeral" => Backend::Ephemeral,
                    _ => Backend::File,
                };
            }
            ("features", "secret_auth_storage") => secrets = value == "true",
            _ => {}
        }
    }
    match store {
        Backend::Keyring | Backend::Either if secrets => Backend::Secrets,
        other => other,
    }
}

/// A TOML value with its quotes and any trailing comment taken off.
fn bare(value: &str) -> &str {
    let value = value.trim();
    for quote in ['"', '\''] {
        if let Some(rest) = value.strip_prefix(quote) {
            return rest.split(quote).next().unwrap_or_default();
        }
    }
    value.split('#').next().unwrap_or_default().trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(dir: &std::path::Path, config: &str) -> Context {
        std::fs::create_dir_all(dir).expect("a scratch codex home");
        std::fs::write(dir.join("config.toml"), config).expect("a config");
        Context::new(PathBuf::from("/nowhere")).with_codex_home(dir.to_string_lossy().into())
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pitboard-codex-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// The shipped default is a packaged one rather than a line anybody wrote, so an absent
    /// setting means the file and not "cannot tell".
    #[test]
    fn no_setting_means_the_file() {
        let dir = scratch("default");
        let ctx = at(&dir, "model = \"gpt-5\"\n");
        assert_eq!(backend(&ctx), Backend::File);
        assert_eq!(auth_file(&ctx), dir.join("auth.json"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn each_store_is_recognised() {
        let dir = scratch("stores");
        for (written, expected) in [
            ("cli_auth_credentials_store = \"keyring\"", Backend::Keyring),
            ("cli_auth_credentials_store=\"auto\"", Backend::Either),
            (
                "  cli_auth_credentials_store = 'ephemeral'",
                Backend::Ephemeral,
            ),
            ("cli_auth_credentials_store = \"file\"", Backend::File),
            ("cli_auth_credentials_store = \"nonsense\"", Backend::File),
        ] {
            let ctx = at(&dir, written);
            assert_eq!(backend(&ctx), expected, "{written}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A trailing comment is not part of the value. Read as part of it, a keyring store
    /// looked like the default file, and pitboard would read a file Codex had deleted.
    #[test]
    fn a_comment_after_the_value_is_not_the_value() {
        assert_eq!(
            backend_in("cli_auth_credentials_store = \"keyring\"  # on this mac\n"),
            Backend::Keyring
        );
        assert_eq!(
            backend_in("cli_auth_credentials_store = keyring # bare\n"),
            Backend::Keyring
        );
    }

    /// A key of the same name inside another table is not the setting.
    #[test]
    fn only_the_top_level_key_is_the_setting() {
        let config = "[profiles.work]\ncli_auth_credentials_store = \"keyring\"\n";
        assert_eq!(backend_in(config), Backend::File);
        let config =
            "cli_auth_credentials_store = \"keyring\"\n[features]\nsecret_auth_storage = true\n";
        assert_eq!(
            backend_in(config),
            Backend::Secrets,
            "an encrypted file whose key is in the keychain is a store of its own"
        );
        let config = "[features]\nsecret_auth_storage = true\n";
        assert_eq!(
            backend_in(config),
            Backend::File,
            "the feature only changes a keyring store"
        );
    }
}
