//! Pitboard's core for its native apps, as UniFFI bindings.
//!
//! A call to a `Pitboard` or a `SignIn` is synchronous and may block on the keychain, a lock,
//! the network or the person's login shell, so an app makes it off its main thread. Making a
//! `Pitboard` blocks on none of them. The free functions answer at once from what they are
//! given, apart from `can_run`, which asks the file system about one path,
//! `download_destination`, which asks it whether each name it tries is taken, and
//! `find_command_line`, which looks along a search path and is made off the main thread.
//! Timestamps are epoch seconds.

use pitboard_core::app::AppContext;
use pitboard_core::context::{Context, Environment};
use pitboard_core::provider::ProviderId;
use pitboard_core::service::{self, Changing};
use pitboard_core::{doctor, status, switch, usage, words};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

uniffi::setup_scaffolding!();

mod sites;
pub use sites::{
    Conjunction, LinkRefusal, Site, SiteLink, link_refusal_reason, pitboard_link,
    read_pitboard_link, site_link, site_names, sites, sites_for,
};

mod account_windows;
pub use account_windows::{
    AlertText, Asker, FrameOrigin, NavigationDecision, NavigationPolicy, NavigationRequest,
    NavigationTarget, PagePermission, PageRole, ProcessEnded, ResponseDecision, ResponseFacts,
    SignInWindowSize, SiteMenu, WindowAccount, WindowNoteKind, after_content_process_ended,
    decide_navigation, decide_response, dialog_title, download_destination, download_host,
    download_question, forget_message, frame_asker, is_site_page, opening_note, page_may_close,
    page_may_use, remove_data_alert, sign_in_window_size, site_menus, store_id, window_accounts,
    window_address, window_home, window_note, window_of_store, windows_of,
};

/// Where each tool and Pitboard keep things, said outright, as a test does. The app passes
/// the environment it was started with to [`Pitboard::for_app`] instead; `None` here means
/// the tool's default.
#[derive(Clone, uniffi::Record)]
pub struct Settings {
    pub home: String,
    pub pitboard_home: Option<String>,
    /// `CLAUDE_CONFIG_DIR`; empty means unset.
    pub claude_config_dir: Option<String>,
    /// `CLAUDE_SECURESTORAGE_CONFIG_DIR`; empty is set, and pins the default slot.
    pub secure_storage_dir: Option<String>,
    /// The login name Claude Code files its keychain items under.
    pub user: Option<String>,
    /// The `claude` that runs a sign-in, since `PATH` may not find it.
    pub claude_program: Option<String>,
    /// `CODEX_HOME`; empty means unset.
    pub codex_home: Option<String>,
    /// The `codex` that runs a sign-in, since `PATH` may not find it.
    pub codex_program: Option<String>,
    /// Where a tool's program is looked for, in `PATH`'s form, and what its sign-in is given
    /// as `PATH`, behind the program's own directory where that is not on it: the person's
    /// login shell's, which an app does not inherit. `None` is this process's own `PATH`.
    #[uniffi(default)]
    pub search_path: Option<String>,
    /// The command line the daily renewal schedule runs: the one the app comes with, since
    /// the app itself is not one. `None` where the app has none, as in a build run from a
    /// build directory, and then there is nothing to schedule.
    #[uniffi(default)]
    pub schedule_program: Option<String>,
    /// `PITBOARD_NO_ARGV=1`: refuse to write a login on the argument line, as the command
    /// line does when its environment says so.
    #[uniffi(default = false)]
    pub no_argv: bool,
    /// `PITBOARD_CLAUDE_DESKTOP_DIR`: where Claude Desktop keeps its data; `None` or empty
    /// means its own default.
    #[uniffi(default)]
    pub desktop_dir: Option<String>,
    /// `PITBOARD_CLAUDE_DESKTOP_APP`: the Claude app bundle; `None` or empty means the one in
    /// /Applications.
    #[uniffi(default)]
    pub desktop_app: Option<String>,
}

impl Settings {
    fn context(self) -> Context {
        let mut ctx = Context::new(PathBuf::from(self.home));
        if let Some(dir) = self.pitboard_home {
            ctx = ctx.with_pitboard_home(PathBuf::from(dir));
        }
        if let Some(dir) = self.claude_config_dir {
            ctx = ctx.with_claude_config_dir(dir);
        }
        if let Some(dir) = self.secure_storage_dir {
            ctx = ctx.with_secure_storage_dir(dir);
        }
        if let Some(user) = self.user {
            ctx = ctx.with_user(user);
        }
        if let Some(program) = self.claude_program {
            ctx = ctx.with_claude_program(PathBuf::from(program));
        }
        if let Some(dir) = self.codex_home {
            ctx = ctx.with_codex_home(dir);
        }
        if let Some(program) = self.codex_program {
            ctx = ctx.with_codex_program(PathBuf::from(program));
        }
        if let Some(path) = self.search_path {
            ctx = ctx.with_search_path(path);
        }
        if let Some(program) = self.schedule_program {
            ctx = ctx.with_schedule_program(PathBuf::from(program));
        }
        if self.no_argv {
            ctx = ctx.with_argv_fallback(false);
        }
        if let Some(dir) = self.desktop_dir {
            ctx = ctx.with_desktop_dir(dir);
        }
        if let Some(app) = self.desktop_app {
            ctx = ctx.with_desktop_app(app);
        }
        // These bindings exist for the app, so a change made through them says so.
        ctx.with_caller("app".into())
    }
}

/// A tool Pitboard handles, as the app names it to a person.
#[derive(Debug, uniffi::Record)]
pub struct Tool {
    /// What a label's prefix and every `provider` field say: `claude`, `codex`, `desktop`.
    pub code: String,
    /// As its own documentation names it: `Claude Code`, `Codex`, `Claude Desktop`.
    pub name: String,
    /// The command that runs it, which is what a person restarts.
    pub program: String,
    /// The company behind it, which is who is asked about its accounts.
    pub service: String,
}

/// Every tool Pitboard handles, in the order a listing shows them.
#[uniffi::export]
pub fn tools() -> Vec<Tool> {
    ProviderId::ALL.iter().copied().map(tool).collect()
}

fn tool(tool: ProviderId) -> Tool {
    Tool {
        code: tool.code().into(),
        name: tool.name().into(),
        program: tool.program().into(),
        service: tool.service().into(),
    }
}

/// The `pitboard` a terminal would run.
#[derive(Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FoundCommandLine {
    /// The app's own, at this path or linked to from it.
    Bundled { path: String },
    /// Another install, at this path.
    Another { path: String },
    /// None anywhere a terminal would look.
    Nowhere,
}

/// Where each way of installing Pitboard puts `pitboard` under `home`, and where the
/// system's package managers put programs, looked in after the login shell's `PATH`.
#[uniffi::export]
pub fn command_line_places(home: String) -> Vec<String> {
    pitboard_core::app::command_line_places(Path::new(&home))
        .iter()
        .map(|place| place.to_string_lossy().into_owned())
        .collect()
}

/// What a tool's sign-in has printed so far comes to, for the sheet that shows it running.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct SignInView {
    /// The address the tool printed for a person to open when the browser did not open by
    /// itself, as printed, to make a link of.
    pub url: Option<String>,
    /// Whether to offer a field for the code the browser shows, which the tool is waiting
    /// to have typed back.
    pub wants_code: bool,
}

/// What a sign-in of `provider`, a `Tool`'s `code`, has printed so far, `said`, comes to.
/// `pasted` is whether a code has been typed back already, after which none is asked for.
///
/// Each tool's own module reads its own words, so both apps show the same. It reads only
/// what it is given and answers at once, on any thread. A provider nobody knows offers
/// nothing.
#[uniffi::export]
pub fn sign_in_view(provider: String, said: String, pasted: bool) -> SignInView {
    let read = pitboard_core::provider::ProviderId::parse(&provider)
        .map(|tool| pitboard_core::provider::sign_in_view(tool, &said, pasted))
        .unwrap_or_default();
    SignInView {
        url: read.url,
        wants_code: read.wants_code,
    }
}

/// The first `pitboard` a terminal would run: on `search_path`, in `PATH`'s form, then in
/// `places`; and whether it is the command line at `helper`, the app's own, once every link
/// is followed. Looks at the file system, so call it off the main thread.
#[uniffi::export]
pub fn find_command_line(
    search_path: Option<String>,
    places: Vec<String>,
    helper: Option<String>,
) -> FoundCommandLine {
    let places: Vec<PathBuf> = places.into_iter().map(PathBuf::from).collect();
    let shown = |path: PathBuf| path.to_string_lossy().into_owned();
    match pitboard_core::app::find_command_line(
        search_path.as_deref().map(std::ffi::OsStr::new),
        &places,
        helper.as_deref().map(Path::new),
    ) {
        pitboard_core::app::CommandLine::Bundled(path) => {
            FoundCommandLine::Bundled { path: shown(path) }
        }
        pitboard_core::app::CommandLine::Another(path) => {
            FoundCommandLine::Another { path: shown(path) }
        }
        pitboard_core::app::CommandLine::Nowhere => FoundCommandLine::Nowhere,
    }
}

