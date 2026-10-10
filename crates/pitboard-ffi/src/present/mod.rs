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
//! The words both apps say and the command line does not are in `words.rs`, each a function
//! of typed values. What the command line says too, a limit's names, a reset, a pace, a
//! parked login's life, a renewal's note and doctor's summary, is `pitboard_core::words`',
//! called from here.
//!
//! A button the snapshot offers comes with its words and what it sends, and with whether it
//! can be pressed now where it can be held back, a `Choice`, an `ItemAction`, an `ItemOffer`
//! or a `NoticeAction`, so a view never words an intent or decides whether to offer one, and
//! both apps label and hold back one offer alike: a row's "Use", "Sign In Again…" or "Name…",
//! what an account's own menu offers, the setup step's "Add Account…" and "Not Now", an empty
//! pane's "Try Again", a window's "Clear" and a sheet's default button. A control an app always
//! has, such as its toolbar's and its menu's "Add Account…", "Refresh", "Settings…" or "Quit
//! Pitboard", is the app's own, in its own catalog, held back where the snapshot says what it
//! waits for is under way, and so is what is about the app's own system or done by the app
//! alone, such as asking for Login Items, copying an email address or showing a file in
//! Finder. So "Add Account…" is said in both places.

mod accounts;
mod machine;
mod notices;
mod setup;
mod sheets;
mod windows;
pub(crate) mod words;

#[cfg(test)]
pub(crate) mod testing;

pub use machine::{
    ActivityLine, ActivityShown, AutoSwitchShown, CheckLine, ChecksShown, CommandLineShown,
    EmptyList, MachineShown, RenewalShown, ScheduleShown,
};
pub use sheets::name_to_save;
pub use windows::{
    AccountWindowsShown, DownloadShown, DownloadState, LinkPicker, OpenWindow, PageLoad,
    PickerAccount, PickerShown, StoreDeletion, WaitingShown, WindowWaiting,
    downloads_quit_question,
};

pub(crate) use accounts::in_order;
pub(crate) use notices::{
    auto_not_watching_notice, auto_refused_notice, auto_skipped_notice, auto_switched_notice,
    run_out_notice, standing_notice,
};

use crate::account_windows::{AlertText, WindowAccount};
use crate::model::state::State;
use crate::model::{Intent, LocalTime, Snapshot};
use crate::{Account, Tool, UsageLevel};
use pitboard_core::host::{OS, Os};
use pitboard_core::provider::ProviderId;

/// What the menu bar item says beside its mark, in each of the forms an app's setting can
/// choose: the name and the figure, the figure alone, or nothing beside the mark. Not called
/// `MenuBar`, which WinUI 3 has a control of, so that a C# file using both namespaces would
/// find the name ambiguous.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MenuBarText {
    /// "work 42%": the account the bar is about and its headline limit, a name past twelve
    /// characters cut to eleven and an ellipsis. Empty before anything is known and while
    /// nobody is signed in.
    pub name_and_usage: String,
    /// "42%", or empty until a figure is measured.
    pub usage: String,
    /// What VoiceOver says of the item, whatever it shows: "Pitboard, work 42%", and the
    /// tool too once there is more than one, "Pitboard, work 80%, Codex".
    pub spoken: String,
}

/// One tool's accounts, under its name: one section, unheaded, while every account shown is
/// one tool's, so a machine with one tool looks as it always did.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AccountSection {
    /// The tool's code.
    pub id: String,
    /// The tool's name, once accounts of more than one tool are shown.
    pub heading: Option<String>,
    pub accounts: Vec<AccountItem>,
}

