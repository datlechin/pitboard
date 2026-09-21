//! Finishing what an interrupted run started.
//!
//! A switch writes this record before it creates the park item the record names, so a run
//! that dies leaves a trail. Whether the install landed is decided by asking Anthropic who
//! the live credential belongs to: that answer survives Claude Code rotating the token,
//! where comparing token fingerprints does not. When any fact cannot be read, recovery
//! changes nothing and keeps the record — could-not-tell is never treated as nothing-there.

use super::{Error, Result, identify};
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
    pub(super) to_uuid: String,
    pub(super) park_service: String,
    /// The generation being installed, so recovery marks exactly that copy as consumed.
    pub(super) incoming_service: String,
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

struct Found {
    /// The park the record reserved: written, never written, or `None` if unreadable.
    parked: Option<Option<Value>>,
    /// Whether the live credential belongs to the destination, or `None` if unknown.
    landed: Option<bool>,
}

#[derive(Default, Debug, PartialEq)]
struct Repair {
    /// Keyed by account id, not label: the label may have been reused since.
    attach: Option<(String, Generation)>,
    mark_installed: Option<(String, String)>,
    set_active: Option<String>,
}

/// `None` when the facts do not settle what happened.
fn repair_for(state: &State, journal: &Journal, found: &Found) -> Option<Repair> {
    let parked = found.parked.as_ref()?;
    let landed = found.landed?;
    let mut repair = Repair::default();

    if let Some(oauth) = parked
        && !state.references(&journal.park_service)
    {
        repair.attach = Some((
            journal.from_uuid.clone(),
            Generation {
                service: journal.park_service.clone(),
                parked_at: journal.started_at,
                refresh_fingerprint: park::fingerprint_of(oauth),
                installed_at: None,
            },
        ));
    }
    if landed {
        repair.set_active = Some(journal.to_label.clone());
        if state
            .get(&journal.to_label)
            .is_some_and(|a| a.references(&journal.incoming_service))
        {
            repair.mark_installed =
                Some((journal.to_label.clone(), journal.incoming_service.clone()));
        }
    }
    Some(repair)
}

fn apply(state: &mut State, repair: Repair, at: i64) {
    if let Some((uuid, generation)) = repair.attach
        && let Some(label) = state.by_uuid(&uuid).map(|a| a.label.clone())
    {
        state.attach(&label, generation);
    }
    if let Some((label, service)) = repair.mark_installed {
        state.mark_installed(&label, &service, at);
    }
    if let Some(label) = repair.set_active {
        state.active = Some(label);
    }
}

fn read_park(service: &str) -> Option<Option<Value>> {
    match store::vault_read(service) {
        Ok(raw) => Some(raw.and_then(|r| serde_json::from_str(&r).ok())),
        Err(_) => None,
    }
}

fn landed_on(to_uuid: &str) -> std::result::Result<bool, String> {
    let live = store::read(&claude::live_service())
        .map_err(|e| e.to_string())?
        .ok_or("nothing is signed in")?;
    let token = live["claudeAiOauth"]["accessToken"]
        .as_str()
        .ok_or("the signed-in credential has no access token")?;
    identify(token)
        .map(|owner| owner.account_uuid == to_uuid)
        .map_err(|e| e.to_string())
}

