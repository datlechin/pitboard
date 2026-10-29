//! What the menu bar, the menu, the window and the settings show, worked out from what the
//! model knows.
//!
//! [`present`] takes the model's state and the moment it is shown at and makes the whole
//! [`Snapshot`]: the accounts as the model holds them, what is known of the machine, and every
//! sentence and row an app shows of them, so the macOS app and the Windows app say the same
//! thing in the same words and a view has nothing left to decide. It is pure but for one
//! thing: a clock time, and a change's date and time, are the person's to read, in their
//! locale and with their 12 or 24 hours, so it asks the app's [`LocalTime`] for each, which
//! formats and reads nothing else. Text that depends on the time is made again on the model's
//! minute tick, and a countdown is the app's native one, from [`PanelNotice::until`].
//!
//! The words both apps say and the command line does not are in `words/`, each a function
//! of typed values, in the file of the surface that says it. What the command line says too,
//! a limit's names, a reset, a renewal's note and doctor's summary, is `pitboard_core::words`',
//! called from here.
//!
//! A button the snapshot offers comes with its words and what it sends, and with whether it
//! can be pressed now where it can be held back, a `Choice`, an `ItemAction`, an `ItemOffer`,
//! a `NoticeAction` or a `WindowQuestion`'s answers, so a view never words an intent or
//! decides whether to offer one, and both apps label and hold back one offer alike: a row's
//! "Switch", "Sign In Again…" or "Add…", what an account's own menu offers, the setup step's
//! "Add Account…" and "Not Now", an empty pane's "Try Again", a window's "Clear", the quit
//! question's "Quit Claude and Switch" and a sheet's default button, whose intent
//! `check_name` gives. A control an app always
//! has, such as its toolbar's and its menu's "Add Account…", "Refresh", "Settings…" or "Quit
//! Pitboard", is the app's own, in its own catalog, held back where the snapshot says what it
//! waits for is under way, and so is what is about the app's own system or done by the app
//! alone, such as asking for Login Items, copying an email address or showing a file in
//! Finder. So "Add Account…" is said in both places.

mod accounts;
mod app_words;
mod checks;
mod glance;
mod machine;
pub(crate) mod next;
mod notices;
mod quick_actions;
mod removal;
mod setup;
mod sheets;
mod welcome;
mod windows;
pub(crate) mod words;

#[cfg(test)]
pub(crate) mod testing;

pub use app_words::{
    AccountWindowWords, ActivityWords, AppWords, GnomeShortcutWords, GnomeWords, MacMenuBarWords,
    MenuWords, SettingsWords, SheetWords, WindowWords, WindowsNotificationAreaWords,
    account_description, already_there, app_words, countdown, gnome_words, install_update,
    links_to, mac_menu_bar_words, named_version, path_too_long, ready_to_install, still_switching,
    trash_says, usage_of, webview_runtime_too_old, windows_notification_area_words,
};
pub use checks::{CheckGroup, CheckLine, ChecksShown};
#[cfg(test)]
pub(crate) use glance::glance_on;
pub(crate) use glance::{GlanceText, glance, id as glance_id};
pub use machine::{
    ActivityLine, ActivityShown, AtLimitChoice, AtLimitShown, CommandLineShown, EmptyList,
    MachineShown, ScheduleShown, TermsShown, WarmAccountShown, WarmShown, WatchServiceShown,
};
pub use quick_actions::{QuickAction, QuickAsk, outside_refused, search};
pub use removal::{Package, PackageCommand, PackageRemoval, RemovalShown, package_removal};
pub use sheets::check_name;
pub(crate) use sheets::saved_name;
pub use welcome::{WelcomeLogin, WelcomeShown, WelcomeStep, WelcomeTool};
pub use windows::{
    AccountWindowsShown, DownloadShown, DownloadState, LinkPicker, OpenWindow, PageLoad,
    PickerAccount, PickerShown, StoreDeletion, WaitingShown, WindowWaiting,
    downloads_quit_question,
};

pub(crate) use accounts::{in_order, needs_sign_in};
pub(crate) use notices::{
    Clicked, UNUSABLE, at_limit_notice, auto_not_watching_notice, auto_refused_notice,
    auto_skipped_notice, auto_switched_notice, notice_buttons, standing_notice, unusable_notice,
    warm_failed_notice,
};

use crate::account_windows::{AlertText, WindowAccount};
use crate::model::state::State;
use crate::model::{Changed, Intent, LocalTime, Snapshot};
use crate::{Account, AccountKey, Front, Tool, UsageLevel};
use pitboard_core::provider::ProviderId;

/// What a front's status item says of the account in use: the macOS menu bar item beside its
/// mark, the Windows notification area icon by its look and its tooltip.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StatusItemShown {
    // "work 42%", a name past twelve characters cut to eleven and an ellipsis; empty while
    // nobody is signed in
    pub name_and_usage: String,
    // "42%", empty until measured
    pub usage: String,
    // "Pitboard, work 80%, Codex, needs attention", whatever the item shows
    pub spoken: String,
    pub level: Option<UsageLevel>,
    // the menu opens on something to look at
    pub attention: bool,
    // the item's tooltip, where its system shows one, as Windows does: "Pitboard", then a line
    // for each account in use
    pub tip: String,
}

/// What went wrong in the sheet that is up: an alert's words, said inside it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FailureText {
    /// One higher for each failure said, so an app tells a new one from one it has shown.
    pub id: u64,
    /// What was being done: "Couldn’t sign in to work".
    pub title: String,
    /// What went wrong.
    pub message: String,
    /// Everything else it warned about, a sentence each.
    pub warnings: Vec<String>,
    pub next: NextStep,
}

