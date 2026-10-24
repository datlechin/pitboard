// The two halves of a move, each recorded before its first rename and committed by one save.
// A login leaves keys first, then the jar, then the rest, and arrives in the reverse order.

use super::super::identify::{self, Noticed};
use super::super::{Error, Result, enrolled, purge};
use super::{midway, quiet, settle};
use crate::context::Context;
use crate::error::Then;
use crate::fault;
use crate::in_use::InUse;
use crate::permit::Permit;
use crate::provider::folder::{AppSettings, Folder, Keys, SignedIn, Unread};
use crate::state::{self, Aside, Awaited, Half, Key, Moving, Park, ParkKind, SetAside, State};
use crate::store::folder::{self as parks, Manifest};
use crate::warning::Warning;
use std::collections::BTreeMap;
use std::path::Path;

// A step of a move after its record: the failure, said for the error that wraps it.
pub(super) type Step<T = ()> = std::result::Result<T, String>;

pub(super) fn jar(app: &dyn Folder) -> &'static str {
    app.items()[0]
}

// The jar and what its store keeps beside it under its name, as `Cookies-journal`.
pub(super) fn of_the_jar(app: &dyn Folder, item: &str) -> bool {
    item == jar(app)
        || item
            .strip_prefix(jar(app))
            .is_some_and(|rest| rest.starts_with('-'))
}

pub(super) fn exists(path: &Path) -> Step<bool> {
    super::found(path).map_err(|e| not_looked_at(path, e))
}

pub(super) fn not_looked_at(path: &Path, why: impl std::fmt::Display) -> String {
    format!("{} could not be looked at: {why}", path.display())
}

pub(super) fn settings(app: &dyn Folder, root: &Path) -> Step<AppSettings> {
    let name = app.app_name();
    app.settings(root).map_err(|refused| match refused {
        Unread::Unknown { what, .. } => format!("{name}'s data folder {what}"),
        Unread::Unreadable(detail) => format!("{name}'s settings could not be read: {detail}"),
    })
}

// Read again just before each write, so the write is refused where the file changed since.
pub(super) fn put_keys(app: &dyn Folder, permit: Permit, root: &Path, keys: &Keys) -> Step {
    let read = settings(app, root)?;
    let name = app.app_name();
    app.write_settings(permit, root, &read, keys)
        .map_err(|refused| match refused {
            Unread::Unknown { what, .. } => format!("{name}'s data folder {what}"),
            Unread::Unreadable(detail) => {
                format!("{name}'s settings could not be written: {detail}")
            }
        })
}

// The account's items the folder holds, in the order a login leaves by.
pub(super) fn present(app: &dyn Folder, root: &Path) -> Step<Vec<String>> {
    let mut found = Vec::new();
    for item in app.items() {
        if exists(&root.join(item))? {
            found.push((*item).to_string());
        }
    }
    Ok(found)
}

fn moved(permit: Permit, from: &Path, to: &Path, item: &str) -> Step {
    parks::move_in(permit, from, to)
        .map(drop)
        .map_err(|e| format!("`{item}` could not be moved: {e}"))
}

// Out of the app's folder into `name`'s `items/`, whatever of `items` is there and not yet in.
pub(super) fn move_out(
    ctx: &Context,
    permit: Permit,
    app: &dyn Folder,
    root: &Path,
    name: &str,
    items: &[String],
    after: fn(bool) -> &'static str,
) -> Step {
    for item in items {
        let (from, to) = (root.join(item), parks::item(ctx, name, item));
        if !exists(&from)? || exists(&to)? {
            continue;
        }
        parks::make_parents(ctx, permit, name, item)
            .map_err(|e| format!("a folder for `{item}` could not be made: {e}"))?;
        moved(permit, &from, &to, item)?;
        fault::point(after(item == jar(app)));
    }
    Ok(())
}

pub(super) fn leave_point(jar: bool) -> &'static str {
    if jar {
        "leave.jar_out"
    } else {
        "leave.item_out"
    }
}

