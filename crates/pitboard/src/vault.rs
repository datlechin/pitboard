//! `pitboard vault`: where parked logins are kept. Which ways a system keeps them is the core's
//! (`pitboard_core::switch::Keeping`).

use crate::Report;
use clap::Subcommand;
use pitboard_core::host::Vault;
use pitboard_core::service::Pitboard;
use pitboard_core::switch::Keeping;
use serde_json::{Value, json};

#[derive(Subcommand)]
pub(crate) enum VaultCommand {
    /// Say where parked logins are kept (the default)
    Status,
}

pub(crate) fn run(pitboard: &Pitboard, what: Option<&VaultCommand>) -> Report {
    match what {
        None | Some(VaultCommand::Status) => status(pitboard),
    }
}

fn status(pitboard: &Pitboard) -> Report {
    match pitboard.vault() {
        Ok(kept) => Report::done("vault", kept_json(&kept), said(&kept, pitboard)),
        Err(error) => Report::failed(Some("vault"), error),
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
    })
}

fn said(kept: &Keeping, pitboard: &Pitboard) -> String {
    let folder = kept
        .folder
        .as_deref()
        .map(|folder| pitboard.shown(folder))
        .unwrap_or_default();
    let mut lines = vec![match kept.place {
        Some(Vault::Keychain) => "Parked logins are kept in the login keychain, which every \
                                  Pitboard directory on this Mac shares."
            .to_string(),
        Some(Vault::SecretService) => "Parked logins are kept in the login keyring, which every \
                                       Pitboard directory of your session shares."
            .to_string(),
        Some(Vault::Files) => {
            format!("Parked logins are kept in 0600 files in {folder}, which only you can read.")
        }
        Some(Vault::SealedFiles) => {
            format!("Parked logins are kept in files in {folder}, sealed to your Windows account.")
        }
        None => "Pitboard cannot tell where this directory keeps its parked logins.".to_string(),
    }];
    match (kept.place, kept.secret_service) {
        (Some(Vault::SecretService), Some(false)) => lines.push(
            "No Secret Service answers on this session's bus, so they cannot be read from here. \
             Run Pitboard in your desktop session, where signing in unlocks the login keyring."
                .into(),
        ),
        (Some(Vault::Files), Some(false)) => lines.push(
            "No Secret Service answers on this session's bus, as over SSH or in a container."
                .into(),
        ),
        _ => {}
    }
    lines.into_iter().map(|line| format!("{line}\n")).collect()
}
