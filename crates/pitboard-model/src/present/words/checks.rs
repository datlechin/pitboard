//! What Diagnostics says of doctor's checks: each check's name and level, what it found in the
//! apps' words, and when the checks were made.

use super::{issues, prose_capitalised, this_machine, titled, typeset, without_commands};
use crate::Front;
use crate::Level;
use pitboard_core::words::capitalised;

pub(crate) fn show_diagnostics(front: Front) -> String {
    titled(front, "Show Diagnostics", "Show diagnostics")
}

/// What stands in for doctor's checks before there are any: "Checking this Mac…".
pub(crate) fn checking(front: Front) -> String {
    format!("Checking {}…", this_machine(front))
}

/// When the checks shown were made, at a clock time the person's clock says: "Checked at
/// 14:05".
pub(crate) fn checked_at(clock: &str) -> String {
    format!("Checked at {clock}")
}

// The line over Diagnostics: "2 Issues", and under it what the rest come to.
pub(crate) fn check_issues(front: Front, count: usize) -> String {
    if count == 0 {
        titled(front, "No Issues", "No issues")
    } else {
        issues(front, count)
    }
}

pub(crate) fn checks_verdict(issues: usize, failed: usize) -> &'static str {
    match (issues, failed) {
        (0, _) => "Everything Pitboard checks is in order.",
        (_, 0) => "Everything else Pitboard checks is in order.",
        (_, 1) => "Don’t switch accounts until the failed check is fixed.",
        _ => "Don’t switch accounts until the failed checks are fixed.",
    }
}

/// A check's level, said in a word where a symbol shows it: a check that reads "state: fine"
/// without saying whether it passed is the same as not running it, and if two levels ever
/// sounded the same a broken check would pass for one that passed.
pub(crate) fn spoken_level(level: Level) -> &'static str {
    match level {
        Level::Ok => "Passed",
        Level::Warn => "Warning",
        Level::Fail => "Failed",
    }
}

// Diagnostics names a check under its tool's heading; the command line keeps doctor's names,
// which maintenance-commands.mdx documents.
pub(crate) fn check_name(front: Front, code: &str, name: &str) -> String {
    let (title, sentence) = match code {
        "system_too_old" => ("Windows Version", "Windows version"),
        "elevated" => ("Permissions", "Permissions"),
        "homes" => ("Folders", "Folders"),
        "security_tool" => ("Keychain", "Keychain"),
        "config_file" => ("Config File", "Config file"),
        "identity" => ("Account in Config", "Account in config"),
        "in_use" | "codex_login" | "desktop_login" => ("Signed-In Account", "Signed-in account"),
        "slot" => ("Sign-In Entry", "Sign-in entry"),
        "credential_size" => ("Sign-In Size", "Sign-in size"),
        "credential_store" | "codex_backend" => ("Sign-In Store", "Sign-in store"),
        "credential" => ("Sign-In", "Sign-in"),
        "fallback_login" => ("Leftover Sign-In File", "Leftover sign-in file"),
        "home" => ("Pitboard Folder", "Pitboard folder"),
        "private_on_disk" => ("Sign-Ins on Disk", "Sign-ins on disk"),
        "vault" | "desktop_parks_folder" => ("Saved Sign-Ins", "Saved sign-ins"),
        "state" => ("Accounts", "Accounts"),
        "interrupted_switch" | "desktop_move" => {
            ("Switch That Didn’t Finish", "Switch that didn’t finish")
        }
        "discarded" => ("Old Saved Sign-Ins", "Old saved sign-ins"),
        "machine_id" => ("Machine ID", "Machine ID"),
        "local_app_data" => ("Local App Data", "Local app data"),
        "roaming_profile" => ("Roaming Profile", "Roaming profile"),
        "pitboard_copies" => ("Command Line Copies", "Command line copies"),
        "system_proxy" => ("Windows Proxy", "Windows proxy"),
        "storage_v5" => ("New Sign-In Storage", "New sign-in storage"),
        "claude_daemon" => ("Daemon", "Daemon"),
        "pending_parks" => (
            "Saved Sign-Ins Being Deleted",
            "Saved sign-ins being deleted",
        ),
        "schedule" => return keep_saved_sign_ins_fresh(front),
        "watch_service" => ("Watch Service", "Watch service"),
        "schedule_proxy" => ("Proxy for Fresh Sign-Ins", "Proxy for fresh sign-ins"),
        "claude_version" | "codex_version" | "desktop_build" => ("Version", "Version"),
        "status_line" => ("Status Line", "Status line"),
        "auth_source" => ("What Sessions Sign In With", "What sessions sign in with"),
        "login_restrictions" | "codex_login_restrictions" => {
            ("Sign-In Restrictions", "Sign-in restrictions")
        }
        "asking" => ("Reading Usage", "Reading usage"),
        "network" => ("Network", "Network"),
        "codex_installer_home" => ("Codex Folders", "Codex folders"),
        "codex_auth_file" => ("Sign-In File", "Sign-in file"),
        "codex_running" => ("Open Sessions", "Open sessions"),
        "desktop_data_folder" => ("Data Folder", "Data folder"),
        "desktop_settings" => ("Settings File", "Settings file"),
        "desktop_layout" => ("Data Folder Layout", "Data folder layout"),
        "desktop_awaiting" => ("Sign-In in Claude", "Sign-in in Claude"),
        "desktop_set_aside" => (
            "Folders Moved Out of the Way",
            "Folders moved out of the way",
        ),
        "notification_not_posted" => (NOTIFICATIONS, NOTIFICATIONS),
        _ => return typeset(&capitalised(name)),
    };
    titled(front, title, sentence)
}

