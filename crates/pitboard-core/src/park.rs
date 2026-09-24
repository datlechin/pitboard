//! Where an account's login waits while another is signed in. Nothing here decides what to
//! delete: a park no account refers to is listed in `State::discarded` and purged from there.

use crate::context::Context;
use crate::error::{Error, Result};
use crate::provider::ProviderId;
use crate::state::{Key, Park, State};
use crate::store;
use serde_json::Value;

const PREFIX: &str = "pitboard-park-";

pub fn service_name(account_uuid: &str, at_millis: i64) -> String {
    format!("{PREFIX}{account_uuid}-{at_millis}")
}

/// A name pitboard made, rather than one Claude Code did. Nothing deletes an item without
/// this being true of its name.
pub fn is_park_name(service: &str) -> bool {
    parts_of(service).is_some()
}

/// The account and the moment a name carries. An account uuid contains dashes, so the
/// moment is taken from the end.
pub fn parts_of(service: &str) -> Option<(String, i64)> {
    let rest = service.strip_prefix(PREFIX)?;
    let (uuid, millis) = rest.rsplit_once('-')?;
    let at_millis: i64 = millis.parse().ok()?;
    (!uuid.is_empty()).then(|| (uuid.to_string(), at_millis))
}

/// Claim a free name before writing to it, so the caller can record it first and recovery
/// can find a park left by a run that died. Reusing a name would destroy the park there.
pub fn reserve(ctx: &Context, account_uuid: &str) -> Result<String> {
    let start = ctx.now_millis();
    for offset in 0..1_000 {
        let candidate = service_name(account_uuid, start + offset);
        if store::vault_read(ctx, &candidate)?.is_none() {
            // Written down before anything is written into it, so a run killed between the
            // two leaves a name the next command can resolve rather than a login nothing
            // on the machine can see.
            crate::pending::reserve(ctx, &candidate)?;
            return Ok(candidate);
        }
    }
    Err(Error::ParkSlotExhausted)
}

/// Whether a login can be parked at all on this machine, and how.
///
/// On macOS a park goes into the keychain through `security`, whose stdin takes 4032 bytes.
/// A Claude Code slice is a few hundred, but a Codex login is its whole `auth.json`, over
/// four kilobytes before it is hex-encoded, so every Codex park is past the ceiling. Above
/// it the write goes on the argument line, which is said, or is refused where
/// `PITBOARD_NO_ARGV` forbids that, before anything has moved rather than halfway through.
pub fn price(
    ctx: &Context,
    provider: ProviderId,
    label: &str,
    service: &str,
    document: &Value,
) -> Result<Option<crate::service::Warning>> {
    let body = serde_json::to_string(document).expect("a credential slice is always serialisable");
    let Some(price) = store::vault_cost(ctx, service, &body) else {
        return Ok(None);
    };
    if price.refused() {
        return Err(Error::CredentialTooLarge {
            tool: provider,
            label: label.to_string(),
            bytes: price.needs,
            limit: price.limit,
        });
    }
    Ok(price
        .on_the_second_route()
        .then_some(crate::service::Warning::WrittenOnTheCommandLine {
            tool: provider,
            bytes: price.needs,
            limit: price.limit,
        }))
}

/// Write a login into a reserved name and prove it reads back.
pub fn store_at(
    ctx: &Context,
    provider: ProviderId,
    service: &str,
    document: &Value,
) -> Result<Park> {
    let park = describe(provider, service, ctx.now(), document);
    if park.refresh_fingerprint.is_empty() {
        return Err(Error::LiveCredentialShapeUnexpected {
            tool: provider,
            detail: "it has no refresh token, so it could never be restored".into(),
        });
    }
    let body = serde_json::to_string(document).expect("a credential slice is always serialisable");
    store::vault_write(ctx, service, &body)?;
    Ok(park)
}

/// What the account index records about a login: nothing secret.
pub fn describe(provider: ProviderId, service: &str, parked_at: i64, document: &Value) -> Park {
    // Through the provider: where the dates are and what unit they are in is a fact about
    // the tool, and the three disagree on both.
    let tool = crate::provider::of(provider);
    let expiry = tool.expiry(document);
    Park {
        service: service.to_string(),
        parked_at,
        refresh_fingerprint: tool.fingerprint(document),
        access_expires_at: expiry.access_expires_at,
        refresh_expires_at: expiry.refresh_expires_at,
    }
}

