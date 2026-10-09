//! Putting away the login Claude Code left in a file behind the keychain, when somebody asks.
//!
//! A sign-in where the keychain could not be read leaves its login in `.credentials.json` and
//! the keychain's where it was (the register's `locked_sign_in_writes_fallback`), and nothing
//! of Claude Code's deletes the file while the keychain holds a login
//! (`fallback_outlives_keychain_writes`). While it is there, sessions already running take a
//! switch only at their login's next renewal (`fallback_file_pins_session_login`).
//!
//! The file goes only once the login it held is somewhere Pitboard can switch to it from, or
//! is shown to be there already, by its refresh token's fingerprint, or to be dead, by
//! Anthropic refusing its refresh token. Whose it is comes from that fingerprint where it is a
//! login Pitboard knows, and otherwise from Anthropic, asked with the login's own access token.
//! Where that has expired, the login is renewed as a park is and written back to the file
//! before anything else. Claude Code's refresh lock is held from the last reading of the file
//! until it is gone, so no session spends the refresh token counted on meanwhile
//! (`refresh_lock`). So a run that stops anywhere leaves the login in the file, in a park, or
//! in both, but for the moment between Anthropic answering a renewal and the answer reaching
//! the file, which every renewal has. What could refuse in that moment is asked before the
//! refresh token is sent, and the answer is saved as Claude Code saves its own: over the
//! login the file holds by then, where that is still the one renewed. One that cannot be
//! saved even so, as where a sign-in replaced the file's login meanwhile, is said to be spent.
//! Left in both, the park is written down as a copy of the file's login (`State::from_file`),
//! and the next run parks what the file holds by then in its place: a session that signs in
//! with the file may have renewed it since, spending the park's refresh token. Any other park
//! the account can be switched to is kept, and the file's login goes with the file.

use super::{
    Refreshing, Settled, identify, live_store, purge, read_stored, shape, to_body, write_lock,
};
use crate::api::Owner;
use crate::context::Context;
use crate::error::{Cause, Error, Partway, Result};
use crate::provider::claude::live;
use crate::provider::{self, ProviderError, ProviderId};
use crate::service::{Permit, Warning};
use crate::state::{self, Account, State};
use crate::store::{self, RawStore};
use crate::{fault, lock, park};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The one tool that keeps a file behind the store it keeps its login in.
const TOOL: ProviderId = ProviderId::Claude;

/// How often Claude Code's write lock is asked for to save a renewed login, each for as long
/// as a change waits: 30 seconds in all, twice the lock's staleness.
const SAVE_ROUNDS: u32 = 4;

/// What is left in the file behind the store Claude Code keeps its login in, as putting it
/// away is confirmed for.
#[derive(Debug, Clone, PartialEq)]
pub struct Left {
    pub path: PathBuf,
    /// A handle on what the file holds, byte for byte, never what it holds: it is put away
    /// only while it still holds what this was taken of.
    pub seen: String,
    /// What putting it away would do with the login it holds, as far as can be told without
    /// renewing it.
    pub login: Foreseen,
    /// Its other keys, by name, such as `mcpOAuth`, which are not Pitboard's to move.
    pub dropped: Vec<String>,
}

impl Left {
    /// What putting it away is refused with where the look says so already: a login of an
    /// account nobody enrolled here, which nothing is asked about.
    pub fn refusal(&self) -> Option<Error> {
        match &self.login {
            Foreseen::NotEnrolled(owner) => Some(not_enrolled(&self.path, owner)),
            Foreseen::Kept(_) | Foreseen::Untold => None,
        }
    }
}

/// What putting the file away would do with the login it holds.
#[derive(Debug, Clone, PartialEq)]
pub enum Foreseen {
    Kept(Kept),
    /// The login of an account nobody enrolled here: it is refused, deleting nothing.
    NotEnrolled(Owner),
    /// Its access token has expired, so whose it is can be asked only once it is renewed,
    /// which only putting it away does.
    Untold,
}

/// What putting the file away did with the login it held. `label` is the account's name as a
/// command takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Kept {
    /// It held none.
    NoLogin,
    /// It can no longer be used: Anthropic refuses its refresh token for good, or it has none
    /// and its access token has expired.
    Refused,
    /// The very login Claude Code has stored, `label`'s where that account is enrolled.
    Stored { label: Option<String> },
    /// The very login Pitboard keeps parked for `label`.
    AlreadyParked { label: String },
    /// Another login of `label`, the account whose login Claude Code has stored: a second
    /// sign-in of it, dropped.
    SecondSignIn { label: String },
    /// `label`'s, which held no parked login it could be switched to in its place: parked for
    /// it.
    ParkedNow { label: String },
    /// Another login of `label`, which keeps the parked login it can be switched to.
    ParkKept { label: String },
}

impl Kept {
    /// Stable, for a program to branch on, and the outcome the activity log records.
    pub fn code(&self) -> &'static str {
        match self {
            Kept::NoLogin => "no_login",
            Kept::Refused => "login_refused",
            Kept::Stored { .. } => "stored",
            Kept::AlreadyParked { .. } => "already_parked",
            Kept::SecondSignIn { .. } => "second_sign_in",
            Kept::ParkedNow { .. } => "parked",
            Kept::ParkKept { .. } => "park_kept",
        }
    }

    /// The enrolled account the login is of, by its name, where that is known.
    pub fn account(&self) -> Option<&str> {
        match self {
            Kept::NoLogin | Kept::Refused => None,
            Kept::Stored { label } => label.as_deref(),
            Kept::AlreadyParked { label }
            | Kept::SecondSignIn { label }
            | Kept::ParkedNow { label }
            | Kept::ParkKept { label } => Some(label),
        }
    }
}

/// The file put away: what became of the login it held, and the keys that went with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stowed {
    pub path: PathBuf,
    pub kept: Kept,
    pub dropped: Vec<String>,
}

/// What is left in the file behind Claude Code's store, and what putting it away would do
/// with it: `None` where nothing is behind that store, which is always so where the file is
/// the store, as on Linux. Takes no lock and writes nothing. It asks Anthropic whose the login
/// in the file is, with that login's own access token, where neither its fingerprint nor its
/// expiry already says; and where that names an enrolled account, whose the login stored is,
/// as a read asks it, where that has changed since Anthropic last named it.
pub(crate) fn find(ctx: &Context, state: &State) -> Result<Option<Left>> {
    let path = live::credential_file(ctx);
    let live = live_store(ctx, TOOL)?;
    let Some((_, raw)) = read_left(&live)? else {
        return Ok(None);
    };
    let held = holding(&raw)?;
    let stored = read_stored(TOOL, &live)?.map(|(_, document)| document);
    let login = match judge(ctx, state, &held, stored.as_ref(), &path)? {
        Judged::Kept(kept) => Foreseen::Kept(kept),
        Judged::Untold => Foreseen::Untold,
        Judged::Owner(owner) => match state.account_of(TOOL, &owner) {
            None => Foreseen::NotEnrolled(owner),
            Some(account) => {
                let stored = stored
                    .map(|document| identify::whose(ctx, state, TOOL, &document))
                    .transpose()
                    .map_err(|e| super::unidentified(TOOL, e))?
                    .map(|found| found.owner);
                Foreseen::Kept(kept_for(ctx, state, account, stored.as_ref())?)
            }
        },
    };
    Ok(Some(Left {
        path,
        seen: store::fingerprint(&raw),
        login,
        dropped: held.dropped,
    }))
}

/// Puts away the file behind Claude Code's store, while it holds what `seen` was taken of:
/// keeps the login it holds where it is not kept already or dead, then deletes the file.
/// Whatever stops it is said with how far it had gone ([`Error::PutAwayStopped`]).
///
/// Under Pitboard's lock throughout. Whose the login is, where that can be asked as it is,
/// is asked first, holding up nobody. From the file's last reading until it is gone, under
/// Claude Code's refresh lock too, which a session takes before it sends a refresh token, so
/// none spends the file's meanwhile. Claude Code's write lock is taken after it, as Claude
/// Code takes them, around the renewed login's write and the file's deletion only, and once
/// before a renewal to find it free, so no network round trip holds up its writes. The file,
/// and the login stored where the file goes because of it, are read again under it as the
/// file goes, since a `/logout` that gave up waiting writes with no lock held.
pub fn stow(settled: Settled, seen: &str) -> Result<(Stowed, Vec<Warning>)> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
        permit,
    } = settled;
    let mut putting = Putting {
        ctx: &ctx,
        permit,
        path: live::credential_file(&ctx),
        partway: Partway::Nothing,
        warnings: Vec::new(),
        lock_lost: false,
    };
    let done = putting.run(&mut state, seen);
    let Putting {
        partway, warnings, ..
    } = putting;
    match done {
        Ok(stowed) => {
            purge(&ctx, permit, &mut state);
            Ok((stowed, warnings))
        }
        // Not purged: a park that could not be recorded in place of another leaves the other
        // released here and still named in the file.
        Err(error) => Err(Error::PutAwayStopped {
            partway,
            warnings,
            error: Box::new(error),
        }),
    }
}

/// One run of [`stow`]: how far it has gone, and what it warned of, as it goes.
struct Putting<'a> {
    ctx: &'a Context,
    permit: Permit,
    path: PathBuf,
    partway: Partway,
    warnings: Vec<Warning>,
    /// A lock of Claude Code's stopped being Pitboard's while it held it.
    lock_lost: bool,
}