// The parts of the items' stores that no item names, by their paths from the root, in order.
pub(crate) fn unread_parts(app: &dyn Folder, root: &Path) -> std::io::Result<Vec<String>> {
    let mut folders = vec![""];
    folders.extend(
        app.items()
            .iter()
            .filter_map(|item| item.rsplit_once('/').map(|(parent, _)| parent)),
    );
    folders.dedup();
    let mut parts = Vec::new();
    for folder in folders {
        let entries = match std::fs::read_dir(root.join(folder)) {
            Err(e) if !folder.is_empty() && e.kind() == std::io::ErrorKind::NotFound => continue,
            entries => entries?,
        };
        for entry in entries {
            let name = entry?.file_name();
            let path = match folder {
                "" => name.to_string_lossy().into_owned(),
                _ => format!("{folder}/{}", name.to_string_lossy()),
            };
            if app.unread_part(&path) {
                parts.push(path);
            }
        }
    }
    parts.sort();
    Ok(parts)
}

// Asked only before a move starts.
fn layout(app: &dyn Folder, root: &Path) -> Result<()> {
    let tool = app.id();
    let parts = unread_parts(app, root).map_err(|e| Error::LoginFolderUnknown {
        tool,
        what: format!("could not be listed ({e})"),
        then: Then::Nothing,
    })?;
    match parts.first() {
        Some(part) => Err(Error::LoginFolderUnknown {
            tool,
            what: format!(
                "holds `{part}`, a part of the account's data this Pitboard has not read"
            ),
            then: Then::UpdatePitboard,
        }),
        None => Ok(()),
    }
}

// Each item there, and each folder between it and the root: no link, and on the root's volume.
fn placed(app: &dyn Folder, root: &Path) -> Result<()> {
    let tool = app.id();
    let refuse = |item: &str, how: &str| Error::LoginFolderUnknown {
        tool,
        what: format!("holds `{item}` {how}"),
        then: Then::Nothing,
    };
    let mut looked: Vec<String> = Vec::new();
    for item in app.items() {
        let mut path = String::new();
        for part in item.split('/') {
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(part);
            if looked.contains(&path) {
                continue;
            }
            looked.push(path.clone());
            let at = root.join(&path);
            let found = match std::fs::symlink_metadata(&at) {
                Ok(found) => found,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
                Err(e) => {
                    return Err(refuse(
                        &path,
                        &format!("where it could not be looked at ({e})"),
                    ));
                }
            };
            if found.file_type().is_symlink() {
                return Err(refuse(&path, "as a link"));
            }
            let (from, to) = (fault::volume_of(root), fault::volume_of(&at));
            if !crate::host::fs::same_volume(from, to).unwrap_or(false) {
                return Err(refuse(&path, "on another volume"));
            }
        }
    }
    Ok(())
}

// What every step of one move of `app`'s folder at `root` works with. `mac` is this Mac's id as
// one check read it, written into every manifest the move makes.
pub(super) struct Move<'a> {
    ctx: &'a Context,
    permit: Permit,
    app: &'static dyn Folder,
    root: &'a Path,
    mac: String,
}

impl<'a> Move<'a> {
    // A manifest that names no Mac could never be put back, so none is written.
    pub(super) fn here(
        ctx: &'a Context,
        permit: Permit,
        app: &'static dyn Folder,
        root: &'a Path,
    ) -> Result<Move<'a>> {
        let mac = ctx.this_machine();
        if mac == state::UNKNOWN_MACHINE {
            return Err(Error::MachineUnknown);
        }
        Ok(Move {
            ctx,
            permit,
            app,
            root,
            mac,
        })
    }
}

// Before anything is reserved or moved. A refusal changes nothing.
pub(super) fn may_start<'a>(
    ctx: &'a Context,
    permit: Permit,
    app: &'static dyn Folder,
    root: &'a Path,
) -> Result<Move<'a>> {
    layout(app, root)?;
    parks::ready(ctx, permit, root)?;
    placed(app, root)?;
    Move::here(ctx, permit, app, root)
}

pub(super) fn nobody(at: i64) -> InUse {
    InUse {
        owner: None,
        login: String::new(),
        known_at: at,
        named: None,
    }
}

pub(super) fn slot(ctx: &Context, app: &dyn Folder) -> String {
    crate::provider::slot(ctx, app.id()).unwrap_or_default()
}

pub(super) struct Leaving<'a> {
    pub key: &'a Key,
    pub session: &'a SignedIn,
    // the sign-in this leave is for
    pub awaited: Option<Awaited>,
}

