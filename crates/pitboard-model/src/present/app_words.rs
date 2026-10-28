// Every control and sentence the apps draw on their own: what every front draws, in its case,
// and what one front alone draws, in that front's own catalogue.

use super::SetupLink;
use super::words::{self, titled};
use crate::Front;
use pitboard_core::provider::ProviderId;

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
    // what VoiceOver and Narrator call the spinner while the checks run
    pub checking: String,
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
    // "Open Pitboard at Login", "Start when you sign in to Windows"
    pub open_at_login: String,
    pub open_at_login_note: String,
    pub notifications: String,
    pub notifications_off: String,
    pub open_notification_settings: String,
    // over General's update settings
    pub updates: String,
    pub check_for_updates_automatically: String,
    pub version: String,
    pub check_now: String,
    pub keep_saved_sign_ins_fresh: String,
    pub command_line: String,
    // beside where the command line is
    pub path: String,
    // what VoiceOver and Narrator call the spinner while Settings looks for one
    pub looking_for_command_line: String,
    // over Uninstall Pitboard… on macOS, beside Uninstall… on Windows
    pub uninstall: String,
    pub uninstall_note: String,
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

// What the macOS app alone draws.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MacMenuBarWords {
    // the toolbar's button that shows and hides the inspector
    pub inspector: String,
    pub inspector_help: String,
    // what the inspector says while no account is selected
    pub no_selection: String,
    // Settings waits for Login Items to allow it, with a button to them
    pub login_items_waiting: String,
    pub open_login_items: String,
    // what the menu bar item shows, and its choices
    pub menu_bar_shows: String,
    pub account_and_usage: String,
    pub usage_only: String,
    pub icon_only: String,
    // under General's update settings
    pub download_and_install_automatically: String,
    pub install_command_line: String,
    pub link_failed: String,
    pub cannot_link_command_line: String,
    // what the app asks of itself once Pitboard's own data is gone, `trash_says` the message
    pub move_to_trash_question: String,
    pub move_to_trash: String,
    // a copy Homebrew installed, which Homebrew removes
    pub remove_with_homebrew: String,
    pub homebrew_removes_it: String,
    // the docs' table of how to remove Pitboard by how it was installed
    pub homebrew_help: SetupLink,
    // what VoiceOver calls the link the Open Link window opens
    pub link: String,
    // what a Shortcuts action says that the model's answers don't
    pub not_read_yet: String,
    pub none_in_use: String,
}

