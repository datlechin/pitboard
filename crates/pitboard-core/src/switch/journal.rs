//! Finishing what an interrupted run started.
//!
//! A switch writes this record before creating the park item it names. Whether an install
//! landed is decided by asking Anthropic who owns the live credential, an answer that
//! survives Claude Code rotating the token. When any fact cannot be read, recovery changes
//! nothing and keeps the record.

use super::{Error, Result, identify_document};
use crate::context::Context;
use crate::provider::ProviderId;
use crate::service::Permit;
use crate::state::{Key, Park, State};
use crate::{atomic, home, park, state, store};
use serde_json::Value;
use std::path::PathBuf;

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct Journal {
    /// Which tool's login moved. Both sides of a switch are always the same tool's: a
    /// switch replaces one tool's live login with another account of that same tool.
    ///
    /// Absent from a record written before there was a second tool, and every one of those
    /// was Claude Code's.
    #[serde(default = "claude")]
    pub(super) provider: ProviderId,
    pub(super) started_at: i64,
    pub(super) from_label: String,
    /// The accounts' ids, under the names a record has always given them.
    #[serde(rename = "from_uuid")]
    pub(super) from_id: String,
    pub(super) to_label: String,
    #[serde(rename = "to_uuid")]
    pub(super) to_id: String,
    pub(super) park_service: String,
    /// The park being installed, so recovery consumes exactly that copy.
    pub(super) incoming_service: String,
    /// The refresh tokens on each side, as fingerprints. Eight bytes of SHA-256, which is
    /// what `Park` already records, and no more a secret there than here.
    ///
    /// These let recovery answer the question it usually needs Anthropic for. Empty on a
    /// record written before they existed, which is a record that simply asks.
    #[serde(default)]
    pub(super) from_fingerprint: String,
    #[serde(default)]
    pub(super) to_fingerprint: String,
    /// Where the tool's live login was when the switch started, as the tool names it.
    ///
    /// Which login is live depends on a home variable, and recovery judges an interrupted
    /// switch by reading the live login. Read from another place, it would compare the
    /// switch's two sides with a login that has nothing to do with them, and could decide
    /// the switch landed when it never did. `None` on a record written before this was kept.
    #[serde(default)]
    pub(super) slot: Option<String>,
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
                "it had in fact finished, and Pitboard has recorded that"
            } else {
                "it had not finished, and nothing was lost"
            }
        )
    }
}

fn claude() -> ProviderId {
    ProviderId::Claude
}

impl Journal {
    fn from(&self) -> Key {
        Key::new(self.provider, self.from_label.clone())
    }

    fn to(&self) -> Key {
        Key::new(self.provider, self.to_label.clone())
    }
}

fn journal_path(ctx: &Context) -> PathBuf {
    home::dir(ctx).join("journal.json")
}

/// Durable before the park it names is created: a record lost to a crash would leave a
/// consumed login looking restorable.
pub(super) fn write_journal(ctx: &Context, permit: Permit, entry: &Journal) -> Result<()> {
    let path = journal_path(ctx);
    let fail = |source| Error::RecoveryFailed {
        path: path.clone(),
        source,
    };
    home::ensure(ctx, permit).map_err(fail)?;
    let body = serde_json::to_string(entry).expect("a journal entry is always serialisable");
    atomic::write(permit, &path, body.as_bytes(), atomic::Perms::Secret).map_err(fail)
}

/// A switch was interrupted, and the next command that changes state will finish it.
pub fn pending(ctx: &Context) -> bool {
    journal_path(ctx).exists()
}

/// Which tool's switch was interrupted, where one was and its record can be read.
pub(crate) fn interrupted_tool(ctx: &Context) -> Option<ProviderId> {
    let raw = std::fs::read_to_string(journal_path(ctx)).ok()?;
    serde_json::from_str::<Journal>(&raw)
        .ok()
        .map(|journal| journal.provider)
}

/// The switch reached a state the account index fully describes.
pub(super) fn clear_journal(ctx: &Context, permit: Permit) {
    let _ = crate::host::fs::remove_file(permit, &journal_path(ctx));
}

struct Found {
    /// The park the record reserved: written, never written, or `None` if unreadable.
    parked: Option<Option<Value>>,
    /// The id of the account the live login belongs to, or `None` if that cannot be learned.
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
        landed: owner == journal.to_id,
        ..Repair::default()
    };
    if owner == journal.from_id {
        repair.drop = parked.is_some();
    } else if let Some(oauth) = parked
        && !state.references(&journal.park_service)
    {
        repair.hold = Some((
            journal.from_id.clone(),
            park::describe(
                journal.provider,
                &journal.park_service,
                journal.started_at,
                oauth,
            ),
        ));
    }
    Some(repair)
}

