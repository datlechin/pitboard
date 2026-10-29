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

use crate::host::{Host, Os};
use crate::provider::ProviderId;
use crate::provider::claude::api::{Anthropic, Api};
use crate::provider::codex::api::{Network as OpenAiNetwork, OpenAi};
use crate::time::{Clock, SystemClock};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::sync::Arc;

/// Every variable Pitboard reads from the environment it was started with, besides
/// [`crate::settings::OVERRIDING_ENV`] and the proxy variables, which [`crate::proxy`] lists
/// in the order ureq tries them: what [`Context::read`] consults, the shell an app
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
    "CLAUDE_CONFIG_DIR",
    "CLAUDE_SECURESTORAGE_CONFIG_DIR",
    "CLAUDE_CODE_CUSTOM_OAUTH_URL",
    "CLAUDE_CODE_HOVER_REST",
    "CLAUDE_CODE_FORCE_WINDOWS_CREDMAN",
    "CLAUDE_CODE_GIT_BASH_PATH",
    "ANTHROPIC_PROFILE",
    "ANTHROPIC_FEDERATION_RULE_ID",
    "ANTHROPIC_ORGANIZATION_ID",
    "ANTHROPIC_CONFIG_DIR",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "XDG_RUNTIME_DIR",
    "DBUS_SESSION_BUS_ADDRESS",
    "WAYLAND_DISPLAY",
    "DISPLAY",
    "SSH_CONNECTION",
    "WSL_DISTRO_NAME",
    "CODEX_HOME",
    "CODEX_INSTALL_DIR",
    "NO_COLOR",
    "XPC_SERVICE_NAME",
    "SUDO_UID",
    "USERPROFILE",
    "USERNAME",
    "APPDATA",
    "LOCALAPPDATA",
    "ProgramData",
    "ProgramFiles",
    "SystemRoot",
    "PATHEXT",
    "TEMP",
    "TMP",
    "OneDrive",
    "OneDriveCommercial",
    "OneDriveConsumer",
];

// Read only by a build for tests: a release lists none and reads none.
#[cfg(any(test, feature = "test-support"))]
const READ_BY_BUILDS_FOR_TESTS: &[&str] = &[TEST_CLAUDE_DESKTOP, TEST_BUS];
#[cfg(not(any(test, feature = "test-support")))]
const READ_BY_BUILDS_FOR_TESTS: &[&str] = &[];

#[cfg(any(test, feature = "test-support"))]
const TEST_CLAUDE_DESKTOP: &str = "PITBOARD_TEST_CLAUDE_DESKTOP";
// The D-Bus address of the bus a test started, the only one a build for tests reaches on Linux.
#[cfg(any(test, feature = "test-support"))]
const TEST_BUS: &str = "PITBOARD_TEST_BUS";

/// Every variable Pitboard reads from its environment, by name.
pub(crate) fn variables() -> impl Iterator<Item = &'static str> {
    READ.iter()
        .chain(READ_BY_BUILDS_FOR_TESTS.iter())
        .chain(crate::settings::OVERRIDING_ENV.iter())
        .chain(crate::proxy::NAMING.iter())
        .chain(crate::proxy::EXEMPTING.iter())
        .copied()
}

/// The variables a process was started with, by name.
///
/// The command line has a shell's. An app the system started has launchd's, which name the
/// home and the login and little else, unless somebody set more with `launchctl setenv`;
/// whatever is there is read the way the command line reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Environment {
    os: Os,
    variables: HashMap<OsString, (OsString, OsString)>,
}

impl Default for Environment {
    fn default() -> Environment {
        Environment::on(crate::host::OS, std::iter::empty::<(OsString, OsString)>())
    }
}

impl Environment {
    pub(crate) fn of_this_process() -> Environment {
        std::env::vars_os().collect()
    }

    pub(crate) fn on<K: Into<OsString>, V: Into<OsString>>(
        os: Os,
        pairs: impl IntoIterator<Item = (K, V)>,
    ) -> Environment {
        let mut variables = HashMap::new();
        for (name, value) in pairs {
            let (name, value) = (name.into(), value.into());
            let key = os.variable_key(&name);
            match os {
                Os::MacOs | Os::Linux => {
                    variables.insert(key, (name, value));
                }
                // Windows hands a program the first of two spellings of a name
                // (docs/facts/windows-runners.md).
                Os::Windows => {
                    variables.entry(key).or_insert((name, value));
                }
            }
        }
        Environment { os, variables }
    }

    pub(crate) fn os(&self) -> Os {
        self.os
    }

    /// `name`'s value, whatever bytes it holds, as a path may hold any.
    pub(crate) fn path(&self, name: &str) -> Option<&OsStr> {
        debug_assert!(
            variables().any(|read| read == name),
            "{name} is read but not listed in context::READ, so the tests would take it \
             from whoever runs them"
        );
        self.variables
            .get(&self.os.variable_key(OsStr::new(name)))
            .map(|(_, value)| value.as_os_str())
    }

    /// `name`'s value as text. One that is not UTF-8 reads as unset, as `std::env::var`
    /// reads it.
    pub(crate) fn text(&self, name: &str) -> Option<&str> {
        self.path(name).and_then(OsStr::to_str)
    }

    // On Windows `all_proxy` may be how `ALL_PROXY` was set.
    pub(crate) fn spelling(&self, name: &str) -> Option<&OsStr> {
        self.path(name)?;
        self.variables
            .get(&self.os.variable_key(OsStr::new(name)))
            .map(|(spelled, _)| spelled.as_os_str())
    }

    /// Whether `name` holds anything: empty reads as unset.
    pub(crate) fn set(&self, name: &str) -> bool {
        self.text(name).is_some_and(|v| !v.is_empty())
    }

    // The folder `name` names, whatever bytes it holds; none where it is unset or empty.
    pub(crate) fn named_folder(&self, name: &str) -> Option<PathBuf> {
        self.path(name)
            .filter(|folder| !folder.is_empty())
            .map(PathBuf::from)
    }

    // Set, even empty, it is the person's: `check_absolute` refuses an empty or relative one.
    pub(crate) fn home(&self) -> PathBuf {
        self.path(self.os.home_variable())
            .map(PathBuf::from)
            .or_else(crate::host::user::home)
            .unwrap_or_default()
    }

    // As the environment spells it, any `.`, `..` or trailing separator kept.
    pub fn pitboard_home(&self) -> PathBuf {
        self.path("PITBOARD_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.os.default_pitboard_home(&self.home()))
    }

    /// Every variable, as a program started in this environment is given them.
    #[cfg_attr(
        windows,
        allow(
            dead_code,
            reason = "what the login shell is given, and Windows has none (`host::login_path`)"
        )
    )]
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&OsStr, &OsStr)> {
        self.variables
            .values()
            .map(|(name, value)| (name.as_os_str(), value.as_os_str()))
    }
}

/// The one folder every home of [`Context::for_unit_test`] names for the test running on
/// this thread: one per test, since the test harness names each test's thread after the
/// test, under the temporary directory, and never made by anything here.
#[cfg(test)]
fn unit_test_sentinel() -> PathBuf {
    let thread = std::thread::current();
    let test = thread.name().map_or_else(
        || format!("{:?}", thread.id()),
        |name| {
            name.chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect()
        },
    );
    std::env::temp_dir()
        .join(format!("pitboard-unit-test-{}", std::process::id()))
        .join(test)
}

impl<K: Into<OsString>, V: Into<OsString>> FromIterator<(K, V)> for Environment {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(pairs: I) -> Environment {
        Environment::on(crate::host::OS, pairs)
    }
}

// Declared in every build, so code behind `cfg!` compiles in a release too.
#[derive(Debug, Clone)]
pub struct ScratchClaude {
    pub data: PathBuf,
    pub app: PathBuf,
}