/// The command line the app at `app` comes with, which its renewal schedule runs. `None` for
/// anything that is not an app, such as a test or a build directory.
#[uniffi::export]
pub fn app_command_line(app: String) -> Option<String> {
    pitboard_core::app::app_command_line(Path::new(&app))
        .map(|path| path.to_string_lossy().into_owned())
}

/// Whether `path` is a program this user may run, as the core judges every program it finds:
/// a regular file, once every link is followed, that this user may execute. Asks the file
/// system about that one path.
#[uniffi::export]
pub fn can_run(path: String) -> bool {
    pitboard_core::app::can_run(Path::new(&path))
}

/// The home an app started with `environment` has, as the core reads it: `HOME`, or this
/// account's own where it is unset.
#[uniffi::export]
pub fn home_directory(environment: HashMap<String, String>) -> String {
    let environment: Environment = environment.into_iter().collect();
    environment.home().to_string_lossy().into_owned()
}

/// The Pitboard directory of an app started with `environment`, as the core reads it:
/// `PITBOARD_HOME`, or `.pitboard` in the home. The path as the environment gives it, with
/// any `.`, `..` or trailing `/` it holds.
#[uniffi::export]
pub fn pitboard_directory(environment: HashMap<String, String>) -> String {
    let environment: Environment = environment.into_iter().collect();
    environment.pitboard_home().to_string_lossy().into_owned()
}

/// Something to know about that did not stop the operation.
#[derive(Debug, uniffi::Record)]
pub struct Warning {
    pub code: String,
    pub message: String,
}

fn warnings(found: &[service::Warning]) -> Vec<Warning> {
    found
        .iter()
        .map(|w| Warning {
            code: w.code().to_string(),
            message: w.to_string(),
        })
        .collect()
}

/// One change Pitboard made, as `pitboard log` shows them.
#[derive(Debug, uniffi::Record)]
pub struct Change {
    /// Local time, as the log records it.
    pub at: String,
    /// Which front end asked: `app`, `cli`, or `unknown` for a line written before this
    /// was recorded.
    pub caller: String,
    pub verb: String,
    pub subject: String,
    /// `ok`, or the code of whatever stopped it.
    pub outcome: String,
}

/// An interrupted switch that was given up on, keeping every login it named.
#[derive(Debug, uniffi::Record)]
pub struct Abandoned {
    pub from: String,
    pub to: String,
    /// Copies kept rather than deleted, because which one is live is now unknown.
    pub logins_kept: u32,
}

/// What renewing every due parked login came to.
#[derive(Debug, uniffi::Record)]
pub struct Renewed {
    /// As a person types it: bare for Claude Code, `codex/work` for Codex.
    pub label: String,
    /// Which tool's login it is.
    pub provider: String,
    /// `renewed`, `renewal_deferred`, `parked_login_refused`, `not_renewable`, or the code
    /// of a failure.
    pub outcome: String,
    /// For a login Pitboard cannot renew, such as Claude Desktop's, when its sign-in lapses.
    #[uniffi(default)]
    pub expires_at: Option<i64>,
}

/// Whether anything keeps parked logins alive on this machine without a command being run.
#[derive(Debug, uniffi::Enum)]
pub enum Schedule {
    /// The platform's own scheduler runs `pitboard renew` every `every_seconds`.
    Installed { path: String, every_seconds: u32 },
    /// Nothing does. Parked logins are renewed when Pitboard runs, and otherwise not.
    Absent,
    /// This platform has no scheduler Pitboard knows how to write.
    Unsupported,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum PitboardError {
    /// `code` is stable, for the app to branch on; `message` names the cause and what to do.
    /// `cause` is what went wrong underneath, where Anthropic was asked, and is what decides
    /// whether another try is worth offering. `warnings` are what was found on the way,
    /// reported even though the operation failed.
    #[error("{message}")]
    Failed {
        code: String,
        cause: Option<Cause>,
        message: String,
        warnings: Vec<Warning>,
    },
}

/// Why a request to Anthropic did not produce an answer Pitboard could use.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Cause {
    /// Stable, for the app to branch on.
    pub code: String,
    /// Whether the same request, later, could answer differently.
    pub worth_retrying: bool,
}

impl From<pitboard_core::error::Error> for PitboardError {
    fn from(error: pitboard_core::error::Error) -> Self {
        PitboardError::Failed {
            code: error.code().to_string(),
            cause: cause(&error),
            message: error.to_string(),
            warnings: Vec::new(),
        }
    }
}

impl From<service::Failed> for PitboardError {
    fn from(failed: service::Failed) -> Self {
        PitboardError::Failed {
            code: failed.error.code().to_string(),
            cause: cause(&failed.error),
            message: failed.error.to_string(),
            warnings: warnings(&failed.warnings),
        }
    }
}

/// What went wrong underneath, where Anthropic was asked. The app decides whether to offer
/// another try from this rather than from the wording of a message.
fn cause(error: &pitboard_core::error::Error) -> Option<Cause> {
    error.cause().map(|c| Cause {
        code: c.code().to_string(),
        worth_retrying: c.worth_retrying(),
    })
}

#[derive(uniffi::Enum)]
pub enum Source {
    Live,
    ClaudeCodeCache,
    Remembered,
    /// From Claude Desktop's own record of its plan usage, without asking claude.ai.
    DesktopHistory,
}

/// One limit an account is measured against, such as the five-hour session or the week.
/// Named for what it is rather than `Window`, which SwiftUI and WinUI each have a type of.
#[derive(uniffi::Record)]
pub struct Limit {
    /// The service's own name for it: Anthropic's `session`, `weekly_all` or
    /// `weekly_scoped`, or one named after its length for OpenAI.
    pub kind: String,
    /// How long the window runs, where that is known. The way to name a window to a person
    /// whatever its service called it.
    pub length_seconds: Option<i64>,
    /// The model a scoped limit applies to.
    pub scope: Option<String>,
    /// Share already used; past 100 once exceeded.
    pub percent: f64,
    pub resets_at: Option<i64>,
    /// How Anthropic grades this row, when it grades it.
    pub severity: Option<String>,
    /// Whether this limit is one the account is working against now.
    pub is_active: bool,
}

#[derive(uniffi::Record)]
pub struct Usage {
    pub source: Source,
    pub observed_at: Option<i64>,
    pub windows: Vec<Limit>,
    /// False for a reading whose meaning is not yet confirmed, which a person should be told
    /// is unconfirmed. Claude Desktop's own history was measured on 4 October 2026, so it is
    /// verified; a source whose register entry is unverified is not.
    #[uniffi(default = true)]
    pub verified: bool,
}

/// Whether Claude Desktop's usage is asked of claude.ai, which needs Claude's key.
#[derive(Debug, uniffi::Record)]
pub struct LiveUsageState {
    pub enabled: bool,
    /// `unknown`, `granted` or `needs_approval`: whether macOS lets Pitboard read the key.
    pub approval: String,
    /// Why it is not working, as a stable code: `no_gui`, `denied`, `auth_failed`,
    /// `timed_out`, `item_changed`, `item_missing`, `key_does_not_decrypt`,
    /// `no_session`, `other`.
    pub reason: Option<String>,
    /// When claude.ai last answered.
    pub last_ok_at: Option<i64>,
}

impl From<status::LiveUsage> for LiveUsageState {
    fn from(state: status::LiveUsage) -> LiveUsageState {
        LiveUsageState {
            enabled: state.enabled,
            approval: match state.approval {
                status::Approval::Unknown => "unknown",
                status::Approval::Granted => "granted",
                status::Approval::NeedsApproval => "needs_approval",
            }
            .into(),
            reason: state.reason,
            last_ok_at: state.last_ok_at,
        }
    }
}

/// A sign-out Pitboard made that no enrolment has followed yet: the app is signed out and
/// waits for somebody to sign in to the next account.
#[derive(Debug, uniffi::Record)]
pub struct Awaiting {
    /// The account that was parked, as its label; `None` where it is not known.
    pub from_label: Option<String>,
    pub started_at: i64,
}

