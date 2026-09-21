//! Everything pitboard takes from its environment, read in one place. The CLI builds a
//! `Context` from the process environment once. A program linking the library builds one
//! itself: an app started from Finder does not see a shell's environment.

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Context {
    pub(crate) home: PathBuf,
    pub(crate) pitboard_home: PathBuf,
    /// `CLAUDE_CONFIG_DIR`, which Claude Code reads with `||`: empty means unset.
    pub(crate) claude_config_dir: Option<String>,
    /// `CLAUDE_SECURESTORAGE_CONFIG_DIR`, which Claude Code reads with `!== undefined`:
    /// empty is set, and pins the default credential slot.
    pub(crate) secure_storage_dir: Option<String>,
    /// `$USER`, which names Claude Code's keychain account once screened by `slot`.
    pub(crate) user: Option<String>,
    /// The `claude` that runs a sign-in; a bare name is looked up on `PATH`.
    pub(crate) claude_program: PathBuf,
    /// Where Anthropic's endpoints are reached instead, for tests; `api` honours loopback only.
    pub(crate) api_base: Option<String>,
    /// `CLAUDE_CODE_HOVER_REST`, which switches on Claude Code's successor credential backend.
    pub(crate) hover_rest: bool,
}

impl Context {
    pub fn from_env() -> Context {
        let var = |name: &str| std::env::var(name).ok();
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        Context {
            pitboard_home: std::env::var_os("PITBOARD_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".pitboard")),
            home,
            claude_config_dir: var("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()),
            secure_storage_dir: var("CLAUDE_SECURESTORAGE_CONFIG_DIR"),
            user: var("USER"),
            claude_program: PathBuf::from("claude"),
            api_base: var("PITBOARD_API_BASE"),
            hover_rest: var("CLAUDE_CODE_HOVER_REST").is_some_and(|v| v == "1" || v == "true"),
        }
    }
}