pub(crate) fn keep_saved_sign_ins_fresh(front: Front) -> String {
    titled(
        front,
        "Keep Saved Sign-Ins Fresh",
        "Keep saved sign-ins fresh",
    )
}

// The check the app adds after doctor's for a notification its system refused.
pub(crate) const NOTIFICATIONS: &str = "Notifications";

pub(crate) fn not_posted(title: &str, clock: &str, reason: &str) -> String {
    format!("“{title}” not posted at {clock} · {reason}")
}

// Diagnostics' words where doctor's say what only the command line names, or a path.
pub(crate) const KEYCHAIN_READY: &str = "Ready";

pub(crate) fn folder_private(front: Front) -> &'static str {
    match front {
        Front::WindowsNotificationArea => "Only your Windows account can open it",
        Front::MacMenuBar | Front::Gnome => "Only you can read it",
    }
}

pub(crate) fn folder_shared(front: Front) -> String {
    match front {
        Front::WindowsNotificationArea => "Other accounts on this PC can open it.".into(),
        Front::MacMenuBar | Front::Gnome => {
            format!("Other people on {} can read it.", this_machine(front))
        }
    }
}

pub(crate) const FOLDER_NAMES_ACCOUNTS: &str = "It names your accounts, so only you should.";

pub(crate) fn sign_in_files_shared(front: Front) -> String {
    match front {
        Front::WindowsNotificationArea => {
            "Other accounts on this PC can read or change a file that holds a sign-in.".into()
        }
        Front::MacMenuBar | Front::Gnome => format!(
            "Other people on {} can read a file that holds a sign-in.",
            this_machine(front)
        ),
    }
}

pub(crate) const NO_SAVED_SIGN_IN_OPENS: &str = "No saved sign-in opens for this Windows account.";

pub(crate) const SIGN_IN_TO_EACH_AGAIN: &str = "Sign in to each account again.";

pub(crate) fn claude_code_config(unreadable: bool) -> &'static str {
    if unreadable {
        "Claude Code’s config file can’t be read."
    } else {
        "Claude Code hasn’t run here yet."
    }
}

pub(crate) const NO_ACCOUNT_NAMED_YET: &str = "Claude Code hasn’t named a signed-in account yet.";

pub(crate) const OPEN_CLAUDE_CODE_ONCE: &str = "Open Claude Code once and sign in.";

pub(crate) const CLAUDE_CODE_NEEDS_SIGNING_IN: &str = "Claude Code needs signing in, now or soon.";

pub(crate) const CLAUDE_CODE_SIGN_IN_UNREAD: &str = "Pitboard can’t read Claude Code’s sign-in.";

// "work, dana@work.example", the account a tool is signed in to now.
pub(crate) fn signed_in_as(label: Option<&str>, email: &str) -> String {
    match (label, email) {
        (Some(label), "") => label.to_owned(),
        (Some(label), email) => format!("{label}, {email}"),
        (None, email) => email.to_owned(),
    }
}

pub(crate) fn not_sure_who(tool: &str) -> String {
    format!("Pitboard isn’t sure which account {tool} is signed in to.")
}

pub(crate) const READ_AT_THE_NEXT_SWITCH: &str =
    "Pitboard reads it again at the next switch, with Claude closed.";

pub(crate) const CODEX_SIGN_IN_UNUSABLE: &str = "Pitboard can’t use the sign-in Codex has now.";

pub(crate) const SIGN_IN_TO_CODEX_WITH_CHATGPT: &str = "Sign in to Codex with a ChatGPT account.";

pub(crate) const CODEX_STORE_UNSWITCHABLE: &str =
    "Codex keeps its sign-in where Pitboard can’t switch it.";

