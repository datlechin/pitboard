//! Pitboard's parked logins on Linux as items of the Secret Service, the store the session's
//! keyring serves on its bus: gnome-keyring, KDE's ksecretd or oo7. One item per park, in the
//! collection the `default` alias names, which every home of the session shares, as every home
//! on a Mac shares the login keychain.
//!
//! Spoken through `secret-service`, a DH session over zbus 5's blocking API, one connection
//! for each question, on the session's bus as `bus::session` finds it. Read in the Secret
//! Service API 0.2 draft and measured with gnome-keyring 48.0 (`docs/facts/linux.md`).
//!
//! A locked collection, a prompt dismissed and nothing on the bus to answer say nothing of what
//! the collection holds, so each is `Locked`, never a collection with nothing in it: a keyring
//! locked since its daemon started answers every search with nothing. A run the schedule or the
//! watch service started never asks to unlock it, since nobody is there to answer. A run a person
//! started asks, as `secret-tool` does, only in the desktop session: secret-service waits for a
//! prompt's answer with no bound, and the prompter puts its dialog on the desktop's display, which
//! a run over SSH or with no display is not in front of.

use super::bus;
use crate::context::Context;
use crate::permit::Permit;
use crate::schedule::Job;
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
// What a provider answers for a collection or an item it keeps locked, and the bus where nothing
// owns `org.freedesktop.secrets`.
const LOCKED: [&str; 3] = [
    "org.freedesktop.Secret.Error.IsLocked",
    "org.freedesktop.DBus.Error.ServiceUnknown",
    "org.freedesktop.DBus.Error.NameHasNoOwner",
];

/// The parks every home of this session keeps in the Secret Service.
pub(super) struct Vault {
    ctx: Context,
    // Whether a person started this run in the desktop session, where they can answer the
    // provider's unlock prompt.
    asks: bool,
}

impl Vault {
    pub(super) fn of(ctx: &Context) -> Vault {
        let started_by_a_job = [Job::Renew, Job::Watch]
            .into_iter()
            .any(|job| crate::schedule::started_this_run(ctx, job));
        Vault {
            ctx: ctx.clone(),
            asks: !started_by_a_job && ctx.in_the_desktop_session(),
        }
    }

    // What `then` makes of the default collection, unlocked first, a failure said as `otherwise`
    // says one. Asked again after, since one locked meanwhile answers a search with nothing. A
    // person who dismissed the prompt has answered for the run, so it is not put up again.
    fn open<T>(
        &self,
        otherwise: fn(String) -> Error,
        then: impl FnOnce(&Collection<'_>) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let service = connected(&self.ctx, otherwise)?;
        let collection = service.get_default_collection().map_err(said(otherwise))?;
        if collection.is_locked().map_err(said(otherwise))? {
            if !self.asks || self.ctx.unlock_was_dismissed() {
                return Err(Error::Locked);
            }
            match collection.unlock() {
                Err(::secret_service::Error::Prompt) => {
                    self.ctx.unlock_dismissed();
                    return Err(Error::Locked);
                }
                unlocked => unlocked.map_err(said(otherwise))?,
            }
        }
        let answer = then(&collection)?;
        if collection.is_locked().map_err(said(otherwise))? {
            return Err(Error::Locked);
        }
        Ok(answer)
    }
}

// A session with the Secret Service, where one answers on this session's bus.
fn connected<'a>(
    ctx: &Context,
    otherwise: fn(String) -> Error,
) -> Result<SecretService<'a>, Error> {
    let connection = bus::session(ctx).map_err(|_| Error::Locked)?;
    SecretService::connect_with_existing(EncryptionType::Dh, connection).map_err(said(otherwise))
}

