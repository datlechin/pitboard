//! Finishing what an interrupted run started.
//!
//! A switch records its intent before it creates the park item that intent names, so a run
//! that dies leaves a trail the next one follows. The decision is a pure function of what
//! was found, which is what lets every point a run can be killed be a table entry rather
//! than a thought experiment.

use super::{Error, Result};
use crate::state::{Generation, State};
use crate::{claude, home, park, state, store, time};
use serde_json::Value;
use std::path::PathBuf;

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct Journal {
    pub(super) started_at: i64,
    pub(super) from_label: String,
    pub(super) from_uuid: String,
    pub(super) to_label: String,
    pub(super) park_service: String,
    pub(super) incoming_fingerprint: String,
}

pub(super) fn journal_path() -> PathBuf {
    home::dir().join("journal.json")
}

pub(super) fn write_journal(entry: &Journal) -> Result<()> {
    let path = journal_path();
    let fail = |source| Error::RecoveryFailed {
        path: path.clone(),
        source,
    };
    home::ensure().map_err(fail)?;
    let body = serde_json::to_string(entry).expect("a journal entry is always serialisable");
    std::fs::write(&path, body).map_err(fail)
}

/// What an interrupted run left behind, expressed as facts rather than as files.
struct Interrupted<'a> {
    journal: &'a Journal,
    /// The credential found at the park name the journal reserved, if anything is there.
    parked: Option<Value>,
    /// The refresh fingerprint of the credential that is live right now.
    live_fingerprint: String,
}

/// What must change in state to finish an interrupted run.
#[derive(Default, Debug, PartialEq)]
struct Repair {
    attach: Option<(String, Generation)>,
    mark_installed: Option<(String, String)>,
    set_active: Option<String>,
    notes: Vec<&'static str>,
}

/// Decide the repair. Pure, so every way a run can be killed is a table entry.
fn repair_for(state: &State, facts: &Interrupted) -> Repair {
    let journal = facts.journal;
    let mut repair = Repair::default();

    // Attaching is idempotent: the run may have died before or after recording the park.
    if !state.references(&journal.park_service)
        && let Some(oauth) = &facts.parked
    {
        repair.attach = Some((
            journal.from_label.clone(),
            Generation {
                service: journal.park_service.clone(),
                parked_at: journal.started_at,
                refresh_fingerprint: park::fingerprint_of(oauth),
                installed_at: None,
            },
        ));
        repair
            .notes
            .push("its parked credential has been recovered");
    }

    // Whether the install landed is decided by the credential itself, not by how far the
    // journal got. An empty fingerprint means "no refresh token found", on either side, and
    // two of those are not a match.
    let completed = !journal.incoming_fingerprint.is_empty()
        && facts.live_fingerprint == journal.incoming_fingerprint;
    if completed {
        repair.set_active = Some(journal.to_label.clone());
        repair.notes.push("the switch had in fact completed");

        // Without this the installed copy stays restorable, and restoring a credential
        // Claude Code has since rotated past zeroes the live one.
        if let Some(account) = state.get(&journal.to_label)
            && let Some(installed) = account
                .generations
                .iter()
                .find(|g| g.refresh_fingerprint == journal.incoming_fingerprint)
        {
            repair.mark_installed = Some((journal.to_label.clone(), installed.service.clone()));
        }
    }
    repair
}

fn apply(state: &mut State, repair: Repair, at: i64) -> String {
    if let Some((label, generation)) = repair.attach {
        state.attach(&label, generation);
    }
    if let Some((label, service)) = repair.mark_installed {
        state.mark_installed(&label, &service, at);
    }
    if let Some(label) = repair.set_active {
        state.active = Some(label);
    }
    repair.notes.join("; ")
}

