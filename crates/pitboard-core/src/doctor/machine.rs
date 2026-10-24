use super::{Check, Facts, fail, ok, running, warn};
use crate::context::Context;
use crate::error::Error;
use crate::host::{Access, Kind, Os};
use crate::provider::ProviderId;
use crate::provider::claude::live as claude_live;
use crate::{store, words};
use std::path::PathBuf;

// Every file holding a login that others may read, or, where it is sealed, change: who a file
// lets in is what a restore, a copy or a careless umask changes, and on Windows Claude Code's
// file takes its folder's access list (Claude Code 2.1.300, RF-C1b).
pub(super) fn loose_logins(ctx: &Context) -> Vec<(String, Kind, Access)> {
    let mut loose = Vec::new();
    let mut look = |path: PathBuf| {
        if let Some(access) = crate::host::fs::access(&path)
            && access.shared
        {
            let kind = if path.is_dir() {
                Kind::Directory
            } else {
                Kind::File
            };
            loose.push((path.display().to_string(), kind, access));
        }
    };
    look(claude_live::credential_file(ctx));
    // A sealed park opens for this Windows account alone, but sealing stops reading, not
    // writing: another account with write access can corrupt or plant a park, so the vault and
    // its parks are still checked.
    let vault = store::vault_dir(ctx);
    look(vault.clone());
    if let Ok(entries) = std::fs::read_dir(&vault) {
        let mut parks: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        parks.sort();
        for park in parks {
            look(park);
        }
    }
    loose
}

/// The check that fails where the host says this process is not the person themselves
/// ([`crate::host::Host::elevation`]), or nobody could tell whether it is, since then Pitboard
/// changes nothing: `None` where it runs as the person themselves.
pub(super) fn elevated(
    os: Os,
    elevation: crate::host::Elevation,
    admin_approval: Option<bool>,
) -> Option<Check> {
    let said = match elevation {
        crate::host::Elevation::Normal => return None,
        crate::host::Elevation::Elevated { why } => words::elevated(os, Some(why)),
        crate::host::Elevation::Unknown => words::elevated(os, None),
    };
    // `EnableLUA` 0 says it is User Account Control off, not the built-in Administrator.
    let way_out = match (elevation, admin_approval) {
        (crate::host::Elevation::Elevated { why }, Some(false))
            if why == crate::host::token::IN_EVERY_PROGRAM =>
        {
            words::ADMIN_APPROVAL_OFF
        }
        _ => said.way_out,
    };
    Some(fail(
        "elevated",
        "runs as",
        said.because,
        format!(
            "Pitboard changes nothing this way: it reads, and renews and writes nothing. {way_out}"
        ),
    ))
}

pub(super) fn too_old(floor: crate::host::Floor) -> Option<Check> {
    let said = match floor {
        crate::host::Floor::Met => return None,
        crate::host::Floor::Below { build } => words::too_old(Some(build)),
        crate::host::Floor::Unknown => words::too_old(None),
    };
    Some(fail(
        "system_too_old",
        "runs on",
        said.because,
        format!(
            "Pitboard changes nothing here: it reads, and renews and writes nothing. {}",
            said.way_out
        ),
    ))
}

pub(super) fn judge_security_tool(facts: &Facts) -> Option<Check> {
    let tool = facts.os.secrets_tool()?;
    Some(match &facts.security_tool {
        Some(path) => ok("security_tool", "security tool", path.clone()),
        None => fail(
            "security_tool",
            "security tool",
            format!("{tool} is missing"),
            "Pitboard reads the keychain the same way Claude Code does. Without it, nothing works.",
        ),
    })
}

pub(super) fn judge_home(facts: &Facts) -> Check {
    match &facts.home_access {
        None => ok(
            "home",
            "Pitboard home",
            format!("{} (not created yet)", facts.home.display()),
        ),
        Some(access) if !access.shared => {
            ok("home", "Pitboard home", facts.home.display().to_string())
        }
        Some(access) => warn(
            "home",
            "Pitboard home",
            format!("{} is {}", facts.home.display(), access.described),
            format!(
                "Park names contain account identifiers, so only you should read it{}",
                running(&facts.os.make_private_commands(
                    Kind::Directory,
                    &[&facts.home.display().to_string()],
                    facts.sid.as_deref(),
                ))
            ),
        ),
    }
}