// What the Windows app alone draws.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WindowsNotificationAreaWords {
    // what Narrator calls the notification area's menu
    pub icon_menu: String,
    // a check's detail's own menu, and an expanded row's email
    pub copy: String,
    pub select_all: String,
    // what Narrator calls the checks
    pub checks: String,
    // a Debug build's refusal of a link outside a fixture
    pub debug_link_refused: String,
    // the Settings page's last section
    pub about: String,
    // a Debug build, which never writes the Run key
    pub debug_never_starts: String,
    // Startup settings turned it off, with a link to them
    pub startup_turned_off: String,
    pub open_startup_settings: String,
    pub notifications_off_for_every_app: String,
    pub notifications_off_by_policy: String,
    // a copy that has no update to check for yet, or is a beta
    pub no_release_to_update_to: String,
    pub beta_updates: String,
    // a copy that cannot update itself
    pub update_key_invalid: String,
    pub win_sparkle_missing: String,
    pub notifications_unoffered: String,
    // the Settings page's section with a card each that opens Diagnostics and Activity
    pub troubleshoot: String,
    pub diagnostics_about: String,
    pub activity_about: String,
    // what Narrator calls the warm-up hours' pickers
    pub warm_from: String,
    pub warm_to: String,
    // an account window whose page could not be made
    pub could_not_open: String,
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
pub fn app_words(front: Front) -> AppWords {
    let add_account_asks = match front {
        Front::WindowsNotificationArea => true,
        Front::MacMenuBar | Front::Gnome => false,
    };
    AppWords {
        app_name: words::PITBOARD.into(),
        menu: MenuWords {
            add_account: words::add_account(front, true),
            refresh: words::REFRESH.into(),
            open_pitboard: titled(front, "Open Pitboard", "Open Pitboard"),
            settings: titled(front, "Settings…", "Settings"),
            check_for_updates: titled(front, "Check for Updates…", "Check for updates"),
            about: titled(front, "About Pitboard", "About Pitboard"),
            help: titled(front, "Pitboard Help", "Pitboard help"),
            quit: match front {
                Front::WindowsNotificationArea => "Exit",
                Front::MacMenuBar | Front::Gnome => "Quit Pitboard",
            }
            .into(),
            account: words::ACCOUNT.into(),
            copy_email: titled(front, "Copy Email Address", "Copy email address"),
        },
        window: WindowWords {
            accounts: "Accounts".into(),
            activity: "Activity".into(),
            diagnostics: "Diagnostics".into(),
            refresh: words::REFRESH.into(),
            refresh_help: "Read every account’s usage again".into(),
            add_account: words::add_account(front, add_account_asks),
            add_account_help: "Sign in to another account".into(),
            check_again: titled(front, "Check Again", "Check again"),
            check_again_help: "Make every check again".into(),
            copy_report: titled(front, "Copy Report", "Copy report"),
            copy_report_help: "Copy the checks for a bug report, with email addresses and \
                               account identifiers hidden"
                .into(),
            open_link: titled(front, "Open Link", "Open link"),
            cancel: words::CANCEL.into(),
            close: "Close".into(),
            ok: "OK".into(),
            dismiss: words::DISMISS.into(),
            email: "Email".into(),
            plan: "Plan".into(),
            sign_in: titled(front, "Sign-In", "Sign-in"),
            warm_up: words::warm_up(front),
            checking: "Checking".into(),
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
            open_at_login: words::open_at_login(front).0.into(),
            open_at_login_note: words::open_at_login(front).1.into(),
            notifications: words::NOTIFICATIONS.into(),
            notifications_off: match front {
                Front::WindowsNotificationArea => {
                    "Notifications from Pitboard are turned off in Settings > System > \
                     Notifications."
                }
                Front::MacMenuBar | Front::Gnome => {
                    "Notifications from Pitboard are turned off in System Settings."
                }
            }
            .into(),
            open_notification_settings: titled(
                front,
                "Open Notification Settings…",
                "Open notification settings",
            ),
            updates: "Updates".into(),
            check_for_updates_automatically: titled(
                front,
                "Check for Updates Automatically",
                "Check for updates automatically",
            ),
            version: "Version".into(),
            check_now: titled(front, "Check Now", "Check now"),
            keep_saved_sign_ins_fresh: words::keep_saved_sign_ins_fresh(front),
            command_line: titled(front, "Command Line", "Command line"),
            path: "Path".into(),
            looking_for_command_line: "Looking for pitboard".into(),
            uninstall: match front {
                Front::WindowsNotificationArea => "Uninstall Pitboard",
                Front::MacMenuBar | Front::Gnome => words::UNINSTALL,
            }
            .into(),
            uninstall_note: "Deletes Pitboard’s saved sign-ins and settings, and leaves each tool \
                             signed in to the account in use."
                .into(),
            from: "From".into(),
            to: "To".into(),
            help: titled(front, "Pitboard Help", "Pitboard help"),
            report_a_problem: titled(front, "Report a Problem", "Report a problem"),
            source_code: titled(front, "Source Code on GitHub", "Source code on GitHub"),
            license: "Apache License 2.0".into(),
            copyright: "Copyright 2026 the Pitboard authors".into(),
        },
        sheet: SheetWords {
            tool: "Tool".into(),
            account: words::ACCOUNT.into(),
            name: "Name".into(),
            sign_in_page: titled(front, "Sign-In Page", "Sign-in page"),
            copy_link: titled(front, "Copy Link", "Copy link"),
            open: words::OPEN.into(),
            code: "Code".into(),
            paste_code: "Paste the code from your browser".into(),
            submit_code: titled(front, "Submit Code", "Submit code"),
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
            reload_page: titled(front, "Reload Page", "Reload page"),
            downloads: "Downloads".into(),
            downloads_help: "Show this window’s downloads".into(),
            could_not_load: titled(front, "Couldn’t Load the Page", "Couldn’t load the page"),
            try_again: words::try_again(front),
            remove_website_data: titled(front, "Remove Website Data…", "Remove website data…"),
            remove: words::REMOVE.into(),
            actual_size: titled(front, "Actual Size", "Actual size"),
            zoom_in: titled(front, "Zoom In", "Zoom in"),
            zoom_out: titled(front, "Zoom Out", "Zoom out"),
            go: "Go".into(),
            download: words::DOWNLOAD.into(),
            stop_download_help: "Stop this download".into(),
            show_file: match front {
                Front::WindowsNotificationArea => "Show in folder",
                Front::MacMenuBar | Front::Gnome => "Show in Finder",
            }
            .into(),
            show_file_help: match front {
                Front::WindowsNotificationArea => "Show the file in its folder",
                Front::MacMenuBar | Front::Gnome => "Show the file in Finder",
            }
            .into(),
        },
    }
}

