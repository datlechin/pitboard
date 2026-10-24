//! The rules of an account's window on its site, for both apps: which accounts have a window
//! and which store keeps each one's data, where a page may go, what a window says, what a
//! download is called, and what a page may do. Each app's web code asks them as its engine
//! asks it, WebKit on macOS and WebView2 on Windows, on its main thread: each answers at once
//! from what it is given, and only `download_destination` looks at the file system.
//!
//! What follows the app's reads is the model's, in `model/windows.rs`: which windows close
//! and which stores go after a read, the records of stores and last pages, which `records.rs`
//! reads and writes, the links waiting for a window, the account picker and the downloads.
//!
//! What differs by system is a `match` on `host::OS`, as the core's facts about the system
//! are, so a system added there does not compile until each is said for it. Each such fact
//! takes the system as an argument, so the tests say every system's on any of them.

mod accounts;
mod downloads;
mod notes;
mod pages;
mod policy;
pub(crate) mod records;
pub(crate) mod stores;

pub use accounts::{SiteMenu, WindowAccount};
pub(crate) use accounts::{forget_message_on, site_menus, window_accounts, windows_of_account};
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

use pitboard_core::host::Os;

/// What an alert says: its title, and the sentences below it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct AlertText {
    pub title: String,
    pub message: String,
}

/// How a sentence names the machine Pitboard runs on, which keeps what a window keeps.
pub(crate) fn this_machine(os: Os) -> &'static str {
    match os {
        Os::MacOs => "this Mac",
        // No app runs on Linux.
        Os::Linux => "this computer",
        // As Windows 11 names the machine in its own Settings and File Explorer.
        Os::Windows => "this PC",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each system's machine is called what the system calls it: a Mac, and a PC on
    /// Windows, as the owner decided.
    #[test]
    fn a_machine_is_called_what_its_system_calls_it() {
        assert_eq!(this_machine(Os::MacOs), "this Mac");
        assert_eq!(this_machine(Os::Linux), "this computer");
        assert_eq!(this_machine(Os::Windows), "this PC");
    }
}
