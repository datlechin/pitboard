// Every control and sentence the apps draw on their own; one only a system shows is None elsewhere.

use super::SetupLink;
use super::words::{self, titled};
use pitboard_core::host::{OS, Os};

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AppWords {
    // what the Windows title bar, About and the icon's first tooltip line call the app
    pub app_name: String,
    pub menu: MenuWords,
    pub window: WindowWords,
    pub activity: ActivityWords,
    pub settings: SettingsWords,
    pub sheet: SheetWords,
    pub account_window: AccountWindowWords,
    pub shortcuts: ShortcutsWords,
}

// What a Shortcuts action says that the model's answers don't: macOS's alone.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ShortcutsWords {
    pub not_read_yet: Option<String>,
    pub none_in_use: Option<String>,
}

// The menu bar item's and the notification area icon's menus, and the app's own menus.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MenuWords {
    pub add_account: String,
    pub refresh: String,
    pub open_pitboard: String,
    pub settings: String,
    pub check_for_updates: String,
    pub about: String,
    pub help: String,
    // "Quit Pitboard" on macOS, "Exit" in the notification area's menu
    pub quit: String,
    pub account: String,
    pub copy_email: String,
    // what Narrator calls the notification area's menu
    pub icon_menu: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WindowWords {
    pub accounts: String,
    pub activity: String,
    pub diagnostics: String,
    pub refresh: String,
    pub refresh_help: String,
    pub add_account: String,
    pub add_account_help: String,
    pub check_again: String,
    pub check_again_help: String,
    pub copy_report: String,
    pub copy_report_help: String,
    pub open_link: String,
    pub cancel: String,
    pub close: String,
    pub ok: String,
    pub dismiss: String,
    // beside an account's values in the macOS inspector and an expanded Windows row
    pub email: String,
    pub plan: String,
    pub sign_in: String,
    pub warm_up: String,
    // the macOS toolbar's button that shows and hides the inspector
    pub inspector: Option<String>,
    pub inspector_help: Option<String>,
    // what the inspector says while no account is selected
    pub no_selection: Option<String>,
    // a check's detail's own menu, and an expanded row's email, on Windows
    pub copy: Option<String>,
    pub select_all: Option<String>,
    // what Narrator calls the checks
    pub checks: Option<String>,
    // what VoiceOver and Narrator call the spinner while the checks run
    pub checking: Option<String>,
    // a Windows Debug build's refusal of a link outside a fixture
    pub debug_link_refused: Option<String>,
}

// The activity list's columns and toolbar.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ActivityWords {
    pub date: String,
    pub change: String,
    pub account: String,
    pub result: String,
    pub by: String,
    pub refresh_help: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SettingsWords {
    pub title: String,
    pub general: String,
    pub limits: String,
    pub advanced: String,
    // the Windows page's last section
    pub about: Option<String>,
    // "Open Pitboard at Login", "Start when you sign in to Windows"
    pub open_at_login: String,
    pub open_at_login_note: String,
    // a Windows Debug build, which never writes the Run key
    pub debug_never_starts: Option<String>,
    // macOS waits for Login Items to allow it, with a button to them
    pub login_items_waiting: Option<String>,
    pub open_login_items: Option<String>,
    // Windows' Startup settings turned it off, with a link to them
    pub startup_turned_off: Option<String>,
    pub open_startup_settings: Option<String>,
    pub notifications: String,
    pub notifications_off: String,
    pub open_notification_settings: String,
    pub notifications_off_for_every_app: Option<String>,
    pub notifications_off_by_policy: Option<String>,
    // what the macOS menu bar item shows, and its choices
    pub menu_bar_shows: Option<String>,
    pub account_and_usage: Option<String>,
    pub usage_only: Option<String>,
    pub icon_only: Option<String>,
    // over General's update settings
    pub updates: String,
    pub check_for_updates_automatically: String,
    pub download_and_install_automatically: Option<String>,
    pub version: String,
    pub check_now: String,
    // a Windows copy that has no update to check for yet, or is a beta
    pub no_release_to_update_to: Option<String>,
    pub beta_updates: Option<String>,
    pub keep_saved_sign_ins_fresh: String,
    pub command_line: String,
    // beside where the command line is
    pub path: String,
    pub install_command_line: Option<String>,
    pub link_failed: Option<String>,
    pub cannot_link_command_line: Option<String>,
    // what VoiceOver and Narrator call the spinner while Settings looks for one
    pub looking_for_command_line: Option<String>,
    // a Windows copy that cannot update itself
    pub update_key_invalid: Option<String>,
    pub win_sparkle_missing: Option<String>,
    pub notifications_unoffered: Option<String>,
    // over Uninstall Pitboard… on macOS, beside Uninstall… on Windows
    pub uninstall: String,
    pub uninstall_note: String,
    // what the macOS app asks of itself once Pitboard's own data is gone, `trash_says` the message
    pub move_to_trash_question: Option<String>,
    pub move_to_trash: Option<String>,
    // a copy Homebrew installed, which Homebrew removes
    pub remove_with_homebrew: Option<String>,
    pub homebrew_removes_it: Option<String>,
    // the docs' table of how to remove Pitboard by how it was installed
    pub homebrew_help: Option<SetupLink>,
    // the Windows page's section with a card each that opens Diagnostics and Activity
    pub troubleshoot: Option<String>,
    pub diagnostics_about: Option<String>,
    pub activity_about: Option<String>,
    // what Narrator calls the warm-up hours' pickers
    pub warm_from: Option<String>,
    pub warm_to: Option<String>,
    pub from: String,
    pub to: String,
    pub help: String,
    pub report_a_problem: String,
    pub source_code: String,
    pub license: String,
    pub copyright: String,
}