/// One account, as the menu and the window both describe it, so the two cannot disagree.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AccountItem {
    /// The account's `id`, unique among the accounts shown.
    pub id: String,
    /// The tool, as a `Tool`'s code.
    pub provider: String,
    /// Its label with its tool, for what is asked of it. `None` for a login not enrolled.
    pub qualified: Option<String>,
    /// Its name, or its email address while it has none, or what is wrong with a login
    /// Pitboard cannot use.
    pub title: String,
    /// Its email address; empty for a login Pitboard cannot read.
    pub email: String,
    /// The plan its login says it is on, as a tag beside its name: "Team 5x", "Plus".
    pub plan: Option<String>,
    /// Its name where nothing around it says which tool it is for, as VoiceOver reads a row
    /// apart from its heading: "work", or "work (Codex)" beside another tool's accounts.
    pub spoken_name: String,
    /// The row as VoiceOver reads it as one thing: "work (Codex), in use".
    pub spoken: String,
    pub in_use: bool,
    /// Its parked login can no longer be used, whatever else is running.
    pub needs_sign_in: bool,
    /// A login of its tool that belongs to no account Pitboard can name.
    pub unplaced: bool,
    /// A switch to it is under way.
    pub switching: bool,
    /// What pressing it does, decided once: switch to it, sign in to it again, or name it.
    /// `None` where nothing can be done: it is the one in use, a switch is under way, or
    /// Pitboard cannot use it.
    pub action: Option<ItemAction>,
    /// The one line under its name in the menu: its limits, or what stands in their way.
    pub summary: String,
    /// Why it cannot be used, in full, when that is so.
    pub problem: Option<String>,
    /// Why its numbers are not new, where it can be used all the same.
    pub stale_note: Option<String>,
    /// Which limit of the account in use runs out first at its pace, where one runs out
    /// before it resets: "Weekly limit runs out in 20h 18m at this pace".
    pub pace: Option<String>,
    /// How long its parked login stays usable, for an account not in use.
    pub parked_note: Option<String>,
    /// What a menu item's help says of it: why it cannot be used, or why its numbers are old.
    pub help: Option<String>,
    /// Each of its limits, in the order its service gave them.
    pub limits: Vec<LimitRow>,
    /// What its own menu offers to do to it, in order: switching to it, or naming it, where
    /// pressing it does that; then, for an account Pitboard has a name for, signing in to it
    /// again and renaming it. Signing in again is offered once, here, and held back while
    /// another sign-in runs.
    pub offers: Vec<ItemOffer>,
    /// Its windows, one for each site of its tool, as its own menu offers to open them.
    pub windows: Vec<WindowOffer>,
    /// Forgetting it, with the question asked first, where it may be forgotten: enrolled,
    /// and not the one in use, whose record is the only one of who is signed in, and which
    /// the core refuses. What its own menu offers last, and what deleting its row does.
    pub forget: Option<ItemOffer>,
}

/// What pressing an account does.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ItemAction {
    /// What the row's button says: "Use", "Sign In Again…", "Name…".
    pub title: String,
    /// What VoiceOver says of that button: "Use work (Codex)".
    pub spoken: String,
    /// What to send.
    pub intent: Intent,
}

/// Something an account's own menu offers to do to it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ItemOffer {
    /// What its item says: "Use work", "Name…", "Sign In Again…", "Rename…", "Forget…".
    pub title: String,
    /// What to send, once any question has been answered.
    pub intent: Intent,
    /// Whether it can be chosen now. One held back is shown all the same, and cannot be
    /// chosen.
    pub enabled: bool,
    /// The question asked first, where there is one.
    pub confirm: Option<Question>,
}

/// One of an account's windows, as its own menu offers to open it. Opening a window is the
/// app's own doing; the app says it has with `Intent::WindowOpened`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WindowOffer {
    /// What its item says: "Open chatgpt.com".
    pub title: String,
    /// The window it opens, as the menus and the picker list it.
    pub window: WindowAccount,
}

/// One limit of an account, as a row of its bars.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LimitRow {
    /// As a sentence names it, with its scope: "5-hour", "weekly Fable".
    pub name: String,
    /// As the column beside its bar names it: "5h", "week · Fable".
    pub short: String,
    /// The share used, as the service said it, for the bar. Past 100 once exceeded.
    pub percent: f64,
    /// The share used in words: "42%".
    pub figure: String,
    /// The step it is at, which its tint follows.
    pub level: UsageLevel,
    /// When it resets: "resets in 2h 05m", "resetting now", or empty where nothing says.
    pub resets: String,
    /// How it compares with an even use of it, where that means something.
    pub pace: Option<LimitPace>,
    /// The whole row as VoiceOver says it: "5-hour limit, 42 percent used, 38 percent under
    /// an even pace, resets in 3 hours".
    pub spoken: String,
}

/// How a limit compares with using it evenly across its window, as of the moment it is
/// shown: what its bar marks, and the words beside it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LimitPace {
    /// Where on its bar an even use would be by now, in percent of the limit: where the mark
    /// goes.
    pub expected: f64,
    /// Which side of an even pace it is on, which the mark's tint follows.
    pub standing: PaceStanding,
    /// Beside its bar: "27% over pace", "13% under pace", "on pace".
    pub said: String,
    /// What its bar's help says: what an even pace would have used, and for a limit over
    /// pace, when it runs out.
    pub help: String,
}

/// Which side of an even pace a limit is on. Over and under are a mark on its bar, red and
/// green; at an even pace there is no mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PaceStanding {
    Under,
    Even,
    Over,
}

/// A question asked before something is done that cannot be undone. Not called
/// `Confirmation`, which Swift Testing has a type of, and every Swift test imports Testing.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Question {
    pub title: String,
    pub message: String,
    /// What the button that does it says: "Forget", "Give Up".
    pub confirm: String,
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
}

