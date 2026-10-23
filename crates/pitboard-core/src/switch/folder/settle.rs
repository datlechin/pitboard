// What becomes of a move a run left recorded. Every fact is read from disk. Recovery never
// puts a jar or an item into the app's folder, and writes keys there only to undo a leave
// whose jar never left. A state no row names stops, and nothing changes.

use super::super::{Error, Result, purge, shape};
use super::moves::{self, Step};
use super::{midway, there};
use crate::context::Context;
use crate::error::Then;
use crate::fault;
use crate::provider::ProviderId;
use crate::provider::folder::{Folder, Keys, Unfit, Unread};
use crate::service::{Permit, Warning};
use crate::state::{self, Half, Key, Moving, ParkKind, SetAside, State};
use crate::store::folder as parks;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    Undone,
    Finished,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveSettled {
    pub tool: ProviderId,
    pub label: String,
    pub half: Half,
    pub ended: Ended,
}

#[derive(Debug)]
pub struct GivenUp {
    pub tool: ProviderId,
    pub label: String,
    pub parked: usize,
    pub set_aside: usize,
    // the account's items that stay in the app's folder
    pub left: Vec<String>,
    // the data folder the move was in, where this run reads another
    pub slot: Option<String>,
}

impl std::fmt::Display for GivenUp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&crate::words::given_up(self))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParkJar {
    None,
    Mine,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FolderJar {
    None,
    NoSession,
    Mine,
    Other,
}

// The settings' account keys against the record's, the first that holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeysAre {
    None,
    Mine,
    SignedOut,
    Named,
    Other,
}

// Where `places` found something it could not look at or read.
#[derive(Clone, Copy)]
enum Place {
    Folder,
    Park,
}

// What `places` could not look at or read: where, the item, its path and why.
type Unseen<'a> = &'a dyn Fn(Place, &str, &Path, String) -> Error;

// Where the login's jar and items are: all a give-up decides on.
struct Places {
    park_jar: ParkJar,
    // the session the park's jar holds, where it holds one
    park_login: Option<String>,
    folder_jar: FolderJar,
    // of the listed items but the jar
    in_root: Vec<String>,
    in_park: Vec<String>,
}

struct Facts {
    places: Places,
    keys: KeysAre,
    // `keys.json` holds the keys the record names
    kept_mine: bool,
}

enum Row {
    LeaveUntouched,
    LeaveKeysBack,
    LeaveKeysRewritten,
    LeaveOn { keys_in: bool },
    LeaveItemsOn,
    ArriveBack { jar: bool },
    ArriveRecorded { superseded: bool },
    ArriveBackKeysAside,
    Stop(String),
}

fn row(app: &dyn Folder, half: Half, facts: &Facts) -> Row {
    use FolderJar as F;
    use ParkJar as P;
    let name = app.app_name();
    let stop = |why: &str| Row::Stop(why.to_string());
    let nothing_parked = facts.places.in_park.is_empty();
    match (facts.places.park_jar, facts.places.folder_jar) {
        (_, F::Other) => Row::Stop(format!(
            "{name}'s folder holds a session that is not the one moving"
        )),
        (P::Other, _) => stop("the park holds a cookie store that is not the one moving"),
        (P::Mine, F::Mine) => stop("the login's cookie store is in both places"),
        (P::None, F::None | F::NoSession) => stop("the login's cookie store is in neither place"),
        (P::None, F::Mine) => match (half, facts.keys) {
            (Half::Leave, KeysAre::Mine) if nothing_parked => Row::LeaveUntouched,
            (Half::Leave, KeysAre::None) if nothing_parked && facts.kept_mine => Row::LeaveKeysBack,
            (Half::Leave, KeysAre::Named) if nothing_parked => Row::LeaveKeysRewritten,
            (Half::Leave, KeysAre::Mine | KeysAre::None) if !nothing_parked && facts.kept_mine => {
                Row::LeaveOn {
                    keys_in: facts.keys == KeysAre::Mine,
                }
            }
            (Half::Arrive, KeysAre::Mine) if nothing_parked => {
                Row::ArriveRecorded { superseded: false }
            }
            (Half::Arrive, KeysAre::Named) if nothing_parked => {
                Row::ArriveRecorded { superseded: true }
            }
            (Half::Arrive, KeysAre::None) => Row::ArriveBack { jar: true },
            (Half::Arrive, KeysAre::SignedOut | KeysAre::Other) => Row::ArriveBackKeysAside,
            _ => Row::Stop(format!(
                "what is in {name}'s folder and in the park is not a step of a move"
            )),
        },
        (P::Mine, F::None | F::NoSession) => match (half, facts.keys) {
            (Half::Leave, KeysAre::None | KeysAre::SignedOut) => Row::LeaveItemsOn,
            (Half::Arrive, KeysAre::None | KeysAre::SignedOut) => Row::ArriveBack { jar: false },
            _ => Row::Stop(format!(
                "{name}'s settings hold a login's tokens and its folder no session"
            )),
        },
    }
}

