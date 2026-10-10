//! Pitboard's core for its native apps, as UniFFI bindings.
//!
//! An app makes one `PitboardModel`, sends it what was asked of it, and shows the snapshots
//! its listener is told of. Nothing the model exports waits on the core: `launch.rs` makes
//! the core from what the app was started with, and only the model's own threads call it.
//! Beside the model and the records its snapshots carry, the bindings export the sites and
//! the links from outside (`sites.rs`), the account windows' rules (`account_windows`),
//! `name_to_save` and `downloads_quit_question` of `present`, the fixtures' `fixture_names`
//! and `fixture_page`, and three free functions the macOS app asks of its bundle and its
//! environment, `app_command_line`, `can_run` and `pitboard_directory`. The snapshot carries
//! what an app shows of the accounts and the machine, such as a limit's names or how long an
//! account lasts, made by `present` and the model, so no free function says it again. The
//! free functions answer at once from what they are given, apart from `can_run`, which asks
//! the file system about one path, and `download_destination`, which asks it whether each
//! name it tries is taken. Timestamps are epoch seconds.
//!
//! The model and what it presents came from the macOS app's Swift. Where a comment here
//! names a Swift file the app no longer has, such as `AppModel.swift`, `MachineModel.swift`
//! or `Notifier.swift`, or a test in `AppModelTests.swift`, `PresentationTests.swift` or
//! `MenuTests.swift`, it means that file as it was before the app ran on the model, at
//! commit 277539b. The `AppModelTests.swift` the app's package has since is another. The
//! account windows' bookkeeping moved later, and a comment that names its Swift, such as
//! `LinkInbox.swift`, `StoreJanitor.swift`'s sweep or `AccountPickerTests.swift`, means it as
//! it was at commit a3e5ce0, where the comment says so.

use pitboard_core::context::Environment;
use pitboard_core::provider::ProviderId;
use pitboard_core::{doctor, switch, words};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

uniffi::setup_scaffolding!();

mod sites;
pub use sites::{
    Conjunction, LinkRefusal, Site, SiteLink, link_refusal_reason, pitboard_link,
    read_pitboard_link, site_link, site_names, sites, sites_for,
};

mod model;
pub use model::{
    AppControl, AppLaunch, DownloadEnd, EarlierPreferences, EarlierWindowRecords, Failure, Intent,
    LastSwitch, LocalTime, ModelListener, Notifications, Pane, PitboardModel, PlatformError,
    QuitQuestion, ReadFailure, RestartNeeded, RunOutNotice, RunningSignIn, Sheet, Snapshot,
    WindowRequest, WindowsLaunch,
};

// The fixtures, which the `fixture` feature compiles; what is exported of them is the same
// in every build.
mod fixture;
pub use fixture::{FixtureError, fixture_names, fixture_page};

mod present;
pub use present::{
    AccountItem, AccountSection, AccountWindowsShown, AccountsShown, ActivityLine, ActivityShown,
    AutoSwitchShown, CheckLine, ChecksShown, Choice, CommandLineShown, DownloadShown,
    DownloadState, EmptyList, Footing, ItemAction, ItemOffer, LimitPace, LimitRow, LinkPicker,
    MachineShown, MenuBarText, MenuEntry, MenuNotices, NoticeAction, OpenWindow, PaceStanding,
    PageLoad, PanelNotice, PickerAccount, PickerShown, Question, RenewalShown, ScheduleShown,
    SetupStep, Severity, SheetText, SheetTool, SigningInText, StoreDeletion, StowText,
    WaitingShown, WindowOffer, WindowWaiting, downloads_quit_question, name_to_save,
};

mod account_windows;
pub use account_windows::{
    AlertText, Asker, FrameOrigin, NavigationDecision, NavigationPolicy, NavigationRequest,
    NavigationTarget, PagePermission, PageRole, ProcessEnded, ResponseDecision, ResponseFacts,
    SignInWindowSize, SiteMenu, WindowAccount, WindowNoteKind, after_content_process_ended,
    decide_navigation, decide_response, dialog_title, download_destination, download_host,
    download_question, frame_asker, is_site_page, opening_note, page_may_close, page_may_use,
    remove_data_alert, sign_in_window_size, site_menus, store_id, window_accounts, window_address,
    window_home, window_note, window_of_store,
};

// The core the model's lanes call, made from what the app was started with, and what it
// answers that only the model reads. None of it is exported.
mod launch;
pub(crate) use launch::{
    Adoption, AppCore, AutoLooked, AutoSwitched, Change, Check, Enrolled, EnrolledAs, Holding,
    OwnCommandLine, PitboardError, Remedy, Renewed, SignInSession, Switch, Switched,
};
// What a test or a fixture makes the app's core of, in place of the environment.
#[cfg(any(test, feature = "fixture"))]
pub(crate) use launch::{ASK_AGAIN_AFTER, Made};

/// A tool Pitboard handles, as the app names it to a person.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Tool {
    /// What a label's prefix and every `provider` field say: `claude`, `codex`.
    pub code: String,
    /// As its own documentation names it: `Claude Code`, `Codex`.
    pub name: String,
    /// The command that runs it, which is what a person restarts.
    pub program: String,
    /// The company behind it, which is who is asked about its accounts.
    pub service: String,
}

