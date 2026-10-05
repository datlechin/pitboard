//! Everything Pitboard takes from its environment, read in one place. Every front end builds
//! its `Context` from the variables it was started with, by the same code: the command line
//! from its own, once. An app passes the ones it was started with, which are not a shell's,
//! so it also looks for each tool's program itself, in [`crate::app`].
//!
//! This module is where the process's environment is read. `clippy.toml` refuses
//! `std::env::var` and its kind anywhere else, unless an `#[allow]` there says why.
#![allow(
    clippy::disallowed_methods,
    reason = "the one module that reads this process's environment"
)]

use crate::api::{Anthropic, Api};
use crate::host::Host;
use crate::provider::ProviderId;
use crate::provider::codex::api::{Network as OpenAiNetwork, OpenAi};
use crate::provider::desktop::safe_storage::SafeStorage;
use crate::provider::desktop::web::{ClaudeAi, ClaudeWeb};
use crate::time::{Clock, SystemClock};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::sync::Arc;

/// Every variable Pitboard reads from the environment it was started with, besides
/// [`crate::settings::OVERRIDING_ENV`]: what [`Context::read`] consults, the shell an app
/// asks for its `PATH`, and the three read straight from the process: `PATH` where a context
/// was given no search path, and where Linux's host finds the path Pitboard was started by,
/// `NO_COLOR` by the command line's status line, and `XPC_SERVICE_NAME` by launchd's host.
/// Each of those reads carries an `#[allow]` saying why it is not made through
/// [`Environment`].
///
/// [`Environment`] refuses, in a build with debug assertions, to read a name missing here,
/// so every test that reads a context fails until a new variable is added. The tests take
/// nothing on this list from whoever runs them, except what they pass on by name.
const READ: &[&str] = &[
    "HOME",
    "USER",
    "PATH",
    "SHELL",
    "PITBOARD_HOME",
    "PITBOARD_NO_ARGV",
    "PITBOARD_API_BASE",
    "PITBOARD_CLAUDE",
    "PITBOARD_CODEX",
    "PITBOARD_CLAUDE_DESKTOP_APP",
    "PITBOARD_CLAUDE_DESKTOP_DIR",
    "PITBOARD_CLAUDE_WEB_BASE",
    "CLAUDE_CONFIG_DIR",
    "CLAUDE_SECURESTORAGE_CONFIG_DIR",
    "CLAUDE_CODE_CUSTOM_OAUTH_URL",
    "CLAUDE_CODE_HOVER_REST",
    "CODEX_HOME",
    "NO_COLOR",
    "XPC_SERVICE_NAME",
];

/// Every variable Pitboard reads from its environment, by name.
pub(crate) fn variables() -> impl Iterator<Item = &'static str> {
    READ.iter()
        .chain(crate::settings::OVERRIDING_ENV.iter())
        .copied()
}

/// The variables a process was started with, by name.
///
/// The command line has a shell's. An app the system started has launchd's, which name the
/// home and the login and little else, unless somebody set more with `launchctl setenv`;
/// whatever is there is read the way the command line reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Environment(HashMap<OsString, OsString>);

impl Environment {
    /// This process's own.
    pub fn of_this_process() -> Environment {
        std::env::vars_os().collect()
    }

    /// `name`'s value, whatever bytes it holds, as a path may hold any.
    pub(crate) fn path(&self, name: &str) -> Option<&OsStr> {
        debug_assert!(
            variables().any(|read| read == name),
            "{name} is read but not listed in context::READ, so the tests would take it \
             from whoever runs them"
        );
        self.0.get(OsStr::new(name)).map(OsString::as_os_str)
    }

    /// `name`'s value as text. One that is not UTF-8 reads as unset, as `std::env::var`
    /// reads it.
    pub(crate) fn text(&self, name: &str) -> Option<&str> {
        self.path(name).and_then(OsStr::to_str)
    }

    /// Whether `name` holds anything: empty reads as unset.
    pub(crate) fn set(&self, name: &str) -> bool {
        self.text(name).is_some_and(|v| !v.is_empty())
    }

