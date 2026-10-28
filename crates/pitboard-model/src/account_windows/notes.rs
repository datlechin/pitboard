//! What a window says: in the bar above its page, and when it asks before removing what it
//! keeps.

use super::accounts::WindowAccount;
use super::this_machine;
use crate::present::{Question, words};
use crate::{Engine, Front, Site};

// Said in the bar above a window's page until dismissed or replaced, or for `StoreHeld` by
// its menu item.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum WindowNoteKind {
    /// The window opened with a store it never had before: how to sign in to its site.
    SignIn,
    /// The policy stopped a page of Google's: its sign-in, its single sign-on, or the
    /// connection of one of its apps.
    GoogleRefused,
    /// The site sent a page to the browser without a click.
    OpenedInBrowser,
    /// A page asked for a link to another app, which a window never opens.
    OtherApp {
        scheme: String,
    },
    // The window's account's old store is still being deleted, which only Windows waits for.
    StoreHeld,
}

#[uniffi::export]
pub fn window_note(front: Front, kind: WindowNoteKind, account: WindowAccount) -> String {
    let site = &account.site;
    match kind {
        WindowNoteKind::SignIn => format!(
            "Sign in to {} as {}. Google’s sign-in doesn’t work inside apps. {}",
            site.name,
            account.email,
            sign_in_steps(front.engine(), site)
        ),
        WindowNoteKind::GoogleRefused => format!(
            "Google doesn’t allow its pages inside apps, so Pitboard stopped it. Sign in as {} \
             another way. {} {}",
            account.email,
            sign_in_steps(front.engine(), site),
            site.blocked_services
        ),
        WindowNoteKind::OpenedInBrowser => format!(
            "{name} opened a page in your browser. Anything you connect there goes to the {name} \
             account your browser is signed in to, which may not be “{label}”.",
            name = site.name,
            label = account.label
        ),
        WindowNoteKind::OtherApp { scheme } => {
            format!("This window doesn’t open links to other apps, such as this {scheme}: link.")
        }
        // WebView2 deletes the folder as the next browser process starts
        // (docs/facts/windows-runners.md).
        WindowNoteKind::StoreHeld => "What this window kept before is deleted once every account \
                                      window has closed. It opens after that."
            .into(),
    }
}

// The site's own steps, and what of them a page in `engine` cannot do.
fn sign_in_steps(engine: Engine, site: &Site) -> String {
    let passkeys = match engine {
        // WebKit gives a page a passkey only in an app macOS lets act as a browser, and
        // Pitboard is not one.
        Engine::WebKit => false,
        // Whether WebView2 gives a page in an app's window a passkey is the Windows app's to
        // measure, and until it has, none is promised.
        Engine::WebView2 => false,
        // No app runs on Linux, and nothing was measured there, so none is promised.
        Engine::WebKitGtk => false,
    };
    let offers = pitboard_sites::Site::serving(&site.host).is_some_and(|site| site.passkeys);
    if offers && !passkeys {
        format!("{} Passkeys don’t work in this window.", site.sign_in_steps)
    } else {
        site.sign_in_steps.clone()
    }
}

/// What a window says as it opens: how to sign in, when this Pitboard directory had not made
/// its store before, since a store never made holds no sign-in. A window whose data was just
/// removed opens as one whose store was never made.
#[uniffi::export]
pub fn opening_note(store_made_before: bool) -> Option<WindowNoteKind> {
    (!store_made_before).then_some(WindowNoteKind::SignIn)
}

// The site is not told, so the account stays signed in everywhere else.
#[uniffi::export]
pub fn remove_data_alert(front: Front, account: WindowAccount) -> Question {
    let name = &account.site.name;
    Question::asking(
        format!("Remove {name} data for “{}”?", account.label),
        format!(
            "Pitboard removes the cookies and everything else {name} keeps in this window on \
             {}, which signs this window out. {name} isn’t told: the account stays signed in \
             on your other devices and browsers.",
            this_machine(front)
        ),
        words::REMOVE.into(),
    )
}

#[cfg(test)]
mod tests {
    use super::super::accounts::{tests::account, window_accounts};
    use super::*;
    use pitboard_core::provider::ProviderId;

    fn window(label: &str, provider: ProviderId) -> WindowAccount {
        let mut found = window_accounts(&[account(Some(label), provider, "dana")]);
        found.pop().expect("a window")
    }