impl Putting<'_> {
    fn run(&mut self, state: &mut State, seen: &str) -> Result<Stowed> {
        let (ctx, permit) = (self.ctx, self.permit);
        let live = live_store(ctx, TOOL)?;
        let (_, mut raw) = read_left(&live)?
            .filter(|(_, raw)| store::fingerprint(raw) == seen)
            .ok_or_else(|| self.changed())?;
        let mut held = holding(&raw)?;
        let stored = read_stored(TOOL, &live)?.map(|(_, document)| document);
        let judged = judge(ctx, state, &held, stored.as_ref(), &self.path)?;
        if let Judged::Owner(owner) = &judged
            && state.account_of(TOOL, owner).is_none()
        {
            return Err(not_enrolled(&self.path, owner));
        }
        // Whose login Claude Code has stored, told and recorded as a switch tells it, where a
        // second sign-in of the account in use is to be told apart. Either way `stored` is the
        // login stored that the file's was told by.
        let (stored, stored_owner) = match judged {
            Judged::Kept(_) => (stored, None),
            Judged::Owner(_) | Judged::Untold => match identify::now(ctx, permit, state, TOOL)? {
                identify::Live::Login(login) => (Some(login.document), Some(login.owner)),
                identify::Live::Nothing => (None, None),
            },
        };
        fault::point("stow.identified");

        let refreshing = Refreshing::take(ctx, permit, TOOL)?;
        if unchanged(&live, &raw)?.is_none() {
            return Err(self.changed());
        }
        let kept = match judged {
            Judged::Kept(kept) => kept,
            Judged::Owner(owner) => self.keep(state, &owner, stored_owner.as_ref(), &held)?,
            Judged::Untold => match self.renew(&live, &mut raw, &mut held)? {
                None => Kept::Refused,
                Some(owner) => self.keep(state, &owner, stored_owner.as_ref(), &held)?,
            },
        };
        fault::point("stow.kept");

        let writing = write_lock(ctx, permit, TOOL)?;
        let file = unchanged(&live, &raw)?.ok_or_else(|| self.changed())?;
        // A file that goes because of the login stored, as that very login or a second
        // sign-in of its account, goes only while Claude Code has still stored the login it
        // was told by.
        if matches!(kept, Kept::Stored { .. } | Kept::SecondSignIn { .. })
            && !still_stored(&live, stored.as_ref())?
        {
            return Err(Error::StoredLoginChanged {
                path: self.path.clone(),
            });
        }
        file.delete(permit, &live.service)?;
        // Unsaved, a park stays written down as a copy of a file that has gone, and the next
        // file's login of that account is parked in its place: nothing is lost by it.
        if state.file_gone() {
            let _ = state::save(ctx, permit, state);
        }
        self.lock_lost |= let_go(writing) | Refreshing::let_go(refreshing);
        if self.lock_lost {
            self.warnings.push(Warning::LockCompromised { tool: TOOL });
        }
        Ok(Stowed {
            path: self.path.clone(),
            kept,
            dropped: held.dropped,
        })
    }

    /// Renews the login in `held`, as a park is renewed, and writes the renewed login back
    /// into the file before anything else: the exchange spends the refresh token it presents.
    /// So what can stop the write is asked before the token is sent, Claude Code's write lock
    /// and the file as it was read, and nothing after it gives up on the write where the file
    /// still holds the token sent. Then asks whose it is. `None` where Anthropic refuses the
    /// refresh token for good.
    fn renew(
        &mut self,
        live: &provider::LiveStore,
        raw: &mut String,
        held: &mut Held,
    ) -> Result<Option<Owner>> {
        let tool = provider::of(TOOL);
        let slice = held.slice.clone().expect("only a login is renewed");
        let writing = write_lock(self.ctx, self.permit, TOOL)?;
        if unchanged(live, raw)?.is_none() {
            return Err(self.changed());
        }
        drop(writing);
        let fresh = match tool.renew(
            self.ctx,
            self.permit,
            &provider::Credential::new(TOOL, slice.clone()),
        ) {
            Ok(fresh) => fresh,
            Err(ProviderError::InvalidGrant { .. }) => return Ok(None),
            Err(error) => return Err(not_identified(&self.path, error)),
        };
        self.partway = Partway::RenewedUnwritten;
        fault::point("stow.exchanged");
        let writing = self.write_lock_to_save();
        let renewed = self.save(&live.service, &slice, &fresh.raw)?;
        self.lock_lost |= let_go(writing);
        self.partway = Partway::Renewed;
        fault::point("stow.renewed");
        *held = holding(&renewed)?;
        *raw = renewed;
        tool.identify(self.ctx, &fresh)
            .map(|found| Some(Owner::from(found)))
            .map_err(|error| not_identified(&self.path, error))
    }

    /// Claude Code's write lock, to save a renewed login under once the refresh token it was
    /// renewed with is spent. Giving up as a change does would drop that login, so it is asked
    /// for again until a lock nobody touches has gone stale twice over. `None` where it is
    /// still not had: the login is then saved without it, as Claude Code writes on once its
    /// own lock is lost, and that is said.
    fn write_lock_to_save(&mut self) -> Option<lock::Guard> {
        for _ in 0..SAVE_ROUNDS {
            match write_lock(self.ctx, self.permit, TOOL) {
                Ok(writing) => return writing,
                Err(Error::Lock(lock::LockError::Busy)) => {}
                Err(_) => break,
            }
        }
        self.lock_lost = true;
        None
    }

    /// Writes `fresh`, the login `sent` renewed, into the file as Claude Code saves a renewal:
    /// over the login the file holds now, where that still has the refresh token `sent` has,
    /// keeping whatever else the file has come to hold. Read from the file itself, since the
    /// store in front of it need not answer for this. What the file then holds.
    fn save(&self, service: &str, sent: &Value, fresh: &Value) -> Result<String> {
        let tool = provider::of(TOOL);
        let file = self.ctx.host().file(self.path.clone());
        let now = file
            .read(service)?
            .map(|now| live::document_in(&now))
            .filter(|now| tool.fingerprint(now) == tool.fingerprint(sent))
            .ok_or_else(|| self.changed())?;
        let renewed = to_body(tool.splice(&now, fresh).map_err(|e| shape(TOOL, e))?);
        match file.write(self.permit, service, &renewed) {
            // Nothing landed, so once more: nothing else holds the renewed login.
            Err(store::Error::Write(_)) => file.write(self.permit, service, &renewed)?,
            written => written?,
        }
        Ok(renewed)
    }

    /// What putting away `owner`'s login comes to, where Claude Code has `stored`'s login
    /// stored, parking it where that is what keeps it.
    fn keep(
        &mut self,
        state: &mut State,
        owner: &Owner,
        stored: Option<&Owner>,
        held: &Held,
    ) -> Result<Kept> {
        let account = state
            .account_of(TOOL, owner)
            .ok_or_else(|| not_enrolled(&self.path, owner))?;
        let kept = kept_for(self.ctx, state, account, stored)?;
        if let (Kept::ParkedNow { label }, Some(slice)) = (&kept, &held.slice) {
            let parking = park_for(self.ctx, self.permit, state, owner, slice)?;
            self.warnings.extend(parking);
            self.partway = Partway::Parked {
                label: label.clone(),
            };
        }
        Ok(kept)
    }

    fn changed(&self) -> Error {
        Error::LeftLoginChanged {
            path: self.path.clone(),
        }
    }
}

/// Lets go of Claude Code's write lock, saying whether it stopped being Pitboard's meanwhile.
fn let_go(writing: Option<lock::Guard>) -> bool {
    writing.as_ref().is_some_and(lock::Guard::compromised)
}

/// What the file behind the store in use holds, with the store it is in, or `None` where
/// nothing is behind that store.
fn read_left(live: &provider::LiveStore) -> Result<Option<(&dyn RawStore, String)>> {
    let Some(file) = store::behind(&live.chain, &live.service)? else {
        return Ok(None);
    };
    Ok(file.read(&live.service)?.map(|raw| (file, raw)))
}

/// The store the file is in, where it still holds `raw` byte for byte.
fn unchanged<'a>(live: &'a provider::LiveStore, raw: &str) -> Result<Option<&'a dyn RawStore>> {
    Ok(read_left(live)?
        .filter(|(_, now)| now == raw)
        .map(|(file, _)| file))
}

/// Whether Claude Code has still stored `judged`, the login stored that the file's was told
/// by.
fn still_stored(live: &provider::LiveStore, judged: Option<&Value>) -> Result<bool> {
    let tool = provider::of(TOOL);
    let login = |document: &Value| tool.slice(document).ok();
    let now = read_stored(TOOL, live)?.map(|(_, document)| document);
    Ok(now.as_ref().and_then(login) == judged.and_then(login))
}

/// What the file holds: the account's login in it, where there is one, and its other keys by
/// name.
struct Held {
    slice: Option<Value>,
    dropped: Vec<String>,
}

/// What the file holds, told as every warning and doctor tell it ([`live::holds_a_login`]):
/// a file that is not a JSON object holds no login and no key, and one with no token holds
/// keys and no login.
fn holding(raw: &str) -> Result<Held> {
    let document = live::document_in(raw);
    let slice = live::holds_a_login(&document)
        .then(|| provider::of(TOOL).slice(&document))
        .transpose()
        .map_err(|e| shape(TOOL, e))?;
    let login = slice.as_ref().and_then(Value::as_object);
    let dropped = document
        .as_object()
        .into_iter()
        .flat_map(serde_json::Map::keys)
        .filter(|key| !login.is_some_and(|login| login.contains_key(*key)))
        .cloned()
        .collect();
    Ok(Held { slice, dropped })
}