pub(super) fn judge_private_on_disk(facts: &Facts) -> Check {
    if facts.readable_by_others.is_empty() {
        return ok(
            "private_on_disk",
            "logins on disk",
            "nothing holding a login is readable by anyone else",
        );
    }
    let names: Vec<String> = facts
        .readable_by_others
        .iter()
        .map(|(path, _, access)| format!("{path} is {}", access.described))
        .collect();
    let sid = facts.sid.as_deref();
    let commands: Vec<String> = match facts.os {
        Os::MacOs | Os::Linux => facts.os.make_private_commands(
            Kind::Any,
            &facts
                .readable_by_others
                .iter()
                .map(|(path, ..)| path.as_str())
                .collect::<Vec<_>>(),
            sid,
        ),
        Os::Windows => facts
            .readable_by_others
            .iter()
            .flat_map(|(path, kind, _)| facts.os.make_private_commands(*kind, &[path], sid))
            .collect(),
    };
    fail(
        "private_on_disk",
        "logins on disk",
        names.join("; "),
        format!(
            "{}{}",
            match facts.os {
                Os::MacOs | Os::Linux =>
                    "These hold usable OAuth tokens in plain text, which is how Claude \
                     Code stores them where there is no keychain. Anyone else on this \
                     machine can read them",
                Os::Windows =>
                    "These hold a login: Claude Code's file in plain text, a sealed park \
                     only this account opens. Claude Code's file takes the access list of \
                     the folder it is in, and a seal stops reading, not writing. Anyone \
                     else on this PC can read or change them",
            },
            running(&commands),
        ),
    )
}

pub(super) fn judge_state(facts: &Facts) -> Check {
    match &facts.state {
        Ok(state) => ok(
            "state",
            "accounts",
            match state.accounts.len() {
                0 => "none enrolled yet".to_string(),
                1 => "1 enrolled".to_string(),
                n => format!("{n} enrolled"),
            },
        ),
        Err(e) => fail(
            "state",
            "accounts",
            e.to_string(),
            match e {
                // The one unreadable state that has a command of its own.
                Error::StateWrongMachine { .. } => {
                    "Run `pitboard adopt` to keep these accounts on this computer. The \
                     logins they came with are dropped, because a login belongs to the \
                     computer that signed in."
                }
                _ => "Pitboard will not switch until its account list can be read.",
            },
        ),
    }
}

pub(super) fn judge_interrupted(facts: &Facts) -> Option<Check> {
    if let Some(refusal) = &facts.stuck {
        Some(warn(
            "interrupted_switch",
            "interrupted switch",
            refusal.clone(),
            "If it still cannot be finished, `pitboard abandon` gives up on the switch and \
             keeps every login.",
        ))
    } else if facts.interrupted {
        Some(warn(
            "interrupted_switch",
            "interrupted switch",
            "a switch did not finish",
            "The next `pitboard use`, `enroll` or `forget` finishes it before anything else.",
        ))
    } else {
        None
    }
}

pub(super) fn judge_discarded(facts: &Facts) -> Option<Check> {
    let state = facts.state.as_ref().ok()?;
    if state.discarded.is_empty() {
        return None;
    }
    // The two kinds apart: a folder is kept until its account is forgotten.
    let folders = state
        .discarded
        .iter()
        .filter(|d| d.kind == crate::state::ParkKind::Folder)
        .count();
    let vault = state.discarded.len() - folders;
    let (detail, advice) = match (vault, folders) {
        (vault, 0) => (
            format!("{vault} waiting to be deleted"),
            "Pitboard deletes them on its next change; if they stay, check the keychain is unlocked.",
        ),
        (0, folders) => (
            format!("{folders} folder(s) waiting to be deleted"),
            "Pitboard deletes them on its next change.",
        ),
        (vault, folders) => (
            format!("{vault} waiting to be deleted, and {folders} folder(s)"),
            "Pitboard deletes them on its next change; if they stay, check the keychain is unlocked.",
        ),
    };
    Some(warn("discarded", "old parked logins", detail, advice))
}

