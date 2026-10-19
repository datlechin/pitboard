pub(crate) mod awaited;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod crash;
mod moves;
mod settle;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests;
mod verdict;

pub use settle::{Ended, GivenUp, MoveSettled};

use super::enroll::{Enrolled, account, claim};
use super::{Error, Outcome, Result, Settled, enrolled, identify, shape};
use crate::context::Context;
use crate::error::{LoginDoubt, Then};
use crate::host::Listed;
use crate::in_use::{self, Doubt, InUse, Known};
use crate::provider::Adoption;
use crate::provider::folder::{AppSettings, Folder};
use crate::service::{Permit, Warning};
use crate::state::{self, Key, ParkKind, SetAside, State, new_id};
use std::path::Path;
use verdict::{Named, Unsure, Verdict, look};

pub(crate) enum Open {
    No,
    Yes(Vec<Listed>),
    // why the process list did not read
    Unknown(String),
}

pub(crate) fn open(ctx: &Context, app: &dyn Folder) -> Open {
    let root = match app.root(ctx) {
        Ok(root) => root,
        Err(e) => return Open::Unknown(e.to_string()),
    };
    let all = match ctx.host().listing() {
        Ok(all) => all,
        Err(why) => return Open::Unknown(why),
    };
    let running: Vec<Listed> = all
        .iter()
        .filter(|one| app.is_the_app(ctx, &root, one, &all))
        .cloned()
        .collect();
    if running.is_empty() {
        Open::No
    } else {
        Open::Yes(running)
    }
}

// The app seen closed, and its data folder held, until this is dropped.
pub(crate) struct Closed {
    lock: Option<std::fs::File>,
}

impl Drop for Closed {
    // Unlocked, not only closed: a child forked meanwhile holds a copy, as in `SignIn::drop`.
    fn drop(&mut self) {
        if let Some(lock) = &self.lock {
            let _ = lock.unlock();
        }
    }
}

pub(crate) fn closed(ctx: &Context, app: &'static dyn Folder) -> Result<Closed> {
    let root = app.root(ctx).map_err(|e| shape(app.id(), e))?;
    quiet(ctx, app)?;
    hold(app, &root)
}

// The app closed and its folder held, with a move an earlier run left settled first.
pub(crate) struct Begun {
    pub closed: Closed,
    // what settling came to
    pub warnings: Vec<Warning>,
}

pub(crate) fn begin(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    app: &'static dyn Folder,
) -> Result<Begun> {
    let closed = closed(ctx, app)?;
    let warnings = settle::settle(ctx, permit, state, app)?;
    Ok(Begun { closed, warnings })
}

// The look after the last write: seen open or not told, nothing is recorded as done.
fn midway(ctx: &Context, app: &dyn Folder, label: Option<String>) -> Result<()> {
    match open(ctx, app) {
        Open::No => Ok(()),
        Open::Yes(_) | Open::Unknown(_) => Err(Error::AppOpenedMidway {
            app: app.app_name(),
            label,
        }),
    }
}

fn quiet(ctx: &Context, app: &dyn Folder) -> Result<()> {
    match open(ctx, app) {
        Open::No => Ok(()),
        Open::Yes(listed) => Err(Error::AppOpen {
            app: app.app_name(),
            still: still(app, &listed),
        }),
        Open::Unknown(why) => Err(Error::AppStateUnknown {
            app: app.app_name(),
            why,
        }),
    }
}

// Nothing where the app's own program runs; otherwise each part of it that does.
fn still(app: &dyn Folder, listed: &[Listed]) -> Vec<String> {
    let main = Path::new("Contents/MacOS").join(app.app_name());
    if listed.iter().any(|one| one.path.ends_with(&main)) {
        return Vec::new();
    }
    listed
        .iter()
        .map(|one| {
            let name = one.path.file_name().unwrap_or(one.path.as_os_str());
            format!("{} (pid {})", name.to_string_lossy(), one.pid)
        })
        .collect()
}

// Only "not found" says a path is not there: any other error is a fact not read, so doubt.
fn found(path: &Path) -> std::io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

fn there(tool: crate::provider::ProviderId, path: &Path) -> Result<bool> {
    found(path).map_err(|e| Error::LoginFolderUnknown {
        tool,
        what: format!("could not be looked at ({e})"),
        then: Then::Nothing,
    })
}

