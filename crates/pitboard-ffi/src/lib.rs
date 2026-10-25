//! Pitboard's core for its native apps, as UniFFI bindings.
//!
//! An app makes one `PitboardModel`, sends it what was asked of it, and shows the snapshots
//! its listener is told of. Nothing the model exports waits on the core: `launch.rs` makes
//! the core from what the app was started with, and only the model's own threads call it.
//! Beside the model and the records its snapshots carry, the bindings export the sites'
//! records (`sites.rs`), the account windows' rules (`account_windows`), `check_name` and
//! `downloads_quit_question` of `present`, the fixtures' `fixture_names` and `fixture_page`,
//! and three free functions the macOS app asks of its bundle and its environment,
//! `app_command_line`, `can_run` and `pitboard_directory`, and the Windows app's
//! `windows_records_key`, `launch_refusal` and `unschedule_app`. The snapshot carries what an
//! app shows of the accounts and the machine, such as a limit's names or how long an account
//! lasts, made by `present` and the model, so no free function says it again. The free
//! functions answer at once from what they are given, apart from `can_run`, which asks the
//! file system about one path, `launch_refusal`, which asks the system how this process runs,
//! `download_destination`, which asks the file system whether each name it tries is taken, and
//! `unschedule_app`, which asks the system's scheduler and may take the watch and renewal
//! tasks away. Timestamps are epoch seconds.
//!
//! The model and what it presents came from the macOS app's Swift. Where a comment here
//! names a Swift file the app no longer has, such as `AppModel.swift`, `MachineModel.swift`
//! or `Notifier.swift`, it means that file as it was before the app ran on the model, at
//! commit 277539b. The account windows' bookkeeping moved later, and a comment that names its
//! Swift, such as `LinkInbox.swift`, `StoreJanitor.swift`'s sweep or
//! `AccountPickerTests.swift`, means it as it was at commit a3e5ce0, where the comment says so.

use pitboard_core::context::{Context, Environment};
use pitboard_core::host::Os;
use pitboard_core::provider::ProviderId;
use pitboard_core::status::{Stale, Standing};
use pitboard_core::{doctor, words};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

uniffi::setup_scaffolding!();

mod sites;
pub use sites::{Site, SiteLink};

mod model;
pub use model::{
    AppControl, AppLaunch, AtLimit, DownloadEnd, EarlierPreferences, EarlierWindowRecords, Intent,
    LocalTime, ModelListener, NotificationButton, NotificationButtonKind, Notifications, Pane,
    PitboardModel, Place, PlatformError, ReadFailure, RunOutNotice, RunningSignIn, SettingsSection,
    Sheet, Snapshot, Tip, WindowRequest, WindowsLaunch,
};

// The fixtures, which the `fixture` feature compiles; what is exported of them is the same
// in every build.
mod fixture;
pub use fixture::{FixtureError, fixture_names, fixture_page};

mod present;
pub use present::{
    AccountAction, AccountDetail, AccountItem, AccountMenuShown, AccountNote, AccountSection,
    AccountStanding, AccountWindowsShown, AccountsShown, ActionKind, ActivityLine, ActivityShown,
    AtLimitChoice, AtLimitShown, CheckLine, ChecksShown, Choice, CommandLineShown, DownloadShown,
    DownloadState, EmptyList, FailureText, Footing, ItemAction, ItemOffer, LimitKind, LimitRow,
    LinkPicker, MachineShown, MenuBarText, MenuEntry, MenuNotices, Missing, NameCheck, NoteKind,
    NoticeAction, OpenWindow, PageLoad, PanelNotice, PickerAccount, PickerShown, Question,
    RemovalShown, RowEnd, ScheduleShown, SetupLink, SetupStep, Severity, SheetText, SheetTool,
    SigningInText, StoreDeletion, StowText, TermsShown, WaitingShown, WarmAccountShown, WarmDetail,
    WarmShown, WindowOffer, WindowQuestion, WindowWaiting, check_name, downloads_quit_question,
};

mod account_windows;
pub use account_windows::{
    AlertText, Asker, FrameOrigin, NavigationDecision, NavigationPolicy, NavigationRequest,
    NavigationTarget, PagePermission, PageRole, ProcessEnded, ResponseDecision, ResponseFacts,
    SignInWindowSize, SiteMenu, WindowAccount, WindowNoteKind, after_content_process_ended,
    decide_navigation, decide_response, dialog_title, download_destination, download_host,
    download_question, frame_asker, opening_note, page_may_close, page_may_use, remove_data_alert,
    sign_in_window_size, window_home, window_note,
};

