//! Everything pitboard takes from its environment, read in one place. The CLI builds a
//! `Context` from the process environment once. A program linking the library builds one
//! itself: an app started from Finder does not see a shell's environment.

use crate::api::{Anthropic, Api};
use crate::provider::codex::api::{Network as OpenAiNetwork, OpenAi};
use crate::store::Host;
use crate::time::{Clock, SystemClock};
use std::path::PathBuf;
use std::sync::Arc;

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
    /// Environment variables this process was started with that make Claude Code use
    /// something other than the login pitboard moves. Only half the answer: the rest is in
    /// files, which [`crate::settings::overrides`] reads and an app can see too.
    pub(crate) overriding_auth: Vec<String>,
    /// Whether a login too large for `security -i` may be written the way Claude Code
    /// writes it: as a command argument, where `ps` can see it for the length of the call.
    /// On by default, because there is no third way and Claude Code writes the same
    /// document that way itself on every token refresh.
    pub(crate) argv_fallback: bool,
    /// Which front end asked, for the audit log. A change made from the menu bar and one
    /// typed at a prompt read the same otherwise.
    pub(crate) caller: String,
    /// The `claude` that runs a sign-in; a bare name is looked up on the search path.
    pub(crate) claude_program: PathBuf,
    /// Where Anthropic's endpoints are reached instead, for tests; `api` honours loopback only.
    pub(crate) api_base: Option<String>,
    /// `CLAUDE_CODE_HOVER_REST`, which switches on Claude Code's successor credential backend.
    pub(crate) hover_rest: bool,
    /// `CODEX_HOME`, which moves everything Codex keeps, including its keyring account.
    pub(crate) codex_home: Option<String>,
    /// The `codex` that runs a sign-in; a bare name is looked up on the search path.
    pub(crate) codex_program: PathBuf,
    /// The pitboard the daily renewal schedule runs. `None` is this program, which is right
    /// for the command line and wrong for an app: the schedule runs `pitboard renew`, so an
    /// app names the command line it comes with.
    pub(crate) schedule_program: Option<PathBuf>,
    /// Where a tool's program is looked for, in `PATH`'s form, and what its sign-in is given
    /// as `PATH`, behind the program's own directory where that is not on it. `None` is this
    /// process's own `PATH`: an app opened from Finder has almost nothing on it, so it
    /// passes the one the person's login shell would have.
    pub(crate) search_path: Option<std::ffi::OsString>,
    /// Where the time comes from. The machine's clock in every real context; a test puts
    /// its own here to reach the judgements that only happen at a particular moment.
    pub(crate) clock: Arc<dyn Clock>,
    /// The machine's credential stores. This build's host in every real context.
    pub(crate) host: Arc<dyn Host>,
    /// Who answers for Anthropic. The network in every real context.
    pub(crate) api: Arc<dyn Api>,
    /// Who answers for OpenAI. The network in every real context.
    pub(crate) openai: Arc<dyn OpenAi>,
}

impl Context {
    /// Epoch seconds, from this context's clock.
    pub(crate) fn now(&self) -> i64 {
        self.clock.now()
    }

    /// Epoch milliseconds, from this context's clock.
    pub(crate) fn now_millis(&self) -> i64 {
        self.clock.now_millis()
    }

    /// The person's home directory, which is where a platform's own scheduler lives.
    pub(crate) fn home(&self) -> &std::path::Path {
        &self.home
    }

    /// The credential stores this context reaches.
    pub(crate) fn host(&self) -> &dyn Host {
        self.host.as_ref()
    }

    /// Who this context asks about a login.
    pub(crate) fn openai(&self) -> &dyn OpenAi {
        self.openai.as_ref()
    }