// Never waited for. A folder that is not there holds nothing to move, so nothing is held.
fn hold(app: &dyn Folder, root: &Path) -> Result<Closed> {
    if !there(app.id(), root)? {
        return Ok(Closed { lock: None });
    }
    match crate::host::fs::lock_dir(root) {
        Ok(lock) => Ok(Closed { lock: Some(lock) }),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Err(Error::LoginFolderBusy {
            app: app.app_name(),
        }),
        Err(e) => Err(Error::LiveStoreUnsupported {
            tool: app.id(),
            reason: format!("{}'s data folder could not be held ({e})", app.app_name()),
        }),
    }
}

// A read's answer, from one read of the settings, which are all it opens.
pub(crate) fn read(ctx: &Context, state: &State, app: &dyn Folder) -> (Known, Option<Key>) {
    match app.root(ctx) {
        Ok(root) => judged(state, app, app.settings(&root).ok().as_ref()),
        Err(_) => {
            let known = Known {
                last: state.in_use(app.id()).cloned(),
                doubt: None,
                own_record: None,
            };
            (known, None)
        }
    }
}

// The record judged against the settings, `None` where they did not read, and who looks signed out.
pub(crate) fn judged(
    state: &State,
    app: &dyn Folder,
    settings: Option<&AppSettings>,
) -> (Known, Option<Key>) {
    let tool = app.id();
    let last = state.in_use(tool).cloned();
    let Some(settings) = settings else {
        let known = Known {
            last,
            doubt: Some(Doubt::LoginUnreadable),
            own_record: None,
        };
        return (known, None);
    };
    let named = settings.named(app).map(in_use::named_only);
    (
        in_use::judged_by(tool, last, named),
        looks_signed_out(state, app, settings),
    )
}

// The account recorded in use, where the settings name it beside caches a Log out leaves.
fn looks_signed_out(state: &State, app: &dyn Folder, settings: &AppSettings) -> Option<Key> {
    let owner = state.in_use(app.id())?.owner.as_ref()?;
    if settings.named(app)? != owner.account_uuid {
        return None;
    }
    let keys = settings.account_keys(app);
    let emptied = app
        .caches()
        .iter()
        .all(|key| keys.get(key).is_some_and(|value| app.emptied(key, value)));
    if !emptied {
        return None;
    }
    state.account_of(app.id(), owner).map(state::Account::key)
}

// The login the app holds, filed under `key`: the person's word for whose it is. Moves
// nothing but what a stopped move left beside it. A park it supersedes is set aside.
pub(crate) fn enroll(
    settled: Settled,
    key: &Key,
    app: &'static dyn Folder,
) -> Result<(Enrolled, Vec<Warning>)> {
    enroll_as(settled, key, app, false)
}