// What to do about a failure: a button that does it, or a link to the docs about its code.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NextStep {
    Act { choice: Choice },
    Learn { link: SetupLink },
}

/// One tool's accounts, under its name: one section, unheaded, while every account shown is
/// one tool's, so a machine with one tool looks as it always did.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AccountSection {
    /// The tool's code, which a view knows the section by.
    pub id: String,
    pub tool: Tool,
    /// The tool's name, once accounts of more than one tool are shown.
    pub heading: Option<String>,
    pub accounts: Vec<AccountItem>,
}

/// One account, as the menu and the window both describe it, so the two cannot disagree.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AccountItem {
    /// The account's `id`, unique among the accounts shown.
    pub id: String,
    /// Its label with its tool, `claude/work`, which a view's identifiers for it are made of.
    /// `None` for a login not enrolled.
    pub qualified: Option<String>,
    /// Its name, or its email address while it has none, or what is wrong with a login
    /// Pitboard cannot use.
    pub title: String,
    /// Its email address; empty for a login Pitboard cannot read.
    pub email: String,
    /// The plan its login says it is on, as the window's tag beside its name: "Team 5x".
    pub plan: Option<String>,
    // "work" on macOS; on Windows its figure or why it can't be switched to: "work (42%)"
    pub menu_title: String,
    /// The row as VoiceOver reads it as one thing: "work (Codex), in use".
    pub spoken: String,
    pub in_use: bool,
    /// Its parked login can no longer be used, whatever else is running.
    pub needs_sign_in: bool,
    pub standing: AccountStanding,
    /// A login of its tool that belongs to no account Pitboard can name.
    pub unplaced: bool,
    /// What the far end of its row in the window shows, where anything.
    pub end: Option<RowEnd>,
    /// What pressing it does, decided once: switch to it, sign in to it again, or name it.
    /// `None` where nothing can be done: it is the one in use, a switch is under way, or
    /// Pitboard cannot use it.
    pub action: Option<ItemAction>,
    /// The one line under its name in the menu: its limits, or what stands in their way.
    pub summary: String,
    // what needs doing about it, the most pressing first
    pub notes: Vec<AccountNote>,
    pub detail: AccountDetail,
    /// What a menu item's help says of it: why it cannot be used, or why its numbers are old.
    pub help: Option<String>,
    /// Each of its limits, in the order its service gave them.
    pub limits: Vec<LimitRow>,
    // everything its own menu offers, in its order, each dimmed while it cannot be chosen
    pub actions: Vec<AccountAction>,
}

// For a view that marks or orders accounts by where each stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AccountStanding {
    InUse,
    Ready,
    // ready, with its parked login due to expire or its Claude Desktop sign-in that may lapse
    Expiring,
    NeedsSignIn,
    // signed in, and not added
    NotAdded,
    // a login Pitboard cannot read, identify or switch
    Unusable,
    Switching,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AccountNote {
    pub kind: NoteKind,
    pub text: String,
    // what does something about it, where anything does
    pub action: Option<AccountAction>,
    // where nothing does, the docs about it
    pub learn: Option<SetupLink>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NoteKind {
    // why it cannot be used
    Problem,
    // its parked login is due to expire, or its Claude Desktop sign-in may lapse
    Expiring,
    // why its numbers are not new
    Stale,
    // a login signed in and not added yet
    Unfinished,
}

// The inspector's on macOS, an expanded row's on Windows: each value beside its sentence.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AccountDetail {
    pub email: Option<String>,
    pub plan: Option<String>,
    // when its parked login stops working, for an account not in use
    pub signed_in_until: Option<i64>,
    // "In use", "Saved sign-in valid until Nov 20", "Sign-in expired"
    pub sign_in: Option<String>,
    // for a Claude Code account Pitboard can warm up
    pub warm: Option<WarmDetail>,
    pub read_at: Option<i64>,
    // for Claude Desktop, "Read 2h 05m ago, with Claude closed"; the rest are read together
    pub read: Option<String>,
    // the account's notes its values don't say: old numbers, or a login Pitboard can't use
    pub notes: Vec<AccountNote>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WarmDetail {
    pub on: bool,
    // "Warms up again at 22:50", "Warming up…", "Off"
    pub said: String,
    pub next_at: Option<i64>,
    // when its five-hour period ends, while one runs
    pub running_until: Option<i64>,
    // "Warm-Up Settings…", to its hours and its toggle
    pub settings: Choice,
}

// From an account's menu, its detail or one of its notes.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AccountAction {
    pub kind: ActionKind,
    pub title: String,
    // sent once any question is answered; none for a window, which the app opens itself
    pub intent: Option<Intent>,
    // what OpenWindow opens
    pub window: Option<WindowAccount>,
    // a shortcut menu leaves out one that cannot be chosen now
    pub enabled: bool,
    pub confirm: Option<Question>,
}

// A view picks an action's symbol and place by this, never by its title.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ActionKind {
    Switch,
    SignInAgain,
    // the login signed in now
    Add,
    Rename,
    WarmUpNow,
    OpenWindow,
    Remove,
}

/// What the far end of an account's row in the window shows.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum RowEnd {
    /// A switch to it is under way: a spinner, which VoiceOver and Narrator name `spoken`.
    Switching { spoken: String },
    /// It is the one in use, said beside a checkmark.
    InUse { title: String },
    /// Its action's button.
    Button { action: ItemAction },
}

// The Account menu's items, dimmed, while no account is selected.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AccountMenuShown {
    pub offers: Vec<String>,
    pub forget: String,
}

