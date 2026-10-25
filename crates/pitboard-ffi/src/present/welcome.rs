//! What the welcome window shows at the first launch.

use super::{Choice, Seen, SetupLink, words};
use crate::model::{Intent, Place, Sheet};
use pitboard_core::host::Os;
use pitboard_core::provider::ProviderId;

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WelcomeShown {
    pub title: String,
    pub lines: Vec<String>,
    // each tool found, in the order the apps list tools; Claude Code's install link where none is
    pub tools: Vec<WelcomeTool>,
    pub open_at_login: WelcomeLogin,
    pub done: Choice,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WelcomeTool {
    pub code: String,
    pub name: String,
    // Codex's and Claude Desktop's login is read only once Add is chosen
    pub account: Option<String>,
    pub step: WelcomeStep,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum WelcomeStep {
    Added { said: String },
    Add { choice: Choice },
    Install { link: SetupLink },
}

// The app turns it on itself, and says what its system has with `Intent::OpensAtLogin`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WelcomeLogin {
    pub title: String,
    // off until the app's system says it is on
    pub on: bool,
    pub note: String,
    // on Windows, where the icon can be kept in view
    pub link: Option<SetupLink>,
}

pub(crate) fn welcome(seen: &Seen) -> WelcomeShown {
    let names: Vec<&str> = seen.tools.iter().map(|tool| tool.name.as_str()).collect();
    let (title, note) = words::open_at_login(seen.os);
    WelcomeShown {
        title: words::WELCOME.into(),
        lines: vec![
            words::welcome_says(&pitboard_core::words::listed(&names)),
            words::where_it_lives(seen.os).into(),
        ],
        tools: tools(seen),
        open_at_login: WelcomeLogin {
            title: title.into(),
            on: seen.state.opens_at_login == Some(true),
            note: note.into(),
            link: (seen.os == Os::Windows).then(|| SetupLink {
                title: words::TASKBAR_SETTINGS.into(),
                link: "ms-settings:taskbar".into(),
            }),
        },
        done: Choice {
            title: words::welcome_done(seen.os),
            intent: Intent::Show {
                place: Some(Place::Accounts),
            },
            enabled: true,
        },
    }
}

fn tools(seen: &Seen) -> Vec<WelcomeTool> {
    let Some(found) = &seen.state.installed else {
        return Vec::new();
    };
    let claude = ProviderId::Claude.code();
    if found.is_empty() {
        return words::missing(seen.os, claude)
            .map(|missing| WelcomeTool {
                code: claude.into(),
                name: seen.tool_name(claude),
                account: None,
                step: WelcomeStep::Install {
                    link: SetupLink {
                        title: missing.link_title,
                        link: missing.link,
                    },
                },
            })
            .into_iter()
            .collect();
    }
    (seen.tools.iter())
        .filter(|tool| found.iter().any(|here| here.code == tool.code))
        .map(|tool| {
            let code = tool.code.as_str();
            let added = (seen.accounts().iter())
                .filter(|account| account.provider == code && account.label.is_some())
                .min_by_key(|account| !account.signed_in);
            let signed_in = seen.unnamed().find(|account| account.provider == code);
            let (account, step) = match (added, signed_in) {
                (Some(added), _) => (
                    Some(added.email.clone()).filter(|email| !email.is_empty()),
                    WelcomeStep::Added {
                        said: words::ADDED.into(),
                    },
                ),
                (None, Some(login)) if code == claude => {
                    (Some(login.email.clone()), add(words::ADD.into(), code))
                }
                (None, _) if code == claude => {
                    (None, add_from_sheet(words::sign_in_asking(seen.os), code))
                }
                (None, _) if code == ProviderId::Codex.code() => {
                    (None, add(words::ADD.into(), code))
                }
                (None, _) => (None, add_from_sheet(words::ADD_ASKING.into(), code)),
            };
            WelcomeTool {
                code: code.into(),
                name: tool.name.clone(),
                account,
                step,
            }
        })
        .collect()
}

// The login the tool has now, read on the click, or its sign-in where it has none.
fn add(title: String, code: &str) -> WelcomeStep {
    WelcomeStep::Add {
        choice: Choice {
            title,
            intent: Intent::Enrol {
                provider: code.into(),
            },
            enabled: true,
        },
    }
}

fn add_from_sheet(title: String, code: &str) -> WelcomeStep {
    WelcomeStep::Add {
        choice: Choice {
            title,
            intent: Intent::PresentSheet {
                sheet: Sheet::Add {
                    provider: Some(code.into()),
                },
            },
            enabled: true,
        },
    }
}
