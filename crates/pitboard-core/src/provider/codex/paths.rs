//! Where Codex CLI keeps things.
//!
//! Read from codex-cli 0.154.0: the binary on this machine, and the matching public source
//! at tag `rust-v0.154.0`.

use crate::context::Context;
use std::path::PathBuf;

/// The file Codex keeps its login in, inside [`home`].
pub(crate) const AUTH_FILE: &str = "auth.json";

/// The keychain service its optional keyring backend uses.
pub(crate) const KEYCHAIN_SERVICE: &str = "Codex Auth";

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

/// The keychain account its keyring backend files items under: `cli|` and the first sixteen
/// hex characters of the SHA-256 of the canonical home.
///
/// Derived from the home, which is what makes `CODEX_HOME` isolate the keyring backend as
/// well as the file one. Gemini's equivalent does not, which is why Gemini gets a refusal
/// where Codex does not need one.
pub(crate) fn keychain_account(ctx: &Context) -> String {
    use sha2::{Digest, Sha256};
    let dir = home(ctx);
    let canonical = std::fs::canonicalize(&dir).unwrap_or(dir);
    let digest = hex::encode(Sha256::digest(canonical.to_string_lossy().as_bytes()));
    format!("cli|{}", &digest[..16])
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
}

pub(crate) fn backend(ctx: &Context) -> Backend {
    let Ok(config) = std::fs::read_to_string(home(ctx).join("config.toml")) else {
        return Backend::File;
    };
    // A whole TOML parser for one string would be a dependency for one line. The setting is
    // a top-level key whose value is one of four bare words, so the line is read directly
    // and anything unrecognised falls back to the shipped default.
    for line in config.lines() {
        let line = line.trim();
        let Some(value) = line.strip_prefix("cli_auth_credentials_store") else {
            continue;
        };
        let Some(value) = value.trim_start().strip_prefix('=') else {
            continue;
        };
        return match value.trim().trim_matches(['"', '\'']) {
            "keyring" => Backend::Keyring,
            "auto" => Backend::Either,
            "ephemeral" => Backend::Ephemeral,
            _ => Backend::File,
        };
    }
    Backend::File
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

    /// The account is derived from the home, which is what makes `CODEX_HOME` isolate the
    /// keyring backend too. If it stopped, a private sign-in would write over the live one.
    #[test]
    fn the_keychain_account_follows_the_home() {
        let one = scratch("acct-one");
        let two = scratch("acct-two");
        std::fs::create_dir_all(&one).unwrap();
        std::fs::create_dir_all(&two).unwrap();
        let account = |dir: &std::path::Path| {
            keychain_account(
                &Context::new(PathBuf::from("/nowhere"))
                    .with_codex_home(dir.to_string_lossy().into()),
            )
        };
        assert_ne!(account(&one), account(&two));
        assert_eq!(account(&one), account(&one), "and is stable");
        assert!(account(&one).starts_with("cli|"));
        assert_eq!(account(&one).len(), 4 + 16);
        let _ = std::fs::remove_dir_all(&one);
        let _ = std::fs::remove_dir_all(&two);
    }
}
