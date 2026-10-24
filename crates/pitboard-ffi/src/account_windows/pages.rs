//! What a page in an account's window may do, how its dialogs say who asks, what happens
//! when its content stops, and how big a sign-in window opens.

use super::policy::PageRole;

/// Something a page can ask the web view to let it use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum PagePermission {
    Camera,
    Microphone,
}

/// Whether a page in an account's window may use `permission`. The camera and the microphone
/// are never given: Pitboard asks the system for neither, and a voice conversation belongs in
/// the site's own app.
#[uniffi::export]
pub fn page_may_use(permission: PagePermission) -> bool {
    match permission {
        PagePermission::Camera | PagePermission::Microphone => false,
    }
}

/// Whether a page in `role` may close the window it is in. A sign-in window's page closes it
/// once its sign-in is done; an account window's page cannot close the account's window.
#[uniffi::export]
pub fn page_may_close(role: PageRole) -> bool {
    match role {
        PageRole::Popup => true,
        PageRole::Window => false,
    }
}

/// The title of a dialog a page asks for: who asks, so a frame cannot pass its words off as
/// the site's own. The page's own host, `host`, when the main frame asks, and a page embedded
/// in it otherwise, by its host when it has one: a frame of an origin of its own, such as a
/// sandboxed one, has none.
#[uniffi::export]
pub fn dialog_title(host: String, main_frame: bool) -> String {
    match (main_frame, host.is_empty()) {
        (true, false) => format!("{host} says"),
        (false, false) => format!("An embedded page at {host} says"),
        (_, true) => "An embedded page says".into(),
    }
}

/// What becomes of a page whose content process ended.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum ProcessEnded {
    /// Loaded again.
    Reload,
    /// Shown as failed, for `reason`, in place of the page.
    Stopped { reason: String },
}

/// How soon a second end of a page's content process counts as the page not working, in
/// seconds.
const CRASH_WINDOW: f64 = 60.0;

/// What becomes of a page whose content process ended at `now`, having last ended at
/// `last_ended`, each in seconds since the epoch, with their fractions. The first end loads
/// the page again, and a second within a minute of the last says the page stopped working: a
/// heavy page ends it after it has loaded, so a page loading again is no sign it works.
#[uniffi::export]
pub fn after_content_process_ended(last_ended: Option<f64>, now: f64) -> ProcessEnded {
    match last_ended {
        Some(last) if now - last < CRASH_WINDOW => ProcessEnded::Stopped {
            reason: "The page stopped working.".into(),
        },
        _ => ProcessEnded::Reload,
    }
}

/// The size a sign-in window opens at, in points.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct SignInWindowSize {
    pub width: f64,
    pub height: f64,
}

/// The size a sign-in window opens at: what the page asked for, no smaller than 320 by 400,
/// and 500 by 640 for a side it did not name.
#[uniffi::export]
pub fn sign_in_window_size(width: Option<f64>, height: Option<f64>) -> SignInWindowSize {
    SignInWindowSize {
        width: width.unwrap_or(500.0).max(320.0),
        height: height.unwrap_or(640.0).max(400.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page is given neither the camera nor the microphone, whichever it asks for.
    #[test]
    fn a_page_is_never_given_the_camera_or_the_microphone() {
        assert!(!page_may_use(PagePermission::Camera));
        assert!(!page_may_use(PagePermission::Microphone));
    }

    #[test]
    fn only_a_sign_in_windows_page_closes_its_window() {
        assert!(page_may_close(PageRole::Popup));
        assert!(!page_may_close(PageRole::Window));
    }

    /// A dialog says who asks: the page, or a page embedded in it.
    #[test]
    fn a_dialog_says_who_asks() {
        assert_eq!(dialog_title("chatgpt.com".into(), true), "chatgpt.com says");
        assert_eq!(
            dialog_title("artifact.example".into(), false),
            "An embedded page at artifact.example says"
        );
        assert_eq!(dialog_title(String::new(), false), "An embedded page says");
        assert_eq!(dialog_title(String::new(), true), "An embedded page says");
    }

    /// A page that keeps ending its content process says so in place of loading forever,
    /// and one that ends it long after the last time is loaded again. From
    /// AccountWindowsTests.swift, whose page ended at 0, 5 and 66 seconds.
    #[test]
    fn a_page_that_keeps_crashing_says_so() {
        assert_eq!(after_content_process_ended(None, 0.0), ProcessEnded::Reload);
        assert_eq!(
            after_content_process_ended(Some(0.0), 5.0),
            ProcessEnded::Stopped {
                reason: "The page stopped working.".into()
            }
        );
        assert_eq!(
            after_content_process_ended(Some(5.0), 66.0),
            ProcessEnded::Reload
        );
        assert_eq!(
            after_content_process_ended(Some(0.9), 60.5),
            ProcessEnded::Stopped {
                reason: "The page stopped working.".into()
            },
            "measured to the fraction"
        );
        assert_eq!(
            after_content_process_ended(Some(0.0), 60.0),
            ProcessEnded::Reload
        );
    }

    #[test]
    fn a_sign_in_window_is_sized_as_the_page_asks() {
        let size = |width, height| {
            let size = sign_in_window_size(width, height);
            (size.width, size.height)
        };
        assert_eq!(size(None, None), (500.0, 640.0));
        assert_eq!(size(Some(100.0), Some(100.0)), (320.0, 400.0));
        assert_eq!(size(Some(900.0), None), (900.0, 640.0));
    }
}
