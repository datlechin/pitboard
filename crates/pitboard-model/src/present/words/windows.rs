//! What the account windows say: opening a site as an account, the Open Link window, and
//! downloads.

use super::titled;
use crate::Front;

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
pub(crate) fn no_site_account(front: Front, site: &str) -> (String, String) {
    (
        titled(
            front,
            &format!("No {site} Account"),
            &format!("No {site} account"),
        ),
        format!(
            "None of the accounts Pitboard has opens {site}. Add one, and this link waits here \
             until you choose it."
        ),
    )
}

// What the Open Link window says on a front with no account windows: its title, and why the
// browser opens the link.
pub(crate) fn no_windows(front: Front, site: &str) -> (String, String) {
    (
        titled(front, "Open in Your Browser", "Open in your browser"),
        format!(
            "Pitboard has no {site} window of its own here, so your browser opens this link as \
             the account it’s signed in to."
        ),
    )
}

pub(crate) fn open_in_browser(front: Front) -> String {
    titled(front, "Open in Browser", "Open in browser")
}

pub(crate) fn cannot_open_link(front: Front) -> String {
    titled(
        front,
        pitboard_sites::LinkRefusal::TITLE,
        "Can’t open this link",
    )
}

// A Linux desktop has no Share menu, so a link is pasted or dropped there.
pub(crate) fn no_link(front: Front) -> (String, String) {
    let sites = pitboard_sites::Site::names(pitboard_sites::Conjunction::Or);
    let detail = match (front, front.offers_account_windows()) {
        (Front::MacMenuBar | Front::WindowsNotificationArea, _) => format!(
            "Share a {sites} page with Pitboard from your browser’s Share menu to open it as one \
             of your accounts."
        ),
        (Front::Gnome, true) => {
            format!("Paste a {sites} link, or drop one here, to open it as one of your accounts.")
        }
        (Front::Gnome, false) => {
            format!("Paste a {sites} link, or drop one here, to open it in your browser.")
        }
    };
    (titled(front, "No Link to Open", "No link to open"), detail)
}

pub(crate) const DOWNLOAD: &str = "Download";

pub(crate) const CLEAR: &str = "Clear";

pub(crate) const DOWNLOADED: &str = "Downloaded";

pub(crate) const CANCELLED: &str = "Cancelled";

// Asked before quitting while downloads run: its title, what quitting does, and its button.
pub(crate) fn quit_with_downloads(
    front: Front,
    running: u32,
) -> (String, &'static str, &'static str) {
    let (quit, stops) = match front {
        Front::WindowsNotificationArea => (
            "Exit",
            "Exiting Pitboard stops them, and they won’t resume.",
        ),
        Front::MacMenuBar | Front::Gnome => (
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

#[cfg(test)]
mod tests {
    use super::*;

    // GNOME says its account windows' words as a Mac does, in its header capitalisation, and
    // with none of its own, what the browser opens instead.
    #[test]
    fn gnome_words_the_account_windows_as_a_mac_does() {
        let gnome = Front::Gnome;
        assert_eq!(
            quit_with_downloads(gnome, 2),
            (
                "2 downloads are in progress. Quit anyway?".into(),
                "Quitting Pitboard stops them, and they won’t resume.",
                "Quit"
            )
        );
        assert_eq!(open_in_browser(gnome), "Open in Browser");
        assert_eq!(
            no_site_account(gnome, "claude.ai").0,
            "No claude.ai Account"
        );
        assert_eq!(cannot_open_link(gnome), "Can’t Open This Link");
        assert_eq!(
            no_windows(gnome, "claude.ai"),
            (
                "Open in Your Browser".into(),
                "Pitboard has no claude.ai window of its own here, so your browser opens this \
                 link as the account it’s signed in to."
                    .into()
            )
        );
        assert_eq!(
            no_link(gnome),
            (
                "No Link to Open".into(),
                "Paste a claude.ai or chatgpt.com link, or drop one here, to open it in your \
                 browser."
                    .into()
            )
        );
    }
}