    pub(crate) fn api(&self) -> &dyn Api {
        self.api.as_ref()
    }

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
            codex_home: None,
            codex_program: PathBuf::from("codex"),
            schedule_program: None,
            search_path: None,
            clock: Arc::new(SystemClock),
            host: crate::store::host(),
            api: Arc::new(Anthropic),
            openai: Arc::new(OpenAiNetwork),
        }
    }

    /// Where Codex keeps its login. Empty means unset, as Codex reads it.
    pub fn with_codex_home(mut self, dir: String) -> Context {
        self.codex_home = Some(dir).filter(|d| !d.is_empty());
        self
    }

    pub(crate) fn codex_home(&self) -> Option<&str> {
        self.codex_home.as_deref()
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

    /// The same for `codex`.
    pub fn with_codex_program(mut self, program: PathBuf) -> Context {
        self.codex_program = program;
        self
    }

    /// The `codex` pitboard would run to sign someone in.
    pub fn codex_program(&self) -> &std::path::Path {
        &self.codex_program
    }

    /// An app is not a command line, so it names the one it comes with for the schedule to
    /// run.
    pub fn with_schedule_program(mut self, program: PathBuf) -> Context {
        self.schedule_program = Some(program);
        self
    }

    /// The pitboard the daily renewal schedule is written to run, where one was named.
    pub fn schedule_program(&self) -> Option<&std::path::Path> {
        self.schedule_program.as_deref()
    }

    /// Look for a tool's program on `path`, in `PATH`'s form, rather than on this process's
    /// own `PATH`. An app opened from Finder has only the system's directories there, so a
    /// tool installed through a version manager or an npm prefix is found only on the `PATH`
    /// the person's shell has, and a script it runs finds its interpreter only there.
    pub fn with_search_path(mut self, path: String) -> Context {
        self.search_path = Some(path.into());
        self
    }

    /// Where a tool's program is looked for: the path given, or this process's `PATH`.
    pub(crate) fn search_path(&self) -> std::ffi::OsString {
        self.search_path
            .clone()
            .or_else(|| std::env::var_os("PATH"))
            .unwrap_or_default()
    }

    /// The program named for this tool, found or not.
    pub fn program_for(&self, tool: crate::provider::ProviderId) -> &std::path::Path {
        match tool {
            crate::provider::ProviderId::Claude => &self.claude_program,
            crate::provider::ProviderId::Codex => &self.codex_program,
        }
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
            overriding_auth: crate::settings::OVERRIDING_ENV
                .iter()
                .filter(|name| var(name).is_some_and(|v| !v.is_empty()))
                .map(|name| (*name).to_string())
                .collect(),
            caller: "cli".into(),
            claude_program: PathBuf::from("claude"),
            api_base: var("PITBOARD_API_BASE"),
            hover_rest: var("CLAUDE_CODE_HOVER_REST").is_some_and(|v| v == "1" || v == "true"),
            codex_home: var("CODEX_HOME").filter(|v| !v.is_empty()),
            codex_program: PathBuf::from("codex"),
            schedule_program: None,
            search_path: None,
            clock: Arc::new(SystemClock),
            host: crate::store::host(),
            api: Arc::new(Anthropic),
            openai: Arc::new(OpenAiNetwork),
        }
    }

    /// Answer for every service from a script, where a test can produce a 429 or a
    /// refusal. One script for all of them, so a test that forgets to script a tool's
    /// service gets a refusal rather than a request to the real one.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_scripted_api(mut self, api: Arc<crate::api::scripted::ScriptedApi>) -> Context {
        self.api = api.clone();
        self.openai = api;
        self
    }

    /// Put the credential stores in memory, where a test can make them fail. Only the
    /// tests do this, which is why the trait behind it is not public.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_memory_stores(mut self, memory: Arc<crate::store::memory::MemoryHost>) -> Context {
        self.host = memory;
        self
    }

    /// Read the time from somewhere else. Only the tests do this, which is why it is not
    /// part of the builder a front end uses.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Context {
        self.clock = clock;
        self
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

    #[test]
    fn only_a_front_end_that_names_one_changes_what_the_schedule_runs() {
        assert_eq!(Context::from_env().schedule_program(), None);
        let ctx = Context::new(PathBuf::from("/Users/x"));
        assert_eq!(ctx.schedule_program(), None);
        let bundled = PathBuf::from("/Applications/Pitboard.app/Contents/Helpers/pitboard");
        assert_eq!(
            ctx.with_schedule_program(bundled.clone())
                .schedule_program(),
            Some(bundled.as_path())
        );
    }
}