/// Takes the account so a failure names it, not an item the user has never seen, and so
/// the fingerprint is read the way that account's tool lays its login out.
pub fn load(ctx: &Context, key: &Key, park: &Park) -> Result<Value> {
    let label = key.typed();
    let raw =
        store::vault_read(ctx, &park.service)?.ok_or_else(|| Error::ParkedCredentialMissing {
            label: label.clone(),
        })?;
    let value: Value = serde_json::from_str(&raw).map_err(|e| Error::ParkedCredentialCorrupt {
        label: label.clone(),
        detail: e.to_string(),
    })?;
    let found = crate::provider::of(key.provider).fingerprint(&value);
    if park.refresh_fingerprint.is_empty() || found != park.refresh_fingerprint {
        return Err(Error::ParkedCredentialCorrupt {
            label,
            detail: "it does not match the fingerprint pitboard recorded".into(),
        });
    }
    Ok(value)
}

/// Whether this parked document is a second copy of the login signed in now, for a tool
/// whose park may never be one.
///
/// Answered from the two refresh tokens' fingerprints, so it needs no network. For a tool
/// whose own sign-out revokes what it finds, keeping such a copy is keeping a token the
/// person's next sign-out would end in both places; it is discarded instead. For a tool
/// that tolerates a copy this is never true, and what was always done still is.
pub fn is_live_twin(ctx: &Context, provider: ProviderId, document: &Value) -> bool {
    let tool = crate::provider::of(provider);
    if tool.park_semantics() != crate::provider::ParkSemantics::MoveOnly {
        return false;
    }
    let parked = tool.fingerprint(document);
    !parked.is_empty() && live_fingerprint(ctx, provider).is_some_and(|live| live == parked)
}

/// The fingerprint of the refresh token the tool has in use now, where there is one and it
/// can be read.
fn live_fingerprint(ctx: &Context, provider: ProviderId) -> Option<String> {
    let tool = crate::provider::of(provider);
    let live = tool.read_live(ctx).ok().flatten()?;
    Some(tool.fingerprint(&live.raw)).filter(|found| !found.is_empty())
}

/// Every park an account holds that is a copy of the login its tool has in use now.
///
/// For every tool, where `is_live_twin` asks only for a tool whose park may never be a
/// copy: whatever a tool does about copies, a copy of the login in use is one refresh token
/// in two places, and renewing it spends the token the tool itself is about to present. One
/// is left when a new login was put in use and could not be read back, so it was parked as
/// well. A tool no account holds a park of is not read at all, and neither is Claude Code's
/// login under a custom OAuth endpoint, which is somewhere pitboard does not act on.
pub fn live_twins(ctx: &Context, state: &State) -> Vec<String> {
    let mut twins = Vec::new();
    for &provider in ProviderId::ALL {
        let held: Vec<&crate::state::Park> = state
            .accounts
            .iter()
            .filter(|a| a.provider() == provider)
            .filter_map(|a| a.parked.as_ref())
            .collect();
        if held.is_empty() || (provider == ProviderId::Claude && crate::settings::custom_oauth(ctx))
        {
            continue;
        }
        let Some(live) = live_fingerprint(ctx, provider) else {
            continue;
        };
        twins.extend(
            held.into_iter()
                .filter(|park| park.refresh_fingerprint == live)
                .map(|park| park.service.clone()),
        );
    }
    twins
}

