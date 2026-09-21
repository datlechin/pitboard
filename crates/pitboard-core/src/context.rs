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
    /// Claude Code's defaults for a person whose home is `home`: `~/.pitboard`, `~/.claude`,
    /// the default credential slot, `claude` looked up on `PATH`. An app starts here and sets
    /// only what differs.
    pub fn new(home: PathBuf) -> Context {
        Context {
            pitboard_home: home.join(".pitboard"),
            home,
            claude_config_dir: None,
            secure_storage_dir: None,
            user: None,
            claude_program: PathBuf::from("claude"),
            api_base: None,
            hover_rest: false,
        }
    }

    pub fn with_pitboard_home(mut self, dir: PathBuf) -> Context {
        self.pitboard_home = dir;
        self
    }

    /// Empty means unset, as Claude Code reads `CLAUDE_CONFIG_DIR`.
    pub fn with_claude_config_dir(mut self, dir: String) -> Context {
        self.claude_config_dir = Some(dir).filter(|d| !d.is_empty());
        self
    }

    /// Empty is set, and pins the default slot, as Claude Code reads
    /// `CLAUDE_SECURESTORAGE_CONFIG_DIR`.
    pub fn with_secure_storage_dir(mut self, dir: String) -> Context {
        self.secure_storage_dir = Some(dir);
        self
    }

    /// The login name whose keychain account Claude Code stores under.
    pub fn with_user(mut self, user: String) -> Context {
        self.user = Some(user);
        self
    }

    /// An app started from Finder does not see the shell's `PATH`, so it names `claude` itself.
    pub fn with_claude_program(mut self, program: PathBuf) -> Context {
        self.claude_program = program;
        self
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_explicit_context_reads_claude_codes_settings_the_way_the_environment_does() {
        let ctx = Context::new(PathBuf::from("/home/x"))
            .with_claude_config_dir(String::new())
            .with_secure_storage_dir(String::new());
        assert_eq!(ctx.pitboard_home, PathBuf::from("/home/x/.pitboard"));
        assert_eq!(ctx.claude_config_dir, None, "empty means unset");
        assert_eq!(
            ctx.secure_storage_dir.as_deref(),
            Some(""),
            "empty is set, and pins the default slot"
        );
        assert_eq!(ctx.claude_program, PathBuf::from("claude"));
    }
}