// Through every link, against the home the system names whatever the environment says.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn refuse_a_real_claude(path: &std::path::Path) {
    use crate::host::user;
    assert!(
        !path
            .components()
            .any(|part| part == std::path::Component::ParentDir),
        "a test named {} for Claude through `..`, which may lead anywhere once a folder is made: \
         give it a folder of its own",
        path.display()
    );
    #[cfg(test)]
    let fake = testing::guarded_home();
    #[cfg(not(test))]
    let fake: Option<PathBuf> = None;
    let real = match crate::host::OS {
        Os::MacOs | Os::Linux => {
            ["Library/Application Support", "Applications/Claude.app"]
                .into_iter()
                .any(|below| {
                    user::lies_in_the_accounts_own(path, below)
                        || fake
                            .as_ref()
                            .is_some_and(|home| crate::host::lies_in(path, &home.join(below)))
                })
                || crate::host::lies_in(path, std::path::Path::new("/Applications/Claude.app"))
        }
        Os::Windows => {
            let (mut roaming, mut local) = user::app_data_folders(true);
            if let Some(home) = &fake {
                roaming.push(home.join("AppData").join("Roaming"));
                local.push(home.join("AppData").join("Local"));
            }
            crate::host::claude_desktop_on_windows(&roaming, &local)
                .iter()
                .any(|place| crate::host::lies_in(path, place))
        }
    };
    assert!(
        !real,
        "a test named {} for Claude, which is, holds or lies in where this account's own \
         Claude Desktop keeps its data or its app: give it a folder of its own",
        path.display()
    );
}

#[cfg(test)]
pub(crate) mod testing {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    thread_local! {
        static GUARDED: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    }

    // While what this returns is kept, the guards on this thread's test refuse `home`'s places
    // as well as the account's own, so a test of them need name no real place.
    pub(crate) fn also_guarding(home: &Path) -> AlsoGuarding {
        GUARDED.with(|guarded| *guarded.borrow_mut() = Some(home.to_path_buf()));
        AlsoGuarding(())
    }

    pub(crate) struct AlsoGuarding(());

    impl Drop for AlsoGuarding {
        fn drop(&mut self) {
            GUARDED.with(|guarded| guarded.borrow_mut().take());
        }
    }

    pub(super) fn guarded_home() -> Option<PathBuf> {
        GUARDED.with(|guarded| guarded.borrow().clone())
    }
}

#[derive(Clone, Debug)]
pub struct Context {
    // This build's system, but where a test reads an environment as another's.
    pub(crate) os: Os,
    pub(crate) home: PathBuf,
    pub(crate) pitboard_home: PathBuf,
    /// `CLAUDE_CONFIG_DIR`, held as it is set, empty included. Claude Code reads an empty
    /// value as unset for its config file and its credential slot's name, but takes it as
    /// the empty path for its config dir, which then is whatever folder it runs in, as the
    /// register's `config_file_location` says. So an empty one is a home that is not a full
    /// path, and refused as one ([`crate::home::check_absolute`]).
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
    pub(crate) anthropic: crate::settings::Profiles,
    /// Whether a login too large for `security -i` may be written the way Claude Code
    /// writes it: as a command argument, where `ps` can see it for the length of the call.
    /// On by default, because there is no third way and Claude Code writes the same
    /// document that way itself on every token refresh.
    pub(crate) argv_fallback: bool,
    /// Which front end asked, for the audit log. A change made from the menu bar and one
    /// typed at a prompt read the same otherwise.
    pub(crate) caller: String,
    // each tool's program for a sign-in; a bare name is looked up on the search path
    pub(crate) programs: std::collections::BTreeMap<ProviderId, PathBuf>,
    /// Where Anthropic's endpoints are reached instead, for tests; `api` honours loopback only.
    pub(crate) api_base: Option<String>,
    /// The proxy the environment names for Pitboard's requests, read as ureq 3.4.2 reads one.
    /// None in a context made without an environment.
    pub(crate) proxy: crate::proxy::Proxies,
    /// The agent this context's requests go out on, made with `proxy` for the first request
    /// and shared by every copy of the context.
    pub(crate) agent: crate::api::Agents,
    /// `CLAUDE_CODE_HOVER_REST`, which switches on Claude Code's successor credential backend.
    pub(crate) hover_rest: bool,
    // `CLAUDE_CODE_FORCE_WINDOWS_CREDMAN` as exactly `1`, which turns Claude Code's Credential
    // Manager store on (`windows_backend_choice`).
    pub(crate) credential_manager_forced: bool,
    /// `CODEX_HOME`, which moves everything Codex keeps, including its keyring account.
    pub(crate) codex_home: Option<String>,
    // Windows' `FOLDERID_Profile`, where Codex finds its home with `CODEX_HOME` unset.
    pub(crate) profile: Option<PathBuf>,
    // Windows' `LOCALAPPDATA`, as the environment gives it.
    pub(crate) local_app_data: Option<PathBuf>,
    // Windows' `ProgramFiles` and `SystemRoot`, where Claude Code looks for PowerShell.
    pub(crate) program_files: Option<PathBuf>,
    pub(crate) system_root: Option<PathBuf>,
    // The person's Windows SID, which an `icacls` line names and a report hides.
    pub(crate) sid: Option<String>,
    // The folders OneDrive syncs on Windows, as its variables name them.
    pub(crate) one_drive: Vec<PathBuf>,
    /// The Pitboard the daily renewal schedule runs. `None` is this program, which is right
    /// for the command line and wrong for an app: the schedule runs `pitboard renew`, so an
    /// app names the command line it comes with.
    pub(crate) schedule_program: Option<PathBuf>,
    /// Where a tool's program is looked for, in `PATH`'s form, and what its sign-in is given
    /// as `PATH`, behind the program's own directory where that is not on it. `None` is this
    /// process's own `PATH`: an app opened from Finder has almost nothing on it, so it
    /// passes the one the person's login shell would have.
    pub(crate) search_path: Option<OsString>,
    // Where each tool's installers put its program, which an app also looks in and a command
    // line leaves to the shell's `PATH`.
    pub(crate) installer_places: std::collections::BTreeMap<ProviderId, Vec<PathBuf>>,
    // `PATHEXT`, which orders the names a Windows shell tries for a program.
    pub(crate) path_ext: Option<OsString>,
    // `CLAUDE_CODE_GIT_BASH_PATH`, the Git Bash Claude Code looks at first on Windows.
    pub(crate) git_bash: Option<PathBuf>,
    // `WSL_DISTRO_NAME` on Linux, set even empty, which WSL gives its programs.
    pub(crate) wsl_distro: Option<String>,
    // The base folders of XDG Base Directory 0.8 on macOS and Linux, Pitboard's reading of
    // them: Claude Code reads `XDG_CONFIG_HOME` its own way (`settings::Profiles`).
    pub(crate) config_home: Option<PathBuf>,
    pub(crate) data_home: Option<PathBuf>,
    pub(crate) cache_home: Option<PathBuf>,
    // `XDG_RUNTIME_DIR` and `DBUS_SESSION_BUS_ADDRESS` on Linux, where the user's systemd and
    // the session's D-Bus are found.
    pub(crate) runtime_dir: Option<PathBuf>,
    pub(crate) session_bus_address: Option<String>,
    // On Linux, whether this run is in the desktop session, where the keyring's prompter shows
    // its dialog: with a display, `WAYLAND_DISPLAY` or `DISPLAY`, and not over SSH, whose
    // forwarded display is not the one the prompter shows on.
    pub(crate) desktop_session: bool,
    // Whether a person dismissed the keyring's unlock prompt in this run, shared by every copy
    // of the context, so it is not put up again for each question.
    pub(crate) prompt_dismissed: Arc<std::sync::atomic::AtomicBool>,
    /// The scheduler's job this process runs as, where a test says. `None` is whatever the
    /// scheduler said when it started this process.
    pub(crate) scheduled_job: Option<String>,
    /// The job of Pitboard's whose marker this process was started with, as the front end it
    /// was started as read it ([`crate::schedule::Job::marked`]).
    pub(crate) started_by: Option<crate::schedule::Job>,
    /// Whether this process was started under sudo, which sets `SUDO_UID`. Part of what the
    /// host answers when the one gate every change passes asks whether this process runs as
    /// the person themselves.
    pub(crate) sudo: bool,
    /// Where the time comes from. The machine's clock in every real context; a test puts
    /// its own here to reach the judgements that only happen at a particular moment.
    pub(crate) clock: Arc<dyn Clock>,
    /// How Claude Code's locks are kept and waited for: as Claude Code does in every real
    /// context.
    pub(crate) locks: crate::lock::Locks,
    /// The machine: its stores, its processes, its scheduler. This build's host in every
    /// real context.
    pub(crate) host: Arc<dyn Host>,
    /// Who answers for Anthropic. The network in every real context.
    pub(crate) api: Arc<dyn Api>,
    /// Who answers for OpenAI. The network in every real context.
    pub(crate) openai: Arc<dyn OpenAi>,
    /// Who plays each tool's own sign-in in place of its program, where a test or a fixture
    /// says. `None` in every real context, and in any build without `test-support` there is
    /// nothing here at all.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) sign_in_script: Option<Arc<dyn crate::switch::SignInScript>>,
    #[cfg(any(test, feature = "test-support"))]
    scratch_claude: Option<ScratchClaude>,
    // The bus a test started, on Linux: in a build for tests, the only one reached.
    #[cfg(any(test, feature = "test-support"))]
    test_bus: Option<String>,
    #[cfg(any(test, feature = "test-support"))]
    awaited_every: Option<std::time::Duration>,
    #[cfg(test)]
    unknown_machine: bool,
    /// Who plays a warm-up's run in place of the tool's program, where a test or a fixture
    /// says. `None` in every real context, and in any build without `test-support` there is
    /// nothing here at all.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) warm_script: Option<Arc<dyn crate::duty::warm::run::WarmScript>>,
}