/// Whether a Secret Service on this session's bus has a default collection to park in, locked
/// or not, as a new home asks before it records its form. Nothing is unlocked to tell.
pub(super) fn answers(ctx: &Context) -> bool {
    connected(ctx, Error::Unreadable).is_ok_and(|service| service.get_default_collection().is_ok())
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

// What the Secret Service said, as a store's error: locked, dismissed or not there is `Locked`.
fn said(otherwise: fn(String) -> Error) -> impl Fn(::secret_service::Error) -> Error {
    use ::secret_service::Error as Said;
    move |error| match &error {
        Said::Locked
        | Said::Prompt
        | Said::PromptDisconnected
        | Said::Unavailable
        | Said::NoResult => Error::Locked,
        Said::Zbus(zbus::Error::MethodError(name, ..)) if LOCKED.contains(&name.as_str()) => {
            Error::Locked
        }
        Said::ZbusFdo(fdo) if LOCKED.contains(&zbus::DBusError::name(fdo).as_str()) => {
            Error::Locked
        }
        _ => otherwise(format!("the Secret Service: {error}")),
    }
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

    // The same, for a run a person started in the desktop session, which may put up a prompt.
    fn at_the_desktop(scratch: &Path, name: &str, on: &Started) -> Context {
        let home = scratch.join(name).to_string_lossy().into_owned();
        let env = crate::context::Environment::on(
            crate::host::Os::Linux,
            [("HOME", home.as_str()), ("WAYLAND_DISPLAY", "wayland-0")],
        );
        Context::read(&env, "cli")
            .with_pitboard_home(scratch.join(name).join(".pitboard"))
            .with_test_bus(on.address().to_owned())
    }

    fn park(at: i64) -> String {
        crate::park::service_name("acc", 1_789_935_600_000 + at)
    }

    fn unlocks(service: &Started) -> usize {
        service
            .asked()
            .iter()
            .filter(|member| *member == "Unlock")
            .count()
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

    // Every question, whoever asks: a person's run is refused once its prompt is dismissed.
    fn refused_every_way(vault: &Vault, park: &str) {
        let permit = Permit::for_a_test();
        let locked = |what: &str, answer: Result<String, Error>| match answer {
            Err(Error::Locked) => {}
            other => panic!("{what}: {other:?}"),
        };
        locked("read", vault.read(park).map(|r| format!("{r:?}")));
        locked("look", vault.contains(park).map(|c| c.to_string()));
        locked("list", vault.list().map(|l| format!("{l:?}")));
        locked(
            "write",
            vault.write(permit, park, AGAIN).map(|()| String::new()),
        );
        locked("delete", vault.delete(permit, park).map(|()| String::new()));
    }

    // gnome-keyring 48.0 answers a search of a keyring locked since it started with nothing.
    #[test]
    fn a_locked_collection_is_locked_never_absent() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let service = started(scratch.path());
        let ctx = home(scratch.path(), "home", &service);
        Vault::of(&ctx)
            .write(Permit::for_a_test(), &park(0), LOGIN)
            .expect("written");
        service.lock();

        for ctx in [ctx.clone(), ctx.clone().started_by_the_schedule()] {
            refused_every_way(&Vault::of(&ctx), &park(0));
        }
        assert!(answers(&ctx), "a locked collection is one to park in");
    }

    // The renewal timer and the watch service: nobody is there to answer a prompt.
    #[test]
    fn a_run_a_job_started_never_asks_to_unlock() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let service = started(scratch.path());
        let ctx = home(scratch.path(), "home", &service);
        service.lock();

        for job in [
            ctx.clone().started_by_the_schedule(),
            ctx.clone().started_by_the_watch_service(),
        ] {
            refused_every_way(&Vault::of(&job), &park(0));
        }
        let asked = service.asked();
        assert!(
            !asked
                .iter()
                .any(|member| member == "Unlock" || member == "Prompt"),
            "{asked:?}"
        );
    }

    // Over SSH the prompter shows its dialog on the desktop's display, where nobody who started
    // this run may be, and with no display at all there is nowhere to show it: secret-service
    // 5.2 waits for an answer with no bound.
    #[test]
    fn a_run_out_of_the_desktop_session_never_asks_to_unlock() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let service = started(scratch.path());
        service.lock();
        let home = scratch.path().join("home").to_string_lossy().into_owned();
        let read = |pairs: &[(&str, &str)]| {
            let env = crate::context::Environment::on(
                crate::host::Os::Linux,
                [("HOME", home.as_str())].iter().chain(pairs).copied(),
            );
            Context::read(&env, "cli")
                .with_pitboard_home(scratch.path().join("home/.pitboard"))
                .with_test_bus(service.address().to_owned())
        };

        for away in [
            read(&[]),
            read(&[
                ("DISPLAY", "localhost:10.0"),
                ("SSH_CONNECTION", "10.0.0.2 50022 10.0.0.1 22"),
            ]),
            read(&[("WAYLAND_DISPLAY", ""), ("DISPLAY", "")]),
        ] {
            refused_every_way(&Vault::of(&away), &park(0));
        }
        assert_eq!(unlocks(&service), 0, "{:?}", service.asked());

        let at_the_desktop = read(&[("DISPLAY", ":0")]);
        let read = Vault::of(&at_the_desktop).read(&park(0));
        assert!(matches!(read, Err(Error::Locked)), "{read:?}");
        assert_eq!(unlocks(&service), 1, "{:?}", service.asked());
    }

    // A person who dismisses the prompt has answered for the run: each park doctor reads, and
    // each step of a switch, would put it up again.
    #[test]
    fn a_prompt_dismissed_is_not_put_up_again_in_the_same_run() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let service = started(scratch.path());
        let ctx = at_the_desktop(scratch.path(), "home", &service);
        service.lock();

        refused_every_way(&Vault::of(&ctx), &park(0));
        refused_every_way(&Vault::of(&ctx.clone()), &park(1));
        assert_eq!(unlocks(&service), 1, "{:?}", service.asked());

        let another_run = at_the_desktop(scratch.path(), "home", &service);
        let read = Vault::of(&another_run).read(&park(0));
        assert!(matches!(read, Err(Error::Locked)), "{read:?}");
        assert_eq!(unlocks(&service), 2, "{:?}", service.asked());
    }

    // A bus with no prompter on it, as in a container: gnome-keyring 48.0 dismisses the prompt
    // it cannot show at once.
    #[test]
    fn a_prompt_nobody_can_answer_is_refused_within_seconds() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let service = started(scratch.path());
        let vault = Vault::of(&at_the_desktop(scratch.path(), "home", &service));
        service.lock();

        let asking = std::time::Instant::now();
        let read = vault.read(&park(0));
        let took = asking.elapsed();

        assert!(matches!(read, Err(Error::Locked)), "{read:?}");
        assert!(took < std::time::Duration::from_secs(5), "{took:?}");
        let asked = service.asked();
        assert!(
            asked.iter().any(|member| member == "Unlock")
                && asked.iter().any(|member| member == "Prompt"),
            "a person's run asks: {asked:?}"
        );
    }

    // As over SSH or in a container: a home on the Secret Service cannot tell what it holds.
    #[test]
    fn no_secret_service_on_the_bus_is_locked_never_absent() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let nobody = Started::without_a_provider(&scratch.path().join("nobody"))
            .unwrap_or_else(|missing| panic!("{missing}"));
        let mut stopped = started(scratch.path());
        let ctx = home(scratch.path(), "home", &stopped);
        Vault::of(&ctx)
            .write(Permit::for_a_test(), &park(0), LOGIN)
            .expect("written");
        stopped.stop();

        let without_a_bus =
            Context::for_unit_test().with_pitboard_home(scratch.path().join("bare"));
        for ctx in [home(scratch.path(), "nobody", &nobody), ctx, without_a_bus] {
            refused_every_way(&Vault::of(&ctx), &park(0));
            assert!(!answers(&ctx));
            assert_eq!(
                super::super::host().new_home_form(&ctx),
                Some(crate::state::VaultForm::Files)
            );
        }
    }
}
