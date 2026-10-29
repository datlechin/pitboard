//! Which app the model serves: the surface it lives in, and what follows from that alone.
//!
//! An app names its front once, in `AppLaunch`, and the model words and decides by it what
//! differs from one app to another: a control's case, what the machine is called, where a
//! sheet comes up, whether it has account windows and their engine, whether it takes a link
//! pasted on its window, whether it has a status item and whether it offers quick actions.
//! Each is a `match` on `Front` with no `_` arm, so a front added does not compile until each
//! is said for it. What the core decides about the system the app runs on, such as which tools
//! it supports or how its file system compares names, is asked of `os()`.

use pitboard_core::host::Os;

/// The app a model serves, by the surface it lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum Front {
    /// The macOS app, in the menu bar.
    MacMenuBar,
    /// The Windows app, in the notification area.
    WindowsNotificationArea,
    /// The libadwaita app, on every Linux desktop.
    Gnome,
}

/// What a front's system gives it to show Pitboard in beside its windows and notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct Surfaces {
    /// An item the system keeps for Pitboard while it runs: the macOS menu bar's, the Windows
    /// notification area's icon.
    pub status_item: bool,
}

/// The web engine an app's account windows load their pages in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Engine {
    WebKit,
    WebView2,
    WebKitGtk,
}

impl Front {
    /// Each front's own: its account windows are the system's web view.
    pub fn engine(self) -> Engine {
        match self {
            Front::MacMenuBar => Engine::WebKit,
            Front::WindowsNotificationArea => Engine::WebView2,
            Front::Gnome => Engine::WebKitGtk,
        }
    }

    /// The system the app runs on, as the core knows it.
    pub fn os(self) -> Os {
        match self {
            Front::MacMenuBar => Os::MacOs,
            Front::WindowsNotificationArea => Os::Windows,
            Front::Gnome => Os::Linux,
        }
    }

    /// What the front has from the start, until it says otherwise with
    /// `Intent::SurfacesChanged`: a front whose system can give or take a status item while
    /// it runs starts with none.
    pub fn surfaces(self) -> Surfaces {
        match self {
            Front::MacMenuBar | Front::WindowsNotificationArea => Surfaces { status_item: true },
            Front::Gnome => Surfaces { status_item: false },
        }
    }

    /// Whether the front opens an account's site in a window of its own.
    pub fn offers_account_windows(self) -> bool {
        match self {
            Front::MacMenuBar | Front::WindowsNotificationArea => true,
            // Not until the distributions it runs on carry WebKitGTK 2.56
            // (docs/facts/linux-app.md, Account windows).
            Front::Gnome => false,
        }
    }

    /// Whether the front takes a link pasted or dropped on its window, which may be a site's
    /// own link as well as a Pitboard link: the Linux app's Open Link, which no Share extension
    /// writes for.
    pub fn takes_pasted_links(self) -> bool {
        match self {
            // A link reaches either from its Share extension alone, as a Pitboard link.
            Front::MacMenuBar | Front::WindowsNotificationArea => false,
            Front::Gnome => true,
        }
    }

    /// Whether the front offers what the model's quick actions ask from outside it: the
    /// Windows Jump List's tasks, GNOME's search and KRunner's results.
    pub fn keeps_quick_actions(self) -> bool {
        match self {
            Front::WindowsNotificationArea | Front::Gnome => true,
            Front::MacMenuBar => false,
        }
    }

    // Whether a sheet, a question or a failure comes up over the welcome while it is shown,
    // whose Add starts them, so its Continue is still in front.
    pub(crate) fn welcome_hosts_sheets(self) -> bool {
        match self {
            Front::MacMenuBar => true,
            // The main window hosts them.
            Front::WindowsNotificationArea => false,
            // The welcome is a dialog over the main window, and libadwaita puts each dialog
            // presented after it over it.
            Front::Gnome => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The Linux app's windows wait on WebKitGTK, its Open Link takes a link pasted or dropped,
    // and its search and KRunner find quick actions.
    #[test]
    fn each_front_says_what_it_offers() {
        assert_eq!(
            [
                Front::MacMenuBar.offers_account_windows(),
                Front::WindowsNotificationArea.offers_account_windows(),
                Front::Gnome.offers_account_windows(),
            ],
            [true, true, false]
        );
        assert_eq!(
            [
                Front::MacMenuBar.takes_pasted_links(),
                Front::WindowsNotificationArea.takes_pasted_links(),
                Front::Gnome.takes_pasted_links(),
            ],
            [false, false, true]
        );
        assert_eq!(
            [
                Front::MacMenuBar.keeps_quick_actions(),
                Front::WindowsNotificationArea.keeps_quick_actions(),
                Front::Gnome.keeps_quick_actions(),
            ],
            [false, true, true]
        );
    }

    // GNOME's welcome is a dialog that what it starts comes up over, as the Mac's window is.
    #[test]
    fn a_sheet_comes_up_over_the_welcome_where_the_welcome_hosts_it() {
        assert!(Front::MacMenuBar.welcome_hosts_sheets());
        assert!(Front::Gnome.welcome_hosts_sheets());
        assert!(!Front::WindowsNotificationArea.welcome_hosts_sheets());
    }

    // So no front loads its pages in another's engine, or runs on another's system.
    #[test]
    fn the_engine_is_the_fronts() {
        assert_eq!(
            (Front::MacMenuBar.engine(), Front::MacMenuBar.os()),
            (Engine::WebKit, Os::MacOs)
        );
        assert_eq!(
            (
                Front::WindowsNotificationArea.engine(),
                Front::WindowsNotificationArea.os()
            ),
            (Engine::WebView2, Os::Windows)
        );
        assert_eq!(
            (Front::Gnome.engine(), Front::Gnome.os()),
            (Engine::WebKitGtk, Os::Linux)
        );
    }
}
