//! Which app the model serves: the surface it lives in, and what follows from that alone.
//!
//! An app names its front once, in `AppLaunch`, and the model words and decides by it what
//! differs from one app to another: a control's case, what the machine is called, where a
//! sheet comes up, and its account windows' engine. Each is a `match` on `Front` with no `_`
//! arm, so a front added does not compile until each is said for it. What the core decides
//! about the system the app runs on, such as which tools it supports or how its file system
//! compares names, is asked of `os()`.

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

    // Whether a sheet, a question or a failure comes up over the welcome while it is shown,
    // whose Add starts them, so its Continue is still in front.
    pub(crate) fn welcome_hosts_sheets(self) -> bool {
        match self {
            Front::MacMenuBar => true,
            // The main window hosts them.
            Front::WindowsNotificationArea => false,
            // No app runs on Linux, which answers as the Mac does.
            Front::Gnome => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
