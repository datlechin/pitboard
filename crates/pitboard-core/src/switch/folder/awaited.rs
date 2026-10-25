// A sign-in inside the app: the account in use is parked, the app is left signed out, and a
// thread waits for it to be opened and quit signed in. The core opens and quits nothing.

use super::super::enroll::{Enrolment, Filing, SignIn, Steps, WatchedSignIn, new_label};
use super::super::{Error, Result, Settled, enrolled, exclusive, identify, shape};
use super::moves::{self, Leaving, Move, Step};
use super::verdict::{Unsure, Verdict, look};
use super::{Begun, Open, added_as, begin, midway, open, refusal, saying, there};
use crate::context::Context;
use crate::error::Then;
use crate::fault;
use crate::permit::Permit;
use crate::provider::folder::{Folder, Keys};
use crate::state::{self, Aside, Awaited, Key, SetAside, State};
use crate::warning::Warning;
use crate::words::{self, WaitLine as Line};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitStep {
    Waiting,
    Open,
    QuitSignedOut,
    QuitUnfinished,
}

pub(crate) struct Wait {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<Result<()>>>,
}

impl Wait {
    // Joins the thread: how the wait ended.
    pub(crate) fn finish(&mut self) -> Result<()> {
        match self.thread.take().map(std::thread::JoinHandle::join) {
            Some(Ok(ended)) => ended,
            Some(Err(_)) | None => Err(Error::SignInIncomplete),
        }
    }

    pub(crate) fn cancel(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.finish();
    }
}

// The keys a signed-out app kept, as after a Log out, set aside before the wait: a
// sign-in quit within seconds then leaves a session with no name, never one under the old.
// The aside is indexed in `state` for the caller to save.
fn set_keys_aside(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    app: &'static dyn Folder,
    root: &Path,
) -> Result<Option<Warning>> {
    let tool = app.id();
    let refused = |detail: String| Error::LoginFolderUnknown {
        tool,
        what: format!("could not be made ready for a sign-in ({detail})"),
        then: Then::Nothing,
    };
    let held = moves::settings(app, root)
        .map_err(refused)?
        .account_keys(app);
    if held.is_empty() {
        return Ok(None);
    }
    let mover = Move::here(ctx, permit, app, root)?;
    crate::store::folder::ready(ctx, permit, root)?;
    let aside = crate::park::reserve_aside(ctx, permit, state)?;
    let kept = || -> Step {
        moves::keep_aside(&mover, &aside, None, Vec::new(), &held)?;
        fault::point("start.aside_kept");
        moves::put_keys(app, permit, root, &Keys::none())?;
        fault::point("start.keys_cleared");
        Ok(())
    };
    kept().map_err(refused)?;
    midway(ctx, app, None)?;
    state.aside.push(Aside {
        name: aside.clone(),
        tool,
        at: ctx.now(),
        why: SetAside::InTheWay,
        account: None,
        login: None,
    });
    Ok(Some(Warning::SetAside {
        tool,
        path: crate::store::folder::at(ctx, &aside),
        label: None,
        why: SetAside::InTheWay,
    }))
}

// Each warning of the start is said as a line after `first`, since a sign-in returns none.
pub(crate) fn start(
    settled: Settled,
    filing: &Filing,
    app: &'static dyn Folder,
    one_at_a_time: std::fs::File,
    first: Vec<String>,
) -> Result<(WatchedSignIn, Vec<Warning>)> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
        permit,
    } = settled;
    let tool = filing.provider();
    // A new account waits under the name an add left halfway waits under, or `desktop`.
    let key = &match filing {
        Filing::Under(key) => key.clone(),
        Filing::FromEmail(_) => Key::new(tool, new_label(&state, tool, added_as(&state, tool), "")),
    };
    let label = state.typed(key);
    let again = state.get(key).map(|a| a.id.clone());
    let Begun {
        closed: _closed,
        warnings,
    } = begin(&ctx, permit, &mut state, app)?;
    let ((put_back, parked), warnings) = saying(warnings, |warnings| {
        let root = app.root(&ctx).map_err(|e| shape(tool, e))?;
        let mut parked = None;
        let put_back = match look(&ctx, &state, app)? {
            Verdict::Account {
                key: holder,
                session,
                as_named,
            } => {
                let mover = moves::may_start(&ctx, permit, app, &root)?;
                let awaited = Awaited {
                    since: ctx.now(),
                    label: key.label.clone(),
                    account: again.clone(),
                    put_back: enrolled(&state, &holder)?.id,
                };
                let leaving = Leaving {
                    key: &holder,
                    session: &session,
                    awaited: Some(awaited),
                };
                let (_, said) = moves::leave(&mover, &mut state, leaving)?;
                if as_named {
                    warnings.push(Warning::TakenAsNamed {
                        tool,
                        label: state.typed(&holder),
                        recorded: None,
                    });
                }
                warnings.extend(said);
                parked = Some(state.typed(&holder));
                Some(holder)
            }
            Verdict::Nobody { .. } => {
                let mut changed = false;
                if there(tool, &root)? {
                    let set_aside = set_keys_aside(&ctx, permit, &mut state, app, &root)?;
                    changed |= set_aside.is_some();
                    warnings.extend(set_aside);
                }
                let now = ctx.now();
                let (recorded, noticed) =
                    identify::record(&mut state, tool, moves::nobody(now), now);
                changed |= recorded;
                // Only a record with an account parked for it stands: it becomes this sign-in's.
                let standing = state.awaiting(tool).cloned();
                let held = match standing {
                    Some(standing) => {
                        let held = state
                            .by_id(tool, &standing.put_back)
                            .map(state::Account::key);
                        let taken_up = Awaited {
                            since: now,
                            label: key.label.clone(),
                            account: again.clone(),
                            put_back: standing.put_back,
                        };
                        state.awaiting.insert(tool.code().to_string(), taken_up);
                        changed = true;
                        held
                    }
                    None => None,
                };
                if changed {
                    state::save(&ctx, permit, &state)?;
                    fault::point("start.saved");
                }
                identify::write_down(&ctx, permit, &noticed);
                held
            }
            Verdict::Unsure(unsure) => return Err(refusal(&state, tool, &unsure)),
        };
        Ok((put_back, parked))
    })?;
    let typed_back = put_back.as_ref().map(|key| state.typed(key));

    let (say, said) = std::sync::mpsc::channel();
    let steps = Steps::default();
    for line in first {
        let _ = say.send(line);
    }
    for warning in &warnings {
        let _ = say.send(format!("{warning}\n"));
    }
    if let Some(parked) = &parked {
        let _ = say.send(words::awaited(Line::Parked { label: parked }));
    }
    steps.set(WaitStep::Waiting);
    let _ = say.send(words::awaited(match again {
        Some(_) => Line::Again { label: &label },
        None => Line::Add { label: &label },
    }));
    let _ = say.send(words::awaited(Line::NoLogOut));
    let _ = say.send(words::awaited(Line::Waiting {
        put_back: typed_back.as_deref(),
    }));

    let stop = Arc::new(AtomicBool::new(false));
    let waiting = Waiting {
        ctx: ctx.clone(),
        permit,
        app,
        label: key.label.clone(),
        recorded: put_back.is_some(),
        stop: Arc::clone(&stop),
        say,
        steps: steps.clone(),
    };
    let thread = std::thread::spawn(move || waiting.run());
    let wait = Wait {
        stop,
        thread: Some(thread),
    };
    let pending = SignIn::in_its_app(
        &ctx,
        permit,
        one_at_a_time,
        app,
        put_back,
        key.label.clone(),
    );
    Ok((WatchedSignIn::awaited(pending, wait, said, steps), warnings))
}

