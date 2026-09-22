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
    /// `CLAUDE_CODE_CUSTOM_OAUTH_URL`. Set, it renames both the keychain item and the config
    /// file Claude Code uses, so pitboard would be reading and writing the wrong ones.
    pub(crate) custom_oauth: bool,
    /// Environment variables that make Claude Code use something other than the login
    /// pitboard moves, so a switch would change nothing it can see.
    pub(crate) overriding_auth: Vec<String>,
    /// Whether a login too large for `security -i` may be written the way Claude Code
    /// writes it: as a command argument, where `ps` can see it for the length of the call.
    /// On by default, because there is no third way and Claude Code writes the same
    /// document that way itself on every token refresh.
    pub(crate) argv_fallback: bool,
    /// Which front end asked, for the audit log. A change made from the menu bar and one
    /// typed at a prompt read the same otherwise.
    pub(crate) caller: String,
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
            custom_oauth: false,
            argv_fallback: true,
            overriding_auth: Vec::new(),
            caller: "unknown".into(),
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
    /// Allows the argument-line write for a login too large for the stdin one.
    pub fn with_argv_fallback(mut self, allowed: bool) -> Context {
        self.argv_fallback = allowed;
        self
    }

    /// Whether a custom OAuth endpoint is configured, which moves Claude Code's login.
    pub fn custom_oauth(&self) -> bool {
        self.custom_oauth
    }

    /// The `claude` pitboard would run to sign someone in.
    pub fn claude_program(&self) -> &std::path::Path {
        &self.claude_program
    }

    /// Whether the argument-line write is allowed for an oversized login.
    pub fn argv_fallback(&self) -> bool {
        self.argv_fallback
    }

    /// Environment variables that authenticate Claude Code some other way, if any.
    pub fn overriding_auth(&self) -> &[String] {
        &self.overriding_auth
    }

    /// Names the front end in the audit log.
    pub fn with_caller(mut self, caller: String) -> Context {
        self.caller = caller;
        self
    }

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
            custom_oauth: var("CLAUDE_CODE_CUSTOM_OAUTH_URL").is_some_and(|v| !v.is_empty()),
            argv_fallback: !var("PITBOARD_NO_ARGV").is_some_and(|v| v == "1"),
            overriding_auth: OVERRIDING_AUTH
                .iter()
                .filter(|name| var(name).is_some_and(|v| !v.is_empty()))
                .map(|name| (*name).to_string())
                .collect(),
            caller: "cli".into(),
            claude_program: PathBuf::from("claude"),
            api_base: var("PITBOARD_API_BASE"),
            hover_rest: var("CLAUDE_CODE_HOVER_REST").is_some_and(|v| v == "1" || v == "true"),
        }
    }
}

/// Set, any of these makes Claude Code authenticate with something other than the login in
/// the credential store, so moving that login changes nothing a session would notice.
const OVERRIDING_AUTH: [&str; 3] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
];

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
