//! What setting Pitboard up says: the setup step and its tips, the welcome, a tool Pitboard
//! didn't find, and what stands in for a list with nothing in it yet.

use super::{CONTINUE, add_account, this_machine, titled};
use crate::Front;
use crate::Tool;
use pitboard_core::provider::ProviderId;

// `tool`: the tool named, where the account's is not the only one shown.
pub(crate) fn add_a_second(tool: Option<&str>) -> String {
    match tool {
        Some(tool) => format!("Add a second {tool} account"),
        None => "Add a second account".into(),
    }
}

// `tool`: as `add_a_second` names it. `app`: where its accounts are signed in to inside an app.
pub(crate) fn only_one(label: &str, tool: Option<&str>, app: Option<&str>) -> String {
    let tool = tool.map(|tool| format!("{tool} ")).unwrap_or_default();
    let adding = match app {
        Some(app) => format!("To add another, {app} signs out while you sign in to it."),
        None => "Add another to switch between them.".into(),
    };
    format!("{label} is your only {tool}account. {adding}")
}

pub(crate) fn not_now(front: Front) -> String {
    titled(front, "Not Now", "Not now")
}

pub(crate) const USE_WITH_CODEX: &str = "Use Pitboard with Codex";

pub(crate) const CODEX_UNUSED: &str =
    "Codex is installed. Pitboard reads nothing of it until you add the account it’s signed in to.";

// The Windows tip's title names Codex, and its half-width button has room for one line.
pub(crate) fn add_codex_account(front: Front) -> String {
    match front {
        Front::WindowsNotificationArea => add_account(front, true),
        Front::MacMenuBar | Front::Gnome => "Add Codex Account…".into(),
    }
}

pub(crate) const SWITCH_BEFORE_RUNNING_OUT: &str = "Switch before a limit runs out";

pub(crate) fn at_limit_tip(at: u8) -> String {
    format!(
        "Pitboard notifies you once a limit of the account in use reaches {at}%. It can switch \
         Claude Code to another of your accounts instead."
    )
}

pub(crate) fn open_settings(front: Front) -> String {
    titled(front, "Open Settings…", "Open settings")
}

pub(crate) const WELCOME: &str = "Welcome to Pitboard";

// `tools`: "Claude Code, Codex and Claude Desktop".
pub(crate) fn welcome_says(tools: &str) -> String {
    format!(
        "Pitboard switches {tools} between your own accounts, and shows how much of each limit \
         you’ve used."
    )
}

pub(crate) fn where_it_lives(front: Front) -> &'static str {
    match front {
        Front::WindowsNotificationArea => {
            "Pitboard’s icon is in the notification area. Select ^ to find it."
        }
        Front::MacMenuBar | Front::Gnome => "Pitboard lives in the menu bar.",
    }
}

// The welcome's switch to open at login: its title, and what it says.
pub(crate) fn open_at_login(front: Front) -> (&'static str, &'static str) {
    match front {
        Front::WindowsNotificationArea => (
            "Start when you sign in to Windows",
            "Pitboard watches your limits only while it’s running.",
        ),
        Front::MacMenuBar | Front::Gnome => (
            "Open Pitboard at Login",
            "Pitboard watches your limits only while it’s open.",
        ),
    }
}

// Windows' welcome says what Microsoft's own first-run pages say.
pub(crate) fn welcome_done(front: Front) -> String {
    match front {
        Front::WindowsNotificationArea => "Get started",
        Front::MacMenuBar | Front::Gnome => CONTINUE,
    }
    .into()
}

pub(crate) fn added_as(label: &str) -> String {
    format!("Added as {label}")
}

// Over the welcome's tools on Windows, whose mockup heads them.
pub(crate) const FOUND_ON_THIS_PC: &str = "Found on this PC";

pub(crate) const ADD_ASKING: &str = "Add…";

pub(crate) fn sign_in_asking(front: Front) -> String {
    titled(front, "Sign In…", "Sign in…")
}

pub(crate) const TASKBAR_SETTINGS: &str = "Taskbar settings";

pub(crate) const CLAUDE_CODE_MISSING: &str = "Claude Code isn’t installed";

pub(crate) fn claude_code_not_installed(front: Front) -> String {
    titled(front, "Claude Code Isn’t Installed", CLAUDE_CODE_MISSING)
}

pub(crate) const LEARN_TO_INSTALL: &str = "Learn how to install it";

pub(crate) fn how_to_install_claude_code(front: Front) -> String {
    titled(
        front,
        "How to Install Claude Code",
        "How to install Claude Code",
    )
}

