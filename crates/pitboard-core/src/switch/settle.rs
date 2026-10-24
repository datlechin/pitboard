use super::journal::{self, Abandoned, Asking, Recovered, interrupted_tool, reconcile};
use super::{Settled, exclusive, folder, refresh};
use crate::context::Context;
use crate::error::{Error, Result};
use crate::park;
use crate::pending::{self, Reclaimed};
use crate::permit::Permit;
use crate::provider::{self, LoginKind, ProviderId};
use crate::state::{self, Key, ParkKind, State};
use crate::warning::Warning;
use folder::GivenUp;

// What giving up came to, of both kinds of change.
#[derive(Debug, Default)]
pub struct GaveUp {
    pub switch: Option<Abandoned>,
    pub moves: Vec<GivenUp>,
    // what settling said, and each move abandon could not give up after giving up a switch
    pub warnings: Vec<Warning>,
}

// Gives up an interrupted switch, and each move that cannot be settled, keeping every copy
// they name. Under a custom Claude Code endpoint only a Claude Code switch is refused.
pub fn abandon(ctx: &Context, permit: Permit) -> Result<GaveUp> {
    // With no switch waiting the endpoint is not asked: it never stops an app's give-up.
    if let Some(tool) = interrupted_tool(ctx) {
        refuse_custom_oauth(ctx, Some(tool))?;
    }
    let _exclusive = exclusive(ctx, permit)?;
    let mut state = state::load(ctx)?;
    // Whatever an app's state: no Claude Code or Codex change waits on an app's move.
    let mut gave_up = GaveUp {
        switch: journal::abandon(ctx, permit, &mut state)?,
        ..GaveUp::default()
    };
    // A move that can be settled is settled and said: only one that cannot is given up.
    for &tool in ProviderId::ALL {
        let LoginKind::Folder(app) = provider::of(tool) else {
            continue;
        };
        let Some(label) = state
            .moving(tool)
            .map(|moving| state.typed(&Key::new(tool, moving.label.clone())))
        else {
            continue;
        };
        let done =
            gave_up.switch.is_some() || !gave_up.moves.is_empty() || !gave_up.warnings.is_empty();
        match folder::abandon(ctx, permit, &mut state, app) {
            Ok((given_up, said)) => {
                gave_up.moves.extend(given_up);
                gave_up.warnings.extend(said);
            }
            // Said beside what was done, whose "nothing has changed" would be false.
            Err(why) if done => {
                gave_up
                    .warnings
                    .push(Warning::MoveNotGivenUp { tool, label, why });
                // A refusal can leave in memory what it never saved, and the next save would.
                state = state::load(ctx)?;
            }
            Err(refused) => return Err(refused),
        }
    }
    Ok(gave_up)
}

// Claude Code's login is in another item under a custom OAuth endpoint, so nothing that could
// move it goes ahead, a switch of another tool's account aside.
fn refuse_custom_oauth(ctx: &Context, tool: Option<ProviderId>) -> Result<()> {
    if !crate::settings::custom_oauth(ctx) {
        return Ok(());
    }
    let claude = Some(ProviderId::Claude);
    if tool.is_none() || tool == claude || interrupted_tool(ctx) == claude {
        return Err(Error::CustomOauthEndpoint);
    }
    Ok(())
}

// What recovery found comes apart from the `Settled`, to be said whether or not the change that
// follows, about `tool` where it is about one, succeeds.
pub(crate) fn settle(
    ctx: &Context,
    permit: Permit,
    tool: Option<ProviderId>,
) -> Result<(Settled, Option<Recovered>)> {
    refuse_custom_oauth(ctx, tool)?;
    let exclusive = exclusive(ctx, permit)?;
    let mut state = state::load(ctx)?;
    let recovered = reconcile(ctx, permit, &mut state)?;
    // A copy that cannot be saved yet stops what acts on its slot's login, where that asks
    // again (`refresh::finished`), and nothing else.
    let _ = refresh::finish(ctx, permit, &mut state);
    // After the journal has had its say, so a switch's own park is already accounted for.
    pending::sweep(ctx, permit, &mut state)?;
    drop_live_twins(ctx, permit, &mut state)?;
    purge(ctx, permit, &mut state);
    Ok((
        Settled {
            _exclusive: exclusive,
            state,
            ctx: ctx.clone(),
            permit,
        },
        recovered,
    ))
}

// As `settle` works it out, read only: a switch under way in another run does not read as stuck.
pub(crate) fn stuck(ctx: &Context, state: &State, asking: Asking) -> Option<Error> {
    if !journal::pending(ctx) {
        return None;
    }
    // Under a custom Claude Code endpoint no change comes to a Claude Code switch: each is
    // refused over the endpoint first, as settling it and giving up on it are.
    refuse_custom_oauth(ctx, interrupted_tool(ctx)).ok()?;
    journal::refusal(ctx, state, asking)
}

// Every park the store holds and the state does not name, for a machine whose state file, and
// with it the list `settle` reads, was lost or written by a version that kept none.
pub(crate) fn repair(settled: Settled) -> Result<(Reclaimed, Vec<Warning>)> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
        permit,
    } = settled;
    // Nothing of an app is read but where a move of its login waits.
    let mut warnings = Vec::new();
    for &tool in ProviderId::ALL {
        match provider::of(tool) {
            LoginKind::Document(_) => {}
            LoginKind::Folder(app) => {
                warnings.extend(folder::settle_for_repair(&ctx, permit, &mut state, app));
            }
        }
    }
    let mut reclaimed = pending::reclaim(&ctx, permit, &mut state)?;
    give_back_asides(&ctx, permit, &mut state, &mut reclaimed)?;
    purge(&ctx, permit, &mut state);
    Ok((reclaimed, warnings))
}

// A folder set aside goes back to its account where its manifest, keys and jar agree.
fn give_back_asides(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    reclaimed: &mut Reclaimed,
) -> Result<()> {
    let folders = park::parks(ParkKind::Folder);
    let names: Vec<String> = state.aside.iter().map(|aside| aside.name.clone()).collect();
    let mut given = false;
    for name in names {
        let park::Orphan::Login { id, park, .. } = folders.orphan(ctx, state, &name) else {
            continue;
        };
        let Some(key) = pending::takes(ctx, state, &id) else {
            continue;
        };
        state.drop_aside(&name);
        pending::give_back(ctx, permit, state, &key, *park, reclaimed);
        given = true;
    }
    if given {
        state::save(ctx, permit, state)?;
    }
    Ok(())
}

// A park that is a copy of the login in use, left where a new login put in use could not be
// read back and was parked as well, is dropped: the journal records only a switch's own copy.
pub(super) fn drop_live_twins(ctx: &Context, permit: Permit, state: &mut State) -> Result<()> {
    let twins = park::live_twins(ctx, state);
    if twins.is_empty() {
        return Ok(());
    }
    for service in &twins {
        state.discard(service, ParkKind::Vault);
    }
    state::save(ctx, permit, state)
}

// A failed save leaves deleted names listed, and deleting a missing item succeeds, so a later
// run clears them.
pub(super) fn purge(ctx: &Context, permit: Permit, state: &mut State) -> usize {
    let listed = state.discarded.len();
    let remaining = park::purge(ctx, permit, state);
    if remaining != listed {
        let _ = state::save(ctx, permit, state);
    }
    remaining
}
