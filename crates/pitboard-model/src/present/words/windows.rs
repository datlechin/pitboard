//! What the account windows say: opening a site as an account, the Open Link window, and
//! downloads.

use super::titled;
use pitboard_core::host::Os;

pub(crate) fn open_site(site: &str) -> String {
    format!("Open {site}")
}

// The one account of a site with a window: "Open claude.ai as work".
pub(crate) fn open_site_as(site: &str, label: &str) -> String {
    format!("Open {site} as {label}")
}

// What a window is called until it shows its account's page.
pub(crate) const ACCOUNT: &str = "Account";

pub(crate) fn open_link_as(site: &str) -> String {
    format!("Open this {site} link as:")
}

// An account in the Open Link window, with a window of its own open.
pub(crate) fn window_open(email: &str) -> String {
    format!("{email}, window open")
}

pub(crate) const OPEN: &str = "Open";

// What the Open Link window says where no account opens `site`: its title, and what to do.
pub(crate) fn no_site_account(os: Os, site: &str) -> (String, String) {
    (
        titled(
            os,
            &format!("No {site} Account"),
            &format!("No {site} account"),
        ),
        format!(
            "None of the accounts Pitboard has opens {site}. Add one, and this link waits here \
             until you choose it."
        ),
    )
}

pub(crate) fn open_in_browser(os: Os) -> String {
    titled(os, "Open in Browser", "Open in browser")
}

pub(crate) fn cannot_open_link(os: Os) -> String {
    titled(
        os,
        pitboard_sites::LinkRefusal::TITLE,
        "Can’t open this link",
    )
}

pub(crate) fn no_link(os: Os) -> (String, String) {
    (
        titled(os, "No Link to Open", "No link to open"),
        format!(
            "Share a {} page with Pitboard from your browser’s Share menu to open it as one of \
             your accounts.",
            pitboard_sites::Site::names(pitboard_sites::Conjunction::Or)
        ),
    )
}

pub(crate) const DOWNLOAD: &str = "Download";

pub(crate) const CLEAR: &str = "Clear";

pub(crate) const DOWNLOADED: &str = "Downloaded";

pub(crate) const CANCELLED: &str = "Cancelled";

// Asked before quitting while downloads run: its title, what quitting does, and its button.
pub(crate) fn quit_with_downloads(os: Os, running: u32) -> (String, &'static str, &'static str) {
    let (quit, stops) = match os {
        Os::Windows => (
            "Exit",
            "Exiting Pitboard stops them, and they won’t resume.",
        ),
        Os::MacOs | Os::Linux => (
            "Quit",
            "Quitting Pitboard stops them, and they won’t resume.",
        ),
    };
    let title = if running == 1 {
        format!("A download is in progress. {quit} anyway?")
    } else {
        format!("{running} downloads are in progress. {quit} anyway?")
    };
    (title, stops, quit)
}