fn label_of(state: &State, tool: ProviderId, moving: &Moving) -> String {
    state.typed(&Key::new(tool, moving.label.clone()))
}

// The app's own jar not looked at or not read this time is its folder's refusal; the park's
// is `unseen`'s. One of a format not read, as `unknown` says it.
fn jar_at(
    ctx: &Context,
    app: &dyn Folder,
    place: Place,
    path: &Path,
    unseen: Unseen,
    unknown: &dyn Fn(String, Then) -> Error,
) -> Result<Option<Option<String>>> {
    let jar = moves::jar(app);
    let found = match place {
        Place::Folder => there(app.id(), path)?,
        Place::Park => super::found(path).map_err(|e| unseen(place, jar, path, e.to_string()))?,
    };
    if !found {
        return Ok(None);
    }
    match app.session(ctx, path) {
        Ok(session) => Ok(Some(session.map(|session| session.login))),
        Err(Unread::Unreadable(detail)) => Err(match place {
            Place::Folder => Error::LoginFolderUnreadable {
                tool: app.id(),
                detail,
            },
            Place::Park => unseen(place, jar, path, detail),
        }),
        Err(Unread::Unknown { what, then }) => Err(unknown(what, then)),
    }
}

fn places(
    ctx: &Context,
    app: &dyn Folder,
    root: &Path,
    moving: &Moving,
    unseen: Unseen,
    unknown: &dyn Fn(String, Then) -> Error,
) -> Result<Places> {
    let jar = moves::jar(app);
    let parked = jar_at(
        ctx,
        app,
        Place::Park,
        &parks::item(ctx, &moving.park, jar),
        unseen,
        unknown,
    )?;
    let in_folder = jar_at(ctx, app, Place::Folder, &root.join(jar), unseen, unknown)?;
    let park_jar = match &parked {
        None => ParkJar::None,
        Some(Some(login)) if *login == moving.login => ParkJar::Mine,
        Some(_) => ParkJar::Other,
    };
    let folder_jar = match &in_folder {
        None => FolderJar::None,
        Some(None) => FolderJar::NoSession,
        Some(Some(login)) if *login == moving.login => FolderJar::Mine,
        Some(Some(_)) => FolderJar::Other,
    };
    let seen = |place, item: &str, path: PathBuf| {
        super::found(&path).map_err(|e| unseen(place, item, &path, e.to_string()))
    };
    let (mut in_root, mut in_park) = (Vec::new(), Vec::new());
    for item in moving.items.iter().filter(|item| *item != jar) {
        if seen(Place::Folder, item, root.join(item))? {
            in_root.push(item.clone());
        }
        if seen(Place::Park, item, parks::item(ctx, &moving.park, item))? {
            in_park.push(item.clone());
        }
    }
    Ok(Places {
        park_jar,
        park_login: parked.flatten(),
        folder_jar,
        in_root,
        in_park,
    })
}

fn facts(
    ctx: &Context,
    state: &State,
    app: &dyn Folder,
    root: &Path,
    moving: &Moving,
    undetermined: &dyn Fn(String) -> Error,
) -> Result<Facts> {
    let read = moves::settings(app, root).map_err(undetermined)?;
    let unseen = |_, _: &str, path: &Path, why| undetermined(moves::not_looked_at(path, why));
    let unknown = |what, _| undetermined(format!("{}'s data folder {what}", app.app_name()));
    let places = places(ctx, app, root, moving, &unseen, &unknown)?;
    let held = read.account_keys(app);
    let uuid = state
        .by_id(app.id(), &moving.id)
        .map(|a| a.account_uuid.clone());
    let keys = if held.is_empty() {
        KeysAre::None
    } else if held.digests() == moving.keys {
        KeysAre::Mine
    } else if app
        .caches()
        .iter()
        .all(|key| held.get(key).is_none_or(|value| app.emptied(key, value)))
    {
        KeysAre::SignedOut
    } else if uuid.is_some() && read.named(app) == uuid {
        KeysAre::Named
    } else {
        KeysAre::Other
    };
    let kept_mine =
        moves::kept_keys(ctx, &moving.park).is_ok_and(|kept| kept.digests() == moving.keys);
    Ok(Facts {
        places,
        keys,
        kept_mine,
    })
}