// `waited`: the enrolment of a sign-in inside the app that Pitboard waited for.
fn enroll_as(
    settled: Settled,
    key: &Key,
    app: &'static dyn Folder,
    waited: bool,
) -> Result<(Enrolled, Vec<Warning>)> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
        permit,
    } = settled;
    let tool = key.provider;
    let _closed = closed(&ctx, app)?;
    // A move that cannot be settled is given up only once this enrolment will go through.
    let (stopped, warnings) = match settle::settle(&ctx, permit, &mut state, app) {
        Ok(settled) => (false, settled),
        Err(Error::RecoveryUndetermined { .. } | Error::RecoveryStopped { .. }) => {
            (true, Vec::new())
        }
        Err(other) => return Err(other),
    };
    saying(warnings, |warnings| {
        let (named, login, aside, recorded) = match look(&ctx, &state, app)? {
            Verdict::Nobody { .. } => {
                return Err(Error::LiveCredentialAbsent {
                    tool,
                    label: Some(state.typed(key)),
                });
            }
            Verdict::Account {
                key: holder,
                session,
                ..
            } => (Named::Enrolled(holder), session.login, None, None),
            Verdict::Unsure(Unsure::Unfinished) => return Err(Error::SignInUnfinished { tool }),
            Verdict::Unsure(Unsure::RecordedUnderAnother { recorded, named }) => {
                let login = state
                    .in_use(tool)
                    .map(|record| record.login.clone())
                    .unwrap_or_default();
                (named, login, None, Some(state.typed(&recorded)))
            }
            Verdict::Unsure(Unsure::AlsoParked {
                key: holder,
                session,
            }) => (Named::Enrolled(holder), session.login, None, None),
            Verdict::Unsure(Unsure::Unseen { named, session }) => {
                (named, session.login, None, None)
            }
            Verdict::Unsure(Unsure::HeldTwice {
                named,
                session,
                also_in,
            }) => (named, session.login, Some(also_in), None),
        };
        let owner = match &named {
            Named::Enrolled(holder) => enrolled(&state, holder)?.owner(),
            Named::NotEnrolled { uuid } => in_use::named_only(uuid.clone()),
        };
        claim(&state, key, &owner).map_err(|refused| match refused {
            Error::AlreadyEnrolled {
                tool,
                email,
                label,
                asked,
                ..
            } => Error::AlreadyEnrolled {
                tool,
                email,
                label,
                asked,
                waited,
            },
            other => other,
        })?;
        if stopped && let Some((_, said)) = settle::give_up(&ctx, permit, &mut state, app, true)? {
            warnings.extend(said);
        }
        let now = ctx.now();
        let again = state.get(key).is_some();
        let parked = state.get(key).and_then(|a| a.parked.clone());
        state.upsert(account(
            tool,
            state.id_of(tool, &owner),
            &key.label,
            &owner,
            parked,
            Some(now),
            None,
        ));
        // Said once saved: a save that fails carries only what settling and giving up saved.
        let mut filed = Vec::new();
        if recorded.is_some() {
            filed.push(Warning::TakenAsNamed {
                tool,
                label: state.typed(key),
                recorded,
            });
        }
        let superseded = |state: &State| -> Vec<Key> {
            state
                .accounts
                .iter()
                .filter(|a| a.provider() == tool)
                .filter(|a| {
                    a.parked.as_ref().is_some_and(|park| {
                        Some(&park.service) == aside.as_ref() || (a.is(key) && park.login != login)
                    })
                })
                .map(state::Account::key)
                .collect()
        };
        for holder in superseded(&state) {
            let Some(park) = state.get(&holder).and_then(|a| a.parked.clone()) else {
                continue;
            };
            let path = crate::store::folder::at(&ctx, &park.service);
            state.set_aside(&holder, park, SetAside::Superseded, now);
            filed.push(Warning::SetAside {
                tool,
                path,
                label: Some(state.typed(&holder)),
                why: SetAside::Superseded,
            });
        }
        let found = InUse {
            named: Some(new_id(tool, &owner)),
            owner: Some(owner),
            login,
            known_at: now,
        };
        let (_, noticed) = identify::record(&mut state, tool, found, now);
        state.awaiting.remove(tool.code());
        moves::forget_left(&ctx, permit, &state, tool)?;
        state::save(&ctx, permit, &state)?;
        warnings.extend(filed);
        identify::write_down(&ctx, permit, &noticed);
        let enrolled = if waited {
            Enrolled::InUse {
                email: String::new(),
                again,
            }
        } else {
            Enrolled::Current {
                email: String::new(),
            }
        };
        Ok(enrolled)
    })
}

// A leave of the account in use, committed, then an arrive of `key`'s. Between them the
// app's folder is signed out.
pub(crate) fn switch(
    settled: Settled,
    key: &Key,
    app: &'static dyn Folder,
) -> Result<(Outcome, Vec<Warning>)> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
        permit,
    } = settled;
    let tool = key.provider;
    enrolled(&state, key)?;
    let Begun {
        closed: _closed,
        warnings,
    } = begin(&ctx, permit, &mut state, app)?;
    saying(warnings, |warnings| {
        let root = app.root(&ctx).map_err(|e| shape(tool, e))?;
        if !there(tool, &root)? {
            return Err(Error::LoginFolderUnknown {
                tool,
                what: "does not exist yet".into(),
                then: Then::OpenAndQuit,
            });
        }
        let adoption = Adoption::NextOpen {
            app: app.app_name(),
        };
        match look(&ctx, &state, app)? {
            Verdict::Account { key: holder, .. } if holder == *key => Ok(Outcome::AlreadyActive {
                label: state.typed(key),
                config_updated: false,
            }),
            Verdict::Account {
                key: holder,
                session,
                as_named,
            } => {
                let mac = moves::may_start(&ctx, permit, app, &root)?;
                let checked = moves::check_park(&ctx, &state, app, key, &mac)?;
                let from = state.typed(&holder);
                let leaving = moves::Leaving {
                    key: &holder,
                    session: &session,
                    awaited: None,
                };
                let (parked, said) =
                    moves::leave(&ctx, permit, &mut state, app, &root, leaving, &mac)?;
                if as_named {
                    warnings.push(Warning::TakenAsNamed {
                        tool,
                        label: from.clone(),
                        recorded: None,
                    });
                }
                warnings.extend(said);
                warnings.extend(moves::arrive(
                    &ctx,
                    permit,
                    &mut state,
                    app,
                    &root,
                    key,
                    checked,
                    false,
                    Some(from.clone()),
                    &[],
                    &mac,
                )?);
                Ok(Outcome::Switched {
                    provider: tool,
                    from,
                    to: state.typed(key),
                    parked,
                    adoption,
                })
            }
            Verdict::Nobody { leftovers } => {
                let mac = moves::may_start(&ctx, permit, app, &root)?;
                let checked = moves::check_park(&ctx, &state, app, key, &mac)?;
                // Saved with the arrive's record: whoever was recorded in use signed out since.
                let now = ctx.now();
                let (_, noticed) = identify::record(&mut state, tool, moves::nobody(now), now);
                warnings.extend(moves::arrive(
                    &ctx, permit, &mut state, app, &root, key, checked, leftovers, None, &noticed,
                    &mac,
                )?);
                Ok(Outcome::Installed {
                    provider: tool,
                    to: state.typed(key),
                    adoption,
                })
            }
            Verdict::Unsure(unsure) => Err(refusal(&state, tool, &unsure)),
        }
    })
}