/// Whose the login in the file is, as far as can be told without renewing it.
enum Judged {
    /// Kept already, or nothing to keep.
    Kept(Kept),
    Owner(Owner),
    /// Its access token has expired, or Anthropic refuses it as expired: renewing it tells
    /// whose it is.
    Untold,
}

/// By its refresh token's fingerprint first: the login `stored` is, the one parked for an
/// account, where that park reads back, or the one whose owner the record holds. Otherwise
/// Anthropic is asked with its own access token, where that has not expired.
fn judge(
    ctx: &Context,
    state: &State,
    held: &Held,
    stored: Option<&Value>,
    path: &Path,
) -> Result<Judged> {
    let Some(slice) = &held.slice else {
        return Ok(Judged::Kept(Kept::NoLogin));
    };
    let tool = provider::of(TOOL);
    let login = tool.fingerprint(slice);
    // A login with no refresh token has no fingerprint, and matches nothing by it.
    if !login.is_empty() {
        if stored.is_some_and(|stored| tool.fingerprint(stored) == login) {
            let label = state
                .in_use(TOOL)
                .filter(|record| record.login == login)
                .and_then(|_| state.account_in_use(TOOL))
                .map(|account| state.typed(&account.key()));
            return Ok(Judged::Kept(Kept::Stored { label }));
        }
        if let Some((account, parked)) = state.accounts.iter().find_map(|account| {
            account
                .parked
                .as_ref()
                .filter(|park| account.provider() == TOOL && park.refresh_fingerprint == login)
                .map(|park| (account, park))
        }) {
            // A park that does not read back keeps nothing: the login is that account's, and
            // is parked in its place.
            return Ok(if reads_back(ctx, account, parked)? {
                Judged::Kept(Kept::AlreadyParked {
                    label: state.typed(&account.key()),
                })
            } else {
                Judged::Owner(account.owner())
            });
        }
        if let Some(owner) = state
            .in_use(TOOL)
            .filter(|record| record.login == login)
            .and_then(|record| record.owner.clone())
        {
            return Ok(Judged::Owner(owner));
        }
    }
    // Renewing it settles whose it is, where it has a refresh token to renew it with.
    let untold = || {
        if login.is_empty() {
            Judged::Kept(Kept::Refused)
        } else {
            Judged::Untold
        }
    };
    if tool
        .expiry(slice)
        .access_expires_at
        .is_some_and(|at| at <= ctx.now())
    {
        return Ok(untold());
    }
    match tool.identify(ctx, &provider::Credential::new(TOOL, slice.clone())) {
        Ok(found) => Ok(Judged::Owner(Owner::from(found))),
        // Expired earlier than its own expiry said, or with no access token at all.
        Err(ProviderError::Unauthorized | ProviderError::ShapeUnexpected { .. }) => Ok(untold()),
        Err(error) => Err(not_identified(path, error)),
    }
}

/// What putting away `held`'s login, `account`'s, comes to, where Claude Code has `stored`'s
/// login stored.
fn kept_for(
    ctx: &Context,
    state: &State,
    account: &Account,
    stored: Option<&Owner>,
) -> Result<Kept> {
    let label = state.typed(&account.key());
    Ok(if stored.is_some_and(|stored| account.owned_by(stored)) {
        Kept::SecondSignIn { label }
    } else if switchable(ctx, state, account)? {
        Kept::ParkKept { label }
    } else {
        Kept::ParkedNow { label }
    })
}

/// Whether `account` holds a parked login to keep in place of the one in the file: one that
/// reads back as recorded, has not expired, and is not a copy an earlier put-away made of the
/// file's login before it stopped. A session that signs in with the file renews the login
/// there, which spends such a copy, and nothing here can tell that it did.
fn switchable(ctx: &Context, state: &State, account: &Account) -> Result<bool> {
    match account
        .parked
        .as_ref()
        .filter(|parked| parked.restorable_at(ctx.now()) && !state.is_from_file(&parked.service))
    {
        Some(parked) => reads_back(ctx, account, parked),
        None => Ok(false),
    }
}

/// Whether `parked`, `account`'s park, reads back as recorded. A vault that cannot be read
/// says nothing either way, and stops it.
fn reads_back(ctx: &Context, account: &Account, parked: &state::Park) -> Result<bool> {
    match park::load(ctx, &account.key(), parked) {
        Ok(_) => Ok(true),
        Err(Error::Store(unreadable)) => Err(Error::Store(unreadable)),
        Err(_) => Ok(false),
    }
}

/// Parks `slice`, `owner`'s login, for that account, the way a sign-in of an account not in
/// use is parked, in place of any park it held, and records it. That the park copies the
/// file's login is written down before the copy is, so a run killed between the two leaves
/// no copy the next run would keep in place of what the file holds by then. What parking it
/// warned of.
fn park_for(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    owner: &Owner,
    slice: &Value,
) -> Result<Option<Warning>> {
    let account = state
        .account_of(TOOL, owner)
        .expect("parked only for an enrolled account");
    let (key, id, label) = (
        account.key(),
        account.id.clone(),
        state.typed(&account.key()),
    );
    let parking = park::price(
        ctx,
        TOOL,
        &label,
        &park::service_name(&id, ctx.now_millis()),
        slice,
    )?;
    let service = park::reserve(ctx, permit, &id)?;
    state.copying_from_file(&service);
    state::save(ctx, permit, state)?;
    let parked = park::store_at(ctx, permit, TOOL, &service, slice)?;
    fault::point("stow.park_stored");
    state.park(&key, parked);
    // Unrecorded, the park would be an item nothing refers to, never deleted.
    state::save(ctx, permit, state).inspect_err(|_| {
        let _ = store::vault_delete(ctx, permit, &service);
    })?;
    Ok(parking)
}

fn not_enrolled(path: &Path, owner: &Owner) -> Error {
    Error::LeftLoginNotEnrolled {
        path: path.to_path_buf(),
        email: owner.email.clone(),
        organization: owner.organization_uuid.clone(),
    }
}