// The Add Account and Sign In Again sheet's fields and buttons.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SheetWords {
    pub tool: String,
    pub account: String,
    pub name: String,
    pub sign_in_page: String,
    pub copy_link: String,
    pub open: String,
    pub code: String,
    pub paste_code: String,
    pub submit_code: String,
}

// An account's claude.ai or chatgpt.com window: its toolbar, its menus and its downloads.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AccountWindowWords {
    pub back: String,
    pub back_help: String,
    pub forward: String,
    pub forward_help: String,
    pub stop: String,
    pub stop_help: String,
    pub reload: String,
    pub reload_help: String,
    pub reload_page: String,
    pub downloads: String,
    pub downloads_help: String,
    pub could_not_load: String,
    pub try_again: String,
    pub remove_website_data: String,
    pub remove: String,
    pub actual_size: String,
    pub zoom_in: String,
    pub zoom_out: String,
    pub go: String,
    pub download: String,
    pub stop_download_help: String,
    // "Show in Finder", "Show in folder"
    pub show_file: String,
    pub show_file_help: String,
    pub could_not_open: Option<String>,
    // what VoiceOver calls the link the Open Link window opens
    pub link: Option<String>,
}

// A switch Pitboard hasn't answered in the time Shortcuts gives an action.
#[uniffi::export]
pub fn still_switching(label: String) -> String {
    format!("Still switching to {label}.")
}

// One account in what Get Usage says: "work: 5-hour 42% used, Weekly 12%".
#[uniffi::export]
pub fn usage_of(name: String, said: String) -> String {
    format!("{name}: {said}")
}

#[uniffi::export]
pub fn install_update(version: String) -> String {
    format!("Install Pitboard {version}…")
}

#[uniffi::export]
pub fn ready_to_install(version: String) -> String {
    format!("Pitboard {version} is ready to install.")
}

// A copy that knows no version is Pitboard alone.
#[uniffi::export]
pub fn named_version(version: String) -> String {
    if version.is_empty() {
        "Pitboard".into()
    } else {
        format!("Pitboard {version}")
    }
}

// What Install Command Line Tool… does, on macOS.
#[uniffi::export]
pub fn links_to(target: String) -> String {
    format!(
        "Links {target} to the one inside this app. macOS asks for an administrator’s password."
    )
}

#[uniffi::export]
pub fn already_there(link: String) -> String {
    format!("{link} is already there and isn’t a link, so it was kept.")
}

// Windows' Run key holds a command of up to `characters`.
#[uniffi::export]
pub fn path_too_long(characters: u32) -> String {
    format!(
        "Windows starts a program when you sign in only from a path of up to {characters} \
         characters, and Pitboard’s is longer."
    )
}

// `have`: the WebView2 Runtime's version, none where it is not installed.
#[uniffi::export]
pub fn webview_runtime_too_old(minimum: String, have: Option<String>) -> String {
    let have = have.map_or_else(|| "none".into(), |version| format!("version {version}"));
    format!(
        "This window needs the Microsoft Edge WebView2 Runtime {minimum} or later, and this PC \
         has {have}. Windows Update keeps it up to date."
    )
}