#[derive(uniffi::Record)]
pub struct Parked {
    pub parked_at: i64,
    pub access_expires_at: Option<i64>,
    pub refresh_expires_at: Option<i64>,
}

#[derive(uniffi::Record)]
pub struct Account {
    /// Unique among the accounts of one status, and stable between two: the tool and the
    /// account, or the tool alone for a login that belongs to no account Pitboard can name.
    /// Two tools' accounts can share a label, so a label cannot be an identity.
    pub id: String,
    /// Which tool the account is for, as a `Tool`'s `code`.
    pub provider: String,
    /// `None` for an account signed in but not enrolled.
    pub label: Option<String>,
    /// The label with its tool, `claude/work` or `codex/work`: what to pass back to switch
    /// to, forget or rename it, which names exactly one account whatever else is enrolled.
    /// `None` exactly when `label` is.
    pub qualified: Option<String>,
    /// A login of this tool is there and belongs to no account Pitboard can name: one it
    /// could not read, or one it cannot switch, such as an API key. Not an account to enrol.
    pub unplaced: bool,
    pub email: String,
    pub account_uuid: String,
    pub signed_in: bool,
    /// Whether switching to it would work now.
    pub switchable: bool,
    pub parked: Option<Parked>,
    pub usage: Option<Usage>,
    /// Why the usage is not live, when it is not.
    pub stale: Option<String>,
    /// What to tell a person about `stale`, when it is worth a word.
    pub stale_explanation: Option<String>,
    /// How long this account lasts, in seconds: until its tightest limit fills at the rate
    /// it has been filling, or until that limit resets, whichever comes first.
    ///
    /// `None` until there is enough to go on. A wrong runway tells somebody to switch when
    /// they need not, which is worse than none.
    pub lasts_seconds: Option<i64>,
    /// Whether `lasts_seconds` is a limit filling or a limit resetting, which is the
    /// difference between "about an hour left" and "whole again in an hour".
    pub lasts_burning: bool,
}

#[derive(uniffi::Record)]
pub struct Status {
    pub now: i64,
    /// The signed-in account first.
    pub accounts: Vec<Account>,
    pub warnings: Vec<Warning>,
}

/// When a session of the tool that is already running picks a switch up.
#[derive(uniffi::Enum)]
pub enum Adoption {
    /// On its own, within this many seconds.
    Follows { within_seconds: u32 },
    /// Never: `program` has to be quit and started again.
    Restart { program: String },
    /// When `program` is next opened: it was quit for the switch and reads the login as it
    /// starts, so there is nothing running to restart.
    NextLaunch { program: String },
}

/// What makes one kind of process take a switch.
#[derive(uniffi::Enum)]
pub enum Remedy {
    /// Quit it and start it again.
    Restart,
    /// Quit the app the way Command-Q does, and open it again. This app may do both.
    ReopenApp { bundle_id: String, name: String },
    /// Run this command.
    Run { command: String },
    /// Do this, somewhere Pitboard cannot reach.
    Do { instruction: String },
}

/// One kind of process running a tool with a login in memory.
#[derive(uniffi::Record)]
pub struct Holding {
    /// Stable, in snake case, for code to tell kinds apart by: `chatgpt_app`, `session`.
    pub kind: String,
    /// As a sentence names what is running: "the ChatGPT app", "2 `codex` sessions".
    pub phrase: String,
    pub pids: Vec<u32>,
    pub remedy: Remedy,
}

impl From<pitboard_core::holder::Holding> for Holding {
    fn from(held: pitboard_core::holder::Holding) -> Holding {
        use pitboard_core::holder::Remedy as Core;
        Holding {
            kind: held.holder.kind.into(),
            phrase: held.phrase(),
            remedy: match held.holder.remedy {
                Core::Restart => Remedy::Restart,
                Core::ReopenApp { bundle_id, name } => Remedy::ReopenApp {
                    bundle_id: bundle_id.into(),
                    name: name.into(),
                },
                Core::Run(command) => Remedy::Run {
                    command: command.into(),
                },
                Core::Do(instruction) => Remedy::Do {
                    instruction: instruction.into(),
                },
            },
            pids: held.pids,
        }
    }
}

/// When sessions already running follow a switch, as the app is told it.
fn adoption_of(adoption: pitboard_core::provider::Adoption) -> Adoption {
    match adoption {
        pitboard_core::provider::Adoption::PollingWithin(seconds) => Adoption::Follows {
            within_seconds: seconds,
        },
        pitboard_core::provider::Adoption::RestartRequired { program, .. } => Adoption::Restart {
            program: program.into(),
        },
        pitboard_core::provider::Adoption::NextLaunch { program } => Adoption::NextLaunch {
            program: program.into(),
        },
    }
}

#[derive(uniffi::Enum)]
pub enum Switch {
    Switched {
        /// Which tool's login moved.
        provider: String,
        from: String,
        to: String,
        adoption: Adoption,
    },
    AlreadyActive {
        label: String,
    },
}

#[derive(uniffi::Record)]
pub struct Switched {
    pub outcome: Switch,
    pub warnings: Vec<Warning>,
}

#[derive(uniffi::Enum)]
pub enum EnrolledAs {
    /// The account signed in now.
    Current,
    /// Another account, signed in privately and parked.
    SignedIn,
    /// An enrolled account's parked login, renewed.
    Renewed,
    /// The account signed in now, signed in to: its new login is the one in use now.
    /// `again` when it was enrolled already, and not when this sign-in enrolled it.
    InUse { again: bool },
}

/// What enrolling an account came to. Its outcome is not named `enrolled`: C# gives a
/// record's fields to its members, and a member may not share its record's name.
#[derive(uniffi::Record)]
pub struct Enrolled {
    pub email: String,
    pub outcome: EnrolledAs,
    pub warnings: Vec<Warning>,
}

fn enrolled(enrolled: switch::Enrolled, warnings: Vec<Warning>) -> Enrolled {
    let (email, outcome) = match enrolled {
        switch::Enrolled::Current { email } => (email, EnrolledAs::Current),
        switch::Enrolled::SignedIn { email } => (email, EnrolledAs::SignedIn),
        switch::Enrolled::Renewed { email } => (email, EnrolledAs::Renewed),
        switch::Enrolled::InUse { email, again } => (email, EnrolledAs::InUse { again }),
    };
    Enrolled {
        email,
        outcome,
        warnings,
    }
}

/// The email of the account a change was made to.
#[derive(uniffi::Record)]
pub struct Changed {
    pub email: String,
    pub warnings: Vec<Warning>,
}

#[derive(uniffi::Enum)]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

#[derive(uniffi::Record)]
pub struct Check {
    pub code: String,
    pub name: String,
    pub level: Level,
    pub detail: String,
    /// Empty when there is nothing to do.
    pub advice: String,
}

#[derive(uniffi::Record)]
pub struct Diagnosis {
    pub checks: Vec<Check>,
    /// No check failed.
    pub healthy: bool,
}

// What the apps show as the core says or decides it, as free functions of the records the
// apps hold: the sentences, column words and usage level of `pitboard_core::words`, and
// `usage::same_reset`, the rule merging readings follows. Each wraps the core's function of
// the same name. The command line calls those directly wherever it says the same thing, and
// never these. None reads a clock, a file or the keychain, so a view may call one on the
// main thread as it draws.

/// A limit in the column form, beside its bar: "5h", "week", "30m", "week · Fable".
#[uniffi::export]
pub fn limit_column(limit: Limit) -> String {
    words::limit_column(&limit.kind, limit.length_seconds, limit.scope.as_deref())
}

/// A limit in the sentence form, without its scope: "5-hour", "weekly", "daily".
#[uniffi::export]
pub fn limit_name(limit: Limit) -> String {
    words::limit_name(&limit.kind, limit.length_seconds)
}

/// How much of a limit is used, in the three steps its colour changes at.
#[derive(Debug, PartialEq, uniffi::Enum)]
pub enum UsageLevel {
    /// Under 70%.
    Plenty,
    /// From 70%.
    Low,
    /// From 90%, and past 100%.
    Out,
}

/// The step a limit is at, from a `Limit`'s `percent`, which passes 100 when a service
/// reports more used than the limit.
#[uniffi::export]
pub fn usage_level(percent: f64) -> UsageLevel {
    match words::usage_level(percent) {
        words::UsageLevel::Plenty => UsageLevel::Plenty,
        words::UsageLevel::Low => UsageLevel::Low,
        words::UsageLevel::Out => UsageLevel::Out,
    }
}

/// When a limit resets, as of `now`: "resets in 2h 05m", or "resetting now" once it is due.
#[uniffi::export]
pub fn resets(resets_at: i64, now: i64) -> String {
    words::resets(resets_at, now)
}