// What a half that failed after its record came to, once settled at once under the same locks.
// With the app seen open the record stays, for the next change to settle with it closed.
fn after_failure(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    app: &'static dyn Folder,
    undone: impl FnOnce(String) -> Error,
    label: &str,
    detail: String,
) -> Result<Vec<Warning>> {
    midway(ctx, app, Some(label.to_string()))?;
    match settle::settle(ctx, permit, state, app) {
        Ok(settled) => {
            let finished = settled.iter().any(|warning| {
                matches!(warning, Warning::MoveSettled(moved) if moved.ended == settle::Ended::Finished)
            });
            // The move's own end is the outcome or the refusal; what else settling did is said.
            let rest = settled
                .into_iter()
                .filter(|warning| !matches!(warning, Warning::MoveSettled(_)))
                .collect();
            if finished {
                Ok(rest)
            } else {
                Err(undone(detail).after(rest))
            }
        }
        Err(_) => Err(Error::MoveStopped {
            label: label.to_string(),
            detail,
        }),
    }
}

// The leave of the account in use: the park it holds afterwards, and what to say.
pub(super) fn leave(
    mover: &Move<'_>,
    state: &mut State,
    leaving: Leaving<'_>,
) -> Result<(Park, Vec<Warning>)> {
    let Move {
        ctx,
        permit,
        app,
        root,
        ..
    } = *mover;
    let tool = app.id();
    let account = enrolled(state, leaving.key)?;
    let label = state.typed(leaving.key);
    let name = crate::park::reserve_folder(ctx, permit, state, &account.id)?;
    quiet(ctx, app)?;
    let refused = |detail: String| Error::LoginFolderUnknown {
        tool,
        what: format!("could not be read ({detail})"),
        then: Then::Nothing,
    };
    let keys = settings(app, root).map_err(refused)?.account_keys(app);
    let moving = Moving {
        half: Half::Leave,
        started_at: ctx.now(),
        id: account.id.clone(),
        label: leaving.key.label.clone(),
        park: name,
        login: leaving.session.login.clone(),
        items: present(app, root).map_err(refused)?,
        keys: keys.digests(),
        asides: BTreeMap::new(),
        slot: slot(ctx, app),
        awaited: leaving.awaited,
    };
    state.moving.insert(tool.code().to_string(), moving.clone());
    state::save(ctx, permit, state)?;
    fault::point("leave.recorded");

    let manifest = Manifest {
        v: parks::VERSION,
        tool: tool.code().to_string(),
        account: Some(account.id.clone()),
        named: Some(account.account_uuid.clone()),
        login: Some(moving.login.clone()),
        session_expires_at: leaving.session.expires_at,
        parked_at: ctx.now(),
        machine: mover.mac.clone(),
        items: moving.items.clone(),
        keys: moving.keys.clone(),
        left_in_folder: Vec::new(),
    };
    let forward = || -> Step<Vec<String>> {
        parks::make(ctx, permit, &moving.park)
            .map_err(|e| format!("its folder under `parks` could not be made: {e}"))?;
        parks::write_manifest(ctx, permit, &moving.park, &manifest)
            .and_then(|()| parks::write_keys(ctx, permit, &moving.park, &keys.to_json()))
            .map_err(|e| format!("its record under `parks` could not be written: {e}"))?;
        fault::point("leave.keys_kept");
        put_keys(app, permit, root, &Keys::none())?;
        fault::point("leave.keys_out");
        move_out(
            ctx,
            permit,
            app,
            root,
            &moving.park,
            &moving.items,
            leave_point,
        )?;
        let gone = verify_parked(ctx, permit, app, root, &moving, true)?;
        fault::point("leave.verified");
        Ok(gone)
    };
    let mut warnings = match forward() {
        Ok(gone) => gone_warnings(tool, &label, gone),
        Err(detail) => {
            let undone = |detail| Error::MoveUndone {
                label: label.clone(),
                detail,
            };
            let said = after_failure(ctx, permit, state, app, undone, &label, detail)?;
            let park = state
                .get(leaving.key)
                .and_then(|a| a.parked.clone())
                .ok_or(Error::ParkedCredentialMissing {
                    label: label.clone(),
                })?;
            return Ok((park, said));
        }
    };
    midway(ctx, app, Some(label.clone()))?;
    let (park, said) = commit_leave(ctx, permit, state, app, &moving)?;
    warnings.extend(said);
    warnings.extend(superseded_by(ctx, state, tool, leaving.key, &park));
    Ok((park, warnings))
}