    /// The person's home: `HOME`, or, where it is unset, the account's own, as the passwd
    /// database names it and Foundation finds it for an app. Set, even empty, it is what the
    /// person said.
    pub fn home(&self) -> PathBuf {
        self.path("HOME")
            .map(PathBuf::from)
            .or_else(crate::host::user::home)
            .unwrap_or_default()
    }

    /// Pitboard's own directory: `PITBOARD_HOME`, or `.pitboard` in [`Environment::home`].
    /// The path as the environment gives it, with any `.`, `..` or trailing `/` it holds.
    pub fn pitboard_home(&self) -> PathBuf {
        self.path("PITBOARD_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.home().join(".pitboard"))
    }

    /// Every variable, as a program started in this environment is given them.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&OsStr, &OsStr)> {
        self.0
            .iter()
            .map(|(name, value)| (name.as_os_str(), value.as_os_str()))
    }
}

impl<K: Into<OsString>, V: Into<OsString>> FromIterator<(K, V)> for Environment {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(pairs: I) -> Environment {
        Environment(
            pairs
                .into_iter()
                .map(|(name, value)| (name.into(), value.into()))
                .collect(),
        )
    }
}

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
    /// file Claude Code uses, so Pitboard would be reading and writing the wrong ones.
    pub(crate) custom_oauth: bool,
    /// Environment variables this process was started with that make Claude Code use
    /// something other than the login Pitboard moves. Only half the answer: the rest is in
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
    /// `PITBOARD_CLAUDE_DESKTOP_DIR`: Claude Desktop's data folder, where a test or an app
    /// says. `None` is the platform's own place for it.
    pub(crate) desktop_dir: Option<PathBuf>,
    /// The Claude app bundle, the system's own place for it unless a test or an app says.
    /// `None` where the system has no Claude app and nobody named one.
    pub(crate) desktop_app: Option<PathBuf>,
    /// The program inside [`Context::desktop_app`], worked out once so
    /// [`Context::program_for`] can lend it. `None` where the system runs no Claude app.
    pub(crate) desktop_app_binary: Option<PathBuf>,
    /// `PITBOARD_CLAUDE_WEB_BASE`: where claude.ai is reached instead, for tests; live
    /// usage honours loopback only.
    pub(crate) web_base: Option<String>,
    /// The Pitboard the daily renewal schedule runs. `None` is this program, which is right
    /// for the command line and wrong for an app: the schedule runs `pitboard renew`, so an
    /// app names the command line it comes with.
    pub(crate) schedule_program: Option<PathBuf>,
    /// Where a tool's program is looked for, in `PATH`'s form, and what its sign-in is given
    /// as `PATH`, behind the program's own directory where that is not on it. `None` is this
    /// process's own `PATH`: an app opened from Finder has almost nothing on it, so it
    /// passes the one the person's login shell would have.
    pub(crate) search_path: Option<std::ffi::OsString>,
    /// The scheduler's job this process runs as, where a test says. `None` is whatever the
    /// scheduler said when it started this process.
    pub(crate) scheduled_job: Option<String>,
    /// Where the time comes from. The machine's clock in every real context; a test puts
    /// its own here to reach the judgements that only happen at a particular moment.
    pub(crate) clock: Arc<dyn Clock>,
    /// The machine: its stores, its processes, its scheduler. This build's host in every
    /// real context.
    pub(crate) host: Arc<dyn Host>,
    /// Who answers for Anthropic. The network in every real context.
    pub(crate) api: Arc<dyn Api>,
    /// Who answers for OpenAI. The network in every real context.
    pub(crate) openai: Arc<dyn OpenAi>,
    /// Claude Desktop's key in the keychain. `/usr/bin/security` in every real context;
    /// a test never reads the real item.
    pub(crate) safe_storage: Arc<dyn SafeStorage>,
    /// Who answers for claude.ai. The network in every real context.
    pub(crate) web: Arc<dyn ClaudeWeb>,
}

