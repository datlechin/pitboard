//! `pitboard vault`: where parked logins are kept, and on Linux a move of them between the login
//! keyring and files. Which ways a system keeps them, and how a move goes, are the core's
//! (`pitboard_core::switch::Keeping`, `pitboard_core::switch::Moved`).

use crate::{Report, changed};
use clap::{Subcommand, ValueEnum};
use pitboard_core::host::{Keyring, Os, Vault};
use pitboard_core::service::Pitboard;
use pitboard_core::state::VaultForm;
use pitboard_core::switch::{Keeping, Moved};
use pitboard_core::words;
use serde_json::{Value, json};

// Only Linux keeps parked logins more than one way.
const MOVE: &str = match pitboard_core::host::OS {
    Os::Linux => "Move every parked login into the login keyring or into 0600 files",
    Os::MacOs => {
        "Move every parked login to another way of keeping them, which macOS does not offer"
    }
    Os::Windows => {
        "Move every parked login to another way of keeping them, which Windows does not offer"
    }
};

const FORM: &str = match pitboard_core::host::OS {
    Os::Linux => {
        "Where to keep them: `secret-service`, the login keyring, or `files`, 0600 files in \
         Pitboard's directory"
    }
    Os::MacOs | Os::Windows => "Where to keep them",
};