    #[test]
    fn a_note_says_what_happened_to_whom() {
        let main = window("main", ProviderId::Codex);
        let steps = "Enter your email address, then its password or the code chatgpt.com emails \
                     you, or click Continue with Microsoft or Continue with Apple. Passkeys don’t \
                     work in this window.";
        for front in [Front::MacMenuBar, Front::WindowsNotificationArea] {
            assert_eq!(
                window_note(front, WindowNoteKind::SignIn, main.clone()),
                format!(
                    "Sign in to chatgpt.com as main@example.com. Google’s sign-in doesn’t work \
                     inside apps. {steps}"
                )
            );
            assert_eq!(
                window_note(front, WindowNoteKind::GoogleRefused, main.clone()),
                format!(
                    "Google doesn’t allow its pages inside apps, so Pitboard stopped it. Sign in \
                     as main@example.com another way. {steps} Google Drive, Gmail and Google \
                     Calendar can’t be connected in this window."
                )
            );
            assert_eq!(
                window_note(front, WindowNoteKind::OpenedInBrowser, main.clone()),
                "chatgpt.com opened a page in your browser. Anything you connect there goes to \
                 the chatgpt.com account your browser is signed in to, which may not be “main”."
            );
            assert_eq!(
                window_note(
                    front,
                    WindowNoteKind::OtherApp {
                        scheme: "vscode".into()
                    },
                    main.clone()
                ),
                "This window doesn’t open links to other apps, such as this vscode: link."
            );
            assert_eq!(
                window_note(
                    front,
                    WindowNoteKind::SignIn,
                    window("work", ProviderId::Claude)
                ),
                "Sign in to claude.ai as work@example.com. Google’s sign-in doesn’t work inside \
                 apps. Click Continue with email, open the email on your phone, tap its link, \
                 then enter here the code claude.ai shows there."
            );
        }
    }

    // Whether a passkey works is the engine's to say, so pitboard-sites' steps leave it out.
    #[test]
    fn a_window_says_a_passkey_does_not_work_where_it_does_not() {
        let chatgpt = Site::from(&pitboard_sites::CHATGPT);
        let claude = Site::from(&pitboard_sites::CLAUDE);
        for engine in [Engine::WebKit, Engine::WebView2, Engine::WebKitGtk] {
            assert_eq!(
                sign_in_steps(engine, &chatgpt),
                "Enter your email address, then its password or the code chatgpt.com emails \
                 you, or click Continue with Microsoft or Continue with Apple. Passkeys don’t \
                 work in this window."
            );
            assert_eq!(
                sign_in_steps(engine, &claude),
                "Click Continue with email, open the email on your phone, tap its link, then \
                 enter here the code claude.ai shows there.",
                "claude.ai's steps say nothing of a passkey"
            );
        }
    }

    #[test]
    fn a_window_whose_old_store_is_held_says_when_it_opens() {
        assert_eq!(
            window_note(
                Front::WindowsNotificationArea,
                WindowNoteKind::StoreHeld,
                window("work", ProviderId::Claude)
            ),
            "What this window kept before is deleted once every account window has closed. It \
             opens after that."
        );
    }

    /// A window whose store was never made says how to sign in; one that was says nothing.
    #[test]
    fn only_a_windows_first_opening_says_how_to_sign_in() {
        assert_eq!(opening_note(false), Some(WindowNoteKind::SignIn));
        assert_eq!(opening_note(true), None);
    }

    #[test]
    fn removing_a_windows_data_says_the_site_is_not_told() {
        let work = window("work", ProviderId::Claude);
        let mac = remove_data_alert(Front::MacMenuBar, work.clone());
        assert_eq!(mac.title, "Remove claude.ai data for “work”?");
        assert_eq!(
            (
                mac.confirm.as_str(),
                mac.destructive,
                mac.confirm_is_default
            ),
            ("Remove", false, true)
        );
        assert_eq!(
            mac.message,
            "Pitboard removes the cookies and everything else claude.ai keeps in this window on \
             this Mac, which signs this window out. claude.ai isn’t told: the account stays \
             signed in on your other devices and browsers."
        );
        let linux = remove_data_alert(Front::Gnome, work.clone());
        assert!(linux.message.contains(" on this computer, "));
        let windows = remove_data_alert(Front::WindowsNotificationArea, work);
        assert_eq!(windows.title, mac.title);
        assert_eq!(
            windows.message,
            "Pitboard removes the cookies and everything else claude.ai keeps in this window on \
             this PC, which signs this window out. claude.ai isn’t told: the account stays \
             signed in on your other devices and browsers."
        );
    }
}