// What `State::park` set aside for the park just recorded, said.
fn superseded_by(
    ctx: &Context,
    state: &State,
    tool: crate::provider::ProviderId,
    key: &Key,
    park: &Park,
) -> Vec<Warning> {
    let Some(account) = state.get(key) else {
        return Vec::new();
    };
    state
        .aside_of(&account.id)
        .filter(|aside| aside.why == SetAside::Superseded && aside.at == park.parked_at)
        .map(|aside| Warning::SetAside {
            tool,
            path: parks::at(ctx, &aside.name),
            label: Some(state.typed(key)),
            why: SetAside::Superseded,
        })
        .collect()
}

pub(super) fn gone_warnings(
    tool: crate::provider::ProviderId,
    label: &str,
    gone: Vec<String>,
) -> Vec<Warning> {
    gone.into_iter()
        .map(|item| Warning::ItemGone {
            tool,
            label: label.to_string(),
            item,
        })
        .collect()
}

// The record's park filed in `state.aside` under the account the record names.
pub(super) fn aside_of(
    ctx: &Context,
    app: &dyn Folder,
    moving: &Moving,
    why: SetAside,
    login: Option<String>,
) -> Aside {
    Aside {
        name: moving.park.clone(),
        tool: app.id(),
        at: ctx.now(),
        why,
        account: Some(moving.id.clone()),
        login,
    }
}

pub(super) fn folder_park(ctx: &Context, moving: &Moving, at: i64) -> Park {
    let session_expires_at = parks::manifest(ctx, &moving.park)
        .ok()
        .flatten()
        .and_then(|manifest| manifest.session_expires_at);
    Park {
        service: moving.park.clone(),
        kind: ParkKind::Folder,
        parked_at: at,
        login: moving.login.clone(),
        access_expires_at: None,
        refresh_expires_at: None,
        session_expires_at,
    }
}

// The leave's commit point: the park recorded, nobody in use, every aside indexed, the record
// gone. A give-up killed partway can have named an aside on a leave's record.
pub(super) fn commit_leave(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    app: &dyn Folder,
    moving: &Moving,
) -> Result<(Park, Vec<Warning>)> {
    let tool = app.id();
    forget_left(ctx, permit, state, tool)?;
    let now = ctx.now();
    let park = folder_park(ctx, moving, now);
    match state.by_id(tool, &moving.id).map(state::Account::key) {
        Some(key) => state.park(&key, park.clone()),
        // No account to hold it: kept, and `repair` gives it back once one is enrolled.
        None => state.aside.push(aside_of(
            ctx,
            app,
            moving,
            SetAside::Orphan,
            Some(moving.login.clone()),
        )),
    }
    state.identified(tool, nobody(now), now);
    let said = index_asides(ctx, state, app, moving);
    state.moving.remove(tool.code());
    if let Some(awaited) = &moving.awaited {
        state
            .awaiting
            .insert(tool.code().to_string(), awaited.clone());
    }
    state::save(ctx, permit, state)?;
    fault::point("leave.done");
    Ok((park, said))
}

// For a leave. `strict`: straight after the renames, the folder holds none of the login
// and the park's keys are the record's. Not strict: the app may have run since, so the park
// alone is judged. The items in neither place, which the move goes on without.
pub(super) fn verify_parked(
    ctx: &Context,
    permit: Permit,
    app: &dyn Folder,
    root: &Path,
    moving: &Moving,
    strict: bool,
) -> Step<Vec<String>> {
    let parked_jar = parks::item(ctx, &moving.park, jar(app));
    match app.session(ctx, &parked_jar) {
        Ok(Some(session)) if session.login == moving.login => {}
        Ok(_) => return Err("the parked cookie store does not hold the login that moved".into()),
        Err(_) => return Err("the parked cookie store could not be read".into()),
    }
    let mut gone = Vec::new();
    for item in moving.items.iter().filter(|item| *item != jar(app)) {
        let in_park = exists(&parks::item(ctx, &moving.park, item))?;
        let in_root = exists(&root.join(item))?;
        match (in_park, in_root) {
            (true, true) if strict => {
                return Err(format!("`{item}` is in both places after the move"));
            }
            (true, _) => {}
            (false, true) => {
                return Err(format!("`{item}` is still in {}'s folder", app.app_name()));
            }
            (false, false) => gone.push(item.clone()),
        }
    }
    if strict {
        if exists(&root.join(jar(app)))? {
            return Err(format!(
                "{}'s folder still holds a cookie store",
                app.app_name()
            ));
        }
        if kept_keys(ctx, &moving.park)?.digests() != moving.keys {
            return Err("the keys kept in the park are not the ones the settings held".into());
        }
        if !settings(app, root)?.account_keys(app).is_empty() {
            return Err(format!(
                "{}'s settings still hold the login's keys",
                app.app_name()
            ));
        }
    }
    hold_what_is_there(ctx, permit, &moving.park, &moving.items)?;
    Ok(gone)
}