/// What pressing an account does.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ItemAction {
    /// What the row's button says: "Switch", "Sign In Again…", "Add…".
    pub title: String,
    /// What VoiceOver says of that button: "Switch to work (Codex)".
    pub spoken: String,
    /// What to send.
    pub intent: Intent,
}

// What a check's fix or Uninstall Pitboard… offers to do, asked first where `confirm` says.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ItemOffer {
    pub title: String,
    // sent once any question has been answered
    pub intent: Intent,
    pub enabled: bool,
    pub confirm: Option<Question>,
}

/// One limit of an account, as a row of its bars.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LimitRow {
    pub kind: LimitKind,
    // as a heading names it: "5-hour limit", "Weekly limit (Fable)"
    pub name: String,
    // beside its bar: "5-hour", "Weekly (Fable)"
    pub short: String,
    /// The share used, as the service said it, for the bar. Past 100 once exceeded.
    pub percent: f64,
    /// The share used in words: "42%".
    pub figure: String,
    // "42% used"
    pub used: String,
    /// The step it is at, which its tint follows.
    pub level: UsageLevel,
    pub resets_at: Option<i64>,
    // "Resets 3:45 PM", "Resets Mon 9:00 AM", "Resetting now", or empty where nothing says
    pub resets: String,
    // "in 2 hr 9 min", or empty: the one part the minute tick moves
    pub resets_in: String,
    // the row as VoiceOver says it: "5-hour limit, 42 percent used, resets 3:45 PM"
    pub spoken: String,
}

// By its length, for a view that tells limits apart by more than their words.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum LimitKind {
    FiveHour,
    Weekly,
    // a weekly limit of one model's
    WeeklyModel { model: String },
    Other { length_seconds: Option<i64> },
}

/// A question asked before something is done that cannot be undone. Not called
/// `Confirmation`, which Swift Testing has a type of, and every Swift test imports Testing.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Question {
    pub title: String,
    pub message: String,
    /// What the button that does it says: "Remove", "Cancel Switch".
    pub confirm: String,
    // only Remove's, as plan 4.4 has it
    pub destructive: bool,
    // Return answers it; Uninstall's is read first, as Apple's alerts guide asks
    pub confirm_is_default: bool,
}

impl Question {
    pub(crate) fn asking(title: String, message: String, confirm: String) -> Question {
        Question {
            title,
            message,
            confirm,
            destructive: false,
            confirm_is_default: true,
        }
    }
}

/// A question the window asks by itself, with what each answer sends. The model takes either
/// before or after the other, so an alert that sends its close before its button's answer
/// loses neither.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WindowQuestion {
    pub question: Question,
    /// Sent by its button.
    pub confirmed: Intent,
    /// Sent by Cancel, Escape, or its going any other way.
    pub declined: Intent,
}

/// How pressing a notice is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, uniffi::Enum)]
pub enum Severity {
    /// Only worth knowing.
    Info,
    /// Worth doing something about.
    Warning,
    /// What stops Pitboard working.
    Error,
}

/// Something Pitboard has to tell somebody that is not an account: an account that ran out
/// with another to switch to, what the last switch means for sessions already open, a read
/// that could not reach a service, a warning, an interrupted switch. Not called `Notice`,
/// which the macOS app already has a type of.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PanelNotice {
    /// Stable while the notice is: "stuck", "read", "advice/claude/work/session/",
    /// "switch/codex", "warning/<code>/<hash>", "abandoned".
    pub id: String,
    pub severity: Severity,
    /// The severity said in a word, where a symbol shows it: "Note", "Warning", "Problem".
    pub spoken_severity: String,
    /// One line, as the menu and the window's heading for it say it.
    pub title: String,
    /// Everything there is to say, a paragraph each.
    pub lines: Vec<String>,
    /// When sessions already open will have followed a switch, in epoch seconds, while that
    /// is still to come: for a countdown the app draws itself, which stops at nought.
    pub until: Option<i64>,
    /// Said before the countdown: "Sessions already open follow in".
    pub until_label: Option<String>,
    /// What can be done about it: buttons, and the one that puts it away.
    pub actions: Vec<NoticeAction>,
    // where no button does anything about a warning or a problem, the docs about it
    pub learn: Option<SetupLink>,
}

/// Something that can be done about a notice.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NoticeAction {
    /// What its button says: "Switch to spare", "Cancel Switch…", "Dismiss".
    pub title: String,
    /// What to send, once any question has been answered.
    pub intent: Intent,
    /// It puts the notice away: an icon at its end rather than a button under it.
    pub dismisses: bool,
    /// It switches account, which a menu offers as an item of its own.
    pub switches: bool,
    /// Whether it can be pressed now: no switch is offered while one is under way.
    pub enabled: bool,
    /// The question asked first, where there is one.
    pub confirm: Option<Question>,
}

/// What the menu says of the notices: a menu has no room for paragraphs, so advice is an item
/// that switches, and everything else one item that opens the window where it is said.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MenuNotices {
    /// No tool Pitboard works with is here: how to install Claude Code.
    pub install: Option<MenuEntry>,
    /// Advice, as items that switch, and an add left halfway, as its way on and its way back.
    pub switches: Vec<MenuEntry>,
    // "Sign In to old Again…", for each account that needs it
    pub sign_ins: Vec<MenuEntry>,
    /// Everything else to know about that is more than a note, as one item.
    pub others: Option<MenuEntry>,
}

/// One item of a menu, as both apps' menus have them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MenuEntry {
    pub title: String,
    /// The line under it.
    pub subtitle: Option<String>,
    /// What its help says.
    pub help: Option<String>,
    /// The severity its symbol shows, where it is about a notice.
    pub severity: Option<Severity>,
    /// What choosing it sends, if anything.
    pub intent: Option<Intent>,
    /// The address choosing it opens, if any.
    pub link: Option<String>,
    pub enabled: bool,
}