pub(super) fn reconcile(state: &mut State) -> Result<Option<String>> {
    let path = journal_path();
    let raw = match std::fs::read_to_string(&path) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(Error::RecoveryFailed { path, source }),
    };
    // The record is written before anything else a switch does, so a torn one means the run
    // died before it changed anything.
    let Ok(journal) = serde_json::from_str::<Journal>(&raw) else {
        let _ = std::fs::remove_file(&path);
        return Ok(Some(
            "an interrupted switch left an unreadable record from before it changed anything"
                .into(),
        ));
    };

    let landed = landed_on(&journal.to_uuid);
    let found = Found {
        parked: read_park(&journal.park_service),
        landed: landed.as_ref().ok().copied(),
    };
    let Some(repair) = repair_for(state, &journal, &found) else {
        return Err(Error::RecoveryUndetermined {
            from: journal.from_label,
            to: journal.to_label,
            detail: landed
                .err()
                .unwrap_or_else(|| "its parked login could not be read".into()),
        });
    };
    let finished = repair.set_active.is_some();
    apply(state, repair, time::now());
    state::save(state)?;
    let _ = std::fs::remove_file(&path);

    Ok(Some(format!(
        "an earlier switch from `{}` to `{}` was interrupted; {}",
        journal.from_label,
        journal.to_label,
        if finished {
            "it had in fact finished, and pitboard has recorded that"
        } else {
            "it had not finished, and nothing was lost"
        }
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Account;

    const PARK: &str = "pitboard-park-from-uuid-1700000000000";
    const INCOMING: &str = "pitboard-park-to-uuid-1690000000000";

    fn journal() -> Journal {
        Journal {
            started_at: 1_700_000_000,
            from_label: "from".into(),
            from_uuid: "from-uuid".into(),
            to_label: "to".into(),
            to_uuid: "to-uuid".into(),
            park_service: PARK.into(),
            incoming_service: INCOMING.into(),
        }
    }

    fn account(label: &str, services: &[&str]) -> Account {
        Account {
            label: label.into(),
            account_uuid: format!("{label}-uuid"),
            email: format!("{label}@example.com"),
            organization_uuid: format!("{label}-org"),
            oauth_account: serde_json::json!({}),
            generations: services
                .iter()
                .map(|s| Generation {
                    service: (*s).into(),
                    parked_at: 1_699_000_000,
                    refresh_fingerprint: "f".into(),
                    installed_at: None,
                })
                .collect(),
        }
    }

    fn state(accounts: Vec<Account>) -> State {
        State {
            accounts,
            ..State::default()
        }
    }

    fn written() -> Option<Option<Value>> {
        Some(Some(
            serde_json::json!({"refreshToken": "outgoing", "accessToken": "a"}),
        ))
    }

    /// Killed after reserving the park name but before writing it.
    #[test]
    fn nothing_parked_and_nothing_installed_changes_nothing() {
        let s = state(vec![account("from", &[]), account("to", &[INCOMING])]);
        let repair = repair_for(
            &s,
            &journal(),
            &Found {
                parked: Some(None),
                landed: Some(false),
            },
        );
        assert_eq!(repair, Some(Repair::default()));
    }

    /// Killed after the park was written, before state recorded it.
    #[test]
    fn an_orphaned_park_is_attached_to_the_account_it_came_from() {
        let s = state(vec![account("from", &[]), account("to", &[INCOMING])]);
        let repair = repair_for(
            &s,
            &journal(),
            &Found {
                parked: written(),
                landed: Some(false),
            },
        )
        .unwrap();
        let (uuid, generation) = repair.attach.expect("the orphan must be recovered");
        assert_eq!(
            uuid, "from-uuid",
            "attached by account id, never by a label"
        );
        assert_eq!(generation.service, PARK);
    }

    #[test]
    fn an_already_recorded_park_is_not_attached_twice() {
        let s = state(vec![account("from", &[PARK]), account("to", &[INCOMING])]);
        let repair = repair_for(
            &s,
            &journal(),
            &Found {
                parked: written(),
                landed: Some(false),
            },
        )
        .unwrap();
        assert_eq!(repair.attach, None);
    }

    /// Killed after the install, before state recorded it. Claude Code may already have
    /// rotated the installed token, which is why landing is decided by who owns the live
    /// credential rather than by comparing fingerprints.
    #[test]
    fn a_landed_install_marks_exactly_the_copy_it_consumed() {
        let s = state(vec![account("from", &[PARK]), account("to", &[INCOMING])]);
        let repair = repair_for(
            &s,
            &journal(),
            &Found {
                parked: written(),
                landed: Some(true),
            },
        )
        .unwrap();
        assert_eq!(repair.set_active.as_deref(), Some("to"));
        assert_eq!(
            repair.mark_installed,
            Some(("to".into(), INCOMING.into())),
            "the copy now live must never be offered again"
        );
    }

    #[test]
    fn an_unknown_outcome_changes_nothing_and_keeps_the_record() {
        let s = state(vec![account("from", &[]), account("to", &[INCOMING])]);
        for found in [
            Found {
                parked: written(),
                landed: None,
            },
            Found {
                parked: None,
                landed: Some(true),
            },
        ] {
            assert_eq!(
                repair_for(&s, &journal(), &found),
                None,
                "could-not-tell must never be read as nothing-there"
            );
        }
    }

    #[test]
    fn a_target_forgotten_since_the_crash_is_still_activated_but_nothing_is_marked() {
        let s = state(vec![account("from", &[PARK])]);
        let repair = repair_for(
            &s,
            &journal(),
            &Found {
                parked: written(),
                landed: Some(true),
            },
        )
        .unwrap();
        assert_eq!(repair.mark_installed, None);
    }

    #[test]
    fn attaching_to_an_account_forgotten_since_the_crash_is_skipped_not_misfiled() {
        let mut s = state(vec![account("other", &[])]);
        apply(
            &mut s,
            Repair {
                attach: Some((
                    "from-uuid".into(),
                    account("from", &[PARK]).generations[0].clone(),
                )),
                ..Repair::default()
            },
            0,
        );
        assert!(
            !s.references(PARK),
            "a park must never be filed under whatever account happens to hold a label"
        );
    }
}