// For a button drawn while `key`'s account was in use: nothing to write while it still is.
pub(crate) fn confirm(
    settled: Settled,
    key: &Key,
    app: &'static dyn Folder,
) -> Result<(Outcome, Vec<Warning>)> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
        permit,
    } = settled;
    enrolled(&state, key)?;
    let Begun {
        closed: _closed,
        warnings,
    } = begin(&ctx, permit, &mut state, app)?;
    saying(warnings, |_| match look(&ctx, &state, app)? {
        Verdict::Account { key: holder, .. } if holder == *key => Ok(Outcome::AlreadyActive {
            label: state.typed(key),
            config_updated: false,
        }),
        _ => Err(Error::AccountNotInUse {
            tool: key.provider,
            label: state.typed(key),
        }),
    })
}

// What `forget` learns of the app's folder, held until it has saved where it was seen closed.
pub(crate) struct Asked {
    // a refusal here is `forget`'s to take by its doubt rule
    pub in_use: std::result::Result<bool, Error>,
    pub closed: Option<Closed>,
    // a move an earlier run left, as it was settled first
    pub warnings: Vec<Warning>,
}

// Whether `key`'s account is the one in use. A refusal returned stops `forget`, since its doubt
// rule would forget the account whose login a waiting move holds.
pub(crate) fn in_use_for_forget(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    key: &Key,
    app: &'static dyn Folder,
) -> Result<Asked> {
    let waits = state.moving(app.id()).is_some();
    let Begun { closed, warnings } = match begin(ctx, permit, state, app) {
        Ok(begun) => begun,
        Err(stopped) if waits => return Err(stopped),
        Err(doubt) => {
            return Ok(Asked {
                in_use: Err(doubt),
                closed: None,
                warnings: Vec::new(),
            });
        }
    };
    let (in_use, warnings) = saying(warnings, |_| {
        let verdict = match look(ctx, state, app) {
            Ok(verdict) => verdict,
            Err(doubt) => return Ok(Err(doubt)),
        };
        match verdict {
            Verdict::Account { key: holder, .. } => Ok(Ok(holder == *key)),
            Verdict::Nobody { .. } => {
                let now = ctx.now();
                let nobody = InUse {
                    owner: None,
                    login: String::new(),
                    known_at: now,
                    named: in_use::named(ctx, key.provider),
                };
                let (_, noticed) = identify::record(state, key.provider, nobody, now);
                state::save(ctx, permit, state)?;
                identify::write_down(ctx, permit, &noticed);
                Ok(Ok(false))
            }
            Verdict::Unsure(unsure) => Ok(Err(refusal(state, key.provider, &unsure))),
        }
    })?;
    Ok(Asked {
        in_use,
        closed: Some(closed),
        warnings,
    })
}

// For `abandon`: a move that settles is settled and said, one that cannot is given up, with
// the app closed and held. A move in a data folder this run does not read is given up with
// none of that folder read.
pub(crate) fn abandon(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    app: &'static dyn Folder,
) -> Result<(Option<GivenUp>, Vec<Warning>)> {
    let Some(moving) = state.moving(app.id()) else {
        return Ok((None, Vec::new()));
    };
    let split = |given: Option<(GivenUp, Vec<Warning>)>| {
        given.map_or((None, Vec::new()), |(given_up, said)| {
            (Some(given_up), said)
        })
    };
    if moving.slot != moves::slot(ctx, app) {
        return settle::give_up(ctx, permit, state, app, false).map(split);
    }
    let _closed = closed(ctx, app)?;
    match settle::settle(ctx, permit, state, app) {
        Ok(settled) => Ok((None, settled)),
        Err(Error::RecoveryUndetermined { .. } | Error::RecoveryStopped { .. }) => {
            settle::give_up(ctx, permit, state, app, false).map(split)
        }
        Err(other) => Err(other),
    }
}