/// How far along setting Pitboard up this machine is, across every tool: somebody signed in
/// to Codex is not somebody nobody is signed in to, and a Claude Code account beside a Codex
/// one still has nothing to switch to.
///
/// Somebody who installed only the app has never typed a Pitboard command and may never want
/// to. Every state before `Ready` used to show either a line naming a command to run or
/// nothing at all, which is the same as telling them the app does not work.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Footing {
    /// No tool Pitboard works with was found here, and none has an account or a login here.
    /// Claude Code is the tool it names.
    NoClaudeCode,
    /// No tool has anybody signed in, and nothing is enrolled.
    NoOneSignedIn,
    /// A tool has one account, so there is nothing yet to switch to in it.
    OnlyOne { account: AccountKey },
    /// Codex is installed and nothing of it is enrolled, so nothing of it is read.
    CodexUnused,
    /// Claude Code has two accounts, and nobody chose what happens once a limit reaches the
    /// share.
    AtLimitUnchosen,
    /// Set up, or too early to say.
    Ready,
}

/// The one next thing to do on a machine that is not set up yet, above its accounts.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SetupStep {
    pub title: String,
    /// Why, in a sentence or two.
    pub detail: String,
    /// What can be pressed, the one to press first first.
    pub actions: Vec<Choice>,
}

// A page of the system's own settings, or a tool's install page, which the app opens.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SetupLink {
    pub title: String,
    pub link: String,
}

/// A button, what pressing it sends, and whether it can be pressed now.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Choice {
    pub title: String,
    pub intent: Intent,
    /// Whether it can be pressed now. One held back is shown all the same, and cannot be
    /// pressed.
    pub enabled: bool,
}

/// What the window's accounts pane shows in place of its list, or the list.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum AccountsShown {
    /// No tool is here: nothing Pitboard does means anything until one is.
    NoTool {
        title: String,
        detail: String,
        link_title: String,
        link: String,
    },
    /// A read failed with nothing to list: why, and a way to try again.
    ReadFailed {
        title: String,
        detail: String,
        retry: Choice,
    },
    /// Nobody is signed in to any tool, and nothing is enrolled.
    NoAccounts {
        title: String,
        detail: String,
        add: Choice,
    },
    /// Nothing has been read yet.
    Reading { title: String },
    /// The notices, the setup step and the accounts.
    List,
}

/// What the sheet over the main window says.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SheetText {
    pub title: String,
    /// What the sheet is for, about the tool it starts on.
    pub message: String,
    // The tools Pitboard found here for a new account, the one it starts on whether found or
    // not, and the account's own for signing in again: a picker shows them where there is
    // more than one.
    pub tools: Vec<SheetTool>,
    /// The tool it starts on.
    pub tool: Tool,
    /// The account it is about, where the name is not to be typed: signing in again.
    pub account: Option<String>,
    /// What the name field starts with.
    pub name: String,
    /// What the empty name field shows.
    pub prompt: String,
    /// What its default button says, which does what the sheet is for: "Sign In", "Save",
    /// "Rename". It can be pressed once `check_name` gives an intent, the tool picked is not
    /// missing, and nothing is being saved.
    pub confirm: String,
    /// A name typed in it is being saved, which cannot be withdrawn: nothing in it can be
    /// pressed meanwhile, and it cannot be closed.
    pub saving: bool,
    // what it says under its fields while an app it asked to quit quits, or its account is read
    pub progress: Option<String>,
}

/// What the sheet for putting away the login Claude Code left in a file behind the keychain
/// says: what putting it away does, and once it has looked, what the file holds, whose the
/// login is and what becomes of it, in the words `pitboard stow` asks with.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StowText {
    pub title: String,
    // one sentence: what Clean Up does, and once it has looked, with what the file holds
    pub message: String,
    /// What the sheet says while it looks: "Finding out whose sign-in it is…".
    pub looking: Option<String>,
    /// What its default button says: "Clean Up".
    pub confirm: String,
    /// Whether that button can be pressed: the look found a login to put away, of an
    /// enrolled account or of none, and it is not being put away already.
    pub can_confirm: bool,
    /// It is being put away, which cannot be withdrawn: nothing in it can be pressed
    /// meanwhile, and it cannot be closed.
    pub saving: bool,
}

/// A tool a sheet's picker offers, and what the sheet says once it is picked.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SheetTool {
    pub tool: Tool,
    // the tool's code, which the picker's identifiers are made of
    pub code: String,
    pub name: String,
    pub message: String,
    // what the default button says once this tool is picked
    pub confirm: String,
    // where Pitboard did not find it, so nothing can sign in to it here
    pub missing: Option<Missing>,
    // what a sign-in inside the tool's own app asks of the person, a short sentence each
    pub steps: Vec<String>,
}

/// A tool Pitboard did not find: what it needs, and where to read how to get it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Missing {
    pub title: String,
    pub detail: String,
    pub link_title: String,
    pub link: String,
}

/// A name typed in a sheet: why it cannot be one, or what the sheet's default button sends
/// with it, `None` while there is nothing to save.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NameCheck {
    pub problem: Option<String>,
    pub intent: Option<Intent>,
}