pub(crate) fn codex_file_shared(front: Front) -> String {
    match front {
        Front::WindowsNotificationArea => {
            "Other accounts on this PC can read Codex’s sign-in file.".into()
        }
        Front::MacMenuBar | Front::Gnome => format!(
            "Other people on {} can read Codex’s sign-in file.",
            this_machine(front)
        ),
    }
}

pub(crate) fn nothing_signed_in_to(tool: &str) -> String {
    format!("Nothing is signed in to {tool}.")
}

// Doctor's saved sign-ins it started to write and could not read back this time.
pub(crate) const READ_AGAIN_FIRST: &str = "Pitboard reads them again before its next change.";

pub(crate) const RUNS_ONCE_A_DAY: &str = "Runs once a day";

pub(crate) fn daily_task(gone: bool) -> &'static str {
    if gone {
        "Its daily task runs a copy of Pitboard that isn’t there."
    } else {
        "Its last daily run didn’t work, or its task changed."
    }
}

pub(crate) fn turn_it_off_and_on(front: Front) -> String {
    format!(
        "Turn it off and on again in {}.",
        super::settings(front, false)
    )
}

// An account's saved sign-in, where its row is not there to say more.
pub(crate) fn saved_sign_in(ready: bool) -> &'static str {
    if ready {
        "Saved sign-in ready"
    } else {
        "Saved sign-in can’t be used"
    }
}

// Doctor says so once an account's sign-in has lived its 30 days unused.
pub(crate) const NOT_SWITCHED_TO_IN_A_MONTH: &str = "Not switched to in over 30 days.";

pub(crate) const KEPT_FRESH_UNTIL_REMOVED: &str =
    "Pitboard keeps its sign-in fresh until you remove it.";

pub(crate) const LEFTOVER_FILE_SAYS: &str =
    "Claude Code can use it in place of the account you switch to.";

pub(crate) const CANNOT_CLEAN_UP_UNREAD: &str = "Pitboard can’t read it, so it can’t clean it up.";

pub(crate) const SWITCH_IN_CLAUDE_WAITS: &str = "A switch in Claude didn’t finish.";

pub(crate) const KEEP_CLAUDE_CLOSED: &str =
    "Keep Claude closed, and Pitboard finishes it with the next switch.";

pub(crate) const CLAUDE_SIGNED_OUT_FOR_A_SIGN_IN: &str =
    "Claude is signed out for a sign-in that didn’t finish.";

pub(crate) const CANNOT_SAVE_CLAUDE_SIGN_INS: &str =
    "Pitboard can’t save Claude’s sign-ins in its folder.";

pub(crate) const CLAUDE_FOLDERS_MOVED: &str =
    "Pitboard moved some of Claude’s folders out of the way.";

