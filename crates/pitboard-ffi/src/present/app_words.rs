// Every control and sentence the apps draw on their own; one only a system shows is None elsewhere.

use super::words::{self, titled};
use pitboard_core::host::{OS, Os};

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AppWords {
    pub menu: MenuWords,
    pub window: WindowWords,
    pub activity: ActivityWords,
    pub settings: SettingsWords,
    pub sheet: SheetWords,
    pub account_window: AccountWindowWords,
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
    pub open_link: String,
    pub cancel: String,
    pub close: String,
    pub ok: String,
    pub dismiss: String,
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
    pub check_for_updates_automatically: String,
    pub download_and_install_automatically: Option<String>,
    pub version: String,
    pub check_now: String,
    // a Windows copy that has no update to check for yet, or is a beta
    pub no_release_to_update_to: Option<String>,
    pub beta_updates: Option<String>,
    pub keep_saved_sign_ins_fresh: String,
    pub command_line: String,
    pub install_command_line: Option<String>,
    pub link_failed: Option<String>,
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
}

#[uniffi::export]
pub fn app_words() -> AppWords {
    app_words_on(OS)
}

pub(crate) fn app_words_on(os: Os) -> AppWords {
    let mac = |words: &str| (os != Os::Windows).then(|| words.to_owned());
    let windows = |words: &str| (os == Os::Windows).then(|| words.to_owned());
    AppWords {
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
            add_account: words::add_account(os, false),
            add_account_help: "Sign in to another account".into(),
            check_again: titled(os, "Check Again", "Check again"),
            check_again_help: "Make every check again".into(),
            open_link: titled(os, "Open Link", "Open link"),
            cancel: words::CANCEL.into(),
            close: "Close".into(),
            ok: "OK".into(),
            dismiss: words::DISMISS.into(),
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
            keep_saved_sign_ins_fresh: titled(
                os,
                "Keep Saved Sign-Ins Fresh",
                "Keep saved sign-ins fresh",
            ),
            command_line: titled(os, "Command Line", "Command line"),
            install_command_line: mac("Install Command Line Tool…"),
            link_failed: mac("The link couldn’t be made."),
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
            download: "Download".into(),
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
            ("Show the file in Finder", "Show the file in its folder"),
        ];
        for (mac, windows) in pairs() {
            if mac.is_empty() || windows.is_empty() {
                continue;
            }
            let alike = mac.to_lowercase() == windows.to_lowercase()
                || mac.to_lowercase().strip_suffix('…') == Some(&windows.to_lowercase());
            let apart = named_apart.contains(&(mac.as_str(), windows.as_str()));
            assert!(alike || apart, "{mac:?} and {windows:?}");
        }
    }

    // Past a sentence's first word, a capital is a name's, or a Settings page's the words name.
    #[test]
    fn windows_says_every_control_in_sentence_case() {
        const NAMES: [&str; 14] = [
            "Pitboard",
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
    }
}