// As XDG Base Directory 0.8 reads a base folder: the one its variable names where that is a full
// path, since a relative one is ignored as one unset or empty, and otherwise `default` in the
// home. Windows has none.
fn base_folder(
    os: Os,
    home: &std::path::Path,
    named: Option<PathBuf>,
    default: &[&str],
) -> Option<PathBuf> {
    match os {
        Os::MacOs | Os::Linux => Some(named.filter(|folder| folder.is_absolute()).unwrap_or_else(
            || {
                default
                    .iter()
                    .fold(home.to_path_buf(), |at, part| at.join(part))
            },
        )),
        Os::Windows => None,
    }
}

/// Where a context finds the session's D-Bus ([`Context::session_bus`]).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    not(target_os = "linux"),
    allow(dead_code, reason = "only Linux has a session bus to find")
)]
pub(crate) enum SessionBus {
    /// A D-Bus address, as `DBUS_SESSION_BUS_ADDRESS` gives it.
    Address(String),
    /// A socket's path.
    Socket(PathBuf),
}

const CONFIG_HOME: &[&str] = &[".config"];
const DATA_HOME: &[&str] = &[".local", "share"];
const CACHE_HOME: &[&str] = &[".cache"];

fn programs(
    named: impl Fn(ProviderId, crate::provider::Cli) -> PathBuf,
) -> std::collections::BTreeMap<ProviderId, PathBuf> {
    ProviderId::ALL
        .iter()
        .filter_map(|&tool| Some((tool, named(tool, tool.cli()?))))
        .collect()
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

    /// The time zone of this context's clock: the machine's, unless a test says otherwise.
    pub(crate) fn zone(&self) -> jiff::tz::TimeZone {
        self.clock.zone()
    }

    /// The person's home directory, which is where a platform's own scheduler lives.
    pub(crate) fn home(&self) -> &std::path::Path {
        &self.home
    }

    /// Where XDG Base Directory 0.8 puts this context's configuration on macOS and Linux:
    /// `XDG_CONFIG_HOME` where it names a full path, else `.config` in the home. None on
    /// Windows.
    pub fn config_home(&self) -> Option<&std::path::Path> {
        self.config_home.as_deref()
    }

    /// The same for data: `XDG_DATA_HOME`, else `.local/share` in the home.
    pub fn data_home(&self) -> Option<&std::path::Path> {
        self.data_home.as_deref()
    }

    /// The same for what may be thrown away: `XDG_CACHE_HOME`, else `.cache` in the home.
    pub fn cache_home(&self) -> Option<&std::path::Path> {
        self.cache_home.as_deref()
    }

    /// `XDG_RUNTIME_DIR` on Linux, where it names a full path. None where it does not, as
    /// XDG Base Directory 0.8 ignores a relative one, and off Linux.
    #[cfg_attr(
        not(target_os = "linux"),
        allow(dead_code, reason = "only Linux has a user manager to find")
    )]
    pub(crate) fn runtime_dir(&self) -> Option<&std::path::Path> {
        self.runtime_dir.as_deref()
    }

    /// The session's D-Bus, as sd-bus finds it at systemd 262 (`bus_set_address_user`):
    /// `DBUS_SESSION_BUS_ADDRESS` as given, else `bus` in the runtime folder. None where neither
    /// is set, and off Linux.
    #[cfg_attr(
        not(target_os = "linux"),
        allow(dead_code, reason = "only Linux has a session bus to find")
    )]
    pub(crate) fn session_bus(&self) -> Option<SessionBus> {
        match (&self.session_bus_address, &self.runtime_dir) {
            (Some(address), _) => Some(SessionBus::Address(address.clone())),
            (None, Some(runtime)) => Some(SessionBus::Socket(runtime.join("bus"))),
            (None, None) => None,
        }
    }

    /// Whether this run is in the desktop session on Linux, where a dialog the session's
    /// keyring puts up is in front of whoever started it.
    #[cfg_attr(
        not(target_os = "linux"),
        allow(dead_code, reason = "only Linux's keyring is asked to unlock")
    )]
    pub(crate) fn in_the_desktop_session(&self) -> bool {
        self.desktop_session
    }

    /// Whether a person dismissed the keyring's unlock prompt in this run.
    #[cfg_attr(
        not(target_os = "linux"),
        allow(dead_code, reason = "only Linux's keyring is asked to unlock")
    )]
    pub(crate) fn unlock_was_dismissed(&self) -> bool {
        self.prompt_dismissed
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    #[cfg_attr(
        not(target_os = "linux"),
        allow(dead_code, reason = "only Linux's keyring is asked to unlock")
    )]
    pub(crate) fn unlock_dismissed(&self) {
        self.prompt_dismissed
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    // The one the schedule renews, whatever `PITBOARD_HOME` says.
    pub(crate) fn default_pitboard_home(&self) -> PathBuf {
        self.os.default_pitboard_home(&self.home)
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

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn new(home: PathBuf) -> Context {
        let base = |default| base_folder(crate::host::OS, &home, None, default);
        let (config_home, data_home, cache_home) =
            (base(CONFIG_HOME), base(DATA_HOME), base(CACHE_HOME));
        Context {
            os: crate::host::OS,
            pitboard_home: crate::host::OS.default_pitboard_home(&home),
            home,
            claude_config_dir: None,
            secure_storage_dir: None,
            user: None,
            custom_oauth: false,
            argv_fallback: true,
            overriding_auth: Vec::new(),
            anthropic: crate::settings::Profiles::default(),
            caller: "unknown".into(),
            programs: programs(|tool, _| PathBuf::from(tool.program())),
            api_base: None,
            proxy: crate::proxy::Proxies::default(),
            agent: crate::api::Agents::default(),
            hover_rest: false,
            credential_manager_forced: false,
            codex_home: None,
            profile: None,
            local_app_data: None,
            program_files: None,
            system_root: None,
            sid: None,
            one_drive: Vec::new(),
            schedule_program: None,
            search_path: None,
            installer_places: std::collections::BTreeMap::new(),
            path_ext: None,
            git_bash: None,
            wsl_distro: None,
            config_home,
            data_home,
            cache_home,
            runtime_dir: None,
            session_bus_address: None,
            desktop_session: false,
            prompt_dismissed: Arc::default(),
            scheduled_job: None,
            started_by: None,
            sudo: false,
            clock: Arc::new(SystemClock),
            locks: crate::lock::Locks::CLAUDE_CODE,
            host: crate::host::current(),
            api: Arc::new(Anthropic),
            openai: Arc::new(OpenAiNetwork),
            #[cfg(any(test, feature = "test-support"))]
            sign_in_script: None,
            #[cfg(any(test, feature = "test-support"))]
            scratch_claude: None,
            #[cfg(any(test, feature = "test-support"))]
            test_bus: None,
            #[cfg(any(test, feature = "test-support"))]
            awaited_every: None,
            #[cfg(test)]
            unknown_machine: false,
            #[cfg(any(test, feature = "test-support"))]
            warm_script: None,
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

    #[cfg(test)]
    pub(crate) fn with_profile(mut self, profile: PathBuf) -> Context {
        self.profile = Some(profile);
        self
    }

    pub fn with_pitboard_home(mut self, dir: PathBuf) -> Context {
        self.pitboard_home = dir;
        self
    }

    /// Claude Code's config directory, as `CLAUDE_CONFIG_DIR` names it. Empty is kept, as
    /// Claude Code keeps it: unset for its config file and its credential slot's name, but
    /// the empty path for its config dir, as the register's `config_file_location` says,
    /// which is refused as a home that is not a full path.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_claude_config_dir(mut self, dir: String) -> Context {
        self.claude_config_dir = Some(dir);
        self
    }

    /// Where a sign-in Pitboard ran keeps Claude Code's config, as `CLAUDE_CONFIG_DIR` named
    /// it for that sign-in, for reading what the sign-in recorded.
    pub(crate) fn signed_in_at(&self, dir: &std::path::Path) -> Context {
        let mut ctx = self.clone();
        ctx.claude_config_dir = Some(dir.to_string_lossy().into_owned());
        ctx
    }

    /// Empty is set, and pins the default slot, as Claude Code reads
    /// `CLAUDE_SECURESTORAGE_CONFIG_DIR`.
    #[cfg(test)]
    pub(crate) fn with_secure_storage_dir(mut self, dir: String) -> Context {
        self.secure_storage_dir = Some(dir);
        self
    }

    /// Allows the argument-line write for a login too large for the stdin one.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_argv_fallback(mut self, allowed: bool) -> Context {
        self.argv_fallback = allowed;
        self
    }

    /// Whether a custom OAuth endpoint is configured, which moves Claude Code's login.
    pub(crate) fn custom_oauth(&self) -> bool {
        self.custom_oauth
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn claude_program(&self) -> &std::path::Path {
        &self.programs[&ProviderId::Claude]
    }

    /// Whether the argument-line write is allowed for an oversized login.
    pub(crate) fn argv_fallback(&self) -> bool {
        self.argv_fallback
    }

    /// Environment variables that authenticate Claude Code some other way, if any.
    pub(crate) fn overriding_auth(&self) -> &[String] {
        &self.overriding_auth
    }

    /// Names the front end in the audit log.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_caller(mut self, caller: String) -> Context {
        self.caller = caller;
        self
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_user(mut self, user: String) -> Context {
        self.user = Some(user);
        self
    }

    /// An app started from Finder does not see the shell's `PATH`, so it names `claude` itself.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_claude_program(mut self, program: PathBuf) -> Context {
        self.programs.insert(ProviderId::Claude, program);
        self
    }

    /// The same for `codex`.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_codex_program(mut self, program: PathBuf) -> Context {
        self.programs.insert(ProviderId::Codex, program);
        self
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn codex_program(&self) -> &std::path::Path {
        &self.programs[&ProviderId::Codex]
    }

    /// An app is not a command line, so it names the one it comes with for the schedule to
    /// run.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
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
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_search_path(mut self, path: String) -> Context {
        self.search_path = Some(path.into());
        self
    }

    // How a shell on this context's system spells a program it looks for.
    pub(crate) fn lookup(&self) -> crate::host::program::Lookup {
        crate::host::program::Lookup::new(self.os, self.path_ext.as_deref())
    }

    /// Where a tool's program is looked for: the path given, or this process's `PATH`.
    pub(crate) fn search_path(&self) -> OsString {
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

    /// Whether this process was started under sudo.
    pub(crate) fn sudo(&self) -> bool {
        self.sudo
    }

    /// Say this process runs as the scheduler's job `label`, which no test does.
    #[cfg(test)]
    pub(crate) fn with_scheduled_job(mut self, label: String) -> Context {
        self.scheduled_job = Some(label);
        self
    }

    /// Say this process is a run of the daily renewal schedule, as the marker its job is
    /// started with says where what the system says of the job it started is not to be
    /// relied on: the command line does this for `pitboard renew --scheduled`. Such a run
    /// renews the default home whatever `PITBOARD_HOME` says
    /// ([`crate::schedule::SCHEDULED_RUN`]).
    pub fn started_by_the_schedule(mut self) -> Context {
        self.started_by = Some(crate::schedule::Job::Renew);
        self
    }

    /// The context a run of the watch service watches in, as the marker its job is started
    /// with says: the command line does this for `pitboard watch --scheduled`. It watches the
    /// default home whatever `PITBOARD_HOME` says, given the variables its job's file keeps
    /// where the system hands the job none, as a run of the renewal schedule renews.
    pub fn started_by_the_watch_service(mut self) -> Context {
        self.started_by = Some(crate::schedule::Job::Watch);
        crate::schedule::for_its_run(&self, crate::schedule::Job::Watch)
    }

    /// The job whose marker this process was started with.
    pub(crate) fn started_by(&self) -> Option<crate::schedule::Job> {
        self.started_by
    }

    // `given` over this context's own proxy variables, as a job's are given over the system's.
    pub(crate) fn with_proxy_variables_over(mut self, given: &[(String, String)]) -> Context {
        let os = self.os;
        let key = |name: &str| os.variable_key(OsStr::new(name));
        let own = self
            .proxy
            .variables()
            .iter()
            .filter(|(name, _)| !given.iter().any(|(over, _)| key(over) == key(name)))
            .map(|(name, value)| ((*name).to_string(), value.clone()));
        let env = Environment::on(os, given.iter().cloned().chain(own));
        self.proxy = crate::proxy::Proxies::read(&env);
        self.agent = crate::api::Agents::default();
        self
    }

    // The program named for this tool, found or not; None for a tool with no program to run.
    pub(crate) fn program_for(&self, tool: ProviderId) -> Option<&std::path::Path> {
        self.programs.get(&tool).map(PathBuf::as_path)
    }

    // The Claude a test named; None in a release.
    pub(crate) fn scratch_claude(&self) -> Option<&ScratchClaude> {
        #[cfg(any(test, feature = "test-support"))]
        {
            self.scratch_claude.as_ref()
        }
        #[cfg(not(any(test, feature = "test-support")))]
        {
            None
        }
    }

    // Refuses before anything there is listed or opened.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_scratch_claude(mut self, folder: PathBuf) -> Context {
        assert!(
            folder.is_absolute(),
            "a test named its Claude folder {} relative to wherever it runs: name it in full",
            folder.display()
        );
        let scratch = ScratchClaude {
            data: folder.join("data"),
            app: folder.join("Claude.app"),
        };
        for named in [&folder, &scratch.data, &scratch.app] {
            refuse_a_real_claude(named);
        }
        self.scratch_claude = Some(scratch);
        self
    }

    // The bus a test started, which a release has no word for.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(
        not(target_os = "linux"),
        allow(dead_code, reason = "only Linux has a session bus to find")
    )]
    pub(crate) fn test_bus(&self) -> Option<&str> {
        self.test_bus.as_deref()
    }

    /// Reach the D-Bus at `address` on Linux, a bus the test started: in a build for tests no
    /// other is reached, the session's own least of all.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_test_bus(mut self, address: String) -> Context {
        self.test_bus = Some(address);
        self
    }

    // How often a sign-in waited for asks the process list: a second, unless a test said.
    pub(crate) fn awaited_every(&self) -> std::time::Duration {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(every) = self.awaited_every {
            return every;
        }
        std::time::Duration::from_secs(1)
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_awaited_every(mut self, every: std::time::Duration) -> Context {
        self.awaited_every = Some(every);
        self
    }

    // What every park's manifest names its Mac by; `unknown` where a unit test said so.
    pub(crate) fn this_machine(&self) -> String {
        #[cfg(test)]
        if self.unknown_machine {
            return crate::state::UNKNOWN_MACHINE.into();
        }
        crate::state::machine_id()
    }

    // For a Claude Desktop test of a Mac that cannot be identified.
    #[cfg(all(test, not(windows)))]
    pub(crate) fn with_unknown_machine(mut self) -> Context {
        self.unknown_machine = true;
        self
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
    ///
    /// Every home is this test's sentinel ([`unit_test_sentinel`]): the person's home,
    /// Pitboard's own directory, Claude Code's config directory and Codex's home. So a test
    /// that forgets to make a scratch home of its own reads nothing real, and anything it
    /// writes lands in a folder of its own. Taking the home from the environment, as this
    /// did, gave every such test the real `~/.pitboard`, `~/.claude` and `~/.codex`.
    #[cfg(test)]
    pub(crate) fn for_unit_test() -> Context {
        let sentinel = unit_test_sentinel().into_os_string();
        let homes = [
            "HOME",
            "USERPROFILE",
            "PITBOARD_HOME",
            "CLAUDE_CONFIG_DIR",
            "CODEX_HOME",
        ];
        let kept: Environment = std::env::vars_os()
            .filter(|(name, _)| !variables().any(|read| name == read))
            .chain(homes.map(|home| (home.into(), sentinel.clone())))
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
        let program = |tool: ProviderId, cli: crate::provider::Cli| {
            env.path(cli.program_variable)
                .filter(|named| !named.is_empty())
                .map_or_else(|| PathBuf::from(tool.program()), PathBuf::from)
        };
        let base =
            |variable, default| base_folder(env.os(), &home, env.named_folder(variable), default);
        let (config_home, data_home, cache_home) = (
            base("XDG_CONFIG_HOME", CONFIG_HOME),
            base("XDG_DATA_HOME", DATA_HOME),
            base("XDG_CACHE_HOME", CACHE_HOME),
        );
        let read = Context {
            os: env.os(),
            pitboard_home: env.pitboard_home(),
            home,
            claude_config_dir: owned("CLAUDE_CONFIG_DIR"),
            secure_storage_dir: owned("CLAUDE_SECURESTORAGE_CONFIG_DIR"),
            user: owned(env.os().user_variable()),
            custom_oauth: env.set("CLAUDE_CODE_CUSTOM_OAUTH_URL"),
            argv_fallback: env.text("PITBOARD_NO_ARGV") != Some("1"),
            overriding_auth: crate::settings::OVERRIDING_ENV
                .iter()
                .filter(|name| env.set(name))
                .map(|name| (*name).to_string())
                .collect(),
            anthropic: crate::settings::Profiles::read(env),
            caller: caller.into(),
            programs: programs(program),
            api_base: owned("PITBOARD_API_BASE"),
            proxy: crate::proxy::Proxies::read(env),
            agent: crate::api::Agents::default(),
            hover_rest: matches!(env.text("CLAUDE_CODE_HOVER_REST"), Some("1" | "true")),
            credential_manager_forced: env.text("CLAUDE_CODE_FORCE_WINDOWS_CREDMAN") == Some("1"),
            codex_home: owned("CODEX_HOME").filter(|v| !v.is_empty()),
            profile: (env.os() == Os::Windows)
                .then(crate::host::user::profile)
                .flatten(),
            local_app_data: env
                .path("LOCALAPPDATA")
                .filter(|_| env.os() == Os::Windows)
                .map(PathBuf::from),
            program_files: env
                .named_folder("ProgramFiles")
                .filter(|_| env.os() == Os::Windows),
            system_root: env
                .named_folder("SystemRoot")
                .filter(|_| env.os() == Os::Windows),
            sid: (env.os() == Os::Windows)
                .then(crate::host::user::sid)
                .flatten(),
            one_drive: ["OneDrive", "OneDriveCommercial", "OneDriveConsumer"]
                .into_iter()
                .filter(|_| env.os() == Os::Windows)
                .filter_map(|name| env.path(name).filter(|folder| !folder.is_empty()))
                .map(PathBuf::from)
                .collect(),
            schedule_program: None,
            // Unset is nowhere, as it is to the shell: nothing is found on an empty `PATH`.
            search_path: Some(env.path("PATH").unwrap_or_default().to_os_string()),
            installer_places: std::collections::BTreeMap::new(),
            path_ext: env.path("PATHEXT").map(OsStr::to_os_string),
            git_bash: env
                .path("CLAUDE_CODE_GIT_BASH_PATH")
                .filter(|named| env.os() == Os::Windows && !named.is_empty())
                .map(PathBuf::from),
            wsl_distro: env
                .path("WSL_DISTRO_NAME")
                .filter(|_| env.os() == Os::Linux)
                .map(|named| named.to_string_lossy().into_owned()),
            config_home,
            data_home,
            cache_home,
            runtime_dir: env
                .named_folder("XDG_RUNTIME_DIR")
                .filter(|folder| env.os() == Os::Linux && folder.is_absolute()),
            session_bus_address: owned("DBUS_SESSION_BUS_ADDRESS")
                .filter(|address| env.os() == Os::Linux && !address.is_empty()),
            desktop_session: env.os() == Os::Linux
                && (env.set("WAYLAND_DISPLAY") || env.set("DISPLAY"))
                && !env.set("SSH_CONNECTION"),
            prompt_dismissed: Arc::default(),
            scheduled_job: None,
            started_by: None,
            sudo: env.set("SUDO_UID"),
            clock: Arc::new(SystemClock),
            locks: crate::lock::Locks::CLAUDE_CODE,
            host: crate::host::current(),
            api: Arc::new(Anthropic),
            openai: Arc::new(OpenAiNetwork),
            #[cfg(any(test, feature = "test-support"))]
            sign_in_script: None,
            #[cfg(any(test, feature = "test-support"))]
            scratch_claude: None,
            #[cfg(any(test, feature = "test-support"))]
            test_bus: owned(TEST_BUS)
                .filter(|address| env.os() == Os::Linux && !address.is_empty()),
            #[cfg(any(test, feature = "test-support"))]
            awaited_every: None,
            #[cfg(test)]
            unknown_machine: false,
            #[cfg(any(test, feature = "test-support"))]
            warm_script: None,
        };
        #[cfg(any(test, feature = "test-support"))]
        if let Some(folder) = env.path(TEST_CLAUDE_DESKTOP).filter(|f| !f.is_empty()) {
            return read.with_scratch_claude(PathBuf::from(folder));
        }
        read
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

    /// Ask for a lock of Claude Code's somebody holds `retries` times more, `every` apart,
    /// rather than as Claude Code does. Only the tests do this: one that holds a lock lets go
    /// of it at a moment of its own, not after a while.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_lock_waiting(mut self, retries: u32, every: std::time::Duration) -> Context {
        self.locks = crate::lock::Locks {
            write: self.locks.write.asked(retries, every),
            refresh: self.locks.refresh.asked(retries, every),
        };
        self
    }

    /// Play each tool's own sign-in with `script` rather than start its program: what the
    /// tool says, what it reads typed back, and the login it stores where the tool would. A
    /// test or a fixture that may start no program does this, and nothing else does, which is
    /// why it is not part of the builder a front end uses.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_sign_in_script(
        mut self,
        script: Arc<dyn crate::switch::enroll::SignInScript>,
    ) -> Context {
        self.sign_in_script = Some(script);
        self
    }

    /// Who plays each tool's own sign-in, where somebody said.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn sign_in_script(&self) -> Option<&Arc<dyn crate::switch::enroll::SignInScript>> {
        self.sign_in_script.as_ref()
    }

    /// Play each warm-up's run with `script` rather than start the tool's program. A test or a
    /// fixture that may start no program does this, and nothing else does.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_warm_script(
        mut self,
        script: Arc<dyn crate::duty::warm::run::WarmScript>,
    ) -> Context {
        self.warm_script = Some(script);
        self
    }

    /// Who plays a warm-up's run, where somebody said.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn warm_script(&self) -> Option<&Arc<dyn crate::duty::warm::run::WarmScript>> {
        self.warm_script.as_ref()
    }

    #[cfg(any(test, feature = "test-support"))]
    fn reaches_only(
        &self,
        memory: &Arc<crate::host::memory::MemoryHost>,
        api: &Arc<crate::api::scripted::ScriptedApi>,
    ) -> bool {
        std::ptr::addr_eq(Arc::as_ptr(&self.host), Arc::as_ptr(memory))
            && std::ptr::addr_eq(Arc::as_ptr(&self.api), Arc::as_ptr(api))
            && std::ptr::addr_eq(Arc::as_ptr(&self.openai), Arc::as_ptr(api))
    }
}