pub(crate) fn check_text(text: &str) -> String {
    typeset(&prose_capitalised(&without_commands(text)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A check is shown as a shape and a colour, and said as a word. If two levels ever came
    /// to sound the same, a broken check would read as a passing one. Its shape and its colour
    /// are each app's own.
    #[test]
    fn every_level_sounds_like_itself() {
        let spoken: Vec<&str> = [Level::Ok, Level::Warn, Level::Fail]
            .into_iter()
            .map(spoken_level)
            .collect();
        assert_eq!(spoken, ["Passed", "Warning", "Failed"]);
        let mut distinct = spoken.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), spoken.len());
        assert!(spoken.iter().all(|word| !word.is_empty()));
    }

    #[test]
    fn the_checks_say_when_they_were_made() {
        assert_eq!(checked_at("14:05"), "Checked at 14:05");
    }

    // Doctor names a check for a terminal's table: "account personal", "config file".
    #[test]
    fn each_check_is_named_for_diagnostics_in_each_systems_case() {
        let named = |front| -> Vec<String> {
            [
                ("config_file", "config file"),
                ("in_use", "in use"),
                ("interrupted_switch", "interrupted switch"),
                ("claude_version", "Claude Code build"),
                ("schedule", "renewal schedule"),
                ("machine_id", "machine id"),
                ("a_new_check", "a new check"),
            ]
            .into_iter()
            .map(|(code, name)| check_name(front, code, name))
            .collect()
        };
        assert_eq!(
            named(Front::MacMenuBar),
            [
                "Config File",
                "Signed-In Account",
                "Switch That Didn’t Finish",
                "Version",
                "Keep Saved Sign-Ins Fresh",
                "Machine ID",
                "A new check",
            ]
        );
        assert_eq!(
            named(Front::WindowsNotificationArea),
            [
                "Config file",
                "Signed-in account",
                "Switch that didn’t finish",
                "Version",
                "Keep saved sign-ins fresh",
                "Machine ID",
                "A new check",
            ]
        );
        assert_eq!(NOTIFICATIONS, "Notifications");
    }

    // One sentence each, as plan.md section 3 has notes.
    #[test]
    fn diagnostics_says_what_doctor_found_in_the_apps_words() {
        assert_eq!(
            (
                folder_private(Front::MacMenuBar),
                folder_private(Front::WindowsNotificationArea)
            ),
            (
                "Only you can read it",
                "Only your Windows account can open it"
            )
        );
        assert_eq!(
            folder_shared(Front::MacMenuBar),
            "Other people on this Mac can read it."
        );
        assert_eq!(
            sign_in_files_shared(Front::Gnome),
            "Other people on this computer can read a file that holds a sign-in."
        );
        assert_eq!(folder_private(Front::Gnome), "Only you can read it");
        assert_eq!(
            folder_shared(Front::Gnome),
            "Other people on this computer can read it."
        );
        assert_eq!(
            codex_file_shared(Front::Gnome),
            "Other people on this computer can read Codex’s sign-in file."
        );
        assert_eq!(
            [
                turn_it_off_and_on(Front::MacMenuBar),
                turn_it_off_and_on(Front::Gnome),
                turn_it_off_and_on(Front::WindowsNotificationArea),
            ],
            [
                "Turn it off and on again in Settings.",
                "Turn it off and on again in Preferences.",
                "Turn it off and on again in Settings.",
            ]
        );
        assert_eq!(
            codex_file_shared(Front::WindowsNotificationArea),
            "Other accounts on this PC can read Codex’s sign-in file."
        );
        assert_eq!(
            not_sure_who("Claude Code"),
            "Pitboard isn’t sure which account Claude Code is signed in to."
        );
        assert_eq!(
            signed_in_as(Some("work"), "dana@work.example"),
            "work, dana@work.example"
        );
        assert_eq!(signed_in_as(Some("desktop"), ""), "desktop");
        assert_eq!(signed_in_as(None, "dana@work.example"), "dana@work.example");
        assert_eq!(
            (claude_code_config(false), claude_code_config(true)),
            (
                "Claude Code hasn’t run here yet.",
                "Claude Code’s config file can’t be read."
            )
        );
        assert_eq!(
            daily_task(true),
            "Its daily task runs a copy of Pitboard that isn’t there."
        );
        assert_eq!(
            (saved_sign_in(true), saved_sign_in(false)),
            ("Saved sign-in ready", "Saved sign-in can’t be used")
        );
        assert_eq!(
            [
                checks_verdict(0, 0),
                checks_verdict(2, 0),
                checks_verdict(1, 1),
                checks_verdict(3, 2)
            ],
            [
                "Everything Pitboard checks is in order.",
                "Everything else Pitboard checks is in order.",
                "Don’t switch accounts until the failed check is fixed.",
                "Don’t switch accounts until the failed checks are fixed.",
            ]
        );
        assert_eq!(
            (
                check_issues(Front::MacMenuBar, 1),
                check_issues(Front::WindowsNotificationArea, 0)
            ),
            ("1 Issue".into(), "No issues".into())
        );
    }

    #[test]
    fn a_checks_prose_starts_with_a_capital_and_data_keeps_its_case() {
        let said: Vec<String> = [
            "inactive",
            "none enrolled yet",
            "direct: no variable names a proxy",
            "config.json read",
            r"C:\Users\dana\AppData\Local\Pitboard",
            "dana@example.com · org 1f2e",
            "2.1.299",
            "1 enrolled",
            "none outstanding",
        ]
        .into_iter()
        .map(check_text)
        .collect();
        assert_eq!(
            said,
            [
                "Inactive",
                "None enrolled yet",
                "Direct: no variable names a proxy",
                "config.json read",
                r"C:\Users\dana\AppData\Local\Pitboard",
                "dana@example.com · org 1f2e",
                "2.1.299",
                "1 enrolled",
                "None outstanding",
            ]
        );
    }

    // A terminal marks a command with backticks; an app shows them as typed.
    #[test]
    fn no_check_says_a_backtick() {
        assert_eq!(
            check_text("It writes this after its first successful call. Run `claude` once."),
            "It writes this after its first successful call. Run claude once."
        );
        assert_eq!(
            check_text("`work` is recorded in use and also holds a parked login"),
            "work is recorded in use and also holds a parked login"
        );
        assert_eq!(check_text("Run `pitboard enroll work --sign-in`."), "");
        assert_eq!(
            check_text(
                "it has expired; run `pitboard doctor`. `PITBOARD_HOME` is `pitboard`. Run \
                 `pitboard` to see who is signed in."
            ),
            "It has expired. PITBOARD_HOME is pitboard."
        );
    }

    #[test]
    fn a_notification_not_posted_says_which_when_and_why() {
        assert_eq!(
            not_posted("Couldn’t switch", "08:00", "Notifications are off"),
            "“Couldn’t switch” not posted at 08:00 · Notifications are off"
        );
    }
}