/// What a sign-in under way says, in the sheet it was started from.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SigningInText {
    pub title: String,
    pub message: String,
    /// Under the field for the code the browser shows.
    pub code_note: String,
    /// Why the field is offered again: the tool refused the code typed back.
    pub refused: Option<String>,
    // what VoiceOver calls its spinner
    pub waiting: String,
    // whether it offers the sign-in page's address to copy or open: a sign-in in the browser
    pub offers_link: bool,
    pub cancel: Choice,
    // whether Escape and closing the sheet are its cancel button
    pub escape_cancels: bool,
    // undoes what was done for it, where something was: Put work Back
    pub undo: Option<Choice>,
    // said aloud as it appears: a step that asks for the sign-in again
    pub announce: Option<String>,
    // what a sign-in inside its app asks of the person, a short sentence each
    pub steps: Vec<String>,
}

/// What every part of a snapshot is worked out from.
pub(crate) struct Seen<'a> {
    pub(crate) state: &'a State,
    pub(crate) now: i64,
    local: &'a dyn LocalTime,
    pub(crate) tools: Vec<ProviderId>,
    pub(crate) front: Front,
}

impl<'a> Seen<'a> {
    fn new(state: &'a State, now: i64, local: &'a dyn LocalTime, front: Front) -> Seen<'a> {
        Seen {
            state,
            now,
            local,
            tools: crate::tools_on(front.os()),
            front,
        }
    }

    /// The accounts shown, or none before anything is known.
    pub(crate) fn accounts(&self) -> &[Account] {
        self.state
            .status
            .as_ref()
            .map_or(&[], |status| status.accounts.as_slice())
    }

    /// Whether accounts of more than one tool are shown, which is when anything says which
    /// tool an account is for. A machine with one tool looks exactly as it did before there
    /// were two.
    pub(crate) fn shows_tools(&self) -> bool {
        let mut providers: Vec<ProviderId> = self
            .accounts()
            .iter()
            .map(|account| account.provider)
            .collect();
        providers.sort_unstable();
        providers.dedup();
        providers.len() > 1
    }

    /// The tool's name where a sentence has to say which, once more than one is shown.
    pub(crate) fn tool_if_shown(&self, tool: ProviderId) -> Option<&'static str> {
        self.shows_tools().then(|| tool.name())
    }

    /// Why the last read did not answer, in Pitboard's own words.
    pub(crate) fn problem(&self) -> Option<&str> {
        self.state
            .failure
            .as_ref()
            .map(|failure| failure.message.as_str())
    }

    /// A moment as a clock time in the person's own form, and on another day than now with
    /// its weekday too, so a reset named in a menu that stays open, or is read again an hour
    /// later, is still true. Where the app's own code cannot say, it is said in UTC and named
    /// so, worked out from the moments alone: the machine's own time zone is the app's
    /// `LocalTime`'s to read, and nothing here reads it.
    pub(crate) fn clock(&self, at: i64) -> String {
        clock_of(self.local, at, self.now)
    }

    pub(crate) fn date_of(&self, at: i64) -> String {
        (self.local.date(at)).unwrap_or_else(|_| words::utc_date(at))
    }

    pub(crate) fn time_of_day(&self, minutes: u16) -> Option<String> {
        self.local.time_of_day(minutes).ok()
    }

    /// A time the log keeps, with its offset, as the person's own calendar and clock say it.
    /// The log's own text where it does not read as a time, or where the app's own code
    /// cannot say it, as `pitboard log` prints it, rather than nothing.
    pub(crate) fn date_and_time(&self, logged: &str) -> String {
        pitboard_core::time::parse(logged)
            .and_then(|at| self.local.date_and_time(at).ok())
            .unwrap_or_else(|| logged.to_owned())
    }

    /// The tools a new account can be added for: each whose program was found or that
    /// already has an account here, and Claude Code, the tool a bare label means, when that
    /// is none of them.
    ///
    /// Not every tool then. A program the app did not find where it looks is almost never on
    /// the `PATH` of an app opened from Finder either, so that offered sign-ins that could
    /// not start, and it asked somebody who only ever had Claude Code about Codex too.
    pub(crate) fn addable(&self) -> Vec<ProviderId> {
        let installed = self.state.installed.as_deref().unwrap_or_default();
        let some: Vec<ProviderId> = self
            .tools
            .iter()
            .copied()
            .filter(|tool| {
                installed.contains(tool)
                    || self
                        .accounts()
                        .iter()
                        .any(|account| account.provider == *tool)
            })
            .collect();
        if some.is_empty() {
            self.tools
                .iter()
                .copied()
                .filter(|&tool| tool == ProviderId::Claude)
                .collect()
        } else {
            some
        }
    }

    /// Who is asked about the accounts shown, as a sentence names them: "Anthropic", or
    /// "Anthropic or OpenAI". Where no account shown is of a tool that is asked, whoever could be.
    pub(crate) fn services(&self) -> String {
        let shown: Vec<ProviderId> = self
            .tools
            .iter()
            .copied()
            .filter(|tool| tool.reads_usage())
            .filter(|tool| {
                self.accounts()
                    .iter()
                    .any(|account| account.provider == *tool)
            })
            .collect();
        let asked = if shown.is_empty() {
            self.addable()
                .into_iter()
                .filter(|tool| tool.reads_usage())
                .collect()
        } else {
            shown
        };
        let mut services: Vec<&str> = Vec::new();
        for tool in &asked {
            if !services.contains(&tool.service()) {
                services.push(tool.service());
            }
        }
        if services.is_empty() {
            services.push(ProviderId::Claude.service());
        }
        services.join(" or ")
    }

    pub(crate) fn in_use_of(&self, tool: ProviderId) -> Option<&str> {
        self.accounts()
            .iter()
            .find(|account| account.provider == tool && account.signed_in)
            .and_then(|account| account.label.as_deref())
    }

    /// An account named where nothing around it says which tool it is for: its label, and
    /// once more than one tool is shown, its tool.
    pub(crate) fn name_of(&self, account: &Account) -> String {
        let label = account.label.as_deref().unwrap_or(&account.email);
        if self.shows_tools() {
            words::named_with_tool(label, account.provider.name())
        } else {
            label.to_owned()
        }
    }

    /// Logins signed in to a tool and not enrolled: the ones the app can name by itself. A
    /// login Pitboard could not read or cannot switch is not one of them, however it looks:
    /// naming it would enrol something that can never be switched to.
    pub(crate) fn unnamed(&self) -> impl Iterator<Item = &Account> {
        self.accounts()
            .iter()
            .filter(|account| account.signed_in && account.label.is_none() && !account.unplaced)
    }
}

