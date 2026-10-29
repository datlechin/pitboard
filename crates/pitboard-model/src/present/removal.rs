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
    pub commands: Vec<PackageCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageCommand {
    // the systems it is for, where one package serves systems whose package managers differ
    pub on: Option<String>,
    pub run: String,
}

// One rpm serves Fedora and openSUSE, so it names both package managers. `stayed` is what the
// app could not take back of its own, each as its system said why.
pub fn package_removal(package: Package, stayed: &[String]) -> PackageRemoval {
    let command = |on: Option<&str>, manager: &str| PackageCommand {
        on: on.map(str::to_owned),
        run: format!("sudo {manager} remove {PACKAGES}"),
    };
    let (then, commands) = match package {
        Package::Deb => (
            "Remove the app and its command line with this command in a terminal.",
            vec![command(None, "apt")],
        ),
        Package::Rpm => (
            "Remove the app and its command line with your system’s command in a terminal.",
            vec![
                command(Some("Fedora"), "dnf"),
                command(Some("openSUSE"), "zypper"),
            ],
        ),
        Package::Unknown => ("Remove the app the way you installed it.", Vec::new()),
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
        commands,
    }
}

// The app's package, then the command line's, which it depends on.
const PACKAGES: &str = "pitboard-app pitboard";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_package_manager_removes_what_the_app_cannot() {
        let deb = package_removal(Package::Deb, &[]);
        assert_eq!(deb.title, "Remove the App");
        assert_eq!(
            deb.message,
            "Pitboard’s saved sign-ins and settings are deleted. Remove the app and its command \
             line with this command in a terminal."
        );
        assert_eq!(
            deb.commands,
            [PackageCommand {
                on: None,
                run: "sudo apt remove pitboard-app pitboard".into(),
            }]
        );
        let rpm = package_removal(Package::Rpm, &[]);
        assert_eq!(
            rpm.commands,
            [
                PackageCommand {
                    on: Some("Fedora".into()),
                    run: "sudo dnf remove pitboard-app pitboard".into(),
                },
                PackageCommand {
                    on: Some("openSUSE".into()),
                    run: "sudo zypper remove pitboard-app pitboard".into(),
                },
            ]
        );
        let unknown = package_removal(Package::Unknown, &[]);
        assert_eq!(
            unknown.message,
            "Pitboard’s saved sign-ins and settings are deleted. Remove the app the way you \
             installed it."
        );
        assert_eq!(unknown.commands, []);
    }

    // The app takes back its own files in the person's home before it names the command, and
    // says which it could not, as its system said why.
    #[test]
    fn what_the_app_could_not_take_back_is_said_before_the_command() {
        let stayed = [
            "/home/dana/.cache/com.usepitboard.Pitboard: Permission denied (os error 13)"
                .to_owned(),
        ];

        let deb = package_removal(Package::Deb, &stayed);

        assert_eq!(
            deb.message,
            "Pitboard’s saved sign-ins and settings are deleted. Pitboard couldn’t remove \
             /home/dana/.cache/com.usepitboard.Pitboard: Permission denied (os error 13). Remove \
             the app and its command line with this command in a terminal."
        );
        assert_eq!(deb.commands, package_removal(Package::Deb, &[]).commands);
    }
}
