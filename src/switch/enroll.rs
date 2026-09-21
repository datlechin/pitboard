//! Bringing an account pitboard has not seen before under its care.

use super::{Error, Result, exclusive, oauth_of};
use crate::state::Account;
use crate::{claude, park, state, store};
use serde_json::Value;

/// Record the account that is signed in now, and park a copy of its credential.
///
/// Parking here rather than only at the next switch is what makes the account survive the
/// user signing in as a different one, which overwrites the live credential.
/// Record the account that is signed in now, and park a copy of its credential.
///
/// Parking here rather than only at the next switch is what makes the account survive the
/// user signing in as a different one, which overwrites the live credential.
pub fn current(label: &str) -> Result<Account> {
    let _exclusive = exclusive()?;
    let mut state = state::load()?;
    let config = claude::load_config()?;
    let identity = claude::identity(&config).ok_or(Error::NotSignedIn)?;

    if let Some(existing) = state.by_uuid(&identity.account_uuid)
        && existing.label != label
    {
        return Err(Error::AlreadyEnrolled {
            email: identity.email.clone(),
            label: existing.label.clone(),
        });
    }
    if let Some(taken) = state.get(label)
        && taken.account_uuid != identity.account_uuid
    {
        return Err(Error::LabelTaken {
            label: label.to_string(),
            email: taken.email.clone(),
        });
    }

    let mut oauth_account = config
        .get("oauthAccount")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));
    if let Some(object) = oauth_account.as_object_mut() {
        object.remove("profileFetchedAt");
    }

    let service = claude::live_service();
    let live = store::read(&service)?.ok_or(Error::LiveCredentialAbsent)?;

    let park_service = park::reserve(&identity.account_uuid)?;
    let generation = park::store_at(&park_service, &oauth_of(&live)?)?;

    let mut generations = state
        .by_uuid(&identity.account_uuid)
        .map(|a| a.generations.clone())
        .unwrap_or_default();
    generations.push(generation);

    let account = Account {
        label: label.to_string(),
        account_uuid: identity.account_uuid.clone(),
        email: identity.email.clone(),
        organization_uuid: identity.organization_uuid.clone(),
        oauth_account,
        generations,
    };
    state.upsert(account.clone());
    state.active = Some(label.to_string());
    state::save(&state)?;
    Ok(account)
}
