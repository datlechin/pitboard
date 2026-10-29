//! Uninstall Pitboard…, and how removing Pitboard from this machine goes once it is asked.

use super::{Choice, ItemOffer, Question, Seen, words};
use crate::model::state::Removing;
use crate::model::{Doing, Intent};

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum RemovalShown {
    // until the core has answered and the account windows' stores have gone
    Removing {
        title: String,
    },
    // Pitboard's folder is gone: the app removes what is its own and quits
    Done,
    // the folder stayed, for the reason `line` says
    Stopped {
        title: String,
        line: String,
        again: Choice,
    },
}

pub(crate) fn uninstall(seen: &Seen) -> ItemOffer {
    ItemOffer {
        title: words::uninstall_pitboard(seen.front),
        intent: Intent::Uninstall,
        enabled: seen.state.may_uninstall(),
        confirm: Some(Question {
            confirm_is_default: false,
            ..Question::asking(
                words::UNINSTALL_QUESTION.into(),
                words::UNINSTALL_SAYS.into(),
                words::UNINSTALL.into(),
            )
        }),
    }
}

pub(crate) fn removal(seen: &Seen) -> Option<RemovalShown> {
    Some(match seen.state.removing.as_ref()? {
        Removing::Ending { .. } | Removing::Running { .. } | Removing::Clearing(_) => {
            RemovalShown::Removing {
                title: words::UNINSTALLING.into(),
            }
        }
        Removing::Done(stayed) => match words::folder_kept(*stayed) {
            None => RemovalShown::Done,
            Some(line) => RemovalShown::Stopped {
                title: words::KEPT_ITS_FOLDER.into(),
                line,
                again: again(seen),
            },
        },
        Removing::Lost => RemovalShown::Stopped {
            title: words::could_not(&Doing::Uninstall),
            line: words::UNINSTALL_NOT_KNOWN.into(),
            again: again(seen),
        },
    })
}

fn again(seen: &Seen) -> Choice {
    Choice {
        title: words::try_again(seen.front),
        intent: Intent::Uninstall,
        enabled: seen.state.may_uninstall(),
    }
}