// The manifest's `items` rewritten to what the folder holds. Its `login` is never rewritten.
pub(super) fn hold_what_is_there(
    ctx: &Context,
    permit: Permit,
    name: &str,
    listed: &[String],
) -> Step {
    let Some(mut manifest) = parks::manifest(ctx, name)? else {
        return Err("its manifest is missing".into());
    };
    let held = parks::held_items(ctx, name, listed)
        .map_err(|e| format!("the park could not be looked at: {e}"))?;
    if manifest.items != held {
        manifest.items = held;
        parks::write_manifest(ctx, permit, name, &manifest)
            .map_err(|e| format!("its manifest could not be written: {e}"))?;
    }
    Ok(())
}

pub(super) fn kept_keys(ctx: &Context, name: &str) -> Step<Keys> {
    park_keys(ctx, name).map_err(|e| format!("the keys kept in the park could not be read: {e}"))?
}

// The inner refusal is a `keys.json` that does not parse.
fn park_keys(ctx: &Context, name: &str) -> std::io::Result<Step<Keys>> {
    parks::keys(ctx, name).map(|read| read.map_err(|why| format!("the park's `keys.json` {why}")))
}

// A park that passed `check_park`, with what the arrive puts back.
pub(super) struct Checked {
    park: Park,
    manifest: Manifest,
    keys: Keys,
}

// The park's own checks, before any record, and for a switch before the leave that precedes it.
pub(super) fn check_park(mover: &Move<'_>, state: &State, key: &Key) -> Result<Checked> {
    let Move { ctx, app, .. } = *mover;
    let tool = app.id();
    let account = enrolled(state, key)?;
    let label = state.typed(key);
    let Some(park) = account.parked.filter(|park| park.kind == ParkKind::Folder) else {
        return Err(Error::NothingParked { tool, label });
    };
    let folder = parks::at(ctx, &park.service);
    // Only "not found" is missing, which asks for a sign-in: what Pitboard could not look at
    // is still there.
    match std::fs::symlink_metadata(&folder) {
        Ok(found) if found.is_dir() && !found.file_type().is_symlink() => {}
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(parks::unread(&label, &folder, e));
        }
        _ => return Err(Error::ParkedCredentialMissing { label }),
    }
    let corrupt = |detail: String| Error::ParkedCredentialCorrupt {
        label: label.clone(),
        detail,
    };
    let unseen = |path: &Path, e: std::io::Error, missing: String| {
        if e.kind() == std::io::ErrorKind::NotFound {
            corrupt(missing)
        } else {
            parks::unread(&label, path, e)
        }
    };
    let manifest_file = parks::manifest_file(ctx, &park.service);
    let manifest = parks::read_manifest(ctx, &park.service)
        .map_err(|e| unseen(&manifest_file, e, "its manifest is missing".into()))?
        .map_err(corrupt)?;
    if manifest.v != parks::VERSION
        || manifest.tool != tool.code()
        || manifest.account.as_deref() != Some(account.id.as_str())
        || manifest.named.as_deref() != Some(account.account_uuid.as_str())
        || manifest.login.as_deref() != Some(park.login.as_str())
    {
        return Err(corrupt("its manifest names another login".into()));
    }
    let name = app.app_name();
    if manifest.machine == state::UNKNOWN_MACHINE {
        return Err(corrupt(format!(
            "its record names no Mac, so Pitboard cannot tell whether {name} can read it here"
        )));
    }
    if manifest.machine != mover.mac {
        return Err(corrupt(format!(
            "it was parked on another Mac, and {name} cannot read it here where its key differs"
        )));
    }
    let cookies = parks::item(ctx, &park.service, jar(app));
    match app.session(ctx, &cookies) {
        Ok(Some(session)) if session.login == park.login => {}
        Ok(_) => {
            return Err(corrupt(
                "its cookie store holds another session, or none".into(),
            ));
        }
        Err(Unread::Unreadable(detail)) => return Err(parks::unread(&label, &cookies, detail)),
        Err(Unread::Unknown { .. }) => {
            return Err(corrupt("its cookie store could not be read".into()));
        }
    }
    for item in &manifest.items {
        let path = parks::item(ctx, &park.service, item);
        match std::fs::symlink_metadata(&path) {
            Ok(found) if !found.file_type().is_symlink() => {}
            Ok(_) => return Err(corrupt(format!("`{item}` is a link"))),
            Err(e) => return Err(unseen(&path, e, format!("`{item}` is missing from it"))),
        }
    }
    let keys_file = parks::keys_file(ctx, &park.service);
    let keys = park_keys(ctx, &park.service)
        .map_err(|e| unseen(&keys_file, e, "the park's `keys.json` is missing".into()))?
        .map_err(corrupt)?;
    let named = keys
        .get(app.naming_key())
        .and_then(|value| serde_json::from_str::<String>(value.get()).ok());
    if keys.digests() != manifest.keys || named.as_deref() != Some(account.account_uuid.as_str()) {
        return Err(corrupt("its keys are not the ones Pitboard kept".into()));
    }
    Ok(Checked {
        park,
        manifest,
        keys,
    })
}