#[uniffi::export]
pub fn mac_menu_bar_words() -> MacMenuBarWords {
    MacMenuBarWords {
        inspector: "Inspector".into(),
        inspector_help: "Show or hide the selected account’s details".into(),
        no_selection: "No Account Selected".into(),
        login_items_waiting: "macOS is waiting for you to allow Pitboard in Login Items.".into(),
        open_login_items: "Open Login Items Settings…".into(),
        menu_bar_shows: "Menu Bar Shows".into(),
        account_and_usage: "Account and Usage".into(),
        usage_only: "Usage Only".into(),
        icon_only: "Icon Only".into(),
        download_and_install_automatically: "Download and Install Automatically".into(),
        install_command_line: "Install Command Line Tool…".into(),
        link_failed: "The link couldn’t be made.".into(),
        cannot_link_command_line: "This copy of Pitboard can’t link the command line inside it."
            .into(),
        move_to_trash_question: "Move Pitboard to the Trash?".into(),
        move_to_trash: "Move to Trash".into(),
        remove_with_homebrew: "Remove Pitboard with Homebrew".into(),
        homebrew_removes_it: format!(
            "{DELETED} Homebrew installed the app, so Homebrew removes it."
        ),
        homebrew_help: SetupLink {
            title: words::learn_more(Front::MacMenuBar),
            link: REMOVE_PAGE.into(),
        },
        link: "Link".into(),
        not_read_yet: "Pitboard hasn’t read its accounts yet. Try again in a moment.".into(),
        none_in_use: words::NONE_IN_USE.into(),
    }
}

