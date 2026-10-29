//! Uninstall Pitboard…, how removing Pitboard from this machine goes once it is asked, and what
//! the Linux app says of its packages after.

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
                words::uninstall_says(seen.front),
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

// How a Linux build of the app was packaged, which says how its package manager removes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Package {
    Deb,
    Rpm,
    // built with no package named
    Unknown,
}

// What the Linux app says once Pitboard's own data is gone: an app removes no package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageRemoval {
    pub title: String,
    pub message: String,
    // what a terminal runs to remove the app and its command line, none where not known
    pub command: Option<String>,
}

// The system the app runs on, as its os-release names it: `ID`, and `ID_LIKE`, the ids of the
// systems it is like, closest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OsRelease {
    pub id: Option<String>,
    pub id_like: Option<String>,
}

// `stayed` is what the app could not take back of its own, each as its system said why.
pub fn package_removal(package: Package, system: &OsRelease, stayed: &[String]) -> PackageRemoval {
    let manager = match package {
        Package::Deb => Some("apt"),
        Package::Rpm => rpm_manager(system),
        Package::Unknown => None,
    };
    let then = match manager {
        Some(_) => "Remove the app and its command line with this command in a terminal.",
        None => "Remove the app the way you installed it.",
    };
    let stayed = stayed.iter().map(|failed| words::could_not_remove(failed));
    let message = std::iter::once(words::DATA_DELETED.to_owned())
        .chain(stayed)
        .chain(std::iter::once(then.to_owned()))
        .collect::<Vec<_>>()
        .join(" ");
    PackageRemoval {
        title: words::REMOVE_THE_APP.into(),
        message,
        command: manager.map(|manager| format!("sudo {manager} remove {PACKAGES}")),
    }
}

// One rpm serves Fedora and openSUSE, whose package managers differ.
fn rpm_manager(system: &OsRelease) -> Option<&'static str> {
    [&system.id, &system.id_like]
        .into_iter()
        .flatten()
        .flat_map(|ids| ids.split_whitespace())
        .find_map(|id| match id {
            "fedora" => Some("dnf"),
            "opensuse" | "suse" => Some("zypper"),
            _ => None,
        })
}

// The app's package, then the command line's, which it depends on.
const PACKAGES: &str = "pitboard-app pitboard";

#[cfg(test)]
mod tests {
    use super::*;

    fn fedora() -> OsRelease {
        OsRelease {
            id: Some("fedora".into()),
            id_like: None,
        }
    }

    #[test]
    fn the_package_manager_removes_what_the_app_cannot() {
        let deb = package_removal(Package::Deb, &OsRelease::default(), &[]);
        assert_eq!(deb.title, "Remove the App");
        assert_eq!(
            deb.message,
            "Pitboard’s saved sign-ins and settings are deleted. Remove the app and its command \
             line with this command in a terminal."
        );
        assert_eq!(
            deb.command.as_deref(),
            Some("sudo apt remove pitboard-app pitboard")
        );
        let rpm = package_removal(Package::Rpm, &fedora(), &[]);
        assert_eq!(rpm.message, deb.message);
        assert_eq!(
            rpm.command.as_deref(),
            Some("sudo dnf remove pitboard-app pitboard")
        );
        let unknown = package_removal(Package::Unknown, &fedora(), &[]);
        assert_eq!(
            unknown.message,
            "Pitboard’s saved sign-ins and settings are deleted. Remove the app the way you \
             installed it."
        );
        assert_eq!(unknown.command, None);
    }

    // One rpm serves Fedora and openSUSE, whose os-release names it as Fedora 43's and
    // Tumbleweed 20261020's do. Elsewhere the app names no package manager it cannot know.
    #[test]
    fn the_rpm_names_dnf_on_fedora_zypper_on_opensuse_and_neither_elsewhere() {
        let tumbleweed = OsRelease {
            id: Some("opensuse-tumbleweed".into()),
            id_like: Some("opensuse suse".into()),
        };
        let other = OsRelease {
            id: Some("other".into()),
            id_like: Some("another".into()),
        };
        let on = |system: &OsRelease| package_removal(Package::Rpm, system, &[]);

        assert_eq!(
            on(&fedora()).command.as_deref(),
            Some("sudo dnf remove pitboard-app pitboard")
        );
        assert_eq!(
            on(&tumbleweed).command.as_deref(),
            Some("sudo zypper remove pitboard-app pitboard")
        );
        for system in [other, OsRelease::default()] {
            let elsewhere = on(&system);
            assert_eq!(elsewhere.command, None, "{system:?}");
            assert_eq!(
                elsewhere.message,
                package_removal(Package::Unknown, &system, &[]).message
            );
        }
        assert_eq!(
            package_removal(Package::Deb, &tumbleweed, &[])
                .command
                .as_deref(),
            Some("sudo apt remove pitboard-app pitboard")
        );
    }

    // The app takes back its own files in the person's home before it names the command, and
    // says which it could not, as its system said why.
    #[test]
    fn what_the_app_could_not_take_back_is_said_before_the_command() {
        let stayed = [
            "/home/dana/.cache/com.usepitboard.Pitboard: Permission denied (os error 13)"
                .to_owned(),
        ];

        let deb = package_removal(Package::Deb, &OsRelease::default(), &stayed);

        assert_eq!(
            deb.message,
            "Pitboard’s saved sign-ins and settings are deleted. Pitboard couldn’t remove \
             /home/dana/.cache/com.usepitboard.Pitboard: Permission denied (os error 13). Remove \
             the app and its command line with this command in a terminal."
        );
        assert_eq!(
            deb.command,
            package_removal(Package::Deb, &OsRelease::default(), &[]).command
        );
    }
}