#[derive(Subcommand)]
pub(crate) enum VaultCommand {
    /// Say where parked logins are kept (the default)
    Status,
    #[command(about = MOVE)]
    Move {
        #[arg(value_enum, value_name = "FORM", help = FORM)]
        to: Form,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum Form {
    SecretService,
    Files,
}

impl From<Form> for VaultForm {
    fn from(form: Form) -> VaultForm {
        match form {
            Form::SecretService => VaultForm::SecretService,
            Form::Files => VaultForm::Files,
        }
    }
}

pub(crate) fn run(pitboard: &Pitboard, what: Option<&VaultCommand>) -> Report {
    match what {
        None | Some(VaultCommand::Status) => status(pitboard),
        Some(VaultCommand::Move { to }) => moved(pitboard, (*to).into()),
    }
}

fn status(pitboard: &Pitboard) -> Report {
    match pitboard.vault() {
        Ok(kept) => {
            let folder = folder(Some(&kept), pitboard);
            Report::done("vault", kept_json(&kept), said(&kept, &folder))
        }
        Err(error) => Report::failed(Some("vault"), error),
    }
}

fn moved(pitboard: &Pitboard, to: VaultForm) -> Report {
    let outcome = pitboard.vault_move(to);
    let kept = pitboard.vault().ok();
    let folder = folder(kept.as_ref(), pitboard);
    let keyring = keyring(kept.as_ref());
    changed("vault", outcome, |moved| {
        (
            json!({
                "kept": moved.to.code(),
                "changed": moved.changed,
                "moved": moved.moved,
                "leaving": moved.leaving,
            }),
            format!("{}\n", moved_line(&moved, &folder, keyring)),
        )
    })
}

fn moved_line(moved: &Moved, folder: &str, keyring: Keyring) -> String {
    let into = form_words(moved.to, folder, keyring);
    match (moved.changed, moved.moved) {
        (false, _) => format!("Parked logins are already kept in {into}."),
        (true, 0) => format!("Parked logins are kept in {into} from now on."),
        (true, count) => format!("Moved {count} parked login(s) into {into}."),
    }
}

// The folder only where they are files.
fn kept_json(kept: &Keeping) -> Value {
    let files = matches!(kept.place, Some(Vault::Files | Vault::SealedFiles));
    json!({
        "kept": kept.place.map(Vault::code),
        "shared": kept.shared,
        "path": kept.folder.as_ref().filter(|_| files),
        "secret_service": kept.secret_service,
        "moving_to": kept.moving_to.map(VaultForm::code),
        "leaving": kept.leaving.map(|(from, parks)| json!({ "from": from.code(), "parks": parks })),
    })
}

fn folder(kept: Option<&Keeping>, pitboard: &Pitboard) -> String {
    kept.and_then(|kept| kept.folder.as_deref())
        .map(|folder| pitboard.shown(folder))
        .unwrap_or_default()
}

// The login keyring where nothing says what answers for the Secret Service.
fn keyring(kept: Option<&Keeping>) -> Keyring {
    kept.and_then(|kept| kept.keyring).unwrap_or(Keyring::Login)
}

fn form_words(form: VaultForm, folder: &str, keyring: Keyring) -> String {
    match form {
        VaultForm::SecretService => words::keyring(keyring).to_string(),
        VaultForm::Files => format!("0600 files in {folder}"),
    }
}

// `folder` as a person reads it.
fn said(kept: &Keeping, folder: &str) -> String {
    let named = words::keyring(keyring(Some(kept)));
    let mut lines = vec![match kept.place {
        Some(Vault::Keychain) => "Parked logins are kept in the login keychain, which every \
                                  Pitboard directory on this Mac shares."
            .to_string(),
        Some(Vault::SecretService) => format!(
            "Parked logins are kept in {named}, which every Pitboard directory of your session \
             shares."
        ),
        Some(Vault::Files) => {
            format!("Parked logins are kept in 0600 files in {folder}, which only you can read.")
        }
        Some(Vault::SealedFiles) => {
            format!("Parked logins are kept in files in {folder}, sealed to your Windows account.")
        }
        None => "Pitboard cannot tell where this directory keeps its parked logins.".to_string(),
    }];
    if let Some(to) = kept.moving_to {
        lines.push(format!(
            "A move into {} is under way: the next `pitboard status`, change or daily renewal \
             finishes it.",
            form_words(to, folder, keyring(Some(kept)))
        ));
    }
    if let Some((from, parks)) = kept.leaving {
        lines.push(format!(
            "{parks} parked login(s) wait to be deleted from {}: the next change deletes them.",
            form_words(from, folder, keyring(Some(kept)))
        ));
    }
    // A move is offered only once none is under way or left anything to delete.
    let settled = kept.moving_to.is_none() && kept.leaving.is_none();
    match (kept.place, kept.secret_service) {
        (Some(Vault::SecretService), Some(true)) if settled => lines.push(format!(
            "`pitboard vault move files` keeps them in 0600 files in {folder} instead."
        )),
        (Some(Vault::SecretService), Some(false)) => lines.push(format!(
            "No Secret Service answers on this session's bus, so they cannot be read from here. \
             Run Pitboard in your desktop session, where signing in unlocks {named}."
        )),
        (Some(Vault::Files), Some(true)) if settled => lines.push(format!(
            "A Secret Service answers on this session's bus: `pitboard vault move secret-service` \
             keeps them in {named} instead."
        )),
        (Some(Vault::Files), Some(false)) => lines.push(
            "No Secret Service answers on this session's bus, as over SSH or in a container."
                .into(),
        ),
        _ => {}
    }
    lines.into_iter().map(|line| format!("{line}\n")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOLDER: &str = "~/.pitboard/vault";

    fn files() -> Keeping {
        Keeping {
            place: Some(Vault::Files),
            shared: false,
            folder: Some("/home/me/.pitboard/vault".into()),
            secret_service: Some(true),
            keyring: Some(Keyring::Login),
            moving_to: None,
            leaving: None,
        }
    }

    // A move cut short, and the copies one left where they were, until the next change.
    #[test]
    fn vault_says_a_move_under_way_and_what_waits_to_be_deleted() {
        let moving = Keeping {
            moving_to: Some(VaultForm::SecretService),
            ..files()
        };
        assert_eq!(
            said(&moving, FOLDER),
            "Parked logins are kept in 0600 files in ~/.pitboard/vault, which only you can read.\n\
             A move into the login keyring is under way: the next `pitboard status`, change or \
             daily renewal finishes it.\n"
        );
        let left = Keeping {
            leaving: Some((VaultForm::SecretService, 2)),
            ..files()
        };
        assert!(
            said(&left, FOLDER).contains(
                "\n2 parked login(s) wait to be deleted from the login keyring: the next change \
                 deletes them.\n"
            ),
            "{}",
            said(&left, FOLDER)
        );
        let keyring = Keeping {
            place: Some(Vault::SecretService),
            shared: true,
            leaving: Some((VaultForm::Files, 1)),
            ..files()
        };
        assert_eq!(
            said(&keyring, FOLDER),
            "Parked logins are kept in the login keyring, which every Pitboard directory of your \
             session shares.\n\
             1 parked login(s) wait to be deleted from 0600 files in ~/.pitboard/vault: the next \
             change deletes them.\n"
        );
    }

    #[test]
    fn a_move_says_where_the_parked_logins_are_now() {
        let moved = |changed, moved| Moved {
            to: VaultForm::Files,
            changed,
            moved,
            leaving: 0,
        };
        let login = Keyring::Login;
        assert_eq!(
            moved_line(&moved(true, 2), FOLDER, login),
            "Moved 2 parked login(s) into 0600 files in ~/.pitboard/vault."
        );
        assert_eq!(
            moved_line(&moved(true, 0), FOLDER, login),
            "Parked logins are kept in 0600 files in ~/.pitboard/vault from now on."
        );
        assert_eq!(
            moved_line(&moved(false, 0), FOLDER, login),
            "Parked logins are already kept in 0600 files in ~/.pitboard/vault."
        );
        let into_the_keyring = Moved {
            to: VaultForm::SecretService,
            ..moved(true, 2)
        };
        assert_eq!(
            moved_line(&into_the_keyring, FOLDER, login),
            "Moved 2 parked login(s) into the login keyring."
        );
        assert_eq!(
            moved_line(&into_the_keyring, FOLDER, Keyring::KWallet),
            "Moved 2 parked login(s) into KWallet."
        );
    }

    // On KDE Plasma KWallet answers for the Secret Service.
    #[test]
    fn vault_names_kwallet_where_it_answers_for_the_secret_service() {
        let kwallet = Keeping {
            place: Some(Vault::SecretService),
            shared: true,
            keyring: Some(Keyring::KWallet),
            leaving: Some((VaultForm::SecretService, 1)),
            ..files()
        };
        assert_eq!(
            said(&kwallet, FOLDER),
            "Parked logins are kept in KWallet, which every Pitboard directory of your session \
             shares.\n\
             1 parked login(s) wait to be deleted from KWallet: the next change deletes them.\n"
        );
        let in_files = Keeping {
            keyring: Some(Keyring::KWallet),
            ..files()
        };
        assert!(
            said(&in_files, FOLDER).ends_with(
                "A Secret Service answers on this session's bus: `pitboard vault move \
                 secret-service` keeps them in KWallet instead.\n"
            ),
            "{}",
            said(&in_files, FOLDER)
        );
    }
}