/// Something that can be done about a notice.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NoticeAction {
    /// What its button says: "Switch to spare", "Give Up…", "Dismiss".
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
    /// Advice, as items that switch.
    pub switches: Vec<MenuEntry>,
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
    /// Somebody is signed in to a tool and Pitboard has not been told what to call them,
    /// so their login cannot be parked yet.
    Unnamed { provider: String, email: String },
    /// A tool has one account, so there is nothing yet to switch to in it.
    OnlyOne { provider: String, label: String },
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
    /// The tools a picker offers, each with what the sheet says about it, where there is
    /// more than one to pick from.
    pub tools: Vec<SheetTool>,
    /// The tool it starts on, as a `Tool`'s code.
    pub tool: String,
    /// The account it is about, where the name is not to be typed: signing in again.
    pub account: Option<String>,
    /// What the name field starts with.
    pub name: String,
    /// What the empty name field shows.
    pub prompt: String,
    /// Why a tool is missing from the picker, rather than leaving it out without a word.
    pub not_offered: Option<String>,
    /// What its default button says, which does what the sheet is for: "Sign In", "Save",
    /// "Rename". It can be pressed once `name_to_save` gives a name, and not while saving.
    pub confirm: String,
    /// A name typed in it is being saved, which cannot be withdrawn: nothing in it can be
    /// pressed meanwhile, and it cannot be closed.
    pub saving: bool,
}

/// What the sheet for putting away the login Claude Code left in a file behind the keychain
/// says: what putting it away does, and once it has looked, what the file holds, whose the
/// login is and what becomes of it, in the words `pitboard stow` asks with.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StowText {
    pub title: String,
    /// What putting it away is for.
    pub message: String,
    /// What the look found, a paragraph each. Empty while it looks, and where it could not
    /// look, which the sheet's failure says.
    pub lines: Vec<String>,
    /// What the sheet says while it looks: "Finding out whose login it is…".
    pub looking: Option<String>,
    /// What its default button says: "Put Away".
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
    pub code: String,
    pub name: String,
    pub message: String,
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
}

/// What every part of a snapshot is worked out from.
pub(crate) struct Seen<'a> {
    pub(crate) state: &'a State,
    pub(crate) now: i64,
    local: &'a dyn LocalTime,
    pub(crate) tools: Vec<Tool>,
    pub(crate) os: Os,
}

impl<'a> Seen<'a> {
    fn new(state: &'a State, now: i64, local: &'a dyn LocalTime, os: Os) -> Seen<'a> {
        Seen {
            state,
            now,
            local,
            tools: crate::tools(),
            os,
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
        let mut providers: Vec<&str> = self
            .accounts()
            .iter()
            .map(|account| account.provider.as_str())
            .collect();
        providers.sort_unstable();
        providers.dedup();
        providers.len() > 1
    }

    pub(crate) fn tool(&self, code: &str) -> Option<&Tool> {
        self.tools.iter().find(|tool| tool.code == code)
    }

    /// A tool's name, or its code where Pitboard does not list it.
    pub(crate) fn tool_name(&self, code: &str) -> String {
        self.tool(code)
            .map_or_else(|| code.to_owned(), |tool| tool.name.clone())
    }