// For `repair`, only where a move waits: settled where it can be, said with why where not.
pub(crate) fn settle_for_repair(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    app: &'static dyn Folder,
) -> Vec<Warning> {
    let tool = app.id();
    let Some(moving) = state.moving(tool) else {
        return Vec::new();
    };
    let label = state.typed(&Key::new(tool, moving.label.clone()));
    match begin(ctx, permit, state, app) {
        Ok(begun) => begun.warnings,
        Err(why) => vec![Warning::MoveNotSettled { tool, label, why }],
    }
}

// For `uninstall`, only where a move waits, an account holds a folder park or a folder set
// aside holds a login: refused where the app holds nobody, since every login Pitboard holds
// would then be deleted with none in use.
pub(crate) fn before_uninstall(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    app: &'static dyn Folder,
) -> Result<Option<(Closed, Vec<Warning>)>> {
    let tool = app.id();
    let parked = |state: &State| {
        state
            .accounts
            .iter()
            .filter(|a| a.provider() == tool)
            .filter_map(|a| Some((a.key(), a.parked.as_ref()?)))
            .filter(|(_, park)| park.kind == ParkKind::Folder)
            .max_by_key(|(_, park)| park.parked_at)
            .map(|(key, _)| key)
    };
    // One deleted by hand strands nothing; one that cannot be looked at still counts.
    let set_aside = |state: &State| -> Vec<std::path::PathBuf> {
        state
            .aside
            .iter()
            .filter(|aside| aside.tool == tool && aside.login.is_some())
            .filter(|aside| {
                !matches!(
                    crate::park::parks(ParkKind::Folder).taken(ctx, &aside.name),
                    Ok(false)
                )
            })
            .map(|aside| crate::store::folder::at(ctx, &aside.name))
            .collect()
    };
    if state.moving(tool).is_none() && parked(state).is_none() && set_aside(state).is_empty() {
        return Ok(None);
    }
    let Begun { closed, warnings } = begin(ctx, permit, state, app)?;
    let ((), warnings) = saying(warnings, |_| match look(ctx, state, app)? {
        Verdict::Account { .. } => Ok(()),
        Verdict::Nobody { .. } => {
            let label = parked(state).map(|last| state.typed(&last));
            let set_aside = set_aside(state);
            if label.is_none() && set_aside.is_empty() {
                return Ok(());
            }
            Err(Error::UninstallWouldStrand { label, set_aside })
        }
        Verdict::Unsure(unsure) => Err(refusal(state, tool, &unsure)),
    })?;
    Ok(Some((closed, warnings)))
}

// What `run` comes to, beside what the change said before and while it ran, refused or not.
fn saying<T>(
    mut said: Vec<Warning>,
    run: impl FnOnce(&mut Vec<Warning>) -> Result<T>,
) -> Result<(T, Vec<Warning>)> {
    match run(&mut said) {
        Ok(value) => Ok((value, said)),
        Err(refused) => Err(refused.after(said)),
    }
}

// What every change but `enroll` says of a login only the person's word can file.
fn refusal(state: &State, tool: crate::provider::ProviderId, unsure: &Unsure) -> Error {
    let named = |named: &Named| match named {
        Named::Enrolled(key) => Some(state.typed(key)),
        Named::NotEnrolled { .. } => None,
    };
    let doubt = |why| Error::LoginInDoubt { tool, why };
    match unsure {
        Unsure::Unfinished => Error::SignInUnfinished { tool },
        Unsure::RecordedUnderAnother {
            recorded,
            named: by,
        } => doubt(LoginDoubt::NamedAnother {
            recorded: state.typed(recorded),
            named: named(by),
        }),
        Unsure::AlsoParked { key, .. }
        | Unsure::Unseen {
            named: Named::Enrolled(key),
            ..
        } => doubt(LoginDoubt::SignedInOutside {
            label: state.typed(key),
        }),
        Unsure::Unseen {
            named: Named::NotEnrolled { .. },
            ..
        } => Error::LiveAccountNotEnrolled {
            tool,
            who: crate::words::login(tool, "", false),
        },
        Unsure::HeldTwice {
            named: by, also_in, ..
        } => {
            let holder = state
                .accounts
                .iter()
                .find(|a| a.parked.as_ref().is_some_and(|p| p.service == *also_in))
                .or_else(|| {
                    state
                        .aside
                        .iter()
                        .find(|aside| aside.name == *also_in)
                        .and_then(|aside| state.owner_of_park(aside.account.as_deref()?))
                });
            doubt(LoginDoubt::NamedAnother {
                recorded: holder.map_or_else(|| also_in.clone(), |a| state.typed(&a.key())),
                named: named(by),
            })
        }
    }
}