// A test's or a fixture's machine: its home, the stores in memory and every service scripted.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub struct ScratchMachine {
    pub ctx: Context,
    pub host: Arc<crate::host::memory::MemoryHost>,
    pub api: Arc<crate::api::scripted::ScriptedApi>,
}

#[cfg(any(test, feature = "test-support"))]
impl ScratchMachine {
    // Panics where `more` put back this machine's keychain or the network, before any write.
    pub fn at(home: &std::path::Path, more: impl FnOnce(Context) -> Context) -> ScratchMachine {
        ScratchMachine::on(crate::host::memory::MemoryHost::new(), home, more)
    }

    // As `at`, on a machine in memory a test made, such as one that parks by form.
    pub fn on(
        host: Arc<crate::host::memory::MemoryHost>,
        home: &std::path::Path,
        more: impl FnOnce(Context) -> Context,
    ) -> ScratchMachine {
        let api = crate::api::scripted::ScriptedApi::new();
        let ctx = more(
            Context::new(home.to_path_buf())
                .with_memory_stores(Arc::clone(&host))
                .with_scripted_api(Arc::clone(&api)),
        );
        assert!(
            ctx.reaches_only(&host, &api),
            "a scratch machine reaches this one's keychain or the network"
        );
        ScratchMachine { ctx, host, api }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn the_test_variable_is_listed_only_in_a_build_for_tests() {
        assert!(variables().any(|name| name == "PITBOARD_TEST_CLAUDE_DESKTOP"));
        assert!(!READ.contains(&"PITBOARD_TEST_CLAUDE_DESKTOP"));
        assert!(Context::for_unit_test().scratch_claude().is_none());
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let folder = scratch.path().join("claude");
        let named = folder.to_string_lossy().into_owned();
        let env: Environment = [
            ("HOME", "/home/x"),
            ("PITBOARD_TEST_CLAUDE_DESKTOP", named.as_str()),
        ]
        .into_iter()
        .collect();
        let read = Context::for_command_line(&env);
        let scratch = read.scratch_claude().expect("named");
        assert_eq!(scratch.data, folder.join("data"));
        assert_eq!(scratch.app, folder.join("Claude.app"));
    }

    // Withheld from every command a test runs unless it passes its own bus on by name.
    #[test]
    fn the_test_bus_is_listed_only_in_a_build_for_tests_and_read_on_linux() {
        assert!(variables().any(|name| name == "PITBOARD_TEST_BUS"));
        assert!(!READ.contains(&"PITBOARD_TEST_BUS"));
        assert_eq!(Context::for_unit_test().test_bus(), None);
        let address = "unix:path=/tmp/pitboard-test/bus";
        let read = |os| {
            let pairs = [("HOME", "/home/x"), ("PITBOARD_TEST_BUS", address)];
            Context::read(&Environment::on(os, pairs), "cli")
        };
        assert_eq!(read(Os::Linux).test_bus(), Some(address));
        assert_eq!(read(Os::MacOs).test_bus(), None);
    }

    // It panics before anything there is listed or opened.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows names no Application Support"
    )]
    fn a_scratch_inside_the_real_application_support_panics() {
        let home = {
            let _reaching = crate::host::user::testing::reaching_the_real_home();
            crate::host::user::home().expect("this account has a home")
        };
        let support = home.join("Library/Application Support");
        let named = |folder: PathBuf| {
            std::panic::catch_unwind(move || {
                Context::new("/nowhere".into()).with_scratch_claude(folder)
            })
            .is_err()
        };
        assert!(named(support.join("pitboard-test-never-made")));
        assert!(named(support.clone()));
        assert!(named(home.join("Library")), "it holds it");
        // The home is linked, since Linux has no `Library` for a link to resolve through.
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let link = scratch.path().join("home-link");
        crate::host::fs::testing::link_dir(&home, &link).expect("a link");
        let through = named(link.join("Library/Application Support/pitboard-test-never-made"));
        let _ = std::fs::remove_file(&link);
        assert!(through, "through a link");
        assert!(!named(scratch.path().join("elsewhere")));
        let _guarding = testing::also_guarding(&scratch.path().join("fake-home"));
        assert!(
            named(support.join("pitboard-test-never-made")),
            "while a test guards a fake home too"
        );
    }

    #[test]
    #[cfg(windows)]
    fn a_scratch_in_claude_desktops_windows_folders_panics() {
        let named = |folder: PathBuf| {
            std::panic::catch_unwind(move || {
                Context::new(PathBuf::from(r"C:\nowhere")).with_scratch_claude(folder)
            })
            .is_err()
        };
        let (roaming, local) = crate::host::user::app_data_folders(true);
        assert!(!roaming.is_empty() && !local.is_empty());
        for dir in &roaming {
            assert!(named(dir.join("Claude").join("pitboard-test-never-made")));
            assert!(named(dir.join("claude-3p")), "in any case");
        }
        for dir in &local {
            assert!(named(dir.join("AnthropicClaude")));
        }
        let scratch = tempfile::tempdir().expect("a scratch folder");
        assert!(!named(scratch.path().join("elsewhere")));
        let fake = scratch.path().join("fake-profile");
        let _guarding = testing::also_guarding(&fake);
        let roaming = fake.join("AppData").join("Roaming");
        assert!(named(roaming.join("Claude").join("x")), "a fake profile's");
    }

    // `..` past a folder not made yet is resolved only once something makes that folder.
    #[test]
    fn a_scratch_named_through_a_folder_not_made_yet_panics() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        let _guarding = testing::also_guarding(&home);
        let climbed = std::panic::catch_unwind(|| {
            Context::new("/nowhere".into())
                .with_scratch_claude(home.join("never-made/../Library/Application Support/x"))
        });
        assert!(climbed.is_err());
    }

    // As a helper given the name would write it: from the working directory.
    #[test]
    fn a_relative_name_lies_where_the_working_directory_puts_it() {
        let here = std::env::current_dir().expect("a working directory");
        let name = Path::new("pitboard-never-made");
        assert!(crate::host::lies_in(name, &here));
        assert!(!crate::host::lies_in(name, &here.join("elsewhere")));
    }

    // A relative name lands wherever the test runs.
    #[test]
    fn a_relative_scratch_panics() {
        let named = std::panic::catch_unwind(|| {
            Context::new("/nowhere".into()).with_scratch_claude("pitboard-scratch".into())
        });
        assert!(named.is_err());
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows makes no link"
    )]
    fn a_scratch_whose_data_or_app_links_to_a_real_place_panics() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let root = scratch.path();
        let home = root.join("home");
        let support = home.join("Library/Application Support/Claude");
        let app = home.join("Applications/Claude.app");
        let other = root.join("other");
        for made in [&support, &app, &other] {
            std::fs::create_dir_all(made).expect("a folder to link to");
        }
        let _guarding = testing::also_guarding(&home);
        let linked = |scratch: &str, name: &str, to: &Path| {
            let scratch = root.join(scratch);
            std::fs::create_dir_all(&scratch).expect("a scratch");
            crate::host::fs::testing::link_dir(to, &scratch.join(name)).expect("a link");
            std::panic::catch_unwind(|| {
                Context::new("/nowhere".into()).with_scratch_claude(scratch)
            })
            .is_err()
        };
        let data = linked("one", "data", &support);
        let claude = linked("two", "Claude.app", &app);
        let elsewhere = linked("three", "data", &other);
        let not_made = linked("four", "data", &support.join("never-made"));
        let not_made_relative = linked(
            "five",
            "data",
            Path::new("../home/Library/Application Support/Claude/never-made"),
        );
        assert!(data, "data linked into Application Support");
        assert!(claude, "Claude.app linked to ~/Applications/Claude.app");
        assert!(!elsewhere, "data linked to a folder of the test's own");
        assert!(not_made, "data linked to a folder not made yet there");
        assert!(not_made_relative, "the same, by a relative link");
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows has no `/Applications`"
    )]
    fn a_scratch_of_an_applications_folder_panics() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        let _guarding = testing::also_guarding(&home);
        let named = |folder: PathBuf| {
            std::panic::catch_unwind(move || {
                Context::new("/nowhere".into()).with_scratch_claude(folder)
            })
            .is_err()
        };
        assert!(named("/Applications".into()));
        assert!(named(home.join("Applications")));
        assert!(!named(home.join("Applications/elsewhere")));
    }

    // /System/Volumes/Data/Users is /Users, and `canonicalize` keeps either name as it is.
    #[test]
    #[cfg_attr(
        not(target_os = "macos"),
        ignore = "a firmlink to the Data volume is macOS's own"
    )]
    fn a_scratch_named_through_the_data_volume_panics() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let root = scratch.path();
        let support = root.join("home/Library/Application Support/Claude");
        let scratch = root.join("scratch");
        for made in [&support, &scratch] {
            std::fs::create_dir_all(made).expect("a folder of the test's own");
        }
        let through = |place: &Path| {
            let real = std::fs::canonicalize(place).expect("a folder of the test's own");
            Path::new("/System/Volumes/Data").join(real.strip_prefix("/").expect("from the root"))
        };
        crate::host::fs::testing::link_dir(&through(&support), &scratch.join("data"))
            .expect("a link");
        let _guarding = testing::also_guarding(&root.join("home"));
        let named = |folder: PathBuf| {
            std::panic::catch_unwind(move || {
                Context::new("/nowhere".into()).with_scratch_claude(folder)
            })
            .is_err()
        };
        let linked = named(scratch);
        let inside = named(through(&support).join("pitboard-test"));
        let own = named(through(root).join("own"));
        let applications = named("/System/Volumes/Data/Applications".into());
        assert!(
            linked,
            "data linked into Application Support through the Data volume"
        );
        assert!(
            inside,
            "a scratch in Application Support named through the Data volume"
        );
        assert!(
            !own,
            "a folder of the test's own named through the Data volume"
        );
        assert!(applications, "/Applications named through the Data volume");
    }

    /// An empty `CLAUDE_CONFIG_DIR` is kept, by the builder as by the environment, since
    /// Claude Code takes it as the empty path for its config dir; the context is then
    /// refused as one whose home is not a full path. It was read as unset, which pointed
    /// Pitboard at `~/.claude`, where no Claude Code started with it keeps anything.
    #[test]
    fn an_explicit_context_reads_claude_codes_settings_the_way_the_environment_does() {
        let (_scratch, home, named) = full_home();
        let ctx = Context::new(home.clone())
            .with_claude_config_dir(String::new())
            .with_secure_storage_dir(String::new());
        assert_eq!(
            ctx.pitboard_home,
            crate::host::OS.default_pitboard_home(&home)
        );
        assert_eq!(
            ctx.claude_config_dir.as_deref(),
            Some(""),
            "empty is the empty path for the config dir"
        );
        assert_eq!(
            ctx.secure_storage_dir.as_deref(),
            Some(""),
            "empty is set, and pins the default slot"
        );
        assert_eq!(ctx.claude_program(), Path::new("claude"));
        let env: Environment = [
            (crate::host::OS.home_variable(), named.as_str()),
            ("CLAUDE_CONFIG_DIR", ""),
        ]
        .into_iter()
        .collect();
        let read = Context::for_command_line(&env);
        assert_eq!(read.claude_config_dir, ctx.claude_config_dir);
        for ctx in [ctx, read] {
            assert!(
                matches!(
                    crate::home::check_absolute(&ctx),
                    Err(crate::error::Error::HomeNotAbsolute {
                        variable: "CLAUDE_CONFIG_DIR",
                        ..
                    })
                ),
                "refused"
            );
        }
    }

    #[test]
    fn a_scratch_machine_whose_context_reaches_another_host_or_api_panics() {
        use crate::api::scripted::ScriptedApi;
        use crate::host::memory::MemoryHost;
        let home = Path::new("/home/x");
        let made = ScratchMachine::at(home, |ctx| ctx.with_caller("app".into()));
        assert_eq!(made.ctx.home(), home);
        let another_machine = std::panic::catch_unwind(|| {
            ScratchMachine::at(home, |ctx| ctx.with_memory_stores(MemoryHost::new()))
        });
        assert!(another_machine.is_err());
        let other_services = std::panic::catch_unwind(|| {
            ScratchMachine::at(home, |ctx| ctx.with_scripted_api(ScriptedApi::new()))
        });
        assert!(other_services.is_err());
    }

    // Leaving out either, or giving another, makes it false.
    #[test]
    fn a_context_says_whether_it_reaches_only_the_machine_and_services_it_was_given() {
        use crate::api::scripted::ScriptedApi;
        use crate::host::memory::MemoryHost;
        let (memory, api) = (MemoryHost::new(), ScriptedApi::new());
        let home = PathBuf::from("/home/x");
        let both = Context::new(home.clone())
            .with_memory_stores(Arc::clone(&memory))
            .with_scripted_api(Arc::clone(&api));
        assert!(both.reaches_only(&memory, &api));
        assert!(
            !Context::new(home.clone())
                .with_scripted_api(Arc::clone(&api))
                .reaches_only(&memory, &api),
            "this machine"
        );
        assert!(
            !Context::new(home)
                .with_memory_stores(Arc::clone(&memory))
                .reaches_only(&memory, &api),
            "the network"
        );
        assert!(
            !both.reaches_only(&MemoryHost::new(), &api),
            "another machine"
        );
        assert!(
            !both.reaches_only(&memory, &ScriptedApi::new()),
            "other services"
        );
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
        let (_scratch, home, named) = full_home();
        let env: Environment = [
            (crate::host::OS.home_variable(), named.as_str()),
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
        assert_eq!(
            ctx.pitboard_home,
            crate::host::OS.default_pitboard_home(&home)
        );
        assert_eq!(ctx.caller, "cli");
        let no_path: Environment = [(crate::host::OS.home_variable(), named.as_str())]
            .into_iter()
            .collect();
        assert_eq!(
            Context::for_command_line(&no_path).search_path(),
            "",
            "no PATH is nowhere, not this process's"
        );
    }

    // RF-B4, run 38418961905: a program gets `Path` alone, and a block holding both spellings
    // hands both to a child, whose `var_os` takes the first for either.
    #[test]
    fn a_windows_environment_has_one_value_per_name_in_any_case_and_the_first_wins() {
        let search_path = |os: Os, pairs: &[(&str, &str)]| {
            let home = [("HOME", "/home/x"), ("USERPROFILE", "/home/x")];
            let env = Environment::on(os, home.iter().chain(pairs).copied());
            Context::read(&env, "cli").search_path()
        };
        let windows = |pairs: &[(&str, &str)]| search_path(Os::Windows, pairs);
        assert_eq!(
            windows(&[("Path", "C:\\a")]),
            "C:\\a",
            "as a program gets it"
        );
        assert_eq!(windows(&[("PATH", "C:\\b")]), "C:\\b");
        assert_eq!(windows(&[("Path", "C:\\a"), ("PATH", "C:\\b")]), "C:\\a");
        assert_eq!(windows(&[("PATH", "C:\\b"), ("Path", "C:\\a")]), "C:\\b");
        for os in [Os::MacOs, Os::Linux] {
            assert_eq!(search_path(os, &[("Path", "/a")]), "", "{os:?}");
            assert_eq!(search_path(os, &[("Path", "/a"), ("PATH", "/b")]), "/b");
        }
        let both = Environment::on(Os::Windows, [("Path", "C:\\a"), ("PATH", "C:\\b")]);
        let given: Vec<_> = both.iter().collect();
        assert_eq!(
            given,
            [(OsStr::new("Path"), OsStr::new("C:\\a"))],
            "a child gets one"
        );
        assert_eq!(both.text("PATH"), Some("C:\\a"));
    }

    // Windows names the person in `USERNAME`, and sets no `USER`.
    #[test]
    fn the_person_is_named_by_the_systems_own_variable() {
        let user = |os: Os| {
            let pairs = [
                ("HOME", "/home/x"),
                ("USERPROFILE", "/home/x"),
                ("USER", "posix"),
                ("username", "windows"),
            ];
            Context::read(&Environment::on(os, pairs), "cli").user
        };
        assert_eq!(user(Os::Windows).as_deref(), Some("windows"));
        assert_eq!(user(Os::MacOs).as_deref(), Some("posix"));
        assert_eq!(user(Os::Linux).as_deref(), Some("posix"));
    }

    // Git Bash sets `HOME`, which neither tool takes for the home on Windows.
    #[test]
    fn each_tool_finds_its_home_in_userprofile_on_windows_whatever_home_says() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let profile = scratch.path().to_path_buf();
        let named = profile.to_string_lossy().into_owned();
        let pairs = [("HOME", "/c/Users/x"), ("USERPROFILE", named.as_str())];
        let ctx = Context::read(&Environment::on(Os::Windows, pairs), "cli");
        assert_eq!(ctx.home, profile);
        assert_eq!(
            crate::provider::claude::paths::config_dir(&ctx),
            profile.join(".claude")
        );
        assert_eq!(
            crate::provider::claude::paths::config_file(&ctx),
            profile.join(".claude.json")
        );
        assert_eq!(
            crate::provider::codex::paths::home(&ctx),
            profile.join(".codex")
        );
        assert_eq!(
            ctx.pitboard_home,
            profile.join("AppData").join("Local").join("Pitboard")
        );
        let posix = Context::read(&Environment::on(Os::Linux, pairs), "cli");
        assert_eq!(posix.home, PathBuf::from("/c/Users/x"));
    }

    // XDG Base Directory 0.8 ignores one that is not a full path, as one unset or empty.
    #[test]
    fn a_relative_xdg_folder_is_taken_for_unset() {
        let (_scratch, home, named) = full_home();
        let elsewhere = home.with_file_name("elsewhere");
        let full = elsewhere.to_string_lossy().into_owned();
        let read = |os: Os, pairs: &[(&str, &str)]| {
            let homes = [("HOME", named.as_str()), ("USERPROFILE", named.as_str())];
            Context::read(
                &Environment::on(os, homes.iter().chain(pairs).copied()),
                "cli",
            )
        };
        let given = read(
            Os::Linux,
            &[
                ("XDG_CONFIG_HOME", &full),
                ("XDG_DATA_HOME", "relative/data"),
                ("XDG_CACHE_HOME", ""),
            ],
        );
        assert_eq!(given.config_home(), Some(elsewhere.as_path()));
        let data = home.join(".local").join("share");
        assert_eq!(given.data_home(), Some(data.as_path()), "relative");
        let cache = home.join(".cache");
        assert_eq!(given.cache_home(), Some(cache.as_path()), "empty");
        let unset = read(Os::MacOs, &[]);
        let config = home.join(".config");
        assert_eq!(
            (unset.config_home(), unset.data_home(), unset.cache_home()),
            (
                Some(config.as_path()),
                Some(data.as_path()),
                Some(cache.as_path())
            )
        );
        let windows = read(Os::Windows, &[("XDG_DATA_HOME", &full)]);
        assert_eq!(
            (
                windows.config_home(),
                windows.data_home(),
                windows.cache_home()
            ),
            (None, None, None),
            "Windows has none"
        );
    }

    // On Linux alone: an empty address reads as unset, and a relative runtime folder is ignored.
    #[test]
    fn the_session_bus_is_found_as_sd_bus_finds_it() {
        let (_scratch, home, named) = full_home();
        let runtime = home.with_file_name("runtime");
        let full = runtime.to_string_lossy().into_owned();
        let read = |os: Os, pairs: &[(&str, &str)]| {
            let homes = [("HOME", named.as_str()), ("USERPROFILE", named.as_str())];
            Context::read(
                &Environment::on(os, homes.iter().chain(pairs).copied()),
                "cli",
            )
        };
        let address = "unix:path=/tmp/a%20bus,guid=0123456789abcdef0123456789abcdef";
        let both = [
            ("XDG_RUNTIME_DIR", full.as_str()),
            ("DBUS_SESSION_BUS_ADDRESS", address),
        ];
        let given = read(Os::Linux, &both);
        assert_eq!(given.runtime_dir(), Some(runtime.as_path()));
        assert_eq!(
            given.session_bus(),
            Some(SessionBus::Address(address.into()))
        );
        let in_the_runtime_folder = read(
            Os::Linux,
            &[("XDG_RUNTIME_DIR", &full), ("DBUS_SESSION_BUS_ADDRESS", "")],
        );
        assert_eq!(
            in_the_runtime_folder.session_bus(),
            Some(SessionBus::Socket(runtime.join("bus")))
        );
        let relative = read(Os::Linux, &[("XDG_RUNTIME_DIR", "relative/runtime")]);
        assert_eq!(
            (relative.runtime_dir(), relative.session_bus()),
            (None, None)
        );
        let mac = read(Os::MacOs, &both);
        assert_eq!((mac.runtime_dir(), mac.session_bus()), (None, None));
    }

    // On Linux alone, with a display, and not over SSH, whose forwarded display is another than
    // the one the keyring's prompter shows its dialog on.
    #[test]
    fn the_desktop_session_has_a_display_not_forwarded_over_ssh() {
        let (_scratch, _home, named) = full_home();
        let read = |os: Os, pairs: &[(&str, &str)]| {
            let home = [("HOME", named.as_str())];
            Context::read(
                &Environment::on(os, home.iter().chain(pairs).copied()),
                "cli",
            )
            .in_the_desktop_session()
        };
        assert!(read(Os::Linux, &[("WAYLAND_DISPLAY", "wayland-0")]));
        assert!(read(Os::Linux, &[("DISPLAY", ":0")]));
        assert!(!read(Os::Linux, &[]));
        assert!(!read(
            Os::Linux,
            &[("WAYLAND_DISPLAY", ""), ("DISPLAY", "")]
        ));
        assert!(!read(
            Os::Linux,
            &[
                ("DISPLAY", "localhost:10.0"),
                ("SSH_CONNECTION", "10.0.0.2 50022 10.0.0.1 22")
            ]
        ));
        assert!(!read(Os::MacOs, &[("DISPLAY", ":0")]));
        assert!(!Context::for_unit_test().in_the_desktop_session());
    }

    // A full path on every system, which nothing makes, in a folder of the test's own.
    fn full_home() -> (tempfile::TempDir, PathBuf, String) {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        let named = home.to_string_lossy().into_owned();
        (scratch, home, named)
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
        let _real = crate::host::user::testing::reaching_the_real_home();
        let own = crate::host::user::home().expect("this account has a home");
        let ctx = Context::for_command_line(&Environment::default());
        assert_eq!(ctx.home, own);
        assert_eq!(
            ctx.pitboard_home,
            crate::host::OS.default_pitboard_home(&own)
        );
        let empty: Environment = [(crate::host::OS.home_variable(), "")]
            .into_iter()
            .collect();
        assert_eq!(
            Context::for_command_line(&empty).home,
            PathBuf::new(),
            "set, even empty, it is what the person said"
        );
    }

    /// A unit test's context names one folder for every home, its own, under the temporary
    /// directory, and makes nothing there, so a test that forgets to give a scratch home of
    /// its own reads none of the real ones and writes only there. It took the person's real
    /// home from the passwd database, and every home under it.
    #[test]
    fn a_unit_tests_homes_are_a_folder_of_its_own_that_nothing_makes() {
        let ctx = Context::for_unit_test();
        let sentinel = ctx.home.clone();
        assert!(
            sentinel.starts_with(std::env::temp_dir()),
            "{}",
            sentinel.display()
        );
        assert!(!sentinel.exists(), "{}", sentinel.display());
        assert_eq!(ctx.pitboard_home, sentinel);
        assert_eq!(
            crate::provider::claude::paths::config_dir(&ctx),
            sentinel,
            "Claude Code's"
        );
        assert_eq!(
            crate::provider::codex::paths::home(&ctx),
            sentinel,
            "Codex's"
        );
        assert_eq!(Context::for_unit_test().home, sentinel, "the same test's");
        let another = std::thread::Builder::new()
            .name("another::test".into())
            .spawn(|| Context::for_unit_test().home)
            .expect("a thread")
            .join()
            .expect("its context");
        assert_ne!(another, sentinel, "another test's");
        assert!(another.ends_with("another--test"), "{}", another.display());
    }

    /// The home and Pitboard directory an app asks of the environment it was started with
    /// are the ones the context it is given reads, path for path, so an app that keys records
    /// of its own by Pitboard's directory keys them by the core's.
    #[test]
    fn the_homes_an_app_asks_for_are_the_contexts() {
        let _real = crate::host::user::testing::reaching_the_real_home();
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