/// How long an account lasts, from an `Account`'s `lasts_seconds` and `lasts_burning`:
/// "about 1h 30m left at this rate", "resets in 1h 30m", and under a minute "about to run
/// out" or "resets any moment". `None` where `lasts_seconds` is, until there is enough to go
/// on.
#[uniffi::export]
pub fn runway(seconds: Option<i64>, burning: bool) -> Option<String> {
    use pitboard_core::history::Runway;
    words::runway(match seconds {
        None => Runway::Unknown,
        Some(seconds) if burning => Runway::Burning(seconds),
        Some(seconds) => Runway::Resting(seconds),
    })
}

/// How long a parked login stays usable, as of `now`, in the sentence form: "Parked login
/// good for 20 more days". `None` when nothing says.
#[uniffi::export]
pub fn parked_life(parked: Option<Parked>, now: i64) -> Option<String> {
    words::parked_life(parked?.refresh_expires_at, now)
}

/// What a renewal run did, from what `Pitboard::renew` returned: "No parked login was due.",
/// "Renewed one.", "Renewed 1 of 2; the rest are tried again next time.".
///
/// A login nothing can renew, such as Claude Desktop's, is never due, as the command line
/// counts it, and its `expires_at` says when it lapses.
#[uniffi::export]
pub fn renewal_note(renewals: Vec<Renewed>) -> String {
    let not_renewable = switch::Renewal::NotRenewable {
        label: String::new(),
        expires_at: None,
    }
    .code();
    let due = renewals
        .iter()
        .filter(|r| r.outcome != not_renewable)
        .count();
    let renewed = renewals
        .iter()
        .filter(|r| r.outcome == switch::Renewal::Renewed.code())
        .count();
    words::renewal_note(due, renewed)
}

/// The line over a diagnosis's checks: what is worth looking at while checks only warn,
/// and not to switch accounts while one fails.
#[uniffi::export]
pub fn doctor_summary(checks: Vec<Check>) -> String {
    words::doctor_summary(checks.iter().map(|c| match c.level {
        Level::Ok => doctor::Level::Ok,
        Level::Warn => doctor::Level::Warn,
        Level::Fail => doctor::Level::Fail,
    }))
}

/// Whether two resets of a limit are one, as the core counts them when it merges readings:
/// less than a minute apart, in either order. A session is given a reset in whole seconds
/// and Anthropic's answer a fraction that is dropped, so one window can come back a second
/// apart.
#[uniffi::export]
pub fn same_reset(between: i64, and: i64) -> bool {
    usage::same_reset(between, and)
}

fn account(row: status::Row, now: i64) -> Account {
    let key = row.key();
    let unplaced = row.unplaced();
    Account {
        id: if row.account_uuid.is_empty() {
            format!("{}:login", row.provider.code())
        } else {
            format!("{}:{}", row.provider.code(), row.account_uuid)
        },
        provider: row.provider.code().into(),
        qualified: key.map(|k| k.qualified()),
        unplaced,
        switchable: row.switchable(now),
        lasts_seconds: row.runway.seconds(),
        lasts_burning: matches!(row.runway, pitboard_core::history::Runway::Burning(_)),
        stale: row.stale.map(|s| s.code().to_string()),
        // In the row's own tool's words: a Codex row is not about Anthropic.
        stale_explanation: row.explanation().map(str::to_owned),
        parked: row.parked.map(|p| Parked {
            parked_at: p.parked_at,
            access_expires_at: p.access_expires_at,
            refresh_expires_at: p.refresh_expires_at,
        }),
        usage: row.usage.map(|u| Usage {
            source: match u.source {
                usage::Source::Live => Source::Live,
                usage::Source::ClaudeCodeCache => Source::ClaudeCodeCache,
                usage::Source::Remembered => Source::Remembered,
                usage::Source::DesktopHistory => Source::DesktopHistory,
            },
            observed_at: u.observed_at,
            verified: u.verified,
            windows: u
                .windows
                .into_iter()
                .map(|w| Limit {
                    length_seconds: w.length_seconds,
                    kind: w.kind,
                    scope: w.scope,
                    percent: w.percent,
                    resets_at: w.resets_at,
                    severity: w.severity,
                    is_active: w.is_active,
                })
                .collect(),
        }),
        label: row.label,
        email: row.email,
        account_uuid: row.account_uuid,
        signed_in: row.signed_in,
    }
}

/// A switch as the app reads it.
fn switched(outcome: Changing<switch::Outcome>) -> Result<Switched, PitboardError> {
    changed(outcome, |outcome, warnings| Switched {
        outcome: match outcome {
            switch::Outcome::Switched {
                provider,
                from,
                to,
                adoption,
                ..
            } => Switch::Switched {
                provider: provider.code().into(),
                from,
                to,
                adoption: adoption_of(adoption),
            },
            // Said as a switch from nobody, an empty side the way a recovered switch
            // names one, so the app reads it without a case of its own.
            switch::Outcome::Installed {
                provider,
                to,
                adoption,
            } => Switch::Switched {
                provider: provider.code().into(),
                from: String::new(),
                to,
                adoption: adoption_of(adoption),
            },
            // Only `switch_to_signed_out` signs out, and it is said as a switch to
            // nobody: an empty side, the way a recovered switch names one.
            switch::Outcome::SignedOut {
                provider,
                from,
                adoption,
                ..
            } => Switch::Switched {
                provider: provider.code().into(),
                from,
                to: String::new(),
                adoption: adoption_of(adoption),
            },
            switch::Outcome::AlreadyActive { label } => Switch::AlreadyActive { label },
            // Nothing changed, said the way a sign-out that changed something is: an empty
            // side for nobody, so the app reads it without a case of its own.
            switch::Outcome::AlreadySignedOut { .. } => Switch::AlreadyActive {
                label: String::new(),
            },
        },
        warnings,
    })
}

fn changed<T, R>(
    outcome: Changing<T>,
    make: impl FnOnce(T, Vec<Warning>) -> R,
) -> Result<R, PitboardError> {
    let done = outcome?;
    Ok(make(done.value, warnings(&done.warnings)))
}

/// A sign-in in progress: the tool's own, running with its output piped here because an
/// app has no terminal to hand it. Both tools open the browser themselves and finish
/// through a loopback callback. Claude Code reads stdin only for a fallback code to paste;
/// Codex prints the address to open when its browser cannot, and reads nothing.
#[derive(uniffi::Object)]
pub struct SignIn {
    watched: Mutex<Option<switch::WatchedSignIn>>,
    /// Apart from `watched`: reading waits on the tool, and Codex says nothing between its
    /// address and the browser coming back, so a read that held `watched` kept a cancel or
    /// a paste waiting until then, and the app's main thread with it.
    said: switch::Said,
    label: String,
    provider: pitboard_core::provider::ProviderId,
    /// The core the sign-in started with, which enrols what it signed in to: one made again
    /// meanwhile, from a login shell that answered late, may look elsewhere for programs,
    /// but keeps its accounts in the same place.
    made: Arc<Made>,
}

#[uniffi::export]
impl SignIn {
    /// Which tool's sign-in this is, as a `Tool`'s `code`.
    pub fn provider(&self) -> String {
        self.provider.code().into()
    }

    /// The next thing the tool said, or nothing once it has stopped saying anything.
    /// Blocks, so call it off the main thread. `sign_in_view` reads what it all comes to.
    pub fn next_line(&self) -> Option<String> {
        self.said.next()
    }

    /// Types back the code the browser showed after signing in.
    pub fn paste(&self, line: String) -> Result<(), PitboardError> {
        let mut held = self.watched.lock().map_err(|_| PitboardError::Failed {
            code: "sign_in_gone".into(),
            cause: None,
            message: "this sign-in is no longer running".into(),
            warnings: Vec::new(),
        })?;
        match held.as_mut() {
            Some(watched) => watched.paste(&line).map_err(PitboardError::from),
            None => Ok(()),
        }
    }

    /// Waits for it to finish, then enrols what it signed in to.
    pub fn finish(&self) -> Result<Enrolled, PitboardError> {
        let watched = self
            .watched
            .lock()
            .ok()
            .and_then(|mut held| held.take())
            .ok_or_else(|| PitboardError::Failed {
                code: "sign_in_gone".into(),
                cause: None,
                message: "this sign-in is no longer running".into(),
                warnings: Vec::new(),
            })?;
        let login = watched.finish()?;
        changed(
            self.made.core.enroll_signed_in(&self.label, login),
            enrolled,
        )
    }