/// The Claude app bundle `named`, or the system's own place for it, with the program inside
/// it. Empty names nothing, as the environment variable is read.
fn desktop_app(named: Option<PathBuf>) -> (Option<PathBuf>, Option<PathBuf>) {
    let app = named
        .filter(|named| !named.as_os_str().is_empty())
        .or_else(|| crate::host::OS.claude_desktop_app().map(PathBuf::from));
    let program = app
        .as_deref()
        .and_then(|app| crate::host::OS.claude_desktop_program(app));
    (app, program)
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

    /// The machine this context reaches.
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

    /// Where this context reads Claude Desktop's key.
    pub(crate) fn safe_storage(&self) -> &dyn SafeStorage {
        self.safe_storage.as_ref()
    }

    /// Who this context asks about Claude Desktop's usage.
    pub(crate) fn web(&self) -> &dyn ClaudeWeb {
        self.web.as_ref()
    }

    /// Claude Code's defaults for a person whose home is `home`: `~/.pitboard`, `~/.claude`,
    /// the default credential slot, `claude` looked up on `PATH`. An app starts here and sets
    /// only what differs.
    pub fn new(home: PathBuf) -> Context {
        let (desktop_app, desktop_app_binary) = desktop_app(None);
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
            desktop_dir: None,
            desktop_app,
            desktop_app_binary,
            web_base: None,
            schedule_program: None,
            search_path: None,
            scheduled_job: None,
            clock: Arc::new(SystemClock),
            host: crate::host::current(),
            api: Arc::new(Anthropic),
            openai: Arc::new(OpenAiNetwork),
            safe_storage: crate::host::safe_storage(),
            web: Arc::new(ClaudeAi),
        }
    }

    /// Where Claude Desktop keeps its data. Empty means unset, as the environment variable
    /// is read.
    pub fn with_desktop_dir(mut self, dir: String) -> Context {
        self.desktop_dir = Some(dir).filter(|d| !d.is_empty()).map(PathBuf::from);
        self
    }

    /// The Claude app bundle. Empty means unset, as the environment variable is read.
    pub fn with_desktop_app(mut self, path: String) -> Context {
        (self.desktop_app, self.desktop_app_binary) = desktop_app(Some(PathBuf::from(path)));
        self
    }

    /// Whether a test or an app put the Claude app bundle somewhere of its own.
    pub(crate) fn desktop_app_moved(&self) -> bool {
        self.desktop_app.as_deref()
            != crate::host::OS
                .claude_desktop_app()
                .map(std::path::Path::new)
    }

    /// Whether the Claude app is installed where this context says, as its bundle's own
    /// manifest shows.
    pub(crate) fn desktop_installed(&self) -> bool {
        self.desktop_app
            .as_ref()
            .is_some_and(|app| app.join("Contents/Info.plist").is_file())
    }

    /// The program inside the Claude app bundle this context names. `None` where the
    /// system runs no Claude app.
    pub(crate) fn desktop_program(&self) -> Option<&std::path::Path> {
        self.desktop_app_binary.as_deref()
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

    /// The `claude` Pitboard would run to sign someone in.
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

    /// The `codex` Pitboard would run to sign someone in.
    pub fn codex_program(&self) -> &std::path::Path {
        &self.codex_program
    }

    /// An app is not a command line, so it names the one it comes with for the schedule to
    /// run.
    pub fn with_schedule_program(mut self, program: PathBuf) -> Context {
        self.schedule_program = Some(program);
        self
    }

    /// The Pitboard the daily renewal schedule is written to run, where one was named.
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

    /// The scheduler's job this process runs as, where a test has said. The scheduler's own
    /// word for it, where it has one, is its to read.
    pub(crate) fn scheduled_job(&self) -> Option<&str> {
        self.scheduled_job.as_deref()
    }

    /// Say this process runs as the scheduler's job `label`, which no test does.
    #[cfg(test)]
    pub(crate) fn with_scheduled_job(mut self, label: String) -> Context {
        self.scheduled_job = Some(label);
        self
    }

    /// The program named for this tool, found or not. Claude Desktop has none on a system
    /// that runs no Claude app, and the empty path stands for it there.
    pub fn program_for(&self, tool: ProviderId) -> &std::path::Path {
        match tool {
            ProviderId::Claude => &self.claude_program,
            ProviderId::Codex => &self.codex_program,
            ProviderId::Desktop => self.desktop_program().unwrap_or(std::path::Path::new("")),
        }
    }

    /// Run `program` for this tool, where an app found it.
    pub(crate) fn set_program(&mut self, tool: ProviderId, program: PathBuf) {
        match tool {
            ProviderId::Claude => self.claude_program = program,
            ProviderId::Codex => self.codex_program = program,
            // Claude Desktop is found in its bundle, which the context already names, and
            // never on a search path.
            ProviderId::Desktop => {}
        }
    }

    /// The command line's context, from this process's own environment.
    pub fn from_env() -> Context {
        Context::for_command_line(&Environment::of_this_process())
    }

    /// The command line's context, from the environment it was started with: a shell's, so
    /// each tool's program is looked for on its `PATH` when it runs.
    pub fn for_command_line(env: &Environment) -> Context {
        Context::read(env, "cli")
    }

    /// The command line's context for a unit test: this process's environment, less every
    /// variable Pitboard reads, so what the person running the tests exported changes
    /// nothing a test checks. `PITBOARD_NO_ARGV=1` would fail a keychain test, and
    /// `PITBOARD_CLAUDE` would name the program a test runs.
    #[cfg(test)]
    pub(crate) fn for_unit_test() -> Context {
        let kept: Environment = std::env::vars_os()
            .filter(|(name, _)| !variables().any(|read| name == read))
            .collect();
        Context::for_command_line(&kept)
    }

    /// Everything `env` says, read the same way for every front end; `caller` names the one
    /// asking, for the audit log.
    ///
    /// Each tool's program is the one its own variable names, `PITBOARD_CLAUDE` or
    /// `PITBOARD_CODEX`, or else its bare name, looked for on `PATH` when it runs. Where
    /// to look is all a front end adds: an app has no shell's `PATH`, and finds the
    /// programs before it runs anything.
    pub(crate) fn read(env: &Environment, caller: &str) -> Context {
        let home = env.home();
        let owned = |name: &str| env.text(name).map(str::to_owned);
        let program = |tool: ProviderId| {
            env.path(tool.program_variable())
                .filter(|named| !named.is_empty())
                .map_or_else(|| PathBuf::from(tool.program()), PathBuf::from)
        };
        let (desktop_app, desktop_app_binary) =
            desktop_app(env.path("PITBOARD_CLAUDE_DESKTOP_APP").map(PathBuf::from));
        Context {
            pitboard_home: env.pitboard_home(),
            home,
            claude_config_dir: owned("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()),
            secure_storage_dir: owned("CLAUDE_SECURESTORAGE_CONFIG_DIR"),
            user: owned("USER"),
            custom_oauth: env.set("CLAUDE_CODE_CUSTOM_OAUTH_URL"),
            argv_fallback: env.text("PITBOARD_NO_ARGV") != Some("1"),
            overriding_auth: crate::settings::OVERRIDING_ENV
                .iter()
                .filter(|name| env.set(name))
                .map(|name| (*name).to_string())
                .collect(),
            caller: caller.into(),
            claude_program: program(ProviderId::Claude),
            api_base: owned("PITBOARD_API_BASE"),
            hover_rest: matches!(env.text("CLAUDE_CODE_HOVER_REST"), Some("1" | "true")),
            codex_home: owned("CODEX_HOME").filter(|v| !v.is_empty()),
            codex_program: program(ProviderId::Codex),
            desktop_dir: env
                .path("PITBOARD_CLAUDE_DESKTOP_DIR")
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from),
            desktop_app,
            desktop_app_binary,
            web_base: owned("PITBOARD_CLAUDE_WEB_BASE").filter(|v| !v.is_empty()),
            schedule_program: None,
            // Unset is nowhere, as it is to the shell: nothing is found on an empty `PATH`.
            search_path: Some(env.path("PATH").unwrap_or_default().to_os_string()),
            scheduled_job: None,
            clock: Arc::new(SystemClock),
            host: crate::host::current(),
            api: Arc::new(Anthropic),
            openai: Arc::new(OpenAiNetwork),
            safe_storage: crate::host::safe_storage(),
            web: Arc::new(ClaudeAi),
        }
    }

    /// Answer for every service from a script, where a test can produce a 429 or a
    /// refusal. One script for all of them, so a test that forgets to script a tool's
    /// service gets a refusal rather than a request to the real one.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_scripted_api(mut self, api: Arc<crate::api::scripted::ScriptedApi>) -> Context {
        self.api = api.clone();
        self.openai = api.clone();
        self.web = api;
        self
    }

    /// Read Claude Desktop's key from a script, which a test needs to say what macOS
    /// answered without anybody's real key being read.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_scripted_safe_storage(
        mut self,
        storage: Arc<crate::api::scripted::ScriptedSafeStorage>,
    ) -> Context {
        self.safe_storage = storage;
        self
    }

    /// Put the credential stores in memory, where a test can make them fail. Only the
    /// tests do this, which is why the trait behind it is not public.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_memory_stores(mut self, memory: Arc<crate::host::memory::MemoryHost>) -> Context {
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
    use std::path::Path;

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

    /// Claude Desktop is looked for in the system's own place for it unless a test or an
    /// app names another, and empty names nothing. A Mac starts the program inside the
    /// bundle; Linux runs no Claude app, so it names none there, and no program even in a
    /// bundle somebody named.
    #[test]
    fn claude_desktop_is_looked_for_where_the_context_says() {
        use crate::host::{OS, Os};
        use crate::provider::ProviderId;
        let program = |app: &str| match OS {
            Os::MacOs => PathBuf::from(app).join("Contents/MacOS/Claude"),
            Os::Linux => PathBuf::new(),
        };
        let own = OS.claude_desktop_app().map(PathBuf::from);
        let ctx = Context::new(PathBuf::from("/home/x"));
        assert_eq!(ctx.desktop_dir, None);
        assert_eq!(ctx.desktop_app, own);
        assert!(!ctx.desktop_app_moved());
        assert_eq!(
            ctx.program_for(ProviderId::Desktop),
            own.as_deref()
                .map_or_else(PathBuf::new, |app| program(app.to_str().unwrap()))
        );
        let moved = ctx
            .with_desktop_dir("/scratch/claude-desktop".into())
            .with_desktop_app("/scratch/Claude.app".into());
        assert_eq!(
            moved.desktop_dir,
            Some(PathBuf::from("/scratch/claude-desktop"))
        );
        assert_eq!(
            moved.desktop_app,
            Some(PathBuf::from("/scratch/Claude.app"))
        );
        assert!(moved.desktop_app_moved());
        assert_eq!(
            moved.program_for(ProviderId::Desktop),
            program("/scratch/Claude.app")
        );
        let unset = moved
            .with_desktop_dir(String::new())
            .with_desktop_app(String::new());
        assert_eq!(unset.desktop_dir, None, "empty means unset");
        assert_eq!(unset.desktop_app, own, "empty means unset");
    }

    /// The command line reads Claude Desktop's two variables the way the app is given them,
    /// so both look at the same Claude, and an empty one is unset.
    #[test]
    fn the_desktop_variables_are_read() {
        use crate::host::{OS, Os};
        use crate::provider::ProviderId;
        let env: Environment = [
            ("HOME", "/scratch"),
            ("PITBOARD_CLAUDE_DESKTOP_DIR", "/scratch/support"),
            ("PITBOARD_CLAUDE_DESKTOP_APP", "/scratch/Claude.app"),
        ]
        .into_iter()
        .collect();
        let ctx = Context::for_command_line(&env);
        assert_eq!(ctx.desktop_dir, Some(PathBuf::from("/scratch/support")));
        assert_eq!(ctx.desktop_app, Some(PathBuf::from("/scratch/Claude.app")));
        assert_eq!(
            ctx.program_for(ProviderId::Desktop),
            match OS {
                Os::MacOs => Path::new("/scratch/Claude.app/Contents/MacOS/Claude"),
                Os::Linux => Path::new(""),
            }
        );
        let empty: Environment = [
            ("HOME", "/scratch"),
            ("PITBOARD_CLAUDE_DESKTOP_DIR", ""),
            ("PITBOARD_CLAUDE_DESKTOP_APP", ""),
        ]
        .into_iter()
        .collect();
        let unset = Context::for_command_line(&empty);
        assert_eq!(unset.desktop_dir, None, "empty is unset");
        assert_eq!(
            unset.desktop_app,
            OS.claude_desktop_app().map(PathBuf::from),
            "empty is unset"
        );
        assert!(!unset.desktop_app_moved());
    }

    #[test]
    fn only_a_front_end_that_names_one_changes_what_the_schedule_runs() {
        assert_eq!(Context::for_unit_test().schedule_program(), None);
        let ctx = Context::new(PathBuf::from("/Users/x"));
        assert_eq!(ctx.schedule_program(), None);
        let bundled = PathBuf::from("/Applications/Pitboard.app/Contents/Helpers/pitboard");
        assert_eq!(
            ctx.with_schedule_program(bundled.clone())
                .schedule_program(),
            Some(bundled.as_path())
        );
    }

    /// The command line reads the environment it is handed rather than this process's, so
    /// every variable can be tried without changing what the tests themselves run in.
    #[test]
    fn the_command_line_reads_the_environment_it_is_given() {
        let env: Environment = [
            ("HOME", "/Users/x"),
            ("PITBOARD_CLAUDE", "/elsewhere/claude"),
            ("PITBOARD_CODEX", ""),
            ("CLAUDE_CODE_CUSTOM_OAUTH_URL", "https://oauth.example"),
            ("ANTHROPIC_API_KEY", "not-a-key"),
            ("ANTHROPIC_AUTH_TOKEN", ""),
            ("PATH", "/opt/tools/bin:/usr/bin"),
        ]
        .into_iter()
        .collect();
        let ctx = Context::for_command_line(&env);
        assert_eq!(ctx.claude_program(), Path::new("/elsewhere/claude"));
        assert_eq!(
            ctx.codex_program(),
            Path::new("codex"),
            "empty names nothing"
        );
        assert!(ctx.custom_oauth());
        assert_eq!(
            ctx.overriding_auth(),
            ["ANTHROPIC_API_KEY"],
            "empty is unset"
        );
        assert_eq!(ctx.search_path(), "/opt/tools/bin:/usr/bin");
        assert_eq!(ctx.pitboard_home, PathBuf::from("/Users/x/.pitboard"));
        assert_eq!(ctx.caller, "cli");
        assert_eq!(
            Context::for_command_line(&Environment::default()).search_path(),
            "",
            "no PATH is nowhere, not this process's"
        );
    }

    /// A variable read without being listed is caught the first time a test reads it, so the
    /// list the tests withhold from every command they run cannot fall behind what is read.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "PITBOARD_SOMETHING_NEW is read but not listed")]
    fn a_variable_read_without_being_listed_is_caught() {
        let _ = Environment::default().path("PITBOARD_SOMETHING_NEW");
    }

    /// With no `HOME`, the home is the account's own, as the passwd database names it, which
    /// is where the app has always looked. The command line took an empty path instead, and
    /// so kept its files in `.pitboard` wherever it was run from.
    #[test]
    fn without_home_the_home_is_the_accounts_own() {
        let own = crate::host::user::home().expect("this account has a home");
        let ctx = Context::for_command_line(&Environment::default());
        assert_eq!(ctx.home, own);
        assert_eq!(ctx.pitboard_home, own.join(".pitboard"));
        let empty: Environment = [("HOME", "")].into_iter().collect();
        assert_eq!(
            Context::for_command_line(&empty).home,
            PathBuf::new(),
            "set, even empty, it is what the person said"
        );
    }

    /// The home and Pitboard directory an app asks of the environment it was started with
    /// are the ones the context it is given reads, path for path, so an app that keys records
    /// of its own by Pitboard's directory keys them by the core's.
    #[test]
    fn the_homes_an_app_asks_for_are_the_contexts() {
        let cases: [&[(&str, &str)]; 5] = [
            &[],
            &[("HOME", "/Users/x")],
            &[("HOME", "/Users/x/")],
            &[
                ("HOME", "/Users/x"),
                ("PITBOARD_HOME", "/elsewhere/./pitboard/"),
            ],
            &[("PITBOARD_HOME", "")],
        ];
        for pairs in cases {
            let env: Environment = pairs.iter().copied().collect();
            let ctx = Context::for_command_line(&env);
            assert_eq!(env.home(), ctx.home, "{pairs:?}");
            assert_eq!(env.pitboard_home(), ctx.pitboard_home, "{pairs:?}");
        }
        let given: Environment = [("PITBOARD_HOME", "/elsewhere/./pitboard/")]
            .into_iter()
            .collect();
        assert_eq!(
            given.pitboard_home().as_os_str(),
            "/elsewhere/./pitboard/",
            "as the environment gives it"
        );
    }
}