// Against the folder as it is: each place is free, or is what `setting_aside` frees.
fn places_free(app: &dyn Folder, root: &Path, checked: &Checked, setting_aside: bool) -> Step {
    if setting_aside {
        return Ok(());
    }
    for item in &checked.manifest.items {
        if exists(&root.join(item))? {
            return Err(format!(
                "`{item}` is in the way in {}'s folder",
                app.app_name()
            ));
        }
    }
    Ok(())
}

// Each folder of the tool's parks and asides whose manifest lists items a give-up left in the
// app's folder, with that manifest.
fn listing_left(
    ctx: &Context,
    state: &State,
    tool: crate::provider::ProviderId,
) -> Vec<(String, Manifest)> {
    let parked = state
        .accounts
        .iter()
        .filter(|a| a.provider() == tool)
        .filter_map(|a| a.parked.as_ref())
        .filter(|park| park.kind == ParkKind::Folder)
        .map(|park| park.service.clone());
    let aside = state
        .aside
        .iter()
        .filter(|aside| aside.tool == tool)
        .map(|aside| aside.name.clone());
    parked
        .chain(aside)
        .filter_map(|name| {
            let manifest = parks::manifest(ctx, &name).ok().flatten()?;
            (!manifest.left_in_folder.is_empty()).then_some((name, manifest))
        })
        .collect()
}

// The account a give-up left items of in the app's folder.
fn left_behind(ctx: &Context, state: &State, tool: crate::provider::ProviderId) -> Option<String> {
    listing_left(ctx, state, tool)
        .into_iter()
        .find_map(|(_, manifest)| manifest.account)
}

// Once the app's folder's items are moved or filed under an account, they are that account's:
// no manifest lists them as a give-up's any more.
pub(super) fn forget_left(
    ctx: &Context,
    permit: Permit,
    state: &State,
    tool: crate::provider::ProviderId,
) -> Result<()> {
    for (name, mut manifest) in listing_left(ctx, state, tool) {
        manifest.left_in_folder.clear();
        parks::write_manifest(ctx, permit, &name, &manifest).map_err(|source| {
            Error::HomeUnwritable {
                path: parks::at(ctx, &name),
                source,
            }
        })?;
    }
    Ok(())
}

// An aside's record, written before anything moves into it: what the settings held, kept.
pub(super) fn keep_aside(
    mover: &Move<'_>,
    name: &str,
    account: Option<String>,
    items: Vec<String>,
    keys: &Keys,
) -> Step {
    let Move {
        ctx, permit, app, ..
    } = *mover;
    let manifest = Manifest {
        v: parks::VERSION,
        tool: app.id().code().to_string(),
        account,
        named: None,
        login: None,
        session_expires_at: None,
        parked_at: ctx.now(),
        machine: mover.mac.clone(),
        items,
        keys: keys.digests(),
        left_in_folder: Vec::new(),
    };
    parks::make(ctx, permit, name)
        .and_then(|_| parks::write_manifest(ctx, permit, name, &manifest))
        .and_then(|()| parks::write_keys(ctx, permit, name, &keys.to_json()))
        .map_err(|e| format!("a folder to set things aside in could not be made: {e}"))
}