    /// Stops it. Whatever it wrote is discarded.
    pub fn cancel(&self) {
        if let Ok(mut held) = self.watched.lock()
            && let Some(watched) = held.take()
        {
            watched.cancel();
        }
    }
}

/// What the app's core is made of. Made once, on first use, and kept: making it can mean
/// asking the person's login shell, which takes up to five seconds.
struct Made {
    core: service::Pitboard,
    /// The tools a program was named or found for, which is what `installed` answers.
    found: Vec<ProviderId>,
    /// The login shell's `PATH` as far as it was looked in, which `search_path` answers.
    search_path: Option<String>,
    /// Whether the app named a command line for the schedule to run. The core schedules
    /// the program asking where none is named, and that is the app, which renews nothing.
    schedules_a_command_line: bool,
}

impl Made {
    fn from_settings(settings: Settings) -> Made {
        let (claude, codex) = (
            settings.claude_program.is_some(),
            settings.codex_program.is_some(),
        );
        let search_path = settings.search_path.clone();
        let schedules_a_command_line = settings.schedule_program.is_some();
        let context = settings.context();
        Made {
            found: ProviderId::ALL
                .iter()
                .copied()
                .filter(|&tool| match tool {
                    ProviderId::Claude => claude,
                    ProviderId::Codex => codex,
                    // The Claude app counts only where its bundle has its program, the one
                    // named or the one in /Applications: a bundle that is not there is not
                    // an installed app.
                    ProviderId::Desktop => pitboard_core::app::can_run(context.program_for(tool)),
                    _ => false,
                })
                .collect(),
            search_path,
            schedules_a_command_line,
            core: service::Pitboard::new(context),
        }
    }

    fn from_app(found: AppContext) -> Made {
        Made {
            schedules_a_command_line: found.context.schedule_program().is_some(),
            found: found.found,
            search_path: found.search_path,
            core: service::Pitboard::new(found.context),
        }
    }
}

/// How long after a login shell too slow to answer it is asked once more.
const ASK_AGAIN_AFTER: Duration = Duration::from_secs(60);

/// A `Made` made the first time it is asked for, by whichever thread asks first. Any other
/// thread asking meanwhile waits for that one rather than making a second.
///
/// One made from a login shell that answered too late is made once more when asked to, a
/// while later, and kept in its place if that answers: startup files are slowest while the
/// machine is busy logging in, which is when an app that opens at login first asks. Once
/// more and no more, because a shell that is always that slow would otherwise cost its
/// five seconds every time. A caller asking meanwhile gets what was made first rather than
/// waiting on the second ask.
struct Kept {
    make: Box<dyn Fn() -> (Made, bool) + Send + Sync>,
    again_after: Duration,
    held: Mutex<Option<Held>>,
}

struct Held {
    made: Arc<Made>,
    /// What it was made from came too late.
    late: bool,
    at: Instant,
    asked_again: bool,
}

impl Kept {
    fn held(&self) -> MutexGuard<'_, Option<Held>> {
        // A panic while making leaves nothing made, which the next ask makes again.
        self.held
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn value(&self) -> Arc<Made> {
        let mut held = self.held();
        if let Some(held) = held.as_ref() {
            return held.made.clone();
        }
        let (made, late) = (self.make)();
        let made = Arc::new(made);
        *held = Some(Held {
            made: made.clone(),
            late,
            at: Instant::now(),
            asked_again: false,
        });
        made
    }

    /// The value, made once more first when what it was made from came too late and at
    /// least `again_after` has passed since.
    fn value_asking_again(&self) -> Arc<Made> {
        let first = self.value();
        let due = match self.held().as_mut() {
            Some(held)
                if held.late && !held.asked_again && held.at.elapsed() >= self.again_after =>
            {
                held.asked_again = true;
                true
            }
            _ => false,
        };
        if !due {
            return self.value();
        }
        let (again, late) = (self.make)();
        if late {
            return first;
        }
        let again = Arc::new(again);
        *self.held() = Some(Held {
            made: again.clone(),
            late: false,
            at: Instant::now(),
            asked_again: true,
        });
        again
    }
}

#[derive(uniffi::Object)]
pub struct Pitboard {
    made: Kept,
}

impl Pitboard {
    /// A core made by `make` on first use, which also says whether the login shell it asked
    /// was too slow to answer.
    fn asking(
        make: impl Fn() -> (Made, bool) + Send + Sync + 'static,
        again_after: Duration,
    ) -> Pitboard {
        Pitboard {
            made: Kept {
                make: Box::new(make),
                again_after,
                held: Mutex::new(None),
            },
        }
    }

    fn core(&self) -> Arc<Made> {
        self.made.value()
    }
}

#[uniffi::export]
impl Pitboard {
    /// A core over what `settings` says, made on first use.
    #[uniffi::constructor]
    pub fn new(settings: Settings) -> Arc<Self> {
        Arc::new(Pitboard::asking(
            move || (Made::from_settings(settings.clone()), false),
            ASK_AGAIN_AFTER,
        ))
    }

    /// The app's core. It reads `environment`, the one the app was started with, by the
    /// code the command line reads its own with, and the schedule runs the command line that
    /// comes with the app at `app`.
    ///
    /// The person's login shell is asked for its `PATH` when something first needs the core,
    /// never here: it can take seconds, and an app makes this on its main thread. Each tool's
    /// program is looked for on that `PATH` and then where its installers put it.
    #[uniffi::constructor]
    pub fn for_app(environment: HashMap<String, String>, app: Option<String>) -> Arc<Self> {
        let environment: Environment = environment.into_iter().collect();
        let app = app.map(PathBuf::from);
        Arc::new(Pitboard::asking(
            move || {
                let found = AppContext::discover(&environment, app.as_deref());
                let late = found.late;
                (Made::from_app(found), late)
            },
            ASK_AGAIN_AFTER,
        ))
    }

    /// The tools whose program was named or found, in the order a listing shows them. A
    /// tool missing here may still be on some `PATH`, so this narrows what is offered and
    /// never forbids anything. May ask the login shell again, so call it off the main
    /// thread.
    pub fn installed(&self) -> Vec<Tool> {
        let made = self.made.value_asking_again();
        ProviderId::ALL
            .iter()
            .copied()
            .filter(|tool| made.found.contains(tool))
            .map(tool)
            .collect()
    }

    /// Where programs are looked for, in `PATH`'s form: the person's login shell's, as far as
    /// the app looks in it. `None` when the shell could not be asked. Asked as `installed`
    /// is.
    pub fn search_path(&self) -> Option<String> {
        self.made.value_asking_again().search_path.clone()
    }

    /// Every account of every tool and what it has left, each asked of its own service.
    /// Parked logins whose access has lapsed are renewed first.
    ///
    /// `fresh` asks about every account whatever was asked moments ago. Pass false for a
    /// poll and true when somebody asked for it: an account is otherwise only asked about
    /// again once its tightest limit could have moved by a percentage point, which is what
    /// keeps the app and the command line to one request between them.
    pub fn status(&self, fresh: bool) -> Result<Status, PitboardError> {
        let done = self.core().core.status(fresh)?;
        let now = done.value.now;
        Ok(Status {
            now,
            accounts: done
                .value
                .rows
                .into_iter()
                .map(|row| account(row, now))
                .collect(),
            warnings: warnings(&done.warnings),
        })
    }

    pub fn switch_to(&self, label: String) -> Result<Switched, PitboardError> {
        switched(self.core().core.switch_to(&label))
    }

    /// Sign `tool`'s app out, parking the account in it, so somebody can sign in to another
    /// account in the app and enrol that one with `enroll_current`. Only Claude Desktop, whose
    /// login is a folder, is signed out this way; the result is a switch to nobody.
    pub fn switch_to_signed_out(&self, tool: String) -> Result<Switched, PitboardError> {
        use pitboard_core::provider::ProviderId;
        let Some(which) = ProviderId::parse(&tool) else {
            return Err(pitboard_core::error::Error::ProviderUnknown {
                typed: tool,
                known: ProviderId::ALL
                    .iter()
                    .map(|p| p.code().to_string())
                    .collect(),
            }
            .into());
        };
        switched(self.core().core.switch_to_signed_out(which))
    }

    /// The sign-out Pitboard made that no enrolment has followed yet, where there is one.
    /// Reads Pitboard's own file and nothing else.
    pub fn awaiting_sign_in(&self) -> Option<Awaiting> {
        self.core().core.awaiting_sign_in().map(|a| Awaiting {
            from_label: Some(a.from_label).filter(|label| !label.is_empty()),
            started_at: a.started_at,
        })
    }