fn apply(state: &mut State, journal: &Journal, repair: Repair) {
    if repair.drop {
        state.discard(&journal.park_service);
    }
    if let Some((id, park)) = repair.hold {
        match state
            .by_id(journal.provider, &id)
            .map(crate::state::Account::key)
        {
            Some(key) => state.park(&key, park),
            // The account it belongs to is gone, so nothing will ever restore this copy.
            // Listing it is what gets it deleted rather than left in the keychain.
            None => state.release(&park.service),
        }
    }
    if repair.landed && state.get(&journal.to()).is_some() {
        state.set_active(journal.provider, Some(journal.to_label.clone()));
        state.discard(&journal.incoming_service);
    }
}

fn read_park(ctx: &Context, service: &str) -> Option<Option<Value>> {
    match store::vault_read(ctx, service) {
        Ok(raw) => Some(raw.and_then(|r| serde_json::from_str(&r).ok())),
        Err(_) => None,
    }
}

/// Whose the live login is, as far as the record and the login itself can say.
enum Owner {
    /// The account's under this id.
    Is(String),
    /// Nobody can say, for this reason.
    Unknown(String),
    /// The record cannot say whose this login is, and the service has not been asked.
    Unasked(Value),
}

/// Whose the live login is, read off the record and the login without asking anybody.
///
/// The fingerprints settle it without a round trip whenever they can, which is what makes
/// an interrupted switch recoverable with no network at all.
fn live_owner(ctx: &Context, journal: &Journal) -> Owner {
    match crate::provider::of(journal.provider).read_live(ctx) {
        Err(e) => Owner::Unknown(e.to_string()),
        Ok(None) => Owner::Unknown("nothing is signed in".into()),
        Ok(Some(live)) => match live_owner_by_fingerprint(journal, &live.raw) {
            Some(id) => Owner::Is(id),
            None => Owner::Unasked(live.raw),
        },
    }
}

/// The id of the account whose the live login is, with the service asked where only it
/// can say.
fn ask(
    ctx: &Context,
    state: &State,
    which: ProviderId,
    owner: Owner,
) -> std::result::Result<String, String> {
    match owner {
        Owner::Is(id) => Ok(id),
        Owner::Unknown(why) => Err(why),
        Owner::Unasked(live) => identify_document(ctx, which, &live)
            .map(|owner| state.id_of(which, &owner))
            .map_err(|e| e.to_string()),
    }
}

/// Who owns the live login, answered from the record rather than from Anthropic, where the
/// record is enough to answer it.
///
/// Recovery needs to know which side of the switch the live credential came from, and
/// asking Anthropic is the only answer that survives Claude Code rotating a token. But the
/// two candidates are both Pitboard's own documents and their refresh tokens were
/// fingerprinted when the record was written, so the common case is a comparison and not a
/// round trip. That is what lets a switch be recovered on a plane.
///
/// It narrows the network dependency rather than removing it. A rotation inside the seconds
/// of an interrupted switch leaves a fingerprint matching neither side, which is exactly
/// when this says nothing and Anthropic is asked after all.
fn live_owner_by_fingerprint(journal: &Journal, live: &Value) -> Option<String> {
    if journal.from_fingerprint.is_empty() || journal.to_fingerprint.is_empty() {
        return None;
    }
    if journal.from_fingerprint == journal.to_fingerprint {
        return None;
    }
    let found = crate::provider::of(journal.provider).fingerprint(live);
    if found.is_empty() {
        return None;
    }
    if found == journal.to_fingerprint {
        Some(journal.to_id.clone())
    } else if found == journal.from_fingerprint {
        Some(journal.from_id.clone())
    } else {
        None
    }
}

/// What abandoning an unfinishable record decided to keep.
#[derive(Debug)]
pub struct Abandoned {
    pub from: String,
    pub to: String,
    /// Copies kept rather than deleted, because which one is live is now unknown.
    pub kept: usize,
}

