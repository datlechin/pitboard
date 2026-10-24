//! What a window says: in the bar above its page, and when it asks before removing what it
//! keeps.

use super::accounts::WindowAccount;
use super::{AlertText, this_machine};
use pitboard_core::host::{OS, Os};

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
pub fn window_note(kind: WindowNoteKind, account: WindowAccount) -> String {
    let site = &account.site;
    match kind {
        WindowNoteKind::SignIn => format!(
            "Sign in to {} as {}. Google’s sign-in doesn’t work inside apps. {}",
            site.name, account.email, site.sign_in_steps
        ),
        WindowNoteKind::GoogleRefused => format!(
            "Google doesn’t allow its pages inside apps, so Pitboard stopped it. Sign in as {} \
             another way. {} {}",
            account.email, site.sign_in_steps, site.blocked_services
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

/// What a window says as it opens: how to sign in, when this Pitboard directory had not made
/// its store before, since a store never made holds no sign-in. A window whose data was just
/// removed opens as one whose store was never made.
#[uniffi::export]
pub fn opening_note(store_made_before: bool) -> Option<WindowNoteKind> {
    (!store_made_before).then_some(WindowNoteKind::SignIn)
}

/// What the alert asking to remove what `account`'s window keeps says. The site is not told,
/// so the account stays signed in everywhere else.
#[uniffi::export]
pub fn remove_data_alert(account: WindowAccount) -> AlertText {
    remove_data_alert_on(OS, &account)
}

fn remove_data_alert_on(os: Os, account: &WindowAccount) -> AlertText {
    let name = &account.site.name;
    AlertText {
        title: format!("Remove {name} data for “{}”?", account.label),
        message: format!(
            "Pitboard removes the cookies and everything else {name} keeps in this window on \
             {}, which signs this window out. {name} isn’t told: the account stays signed in \
             on your other devices and browsers.",
            this_machine(os)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::super::accounts::{tests::account, window_accounts};
    use super::*;

    fn window(label: &str, provider: &str) -> WindowAccount {
        let mut found = window_accounts(&[account(Some(label), provider, "dana")]);
        found.pop().expect("a window")
    }

    #[test]
    fn a_note_says_what_happened_to_whom() {
        let main = window("main", "codex");
        let steps = &main.site.sign_in_steps;
        assert_eq!(
            window_note(WindowNoteKind::SignIn, main.clone()),
            format!(
                "Sign in to chatgpt.com as main@example.com. Google’s sign-in doesn’t work \
                 inside apps. {steps}"
            )
        );
        assert!(steps.starts_with("Enter your email address"));
        assert_eq!(
            window_note(WindowNoteKind::GoogleRefused, main.clone()),
            format!(
                "Google doesn’t allow its pages inside apps, so Pitboard stopped it. Sign in \
                 as main@example.com another way. {steps} Google Drive, Gmail and Google \
                 Calendar can’t be connected in this window."
            )
        );
        assert_eq!(
            window_note(WindowNoteKind::OpenedInBrowser, main.clone()),
            "chatgpt.com opened a page in your browser. Anything you connect there goes to the \
             chatgpt.com account your browser is signed in to, which may not be “main”."
        );
        assert_eq!(
            window_note(
                WindowNoteKind::OtherApp {
                    scheme: "vscode".into()
                },
                main
            ),
            "This window doesn’t open links to other apps, such as this vscode: link."
        );
        let work = window("work", "claude");
        assert_eq!(
            window_note(WindowNoteKind::SignIn, work),
            "Sign in to claude.ai as work@example.com. Google’s sign-in doesn’t work inside \
             apps. Click Continue with email, open the email on your phone, tap its link, then \
             enter here the code claude.ai shows there."
        );
    }

    #[test]
    fn a_window_whose_old_store_is_held_says_when_it_opens() {
        assert_eq!(
            window_note(WindowNoteKind::StoreHeld, window("work", "claude")),
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
        let work = window("work", "claude");
        let mac = remove_data_alert_on(Os::MacOs, &work);
        assert_eq!(mac.title, "Remove claude.ai data for “work”?");
        assert_eq!(
            mac.message,
            "Pitboard removes the cookies and everything else claude.ai keeps in this window on \
             this Mac, which signs this window out. claude.ai isn’t told: the account stays \
             signed in on your other devices and browsers."
        );
        let linux = remove_data_alert_on(Os::Linux, &work);
        assert!(linux.message.contains(" on this computer, "));
        let windows = remove_data_alert_on(Os::Windows, &work);
        assert_eq!(windows.title, mac.title);
        assert_eq!(
            windows.message,
            "Pitboard removes the cookies and everything else claude.ai keeps in this window on \
             this PC, which signs this window out. claude.ai isn’t told: the account stays \
             signed in on your other devices and browsers."
        );
        assert_eq!(
            remove_data_alert(work.clone()),
            remove_data_alert_on(OS, &work)
        );
    }
}