#[uniffi::export]
pub fn windows_notification_area_words() -> WindowsNotificationAreaWords {
    let names = crate::tools_on(Front::WindowsNotificationArea.os())
        .into_iter()
        .map(ProviderId::name);
    WindowsNotificationAreaWords {
        icon_menu: "Pitboard menu".into(),
        copy: "Copy".into(),
        select_all: "Select all".into(),
        checks: "Checks".into(),
        debug_link_refused: "A Debug build opens a pitboard-debug link only in a fixture.".into(),
        about: "About".into(),
        debug_never_starts: "A Debug build never starts when you sign in to Windows.".into(),
        startup_turned_off: "Turned off in Settings > Apps > Startup, where you can turn it on \
                             again."
            .into(),
        open_startup_settings: "Open Startup settings".into(),
        notifications_off_for_every_app: "Notifications are turned off for every app in Settings \
                                          > System > Notifications."
            .into(),
        notifications_off_by_policy: "Group Policy turns off app notifications on this PC.".into(),
        no_release_to_update_to: "No full release of Pitboard for Windows is out yet, so there \
                                  is no update to check for."
            .into(),
        beta_updates: "A beta updates itself to 1.0.0 once that is out. Install a later beta by \
                       hand."
            .into(),
        update_key_invalid: "This copy of Pitboard can’t update itself: the update key it was \
                             built with isn’t a valid key. Install a later version by hand."
            .into(),
        win_sparkle_missing: "This copy of Pitboard can’t update itself: WinSparkle.dll, which \
                              updates it, is missing. Install Pitboard again."
            .into(),
        notifications_unoffered: "Windows offers this process no app notifications.".into(),
        troubleshoot: "Troubleshoot".into(),
        diagnostics_about: format!(
            "Check how {} are set up on this PC.",
            pitboard_core::words::listed(std::iter::once(words::PITBOARD).chain(names))
        ),
        activity_about: "Every switch and change Pitboard made.".into(),
        warm_from: "Warm up from".into(),
        warm_to: "Warm up to".into(),
        could_not_open: "Couldn’t open this window".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::present::testing::{mechanism_words, strings_of, title_cased};

    // Each shipping front's words of one control.
    fn pairs() -> Vec<(String, String)> {
        let (mac, windows) = (
            strings_of(&app_words(Front::MacMenuBar)),
            strings_of(&app_words(Front::WindowsNotificationArea)),
        );
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
        let words = strings_of(&app_words(Front::WindowsNotificationArea));
        for said in words
            .into_iter()
            .chain(strings_of(&windows_notification_area_words()))
        {
            assert!(title_cased(&said, &NAMES).is_empty(), "{said:?}");
        }
    }

    #[test]
    fn no_control_says_park_put_away_give_up_enrol_record_or_settle() {
        let said = [
            strings_of(&app_words(Front::MacMenuBar)),
            strings_of(&app_words(Front::WindowsNotificationArea)),
            strings_of(&mac_menu_bar_words()),
            strings_of(&windows_notification_area_words()),
        ];
        for said in said.into_iter().flatten() {
            assert!(mechanism_words(&said).is_empty(), "{said}");
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
        let (mac, windows) = (
            app_words(Front::MacMenuBar),
            app_words(Front::WindowsNotificationArea),
        );
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
        let own = windows_notification_area_words();
        assert_eq!(
            own.debug_never_starts,
            "A Debug build never starts when you sign in to Windows."
        );
        assert_eq!(
            own.debug_link_refused,
            "A Debug build opens a pitboard-debug link only in a fixture."
        );
    }

    #[test]
    fn each_front_words_a_control_its_own_way() {
        let (mac, windows) = (
            app_words(Front::MacMenuBar),
            app_words(Front::WindowsNotificationArea),
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

    // Every word one shipping app alone draws, as it drew each before its front had a catalogue.
    #[test]
    fn each_shipping_fronts_catalogue_says_what_its_app_drew() {
        let mac = mac_menu_bar_words();
        assert_eq!(
            [
                mac.inspector,
                mac.inspector_help,
                mac.no_selection,
                mac.login_items_waiting,
                mac.open_login_items,
                mac.menu_bar_shows,
                mac.account_and_usage,
                mac.usage_only,
                mac.icon_only,
                mac.download_and_install_automatically,
                mac.install_command_line,
                mac.link_failed,
                mac.cannot_link_command_line,
                mac.move_to_trash_question,
                mac.move_to_trash,
                mac.remove_with_homebrew,
                mac.homebrew_removes_it,
                mac.link,
                mac.not_read_yet,
                mac.none_in_use,
            ],
            [
                "Inspector",
                "Show or hide the selected account’s details",
                "No Account Selected",
                "macOS is waiting for you to allow Pitboard in Login Items.",
                "Open Login Items Settings…",
                "Menu Bar Shows",
                "Account and Usage",
                "Usage Only",
                "Icon Only",
                "Download and Install Automatically",
                "Install Command Line Tool…",
                "The link couldn’t be made.",
                "This copy of Pitboard can’t link the command line inside it.",
                "Move Pitboard to the Trash?",
                "Move to Trash",
                "Remove Pitboard with Homebrew",
                "Its saved sign-ins and settings are deleted. Homebrew installed the app, so \
                 Homebrew removes it.",
                "Link",
                "Pitboard hasn’t read its accounts yet. Try again in a moment.",
                "No account is in use.",
            ]
        );
        assert_eq!(
            mac.homebrew_help,
            SetupLink {
                title: "Learn More".into(),
                link: "https://docs.usepitboard.com/install/remove#remove-pitboard-itself".into(),
            }
        );
        let windows = windows_notification_area_words();
        assert_eq!(
            [
                windows.icon_menu,
                windows.copy,
                windows.select_all,
                windows.checks,
                windows.debug_link_refused,
                windows.about,
                windows.debug_never_starts,
                windows.startup_turned_off,
                windows.open_startup_settings,
                windows.notifications_off_for_every_app,
                windows.notifications_off_by_policy,
                windows.no_release_to_update_to,
                windows.beta_updates,
                windows.update_key_invalid,
                windows.win_sparkle_missing,
                windows.notifications_unoffered,
                windows.troubleshoot,
                windows.diagnostics_about,
                windows.activity_about,
                windows.warm_from,
                windows.warm_to,
                windows.could_not_open,
            ],
            [
                "Pitboard menu",
                "Copy",
                "Select all",
                "Checks",
                "A Debug build opens a pitboard-debug link only in a fixture.",
                "About",
                "A Debug build never starts when you sign in to Windows.",
                "Turned off in Settings > Apps > Startup, where you can turn it on again.",
                "Open Startup settings",
                "Notifications are turned off for every app in Settings > System > Notifications.",
                "Group Policy turns off app notifications on this PC.",
                "No full release of Pitboard for Windows is out yet, so there is no update to \
                 check for.",
                "A beta updates itself to 1.0.0 once that is out. Install a later beta by hand.",
                "This copy of Pitboard can’t update itself: the update key it was built with isn’t \
                 a valid key. Install a later version by hand.",
                "This copy of Pitboard can’t update itself: WinSparkle.dll, which updates it, is \
                 missing. Install Pitboard again.",
                "Windows offers this process no app notifications.",
                "Troubleshoot",
                "Check how Pitboard, Claude Code and Codex are set up on this PC.",
                "Every switch and change Pitboard made.",
                "Warm up from",
                "Warm up to",
                "Couldn’t open this window",
            ]
        );
    }

    // A spinner says nothing on screen, so each system's screen reader is told what it waits for.
    #[test]
    fn a_spinner_is_named_on_each_system() {
        for front in [Front::MacMenuBar, Front::WindowsNotificationArea] {
            let words = app_words(front);
            assert_eq!(words.window.checking, "Checking");
            assert_eq!(
                words.settings.looking_for_command_line,
                "Looking for pitboard"
            );
        }
    }

    // Plan 6.5: once Pitboard's own data is gone, the macOS app takes itself to the Trash, or
    // points a copy Homebrew installed at Homebrew.
    #[test]
    fn the_macos_uninstall_ends_with_the_app_itself() {
        let mac = mac_menu_bar_words();
        assert_eq!(
            (
                mac.move_to_trash_question.as_str(),
                mac.move_to_trash.as_str()
            ),
            ("Move Pitboard to the Trash?", "Move to Trash")
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
                mac.remove_with_homebrew.as_str(),
                mac.homebrew_removes_it.as_str()
            ),
            (
                "Remove Pitboard with Homebrew",
                "Its saved sign-ins and settings are deleted. Homebrew installed the app, so \
                 Homebrew removes it."
            )
        );
        assert_eq!(
            mac.homebrew_help,
            SetupLink {
                title: "Learn More".into(),
                link: "https://docs.usepitboard.com/install/remove#remove-pitboard-itself".into()
            }
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
    }

    // Plan 3: a command that asks for more says so on Windows; the macOS toolbar's has no ellipsis.
    #[test]
    fn the_windows_command_bar_says_add_account_asks_for_more() {
        let (mac, windows) = (
            app_words(Front::MacMenuBar),
            app_words(Front::WindowsNotificationArea),
        );
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
        let (mac, windows) = (
            app_words(Front::MacMenuBar),
            app_words(Front::WindowsNotificationArea),
        );
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
        assert_eq!(mac_menu_bar_words().inspector, "Inspector");
        assert_eq!(mac_menu_bar_words().no_selection, "No Account Selected");
        assert_eq!(
            windows_notification_area_words().diagnostics_about,
            "Check how Pitboard, Claude Code and Codex are set up on this PC."
        );
        assert_eq!(
            (
                mac.settings.uninstall.as_str(),
                windows.settings.uninstall.as_str()
            ),
            ("Uninstall", "Uninstall Pitboard")
        );
        assert_eq!(mac.settings.path, "Path");
        assert_eq!(windows_notification_area_words().copy, "Copy");
    }
}