/// Finish, in state, what an interrupted run already did in the keychain.
///
/// Called while holding pitboard's own lock, so no other run is in flight. A park item that
/// exists but is referenced by nothing would otherwise be invisible to every command,
/// leaving the account pointing at an older copy whose token has since been rotated.
pub(super) fn reconcile(state: &mut State) -> Result<Option<String>> {
    let path = journal_path();
    let raw = match std::fs::read_to_string(&path) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(Error::RecoveryFailed { path, source }),
    };
    let Ok(journal) = serde_json::from_str::<Journal>(&raw) else {
        let _ = std::fs::remove_file(&path);
        return Ok(Some(
            "an interrupted switch left an unreadable record; ignoring it".into(),
        ));
    };

    let parked = store::vault_read(&journal.park_service)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
    let live_fingerprint = store::read(&claude::live_service())
        .ok()
        .flatten()
        .map(|live| park::fingerprint_of(&live["claudeAiOauth"]))
        .unwrap_or_default();

    let repair = repair_for(
        state,
        &Interrupted {
            journal: &journal,
            parked,
            live_fingerprint,
        },
    );
    let detail = apply(state, repair, time::now());

    let mut note = format!(
        "an earlier switch from `{}` to `{}` was interrupted",
        journal.from_label, journal.to_label
    );
    if !detail.is_empty() {
        note.push_str("; ");
        note.push_str(&detail);
    }

    state::save(state)?;
    let _ = std::fs::remove_file(&path);
    Ok(Some(note))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Account;

    const PARK: &str = "pitboard-park-from-uuid-1700000000000";

    fn journal(incoming: &str) -> Journal {
        Journal {
            started_at: 1_700_000_000,
            from_label: "from".into(),
            from_uuid: "from-uuid".into(),
            to_label: "to".into(),
            park_service: PARK.into(),
            incoming_fingerprint: incoming.into(),
        }
    }

    fn account(label: &str, generations: Vec<Generation>) -> Account {
        Account {
            label: label.into(),
            account_uuid: format!("{label}-uuid"),
            email: format!("{label}@example.com"),
            organization_uuid: format!("{label}-org"),
            oauth_account: serde_json::json!({}),
            generations,
        }
    }

    fn generation(service: &str, fingerprint: &str) -> Generation {
        Generation {
            service: service.into(),
            parked_at: 1_699_000_000,
            refresh_fingerprint: fingerprint.into(),
            installed_at: None,
        }
    }

    fn state(accounts: Vec<Account>) -> State {
        State {
            accounts,
            ..State::default()
        }
    }

    fn parked_credential(refresh: &str) -> Value {
        serde_json::json!({"refreshToken": refresh, "accessToken": "a"})
    }

    /// Killed after reserving the park name but before writing it.
    #[test]
    fn nothing_was_parked_so_nothing_is_recovered() {
        let j = journal("incoming-fp");
        let s = state(vec![account("from", vec![]), account("to", vec![])]);
        let repair = repair_for(
            &s,
            &Interrupted {
                journal: &j,
                parked: None,
                live_fingerprint: "something-else".into(),
            },
        );
        assert_eq!(repair, Repair::default());
    }

    /// Killed after the park was written but before state recorded it.
    #[test]
    fn an_orphaned_park_is_reattached_to_the_account_it_belongs_to() {
        let j = journal("incoming-fp");
        let s = state(vec![account("from", vec![]), account("to", vec![])]);
        let repair = repair_for(
            &s,
            &Interrupted {
                journal: &j,
                parked: Some(parked_credential("outgoing-token")),
                live_fingerprint: "something-else".into(),
            },
        );
        let (label, generation) = repair.attach.expect("the orphan must be recovered");
        assert_eq!(label, "from");
        assert_eq!(generation.service, PARK);
        assert_eq!(generation.installed_at, None);
    }

    /// Killed after state already recorded the park. Recovery must not record it twice.
    #[test]
    fn an_already_recorded_park_is_not_attached_again() {
        let j = journal("incoming-fp");
        let s = state(vec![
            account("from", vec![generation(PARK, "outgoing-fp")]),
            account("to", vec![]),
        ]);
        let repair = repair_for(
            &s,
            &Interrupted {
                journal: &j,
                parked: Some(parked_credential("outgoing-token")),
                live_fingerprint: "something-else".into(),
            },
        );
        assert_eq!(repair.attach, None);
    }

    /// Killed after the credential was installed but before state was saved. The installed
    /// copy must be marked, or it stays restorable and restoring it zeroes the live one.
    #[test]
    fn a_completed_install_marks_the_copy_it_consumed() {
        let j = journal("incoming-fp");
        let s = state(vec![
            account("from", vec![]),
            account("to", vec![generation("pitboard-park-to-1", "incoming-fp")]),
        ]);
        let repair = repair_for(
            &s,
            &Interrupted {
                journal: &j,
                parked: None,
                live_fingerprint: "incoming-fp".into(),
            },
        );
        assert_eq!(repair.set_active.as_deref(), Some("to"));
        assert_eq!(
            repair.mark_installed,
            Some(("to".into(), "pitboard-park-to-1".into())),
            "the generation that is now live must never be offered again"
        );
    }

    /// A credential with no refresh token fingerprints to the empty string on both sides.
    /// Two of those are not a match, and treating them as one declares a failed switch
    /// complete.
    #[test]
    fn two_missing_fingerprints_are_not_a_match() {
        let j = journal("");
        let s = state(vec![account("from", vec![]), account("to", vec![])]);
        let repair = repair_for(
            &s,
            &Interrupted {
                journal: &j,
                parked: None,
                live_fingerprint: String::new(),
            },
        );
        assert_eq!(
            repair.set_active, None,
            "an empty fingerprint proves nothing"
        );
        assert_eq!(repair.mark_installed, None);
    }

    /// The target was forgotten between the crash and the recovery.
    #[test]
    fn a_vanished_target_account_does_not_panic() {
        let j = journal("incoming-fp");
        let s = state(vec![account("from", vec![])]);
        let repair = repair_for(
            &s,
            &Interrupted {
                journal: &j,
                parked: None,
                live_fingerprint: "incoming-fp".into(),
            },
        );
        assert_eq!(repair.set_active.as_deref(), Some("to"));
        assert_eq!(repair.mark_installed, None);
    }
}