    /// Whether Claude Desktop's usage is asked of claude.ai. Reads Pitboard's own file and
    /// nothing else, so it answers at once and never asks macOS anything.
    pub fn live_usage(&self) -> LiveUsageState {
        self.core().core.live_usage().into()
    }

    /// Turn live usage on: reads Claude's key once, which puts macOS's question in front of
    /// the person. Only for a button somebody pressed, after saying what it does.
    pub fn enable_live_usage(&self) -> Result<LiveUsageState, PitboardError> {
        Ok(self.core().core.enable_live_usage()?.into())
    }

    /// Turn live usage off and forget the key. Reads nothing.
    pub fn disable_live_usage(&self) -> Result<LiveUsageState, PitboardError> {
        Ok(self.core().core.disable_live_usage()?.into())
    }

    /// Enroll the account signed in now under `label`.
    pub fn enroll_current(&self, label: String) -> Result<Enrolled, PitboardError> {
        changed(self.core().core.enroll_current(&label), enrolled)
    }

    /// Starts the tool's own sign-in for a new account, watched rather than inherited. The
    /// label may name the tool, as in `codex/work`; a bare one means Claude Code. The caller
    /// shows what the tool says, can paste a fallback code, and finishes it.
    ///
    /// May ask the login shell again first, as `installed` does: a sign-in is when a program
    /// the first, late, ask could not find is needed.
    pub fn sign_in(&self, label: String) -> Result<Arc<SignIn>, PitboardError> {
        let made = self.made.value_asking_again();
        let watched = made.core.sign_in_watched(&label)?;
        let provider = watched.provider();
        Ok(Arc::new(SignIn {
            said: watched.said(),
            watched: Mutex::new(Some(watched)),
            label,
            provider,
            made,
        }))
    }

    pub fn forget(&self, label: String) -> Result<Changed, PitboardError> {
        changed(self.core().core.forget(&label), |email, warnings| Changed {
            email,
            warnings,
        })
    }

    pub fn rename(&self, from: String, to: String) -> Result<Changed, PitboardError> {
        changed(self.core().core.rename(&from, &to), |email, warnings| {
            Changed { email, warnings }
        })
    }

    /// When Pitboard's account index last changed, in epoch seconds, or 0 when there is
    /// none.
    ///
    /// One stat of one file, so an app can ask often. A switch typed in a terminal used to
    /// leave the menu bar naming the account the person had just stopped using, for as
    /// long as five minutes, with a button offering a switch that had already happened.
    /// Poll this, and when it moves, read `status_offline`: no network and no keychain.
    pub fn changed_at(&self) -> i64 {
        self.core().core.changed_at()
    }

    /// When Pitboard's usage readings last changed, in epoch milliseconds, or 0 when there
    /// are none.
    ///
    /// Every session's status line records what that session has seen, and a reading only
    /// moves forward, so what is remembered is the newest any front end has. Poll this
    /// beside `changed_at`, and when it moves, take the numbers from `status_offline`: no
    /// network and no keychain. Only the numbers: a reading moving says nothing about who is
    /// signed in, which is `changed_at`'s to say.
    pub fn readings_changed_at(&self) -> i64 {
        self.core().core.readings_changed_at()
    }

    /// The same report without asking anyone: the last numbers Pitboard measured, and who
    /// Claude Code's config says is signed in.
    ///
    /// What the app shows on a plane, and what it shows while a live read is still in
    /// flight, rather than an empty panel and a spinner.
    pub fn status_offline(&self) -> Result<Status, PitboardError> {
        let done = self.core().core.status_offline()?;
        let now = done.value.now;
        Ok(Status {
            now,
            accounts: done
                .value
                .rows
                .into_iter()
                .map(|row| account(row, now))
                .collect(),
            warnings: warnings(&done.warnings),
        })
    }

    /// Give up on an interrupted switch that cannot be finished, keeping every login it
    /// names. The way out when recovery cannot reach Anthropic, which until now sent the
    /// person to a terminal.
    ///
    /// `None` when there was no interrupted switch.
    pub fn abandon_recovery(&self) -> Result<Option<Abandoned>, PitboardError> {
        Ok(self.core().core.abandon_recovery()?.map(|a| Abandoned {
            from: a.from,
            to: a.to,
            logins_kept: u32::try_from(a.kept).unwrap_or(u32::MAX),
        }))
    }

    /// What Pitboard has changed, newest last.
    pub fn log(&self, limit: u32) -> Vec<Change> {
        self.core()
            .core
            .log(limit as usize)
            .into_iter()
            .map(|e| Change {
                at: e.at,
                caller: e.caller,
                verb: e.verb,
                subject: e.subject,
                outcome: e.outcome,
            })
            .collect()
    }

    /// Renew every parked login that is due, and nothing else.
    pub fn renew(&self) -> Vec<Renewed> {
        self.core()
            .core
            .renew()
            .into_iter()
            .map(|(key, outcome)| Renewed {
                label: key.typed(),
                provider: key.provider.code().into(),
                outcome: outcome.code().to_string(),
                expires_at: match &outcome {
                    switch::Renewal::NotRenewable { expires_at, .. } => *expires_at,
                    _ => None,
                },
            })
            .collect()
    }

    /// Whether anything keeps parked logins alive without a command being run.
    pub fn schedule(&self) -> Schedule {
        match self.core().core.schedule() {
            pitboard_core::schedule::Installed::Yes {
                path,
                every_seconds,
            } => Schedule::Installed {
                path: path.to_string_lossy().into_owned(),
                every_seconds,
            },
            pitboard_core::schedule::Installed::No => Schedule::Absent,
            pitboard_core::schedule::Installed::Unsupported => Schedule::Unsupported,
        }
    }

    /// Ask this computer's own scheduler to renew parked logins daily. Opt-in, and the
    /// caller is expected to say what it does before offering it. Refused where the app
    /// named no command line to run.
    pub fn schedule_install(&self) -> Result<String, PitboardError> {
        let made = self.core();
        if !made.schedules_a_command_line {
            return Err(pitboard_core::error::Error::ScheduleProgramUnnamed.into());
        }
        Ok(made.core.schedule_install()?.to_string_lossy().into_owned())
    }

    /// Take it away. `false` when there was nothing installed.
    pub fn schedule_uninstall(&self) -> Result<bool, PitboardError> {
        Ok(self.core().core.schedule_uninstall()?)
    }

    /// Point a schedule an app up to 0.3.0 wrote at the command line this app comes with.
    /// That app scheduled itself, so launchd has been starting a second app every day and
    /// renewing nothing. For the app to call when it starts: `true` when it repaired one,
    /// and nothing changes where the schedule already runs a command line or there is none.
    pub fn schedule_repair(&self) -> Result<bool, PitboardError> {
        Ok(self.core().core.schedule_repair()?)
    }