// For an account whose park passed `check_park`. `from`: the account the leave before it parked.
// `noticed`: what `state` holds that changed outside Pitboard, written down once it is saved.
pub(super) fn arrive(
    mover: &Move<'_>,
    state: &mut State,
    key: &Key,
    checked: Checked,
    leftovers: bool,
    from: Option<String>,
    noticed: &[Noticed],
) -> Result<Vec<Warning>> {
    let Move {
        ctx,
        permit,
        app,
        root,
        ..
    } = *mover;
    let tool = app.id();
    let account = enrolled(state, key)?;
    let label = state.typed(key);
    let left_signed_out = |detail: String| Error::SwitchLeftSignedOut {
        from: from.clone(),
        to: label.clone(),
        detail,
    };
    // After a switch's leave committed, the app is signed out whatever stops its arrive.
    let before_record = |refused: Error| match (&from, refused) {
        (None, refused) => refused,
        (Some(_), Error::AppOpen { app, .. }) => left_signed_out(format!("{app} was opened")),
        (Some(_), Error::AppStateUnknown { app, why }) => {
            left_signed_out(format!("whether {app} is open could not be told: {why}"))
        }
        (Some(_), refused) => left_signed_out(refused.to_string()),
    };
    let aside = match leftovers {
        true => Some(crate::park::reserve_aside(ctx, permit, state)?),
        false => None,
    };
    quiet(ctx, app).map_err(before_record)?;
    places_free(app, root, &checked, leftovers).map_err(left_signed_out)?;
    let moving = Moving {
        half: Half::Arrive,
        started_at: ctx.now(),
        id: account.id.clone(),
        label: key.label.clone(),
        park: checked.park.service.clone(),
        login: checked.park.login.clone(),
        items: checked.manifest.items.clone(),
        keys: checked.manifest.keys.clone(),
        asides: aside
            .iter()
            .map(|name| (name.clone(), SetAside::InTheWay))
            .collect(),
        slot: slot(ctx, app),
        awaited: None,
    };
    state.moving.insert(tool.code().to_string(), moving.clone());
    state::save(ctx, permit, state).map_err(before_record)?;
    identify::write_down(ctx, permit, noticed);
    fault::point("arrive.recorded");

    let left = left_behind(ctx, state, tool);
    let forward = || -> Step<Vec<String>> {
        let held = settings(app, root)?.account_keys(app);
        if let Some(aside) = &aside {
            keep_aside(mover, aside, left.clone(), present(app, root)?, &held)?;
            fault::point("arrive.aside_kept");
        }
        if !held.is_empty() {
            if aside.is_none() {
                return Err(format!(
                    "{}'s settings hold keys and nothing was set aside for them",
                    app.app_name()
                ));
            }
            put_keys(app, permit, root, &Keys::none())?;
            fault::point("arrive.keys_cleared");
        }
        if let Some(aside) = &aside {
            let in_the_way = present(app, root)?;
            move_out(
                ctx,
                permit,
                app,
                root,
                aside,
                &in_the_way,
                |_| "arrive.item_aside",
            )?;
        }
        // What a give-up listed is in the aside, under the account its manifest names, or gone.
        forget_left(ctx, permit, state, tool).map_err(|e| e.to_string())?;
        let rest = moving.items.iter().filter(|item| *item != jar(app)).rev();
        for item in rest.chain(moving.items.iter().filter(|item| *item == jar(app))) {
            let (from, to) = (parks::item(ctx, &moving.park, item), root.join(item));
            if let Some(parent) = to.parent().filter(|parent| *parent != root) {
                crate::host::fs::create_private_dir(permit, parent)
                    .map_err(|e| format!("a folder for `{item}` could not be made: {e}"))?;
            }
            moved(permit, &from, &to, item)?;
            fault::point(if item == jar(app) {
                "arrive.jar_in"
            } else {
                "arrive.item_in"
            });
        }
        put_keys(app, permit, root, &checked.keys)?;
        fault::point("arrive.keys_in");
        let gone = verify_arrived(ctx, app, root, &moving, &account.account_uuid)?;
        fault::point("arrive.verified");
        Ok(gone)
    };
    let mut warnings = match forward() {
        Ok(gone) => gone_warnings(tool, &label, gone),
        Err(detail) => {
            return after_failure(ctx, permit, state, app, left_signed_out, &label, detail);
        }
    };
    midway(ctx, app, Some(label.clone()))?;
    warnings.extend(commit_arrive(ctx, permit, state, app, &moving, false)?);
    purge(ctx, permit, state);
    Ok(warnings)
}