/// A moment as `local` says it, as [`Seen::clock`] does, as of `now`: doctor's checks say
/// theirs as of when they are made.
pub(crate) fn clock_of(local: &dyn LocalTime, at: i64, now: i64) -> String {
    let today = local.same_day(at, now).unwrap_or(false);
    local
        .clock(at, !today)
        .unwrap_or_else(|_| words::utc_clock(at, now))
}

/// Everything the model has to show, as of `now` in epoch seconds, numbered 0 and naming every
/// part: the actor numbers what it hands on, and the notifier names what changed in what it
/// tells. Asks `local` for each clock time, and nothing else of anyone. What differs by front
/// is the state's front's.
pub(crate) fn present(state: &State, now: i64, local: &dyn LocalTime) -> Snapshot {
    present_on(state.front, state, now, local)
}

/// The same, as `front` says what differs by front: a test's look at another front's.
pub(crate) fn present_on(front: Front, state: &State, now: i64, local: &dyn LocalTime) -> Snapshot {
    let seen = Seen::new(state, now, local, front);
    let footing = setup::footing(&seen);
    let notices = notices::notices(&seen, &footing);
    let menu_notices = notices::menu_notices(&seen, &footing, &notices);
    let accounts_shown = setup::accounts_shown(&seen, &footing);
    let notices = notices::over(&accounts_shown, notices);
    Snapshot {
        revision: 0,
        changed: Changed::every(),
        reading: state.reads > 0,
        quit_question: state
            .asking_for()
            .map(|(question, then)| sheets::quit_question(&seen, question, then)),
        replace_question: state
            .replace_asked()
            .map(|(account, why)| sheets::replace_question(account, why)),
        window_request: state.window,
        signing_in: state.shown_sign_in(),
        sheet: state.sheet.clone(),
        sheet_failure: (state.sheet_failure.as_ref())
            .map(|failure| sheets::failure_text(front, failure)),
        status_item: (state.surfaces.status_item)
            .then(|| accounts::status_item(&seen, &menu_notices)),
        sections: accounts::sections(&seen),
        account_menu: accounts::account_menu(seen.front),
        menu_notices,
        notices,
        setup: setup::step(&seen, &footing),
        accounts_shown,
        menu_accounts_note: setup::menu_accounts_note(&seen, &footing),
        updated_at: state.updated_ms.map(|ms| ms.div_euclid(1000)),
        updated_menu: setup::updated_menu(&seen),
        updated_window: setup::updated_window(&seen),
        footing,
        sheet_text: sheets::sheet_text(&seen),
        signing_in_text: sheets::signing_in_text(&seen),
        stow_text: sheets::stow_text(&seen),
        failure_alert: (state.presented.as_ref())
            .map(|failure| sheets::failure_alert(front, failure)),
        machine: machine::machine(&seen),
        account_windows: windows::account_windows(&seen),
        uninstall: removal::uninstall(&seen),
        removal: removal::removal(&seen),
        welcome: welcome::welcome(&seen),
        widget_revision: state.widget_revision(),
        answers: glance::answers(&seen),
        quick_actions: quick_actions::quick_actions(&seen),
    }
}