/// Every tool Pitboard handles, in the order a listing shows them. A snapshot carries those
/// whose program was found, in this order.
pub(crate) fn tools() -> Vec<Tool> {
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
/// `PITBOARD_HOME`, or `.pitboard` in the home. The path as the environment gives it, with
/// any `.`, `..` or trailing `/` it holds.
#[uniffi::export]
pub fn pitboard_directory(environment: HashMap<String, String>) -> String {
    let environment: Environment = environment.into_iter().collect();
    environment.pitboard_home().to_string_lossy().into_owned()
}

/// Something to know about that did not stop the operation.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Warning {
    pub code: String,
    pub message: String,
    /// The account a warning that stands until it is put right is about, as `Account.id`
    /// names it, where its words change while it stands: `login_replaced` names whose login
    /// the tool has stored now, which every switch changes. `None` for every other warning.
    pub account: Option<String>,
    /// What the file a `fallback_login` warning is about holds, which says whether it can be
    /// put away: one Pitboard cannot read cannot. `None` for every other warning.
    pub held: Option<Held>,
}

/// What a file behind a tool's store holds, as far as Pitboard can read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Held {
    /// A login of the tool's, which a session that cannot read that store signs in with.
    Login,
    /// No login of the tool's.
    NoLogin,
    /// It is there, and cannot be read.
    Unreadable,
}

/// An interrupted switch that was given up on, keeping every login it named.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Abandoned {
    pub from: String,
    pub to: String,
    /// Copies kept rather than deleted, because which one is live is now unknown.
    pub logins_kept: u32,
}

/// Whether anything keeps parked logins alive on this machine without a command being run.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Schedule {
    /// The platform's own scheduler runs `pitboard renew` every `every_seconds`.
    Installed { path: String, every_seconds: u32 },
    /// Nothing does. Parked logins are renewed when Pitboard runs, and otherwise not.
    Absent,
    /// This platform has no scheduler Pitboard knows how to write.
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Source {
    Live,
    Remembered,
}

/// One limit an account is measured against, such as the five-hour session or the week.
/// Named for what it is rather than `Window`, which SwiftUI and WinUI each have a type of.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
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

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Usage {
    pub source: Source,
    pub observed_at: Option<i64>,
    pub windows: Vec<Limit>,
    /// Whether `windows` are every limit the account has, so a limit they leave out is one
    /// it does not have.
    pub lists_every_limit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Parked {
    pub parked_at: i64,
    pub access_expires_at: Option<i64>,
    pub refresh_expires_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
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
    /// What Pitboard files the account under, which its windows' stores derive from. Empty
    /// for a login Pitboard cannot place.
    pub account_id: String,
    pub signed_in: bool,
    /// Whether switching to it would work now.
    pub switchable: bool,
    pub parked: Option<Parked>,
    pub usage: Option<Usage>,
    /// Why the usage is not live, when it is not.
    pub stale: Option<String>,
    /// What to tell a person about `stale`, when it is worth a word.
    pub stale_explanation: Option<String>,
    /// What its login says it is on, "Max 20x" or "Plus", where a read says.
    pub plan: Option<String>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Status {
    pub now: i64,
    /// The signed-in account first.
    pub accounts: Vec<Account>,
    pub warnings: Vec<Warning>,
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

/// What a renewal run did, from what renewing each due login came to: "No parked login was
/// due.", "Renewed one.", "Renewed 1 of 2; the rest are tried again next time.".
pub(crate) fn renewal_note(renewals: &[Renewed]) -> String {
    let renewed = renewals
        .iter()
        .filter(|r| r.outcome == switch::Renewal::Renewed.code())
        .count();
    words::renewal_note(renewals.len(), renewed)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Only a renewal that renewed counts as one: a deferred or refused one was due and
    /// was not renewed.
    #[test]
    fn a_renewal_note_counts_what_was_renewed() {
        let renewed = |outcome: &str| Renewed {
            label: "work".into(),
            provider: "claude".into(),
            outcome: outcome.into(),
        };
        assert_eq!(renewal_note(&[]), "No parked login was due.");
        assert_eq!(
            renewal_note(&[renewed("renewed"), renewed("renewal_deferred")]),
            "Renewed 1 of 2; the rest are tried again next time."
        );
        assert_eq!(
            renewal_note(&[renewed("parked_login_refused")]),
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
            doctor_summary(&[check(Level::Ok)]),
            "Everything Pitboard checks is in order."
        );
        assert_eq!(
            doctor_summary(&[check(Level::Warn), check(Level::Ok), check(Level::Fail)]),
            "1 broken: do not switch accounts until fixed."
        );
        assert_eq!(
            doctor_summary(&[check(Level::Warn), check(Level::Warn)]),
            "2 things are worth looking at."
        );
    }

    /// Where an app's Pitboard directory is, which command line an app comes with, and
    /// whether a path is a program, cross the bindings as the core reads them, so the app has
    /// no rule of its own for any of them.
    #[test]
    #[cfg_attr(windows, ignore = "W17: finding programs on Windows")]
    fn the_app_asks_the_core_where_things_are_and_what_runs() {
        let environment = |pairs: &[(&str, &str)]| -> HashMap<String, String> {
            pairs
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect()
        };
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
        assert_eq!(
            app_command_line("/Users/x/pitboard".into()),
            None,
            "not an app"
        );
        assert!(can_run("/bin/sh".into()));
        assert!(!can_run("/bin".into()), "a directory is not a program");
        assert!(!can_run("/nowhere/at/all".into()));
    }
}