// `tools`: every tool Pitboard switches, listed.
pub(crate) fn no_tool(tools: &str) -> String {
    format!("Pitboard switches {tools} between your accounts. Install one and sign in to it once.")
}

// What Pitboard needs of `tool` to sign in to it, where it did not find that.
pub(crate) fn missing(front: Front, tool: ProviderId) -> crate::present::Missing {
    let here = this_machine(front);
    let (title, detail, link_title, link) = match Tool::from(tool) {
        Tool::Claude => (
            format!("Pitboard can’t find Claude Code on {here}."),
            "Adding a Claude Code account runs Claude Code’s own sign-in. Install it, then come \
             back to this window.",
            how_to_install_claude_code(front),
            crate::present::notices::INSTALL_CLAUDE_CODE,
        ),
        Tool::Codex => (
            format!("Pitboard can’t find the Codex CLI on {here}."),
            "Adding a Codex account runs the Codex CLI’s sign-in, not the copy inside the ChatGPT \
             app. Install the CLI, then come back to this window.",
            titled(
                front,
                "How to Install the Codex CLI",
                "How to install the Codex CLI",
            ),
            INSTALL_CODEX_CLI,
        ),
        Tool::Desktop => (
            format!("Pitboard can’t find Claude Desktop on {here}."),
            "Pitboard finds Claude Desktop by the data it keeps once it has been opened. Open \
             Claude, sign in, then come back to this window.",
            "Download Claude".to_owned(),
            DOWNLOAD_CLAUDE,
        ),
    };
    crate::present::Missing {
        title,
        detail: detail.into(),
        link_title,
        link: link.into(),
    }
}

// OpenAI's page for the Codex CLI, whose install section names each way to install it.
const INSTALL_CODEX_CLI: &str = "https://developers.openai.com/codex/cli";

const DOWNLOAD_CLAUDE: &str = "https://claude.ai/download";

// What stands in for the accounts while nobody is signed in: its title, and why to add one.
pub(crate) fn no_accounts(front: Front) -> (String, &'static str) {
    (
        titled(front, "No Accounts", "No accounts"),
        "Add an account to start. Pitboard keeps each one signed in, so you can switch between \
         them.",
    )
}

pub(crate) const READING_ACCOUNTS: &str = "Reading accounts…";

// The menu's item in place of its accounts.
pub(crate) fn no_accounts_yet(front: Front) -> String {
    titled(front, "No Accounts Yet", "No accounts yet")
}

pub(crate) fn no_accounts_to_show(front: Front) -> String {
    titled(front, "No Accounts to Show", "No accounts to show")
}

// What stands in for the activity list while it is empty: its title, and why.
pub(crate) fn no_activity(front: Front) -> (String, &'static str) {
    (
        titled(front, "No Activity", "No activity"),
        "Pitboard lists every change it makes here.",
    )
}