// The core the model's lanes call, made from what the app was started with, and what it
// answers that only the model reads. None of it is exported.
mod launch;
pub(crate) use launch::{
    Adoption, AppCore, AutoLooked, AutoSwitched, Change, Check, Enrolled, EnrolledAs, GaveUp,
    Holding, OwnCommandLine, PitboardError, Remedy, SignInSession, Switch, Switched, Uninstalled,
    WarmRefused,
};
// What a test or a fixture makes the app's core of, in place of the environment.
#[cfg(any(test, feature = "fixture"))]
pub(crate) use launch::{ASK_AGAIN_AFTER, Made};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tool {
    // what a label's prefix and every `provider` field say: `claude`, `codex`
    pub code: String,
    // as its own documentation names it: `Claude Code`, `Codex`
    pub name: String,
    pub program: String,
    // the company asked about its accounts
    pub service: String,
}

// The tools the app lists on `os`, in the order a listing shows them.
pub(crate) fn listed(os: Os) -> impl Iterator<Item = ProviderId> {
    ProviderId::ALL
        .iter()
        .copied()
        .filter(move |tool| tool.supported_on(os))
}

pub(crate) fn tools_on(os: Os) -> Vec<Tool> {
    listed(os).map(tool).collect()
}

#[cfg(test)]
pub(crate) fn tools() -> Vec<Tool> {
    tools_on(pitboard_core::host::OS)
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
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum FoundCommandLine {
    /// The app's own, at this path or linked to from it.
    Bundled { path: String },
    /// Another install, at this path.
    Another { path: String },
    /// None anywhere a terminal would look.
    Nowhere,
}

/// The `pitboard` the core found, as a snapshot carries it.
pub(crate) fn found_command_line(found: pitboard_core::app::CommandLine) -> FoundCommandLine {
    let shown = |path: PathBuf| path.to_string_lossy().into_owned();
    match found {
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

/// The Pitboard directory of an app started with `environment`, as the core reads it:
/// `PITBOARD_HOME`, or the system's default in the home, `AppData\Local\Pitboard` on
/// Windows. The path as the environment gives it, with any `.`, `..` or trailing separator
/// it holds.
#[uniffi::export]
pub fn pitboard_directory(environment: HashMap<String, String>) -> String {
    let environment: Environment = environment.into_iter().collect();
    environment.pitboard_home().to_string_lossy().into_owned()
}

#[uniffi::export]
pub fn no_update_key() -> String {
    present::words::NO_UPDATE_KEY.into()
}

// macOS shows a notification's buttons from categories registered at launch, by their titles.
#[uniffi::export]
pub fn notification_buttons() -> Vec<NotificationButtonKind> {
    present::notification_buttons()
}

// What stops the Windows app before it makes a model, which then shows this alone.
#[uniffi::export]
pub fn launch_refusal(environment: HashMap<String, String>) -> Option<ReadFailure> {
    let environment: Environment = environment.into_iter().collect();
    // The gate writes nothing, so the command line's caller name never reaches the audit log.
    launch::refusal(Context::for_command_line(&environment))
}

// `Pitboard.exe --unregister`, as the MSI's full uninstall runs it; a refusal takes nothing away.
#[uniffi::export]
pub fn unschedule_app(environment: HashMap<String, String>, app: String) -> bool {
    pitboard_core::release::set_version(env!("CARGO_PKG_VERSION"));
    let environment: Environment = environment.into_iter().collect();
    let found = pitboard_core::app::AppContext::discover(&environment, Some(Path::new(&app)));
    pitboard_core::service::Pitboard::new(found.context)
        .schedule_uninstall_for_app()
        .unwrap_or(false)
}

// `WindowsLaunch::key` in the Windows app: the Pitboard directory as Windows compares paths.
#[uniffi::export]
pub fn windows_records_key(directory: String) -> String {
    pitboard_core::host::path_key_in_any_case(&directory)
}

// Something to know about that did not stop the operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Warning {
    pub code: String,
    pub message: String,
    // as `Account.id` names it, for a warning whose words change while it stands
    pub account: Option<String>,
    // what the file a `fallback_login` warning is about holds, so whether it can be put away
    pub held: Option<Held>,
    // the tool it is about, as a `Tool`'s code, where it names one
    pub provider: Option<String>,
}

// What a file behind a tool's store holds, as far as Pitboard can read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Held {
    Login,
    NoLogin,
    Unreadable,
}

// What the core reports, as the model keeps it: the snapshot shows it only as presented.

// An interrupted switch given up on, which kept every login it named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Abandoned {
    pub from: String,
    pub to: String,
    // copies kept rather than deleted, since which one is live is now unknown
    pub logins_kept: u32,
}