// Empty where no move waits; else what the move came to, said first.
pub(super) fn settle(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    app: &'static dyn Folder,
) -> Result<Vec<Warning>> {
    let tool = app.id();
    let Some(moving) = state.moving(tool).cloned() else {
        return Ok(Vec::new());
    };
    let label = label_of(state, tool, &moving);
    let undetermined = |detail: String| Error::RecoveryUndetermined {
        tool,
        from: label.clone(),
        to: label.clone(),
        detail,
    };
    // A row's own step failed: it may have moved or written before it did.
    let stopped = |detail: String| Error::RecoveryStopped {
        label: label.clone(),
        detail,
    };
    if moving.slot != moves::slot(ctx, app) {
        return Err(Error::RecoveryElsewhere {
            tool,
            from: label.clone(),
            to: label.clone(),
            slot: moving.slot.clone(),
        });
    }
    let root = app.root(ctx).map_err(|e| shape(tool, e))?;
    if !there(tool, &root)? {
        return Err(undetermined(format!(
            "{}'s data folder is not there",
            app.app_name()
        )));
    }
    match app.refuses(&root) {
        // Where the move stopped is known; nothing moves in a folder the app is to empty.
        Some(unfit @ Unfit::ResetWaits) => {
            return Err(Error::LoginFolderUnknown {
                tool,
                what: unfit.to_string(),
                then: unfit.then(),
            });
        }
        Some(unfit) => {
            return Err(undetermined(format!(
                "{}'s data folder {unfit}",
                app.app_name()
            )));
        }
        None => {}
    }
    // A temporary a kill left inside a settings write holds the keys.
    crate::atomic::sweep(permit, &root);
    let facts = facts(ctx, state, app, &root, &moving, &undetermined)?;
    let mut said = Vec::new();
    let ended = match row(app, moving.half, &facts) {
        Row::Stop(why) => return Err(undetermined(why)),
        Row::LeaveUntouched => {
            state.discard(&moving.park, ParkKind::Folder);
            Ended::Undone
        }
        Row::LeaveKeysBack => {
            let back = || -> Step {
                let kept = moves::kept_keys(ctx, &moving.park)?;
                moves::put_keys(app, permit, &root, &kept)
            };
            back().map_err(stopped)?;
            fault::point("settle.keys_back");
            midway(ctx, app, Some(label.clone()))?;
            state.discard(&moving.park, ParkKind::Folder);
            Ended::Undone
        }
        Row::LeaveKeysRewritten => {
            // The app wrote its own keys since, so the copy kept differs and is kept aside.
            if parks::at(ctx, &moving.park).is_dir() {
                said.push(moves::keep_superseded(ctx, state, app, &moving));
            } else {
                state.discard(&moving.park, ParkKind::Folder);
            }
            Ended::Undone
        }
        Row::LeaveOn { keys_in } => {
            let on = || -> Step<Vec<String>> {
                if keys_in {
                    moves::put_keys(app, permit, &root, &Keys::none())?;
                    fault::point("leave.keys_out");
                }
                leave_on(ctx, permit, app, &root, &moving, &moving.items)
            };
            let gone = on().map_err(stopped)?;
            said.extend(moves::gone_warnings(tool, &label, gone));
            midway(ctx, app, Some(label.clone()))?;
            said.extend(moves::commit_leave(ctx, permit, state, app, &moving)?.1);
            return Ok(settled(tool, &moving, Ended::Finished, said));
        }
        Row::LeaveItemsOn => {
            // What is in both places is the app's own since, and stays.
            let rest: Vec<String> = facts
                .places
                .in_root
                .iter()
                .filter(|item| !facts.places.in_park.contains(item))
                .cloned()
                .collect();
            let gone = leave_on(ctx, permit, app, &root, &moving, &rest).map_err(stopped)?;
            said.extend(moves::gone_warnings(tool, &label, gone));
            midway(ctx, app, Some(label.clone()))?;
            said.extend(moves::commit_leave(ctx, permit, state, app, &moving)?.1);
            return Ok(settled(tool, &moving, Ended::Finished, said));
        }
        Row::ArriveRecorded { superseded } => {
            let gone: Vec<String> = moving
                .items
                .iter()
                .filter(|item| *item != moves::jar(app) && !facts.places.in_root.contains(item))
                .cloned()
                .collect();
            said.extend(moves::gone_warnings(tool, &label, gone));
            said.extend(moves::commit_arrive(
                ctx, permit, state, app, &moving, superseded,
            )?);
            purge(ctx, permit, state);
            return Ok(settled(tool, &moving, Ended::Finished, said));
        }
        Row::ArriveBack { jar } => {
            let back = || -> Step {
                take_back(ctx, permit, app, &root, &moving, jar)?;
                if facts.places.folder_jar == FolderJar::None {
                    finish_setting_aside(ctx, permit, state, app, &root, &moving)?;
                }
                Ok(())
            };
            back().map_err(stopped)?;
            midway(ctx, app, Some(label.clone()))?;
            arrive_undone(ctx, state, app, &moving, &mut said);
            Ended::Undone
        }
        Row::ArriveBackKeysAside => {
            let mac = moves::Mac::here(ctx)?;
            let mut moving = moving.clone();
            let aside = crate::park::reserve_aside(ctx, permit, state)?;
            moving.asides.insert(aside.clone(), SetAside::InTheWay);
            state.moving.insert(tool.code().to_string(), moving.clone());
            state::save(ctx, permit, state)?;
            fault::point("settle.aside_named");
            let back = || -> Step {
                let held = moves::settings(app, &root)?.account_keys(app);
                moves::keep_aside(ctx, permit, app, &aside, None, Vec::new(), &held, &mac)?;
                fault::point("settle.aside_kept");
                moves::put_keys(app, permit, &root, &Keys::none())?;
                fault::point("settle.keys_cleared");
                take_back(ctx, permit, app, &root, &moving, true)
            };
            back().map_err(stopped)?;
            midway(ctx, app, Some(label.clone()))?;
            arrive_undone(ctx, state, app, &moving, &mut said);
            Ended::Undone
        }
    };
    // A leave's record names an aside where a give-up was killed partway.
    said.extend(moves::index_asides(ctx, state, app, &moving));
    state.moving.remove(tool.code());
    state::save(ctx, permit, state)?;
    fault::point("settle.saved");
    purge(ctx, permit, state);
    Ok(settled(tool, &moving, ended, said))
}