struct Waiting {
    ctx: Context,
    permit: Permit,
    app: &'static dyn Folder,
    label: String,
    // an `awaiting` record stands for this sign-in
    recorded: bool,
    stop: Arc<AtomicBool>,
    say: Sender<String>,
    steps: Steps,
}

enum Looked {
    SignedIn,
    Nobody,
    Unfinished,
    OpenAgain,
    Busy,
}

impl Waiting {
    fn says(&self, step: WaitStep, line: Line<'_>) {
        self.steps.set(step);
        let _ = self.say.send(words::awaited(line));
    }

    // Holds no lock while it waits, and reads nothing but the process list.
    fn run(self) -> Result<()> {
        let (mut was_open, mut look_again) = (false, false);
        loop {
            if self.stop.load(Ordering::SeqCst) {
                return Err(Error::SignInIncomplete);
            }
            match open(&self.ctx, self.app) {
                Open::Yes(_) => {
                    if !was_open {
                        self.says(WaitStep::Open, Line::Open);
                    }
                    (was_open, look_again) = (true, false);
                }
                Open::No if was_open || look_again => {
                    (was_open, look_again) = (false, false);
                    match self.look()? {
                        Looked::SignedIn => return Ok(()),
                        Looked::Nobody => self.says(WaitStep::QuitSignedOut, Line::QuitSignedOut),
                        Looked::Unfinished => {
                            self.says(WaitStep::QuitUnfinished, Line::QuitUnfinished);
                        }
                        Looked::OpenAgain => was_open = true,
                        Looked::Busy => look_again = true,
                    }
                }
                Open::No | Open::Unknown(_) => {}
            }
            std::thread::sleep(self.ctx.awaited_every());
        }
    }

    // One look once the app has quit, under the locks a change holds. It records only what
    // settling a waiting move records, and says it.
    fn look(&self) -> Result<Looked> {
        let tool = self.app.id();
        let elsewhere = || Error::SignInSettledElsewhere {
            label: self.label.clone(),
        };
        // Asked before the lock, which would make the home again.
        if !crate::home::dir(&self.ctx).is_dir() {
            return Err(elsewhere());
        }
        let _exclusive = exclusive(&self.ctx, self.permit)?;
        let mut state = state::load(&self.ctx)?;
        if self.recorded
            && state
                .awaiting(tool)
                .is_none_or(|awaited| awaited.label != self.label)
        {
            return Err(elsewhere());
        }
        let _closed = match begin(&self.ctx, self.permit, &mut state, self.app) {
            Ok(Begun { closed, warnings }) => {
                for warning in warnings {
                    let _ = self.say.send(format!("{warning}\n"));
                }
                closed
            }
            Err(Error::AppOpen { .. } | Error::AppStateUnknown { .. }) => {
                return Ok(Looked::OpenAgain);
            }
            Err(Error::LoginFolderBusy { .. }) => return Ok(Looked::Busy),
            Err(other) => return Err(other),
        };
        Ok(match look(&self.ctx, &state, self.app)? {
            Verdict::Nobody { .. } => Looked::Nobody,
            Verdict::Unsure(Unsure::Unfinished) => Looked::Unfinished,
            Verdict::Account { .. } | Verdict::Unsure(_) => Looked::SignedIn,
        })
    }
}

// The one enrolment of a sign-in waited for under `waits_as`: `folder::enroll`, as a sign-in.
pub(crate) fn enroll(
    settled: Settled,
    filing: &Filing,
    app: &'static dyn Folder,
    waits_as: &str,
) -> Result<(Enrolment, Vec<Warning>)> {
    super::enroll_as(settled, filing, app, Some(waits_as))
}