// Whether anything keeps parked logins alive on this machine without a command being run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Schedule {
    // the platform's own scheduler runs `pitboard renew` every `every_seconds`
    Installed { path: String, every_seconds: u32 },
    // parked logins are renewed when Pitboard runs, and otherwise not
    Absent,
    // no scheduler Pitboard knows how to write
    Unsupported,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Limit {
    // `session`, `weekly_all`, `weekly_scoped`, or another limit's name, such as `1_day`
    pub kind: String,
    pub length_seconds: Option<i64>,
    // the model a scoped limit applies to
    pub scope: Option<String>,
    // past 100 once exceeded
    pub percent: f64,
    pub resets_at: Option<i64>,
    pub severity: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Usage {
    pub observed_at: Option<i64>,
    pub windows: Vec<Limit>,
    // a limit `windows` leaves out is one the account does not have
    pub lists_every_limit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Parked {
    pub refresh_expires_at: Option<i64>,
    // when the session cookie a parked app login holds says it lapses
    pub session_expires_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Account {
    // the tool and the account, since two tools' accounts can share a label
    pub id: String,
    // a `Tool`'s code
    pub provider: String,
    // `None` for an account signed in but not enrolled
    pub label: Option<String>,
    // `claude/work`: what a switch, a forget or a rename names it by
    pub qualified: Option<String>,
    // a login of the tool that belongs to no account Pitboard can name
    pub unplaced: bool,
    pub email: String,
    // what its windows' stores derive from; empty for a login Pitboard cannot place
    pub account_id: String,
    pub signed_in: bool,
    pub standing: Standing,
    pub parked: Option<Parked>,
    pub usage: Option<Usage>,
    pub stale: Option<Stale>,
    pub stale_explanation: Option<String>,
    pub plan: Option<String>,
    // when Pitboard last read it in use, for a tool no read asks
    pub read_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Status {
    pub now: i64,
    // the signed-in account first
    pub accounts: Vec<Account>,
    pub warnings: Vec<Warning>,
    // the sign-ins inside an app left halfway, each with an account parked for it
    pub awaiting: Vec<Awaiting>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Awaiting {
    pub provider: String,
    pub label: String,
    // the account parked for it, as SwitchTo takes it: desktop/work
    pub put_back: String,
    // a Sign In Again of `label`, not an add
    pub again: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

// What `present` says of the records above in the core's words, where a record's fields are
// not the core's own types: the step a limit's colour is at, how long an account lasts, what
// a renewal run did and the line over doctor's checks. Each wraps the core's function of the
// same name, which the command line calls directly wherever it says the same thing. None
// reads a clock, a file or the keychain.

/// How much of a limit is used, in the three steps its colour changes at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
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
pub(crate) fn usage_level(percent: f64) -> UsageLevel {
    match words::usage_level(percent) {
        words::UsageLevel::Plenty => UsageLevel::Plenty,
        words::UsageLevel::Low => UsageLevel::Low,
        words::UsageLevel::Out => UsageLevel::Out,
    }
}

/// The line over a diagnosis's checks: what is worth looking at while checks only warn,
/// and not to switch accounts while one fails.
pub(crate) fn doctor_summary(checks: &[Check]) -> String {
    words::doctor_summary(checks.iter().map(|c| match c.level {
        Level::Ok => doctor::Level::Ok,
        Level::Warn => doctor::Level::Warn,
        Level::Fail => doctor::Level::Fail,
    }))
}

// One failure outweighs every warning, as in the summary's words.
pub(crate) fn doctor_level(checks: &[Check]) -> Level {
    let has = |level| checks.iter().any(|c| c.level == level);
    if has(Level::Fail) {
        Level::Fail
    } else if has(Level::Warn) {
        Level::Warn
    } else {
        Level::Ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each check keeps doctor's level, so the line over the checks says what doctor says.
    #[test]
    fn the_doctor_summary_says_not_to_switch_while_a_check_fails() {
        let check = |level: Level| Check {
            code: "credential".into(),
            name: "credential".into(),
            level,
            detail: String::new(),
            advice: String::new(),
        };
        use doctor::Level as Core;
        assert_eq!(
            doctor_summary(&[check(Level::Ok)]),
            words::doctor_summary([Core::Ok])
        );
        assert_eq!(
            doctor_summary(&[check(Level::Warn), check(Level::Ok), check(Level::Fail)]),
            words::doctor_summary([Core::Warn, Core::Ok, Core::Fail])
        );
        assert_eq!(
            doctor_summary(&[check(Level::Warn), check(Level::Warn)]),
            words::doctor_summary([Core::Warn, Core::Warn])
        );
    }

    // So the app has no rule of its own for any of them.
    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "where Windows is, as it names it"
    )]
    fn the_app_asks_the_core_where_things_are_and_what_runs() {
        use pitboard_core::host::OS;
        let environment = |pairs: &[(&str, &str)]| -> HashMap<String, String> {
            pairs
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect()
        };
        let (home, directory) = match OS {
            Os::MacOs | Os::Linux => ("/Users/x", "/Users/x/.pitboard"),
            Os::Windows => (r"C:\Users\x", r"C:\Users\x\AppData\Local\Pitboard"),
        };
        assert_eq!(
            pitboard_directory(environment(&[("HOME", home), ("USERPROFILE", home)])),
            directory
        );
        assert_eq!(
            pitboard_directory(environment(&[
                ("HOME", home),
                ("USERPROFILE", home),
                ("PITBOARD_HOME", "/elsewhere/./p/")
            ])),
            "/elsewhere/./p/"
        );
        assert_eq!(
            app_command_line("/Users/x/pitboard".into()),
            None,
            "not an app"
        );
        let (program, folder) = match OS {
            Os::MacOs | Os::Linux => (PathBuf::from("/bin/sh"), PathBuf::from("/bin")),
            Os::Windows => {
                let system = std::env::var_os("SystemRoot")
                    .map(|windows| PathBuf::from(windows).join("System32"))
                    .expect("SystemRoot");
                (system.join("cmd.exe"), system)
            }
        };
        let text = |path: &PathBuf| path.to_string_lossy().into_owned();
        assert!(can_run(text(&program)));
        assert!(!can_run(text(&folder)), "a directory is not a program");
        assert!(!can_run("/nowhere/at/all".into()));
    }

    // Every system, as the release gate opens each build the tests run.
    #[test]
    fn the_app_asks_the_core_whether_it_may_start() {
        let folder = tempfile::tempdir().expect("a scratch folder");
        let scratch = folder.path().join("home");
        let environment = |pitboard: &str| -> HashMap<String, String> {
            HashMap::from([
                ("HOME".into(), scratch.to_string_lossy().into_owned()),
                ("USERPROFILE".into(), scratch.to_string_lossy().into_owned()),
                ("PITBOARD_HOME".into(), pitboard.into()),
            ])
        };
        let full = scratch.join(".pitboard");
        assert_eq!(launch_refusal(environment(&full.to_string_lossy())), None);
        assert_eq!(
            pitboard_core::release::version(),
            Some(env!("CARGO_PKG_VERSION")),
            "named to the release gate first"
        );
        let relative = launch_refusal(environment("pitboard")).expect("refused");
        assert_eq!(relative.code, "home_not_absolute");
        assert!(
            relative.message.starts_with("PITBOARD_HOME"),
            "{relative:?}"
        );
        assert!(!scratch.exists(), "nothing is made");
    }

    // The core's own tests take a task away on a scheduler in memory; this one reaches none.
    #[test]
    fn a_path_that_is_no_app_takes_no_renewal_task_away() {
        let folder = tempfile::tempdir().expect("a scratch folder");
        let scratch = folder.path().join("home");
        let home = scratch.to_string_lossy().into_owned();
        let environment = HashMap::from([
            ("HOME".into(), home.clone()),
            ("USERPROFILE".into(), home),
            ("SHELL".into(), "/nowhere/at/all/sh".into()),
        ]);
        let app = scratch.join("Pitboard").to_string_lossy().into_owned();
        assert!(!unschedule_app(environment, app));
        assert!(!scratch.exists(), "nothing is made");
    }

    #[test]
    fn the_windows_app_keys_its_records_by_its_directory_in_any_case() {
        assert_eq!(
            windows_records_key(r"c:\users\x\appdata\local\pitboard\".into()),
            windows_records_key(r"\\?\C:\Users\X\AppData\Local\Pitboard".into())
        );
        assert_eq!(
            windows_records_key("C:/Users/X/AppData/Local/Pitboard".into()),
            r"C:\USERS\X\APPDATA\LOCAL\PITBOARD"
        );
    }
}