/// Throws away a record that cannot be finished, keeping every copy it names.
///
/// Recovery needs the service to say who owns the live login. Offline, or with a session
/// the service no longer accepts, it cannot, and every command that changes anything stops
/// at that. This is the way out: nothing is installed and nothing that might be the only
/// copy is deleted, so the worst case is a copy that outlives its use, which `status` shows
/// and `doctor` reports.
///
/// One exception, for a tool whose park may never be a copy: a park whose refresh token is
/// the live login's own is a second copy for certain, whoever owns it, and is dropped
/// rather than kept.
pub(super) fn abandon(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
) -> Result<Option<Abandoned>> {
    let path = journal_path(ctx);
    let raw = match std::fs::read_to_string(&path) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(Error::RecoveryFailed { path, source }),
    };
    let journal = serde_json::from_str::<Journal>(&raw)
        .map_err(|source| Error::RecoveryRecordCorrupt { path, source })?;

    // The copy the interrupted run parked is recorded against the account it came from, so
    // nothing holds a keychain item that no file names.
    let mut kept = 0;
    if let Some(Some(document)) = read_park(ctx, &journal.park_service)
        && park::is_live_twin(ctx, journal.provider, &document)
    {
        state.discard(&journal.park_service);
    } else if let Some(Some(document)) = read_park(ctx, &journal.park_service)
        && let Some(key) = state
            .by_id(journal.provider, &journal.from_id)
            .map(crate::state::Account::key)
    {
        state.park(
            &key,
            park::describe(
                journal.provider,
                &journal.park_service,
                ctx.now(),
                &document,
            ),
        );
        kept += 1;
    }
    let incoming = read_park(ctx, &journal.incoming_service).flatten();
    if incoming.is_some_and(|document| park::is_live_twin(ctx, journal.provider, &document)) {
        state.discard(&journal.incoming_service);
    } else if state
        .by_id(journal.provider, &journal.to_id)
        .and_then(|a| a.parked.as_ref())
        .is_some()
    {
        kept += 1;
    }
    state::save(ctx, permit, state)?;
    clear_journal(ctx, permit);
    Ok(Some(Abandoned {
        from: state.typed(&journal.from()),
        to: state.typed(&journal.to()),
        kept,
    }))
}

/// The record of an interrupted switch, with every fact recovery decides it from that can be
/// read without asking anybody.
struct Waiting {
    journal: Journal,
    /// The park the record reserved: written, never written, or `None` if unreadable.
    parked: Option<Option<Value>>,
    owner: Owner,
}

/// The record of an interrupted switch, where one is waiting, and what recovery decides it
/// from. Reads, and writes nothing whatever it finds.
fn read(ctx: &Context, state: &State) -> Result<Option<Waiting>> {
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

    if let Some(slot) = &journal.slot {
        let here = crate::provider::of(journal.provider).slot(ctx);
        if *slot != here {
            return Err(Error::RecoveryElsewhere {
                tool: journal.provider,
                from: state.typed(&journal.from()),
                to: state.typed(&journal.to()),
                slot: slot.clone(),
            });
        }
    }

    let owner = live_owner(ctx, &journal);
    Ok(Some(Waiting {
        parked: read_park(ctx, &journal.park_service),
        journal,
        owner,
    }))
}

/// What finishes or undoes the switch, or, where the facts do not settle what it did, the
/// refusal of the change that found them so.
fn decide(
    state: &State,
    journal: &Journal,
    parked: Option<Option<Value>>,
    owner: std::result::Result<String, String>,
) -> Result<Repair> {
    let found = Found {
        parked,
        live_owner: owner.as_ref().ok().cloned(),
    };
    repair_for(state, journal, &found).ok_or_else(|| Error::RecoveryUndetermined {
        tool: journal.provider,
        from: state.typed(&journal.from()),
        to: state.typed(&journal.to()),
        detail: owner
            .err()
            .unwrap_or_else(|| "its parked login could not be read".into()),
    })
}

pub(super) fn reconcile(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
) -> Result<Option<Recovered>> {
    let Some(Waiting {
        journal,
        parked,
        owner,
    }) = read(ctx, state)?
    else {
        return Ok(None);
    };
    let owner = ask(ctx, state, journal.provider, owner);
    let repair = decide(state, &journal, parked, owner)?;
    let finished = repair.landed;
    apply(state, &journal, repair);
    state::save(ctx, permit, state)?;
    clear_journal(ctx, permit);

    Ok(Some(Recovered {
        from: state.typed(&journal.from()),
        to: state.typed(&journal.to()),
        finished,
    }))
}

/// Whether working out what an interrupted switch did may ask the service whose the live
/// login is, where the record cannot say. A change asks, and so does `status`, which asks
/// the service about every account anyway; `doctor` and `status_offline` send no request.
/// A tool whose login names its own account is identified from the login either way, which
/// asks nobody.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Asking {
    Service,
    Nobody,
}