fn settled(tool: ProviderId, moving: &Moving, ended: Ended, also: Vec<Warning>) -> Vec<Warning> {
    let first = Warning::MoveSettled(MoveSettled {
        tool,
        label: moving.label.clone(),
        half: moving.half,
        ended,
    });
    std::iter::once(first).chain(also).collect()
}

// A leave gone on with: `items` out, then the park judged alone, since the app may have run.
fn leave_on(
    ctx: &Context,
    permit: Permit,
    app: &dyn Folder,
    root: &Path,
    moving: &Moving,
    items: &[String],
) -> Step<Vec<String>> {
    moves::move_out(
        ctx,
        permit,
        app,
        root,
        &moving.park,
        items,
        moves::leave_point,
    )?;
    let gone = moves::verify_parked(ctx, permit, app, root, moving, false)?;
    fault::point("leave.verified");
    Ok(gone)
}

// An arrive taken back: the jar where it came in, then each listed item that is in the app's
// folder and not in the park. A jar the app made for itself stays.
fn take_back(
    ctx: &Context,
    permit: Permit,
    app: &dyn Folder,
    root: &Path,
    moving: &Moving,
    jar: bool,
) -> Step {
    let items: Vec<String> = moving
        .items
        .iter()
        .filter(|item| jar || *item != moves::jar(app))
        .cloned()
        .collect();
    moves::move_out(
        ctx,
        permit,
        app,
        root,
        &moving.park,
        &items,
        |_| "settle.item_back",
    )?;
    moves::verify_parked(ctx, permit, app, root, moving, false).map(drop)
}

