//! Pitboard's parked logins on Linux as items of the Secret Service, the store the session's
//! keyring serves on its bus: gnome-keyring, KDE's ksecretd or oo7. One item per park, in the
//! collection the `default` alias names, which every home of the session shares, as every home
//! on a Mac shares the login keychain.
//!
//! Spoken through `secret-service`, a DH session over zbus 5's blocking API, one connection
//! for each question, on the session's bus as `bus::session` finds it. Read in the Secret
//! Service API 0.2 draft and measured with gnome-keyring 48.0 (`docs/facts/linux.md`).

use super::bus;
use crate::context::Context;
use crate::permit::Permit;
use crate::store::{Backend, Error, RawStore};
use ::secret_service::EncryptionType;
use ::secret_service::blocking::{Collection, Item, SecretService};
use std::collections::HashMap;

// libsecret's attribute for an item's schema, which `secret-tool` and Seahorse read.
const SCHEMA: (&str, &str) = ("xdg:schema", "com.usepitboard.Pitboard.Park");
const SERVICE: &str = "service";
const LABEL: &str = "Pitboard parked login";
// What it holds; gnome-keyring 48.0 reads it back as `text/plain`, so nothing is told by it.
const CONTENT_TYPE: &str = "application/json";

/// The parks every home of this session keeps in the Secret Service.
pub(super) struct Vault {
    ctx: Context,
}

impl Vault {
    pub(super) fn of(ctx: &Context) -> Vault {
        Vault { ctx: ctx.clone() }
    }

    // What `then` makes of the default collection, a failure said as `otherwise` says one.
    fn open<T>(
        &self,
        otherwise: fn(String) -> Error,
        then: impl FnOnce(&Collection<'_>) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let connection = bus::session(&self.ctx).map_err(otherwise)?;
        let service = SecretService::connect_with_existing(EncryptionType::Dh, connection)
            .map_err(said(otherwise))?;
        let collection = service.get_default_collection().map_err(said(otherwise))?;
        then(&collection)
    }
}

/// Whether a Secret Service on this session's bus has a default collection to park in, as a
/// new home asks before it records its form.
pub(super) fn answers(ctx: &Context) -> bool {
    Vault::of(ctx).open(Error::Unreadable, |_| Ok(())).is_ok()
}

fn attributes(service: &str) -> HashMap<&str, &str> {
    HashMap::from([SCHEMA, (SERVICE, service)])
}

// Two items of one park are none Pitboard wrote: a write replaces the one there.
fn secret_of(collection: &Collection<'_>, service: &str) -> Result<Option<String>, Error> {
    let items = collection
        .search_items(attributes(service))
        .map_err(said(Error::Unreadable))?;
    match items.as_slice() {
        [] => Ok(None),
        [item] => {
            let secret = item.get_secret().map_err(said(Error::Unreadable))?;
            String::from_utf8(secret)
                .map(Some)
                .map_err(|_| Error::Unreadable(format!("{service} does not hold text")))
        }
        several => Err(Error::Unreadable(format!(
            "{} items of the Secret Service hold {service}",
            several.len()
        ))),
    }
}

// What the Secret Service said, as a store's error.
fn said(otherwise: fn(String) -> Error) -> impl Fn(::secret_service::Error) -> Error {
    move |error| otherwise(format!("the Secret Service: {error}"))
}

impl RawStore for Vault {
    fn kind(&self) -> Backend {
        Backend::SecretService
    }

    fn contains(&self, service: &str) -> Result<bool, Error> {
        self.open(Error::Unreadable, |collection| {
            let items = collection
                .search_items(attributes(service))
                .map_err(said(Error::Unreadable))?;
            Ok(!items.is_empty())
        })
    }

    fn read(&self, service: &str) -> Result<Option<String>, Error> {
        self.open(Error::Unreadable, |collection| {
            secret_of(collection, service)
        })
    }

    // `replace` has the provider replace the item these attributes name.
    fn write(&self, _: Permit, service: &str, contents: &str) -> Result<(), Error> {
        let back = self.open(Error::Write, |collection| {
            collection
                .create_item(
                    LABEL,
                    attributes(service),
                    contents.as_bytes(),
                    true,
                    CONTENT_TYPE,
                )
                .map_err(said(Error::Write))?;
            secret_of(collection, service)
        })?;
        match back {
            Some(back) if back == contents => Ok(()),
            _ => Err(Error::NotDurable(format!(
                "the Secret Service does not hold what was written to {service}"
            ))),
        }
    }