/// The refusal the next change would make over the interrupted switch waiting here, worked
/// out by reading what [`reconcile`] reads and deciding as it decides, and changing nothing.
///
/// `None` where no switch is waiting, where the next change finishes or undoes it, and
/// where telling which would need the service and `asking` says not to ask it. A record
/// that cannot be read, or that was written for another slot, is refused by the next
/// change for that reason and not for this one, and is not this function's to say.
pub(super) fn refusal(ctx: &Context, state: &State, asking: Asking) -> Option<Error> {
    let Waiting {
        journal,
        parked,
        owner,
    } = read(ctx, state).ok()??;
    if asking == Asking::Nobody
        && matches!(owner, Owner::Unasked(_))
        && !crate::provider::of(journal.provider).identifies_by_itself()
    {
        return None;
    }
    let owner = ask(ctx, state, journal.provider, owner);
    decide(state, &journal, parked, owner).err()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Account;

    const PARK: &str = "pitboard-park-from-uuid-1700000000000";
    const INCOMING: &str = "pitboard-park-to-uuid-1690000000000";

    /// A record as 0.8.0 writes it. An account enrolled then kept its account uuid as its
    /// id, so what the record names is still each account's id.
    #[test]
    fn a_record_0_8_0_wrote_names_each_side_by_its_id() {
        let written: Journal = serde_json::from_value(serde_json::json!({
            "provider": "claude",
            "started_at": 1_700_000_000,
            "from_label": "from",
            "from_uuid": "from-uuid",
            "to_label": "to",
            "to_uuid": "to-uuid",
            "park_service": PARK,
            "incoming_service": INCOMING,
            "from_fingerprint": "f",
            "to_fingerprint": "t",
            "slot": "Claude Code-credentials"
        }))
        .expect("still read");
        assert_eq!(
            (written.from_id.as_str(), written.to_id.as_str()),
            ("from-uuid", "to-uuid")
        );
    }

    fn journal() -> Journal {
        Journal {
            provider: ProviderId::Claude,
            started_at: 1_700_000_000,
            from_label: "from".into(),
            from_id: "from-uuid".into(),
            to_label: "to".into(),
            to_id: "to-uuid".into(),
            park_service: PARK.into(),
            incoming_service: INCOMING.into(),
            from_fingerprint: "ffffffffffffffff".into(),
            to_fingerprint: "0000000000000000".into(),
            slot: None,
        }
    }

    /// A record written before fingerprints existed simply asks, which is what it always
    /// did. Reading an old record must never be a reason to refuse.
    #[test]
    fn a_record_from_before_the_fingerprints_falls_back_to_asking() {
        let mut j = journal();
        j.from_fingerprint = String::new();
        j.to_fingerprint = String::new();
        assert_eq!(live_owner_by_fingerprint(&j, &live("refresh")), None);
    }

    /// Two sides that fingerprint the same are not two sides. Nothing can be read off that,
    /// not even from a login that matches both.
    #[test]
    fn identical_fingerprints_settle_nothing() {
        let mut j = journal();
        let live = live("refresh");
        j.from_fingerprint = crate::provider::of(ProviderId::Claude).fingerprint(&live);
        j.to_fingerprint = j.from_fingerprint.clone();
        assert_eq!(live_owner_by_fingerprint(&j, &live), None);
    }

    /// A Claude Code login on `refresh`.
    fn live(refresh: &str) -> Value {
        serde_json::json!({"claudeAiOauth": {"refreshToken": refresh, "accessToken": "a"}})
    }

    fn account(label: &str, parked: Option<&str>) -> Account {
        Account {
            last_used_at: None,
            label: label.into(),
            id: format!("{label}-uuid"),
            account_uuid: format!("{label}-uuid"),
            email: format!("{label}@example.com"),
            detail: state::Detail::Claude {
                organization_uuid: format!("{label}-org"),
                oauth_account: serde_json::json!({}),
            },
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
            recorded.park(
                &crate::state::Key::new(crate::provider::ProviderId::Claude, "from"),
                account("x", Some(PARK)).parked.unwrap(),
            );
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
        assert_eq!(s.active_for(ProviderId::Claude), Some("to"));
        assert_eq!(
            s.get(&crate::state::Key::new(
                crate::provider::ProviderId::Claude,
                "from"
            ))
            .unwrap()
            .parked
            .as_ref()
            .unwrap()
            .service,
            PARK
        );
        assert!(
            s.get(&crate::state::Key::new(
                crate::provider::ProviderId::Claude,
                "to"
            ))
            .unwrap()
            .parked
            .is_none(),
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
        s.park(
            &crate::state::Key::new(crate::provider::ProviderId::Claude, "from"),
            account("x", Some(PARK)).parked.unwrap(),
        );
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
            s.active_for(ProviderId::Claude),
            None,
            "a destination that is gone is not made active"
        );
    }
}