// An arrive killed while it set aside what the app had left, its jar first: the rest goes
// where that jar went. Left there, the folder would hold items and no jar, which every
// change refuses until the app has run once.
fn finish_setting_aside(
    ctx: &Context,
    permit: Permit,
    state: &State,
    app: &dyn Folder,
    root: &Path,
    moving: &Moving,
) -> Step {
    for aside in moving.asides.keys() {
        if moves::exists(&parks::item(ctx, aside, moves::jar(app)))? {
            let rest = moves::present(app, root)?;
            moves::move_out(
                ctx,
                permit,
                app,
                root,
                aside,
                &rest,
                |_| "settle.item_aside",
            )?;
            // What a give-up listed is in this aside now, under the account its manifest names.
            return moves::forget_left(ctx, permit, state, app.id()).map_err(|e| e.to_string());
        }
    }
    Ok(())
}

// The account's park as it was, nobody in use, and each folder the record set aside indexed.
fn arrive_undone(
    ctx: &Context,
    state: &mut State,
    app: &dyn Folder,
    moving: &Moving,
    said: &mut Vec<Warning>,
) {
    let now = ctx.now();
    state.identified(app.id(), moves::nobody(now), now);
    said.extend(moves::index_asides(ctx, state, app, moving));
}

// Deletes nothing, and moves only what `said` lets it set aside. `None` where no
// move waits.
pub(super) fn give_up(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    app: &'static dyn Folder,
    said: bool,
) -> Result<Option<(GivenUp, Vec<Warning>)>> {
    let tool = app.id();
    let Some(mut moving) = state.moving(tool).cloned() else {
        return Ok(None);
    };
    let label = label_of(state, tool, &moving);
    let elsewhere = moving.slot != moves::slot(ctx, app);
    let stopped = |detail: String| Error::RecoveryStopped {
        label: label.clone(),
        detail,
    };
    let mut warnings = Vec::new();
    let mut left = Vec::new();
    if !elsewhere {
        let root = app.root(ctx).map_err(|e| shape(tool, e))?;
        // Not the settings, which a give-up never writes. What it cannot read refuses with its
        // own advice: `recovery_undetermined` would send `abandon` back to itself.
        let unknown = |what, then| Error::LoginFolderUnknown { tool, what, then };
        let unseen = |place, item: &str, path: &Path, why: String| match place {
            Place::Folder => Error::LoginFolderUnknown {
                tool,
                what: format!("has `{item}`, which Pitboard could not look at ({why})"),
                then: Then::Nothing,
            },
            Place::Park => parks::unread(&label, path, why),
        };
        let places = places(ctx, app, &root, &moving, &unseen, &unknown)?;
        // A jar leaves first and arrives last, so where it is in the folder the rest is too.
        if places.folder_jar != FolderJar::Mine && places.park_jar == ParkJar::Mine {
            left = places
                .in_root
                .iter()
                .filter(|item| !places.in_park.contains(item) && !moves::of_the_jar(app, item))
                .cloned()
                .collect();
        }
        match places.folder_jar {
            _ if left.is_empty() => {}
            FolderJar::None | FolderJar::NoSession => {
                let listed = || -> Step {
                    let Some(mut manifest) = parks::manifest(ctx, &moving.park)? else {
                        return Err("the park's manifest is missing".into());
                    };
                    manifest.left_in_folder = left.clone();
                    parks::write_manifest(ctx, permit, &moving.park, &manifest)
                        .map_err(|e| format!("the park's manifest could not be written: {e}"))
                };
                listed().map_err(stopped)?;
            }
            FolderJar::Other if !said => {
                let folder = parks::at(ctx, &moving.park);
                let listed = left
                    .iter()
                    .map(|item| format!("`{item}` in {}'s folder", app.app_name()))
                    .chain(std::iter::once(format!(
                        "`{}` in {}",
                        moves::jar(app),
                        folder.display()
                    )))
                    .collect();
                return Err(Error::MoveMixed {
                    label,
                    places: listed,
                });
            }
            FolderJar::Other | FolderJar::Mine => {
                let mac = moves::Mac::here(ctx)?;
                let aside = crate::park::reserve_aside(ctx, permit, state)?;
                moving.asides.insert(aside.clone(), SetAside::GivenUp);
                state.moving.insert(tool.code().to_string(), moving.clone());
                state::save(ctx, permit, state)?;
                fault::point("give_up.aside_named");
                let aside_it = || -> Step {
                    moves::keep_aside(
                        ctx,
                        permit,
                        app,
                        &aside,
                        Some(moving.id.clone()),
                        left.clone(),
                        &Keys::none(),
                        &mac,
                    )?;
                    moves::move_out(
                        ctx,
                        permit,
                        app,
                        &root,
                        &aside,
                        &left,
                        |_| "give_up.item_aside",
                    )
                };
                aside_it().map_err(stopped)?;
                left.clear();
            }
        }
        let park = park_given_up(
            ctx,
            permit,
            state,
            app,
            &moving,
            places.park_jar,
            places.park_login,
        );
        park.map_err(stopped)?;
        midway(ctx, app, Some(label.clone()))?;
    } else {
        let (park_jar, park_login) = park_alone(ctx, app, &moving);
        park_given_up(ctx, permit, state, app, &moving, park_jar, park_login).map_err(stopped)?;
    }
    let before = state.aside.len();
    warnings.extend(moves::index_asides(ctx, state, app, &moving));
    let parked = state
        .by_id(tool, &moving.id)
        .is_some_and(|a| a.parked.is_some());
    let given_up = GivenUp {
        tool,
        label: moving.label.clone(),
        parked: usize::from(parked),
        set_aside: state.aside.len() - before + usize::from(kept_aside(state, &moving)),
        left,
        slot: elsewhere.then(|| moving.slot.clone()),
    };
    if elsewhere {
        // `load` put this run's slot's record in `in_use`; the move's waits in `other_slots`.
        if let Some(records) = state.other_slots.get_mut(tool.code()) {
            records.remove(&moving.slot);
            if records.is_empty() {
                state.other_slots.remove(tool.code());
            }
        }
    } else {
        state.in_use.remove(tool.code());
    }
    state.moving.remove(tool.code());
    state.awaiting.remove(tool.code());
    state::save(ctx, permit, state)?;
    fault::point("give_up.done");
    purge(ctx, permit, state);
    Ok(Some((given_up, warnings)))
}