pub(super) fn judge_machine_id(facts: &Facts) -> Option<Check> {
    if !facts.machine_id_known {
        Some(warn(
            "machine_id",
            "machine id",
            "this machine has no stable identifier",
            format!(
                "Pitboard cannot tell this machine from another that also lacks one, so it \
                 cannot refuse state copied between them. Never copy {} between machines.",
                facts.home.display()
            ),
        ))
    } else if facts.os == Os::Windows {
        Some(Check {
            advice: "Pitboard tells this PC apart by it, and a copy of the PC keeps it, such as a \
                     virtual machine copied whole. Such a copy takes this PC's accounts for its \
                     own, and both renew the same parked logins: on a copy, forget each account \
                     and enrol it again with `--sign-in`."
                .into(),
            ..ok("machine_id", "machine id", "Windows' MachineGuid")
        })
    } else {
        None
    }
}

/// Whether Pitboard is waiting before asking Anthropic about anything.
///
/// Ordinarily nothing is waiting: an account is asked about whenever its tightest limit
/// could have moved by a percentage point, and that is the floor rather than a wait. A wait
/// means Anthropic asked for less traffic or could not be reached, and a person watching a
/// number not move deserves to know which.
pub(super) fn judge_asking(facts: &Facts) -> Check {
    // Which services Pitboard asks, named by the tools that have accounts here, and which
    // of them are being held back: a hold is a service's answer, so blaming the wrong one
    // sends somebody to look at a service that is answering normally.
    let tool_of = |id: &str| {
        facts
            .state
            .as_ref()
            .ok()
            .and_then(|s| s.owner_of_park(id))
            .map(crate::state::Account::provider)
    };
    let services = |tools: &mut Vec<ProviderId>| {
        tools.sort();
        if tools.is_empty() {
            tools.push(ProviderId::Claude);
        }
        let mut named: Vec<&str> = Vec::new();
        for service in tools.iter().map(|t| t.service()) {
            if !named.contains(&service) {
                named.push(service);
            }
        }
        named.join(" and ")
    };
    let mut asked: Vec<ProviderId> = facts
        .state
        .as_ref()
        .map(|s| {
            s.accounts
                .iter()
                .map(crate::state::Account::provider)
                .filter(|tool| tool.reads_usage())
                .collect()
        })
        .unwrap_or_default();
    let name = format!("asking {}", services(&mut asked));
    let mut holding: Vec<ProviderId> = facts
        .asking_held
        .iter()
        .filter_map(|(uuid, _)| tool_of(uuid))
        .collect();
    let holders = services(&mut holding);
    match facts.asking_held.len() {
        0 => ok("asking", name, "nothing is being held back"),
        n => {
            let longest = facts
                .asking_held
                .iter()
                .map(|(_, seconds)| *seconds)
                .max()
                .unwrap_or_default();
            warn(
                "asking",
                name,
                format!(
                    "{n} account(s) not being asked about for up to {}",
                    words::span(longest)
                ),
                format!(
                    "{holders} asked for less traffic, or could not be reached. The numbers \
                     shown are the last ones measured until then; `pitboard status --fresh` \
                     does not override a wait {holders} asked for."
                ),
            )
        }
    }
}

/// A name written down before a park was created, still unresolved. Ordinarily there is
/// nothing here: the next change resolves every one of them. What is left is an item that
/// could not be read, which on macOS is a locked keychain and nothing worse.
pub(super) fn judge_pending(facts: &Facts) -> Check {
    match facts.pending_parks.len() {
        0 => ok("pending_parks", "parks being reclaimed", "none outstanding"),
        n => warn(
            "pending_parks",
            "parks being reclaimed",
            format!("{n} could not be read this time"),
            "Pitboard wrote these names down before creating a login in them and cannot \
             read them back to find out what is there. Unlock the keychain and run any \
             Pitboard command; it resolves them before doing anything else.",
        ),
    }
}