pub(crate) fn could_not_read_accounts(front: Front) -> String {
    titled(front, "Couldn’t Read Accounts", "Couldn’t read accounts")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Each tool not found says what Pitboard needs of it, on each system in its own name.
    #[test]
    fn a_tool_not_found_says_what_to_install() {
        let codex = missing(Front::WindowsNotificationArea, ProviderId::Codex);
        assert_eq!(codex.title, "Pitboard can’t find the Codex CLI on this PC.");
        assert_eq!(codex.link_title, "How to install the Codex CLI");
        assert_eq!(
            codex.detail,
            "Adding a Codex account runs the Codex CLI’s sign-in, not the copy inside the ChatGPT \
             app. Install the CLI, then come back to this window."
        );
        assert_eq!(codex.link, "https://developers.openai.com/codex/cli");
        let codex = missing(Front::MacMenuBar, ProviderId::Codex);
        assert_eq!(codex.link_title, "How to Install the Codex CLI");
        let claude = missing(Front::MacMenuBar, ProviderId::Claude);
        assert_eq!(claude.title, "Pitboard can’t find Claude Code on this Mac.");
        assert_eq!(
            claude.detail,
            "Adding a Claude Code account runs Claude Code’s own sign-in. Install it, then \
             come back to this window."
        );
        assert_eq!(claude.link_title, "How to Install Claude Code");
        let desktop = missing(Front::MacMenuBar, ProviderId::Desktop);
        assert_eq!(
            desktop.title,
            "Pitboard can’t find Claude Desktop on this Mac."
        );
        assert_eq!(
            desktop.detail,
            "Pitboard finds Claude Desktop by the data it keeps once it has been opened. Open \
             Claude, sign in, then come back to this window."
        );
    }

    // What the window and the menu say until the accounts are set up, pinned once.
    #[test]
    fn setting_up_is_said_in_these_words() {
        let both = |said: &dyn Fn(Front) -> String| {
            (
                said(Front::MacMenuBar),
                said(Front::WindowsNotificationArea),
            )
        };
        let pair = |mac: &str, windows: &str| (mac.to_owned(), windows.to_owned());
        assert_eq!(
            only_one("work", None, None),
            "work is your only account. Add another to switch between them."
        );
        assert_eq!(
            only_one("work", Some("Claude Desktop"), Some("Claude")),
            "work is your only Claude Desktop account. To add another, Claude signs out while \
             you sign in to it."
        );
        assert_eq!(
            both(&|front| add_account(front, true)),
            pair("Add Account…", "Add account…")
        );
        assert_eq!(both(&not_now), pair("Not Now", "Not now"));
        assert_eq!(USE_WITH_CODEX, "Use Pitboard with Codex");
        assert_eq!(
            CODEX_UNUSED,
            "Codex is installed. Pitboard reads nothing of it until you add the account it’s \
             signed in to."
        );
        assert_eq!(
            both(&add_codex_account),
            pair("Add Codex Account…", "Add account…")
        );
        assert_eq!(
            open_at_login(Front::MacMenuBar),
            (
                "Open Pitboard at Login",
                "Pitboard watches your limits only while it’s open."
            )
        );
        assert_eq!(
            open_at_login(Front::WindowsNotificationArea),
            (
                "Start when you sign in to Windows",
                "Pitboard watches your limits only while it’s running."
            )
        );
        assert_eq!(WELCOME, "Welcome to Pitboard");
        assert_eq!(
            welcome_says("Claude Code and Codex"),
            "Pitboard switches Claude Code and Codex between your own accounts, and shows how \
             much of each limit you’ve used."
        );
        assert_eq!(
            (
                where_it_lives(Front::MacMenuBar),
                where_it_lives(Front::WindowsNotificationArea)
            ),
            (
                "Pitboard lives in the menu bar.",
                "Pitboard’s icon is in the notification area. Select ^ to find it."
            )
        );
        assert_eq!(both(&welcome_done), pair("Continue", "Get started"));
        assert_eq!(added_as("work"), "Added as work");
        assert_eq!(FOUND_ON_THIS_PC, "Found on this PC");
        assert_eq!(ADD_ASKING, "Add…");
        assert_eq!(both(&sign_in_asking), pair("Sign In…", "Sign in…"));
        assert_eq!(TASKBAR_SETTINGS, "Taskbar settings");
        assert_eq!(SWITCH_BEFORE_RUNNING_OUT, "Switch before a limit runs out");
        assert_eq!(
            at_limit_tip(95),
            "Pitboard notifies you once a limit of the account in use reaches 95%. It can switch \
             Claude Code to another of your accounts instead."
        );
        assert_eq!(
            both(&open_settings),
            pair("Open Settings…", "Open settings")
        );
        assert_eq!(
            both(&claude_code_not_installed),
            pair("Claude Code Isn’t Installed", "Claude Code isn’t installed")
        );
        assert_eq!(CLAUDE_CODE_MISSING, "Claude Code isn’t installed");
        assert_eq!(LEARN_TO_INSTALL, "Learn how to install it");
        assert_eq!(
            no_tool("Claude Code and Codex"),
            "Pitboard switches Claude Code and Codex between your accounts. Install one and sign \
             in to it once."
        );
        assert_eq!(
            both(&how_to_install_claude_code),
            pair("How to Install Claude Code", "How to install Claude Code")
        );
        assert_eq!(
            no_accounts(Front::MacMenuBar),
            (
                "No Accounts".into(),
                "Add an account to start. Pitboard keeps each one signed in, so you can switch \
                 between them."
            )
        );
        assert_eq!(no_accounts(Front::WindowsNotificationArea).0, "No accounts");
        assert_eq!(READING_ACCOUNTS, "Reading accounts…");
        assert_eq!(
            both(&no_accounts_yet),
            pair("No Accounts Yet", "No accounts yet")
        );
        assert_eq!(
            both(&no_accounts_to_show),
            pair("No Accounts to Show", "No accounts to show")
        );
        assert_eq!(
            no_activity(Front::MacMenuBar),
            (
                "No Activity".into(),
                "Pitboard lists every change it makes here."
            )
        );
        assert_eq!(no_activity(Front::WindowsNotificationArea).0, "No activity");
    }
}