// For an arrive: the folder holds the login whole, and the park none of it.
fn verify_arrived(
    ctx: &Context,
    app: &dyn Folder,
    root: &Path,
    moving: &Moving,
    uuid: &str,
) -> Step<Vec<String>> {
    let name = app.app_name();
    match app.session(ctx, &root.join(jar(app))) {
        Ok(Some(session)) if session.login == moving.login => {}
        Ok(_) => {
            return Err(format!(
                "{name}'s cookie store does not hold the login that moved"
            ));
        }
        Err(_) => return Err(format!("{name}'s cookie store could not be read")),
    }
    let read = settings(app, root)?;
    if read.account_keys(app).digests() != moving.keys || read.named(app).as_deref() != Some(uuid) {
        return Err(format!("{name}'s settings do not hold the login's keys"));
    }
    let mut gone = Vec::new();
    for item in moving.items.iter().filter(|item| *item != jar(app)) {
        let in_park = exists(&parks::item(ctx, &moving.park, item))?;
        let in_root = exists(&root.join(item))?;
        match (in_root, in_park) {
            (_, true) => return Err(format!("`{item}` is still in the park")),
            (true, false) => {}
            (false, false) => gone.push(item.clone()),
        }
    }
    Ok(gone)
}

// Every folder the record reserved for what it set aside, indexed where it was made.
pub(super) fn index_asides(
    ctx: &Context,
    state: &mut State,
    app: &dyn Folder,
    moving: &Moving,
) -> Vec<Warning> {
    let tool = app.id();
    let mut said = Vec::new();
    for (name, why) in &moving.asides {
        if !parks::at(ctx, name).is_dir() || state.aside.iter().any(|aside| aside.name == *name) {
            continue;
        }
        let account = parks::manifest(ctx, name)
            .ok()
            .flatten()
            .and_then(|manifest| manifest.account);
        let label = account
            .as_deref()
            .and_then(|id| state.by_id(tool, id))
            .map(|a| state.typed(&a.key()));
        state.aside.push(Aside {
            name: name.clone(),
            tool,
            at: ctx.now(),
            why: *why,
            account,
            login: None,
        });
        said.push(Warning::SetAside {
            tool,
            path: parks::at(ctx, name),
            label: label.filter(|_| *why != SetAside::InTheWay),
            why: *why,
        });
    }
    said
}

// The park's copy of the keys, which the app rewrote since, kept aside. It holds no session.
pub(super) fn keep_superseded(
    ctx: &Context,
    state: &mut State,
    app: &dyn Folder,
    moving: &Moving,
) -> Warning {
    let tool = app.id();
    state.keep_aside(aside_of(ctx, app, moving, SetAside::Superseded, None));
    Warning::KeysSetAside {
        tool,
        path: parks::at(ctx, &moving.park),
        label: state.by_id(tool, &moving.id).map(|a| state.typed(&a.key())),
    }
}

// The arrive's commit point. `superseded`: the app wrote its own keys since, so the park's
// copy differs and is kept aside.
pub(super) fn commit_arrive(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    app: &dyn Folder,
    moving: &Moving,
    superseded: bool,
) -> Result<Vec<Warning>> {
    let tool = app.id();
    let now = ctx.now();
    let account = state.by_id(tool, &moving.id).cloned();
    let mut said = Vec::new();
    if superseded {
        // The jar is back in the app's folder.
        said.push(keep_superseded(ctx, state, app, moving));
    } else {
        state.discard(&moving.park, ParkKind::Folder);
    }
    let owner = account.as_ref().map(state::Account::owner);
    let in_use = InUse {
        named: owner
            .as_ref()
            .and_then(|owner| crate::in_use::naming(tool, owner)),
        owner,
        login: moving.login.clone(),
        known_at: now,
    };
    state.identified(tool, in_use, now);
    said.extend(index_asides(ctx, state, app, moving));
    state.moving.remove(tool.code());
    state.awaiting.remove(tool.code());
    state::save(ctx, permit, state)?;
    fault::point("arrive.done");
    Ok(said)
}