    fn delete(&self, _: Permit, service: &str) -> Result<(), Error> {
        self.open(Error::Write, |collection| {
            let items = collection
                .search_items(attributes(service))
                .map_err(said(Error::Write))?;
            items
                .iter()
                .try_for_each(Item::delete)
                .map_err(said(Error::Write))
        })
    }

    /// Every park of Pitboard's schema in the collection, whichever home wrote it.
    fn list(&self) -> Result<Option<Vec<String>>, Error> {
        self.open(Error::Unreadable, |collection| {
            let items = collection
                .search_items(HashMap::from([SCHEMA]))
                .map_err(said(Error::Unreadable))?;
            let mut names = Vec::new();
            for item in &items {
                let mut attributes = item.get_attributes().map_err(said(Error::Unreadable))?;
                if let Some(name) = attributes
                    .remove(SERVICE)
                    .filter(|name| crate::park::is_park_name(name))
                {
                    names.push(name);
                }
            }
            names.sort();
            names.dedup();
            Ok(Some(names))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret_service_testing::{Item as Held, SecretService as Started};
    use std::collections::BTreeMap;
    use std::path::Path;

    const LOGIN: &str = r#"{"claudeAiOauth":{"refreshToken":"r"}}"#;
    const AGAIN: &str = r#"{"claudeAiOauth":{"refreshToken":"s"}}"#;

    fn started(scratch: &Path) -> Started {
        Started::start(&scratch.join("keyring")).unwrap_or_else(|missing| panic!("{missing}"))
    }

    // A home of its own on the test's bus, as a command a test runs is given it.
    fn home(scratch: &Path, name: &str, on: &Started) -> Context {
        Context::for_unit_test()
            .with_pitboard_home(scratch.join(name))
            .with_test_bus(on.address().to_owned())
    }

    fn park(at: i64) -> String {
        crate::park::service_name("acc", 1_789_935_600_000 + at)
    }

    #[test]
    fn a_park_round_trips_as_one_item_of_pitboards_schema() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let service = started(scratch.path());
        let ctx = home(scratch.path(), "home", &service);
        assert!(answers(&ctx));
        assert_eq!(
            super::super::host().new_home_form(&ctx),
            Some(crate::state::VaultForm::SecretService)
        );
        let (vault, park, permit) = (Vault::of(&ctx), park(0), Permit::for_a_test());
        assert_eq!(vault.kind(), Backend::SecretService);
        assert_eq!(vault.read(&park).expect("read"), None);
        assert!(!vault.contains(&park).expect("looked for"));

        vault.write(permit, &park, LOGIN).expect("written");
        assert_eq!(vault.read(&park).expect("read").as_deref(), Some(LOGIN));
        vault.write(permit, &park, AGAIN).expect("written over");
        assert_eq!(
            service.items(),
            vec![Held {
                label: LABEL.into(),
                attributes: BTreeMap::from([
                    ("service".into(), park.clone()),
                    ("xdg:schema".into(), "com.usepitboard.Pitboard.Park".into()),
                ]),
                secret: AGAIN.into(),
            }]
        );
        assert!(vault.contains(&park).expect("looked for"));

        vault.delete(permit, &park).expect("deleted");
        assert_eq!(vault.read(&park).expect("read"), None);
        assert!(service.items().is_empty());
        vault
            .delete(permit, &park)
            .expect("a park already gone is deleted");
    }

    // As the login keychain on macOS: every home of the session parks in the one collection.
    #[test]
    fn the_list_holds_every_park_on_the_bus_and_nothing_of_another_schema() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let service = started(scratch.path());
        let here = Vault::of(&home(scratch.path(), "here", &service));
        let there = Vault::of(&home(scratch.path(), "there", &service));
        let permit = Permit::for_a_test();
        assert_eq!(here.list().expect("listed"), Some(Vec::new()));
        here.write(permit, &park(1), LOGIN).expect("written");
        there.write(permit, &park(0), LOGIN).expect("written");
        service.plant(
            "Another program's",
            &[("xdg:schema", "org.example.Other"), ("service", &park(2))],
            "theirs",
        );
        service.plant("Not a park", &[SCHEMA, (SERVICE, "pitboard-park-")], "x");

        for vault in [&here, &there] {
            assert_eq!(vault.list().expect("listed"), Some(vec![park(0), park(1)]));
        }
    }
}