    /// What is running `provider`'s tool with a login a switch would leave it on, by kind,
    /// for the app to say so or to offer to quit an app first. Empty where nothing is, for a
    /// tool that follows a switch by itself, and for a provider code nobody knows. Reads the
    /// process list and nothing else, so it answers at once.
    pub fn holding(&self, provider: String) -> Vec<Holding> {
        pitboard_core::provider::ProviderId::parse(&provider)
            .map(|which| {
                self.core()
                    .core
                    .holding(which)
                    .into_iter()
                    .map(Holding::from)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn doctor(&self) -> Diagnosis {
        let diagnosis = self.core().core.doctor();
        Diagnosis {
            healthy: doctor::healthy(&diagnosis.checks),
            checks: diagnosis
                .checks
                .into_iter()
                .map(|c| Check {
                    code: c.code.to_string(),
                    name: c.name,
                    level: match c.level {
                        doctor::Level::Ok => Level::Ok,
                        doctor::Level::Warn => Level::Warn,
                        doctor::Level::Fail => Level::Fail,
                    },
                    detail: c.detail,
                    advice: c.advice,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(schedule_program: Option<String>) -> Settings {
        Settings {
            home: "/Users/x".into(),
            pitboard_home: None,
            claude_config_dir: None,
            secure_storage_dir: None,
            user: None,
            claude_program: None,
            codex_home: None,
            codex_program: None,
            search_path: None,
            schedule_program,
            no_argv: false,
            desktop_dir: None,
            desktop_app: None,
        }
    }

    /// A sign-in is read by its own tool's module, named by its code as every `provider`
    /// field names it, and one nobody knows offers nothing rather than another tool's
    /// reading.
    #[test]
    fn a_sign_in_is_read_by_its_own_tools_module() {
        let said = "If the browser didn't open, visit: https://claude.com/cai/oauth/authorize?x\n\
                    Paste code here if prompted > ";
        let address = Some("https://claude.com/cai/oauth/authorize?x".to_string());
        assert_eq!(
            sign_in_view("claude".into(), said.into(), false),
            SignInView {
                url: address.clone(),
                wants_code: true
            }
        );
        assert!(!sign_in_view("claude".into(), said.into(), true).wants_code);
        assert_eq!(
            sign_in_view("codex".into(), said.into(), false),
            SignInView {
                url: address,
                wants_code: false
            }
        );
        assert_eq!(
            sign_in_view("gemini".into(), said.into(), false),
            SignInView {
                url: None,
                wants_code: false
            }
        );
    }

    /// Every tool the core handles is one the app lists, Claude Desktop included, now that
    /// the app can sign it out, switch it and show it.
    #[test]
    fn the_app_lists_every_tool() {
        let codes: Vec<String> = tools().into_iter().map(|tool| tool.code).collect();
        assert_eq!(codes, ["claude", "codex", "desktop"]);
    }

    /// A home nothing can be written under: what these read is Pitboard's own files, and
    /// there are none.
    fn nowhere() -> Arc<Pitboard> {
        Pitboard::new(Settings {
            home: "/dev/null".into(),
            desktop_dir: Some("/dev/null/claude-desktop".into()),
            desktop_app: Some("/dev/null/Claude.app".into()),
            ..settings(None)
        })
    }

    /// Live usage is off until somebody turns it on, and reading whether it is reads
    /// Pitboard's own file and nothing else.
    #[test]
    fn live_usage_is_off_until_somebody_turns_it_on() {
        let state = nowhere().live_usage();
        assert!(!state.enabled);
        assert_eq!(state.approval, "unknown");
        assert_eq!(state.reason, None);
        assert_eq!(state.last_ok_at, None);
        assert!(nowhere().awaiting_sign_in().is_none());
    }

    /// Only an app whose login is a folder is signed out by Pitboard, and a tool nobody
    /// knows is said to be one rather than taken for another.
    #[test]
    fn only_claude_desktop_is_signed_out() {
        let Err(PitboardError::Failed { code, .. }) =
            nowhere().switch_to_signed_out("codex".into())
        else {
            panic!("Codex was signed out");
        };
        assert_eq!(code, "usage");
        let Err(PitboardError::Failed { code, .. }) =
            nowhere().switch_to_signed_out("nonsense".into())
        else {
            panic!("an unknown tool was signed out");
        };
        assert_eq!(code, "provider_unknown");
    }

    /// A sign-out that found nobody signed in reaches the app as nothing changed, with no
    /// account named, which is what it read before the core said it apart.
    #[test]
    fn a_sign_out_of_nobody_reaches_the_app_as_nothing_changed() {
        let said = switched(Ok(service::Done {
            value: switch::Outcome::AlreadySignedOut {
                provider: pitboard_core::provider::ProviderId::Desktop,
            },
            warnings: Vec::new(),
        }))
        .unwrap_or_else(|_| panic!("not a failure"));
        assert!(
            matches!(&said.outcome, Switch::AlreadyActive { label } if label.is_empty()),
            "said as something else"
        );
    }

    /// `PITBOARD_NO_ARGV` reaches the app's core as it reaches the command line's. The app
    /// used to write on the argument line whatever it was set to.
    #[test]
    fn the_app_refuses_the_argument_line_where_it_is_told_to() {
        assert!(settings(None).context().argv_fallback());
        let refusing = Settings {
            no_argv: true,
            ..settings(None)
        };
        assert!(!refusing.context().argv_fallback());
    }

    #[test]
    fn the_app_names_the_command_line_its_schedule_runs() {
        let bundled = "/Applications/Pitboard.app/Contents/Helpers/pitboard";
        assert_eq!(
            settings(Some(bundled.into())).context().schedule_program(),
            Some(std::path::Path::new(bundled))
        );
        assert_eq!(settings(None).context().schedule_program(), None);
    }

    /// These bindings serve the app, and the app is not a command line: where it names
    /// none, scheduling this process would start a second app every day and renew nothing.
    /// The home is one nothing can be written under, so even a regression here reaches no
    /// scheduler.
    #[test]
    fn the_app_schedules_nothing_without_a_command_line_to_run() {
        let pitboard = Pitboard::new(Settings {
            home: "/dev/null".into(),
            ..settings(None)
        });
        let Err(PitboardError::Failed { code, message, .. }) = pitboard.schedule_install() else {
            panic!("the app scheduled itself");
        };
        assert_eq!(code, "schedule_program_unnamed");
        assert!(message.contains("command line"), "{message}");
    }

    fn limit(kind: &str, length_seconds: Option<i64>, scope: Option<&str>) -> Limit {
        Limit {
            kind: kind.into(),
            length_seconds,
            scope: scope.map(str::to_owned),
            percent: 42.0,
            resets_at: None,
            severity: None,
            is_active: true,
        }
    }

    /// The apps name a limit from the record they were given, as the command line names
    /// the reading it came from.
    #[test]
    fn a_limit_is_named_from_its_record() {
        let fable = limit("weekly_scoped", Some(604_800), Some("Fable"));
        assert_eq!(limit_column(fable), "week · Fable");
        let fable = limit("weekly_scoped", Some(604_800), Some("Fable"));
        assert_eq!(
            limit_name(fable),
            "weekly",
            "a sentence places the scope itself"
        );
        assert_eq!(limit_column(limit("90_minute", Some(5_400), None)), "90m");
        assert_eq!(limit_name(limit("session", None, None)), "5-hour");
    }

    /// An account's runway reaches the apps as seconds and whether its limit is filling,
    /// and reads the way `pitboard status` says it. Without the seconds there is nothing to
    /// say, whichever way the account is going.
    #[test]
    fn a_runway_is_said_from_an_accounts_two_fields() {
        assert_eq!(
            runway(Some(5_400), true).as_deref(),
            Some("about 1h 30m left at this rate")
        );
        assert_eq!(
            runway(Some(3_900), false).as_deref(),
            Some("resets in 1h 05m")
        );
        assert_eq!(runway(Some(30), true).as_deref(), Some("about to run out"));
        assert_eq!(
            runway(Some(30), false).as_deref(),
            Some("resets any moment")
        );
        assert_eq!(runway(None, true), None);
        assert_eq!(runway(None, false), None);
    }

    /// Only a renewal that renewed counts as one: a deferred or refused one was due and
    /// was not renewed.
    #[test]
    fn a_renewal_note_counts_what_was_renewed() {
        let renewed = |outcome: &str| Renewed {
            label: "work".into(),
            provider: "claude".into(),
            outcome: outcome.into(),
            expires_at: None,
        };
        assert_eq!(renewal_note(Vec::new()), "No parked login was due.");
        assert_eq!(
            renewal_note(vec![renewed("renewed"), renewed("not_renewable")]),
            "Renewed one.",
            "a login nothing can renew is not due"
        );
        assert_eq!(
            renewal_note(vec![renewed("renewed"), renewed("renewal_deferred")]),
            "Renewed 1 of 2; the rest are tried again next time."
        );
        assert_eq!(
            renewal_note(vec![renewed("parked_login_refused")]),
            "1 due; none could be renewed this time."
        );
    }

    /// A check that warns is worth looking at, and one that fails outweighs every warning.
    #[test]
    fn the_doctor_summary_says_not_to_switch_while_a_check_fails() {
        let check = |level: Level| Check {
            code: "credential".into(),
            name: "credential".into(),
            level,
            detail: String::new(),
            advice: String::new(),
        };
        assert_eq!(
            doctor_summary(vec![check(Level::Ok)]),
            "Everything Pitboard checks is in order."
        );
        assert_eq!(
            doctor_summary(vec![
                check(Level::Warn),
                check(Level::Ok),
                check(Level::Fail)
            ]),
            "1 broken: do not switch accounts until fixed."
        );
        assert_eq!(
            doctor_summary(vec![check(Level::Warn), check(Level::Warn)]),
            "2 things are worth looking at."
        );
    }

    /// The apps read a parked login's life from the record they hold, in the sentence form.
    #[test]
    fn a_parked_logins_life_is_read_from_its_record() {
        let parked = |refresh_expires_at| Parked {
            parked_at: 0,
            access_expires_at: None,
            refresh_expires_at,
        };
        assert_eq!(parked_life(None, 1_000), None);
        assert_eq!(parked_life(Some(parked(None)), 1_000), None);
        assert_eq!(
            parked_life(Some(parked(Some(1_000 + 3 * 86_400))), 1_000).as_deref(),
            Some("Parked login good for 3 more days")
        );
    }

    /// Repairing at launch is a no-op wherever there is nothing to repair, and never
    /// reaches a scheduler to find that out. This test's own program stands in for a
    /// command line that is there.
    #[test]
    fn the_app_repairs_nothing_where_no_schedule_runs_it() {
        let there = std::env::current_exe().expect("this test's own program");
        for named in [None, Some(there.to_string_lossy().into_owned())] {
            let pitboard = Pitboard::new(Settings {
                home: "/dev/null".into(),
                ..settings(named)
            });
            assert!(!pitboard.schedule_repair().expect("nothing to repair"));
        }
    }

    /// A core over a home nothing can be written under, with a `codex` named where one is
    /// given. Its Claude app is named and not there, so whether this Mac has Claude in
    /// /Applications says nothing here.
    fn made(codex: Option<&str>) -> Made {
        Made::from_settings(Settings {
            home: "/dev/null".into(),
            codex_program: codex.map(str::to_owned),
            desktop_app: Some("/dev/null/Claude.app".into()),
            ..settings(None)
        })
    }

    fn installed(pitboard: &Pitboard) -> Vec<String> {
        pitboard
            .installed()
            .into_iter()
            .map(|tool| tool.code)
            .collect()
    }

    /// Counts how often the app's core was made.
    #[derive(Clone, Default)]
    struct Asks(Arc<std::sync::atomic::AtomicUsize>);

    impl Asks {
        /// One more ask, and how many there have been with it.
        fn note(&self) -> usize {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
        }

        fn count(&self) -> usize {
            self.0.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    /// Where the tools are can take asking the person's login shell, so the core is made when
    /// something first needs it, and not when the app makes this object, which it does on its
    /// main thread; and once, however many calls arrive at the same time.
    #[test]
    fn the_core_is_made_once_when_it_is_first_needed() {
        let asks = Asks::default();
        let pitboard = Arc::new(Pitboard::asking(
            {
                let asks = asks.clone();
                move || {
                    asks.note();
                    std::thread::sleep(Duration::from_millis(100));
                    (made(Some("/nowhere/codex")), false)
                }
            },
            ASK_AGAIN_AFTER,
        ));
        assert_eq!(tools().len(), 3, "listing the tools asks nothing");
        assert_eq!(asks.count(), 0, "and nor does making the object");
        let calls: Vec<_> = (0..3)
            .map(|which| {
                let pitboard = pitboard.clone();
                std::thread::spawn(move || match which {
                    0 => drop(pitboard.installed()),
                    1 => drop(pitboard.changed_at()),
                    _ => drop(pitboard.search_path()),
                })
            })
            .collect();
        for call in calls {
            call.join().expect("a call that answers");
        }
        assert_eq!(installed(&pitboard), ["codex"]);
        assert_eq!(asks.count(), 1);
    }

    /// A login shell too slow to answer, which startup files are while the machine is busy
    /// logging in, is asked once more when something next asks what is installed, a while
    /// later, and what it answers then is what is used. Once more and no more: a shell that
    /// is always that slow would otherwise cost its patience on every ask.
    #[test]
    fn a_login_shell_too_slow_to_answer_is_asked_once_more_later() {
        let asks = Asks::default();
        let pitboard = Pitboard::asking(
            {
                let asks = asks.clone();
                move || match asks.note() {
                    1 => (made(None), true),
                    _ => (made(Some("/nowhere/codex")), false),
                }
            },
            Duration::from_millis(200),
        );
        assert!(
            installed(&pitboard).is_empty(),
            "what the first, late, ask found"
        );
        assert!(
            installed(&pitboard).is_empty(),
            "and nothing more until the while is up"
        );
        assert_eq!(asks.count(), 1);
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(installed(&pitboard), ["codex"]);
        assert_eq!(asks.count(), 2);
        assert_eq!(installed(&pitboard), ["codex"]);
        assert_eq!(asks.count(), 2, "asked once more, and no more");
    }

    /// Not before the while is up, and never when the shell answered or could not be asked.
    /// A second ask that is late as well leaves what the first made in its place.
    #[test]
    fn a_login_shell_is_not_asked_again_sooner_or_for_nothing() {
        let soon = Asks::default();
        let early = Pitboard::asking(
            {
                let soon = soon.clone();
                move || {
                    soon.note();
                    (made(None), true)
                }
            },
            Duration::from_secs(3600),
        );
        installed(&early);
        installed(&early);
        assert_eq!(soon.count(), 1);

        let answered = Asks::default();
        let pitboard = Pitboard::asking(
            {
                let answered = answered.clone();
                move || {
                    answered.note();
                    (made(None), false)
                }
            },
            Duration::ZERO,
        );
        installed(&pitboard);
        installed(&pitboard);
        assert_eq!(answered.count(), 1);

        let late = Asks::default();
        let again = Pitboard::asking(
            {
                let late = late.clone();
                move || match late.note() {
                    1 => (made(None), true),
                    _ => (made(Some("/nowhere/codex")), true),
                }
            },
            Duration::ZERO,
        );
        assert!(installed(&again).is_empty());
        assert!(installed(&again).is_empty(), "the second ask was late too");
        assert!(installed(&again).is_empty());
        assert_eq!(late.count(), 2);
    }

    /// The app's core reads the environment it was started with as the command line reads its
    /// own, and asks the login shell that environment names. A shell that cannot be run is no
    /// answer, so nothing is said to be on its `PATH`, and a program named outright counts as
    /// found. A custom OAuth endpoint refuses a change to a Claude Code account, and an app
    /// outside a bundle has no command line to schedule.
    #[test]
    fn the_apps_core_is_made_from_the_environment_it_was_started_with() {
        // A home of its own, so Claude Code's slot is hashed from it and nothing real is read.
        let home = std::env::temp_dir().join(format!("pitboard-ffi-app-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("a scratch home");
        let at = |name: &str| home.join(name).to_string_lossy().into_owned();
        let environment: HashMap<String, String> = [
            ("HOME", at("")),
            ("SHELL", "/nowhere/at/all/sh".into()),
            ("PITBOARD_HOME", at("pitboard")),
            ("CLAUDE_CONFIG_DIR", at("claude")),
            ("CODEX_HOME", at("codex")),
            ("PITBOARD_CODEX", "/nowhere/codex".into()),
            (
                "CLAUDE_CODE_CUSTOM_OAUTH_URL",
                "https://oauth.example".into(),
            ),
        ]
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect();
        let pitboard = Pitboard::for_app(environment, None);
        assert!(installed(&pitboard).contains(&"codex".to_owned()));
        assert_eq!(pitboard.search_path(), None, "the shell could not be run");
        let Err(PitboardError::Failed { code, .. }) = pitboard.enroll_current("work".into()) else {
            panic!("a Claude Code account was enrolled under a custom OAuth endpoint");
        };
        assert_eq!(code, "custom_oauth_endpoint");
        let Err(PitboardError::Failed { code, .. }) = pitboard.schedule_install() else {
            panic!("an app outside a bundle scheduled itself");
        };
        assert_eq!(code, "schedule_program_unnamed");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The command line's search crosses the bindings with its answer as the core gives it.
    #[test]
    fn a_command_line_found_nowhere_is_said_to_be_nowhere() {
        assert_eq!(
            find_command_line(
                Some("/nowhere/at/all".into()),
                vec!["/nowhere/else".into()],
                None
            ),
            FoundCommandLine::Nowhere
        );
        let places = command_line_places("/Users/x".into());
        assert_eq!(places[..2], ["/Users/x/.cargo/bin", "/Users/x/.local/bin"]);
        assert_eq!(app_command_line("/Users/x/pitboard".into()), None);
    }

    /// Where an app's home and Pitboard directory are, and whether a path is a program, cross
    /// the bindings as the core reads them, so the app has no rule of its own for either.
    #[test]
    fn the_app_asks_the_core_where_things_are_and_what_runs() {
        let environment = |pairs: &[(&str, &str)]| -> HashMap<String, String> {
            pairs
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect()
        };
        assert_eq!(
            home_directory(environment(&[("HOME", "/Users/x")])),
            "/Users/x"
        );
        assert_eq!(
            pitboard_directory(environment(&[("HOME", "/Users/x")])),
            "/Users/x/.pitboard"
        );
        assert_eq!(
            pitboard_directory(environment(&[
                ("HOME", "/Users/x"),
                ("PITBOARD_HOME", "/elsewhere/./p/")
            ])),
            "/elsewhere/./p/"
        );
        assert!(can_run("/bin/sh".into()));
        assert!(!can_run("/bin".into()), "a directory is not a program");
        assert!(!can_run("/nowhere/at/all".into()));
    }
}
