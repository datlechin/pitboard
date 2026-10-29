//! What the welcome window shows at the first launch.

use super::{Choice, Seen, SetupLink, words};
use crate::Front;
use crate::model::{Intent, Place, Sheet};
use pitboard_core::provider::ProviderId;

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WelcomeShown {
    pub title: String,
    pub lines: Vec<String>,
    // beside the line that says where the icon is, on Windows, where it can be kept in view
    pub icon_link: Option<SetupLink>,
    // over the tools, on Windows
    pub tools_heading: Option<String>,
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
}

pub(crate) fn welcome(seen: &Seen) -> WelcomeShown {
    let names: Vec<&str> = seen.tools.iter().map(|tool| tool.name()).collect();
    let (title, note) = words::open_at_login(seen.front);
    WelcomeShown {
        title: words::WELCOME.into(),
        lines: vec![
            words::welcome_says(&pitboard_core::words::listed(&names)),
            words::where_it_lives(seen.front, seen.state.surfaces.status_item),
        ],
        icon_link: match seen.front {
            Front::WindowsNotificationArea => Some(SetupLink {
                title: words::TASKBAR_SETTINGS.into(),
                link: "ms-settings:taskbar".into(),
            }),
            Front::MacMenuBar | Front::Gnome => None,
        },
        tools_heading: match seen.front {
            Front::WindowsNotificationArea => Some(words::FOUND_ON_THIS_PC.into()),
            Front::MacMenuBar | Front::Gnome => None,
        },
        tools: tools(seen),
        open_at_login: WelcomeLogin {
            title: title.into(),
            on: seen.state.opens_at_login == Some(true),
            note: note.into(),
        },
        done: Choice {
            title: words::welcome_done(seen.front),
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
    let claude = ProviderId::Claude;
    if found.is_empty() {
        let missing = words::missing(seen.front, claude);
        return vec![WelcomeTool {
            code: claude.code().into(),
            name: claude.name().into(),
            account: None,
            step: WelcomeStep::Install {
                link: SetupLink {
                    title: missing.link_title,
                    link: missing.link,
                },
            },
        }];
    }
    (seen.tools.iter().copied())
        .filter(|tool| found.contains(tool))
        .map(|tool| {
            let added = (seen.accounts().iter())
                .filter(|account| account.provider == tool && account.label.is_some())
                .min_by_key(|account| !account.signed_in);
            let signed_in = seen.unnamed().find(|account| account.provider == tool);
            let (account, step) = match (added, signed_in) {
                (Some(added), _) => (
                    Some(added.email.clone()).filter(|email| !email.is_empty()),
                    WelcomeStep::Added {
                        said: words::added_as(added.label.as_deref().unwrap_or_default()),
                    },
                ),
                (None, Some(login)) if tool == claude => (
                    Some(login.email.clone()),
                    add(words::ADD_ASKING.into(), tool),
                ),
                (None, _) if tool == claude => (
                    None,
                    add_from_sheet(words::sign_in_asking(seen.front), tool),
                ),
                (None, _) if tool == ProviderId::Codex => {
                    (None, add(words::ADD_ASKING.into(), tool))
                }
                (None, _) => (None, add_from_sheet(words::ADD_ASKING.into(), tool)),
            };
            WelcomeTool {
                code: tool.code().into(),
                name: tool.name().into(),
                account,
                step,
            }
        })
        .collect()
}

// The login the tool has now, read on the click, or its sign-in where it has none.
fn add(title: String, tool: ProviderId) -> WelcomeStep {
    WelcomeStep::Add {
        choice: Choice {
            title,
            intent: Intent::Enrol { tool: tool.into() },
            enabled: true,
        },
    }
}

fn add_from_sheet(title: String, tool: ProviderId) -> WelcomeStep {
    WelcomeStep::Add {
        choice: Choice {
            title,
            intent: Intent::PresentSheet {
                sheet: Sheet::Add {
                    tool: Some(tool.into()),
                },
            },
            enabled: true,
        },
    }
}