const DELETED: &str = "Its saved sign-ins and settings are deleted.";

const REMOVE_PAGE: &str = "https://docs.usepitboard.com/install/remove#remove-pitboard-itself";

// What Move Pitboard to the Trash? says, `link` the command line link Move to Trash removes.
#[uniffi::export]
pub fn trash_says(link: Option<String>) -> String {
    match link {
        None => DELETED.into(),
        Some(link) => format!(
            "{DELETED} Removing the command line link at {link} takes an administrator’s password."
        ),
    }
}

#[uniffi::export]
pub fn app_words() -> AppWords {
    app_words_on(OS)
}

pub(crate) fn app_words_on(os: Os) -> AppWords {
    let mac = |words: &str| (os != Os::Windows).then(|| words.to_owned());
    let windows = |words: &str| (os == Os::Windows).then(|| words.to_owned());
    AppWords {
        app_name: words::PITBOARD.into(),
        menu: MenuWords {
            add_account: words::add_account(os, true),
            refresh: words::REFRESH.into(),
            open_pitboard: titled(os, "Open Pitboard", "Open Pitboard"),
            settings: titled(os, "Settings…", "Settings"),
            check_for_updates: titled(os, "Check for Updates…", "Check for updates"),
            about: titled(os, "About Pitboard", "About Pitboard"),
            help: titled(os, "Pitboard Help", "Pitboard help"),
            quit: match os {
                Os::Windows => "Exit",
                Os::MacOs | Os::Linux => "Quit Pitboard",
            }
            .into(),
            account: words::ACCOUNT.into(),
            copy_email: titled(os, "Copy Email Address", "Copy email address"),
            icon_menu: windows("Pitboard menu"),
        },
        window: WindowWords {
            accounts: "Accounts".into(),
            activity: "Activity".into(),
            diagnostics: "Diagnostics".into(),
            refresh: words::REFRESH.into(),
            refresh_help: "Read every account’s usage again".into(),
            add_account: words::add_account(os, os == Os::Windows),
            add_account_help: "Sign in to another account".into(),
            check_again: titled(os, "Check Again", "Check again"),
            check_again_help: "Make every check again".into(),
            copy_report: titled(os, "Copy Report", "Copy report"),
            copy_report_help: "Copy the checks for a bug report, with email addresses and \
                               account identifiers hidden"
                .into(),
            open_link: titled(os, "Open Link", "Open link"),
            cancel: words::CANCEL.into(),
            close: "Close".into(),
            ok: "OK".into(),
            dismiss: words::DISMISS.into(),
            email: "Email".into(),
            plan: "Plan".into(),
            sign_in: titled(os, "Sign-In", "Sign-in"),
            warm_up: words::warm_up(os),
            inspector: mac("Inspector"),
            inspector_help: mac("Show or hide the selected account’s details"),
            no_selection: mac("No Account Selected"),
            copy: windows("Copy"),
            select_all: windows("Select all"),
            checks: windows("Checks"),
            checking: Some("Checking".into()),
            debug_link_refused: windows(
                "A Debug build opens a pitboard-debug link only in a fixture.",
            ),
        },
        activity: ActivityWords {
            date: "Date".into(),
            change: "Change".into(),
            account: words::ACCOUNT.into(),
            result: "Result".into(),
            by: "By".into(),
            refresh_help: "Read the log again".into(),
        },
        settings: SettingsWords {
            title: "Settings".into(),
            general: "General".into(),
            limits: "Limits".into(),
            advanced: "Advanced".into(),
            about: windows("About"),
            open_at_login: words::open_at_login(os).0.into(),
            open_at_login_note: words::open_at_login(os).1.into(),
            debug_never_starts: windows("A Debug build never starts when you sign in to Windows."),
            login_items_waiting: mac("macOS is waiting for you to allow Pitboard in Login Items."),
            open_login_items: mac("Open Login Items Settings…"),
            startup_turned_off: windows(
                "Turned off in Settings > Apps > Startup, where you can turn it on again.",
            ),
            open_startup_settings: windows("Open Startup settings"),
            notifications: words::NOTIFICATIONS.into(),
            notifications_off: match os {
                Os::Windows => {
                    "Notifications from Pitboard are turned off in Settings > System > \
                     Notifications."
                }
                Os::MacOs | Os::Linux => {
                    "Notifications from Pitboard are turned off in System Settings."
                }
            }
            .into(),
            open_notification_settings: titled(
                os,
                "Open Notification Settings…",
                "Open notification settings",
            ),
            notifications_off_for_every_app: windows(
                "Notifications are turned off for every app in Settings > System > Notifications.",
            ),
            notifications_off_by_policy: windows(
                "Group Policy turns off app notifications on this PC.",
            ),
            menu_bar_shows: mac("Menu Bar Shows"),
            account_and_usage: mac("Account and Usage"),
            usage_only: mac("Usage Only"),
            icon_only: mac("Icon Only"),
            updates: "Updates".into(),
            check_for_updates_automatically: titled(
                os,
                "Check for Updates Automatically",
                "Check for updates automatically",
            ),
            download_and_install_automatically: mac("Download and Install Automatically"),
            version: "Version".into(),
            check_now: titled(os, "Check Now", "Check now"),
            no_release_to_update_to: windows(
                "No full release of Pitboard for Windows is out yet, so there is no update to \
                 check for.",
            ),
            beta_updates: windows(
                "A beta updates itself to 1.0.0 once that is out. Install a later beta by hand.",
            ),
            keep_saved_sign_ins_fresh: words::keep_saved_sign_ins_fresh(os),
            command_line: titled(os, "Command Line", "Command line"),
            path: "Path".into(),
            install_command_line: mac("Install Command Line Tool…"),
            link_failed: mac("The link couldn’t be made."),
            cannot_link_command_line: mac(
                "This copy of Pitboard can’t link the command line inside it.",
            ),
            looking_for_command_line: Some("Looking for pitboard".into()),
            update_key_invalid: windows(
                "This copy of Pitboard can’t update itself: the update key it was built with isn’t \
                 a valid key. Install a later version by hand.",
            ),
            win_sparkle_missing: windows(
                "This copy of Pitboard can’t update itself: WinSparkle.dll, which updates it, is \
                 missing. Install Pitboard again.",
            ),
            notifications_unoffered: windows("Windows offers this process no app notifications."),
            uninstall: match os {
                Os::Windows => "Uninstall Pitboard",
                Os::MacOs | Os::Linux => words::UNINSTALL,
            }
            .into(),
            uninstall_note: "Deletes Pitboard’s saved sign-ins and settings, and leaves each tool \
                             signed in to the account in use."
                .into(),
            move_to_trash_question: mac("Move Pitboard to the Trash?"),
            move_to_trash: mac("Move to Trash"),
            remove_with_homebrew: mac("Remove Pitboard with Homebrew"),
            homebrew_removes_it: mac(&format!(
                "{DELETED} Homebrew installed the app, so Homebrew removes it."
            )),
            homebrew_help: (os != Os::Windows).then(|| SetupLink {
                title: words::learn_more(os),
                link: REMOVE_PAGE.into(),
            }),
            troubleshoot: windows("Troubleshoot"),
            diagnostics_about: (os == Os::Windows).then(|| {
                let names = crate::tools_on(os).into_iter().map(|tool| tool.name);
                format!(
                    "Check how {} are set up on this PC.",
                    pitboard_core::words::listed(
                        std::iter::once(words::PITBOARD.into()).chain(names)
                    )
                )
            }),
            activity_about: windows("Every switch and change Pitboard made."),
            warm_from: windows("Warm up from"),
            warm_to: windows("Warm up to"),
            from: "From".into(),
            to: "To".into(),
            help: titled(os, "Pitboard Help", "Pitboard help"),
            report_a_problem: titled(os, "Report a Problem", "Report a problem"),
            source_code: titled(os, "Source Code on GitHub", "Source code on GitHub"),
            license: "Apache License 2.0".into(),
            copyright: "Copyright 2026 the Pitboard authors".into(),
        },
        sheet: SheetWords {
            tool: "Tool".into(),
            account: words::ACCOUNT.into(),
            name: "Name".into(),
            sign_in_page: titled(os, "Sign-In Page", "Sign-in page"),
            copy_link: titled(os, "Copy Link", "Copy link"),
            open: words::OPEN.into(),
            code: "Code".into(),
            paste_code: "Paste the code from your browser".into(),
            submit_code: titled(os, "Submit Code", "Submit code"),
        },
        account_window: AccountWindowWords {
            back: "Back".into(),
            back_help: "Show the previous page".into(),
            forward: "Forward".into(),
            forward_help: "Show the next page".into(),
            stop: "Stop".into(),
            stop_help: "Stop loading this page".into(),
            reload: "Reload".into(),
            reload_help: "Load this page again".into(),
            reload_page: titled(os, "Reload Page", "Reload page"),
            downloads: "Downloads".into(),
            downloads_help: "Show this window’s downloads".into(),
            could_not_load: titled(os, "Couldn’t Load the Page", "Couldn’t load the page"),
            try_again: words::try_again(os),
            remove_website_data: titled(os, "Remove Website Data…", "Remove website data…"),
            remove: words::REMOVE.into(),
            actual_size: titled(os, "Actual Size", "Actual size"),
            zoom_in: titled(os, "Zoom In", "Zoom in"),
            zoom_out: titled(os, "Zoom Out", "Zoom out"),
            go: "Go".into(),
            download: words::DOWNLOAD.into(),
            stop_download_help: "Stop this download".into(),
            show_file: match os {
                Os::Windows => "Show in folder",
                Os::MacOs | Os::Linux => "Show in Finder",
            }
            .into(),
            show_file_help: match os {
                Os::Windows => "Show the file in its folder",
                Os::MacOs | Os::Linux => "Show the file in Finder",
            }
            .into(),
            could_not_open: windows("Couldn’t open this window"),
            link: mac("Link"),
        },
        shortcuts: ShortcutsWords {
            not_read_yet: mac("Pitboard hasn’t read its accounts yet. Try again in a moment."),
            none_in_use: mac(words::NONE_IN_USE),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::present::testing::{mechanism_words, strings_in, strings_of, title_cased};

    // Each system's words of one control, a word only one system shows empty on the other.
    fn pairs() -> Vec<(String, String)> {
        let both = |os| strings_in(&format!("{:?}", app_words_on(os)).replace("None", "\"\""));
        let (mac, windows) = (both(Os::MacOs), both(Os::Windows));
        assert_eq!(mac.len(), windows.len());
        mac.into_iter().zip(windows).collect()
    }

    // But where each system names a thing its own way, which these pairs are.
    #[test]
    fn each_titled_pair_differs_only_in_case_and_windows_ellipsis() {
        let named_apart = [
            (
                "Open Pitboard at Login",
                "Start when you sign in to Windows",
            ),
            ("Quit Pitboard", "Exit"),
            (
                "Notifications from Pitboard are turned off in System Settings.",
                "Notifications from Pitboard are turned off in Settings > System > Notifications.",
            ),
            ("Show in Finder", "Show in folder"),
            ("Uninstall", "Uninstall Pitboard"),
            ("Show the file in Finder", "Show the file in its folder"),
            (
                "Pitboard watches your limits only while it’s open.",
                "Pitboard watches your limits only while it’s running.",
            ),
        ];
        for (mac, windows) in pairs() {
            if mac.is_empty() || windows.is_empty() {
                continue;
            }
            let (one, other) = (mac.to_lowercase(), windows.to_lowercase());
            let alike = one == other
                || one.strip_suffix('…') == Some(&other)
                || other.strip_suffix('…') == Some(&one);
            let apart = named_apart.contains(&(mac.as_str(), windows.as_str()));
            assert!(alike || apart, "{mac:?} and {windows:?}");
        }
    }

    // Past a sentence's first word, a capital is a name's, or a Settings page's the words name.
    #[test]
    fn windows_says_every_control_in_sentence_case() {
        const NAMES: [&str; 19] = [
            "Pitboard",
            "Debug",
            "Claude",
            "Code",
            "Codex",
            "WinSparkle.dll",
            "Windows",
            "GitHub",
            "Apache",
            "License",
            "Settings",
            "Apps",
            "Startup",
            "System",
            "Notifications",
            "Group",
            "Policy",
            "PC",
            "OK",
        ];
        for said in strings_of(&app_words_on(Os::Windows)) {
            assert!(title_cased(&said, &NAMES).is_empty(), "{said:?}");
        }
    }

    #[test]
    fn no_control_says_park_put_away_give_up_enrol_record_or_settle() {
        for os in [Os::MacOs, Os::Windows] {
            for said in strings_of(&app_words_on(os)) {
                assert!(mechanism_words(&said).is_empty(), "{said}");
            }
        }
    }

    #[test]
    fn a_sentence_with_a_value_is_made_here_not_in_a_view() {
        assert_eq!(
            still_switching("personal".into()),
            "Still switching to personal."
        );
        assert_eq!(
            usage_of("work".into(), "5-hour 42% used, Weekly 12%".into()),
            "work: 5-hour 42% used, Weekly 12%"
        );
        assert_eq!(install_update("1.1.0".into()), "Install Pitboard 1.1.0…");
        assert_eq!(
            ready_to_install("1.1.0".into()),
            "Pitboard 1.1.0 is ready to install."
        );
        assert_eq!(
            (named_version("1.1.0".into()), named_version(String::new())),
            ("Pitboard 1.1.0".into(), "Pitboard".into())
        );
        assert_eq!(
            links_to("/usr/local/bin/pitboard".into()),
            "Links /usr/local/bin/pitboard to the one inside this app. macOS asks for an \
             administrator’s password."
        );
        assert_eq!(
            already_there("/usr/local/bin/pitboard".into()),
            "/usr/local/bin/pitboard is already there and isn’t a link, so it was kept."
        );
        assert_eq!(
            path_too_long(220),
            "Windows starts a program when you sign in only from a path of up to 220 characters, \
             and Pitboard’s is longer."
        );
        assert_eq!(
            webview_runtime_too_old("144.0.3719.77".into(), None),
            "This window needs the Microsoft Edge WebView2 Runtime 144.0.3719.77 or later, and \
             this PC has none. Windows Update keeps it up to date."
        );
        assert!(
            webview_runtime_too_old("144.0.3719.77".into(), Some("120.0".into()))
                .contains("this PC has version 120.0.")
        );
    }

    // Plan 2: the Windows views said these themselves.
    #[test]
    fn the_windows_views_words_of_their_own_are_here() {
        let (mac, windows) = (app_words_on(Os::MacOs), app_words_on(Os::Windows));
        assert_eq!(windows.app_name, "Pitboard");
        assert_eq!(
            (
                mac.settings.open_at_login_note.as_str(),
                windows.settings.open_at_login_note.as_str()
            ),
            (
                "Pitboard watches your limits only while it’s open.",
                "Pitboard watches your limits only while it’s running."
            )
        );
        assert_eq!(
            windows.settings.debug_never_starts.as_deref(),
            Some("A Debug build never starts when you sign in to Windows.")
        );
        assert_eq!(
            windows.window.debug_link_refused.as_deref(),
            Some("A Debug build opens a pitboard-debug link only in a fixture.")
        );
        assert_eq!(
            (
                mac.settings.debug_never_starts,
                mac.window.debug_link_refused
            ),
            (None, None)
        );
    }

    #[test]
    fn a_word_only_one_system_shows_is_none_on_the_other() {
        let (mac, windows) = (app_words_on(Os::MacOs), app_words_on(Os::Windows));
        assert_eq!(windows.settings.menu_bar_shows, None);
        assert_eq!(mac.settings.startup_turned_off, None);
        assert_eq!(
            windows.settings.startup_turned_off.as_deref(),
            Some("Turned off in Settings > Apps > Startup, where you can turn it on again.")
        );
        assert_eq!(
            (mac.menu.settings.as_str(), windows.menu.settings.as_str()),
            ("Settings…", "Settings")
        );
        assert_eq!(
            (mac.menu.quit.as_str(), windows.menu.quit.as_str()),
            ("Quit Pitboard", "Exit")
        );
        assert_eq!(
            (
                mac.settings.keep_saved_sign_ins_fresh.as_str(),
                windows.settings.keep_saved_sign_ins_fresh.as_str()
            ),
            ("Keep Saved Sign-Ins Fresh", "Keep saved sign-ins fresh")
        );
        assert_eq!(mac.window.diagnostics, "Diagnostics");
        assert_eq!(mac.activity.by, "By");
        assert_eq!(
            mac.shortcuts,
            ShortcutsWords {
                not_read_yet: Some(
                    "Pitboard hasn’t read its accounts yet. Try again in a moment.".into()
                ),
                none_in_use: Some("No account is in use.".into()),
            }
        );
        assert_eq!(
            windows.shortcuts,
            ShortcutsWords {
                not_read_yet: None,
                none_in_use: None,
            }
        );
    }

    // A spinner says nothing on screen, so each system's screen reader is told what it waits for.
    #[test]
    fn a_spinner_is_named_on_each_system() {
        for os in [Os::MacOs, Os::Windows] {
            let words = app_words_on(os);
            assert_eq!(words.window.checking.as_deref(), Some("Checking"));
            assert_eq!(
                words.settings.looking_for_command_line.as_deref(),
                Some("Looking for pitboard")
            );
        }
    }

    // Plan 6.5: once Pitboard's own data is gone, the macOS app takes itself to the Trash, or
    // points a copy Homebrew installed at Homebrew.
    #[test]
    fn the_macos_uninstall_ends_with_the_app_itself() {
        let (mac, windows) = (
            app_words_on(Os::MacOs).settings,
            app_words_on(Os::Windows).settings,
        );
        assert_eq!(
            (
                mac.move_to_trash_question.as_deref(),
                mac.move_to_trash.as_deref()
            ),
            (Some("Move Pitboard to the Trash?"), Some("Move to Trash"))
        );
        assert_eq!(
            trash_says(None),
            "Its saved sign-ins and settings are deleted."
        );
        assert_eq!(
            trash_says(Some("/usr/local/bin/pitboard".into())),
            "Its saved sign-ins and settings are deleted. Removing the command line link at \
             /usr/local/bin/pitboard takes an administrator’s password."
        );
        assert_eq!(
            (
                mac.remove_with_homebrew.as_deref(),
                mac.homebrew_removes_it.as_deref()
            ),
            (
                Some("Remove Pitboard with Homebrew"),
                Some(
                    "Its saved sign-ins and settings are deleted. Homebrew installed the app, so \
                     Homebrew removes it."
                )
            )
        );
        assert_eq!(
            mac.homebrew_help,
            Some(SetupLink {
                title: "Learn More".into(),
                link: "https://docs.usepitboard.com/install/remove#remove-pitboard-itself".into()
            })
        );
        let page = include_str!("../../../../docs/install/remove.mdx");
        assert!(
            page.contains("\n## Remove Pitboard itself\n"),
            "the anchor is a heading"
        );
        assert!(
            page.contains("`brew uninstall --cask pitboard-app`"),
            "the command is the page's, which Learn More opens"
        );
        assert_eq!(
            (
                windows.move_to_trash,
                windows.remove_with_homebrew,
                windows.homebrew_help
            ),
            (None, None, None),
            "Windows' installer removes the app"
        );
    }

    // Plan 3: a command that asks for more says so on Windows; the macOS toolbar's has no ellipsis.
    #[test]
    fn the_windows_command_bar_says_add_account_asks_for_more() {
        let (mac, windows) = (app_words_on(Os::MacOs), app_words_on(Os::Windows));
        assert_eq!(
            (
                mac.window.add_account.as_str(),
                windows.window.add_account.as_str()
            ),
            ("Add Account", "Add account…")
        );
    }

    // Plan 4.3 and 4.9: what the inspector and an expanded row label an account's values.
    #[test]
    fn an_accounts_values_are_labelled_in_each_systems_case() {
        let (mac, windows) = (app_words_on(Os::MacOs), app_words_on(Os::Windows));
        let labels = |words: &AppWords| {
            [
                words.window.email.clone(),
                words.window.plan.clone(),
                words.window.sign_in.clone(),
                words.window.warm_up.clone(),
            ]
        };
        assert_eq!(labels(&mac), ["Email", "Plan", "Sign-In", "Warm-Up"]);
        assert_eq!(labels(&windows), ["Email", "Plan", "Sign-in", "Warm-up"]);
        assert_eq!(mac.window.inspector.as_deref(), Some("Inspector"));
        assert_eq!(
            (
                mac.window.no_selection.as_deref(),
                windows.window.no_selection
            ),
            (Some("No Account Selected"), None)
        );
        assert_eq!(
            windows.settings.diagnostics_about.as_deref(),
            Some("Check how Pitboard, Claude Code and Codex are set up on this PC.")
        );
        assert_eq!(
            (
                mac.settings.uninstall.as_str(),
                windows.settings.uninstall.as_str()
            ),
            ("Uninstall", "Uninstall Pitboard")
        );
        assert_eq!(
            (mac.settings.troubleshoot, mac.settings.path.as_str()),
            (None, "Path")
        );
        assert_eq!(
            (windows.window.inspector, windows.window.copy.as_deref()),
            (None, Some("Copy"))
        );
    }
}
