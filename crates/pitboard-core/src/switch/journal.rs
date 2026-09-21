//! Finishing what an interrupted run started.
//!
//! A switch writes this record before creating the park item it names. Whether an install
//! landed is decided by asking Anthropic who owns the live credential, an answer that
//! survives Claude Code rotating the token. When any fact cannot be read, recovery changes
//! nothing and keeps the record.

use super::{Error, Result, identify};
use crate::context::Context;
use crate::state::{Park, State};
use crate::{atomic, claude, home, park, state, store};
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
    /// The park being installed, so recovery consumes exactly that copy.
    pub(super) incoming_service: String,
}

/// What a later run found an interrupted switch had done, now recorded in the state.
#[derive(Debug)]
pub struct Recovered {
    pub from: String,
    pub to: String,
    pub finished: bool,
}

impl Recovered {
    pub fn code(&self) -> &'static str {
        if self.finished {
            "interrupted_switch_finished"
        } else {
            "interrupted_switch_undone"
        }
    }
}

impl std::fmt::Display for Recovered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "an earlier switch from `{}` to `{}` was interrupted; {}",
            self.from,
            self.to,
            if self.finished {
                "it had in fact finished, and pitboard has recorded that"
            } else {
                "it had not finished, and nothing was lost"
            }
        )
    }
}

fn journal_path(ctx: &Context) -> PathBuf {
    home::dir(ctx).join("journal.json")
}

/// Durable before the park it names is created: a record lost to a crash would leave a
/// consumed login looking restorable.
pub(super) fn write_journal(ctx: &Context, entry: &Journal) -> Result<()> {
    let path = journal_path(ctx);
    let fail = |source| Error::RecoveryFailed {
        path: path.clone(),
        source,
    };
    home::ensure(ctx).map_err(fail)?;
    let body = serde_json::to_string(entry).expect("a journal entry is always serialisable");
    atomic::write(&path, body.as_bytes(), atomic::Perms::Secret).map_err(fail)
}

/// A switch was interrupted, and the next command that changes state will finish it.
pub fn pending(ctx: &Context) -> bool {
    journal_path(ctx).exists()
}

/// The switch reached a state the account index fully describes.
pub(super) fn clear_journal(ctx: &Context) {
    let _ = std::fs::remove_file(journal_path(ctx));
}

struct Found {
    /// The park the record reserved: written, never written, or `None` if unreadable.
    parked: Option<Option<Value>>,
    /// The account the live login belongs to, or `None` if that cannot be learned.
    live_owner: Option<String>,
}

#[derive(Default, Debug, PartialEq)]
struct Repair {
    /// Hold the interrupted run's park for the account it came from. Keyed by account id,
    /// not label: the label may have been reused since.
    hold: Option<(String, Park)>,
    /// The interrupted run's park copies a login that is still signed in, so Claude Code
    /// will rotate past it.
    drop: bool,
    /// The destination's login is live: it is active, and its park was consumed.
    landed: bool,
}

/// `None` when the facts do not settle what happened.
fn repair_for(state: &State, journal: &Journal, found: &Found) -> Option<Repair> {
    let parked = found.parked.as_ref()?;
    let owner = found.live_owner.as_deref()?;
    let mut repair = Repair {
        landed: owner == journal.to_uuid,
        ..Repair::default()
    };
    if owner == journal.from_uuid {
        repair.drop = parked.is_some();
    } else if let Some(oauth) = parked
        && !state.references(&journal.park_service)
    {
        repair.hold = Some((
            journal.from_uuid.clone(),
            park::describe(&journal.park_service, journal.started_at, oauth),
        ));
    }
    Some(repair)
}

fn apply(state: &mut State, journal: &Journal, repair: Repair) {
    if repair.drop {
        state.discard(&journal.park_service);
    }
    if let Some((uuid, park)) = repair.hold {
        match state.by_uuid(&uuid).map(|a| a.label.clone()) {
            Some(label) => state.park(&label, park),
            // The account it belongs to is gone, so nothing will ever restore this copy.
            // Listing it is what gets it deleted rather than left in the keychain.
            None => state.discard(&park.service),
        }
    }
    if repair.landed && state.get(&journal.to_label).is_some() {
        state.active = Some(journal.to_label.clone());
        state.discard(&journal.incoming_service);
    }
}

fn read_park(ctx: &Context, service: &str) -> Option<Option<Value>> {
    match store::vault_read(ctx, service) {
        Ok(raw) => Some(raw.and_then(|r| serde_json::from_str(&r).ok())),
        Err(_) => None,
    }
}

fn live_owner(ctx: &Context) -> std::result::Result<String, String> {
    let live = store::read(ctx, &claude::live_service(ctx))
        .map_err(|e| e.to_string())?
        .ok_or("nothing is signed in")?;
    let token = live["claudeAiOauth"]["accessToken"]
        .as_str()
        .ok_or("the signed-in credential has no access token")?;
    identify(ctx, token)
        .map(|owner| owner.account_uuid)
        .map_err(|e| e.to_string())
}