fn kept_aside(state: &State, moving: &Moving) -> bool {
    state.aside.iter().any(|aside| aside.name == moving.park)
}

// The record's park stays the account's only where its jar holds the record's
// session. A fingerprint found where another was parked lives in the aside's record alone.
fn park_given_up(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    app: &dyn Folder,
    moving: &Moving,
    park_jar: ParkJar,
    park_login: Option<String>,
) -> Step {
    let tool = app.id();
    let holder = state.by_id(tool, &moving.id).cloned();
    let holds_it = holder
        .as_ref()
        .and_then(|a| a.parked.as_ref())
        .is_some_and(|park| park.service == moving.park);
    let anything = parks::holds_anything(ctx, &moving.park)
        .map_err(|e| format!("the park could not be looked at: {e}"))?;
    let given_up = |login| moves::aside_of(ctx, app, moving, SetAside::GivenUp, login);
    match park_jar {
        ParkJar::Mine => {
            moves::hold_what_is_there(ctx, permit, &moving.park, &moving.items)?;
            match &holder {
                Some(_) if holds_it => {}
                Some(account) if account.parked.is_none() => {
                    state.park(&account.key(), moves::folder_park(ctx, moving, ctx.now()));
                }
                _ => state.keep_aside(given_up(Some(moving.login.clone()))),
            }
        }
        ParkJar::Other => state.keep_aside(given_up(park_login)),
        ParkJar::None if anything => state.keep_aside(given_up(None)),
        ParkJar::None => state.discard(&moving.park, ParkKind::Folder),
    }
    Ok(())
}

// The park's own jar, where no app's folder is read. One that does not read is kept as
// another's, so it is never handed to an account.
fn park_alone(ctx: &Context, app: &dyn Folder, moving: &Moving) -> (ParkJar, Option<String>) {
    let jar = parks::item(ctx, &moving.park, moves::jar(app));
    if let Ok(false) = moves::exists(&jar) {
        return (ParkJar::None, None);
    }
    match app.session(ctx, &jar) {
        Ok(Some(session)) if session.login == moving.login => (ParkJar::Mine, Some(session.login)),
        Ok(session) => (ParkJar::Other, session.map(|session| session.login)),
        Err(_) => (ParkJar::Other, None),
    }
}
