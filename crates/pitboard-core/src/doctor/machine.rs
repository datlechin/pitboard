use super::{Check, Facts, fail, ok, warn, words};
use crate::context::Context;
use crate::error::Error;
use crate::host::{Access, Kind, Os};
use crate::provider::ProviderId;
use crate::provider::claude::live as claude_live;
use crate::store;
use crate::words as said;
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

pub(super) fn elevated(
    os: Os,
    elevation: crate::host::Elevation,
    admin_approval: Option<bool>,
) -> Option<Check> {
    let told = match elevation {
        crate::host::Elevation::Normal => return None,
        crate::host::Elevation::Elevated { why } => said::elevated(os, Some(why)),
        crate::host::Elevation::Unknown => said::elevated(os, None),
    };
    // `EnableLUA` 0 says it is User Account Control off, not the built-in Administrator.
    let way_out = match (elevation, admin_approval) {
        (crate::host::Elevation::Elevated { why }, Some(false))
            if why == crate::host::token::IN_EVERY_PROGRAM =>
        {
            said::ADMIN_APPROVAL_OFF
        }
        _ => told.way_out,
    };
    Some(fail(
        "elevated",
        "runs as",
        told.because,
        words::changes_nothing_this_way(way_out),
    ))
}

pub(super) fn too_old(floor: crate::host::Floor) -> Option<Check> {
    let told = match floor {
        crate::host::Floor::Met => return None,
        crate::host::Floor::Below { build } => said::too_old(Some(build)),
        crate::host::Floor::Unknown => said::too_old(None),
    };
    Some(fail(
        "system_too_old",
        "runs on",
        told.because,
        words::changes_nothing_here(told.way_out),
    ))
}

pub(super) fn judge_security_tool(facts: &Facts) -> Option<Check> {
    let tool = facts.os.secrets_tool()?;
    Some(match &facts.security_tool {
        Some(path) => ok("security_tool", "security tool", path.clone()),
        None => fail(
            "security_tool",
            "security tool",
            words::missing(tool),
            words::READS_THE_KEYCHAIN_AS_CLAUDE_CODE,
        ),
    })
}

pub(super) fn judge_home(facts: &Facts) -> Check {
    let home = facts.home.display();
    match &facts.home_access {
        None => ok("home", "Pitboard home", words::not_created_yet(home)),
        Some(access) if !access.shared => ok("home", "Pitboard home", home.to_string()),
        Some(access) => warn(
            "home",
            "Pitboard home",
            words::open_to(&home, access),
            words::only_you_should_read_the_home(&facts.os.make_private_commands(
                Kind::Directory,
                &[&home.to_string()],
                facts.sid.as_deref(),
            )),
        ),
    }
}

pub(super) fn judge_private_on_disk(facts: &Facts) -> Check {
    if facts.readable_by_others.is_empty() {
        return ok(
            "private_on_disk",
            "logins on disk",
            words::NOTHING_READABLE_BY_OTHERS,
        );
    }
    let names: Vec<String> = facts
        .readable_by_others
        .iter()
        .map(|(path, _, access)| words::open_to(path, access))
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
        words::others_can_read(facts.os, &commands),
    )
}

pub(super) fn judge_state(facts: &Facts) -> Check {
    match &facts.state {
        Ok(state) => ok("state", "accounts", words::enrolled(state.accounts.len())),
        Err(e) => fail(
            "state",
            "accounts",
            e.to_string(),
            match e {
                // The one unreadable state that has a command of its own.
                Error::StateWrongMachine { .. } => words::ADOPT_TO_KEEP_THEM,
                _ => words::NO_SWITCH_UNTIL_READABLE,
            },
        ),
    }
}

pub(super) fn judge_interrupted(facts: &Facts) -> Option<Check> {
    let (detail, advice) = match &facts.stuck {
        Some(refusal) => (refusal.clone(), words::ABANDON_KEEPS_EVERY_LOGIN),
        None if facts.interrupted => (
            words::A_SWITCH_DID_NOT_FINISH.to_string(),
            words::THE_NEXT_CHANGE_FINISHES_IT,
        ),
        None => return None,
    };
    Some(warn(
        "interrupted_switch",
        "interrupted switch",
        detail,
        advice,
    ))
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
    let advice = if vault == 0 {
        words::DELETED_ON_THE_NEXT_CHANGE
    } else {
        words::DELETED_ONCE_THE_KEYCHAIN_OPENS
    };
    Some(warn(
        "discarded",
        "old parked logins",
        words::waiting_to_be_deleted(vault, folders),
        advice,
    ))
}

pub(super) fn judge_machine_id(facts: &Facts) -> Option<Check> {
    if !facts.machine_id_known {
        Some(warn(
            "machine_id",
            "machine id",
            words::NO_STABLE_IDENTIFIER,
            words::never_copy_between_machines(facts.home.display()),
        ))
    } else if facts.os == Os::Windows {
        Some(Check {
            advice: words::A_COPY_KEEPS_THE_MACHINE_GUID.into(),
            ..ok("machine_id", "machine id", words::MACHINE_GUID)
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
        0 => ok("asking", name, words::NOTHING_HELD_BACK),
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
                words::not_asked_about(n, longest),
                words::held_back_by(&holders),
            )
        }
    }
}

/// A name written down before a park was created, still unresolved. Ordinarily there is
/// nothing here: the next change resolves every one of them. What is left is an item that
/// could not be read, which on macOS is a locked keychain and nothing worse.
pub(super) fn judge_pending(facts: &Facts) -> Check {
    match facts.pending_parks.len() {
        0 => ok(
            "pending_parks",
            "parks being reclaimed",
            words::NONE_OUTSTANDING,
        ),
        n => warn(
            "pending_parks",
            "parks being reclaimed",
            words::could_not_be_read_this_time(n),
            words::UNLOCK_AND_RUN_ANY_COMMAND,
        ),
    }
}