/// Delete every discarded item, keeping listed only those that resisted. Returns how many
/// remain.
pub fn purge(ctx: &Context, state: &mut State) -> usize {
    state
        .discarded
        .retain(|service| store::vault_delete(ctx, service).is_err());
    state.discarded.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::memory::{Fault, MemoryHost};
    use crate::time::FixedClock;
    use serde_json::json;
    use std::sync::Arc;

    fn work() -> Key {
        Key::new(ProviderId::Claude, "work")
    }

    fn oauth(token: &str) -> Value {
        json!({
            "refreshToken": token,
            "accessToken": "a",
            "expiresAt": 1_790_000_000_000i64,
            "refreshTokenExpiresAt": 1_792_000_000_000i64
        })
    }

    /// A machine whose stores are in memory, whose clock stands still, and whose home is a
    /// scratch directory: reserving a name writes it down before it is used.
    fn machine() -> (Context, Arc<MemoryHost>, Scratch) {
        let root = std::env::temp_dir().join(format!(
            "pitboard-park-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let mem = MemoryHost::new();
        let clock = Arc::new(FixedClock::at(1_760_000_000));
        let ctx = Context::new(root.clone())
            .with_pitboard_home(root.clone())
            .with_memory_stores(Arc::clone(&mem))
            .with_clock(clock as Arc<dyn crate::time::Clock>);
        (ctx, mem, Scratch(root))
    }

    /// Takes the scratch home away when the test ends, however it ends.
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Park, then read it back through the same rules a switch uses. Before the stores were
    /// a seam this needed a real keychain, so it only ran on one platform and only against
    /// whatever the machine happened to hold.
    #[test]
    fn a_parked_login_reads_back_through_its_fingerprint() {
        let (ctx, mem, _scratch) = machine();
        let name = reserve(&ctx, "acc").expect("a free name");
        let park = store_at(&ctx, ProviderId::Claude, &name, &oauth("r")).expect("stored");

        assert_eq!(mem.vault().services(), vec![name.clone()]);
        assert_eq!(load(&ctx, &work(), &park).expect("loads"), oauth("r"));
    }

    /// The distinction the whole store layer exists to keep. A park that is gone is gone and
    /// the account needs signing in to again; a park that cannot be read says nothing about
    /// whether it is there, and telling someone to sign in again would be wrong.
    #[test]
    fn a_park_that_vanished_and_one_that_cannot_be_read_are_different_answers() {
        let (ctx, mem, _scratch) = machine();
        let name = reserve(&ctx, "acc").expect("a free name");
        let park = store_at(&ctx, ProviderId::Claude, &name, &oauth("r")).expect("stored");

        mem.vault().fault(&name, Fault::Vanish);
        assert!(matches!(
            load(&ctx, &work(), &park),
            Err(Error::ParkedCredentialMissing { .. })
        ));

        mem.vault()
            .fault(&name, Fault::Unreadable("the keychain is locked".into()));
        assert!(
            matches!(
                load(&ctx, &work(), &park),
                Err(Error::Store(crate::store::Error::Unreadable(_)))
            ),
            "a store that could not answer must never read as an absent login"
        );
    }

    /// Two parks of one account in the same millisecond must not share a name: reusing one
    /// would destroy the login already there.
    #[test]
    fn a_reserved_name_steps_past_one_that_is_taken() {
        let (ctx, mem, _scratch) = machine();
        let first = reserve(&ctx, "acc").expect("a free name");
        mem.vault().plant(&first, "{}");
        let second = reserve(&ctx, "acc").expect("another free name");
        assert_ne!(first, second, "the clock has not moved, so the name must");
    }

    /// A login with nothing to restore is refused before it is written, so the vault never
    /// holds a park that could not be used.
    #[test]
    fn a_login_with_no_refresh_token_is_never_parked() {
        let (ctx, mem, _scratch) = machine();
        let name = reserve(&ctx, "acc").expect("a free name");
        assert!(matches!(
            store_at(
                &ctx,
                ProviderId::Claude,
                &name,
                &json!({"accessToken": "a"})
            ),
            Err(Error::LiveCredentialShapeUnexpected { .. })
        ));
        assert!(mem.vault().services().is_empty());
    }

    #[test]
    fn a_park_records_when_its_login_stops_working() {
        let park = describe(
            ProviderId::Claude,
            "pitboard-park-x-1",
            50,
            &serde_json::json!({
                "refreshToken": "r",
                "expiresAt": 1_790_000_000_123i64,
                "refreshTokenExpiresAt": 1_792_000_000_999i64
            }),
        );
        assert_eq!(park.access_expires_at, Some(1_790_000_000));
        assert_eq!(park.refresh_expires_at, Some(1_792_000_000));
        assert_eq!(
            park.refresh_fingerprint,
            crate::provider::claude::document::fingerprint_of(&serde_json::json!({
                "refreshToken": "r"
            }))
        );
    }

    #[test]
    fn service_names_carry_the_account_and_the_moment() {
        let s = service_name("1f0e2d3c-4b5a-4968-8776-a5b4c3d2e1f0", 1789935600123);
        assert_eq!(
            s,
            "pitboard-park-1f0e2d3c-4b5a-4968-8776-a5b4c3d2e1f0-1789935600123"
        );
        assert!(
            s.starts_with("pitboard-park-"),
            "must never collide with a Claude Code item"
        );
    }

    /// Its fingerprint would be the empty string, which every later check would match.
    #[test]
    fn a_credential_with_no_refresh_token_is_refused_rather_than_parked() {
        let refused = store_at(
            &Context::from_env(),
            ProviderId::Claude,
            "pitboard-park-test-no-refresh",
            &serde_json::json!({"accessToken": "a"}),
        );
        assert!(refused.is_err());
    }
}