pub(super) fn reconcile(ctx: &Context, state: &mut State) -> Result<Option<Recovered>> {
    let path = journal_path(ctx);
    let raw = match std::fs::read_to_string(&path) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(Error::RecoveryFailed { path, source }),
    };
    // Written atomically, so a record that does not parse was damaged afterwards and says
    // nothing about how far its switch got.
    let journal = serde_json::from_str::<Journal>(&raw)
        .map_err(|source| Error::RecoveryRecordCorrupt { path, source })?;

    let owner = live_owner(ctx);
    let found = Found {
        parked: read_park(ctx, &journal.park_service),
        live_owner: owner.as_ref().ok().cloned(),
    };
    let Some(repair) = repair_for(state, &journal, &found) else {
        return Err(Error::RecoveryUndetermined {
            from: journal.from_label,
            to: journal.to_label,
            detail: owner
                .err()
                .unwrap_or_else(|| "its parked login could not be read".into()),
        });
    };
    let finished = repair.landed;
    apply(state, &journal, repair);
    state::save(ctx, state)?;
    clear_journal(ctx);

    Ok(Some(Recovered {
        from: journal.from_label,
        to: journal.to_label,
        finished,
    }))
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

    fn account(label: &str, parked: Option<&str>) -> Account {
        Account {
            label: label.into(),
            account_uuid: format!("{label}-uuid"),
            email: format!("{label}@example.com"),
            organization_uuid: format!("{label}-org"),
            oauth_account: serde_json::json!({}),
            parked: parked.map(|s| Park {
                service: s.into(),
                parked_at: 1_699_000_000,
                refresh_fingerprint: "f".into(),
                access_expires_at: None,
                refresh_expires_at: None,
            }),
        }
    }

    /// `from` signed in with nothing parked, `to` parked: the state before a switch.
    fn before() -> State {
        State {
            accounts: vec![account("from", None), account("to", Some(INCOMING))],
            ..State::default()
        }
    }

    fn written() -> Option<Option<Value>> {
        Some(Some(
            serde_json::json!({"refreshToken": "outgoing", "accessToken": "a"}),
        ))
    }

    fn found(parked: Option<Option<Value>>, owner: Option<&str>) -> Found {
        Found {
            parked,
            live_owner: owner.map(str::to_owned),
        }
    }

    /// Killed after reserving the park name but before writing it.
    #[test]
    fn nothing_parked_and_nothing_installed_changes_nothing() {
        let repair = repair_for(&before(), &journal(), &found(Some(None), Some("from-uuid")));
        assert_eq!(repair, Some(Repair::default()));
    }

    /// Killed after the park was written, before the install. `from` is still signed in, so
    /// the park is a second copy of a live login and Claude Code will rotate past it.
    #[test]
    fn a_park_of_a_login_still_signed_in_is_dropped_not_kept() {
        for s in [before(), {
            let mut recorded = before();
            recorded.park("from", account("x", Some(PARK)).parked.unwrap());
            recorded
        }] {
            let repair = repair_for(&s, &journal(), &found(written(), Some("from-uuid"))).unwrap();
            assert!(repair.drop && repair.hold.is_none() && !repair.landed);

            let mut applied = s;
            apply(&mut applied, &journal(), repair);
            assert!(!applied.references(PARK));
            assert!(applied.discarded.contains(&PARK.to_string()));
        }
    }

    /// Killed after the install, before state recorded it. Claude Code may already have
    /// rotated the token.
    #[test]
    fn a_landed_switch_holds_the_outgoing_login_and_consumes_the_incoming_one() {
        let mut s = before();
        let repair = repair_for(&s, &journal(), &found(written(), Some("to-uuid"))).unwrap();
        let (uuid, park) = repair.hold.clone().expect("the orphan must be recovered");
        assert_eq!(uuid, "from-uuid", "held by account id, never by a label");
        assert_eq!(park.service, PARK);
        assert!(repair.landed);

        apply(&mut s, &journal(), repair);
        assert_eq!(s.active.as_deref(), Some("to"));
        assert_eq!(
            s.get("from").unwrap().parked.as_ref().unwrap().service,
            PARK
        );
        assert!(
            s.get("to").unwrap().parked.is_none(),
            "the copy now live must never be offered again"
        );
        assert!(s.discarded.contains(&INCOMING.to_string()));
    }

    /// Someone signed in as a third account since: the orphan may be the only copy of
    /// `from`'s login, and the destination's park may still be good.
    #[test]
    fn a_third_account_signed_in_since_keeps_both_parks() {
        let mut s = before();
        let repair = repair_for(&s, &journal(), &found(written(), Some("other-uuid"))).unwrap();
        apply(&mut s, &journal(), repair);
        assert!(s.references(PARK) && s.references(INCOMING));
        assert!(s.discarded.is_empty());
    }

    #[test]
    fn an_already_recorded_park_is_not_held_twice() {
        let mut s = before();
        s.park("from", account("x", Some(PARK)).parked.unwrap());
        let repair = repair_for(&s, &journal(), &found(written(), Some("to-uuid"))).unwrap();
        assert_eq!(repair.hold, None);
    }

    #[test]
    fn an_unknown_outcome_changes_nothing_and_keeps_the_record() {
        for unknown in [found(written(), None), found(None, Some("to-uuid"))] {
            assert_eq!(
                repair_for(&before(), &journal(), &unknown),
                None,
                "could-not-tell must never be read as nothing-there"
            );
        }
    }

    #[test]
    fn a_park_whose_account_was_forgotten_is_not_filed_under_another() {
        let mut s = State {
            accounts: vec![account("other", None)],
            ..State::default()
        };
        let repair = repair_for(&s, &journal(), &found(written(), Some("to-uuid"))).unwrap();
        apply(&mut s, &journal(), repair);
        assert!(
            !s.references(PARK),
            "a park must never be filed under whatever account happens to hold a label"
        );
        assert_eq!(
            s.active, None,
            "a destination that is gone is not made active"
        );
    }
}
