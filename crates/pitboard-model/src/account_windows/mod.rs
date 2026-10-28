//! The rules of an account's window on its site, for both apps: which accounts have a window
//! and which store keeps each one's data, where a page may go, what a window says, what a
//! download is called, and what a page may do. Each app's web code asks them as its engine
//! asks it, WebKit on macOS and WebView2 on Windows, on its main thread: each answers at once
//! from what it is given, and only `download_destination` looks at the file system. The Linux
//! app has no account windows yet, so nothing offers it one (`offered_windows`).
//!
//! What follows the app's reads is the model's, in `model/windows.rs`: which windows close
//! and which stores go after a read, the records of stores and last pages, which `records.rs`
//! reads and writes, the links waiting for a window, the account picker and the downloads.
//!
//! What differs by front is a `match` on the app's `Front`, its engine or its system, so a
//! front added does not compile until each is said for it. Each such fact takes the front as
//! an argument, so the tests say every front's on any of them.

mod accounts;
mod downloads;
mod notes;
mod pages;
mod policy;
pub(crate) mod records;
pub(crate) mod stores;

pub use accounts::{SiteMenu, WindowAccount};
pub(crate) use accounts::{
    forget_message, offered_windows, site_menus, window_accounts, windows_of_account,
};
pub use downloads::{download_destination, download_host, download_question};
pub use notes::{WindowNoteKind, opening_note, remove_data_alert, window_note};
pub use pages::{
    PagePermission, ProcessEnded, SignInWindowSize, after_content_process_ended, dialog_title,
    page_may_close, page_may_use, sign_in_window_size,
};
pub use policy::{
    Asker, FrameOrigin, NavigationDecision, NavigationPolicy, NavigationRequest, NavigationTarget,
    PageRole, ResponseDecision, ResponseFacts, decide_navigation, decide_response, frame_asker,
    window_home,
};
pub(crate) use policy::{is_site_page, window_address};

use crate::Front;

/// What an alert says: its title, and the sentences below it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AlertText {
    pub title: String,
    pub message: String,
    // for a failure, what to do about it; a question asks its own
    pub next: Option<crate::present::NextStep>,
}

/// How a sentence names the machine Pitboard runs on, which keeps what a window keeps.
pub(crate) fn this_machine(front: Front) -> &'static str {
    match front {
        Front::MacMenuBar => "this Mac",
        // One front on every Linux desktop, so no desktop's own name for it.
        Front::Gnome => "this computer",
        // As Windows 11 names the machine in its own Settings and File Explorer.
        Front::WindowsNotificationArea => "this PC",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each system's machine is called what the system calls it: a Mac, and a PC on
    /// Windows, as the owner decided.
    #[test]
    fn a_machine_is_called_what_its_system_calls_it() {
        assert_eq!(this_machine(Front::MacMenuBar), "this Mac");
        assert_eq!(this_machine(Front::Gnome), "this computer");
        assert_eq!(this_machine(Front::WindowsNotificationArea), "this PC");
    }
}