/// Why nobody could say whose the login in the file is.
fn not_identified(path: &Path, error: ProviderError) -> Error {
    Error::LeftLoginUnidentified {
        path: path.to_path_buf(),
        cause: Cause::of_provider(&error),
        detail: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::harness::{
        Machine, NOW, Session, account, audit_lines, document, lock_dir, machine, oauth, owner,
        renews, renews_meanwhile, saves, state_file, write_target,
    };
    use super::super::settle;
    use super::*;
    use crate::api::scripted::{Asked, Trouble};
    use crate::service::Pitboard;
    use crate::store::memory::Fault;
    use serde_json::json;

    /// `contents` left in `.credentials.json` behind the keychain, as a sign-in where the
    /// keychain could not be read leaves it.
    fn leave(m: &Machine, contents: &Value) {
        left_file(m).plant(&m.service, &contents.to_string());
    }

    fn left_file(m: &Machine) -> std::sync::Arc<crate::store::memory::MemoryStore> {
        m.mem.file_at(live::credential_file(&m.ctx))
    }

    /// What the file holds now, as its fingerprint, or `None` where it is gone.
    fn in_the_file(m: &Machine) -> Option<String> {
        let held = left_file(m).peek(&m.service)?;
        let document: Value = serde_json::from_str(&held).expect("JSON");
        Some(provider::of(TOOL).fingerprint(&document))
    }

    /// `refresh`'s login, its access token expired a minute ago.
    fn lapsed(refresh: &str) -> Value {
        let mut login = document(refresh);
        login["claudeAiOauth"]["expiresAt"] = json!((NOW - 60) * 1000);
        login
    }

    /// `label`, enrolled with nothing parked.
    fn enrolled(m: &Machine, label: &str) {
        let mut state = state::load(&m.ctx).expect("state");
        state.accounts.push(account(label, label, None));
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
    }

    fn found(m: &Machine) -> Left {
        let state = state::load(&m.ctx).expect("state");
        find(&m.ctx, &state)
            .expect("a look")
            .expect("something left")
    }

    /// Putting away what is in the file now, confirmed as the look found it.
    fn put_away(m: &Machine) -> Result<(Stowed, Vec<Warning>)> {
        let seen = found(m).seen;
        put_away_seen(m, &seen)
    }

    fn put_away_seen(m: &Machine, seen: &str) -> Result<(Stowed, Vec<Warning>)> {
        let settled = settle(&m.ctx, Permit::for_a_test(), Some(TOOL))?.0;
        stow(settled, seen)
    }

    /// [`put_away`], with `elsewhere` writing at `point`, as another program writing at that
    /// moment would.
    fn put_away_while(
        m: &Machine,
        point: &'static str,
        elsewhere: impl FnOnce() + 'static,
    ) -> Result<(Stowed, Vec<Warning>)> {
        let seen = found(m).seen;
        crate::fault::meanwhile(point, elsewhere, || put_away_seen(m, &seen))
    }

    /// Plants `contents` in the file at the moment it is called, as Claude Code writes it.
    fn rewrites_the_file(m: &Machine, contents: &Value) -> impl FnOnce() + 'static {
        let (file, service, contents) = (left_file(m), m.service.clone(), contents.to_string());
        move || file.plant(&service, &contents)
    }

    /// Locks the keychain at the moment it is called, as one locks under a session started
    /// over SSH.
    fn locks_the_keychain(m: &Machine) -> impl FnOnce() + 'static {
        let (keychain, service) = m.live_store();
        move || keychain.fault(&service, Fault::Locked)
    }

    /// Plants `document` in the keychain at the moment it is called, as a sign-in writes it.
    fn signs_in(m: &Machine, document: &Value) -> impl FnOnce() + 'static {
        let (keychain, service) = (std::sync::Arc::clone(m.mem.live()), m.service.clone());
        let document = document.to_string();
        move || keychain.plant(&service, &document)
    }

    /// `refresh`'s login as a renewal an hour after [`document`]'s gives it: its access token
    /// expires an hour later.
    fn renewed_later(refresh: &str) -> Value {
        let mut login = document(refresh);
        login["claudeAiOauth"]["expiresAt"] = json!((NOW + 7200) * 1000);
        login
    }

    fn parked_fingerprint(m: &Machine, label: &str) -> Option<String> {
        let state = state::load(&m.ctx).expect("state");
        let parked = state.get(&m.key(label))?.parked.clone()?;
        Some(parked.refresh_fingerprint)
    }

    fn fingerprint(refresh: &str) -> String {
        store::fingerprint(refresh)
    }

    /// The login Claude Code has stored, left in the file too, goes with the file: the
    /// keychain holds it, known by its fingerprint, so nobody is asked.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn the_login_claude_code_has_stored_goes_with_the_file() {
        let m = machine("stow-stored");
        leave(&m, &document("here-refresh"));

        let (stowed, _) = put_away(&m).expect("put away");

        assert_eq!(
            stowed.kept,
            Kept::Stored {
                label: Some("here".into())
            }
        );
        assert_eq!(stowed.dropped, ["mcpOAuth"]);
        assert_eq!(in_the_file(&m), None);
        assert_eq!(m.live(), Some(document("here-refresh")));
        assert!(m.api.asked().is_empty(), "{:?}", m.api.asked());
    }

    /// A login Pitboard keeps parked, left in the file too, goes with the file, and the park
    /// stays as it was.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_login_pitboard_keeps_parked_goes_with_the_file() {
        let m = machine("stow-already-parked");
        leave(&m, &json!({"claudeAiOauth": oauth("there-refresh", 30)}));
        let vault = m.mem.vault().services();

        let (stowed, _) = put_away(&m).expect("put away");

        assert_eq!(
            stowed.kept,
            Kept::AlreadyParked {
                label: "there".into()
            }
        );
        assert_eq!(in_the_file(&m), None);
        assert_eq!(m.mem.vault().services(), vault);
        assert_eq!(
            parked_fingerprint(&m, "there"),
            Some(fingerprint("there-refresh"))
        );
    }

    /// A park that does not read back keeps nothing, so the same login left in the file is
    /// parked in its place before the file goes: the park's record alone is not a login.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_login_whose_park_does_not_read_back_is_parked_in_its_place() {
        let m = machine("stow-park-gone");
        m.mem.vault().delete_everything();
        leave(&m, &json!({"claudeAiOauth": oauth("there-refresh", 30)}));
        let there = Kept::ParkedNow {
            label: "there".into(),
        };
        assert_eq!(found(&m).login, Foreseen::Kept(there.clone()));

        let (stowed, _) = put_away(&m).expect("put away");

        assert_eq!(stowed.kept, there);
        assert_eq!(in_the_file(&m), None);
        let state = state::load(&m.ctx).expect("state");
        let parked = state
            .get(&m.key("there"))
            .and_then(|a| a.parked.clone())
            .expect("parked");
        let held = park::load(&m.ctx, &m.key("there"), &parked).expect("it reads back");
        assert_eq!(held["claudeAiOauth"]["refreshToken"], "there-refresh");
        assert!(m.api.asked().is_empty(), "{:?}", m.api.asked());
    }

    /// Another login of the account in use is a second sign-in of it: the keychain holds a
    /// login of that account already, so this one is dropped with the file and nothing is
    /// parked.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn another_login_of_the_account_in_use_is_dropped_as_a_second_sign_in() {
        let m = machine("stow-second-sign-in");
        m.api.owned_by("access-here-again", owner("here"));
        leave(&m, &document("here-again"));

        let (stowed, _) = put_away(&m).expect("put away");

        assert_eq!(
            stowed.kept,
            Kept::SecondSignIn {
                label: "here".into()
            }
        );
        assert_eq!(in_the_file(&m), None);
        assert_eq!(m.live(), Some(document("here-refresh")));
        assert_eq!(parked_fingerprint(&m, "here"), None);
    }

    /// The login of an enrolled account Pitboard holds no login of is parked for it, and the
    /// file goes only once the park is recorded.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_login_of_an_account_with_nothing_parked_is_parked_before_the_file_goes() {
        let m = machine("stow-parks");
        enrolled(&m, "elsewhere");
        m.api
            .owned_by("access-elsewhere-refresh", owner("elsewhere"));
        leave(&m, &document("elsewhere-refresh"));

        let (stowed, _) = put_away(&m).expect("put away");

        assert_eq!(
            stowed.kept,
            Kept::ParkedNow {
                label: "elsewhere".into()
            }
        );
        assert_eq!(in_the_file(&m), None);
        assert_eq!(
            parked_fingerprint(&m, "elsewhere"),
            Some(fingerprint("elsewhere-refresh"))
        );
        let state = state::load(&m.ctx).expect("state");
        let parked = state
            .get(&m.key("elsewhere"))
            .and_then(|a| a.parked.clone())
            .expect("parked");
        let held = park::load(&m.ctx, &m.key("elsewhere"), &parked).expect("it reads back");
        assert_eq!(held["claudeAiOauth"]["refreshToken"], "elsewhere-refresh");
        assert!(
            held.get("mcpOAuth").is_none(),
            "the machine's keys are not the account's"
        );
    }

    /// An account whose park has expired holds no login it can be switched to, so the login
    /// in the file is parked in its place.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_login_of_an_account_whose_park_has_expired_takes_its_place() {
        let m = machine("stow-expired-park");
        let mut state = state::load(&m.ctx).expect("state");
        let expired = state
            .accounts
            .iter_mut()
            .find(|a| a.label == "there")
            .and_then(|a| a.parked.as_mut())
            .expect("there is parked");
        expired.refresh_expires_at = Some(NOW - 1);
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        m.api.owned_by("access-there-again", owner("there"));
        leave(&m, &document("there-again"));

        let (stowed, _) = put_away(&m).expect("put away");

        assert_eq!(
            stowed.kept,
            Kept::ParkedNow {
                label: "there".into()
            }
        );
        assert_eq!(
            parked_fingerprint(&m, "there"),
            Some(fingerprint("there-again"))
        );
        assert_eq!(in_the_file(&m), None);
    }

    /// An account holding a park it can be switched to keeps that park, and the other login
    /// of it goes with the file.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn an_account_with_a_park_it_can_switch_to_keeps_that_park() {
        let m = machine("stow-park-kept");
        m.api.owned_by("access-there-again", owner("there"));
        leave(&m, &document("there-again"));

        let (stowed, _) = put_away(&m).expect("put away");

        assert_eq!(
            stowed.kept,
            Kept::ParkKept {
                label: "there".into()
            }
        );
        assert_eq!(
            parked_fingerprint(&m, "there"),
            Some(fingerprint("there-refresh"))
        );
        assert_eq!(in_the_file(&m), None);
    }

    /// A park the account can be switched to is kept whichever login is the newer: a login of
    /// that account signed in to since goes with the file, and so does one whose access token
    /// had expired and was renewed to tell whose it is, which then expires later than any park.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_park_it_can_switch_to_is_kept_though_the_files_login_is_newer() {
        let kept = Kept::ParkKept {
            label: "there".into(),
        };
        let m = machine("stow-park-kept-newer");
        m.api.owned_by("access-there-again", owner("there"));
        leave(&m, &renewed_later("there-again"));
        let vault = m.mem.vault().services();
        assert_eq!(found(&m).login, Foreseen::Kept(kept.clone()));

        let (stowed, _) = put_away(&m).expect("put away");

        assert_eq!(stowed.kept, kept);
        assert_eq!(m.mem.vault().services(), vault);
        assert_eq!(
            parked_fingerprint(&m, "there"),
            Some(fingerprint("there-refresh"))
        );
        assert_eq!(in_the_file(&m), None);

        let m = machine("stow-park-kept-renewed");
        m.api.renews(
            "there-again",
            crate::api::Renewed {
                access_token: "access-there-renewed".into(),
                refresh_token: Some("there-renewed".into()),
                expires_in: 28_800,
                refresh_token_expires_in: Some(30 * 86_400),
                scopes: None,
                at: None,
            },
        );
        m.api.owned_by("access-there-renewed", owner("there"));
        leave(&m, &lapsed("there-again"));
        let vault = m.mem.vault().services();

        let (stowed, _) = put_away(&m).expect("put away");

        assert_eq!(stowed.kept, kept);
        assert_eq!(m.mem.vault().services(), vault);
        assert_eq!(
            parked_fingerprint(&m, "there"),
            Some(fingerprint("there-refresh"))
        );
        assert_eq!(in_the_file(&m), None);
    }

    /// The copy a put-away makes is left out only while the file it was copied from is there.
    /// Once the file has gone it is the account's park as any other, and another login of that
    /// account left in a file later goes with that file.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_login_parked_from_the_file_is_kept_once_that_file_has_gone() {
        let m = machine("stow-parked-then-kept");
        enrolled(&m, "elsewhere");
        m.api
            .owned_by("access-elsewhere-refresh", owner("elsewhere"));
        m.api.owned_by("access-elsewhere-again", owner("elsewhere"));
        leave(&m, &document("elsewhere-refresh"));
        let (stowed, _) = put_away(&m).expect("put away");
        assert_eq!(
            stowed.kept,
            Kept::ParkedNow {
                label: "elsewhere".into()
            }
        );
        assert!(state::load(&m.ctx).expect("state").from_file.is_empty());

        leave(&m, &renewed_later("elsewhere-again"));
        let (stowed, _) = put_away(&m).expect("put away");

        assert_eq!(
            stowed.kept,
            Kept::ParkKept {
                label: "elsewhere".into()
            }
        );
        assert_eq!(
            parked_fingerprint(&m, "elsewhere"),
            Some(fingerprint("elsewhere-refresh"))
        );
    }

    /// That a park is a copy of the file's login is written down before the copy is. A run
    /// killed before it recorded the copy leaves one the next change gives to the account,
    /// and a session that signs in with the file can renew the login there meanwhile, which
    /// spends the copy: the renewed login is parked in its place.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_copy_a_killed_run_never_recorded_is_not_kept_over_the_login_renewed_since() {
        let m = machine("stow-killed-unrecorded");
        enrolled(&m, "elsewhere");
        m.api
            .owned_by("access-elsewhere-refresh", owner("elsewhere"));
        leave(&m, &document("elsewhere-refresh"));
        let seen = found(&m).seen;
        let died = crate::fault::killing("stow.park_stored", || put_away_seen(&m, &seen));
        assert_eq!(died.unwrap_err(), "stow.park_stored");
        assert_eq!(parked_fingerprint(&m, "elsewhere"), None);

        m.api
            .renew_trouble("elsewhere-refresh", Trouble::InvalidGrant);
        m.api
            .owned_by("access-elsewhere-renewed", owner("elsewhere"));
        leave(&m, &renewed_later("elsewhere-renewed"));
        let (stowed, _) = put_away(&m).expect("put away again");

        assert_eq!(
            stowed.kept,
            Kept::ParkedNow {
                label: "elsewhere".into()
            }
        );
        assert_eq!(
            parked_fingerprint(&m, "elsewhere"),
            Some(fingerprint("elsewhere-renewed"))
        );
        assert_eq!(in_the_file(&m), None);
    }

    /// The login of an account nobody enrolled is refused, naming the account by its email
    /// and organisation, and nothing is deleted or parked.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn the_login_of_an_account_not_enrolled_is_refused_and_named() {
        let m = machine("stow-not-enrolled");
        m.api.owned_by("access-stranger-refresh", owner("stranger"));
        leave(&m, &document("stranger-refresh"));
        let vault = m.mem.vault().services();

        let left = found(&m);
        assert_eq!(
            left.login,
            Foreseen::NotEnrolled(owner("stranger")),
            "the look says so first"
        );
        assert_eq!(
            left.refusal().map(|refused| refused.code()),
            Some("left_login_not_enrolled")
        );
        let refused = put_away(&m).expect_err("refused");

        assert_eq!(refused.code(), "left_login_not_enrolled");
        let said = refused.to_string();
        assert!(
            said.contains("stranger@example.com")
                && said.contains("org-stranger")
                && said.contains("pitboard enroll <label> --sign-in")
                && said.contains("pitboard stow"),
            "{said}"
        );
        assert_eq!(in_the_file(&m), Some(fingerprint("stranger-refresh")));
        assert_eq!(m.mem.vault().services(), vault);
    }

    /// A file with no login in it holds nothing to keep, and goes. Its other keys are named.
    /// A file that is not a JSON object, empty or not, holds no login either, nor does one
    /// whose `claudeAiOauth` holds no token, as every warning and doctor tell it: the look,
    /// the warning and putting it away agree, and nothing is sent anywhere.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_file_with_no_login_in_it_is_deleted() {
        let mcp = json!({"mcpOAuth": {"some-server": {"token": "t"}}}).to_string();
        for (contents, dropped) in [
            ("{}", Vec::<&str>::new()),
            (mcp.as_str(), vec!["mcpOAuth"]),
            ("", Vec::new()),
            ("not json", Vec::new()),
            ("[]", Vec::new()),
            (r#"{"claudeAiOauth": {}}"#, vec!["claudeAiOauth"]),
        ] {
            let m = machine("stow-no-login");
            left_file(&m).plant(&m.service, contents);

            assert_eq!(
                provider::of(TOOL).behind(&m.ctx).map(|behind| behind.held),
                Some(provider::Held::NoLogin),
                "{contents:?}"
            );
            assert_eq!(
                found(&m).login,
                Foreseen::Kept(Kept::NoLogin),
                "{contents:?}"
            );
            let (stowed, _) = put_away(&m).expect("put away");

            assert_eq!(stowed.kept, Kept::NoLogin, "{contents:?}");
            assert_eq!(stowed.dropped, dropped, "{contents:?}");
            assert!(left_file(&m).peek(&m.service).is_none(), "{contents:?}");
            assert!(m.api.asked().is_empty());
        }
    }

    /// A login whose access token has expired is renewed before anything else, as a park is,
    /// and one whose refresh token Anthropic refuses for good was no longer valid: the file
    /// goes.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_login_anthropic_refuses_for_good_goes_with_the_file() {
        let m = machine("stow-refused");
        m.api.renew_trouble("gone-refresh", Trouble::InvalidGrant);
        leave(&m, &lapsed("gone-refresh"));
        assert_eq!(found(&m).login, Foreseen::Untold);
        assert!(m.api.asked().is_empty(), "an expired token is not sent");

        let (stowed, _) = put_away(&m).expect("put away");

        assert_eq!(stowed.kept, Kept::Refused);
        assert_eq!(in_the_file(&m), None);
    }

    /// The renewed login is written back to the file before anything is asked of it, so a
    /// refresh token the exchange rotated is never lost: refused as an account nobody
    /// enrolled, which says it renewed it, the file holds the renewed login, with the
    /// machine's keys kept.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn an_expired_login_is_renewed_and_written_back_before_anything_else() {
        let m = machine("stow-renewed");
        renews(&m, "left-refresh", "left-renewed");
        m.api.owned_by("access-left-renewed", owner("stranger"));
        leave(&m, &lapsed("left-refresh"));

        let refused = put_away(&m).expect_err("not enrolled");

        assert_eq!(refused.code(), "left_login_not_enrolled");
        assert!(
            refused.to_string().ends_with(
                "Pitboard deleted nothing, having renewed the login in the file and written it \
                 back there."
            ),
            "{refused}"
        );
        assert_eq!(in_the_file(&m), Some(fingerprint("left-renewed")));
        let held: Value =
            serde_json::from_str(&left_file(&m).peek(&m.service).expect("still there"))
                .expect("JSON");
        assert_eq!(held["mcpOAuth"], document("x")["mcpOAuth"]);

        enrolled(&m, "stranger");
        let (stowed, _) = put_away(&m).expect("put away once enrolled");
        assert_eq!(
            stowed.kept,
            Kept::ParkedNow {
                label: "stranger".into()
            }
        );
        assert_eq!(
            parked_fingerprint(&m, "stranger"),
            Some(fingerprint("left-renewed"))
        );
        assert_eq!(in_the_file(&m), None);
    }

    /// With Anthropic out of reach nobody can say whose the login is, so nothing changes:
    /// neither when it is asked about, nor when it needs renewing first.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn with_anthropic_out_of_reach_nothing_changes() {
        let m = machine("stow-offline");
        m.api
            .token_trouble("access-unknown-refresh", Trouble::Offline);
        leave(&m, &document("unknown-refresh"));
        let before = state_file(&m);

        let state = state::load(&m.ctx).expect("state");
        let looked = find(&m.ctx, &state).expect_err("nobody to ask");
        assert_eq!(looked.code(), "left_login_unidentified");
        assert_eq!(looked.cause(), Some(crate::error::Cause::Unreachable));
        let seen = store::fingerprint(&left_file(&m).peek(&m.service).expect("there"));
        let refused = put_away_seen(&m, &seen).expect_err("nobody to ask");
        assert_eq!(refused.code(), "left_login_unidentified");
        assert_eq!(in_the_file(&m), Some(fingerprint("unknown-refresh")));
        assert_eq!(state_file(&m), before);

        let m = machine("stow-offline-renewing");
        m.api.renew_trouble("lapsed-refresh", Trouble::Offline);
        leave(&m, &lapsed("lapsed-refresh"));
        let before = state_file(&m);
        let refused = put_away(&m).expect_err("nobody to renew it");
        assert_eq!(refused.code(), "left_login_unidentified");
        assert_eq!(in_the_file(&m), Some(fingerprint("lapsed-refresh")));
        assert_eq!(state_file(&m), before);
    }

    /// A keychain that cannot be read, or a file that cannot, says why, and nothing changes.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_locked_keychain_or_an_unreadable_file_changes_nothing() {
        let m = machine("stow-locked");
        m.api.owned_by("access-there-again", owner("there"));
        leave(&m, &document("there-again"));
        let seen = found(&m).seen;
        m.fault_live(Fault::Locked);
        let state = state::load(&m.ctx).expect("state");
        assert_eq!(
            find(&m.ctx, &state).expect_err("locked").code(),
            "credential_store_locked"
        );
        assert_eq!(
            put_away_seen(&m, &seen).expect_err("locked").code(),
            "credential_store_locked"
        );
        assert_eq!(in_the_file(&m), Some(fingerprint("there-again")));

        let m = machine("stow-unreadable");
        leave(&m, &document("there-again"));
        left_file(&m).fault(
            &m.service,
            Fault::UnreadableContents("permission denied".into()),
        );
        let state = state::load(&m.ctx).expect("state");
        assert_eq!(
            find(&m.ctx, &state).expect_err("unreadable").code(),
            "credential_store_unreadable"
        );
        left_file(&m).heal(&m.service);
        assert_eq!(in_the_file(&m), Some(fingerprint("there-again")));
    }

    /// The file is put away only while it holds what was confirmed: one that changed since,
    /// or went, is left as it is, and so is everything else.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_file_that_changed_since_it_was_confirmed_is_left_alone() {
        let m = machine("stow-changed");
        leave(&m, &document("here-refresh"));
        let seen = found(&m).seen;
        leave(&m, &document("here-again"));

        let refused = put_away_seen(&m, &seen).expect_err("it changed");

        assert_eq!(refused.code(), "left_login_changed");
        assert_eq!(in_the_file(&m), Some(fingerprint("here-again")));

        left_file(&m).delete_everything();
        assert_eq!(
            put_away_seen(&m, &seen).expect_err("it went").code(),
            "left_login_changed"
        );
    }

    /// Read again under Claude Code's lock, and again as it goes: a session renewing the
    /// login in the file while Pitboard waits for that lock, or a `/logout` that took none
    /// rewriting it before it goes, leaves the file as they wrote it, and nothing else changes.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_file_rewritten_meanwhile_is_left_as_it_was_written() {
        // Before Claude Code's lock, a login that would be parked; before the file goes, one
        // whose account keeps its park.
        for (point, label, left, parked) in [
            ("stow.identified", "elsewhere", "elsewhere-refresh", None),
            (
                "stow.kept",
                "there",
                "there-again",
                Some(fingerprint("there-refresh")),
            ),
        ] {
            let m = machine(&format!("stow-rewritten-{point}"));
            enrolled(&m, "elsewhere");
            m.api.owned_by(&format!("access-{left}"), owner(label));
            leave(&m, &document(left));
            let vault = m.mem.vault().services();

            let refused = put_away_while(&m, point, rewrites_the_file(&m, &document("new")))
                .expect_err("it changed");

            assert_eq!(refused.code(), "left_login_changed", "{point}");
            assert!(
                refused.to_string().ends_with("Pitboard changed nothing."),
                "{point}: {refused}"
            );
            assert_eq!(in_the_file(&m), Some(fingerprint("new")), "{point}");
            assert_eq!(m.mem.vault().services(), vault, "{point}");
            assert_eq!(parked_fingerprint(&m, label), parked, "{point}");
        }
    }

    /// The login Claude Code has stored, which the file's login was told to be, or told to be
    /// another login of the same account, is read again as the file goes: a sign-in that took
    /// Claude Code's lock first replaced it, so the file, which may hold the only copy of that
    /// account's login, stays.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_sign_in_before_the_file_goes_keeps_the_file_whose_login_it_counted_on() {
        for (name, left) in [
            ("stow-stored-replaced", "here-refresh"),
            ("stow-second-sign-in-replaced", "here-again"),
        ] {
            let m = machine(name);
            m.api.owned_by("access-here-again", owner("here"));
            m.api
                .owned_by("access-elsewhere-refresh", owner("elsewhere"));
            leave(&m, &document(left));

            let refused = put_away_while(
                &m,
                "stow.identified",
                signs_in(&m, &document("elsewhere-refresh")),
            )
            .expect_err("the login stored changed");

            assert_eq!(refused.code(), "stored_login_changed", "{name}");
            assert!(
                refused.to_string().ends_with("Pitboard changed nothing."),
                "{name}: {refused}"
            );
            assert_eq!(in_the_file(&m), Some(fingerprint(left)), "{name}");
            assert_eq!(m.live(), Some(document("elsewhere-refresh")), "{name}");
            assert_eq!(parked_fingerprint(&m, "here"), None, "{name}");
        }
    }

    /// Whatever stops it after the login was renewed and written back, or parked, says so,
    /// its own refusals and a keychain that locks alike: the file holds the renewed login, or
    /// the park stays, and the file is not deleted.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_refusal_after_the_login_was_renewed_or_parked_says_so() {
        let m = machine("stow-renewed-then-offline");
        renews(&m, "left-refresh", "left-renewed");
        m.api.token_trouble("access-left-renewed", Trouble::Offline);
        leave(&m, &lapsed("left-refresh"));

        let refused = put_away(&m).expect_err("nobody to ask");

        assert_eq!(refused.code(), "left_login_unidentified");
        assert_eq!(refused.cause(), Some(Cause::Unreachable));
        assert!(
            refused.to_string().ends_with(
                "Pitboard deleted nothing, having renewed the login in the file and written it \
                 back there."
            ),
            "{refused}"
        );
        assert_eq!(in_the_file(&m), Some(fingerprint("left-renewed")));

        let m = machine("stow-parked-then-rewritten");
        enrolled(&m, "elsewhere");
        m.api
            .owned_by("access-elsewhere-refresh", owner("elsewhere"));
        leave(&m, &document("elsewhere-refresh"));

        let refused = put_away_while(
            &m,
            "stow.kept",
            rewrites_the_file(&m, &json!({"mcpOAuth": {}})),
        )
        .expect_err("it changed");

        assert_eq!(refused.code(), "left_login_changed");
        assert!(
            refused.to_string().ends_with(
                "Pitboard deleted nothing, having parked the login in the file for `elsewhere`."
            ),
            "{refused}"
        );
        assert_eq!(
            parked_fingerprint(&m, "elsewhere"),
            Some(fingerprint("elsewhere-refresh"))
        );
        assert!(left_file(&m).peek(&m.service).is_some(), "left as written");

        let m = machine("stow-parked-then-locked");
        enrolled(&m, "elsewhere");
        m.api
            .owned_by("access-elsewhere-refresh", owner("elsewhere"));
        leave(&m, &document("elsewhere-refresh"));

        let refused = put_away_while(&m, "stow.kept", locks_the_keychain(&m))
            .expect_err("the keychain is locked");

        assert_eq!(refused.code(), "credential_store_locked");
        assert!(
            refused.to_string().ends_with(
                "Pitboard deleted nothing, having parked the login in the file for `elsewhere`."
            ),
            "{refused}"
        );
        assert_eq!(
            parked_fingerprint(&m, "elsewhere"),
            Some(fingerprint("elsewhere-refresh"))
        );
        assert_eq!(in_the_file(&m), Some(fingerprint("elsewhere-refresh")));
    }

    /// `elsewhere` enrolled with nothing parked, and its login left in the file with its
    /// access token expired, which Anthropic renews.
    fn lapsed_login_left(name: &str) -> Machine {
        let m = machine(name);
        enrolled(&m, "elsewhere");
        renews(&m, "left-refresh", "left-renewed");
        m.api.owned_by("access-left-renewed", owner("elsewhere"));
        leave(&m, &lapsed("left-refresh"));
        m
    }

    /// Takes Claude Code's write lock at the moment it is called, as a session writing its
    /// credentials does, last touched `ago` before.
    fn takes_the_write_lock(m: &Machine, ago: std::time::Duration) -> impl FnOnce() + 'static {
        let writing = lock_dir(&write_target(m));
        move || {
            let parent = writing.parent().expect("the storage directory");
            std::fs::create_dir_all(parent).expect("the storage directory is made");
            std::fs::create_dir(&writing).expect("the write lock is free");
            let touched = std::time::SystemTime::now() - ago;
            crate::host::fs::touch_dir(Permit::for_a_test(), &writing, touched)
                .expect("the write lock is touched");
        }
    }

    fn renewed(m: &Machine) -> bool {
        m.api.asked().contains(&Asked::Renew("left-refresh".into()))
    }

    /// The renewed login is saved under Claude Code's write lock, and the exchange spends the
    /// refresh token the file holds. So that lock is taken once before the token is sent:
    /// held by a session then, nothing is sent, and nothing has changed.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_write_lock_held_before_a_renewal_stops_it_with_the_refresh_token_unsent() {
        let m = lapsed_login_left("stow-write-lock-before");
        let writing = lock_dir(&write_target(&m));

        let refused = put_away_while(
            &m,
            "stow.identified",
            takes_the_write_lock(&m, std::time::Duration::ZERO),
        )
        .expect_err("Claude Code is writing");

        assert_eq!(refused.code(), "switch_in_progress");
        assert!(
            refused.to_string().ends_with("Pitboard changed nothing."),
            "{refused}"
        );
        assert!(!renewed(&m), "{:?}", m.api.asked());
        assert_eq!(in_the_file(&m), Some(fingerprint("left-refresh")));

        std::fs::remove_dir(&writing).expect("the session lets go");
        let (stowed, _) = put_away(&m).expect("put away");
        assert_eq!(
            stowed.kept,
            Kept::ParkedNow {
                label: "elsewhere".into()
            }
        );
        assert_eq!(
            parked_fingerprint(&m, "elsewhere"),
            Some(fingerprint("left-renewed"))
        );
    }

    /// Once the refresh token is spent, giving up on Claude Code's write lock would drop the
    /// only login left of that grant. So it is waited for longer than a change waits: here a
    /// lock a killed session left 7 seconds ago, which nobody may take for 8 more.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_write_lock_held_once_the_refresh_token_is_spent_is_waited_for() {
        let m = lapsed_login_left("stow-write-lock-after");

        let (stowed, _) = put_away_while(
            &m,
            "stow.exchanged",
            takes_the_write_lock(&m, std::time::Duration::from_secs(7)),
        )
        .expect("put away");

        assert_eq!(
            stowed.kept,
            Kept::ParkedNow {
                label: "elsewhere".into()
            }
        );
        assert_eq!(
            parked_fingerprint(&m, "elsewhere"),
            Some(fingerprint("left-renewed"))
        );
        assert_eq!(in_the_file(&m), None);
    }

    /// A write lock that never comes free is not waited on for ever, nor is the renewed login
    /// dropped over it: the login is saved without the lock, as Claude Code writes on once
    /// its own is lost. Here the lock is a file, which taking it over fails on at once.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_write_lock_never_let_go_of_does_not_cost_the_renewed_login() {
        let m = lapsed_login_left("stow-write-lock-stuck");
        let stuck = lock_dir(&write_target(&m));

        let refused = put_away_while(&m, "stow.exchanged", move || {
            let parent = stuck.parent().expect("the storage directory");
            std::fs::create_dir_all(parent).expect("the storage directory is made");
            let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
            std::fs::File::create(&stuck)
                .and_then(|lock| lock.set_modified(long_ago))
                .expect("something in the lock's place");
        })
        .expect_err("the lock is never had, to delete the file under");

        assert_eq!(refused.code(), "switch_in_progress");
        assert!(
            refused.to_string().ends_with(
                "Pitboard deleted nothing, having parked the login in the file for `elsewhere`."
            ),
            "{refused}"
        );
        assert_eq!(in_the_file(&m), Some(fingerprint("left-renewed")));
        assert_eq!(
            parked_fingerprint(&m, "elsewhere"),
            Some(fingerprint("left-renewed"))
        );
    }

    /// The renewed login goes back into the file as Claude Code saves a renewal: wherever the
    /// file still holds the refresh token that was sent, whatever else it has come to hold. A
    /// session that cannot read the keychain saves an MCP token to the file, and that is kept
    /// beside the renewed login.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_file_whose_other_keys_were_written_meanwhile_takes_the_renewed_login() {
        let m = machine("stow-other-keys-written");
        renews(&m, "left-refresh", "left-renewed");
        m.api.owned_by("access-left-renewed", owner("stranger"));
        leave(&m, &lapsed("left-refresh"));
        let mut saved = lapsed("left-refresh");
        saved["mcpOAuth"] = json!({"another-server": {"token": "saved meanwhile"}});

        let refused = put_away_while(&m, "stow.exchanged", rewrites_the_file(&m, &saved))
            .expect_err("not enrolled");

        assert_eq!(refused.code(), "left_login_not_enrolled");
        assert!(
            refused.to_string().ends_with(
                "Pitboard deleted nothing, having renewed the login in the file and written it \
                 back there."
            ),
            "{refused}"
        );
        let held: Value =
            serde_json::from_str(&left_file(&m).peek(&m.service).expect("still there"))
                .expect("JSON");
        assert_eq!(held["claudeAiOauth"]["refreshToken"], "left-renewed");
        assert_eq!(held["mcpOAuth"], saved["mcpOAuth"]);
    }

    /// The renewed login is written into the file itself, which a keychain that locks while
    /// Anthropic answers does not stand in the way of: what stops then says the login was
    /// kept, and putting away again finishes.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_keychain_that_locks_while_the_login_is_renewed_does_not_lose_it() {
        let m = lapsed_login_left("stow-locks-while-renewing");

        let refused = put_away_while(&m, "stow.exchanged", locks_the_keychain(&m))
            .expect_err("the keychain is locked");

        assert_eq!(refused.code(), "credential_store_locked");
        assert!(
            refused.to_string().ends_with(
                "Pitboard deleted nothing, having parked the login in the file for `elsewhere`."
            ),
            "{refused}"
        );
        assert_eq!(in_the_file(&m), Some(fingerprint("left-renewed")));

        m.mem.live().heal(&m.service);
        let (stowed, _) = put_away(&m).expect("put away");
        assert_eq!(
            stowed.kept,
            Kept::AlreadyParked {
                label: "elsewhere".into()
            }
        );
        assert_eq!(in_the_file(&m), None);
    }

    /// Where the renewed login cannot go back into the file, a sign-in having replaced the
    /// login there or the write failing, what stops it says that login is spent, never that
    /// nothing changed.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_renewed_login_that_cannot_be_written_back_is_said_to_be_spent() {
        const SPENT: &str = "Pitboard deleted nothing. It renewed the login the file held and \
                             could not write the renewed login back there, so that login no \
                             longer works.";
        let m = lapsed_login_left("stow-signed-in-while-renewing");
        let refused = put_away_while(
            &m,
            "stow.exchanged",
            rewrites_the_file(&m, &document("signed-in")),
        )
        .expect_err("another login is in the file");
        assert_eq!(refused.code(), "left_login_changed");
        assert!(refused.to_string().ends_with(SPENT), "{refused}");
        assert_eq!(in_the_file(&m), Some(fingerprint("signed-in")));
        assert_eq!(parked_fingerprint(&m, "elsewhere"), None);

        let m = lapsed_login_left("stow-unwritable-while-renewing");
        let (file, service) = (left_file(&m), m.service.clone());
        let refused = put_away_while(&m, "stow.exchanged", move || {
            file.fault(&service, Fault::FailWrite("disk full".into()));
        })
        .expect_err("the file cannot be written");
        assert_eq!(refused.code(), "credential_write_failed");
        assert!(refused.to_string().ends_with(SPENT), "{refused}");
        assert_eq!(in_the_file(&m), Some(fingerprint("left-refresh")));
    }

    /// Nothing behind the keychain is nothing to put away, and nor is the file where it is
    /// the store Claude Code keeps its login in: on Linux always, and on macOS where the
    /// keychain holds none.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn nothing_behind_the_store_is_nothing_to_put_away() {
        let m = machine("stow-nothing");
        let state = state::load(&m.ctx).expect("state");
        assert_eq!(find(&m.ctx, &state).expect("a look"), None);

        leave(&m, &document("here-refresh"));
        m.mem.live().delete_everything();
        assert_eq!(find(&m.ctx, &state).expect("a look"), None);
    }

    /// The look says what putting the file away would do, and changes nothing: no file is
    /// written, nothing is parked, and an expired access token is not sent anywhere.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn the_look_says_what_putting_away_would_do_and_changes_nothing() {
        let m = machine("stow-look");
        enrolled(&m, "elsewhere");
        m.api
            .owned_by("access-elsewhere-refresh", owner("elsewhere"));
        leave(&m, &document("elsewhere-refresh"));
        let before = state_file(&m);
        let vault = m.mem.vault().services();

        let left = found(&m);

        assert_eq!(left.path, live::credential_file(&m.ctx));
        assert_eq!(
            left.login,
            Foreseen::Kept(Kept::ParkedNow {
                label: "elsewhere".into()
            })
        );
        assert_eq!(left.dropped, ["mcpOAuth"]);
        assert_eq!(
            left.seen,
            store::fingerprint(&document("elsewhere-refresh").to_string())
        );
        assert_eq!(state_file(&m), before);
        assert_eq!(m.mem.vault().services(), vault);
        assert_eq!(in_the_file(&m), Some(fingerprint("elsewhere-refresh")));
    }

    /// Killed anywhere, a login is left somewhere: in the file, renewed or not, in a park, or
    /// in both. Putting it away again finishes the job.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn killed_anywhere_the_login_is_left_somewhere_and_putting_away_again_finishes() {
        for point in ["stow.identified", "stow.renewed", "stow.kept"] {
            let m = machine(&format!("stow-killed-{point}"));
            enrolled(&m, "elsewhere");
            renews(&m, "left-refresh", "left-renewed");
            m.api.owned_by("access-left-renewed", owner("elsewhere"));
            leave(&m, &lapsed("left-refresh"));
            let seen = found(&m).seen;

            let died = crate::fault::killing(point, || put_away_seen(&m, &seen));
            assert_eq!(died.unwrap_err(), point);

            let renewed = Some(fingerprint("left-renewed"));
            let held = in_the_file(&m);
            assert!(
                held == Some(fingerprint("left-refresh"))
                    || held == renewed
                    || parked_fingerprint(&m, "elsewhere") == renewed,
                "{point}: the login is nowhere"
            );
            let (stowed, _) = put_away(&m).expect("put away again");
            assert!(
                matches!(
                    stowed.kept,
                    Kept::ParkedNow { .. } | Kept::AlreadyParked { .. }
                ),
                "{point}: {:?}",
                stowed.kept
            );
            assert_eq!(parked_fingerprint(&m, "elsewhere"), renewed, "{point}");
            assert_eq!(in_the_file(&m), None, "{point}");
        }
    }

    /// The front ends' way in: the activity log records it as `stow`, with the account the
    /// login was found to be and what became of it, and a refusal with its code. A read after
    /// it no longer finds the file.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn putting_away_is_recorded_in_the_activity_log() {
        let m = machine("stow-audit");
        enrolled(&m, "elsewhere");
        m.api
            .owned_by("access-elsewhere-refresh", owner("elsewhere"));
        leave(&m, &document("elsewhere-refresh"));
        let pitboard = Pitboard::new(m.ctx.clone());

        let left = pitboard.left_login().expect("a look").expect("left");
        let done = pitboard.stow(&left.seen).expect("put away");

        assert_eq!(
            done.value.kept,
            Kept::ParkedNow {
                label: "elsewhere".into()
            }
        );
        assert_eq!(
            audit_lines(&m, "stow"),
            [("elsewhere".to_string(), "parked".to_string())]
        );
        let read = pitboard.status(false).expect("a read");
        assert!(
            read.warnings.iter().all(|w| w.code() != "fallback_login"),
            "{:?}",
            read.warnings
        );

        m.api.owned_by("access-stranger-refresh", owner("stranger"));
        leave(&m, &document("stranger-refresh"));
        let left = pitboard.left_login().expect("a look").expect("left");
        let refused = pitboard.stow(&left.seen).expect_err("not enrolled");
        assert_eq!(refused.error.code(), "left_login_not_enrolled");
        assert_eq!(
            audit_lines(&m, "stow").last(),
            Some(&(String::new(), "left_login_not_enrolled".to_string()))
        );
    }

    /// A run that stops after parking the file's login, killed or refused, leaves that login
    /// in the park and in the file both. A session that signs in with the file renews it
    /// there, spending the park's refresh token. The park is written down as a copy of the
    /// file's login, so the next run parks the renewed login in its place, rather than keep
    /// the spent park and delete the only login that still works.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_login_renewed_in_the_file_after_a_run_stopped_takes_the_spent_parks_place() {
        for stop in ["killed", "locked"] {
            let m = machine(&format!("stow-renewed-after-{stop}"));
            enrolled(&m, "elsewhere");
            m.api
                .owned_by("access-elsewhere-refresh", owner("elsewhere"));
            leave(&m, &document("elsewhere-refresh"));
            let seen = found(&m).seen;
            if stop == "killed" {
                let died = crate::fault::killing("stow.kept", || put_away_seen(&m, &seen));
                assert_eq!(died.unwrap_err(), "stow.kept");
            } else {
                let refused = crate::fault::meanwhile("stow.kept", locks_the_keychain(&m), || {
                    put_away_seen(&m, &seen)
                })
                .expect_err("the keychain is locked");
                assert_eq!(refused.code(), "credential_store_locked");
                m.mem.live().heal(&m.service);
            }
            assert_eq!(
                parked_fingerprint(&m, "elsewhere"),
                Some(fingerprint("elsewhere-refresh")),
                "{stop}"
            );
            assert_eq!(
                in_the_file(&m),
                Some(fingerprint("elsewhere-refresh")),
                "{stop}"
            );

            m.api
                .renew_trouble("elsewhere-refresh", Trouble::InvalidGrant);
            m.api
                .owned_by("access-elsewhere-renewed", owner("elsewhere"));
            leave(&m, &renewed_later("elsewhere-renewed"));
            let (stowed, _) = put_away(&m).expect("put away again");

            assert_eq!(
                stowed.kept,
                Kept::ParkedNow {
                    label: "elsewhere".into()
                },
                "{stop}"
            );
            assert_eq!(
                parked_fingerprint(&m, "elsewhere"),
                Some(fingerprint("elsewhere-renewed")),
                "{stop}"
            );
            assert_eq!(in_the_file(&m), None, "{stop}");
            let state = state::load(&m.ctx).expect("state");
            let parked = state
                .get(&m.key("elsewhere"))
                .and_then(|a| a.parked.clone())
                .expect("parked");
            let held = park::load(&m.ctx, &m.key("elsewhere"), &parked).expect("it reads back");
            assert_eq!(
                held["claudeAiOauth"]["refreshToken"], "elsewhere-renewed",
                "{stop}"
            );
        }
    }

    /// A session renewing the login in the file takes Claude Code's refresh lock before it
    /// sends the refresh token, and saves the renewed login only where the file still holds
    /// the token it sent (the register's `refresh_lock`). Putting the file away holds that
    /// lock from the file's last reading until it is gone, so a session renewing meanwhile
    /// waits: the login Pitboard parks, renewed by Pitboard first or as it was, is never one a
    /// session spent with its renewal lost when the file went.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_session_renewing_the_login_in_the_file_meanwhile_waits_for_it() {
        for (point, left, parked) in [
            ("stow.renewed", lapsed("left-refresh"), "left-renewed"),
            ("stow.kept", document("left-refresh"), "left-refresh"),
        ] {
            let m = machine(&format!("stow-session-{point}"));
            enrolled(&m, "elsewhere");
            renews(&m, "left-refresh", "left-renewed");
            m.api.owned_by("access-left-refresh", owner("elsewhere"));
            m.api.owned_by("access-left-renewed", owner("elsewhere"));
            leave(&m, &left);
            let session = Session::new();

            let done = put_away_while(&m, point, renews_meanwhile(&m, left_file(&m), &session));
            saves(&m, &left_file(&m), &session);

            assert!(
                session.waited.get(),
                "{point}: a session renewed the login while Pitboard counted on it"
            );
            let (stowed, _) = done.expect("put away");
            assert_eq!(
                stowed.kept,
                Kept::ParkedNow {
                    label: "elsewhere".into()
                },
                "{point}"
            );
            assert_eq!(
                parked_fingerprint(&m, "elsewhere"),
                Some(fingerprint(parked)),
                "{point}"
            );
            assert_eq!(in_the_file(&m), None, "{point}");
            assert!(
                !lock_dir(&crate::provider::claude::paths::refresh_lock(&m.ctx)).exists(),
                "{point}: the refresh lock is let go of"
            );
        }

        // A session that renewed before Pitboard took the lock has saved what it renewed to by
        // then, so the file has changed since it was confirmed, and stays as it saved it.
        let m = machine("stow-session-before");
        enrolled(&m, "elsewhere");
        m.api.owned_by("access-left-refresh", owner("elsewhere"));
        leave(&m, &document("left-refresh"));
        let session = Session::new();

        let refused = put_away_while(
            &m,
            "stow.identified",
            renews_meanwhile(&m, left_file(&m), &session),
        )
        .expect_err("it changed");

        assert!(!session.waited.get());
        assert_eq!(refused.code(), "left_login_changed");
        assert_eq!(
            in_the_file(&m),
            Some(fingerprint("left-refresh-by-a-session"))
        );
        assert_eq!(parked_fingerprint(&m, "elsewhere"), None);
    }

    /// A login of an account nobody enrolled is named from what Anthropic said of it alone:
    /// whose the login stored is matters only for an enrolled account, so it is not asked,
    /// and Anthropic out of reach for it does not stand in the way.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_login_of_an_account_not_enrolled_is_named_without_asking_whose_the_login_stored_is() {
        let m = machine("stow-not-enrolled-stored-unknown");
        m.sign_in(&document("here-renewed"));
        m.api.token_trouble("access-here-renewed", Trouble::Offline);
        m.api.owned_by("access-stranger-refresh", owner("stranger"));
        leave(&m, &document("stranger-refresh"));

        let left = found(&m);
        let refused = put_away_seen(&m, &left.seen).expect_err("not enrolled");

        assert_eq!(left.login, Foreseen::NotEnrolled(owner("stranger")));
        assert_eq!(refused.code(), "left_login_not_enrolled");
        assert!(
            m.api
                .asked()
                .iter()
                .all(|asked| *asked != Asked::Owner("access-here-renewed".into())),
            "{:?}",
            m.api.asked()
        );
        assert_eq!(in_the_file(&m), Some(fingerprint("stranger-refresh")));
    }

    /// A login with no refresh token whose access token has expired can never be used again:
    /// it is dead, and goes with the file, with nothing sent anywhere.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_login_with_no_refresh_token_left_to_expire_is_dead() {
        let m = machine("stow-no-refresh-token");
        leave(
            &m,
            &json!({"claudeAiOauth": {
                "accessToken": "access-without-a-refresh-token",
                "expiresAt": (NOW - 60) * 1000,
            }}),
        );

        assert_eq!(found(&m).login, Foreseen::Kept(Kept::Refused));
        let (stowed, _) = put_away(&m).expect("put away");

        assert_eq!(stowed.kept, Kept::Refused);
        assert_eq!(in_the_file(&m), None);
        assert!(m.api.asked().is_empty(), "{:?}", m.api.asked());
    }

    /// Settled as any change of Claude Code's login is but for the file, which is what it puts
    /// away: something that signs Claude Code in some other way is said beside it, since
    /// sessions then follow no switch, file or no file.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn putting_away_says_what_signs_claude_code_in_some_other_way() {
        let m = machine("stow-overridden");
        let config = m.ctx_home().join(".claude");
        std::fs::create_dir_all(&config).expect("a config dir");
        std::fs::write(
            config.join("settings.json"),
            json!({"apiKeyHelper": "/usr/local/bin/get-key"}).to_string(),
        )
        .expect("settings");
        leave(&m, &document("here-refresh"));
        let pitboard = Pitboard::new(m.ctx.clone());

        let left = pitboard.left_login().expect("a look").expect("left");
        let done = pitboard.stow(&left.seen).expect("put away");

        let codes: Vec<&str> = done.warnings.iter().map(Warning::code).collect();
        assert_eq!(codes, ["auth_overridden"]);
        assert_eq!(in_the_file(&m), None);
    }
}