    /// The tool's name where a sentence has to say which, once more than one is shown.
    pub(crate) fn tool_if_shown(&self, code: &str) -> Option<String> {
        self.shows_tools().then(|| self.tool_name(code))
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
        let today = self.local.same_day(at, self.now).unwrap_or(false);
        self.local
            .clock(at, !today)
            .unwrap_or_else(|_| words::utc_clock(at, self.now))
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
    pub(crate) fn addable(&self) -> Vec<Tool> {
        let installed = self.state.installed.as_deref().unwrap_or_default();
        let some: Vec<Tool> = self
            .tools
            .iter()
            .filter(|tool| {
                installed.iter().any(|found| found.code == tool.code)
                    || self
                        .accounts()
                        .iter()
                        .any(|account| account.provider == tool.code)
            })
            .cloned()
            .collect();
        if some.is_empty() {
            self.tools
                .iter()
                .filter(|tool| tool.code == ProviderId::Claude.code())
                .cloned()
                .collect()
        } else {
            some
        }
    }

    /// Who is asked about the accounts shown, as a sentence names them: "Anthropic", or
    /// "Anthropic or OpenAI". Before there are accounts, whoever could be.
    pub(crate) fn services(&self) -> String {
        let asked: Vec<Tool> = if self.accounts().is_empty() {
            self.addable()
        } else {
            self.tools
                .iter()
                .filter(|tool| {
                    self.accounts()
                        .iter()
                        .any(|account| account.provider == tool.code)
                })
                .cloned()
                .collect()
        };
        asked
            .iter()
            .map(|tool| tool.service.as_str())
            .collect::<Vec<_>>()
            .join(" or ")
    }

    /// An account named where nothing around it says which tool it is for: its label, and
    /// once more than one tool is shown, its tool.
    pub(crate) fn name_of(&self, account: &Account) -> String {
        let label = account.label.as_deref().unwrap_or(&account.email);
        if self.shows_tools() {
            words::named_with_tool(label, &self.tool_name(&account.provider))
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

/// Everything the model has to show, as of `now` in epoch seconds, numbered 0: the actor
/// numbers what it hands on. Asks `local` for each clock time, and nothing else of anyone.
pub(crate) fn present(state: &State, now: i64, local: &dyn LocalTime) -> Snapshot {
    present_on(OS, state, now, local)
}

/// The same, as `os` says what differs by system.
pub(crate) fn present_on(os: Os, state: &State, now: i64, local: &dyn LocalTime) -> Snapshot {
    let seen = Seen::new(state, now, local, os);
    let footing = setup::footing(&seen);
    let notices = notices::notices(&seen, &footing);
    Snapshot {
        revision: 0,
        now,
        reading: state.reads > 0,
        updated_at: state.updated_ms.map(|ms| ms.div_euclid(1000)),
        status: state.status.clone(),
        warnings: state.warnings.clone(),
        read_failure: state.failure.clone(),
        stuck: state.stuck,
        installed: state.installed.clone(),
        switch_under_way: state.switch_under_way().map(str::to_owned),
        quit_question: state.asking().cloned(),
        last_switches: state.last_switches.clone(),
        abandoned: state.abandoned.clone(),
        failure: state.presented.clone(),
        window_request: state.window,
        signing_in: state.shown_sign_in(),
        sheet: state.sheet.clone(),
        sheet_failure: state.sheet_failure.clone(),
        menu_bar: accounts::menu_bar(&seen),
        sections: accounts::sections(&seen),
        shows_tools: seen.shows_tools(),
        menu_notices: notices::menu_notices(&seen, &footing, &notices),
        notices,
        setup: setup::step(&seen, &footing),
        accounts_shown: setup::accounts_shown(&seen, &footing),
        menu_accounts_note: setup::menu_accounts_note(&seen, &footing),
        updated_menu: setup::updated_menu(&seen),
        updated_window: setup::updated_window(&seen),
        footing,
        sheet_text: sheets::sheet_text(&seen),
        signing_in_text: sheets::signing_in_text(&seen),
        stow_text: sheets::stow_text(&seen),
        quit_confirmation: state.asking().map(sheets::quit_confirmation),
        failure_alert: state.presented.as_ref().map(sheets::failure_alert),
        machine: machine::machine(&seen),
        account_windows: windows::account_windows(&seen),
    }
}

/// A notice's severity, said in a word where a symbol shows it: if two ever sounded the
/// same, an error would pass for a note.
pub(crate) fn spoken_severity(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "Note",
        Severity::Warning => "Warning",
        Severity::Error => "Problem",
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::state::{Cadence, State};
    use crate::present::testing::{Unreadable, Utc};

    /// 12:00 UTC on Wednesday 14 January 2026.
    const MIDDAY: i64 = 1_768_392_000;

    /// A moment later today is a clock time, and one on another day names its day as well,
    /// whether it is days away or a minute past midnight: which day it falls on decides, not
    /// how far off it is. The person's own calendar says which day that is.
    ///
    /// PresentationTests.swift's aClockTimeNamesItsDayOnlyWhenItIsNotToday, in UTC.
    #[test]
    fn a_clock_time_names_its_day_only_when_it_is_not_today() {
        let state = State::new(Cadence::APP);
        let seen = Seen::new(&state, MIDDAY, &Utc, OS);
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
        let state = State::new(Cadence::APP);
        let seen = Seen::new(&state, MIDDAY, &Unreadable, OS);
        assert_eq!(seen.clock(MIDDAY + 2 * 3600), "14:00 UTC");
        assert_eq!(seen.clock(MIDDAY + 3 * 86_400), "Sat 12:00 UTC");
    }

    /// The log keeps local time with its offset. The same moment written in three time zones
    /// is one moment, said as the person's own calendar and clock say it, and text that is
    /// not a time is said as the log keeps it, as is a time the app's own code cannot say.
    ///
    /// PresentationTests.swift's aChangesTimeIsReadWithItsOffset.
    #[test]
    fn a_changes_time_is_read_with_its_offset() {
        let state = State::new(Cadence::APP);
        let seen = Seen::new(&state, MIDDAY, &Utc, OS);
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

        let unreadable = Seen::new(&state, MIDDAY, &Unreadable, OS);
        assert_eq!(
            unreadable.date_and_time("2026-09-27T14:05:09+07:00"),
            "2026-09-27T14:05:09+07:00"
        );
    }
}