/// An alert's words for a failure: its title, and its message with everything else it
/// warned about, a paragraph each.
pub(crate) fn alert_of(title: &str, message: &str, warnings: &[crate::Warning]) -> AlertText {
    let mut paragraphs = vec![message.to_owned()];
    paragraphs.extend(warnings.iter().map(|warning| warning.message.clone()));
    AlertText {
        title: title.to_owned(),
        message: paragraphs.join("\n\n"),
        next: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::state::{Cadence, State};
    use crate::present::testing::{Unreadable, Utc};
    use pitboard_core::state::Key;

    /// 12:00 UTC on Wednesday 14 January 2026.
    const MIDDAY: i64 = 1_768_392_000;

    /// Each system writes a control's words its own way: title case on macOS, as Apple's
    /// guidelines have it, and sentence case on Windows, as Microsoft's have all of an app's
    /// text.
    #[test]
    fn windows_says_its_controls_in_sentence_case() {
        use crate::model::Sheet;
        use crate::model::testing::records::warning;
        use crate::present::testing::{
            account, claude_code, claude_desktop, codex, desktop_account, read, status, strings_of,
            title_cased,
        };
        use pitboard_core::status::Standing;
        let mut state = read(status(Vec::new(), Vec::new()), vec![claude_code()]);
        state.sheet = Some(Sheet::Add { tool: None });
        let words = |front| {
            let shown = present_on(front, &state, MIDDAY, &Utc);
            let AccountsShown::NoAccounts { title, add, .. } = shown.accounts_shown else {
                panic!("{:?}", shown.accounts_shown);
            };
            let sheet = shown.sheet_text.expect("the sheet");
            let empty = shown.machine.activity.empty.expect("no activity");
            [title, add.title, sheet.title, sheet.confirm, empty.title]
        };
        for front in [Front::MacMenuBar, Front::WindowsNotificationArea] {
            assert_eq!(
                words(front),
                [
                    words::no_accounts(front).0,
                    words::add_account(front, true),
                    words::add_account(front, false),
                    words::sign_in_asking(front),
                    words::no_activity(front).0
                ]
            );
        }
        assert_ne!(
            words(Front::MacMenuBar),
            words(Front::WindowsNotificationArea),
            "each system its own case"
        );
        let mut accounts = vec![
            account(Some("work")).signed_in().build(),
            account(Some("old"))
                .standing(Standing::LoginExpired)
                .build(),
            account(None)
                .of(ProviderId::Codex)
                .signed_in()
                .uuid("u")
                .build(),
        ];
        accounts.push(desktop_account("home", false));
        let mut state = read(
            status(accounts, vec![warning("fallback_login", "")]),
            vec![claude_code(), codex(), claude_desktop()],
        );
        state.sheet = Some(Sheet::Add { tool: None });
        let names = [
            "Pitboard",
            "Claude",
            "Code",
            "Codex",
            "Desktop",
            "ChatGPT",
            "Windows",
            "Credential",
            "Manager",
            "PC",
            "Mon",
            "Tue",
            "Wed",
            "Thu",
            "Fri",
            "Sat",
            "Sun",
            "Gmail",
            "Google",
            "Drive",
            "Calendar",
            "Continue",
            "Taskbar",
            "Share",
        ];
        let mut found = Vec::new();
        for said in strings_of(&present_on(
            Front::WindowsNotificationArea,
            &state,
            MIDDAY,
            &Utc,
        )) {
            if !title_cased(&said, &names).is_empty() {
                found.push(said);
            }
        }
        assert!(found.is_empty(), "{found:#?}");
    }

    // Critique 14: one front on every Linux desktop, Plasma's as well as GNOME's, so no word of
    // it names one desktop's settings, nor a place another system has.
    #[test]
    fn no_gnome_word_names_a_desktops_settings_or_another_systems_place() {
        use crate::model::Sheet;
        use crate::model::testing::records::warning;
        use crate::present::testing::{account, claude_code, codex, read, status, strings_of};
        use crate::{Package, Surfaces, gnome_words, notice_buttons, package_removal};
        use pitboard_core::status::Standing;
        const PLACES: [&str; 8] = [
            "settings >",
            "system settings",
            "login items",
            "finder",
            "menu bar",
            "notification area",
            "taskbar",
            "control center",
        ];
        let gnome = Front::Gnome;
        let accounts = vec![
            account(Some("work")).signed_in().build(),
            account(Some("old"))
                .standing(Standing::LoginExpired)
                .build(),
            account(Some("main"))
                .of(ProviderId::Codex)
                .signed_in()
                .uuid("m")
                .build(),
        ];
        let mut said = Vec::new();
        for status_item in [false, true] {
            let mut state = read(
                status(accounts.clone(), vec![warning("fallback_login", "")]),
                vec![claude_code(), codex()],
            );
            state.surfaces = Surfaces { status_item };
            state.sheet = Some(Sheet::Add { tool: None });
            said.extend(strings_of(&present_on(gnome, &state, MIDDAY, &Utc)));
            said.push(words::where_it_lives(gnome, status_item));
        }
        said.extend(strings_of(&app_words(gnome)));
        said.extend(strings_of(&gnome_words()));
        said.extend(strings_of(&notice_buttons(gnome)));
        said.extend(strings_of(&downloads_quit_question(gnome, 2)));
        for package in [Package::Deb, Package::Rpm, Package::Unknown] {
            said.extend(strings_of(&package_removal(package, &[])));
        }
        said.extend([
            words::cannot_schedule(gnome, true),
            words::cannot_link(gnome),
            words::no_scheduler(gnome),
            words::turn_it_off_and_on(gnome),
            words::open_settings(gnome),
            words::warm_up_settings(gnome),
            words::sign_in_again_button(gnome),
            words::switch_button(gnome, "personal"),
            words::uninstall_pitboard(gnome),
        ]);
        let named: Vec<&String> = said
            .iter()
            .filter(|said| {
                let said = said.to_lowercase();
                PLACES.iter().any(|place| said.contains(place))
            })
            .collect();
        assert!(named.is_empty(), "{named:#?}");
    }

    /// A moment later today is a clock time, and one on another day names its day as well,
    /// whether it is days away or a minute past midnight: which day it falls on decides, not
    /// how far off it is. The person's own calendar says which day that is.
    ///
    /// The clock is read in UTC.
    #[test]
    fn a_clock_time_names_its_day_only_when_it_is_not_today() {
        let state = State::new(Front::MacMenuBar, Cadence::APP);
        let seen = Seen::new(&state, MIDDAY, &Utc, Front::MacMenuBar);
        assert_eq!(seen.clock(MIDDAY + 6 * 3600), "18:00");
        assert_eq!(seen.clock(MIDDAY - 11 * 3600), "01:00");
        assert_eq!(seen.clock(MIDDAY + 6 * 3600 + 86_400), "Thu 18:00");
        assert_eq!(seen.clock(MIDDAY + 12 * 3600 + 60), "Thu 00:01");
    }

    /// A clock the app's own code cannot read is said in UTC and named so, worked out from
    /// the moments alone: the same whatever zone the machine running this is in, since
    /// nothing but the app's `LocalTime` reads that zone.
    #[test]
    fn a_clock_the_app_cannot_read_is_said_in_utc() {
        let state = State::new(Front::MacMenuBar, Cadence::APP);
        let seen = Seen::new(&state, MIDDAY, &Unreadable, Front::MacMenuBar);
        assert_eq!(seen.clock(MIDDAY + 2 * 3600), "14:00 UTC");
        assert_eq!(seen.clock(MIDDAY + 3 * 86_400), "Sat 12:00 UTC");
    }

    // A day is the person's calendar's, and UTC's, named so, where the app's code can't say it.
    #[test]
    fn a_date_is_the_persons_or_utcs_named_so() {
        let state = State::new(Front::MacMenuBar, Cadence::APP);
        let at = MIDDAY + 37 * 86_400;
        assert_eq!(
            Seen::new(&state, MIDDAY, &Utc, Front::MacMenuBar).date_of(at),
            "Feb 20"
        );
        assert_eq!(
            Seen::new(&state, MIDDAY, &Unreadable, Front::MacMenuBar).date_of(at),
            "Feb 20 UTC"
        );
    }

    /// The log keeps local time with its offset. The same moment written in three time zones
    /// is one moment, said as the person's own calendar and clock say it, and text that is
    /// not a time is said as the log keeps it, as is a time the app's own code cannot say.
    #[test]
    fn a_changes_time_is_read_with_its_offset() {
        let state = State::new(Front::MacMenuBar, Cadence::APP);
        let seen = Seen::new(&state, MIDDAY, &Utc, Front::MacMenuBar);
        let moment = Utc
            .date_and_time(1_790_492_709)
            .expect("the test's own clock");
        for logged in [
            "2026-09-27T14:05:09+07:00",
            "2026-09-27T07:05:09+00:00",
            "2026-09-27T02:05:09-05:00",
        ] {
            assert_eq!(seen.date_and_time(logged), moment, "{logged}");
        }
        assert_eq!(moment, "Sun 07:05");
        assert_eq!(seen.date_and_time("yesterday"), "yesterday");
        assert_eq!(seen.date_and_time(""), "");

        let unreadable = Seen::new(&state, MIDDAY, &Unreadable, Front::MacMenuBar);
        assert_eq!(
            unreadable.date_and_time("2026-09-27T14:05:09+07:00"),
            "2026-09-27T14:05:09+07:00"
        );
    }

    // Plan 4.4: the verb answers Return, red only on Remove; Uninstall asks to be read first.
    #[test]
    fn only_removes_answer_is_destructive_and_only_uninstall_has_no_default() {
        use crate::present::testing::{account, claude_code, read, status};
        let accounts = vec![
            account(Some("work")).signed_in().build(),
            account(Some("spare")).build(),
        ];
        let mut state = read(status(accounts, Vec::new()), vec![claude_code()]);
        state.stuck = true;
        let shown = present_on(Front::MacMenuBar, &state, MIDDAY, &Utc);
        let asked = |question: &Question| (question.destructive, question.confirm_is_default);
        let remove = (shown.sections[0].accounts[1].actions.iter())
            .find(|action| action.kind == ActionKind::Remove)
            .and_then(|action| action.confirm.clone())
            .expect("spare may be removed");
        assert_eq!(asked(&remove), (true, true));
        let cancel = (shown.notices.iter())
            .flat_map(|notice| &notice.actions)
            .find_map(|action| action.confirm.clone())
            .expect("Cancel Switch asks");
        assert_eq!(asked(&cancel), (false, true));
        let replace = sheets::replace_question(
            &Key::new(ProviderId::Claude, "spare"),
            "Anthropic refuses it.",
        );
        assert_eq!(asked(&replace.question), (false, true));
        let quit = downloads_quit_question(Front::MacMenuBar, 1).expect("a download runs");
        assert_eq!(asked(&quit), (false, true));
        let uninstall = shown.uninstall.confirm.expect("Uninstall asks");
        assert_eq!(asked(&uninstall), (false, false));
    }

    #[test]
    fn each_service_is_named_once() {
        use crate::present::testing::{
            account, claude_code, claude_desktop, codex, desktop, desktop_account, read, status,
        };
        let claude = account(Some("work")).signed_in().build();
        let state = read(
            status(
                vec![claude.clone(), desktop_account("work", true)],
                Vec::new(),
            ),
            vec![claude_code(), claude_desktop()],
        );
        assert_eq!(
            Seen::new(&state, MIDDAY, &Utc, Front::MacMenuBar).services(),
            "Anthropic"
        );
        let state = read(
            status(
                vec![
                    claude,
                    account(Some("main"))
                        .of(ProviderId::Codex)
                        .signed_in()
                        .build(),
                    desktop_account("work", true),
                ],
                Vec::new(),
            ),
            desktop(),
        );
        assert_eq!(
            Seen::new(&state, MIDDAY, &Utc, Front::MacMenuBar).services(),
            "Anthropic or OpenAI"
        );
        let state = read(
            status(
                vec![
                    account(Some("main"))
                        .of(ProviderId::Codex)
                        .signed_in()
                        .build(),
                    desktop_account("work", true),
                ],
                Vec::new(),
            ),
            desktop(),
        );
        assert_eq!(
            Seen::new(&state, MIDDAY, &Utc, Front::MacMenuBar).services(),
            "OpenAI",
            "nobody is asked about a Claude Desktop login"
        );
        let state = read(
            status(Vec::new(), Vec::new()),
            vec![codex(), claude_desktop()],
        );
        assert_eq!(
            Seen::new(&state, MIDDAY, &Utc, Front::MacMenuBar).services(),
            "OpenAI"
        );
        let state = read(
            status(vec![desktop_account("work", true)], Vec::new()),
            vec![claude_desktop()],
        );
        assert_eq!(
            Seen::new(&state, MIDDAY, &Utc, Front::MacMenuBar).services(),
            "Anthropic",
            "never nobody"
        );
    }
}
