//! GNOME Shell's search, as `org.gnome.Shell.SearchProvider2` asks the app at
//! `<app's path>/SearchProvider`, on the app's own connection, whose name the provider's ini gives
//! as its `BusName` (docs/facts/linux-app.md, Search and KRunner). The results are what the app
//! answers for the terms; the provider says what each is, and which was chosen.
//!
//! GNOME Shell asks only a Pitboard that runs, since the ini says `AutoStart=false`, and a choice
//! is told only of a result given since the search began.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use gio::prelude::*;

use crate::resources;

const INTERFACE: &str = "org.gnome.Shell.SearchProvider2";

/// Where under the app's object path the provider answers.
pub const OBJECT: &str = "SearchProvider";

pub fn path(app: &str) -> String {
    format!("{app}/{OBJECT}")
}

/// A result, as GNOME Shell and KRunner each show one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub id: String,
    pub name: String,
    pub description: String,
    // a name in the icon theme
    pub icon: String,
}

// What the app answers for a search's terms.
pub(crate) type Answers = Box<dyn Fn(&[String]) -> Vec<Found>>;

/// What GNOME Shell asks of the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// The result chosen, by its id.
    Activate(String),
    /// The app itself, for a search of its own, which Pitboard answers with its window.
    Open,
}

/// The provider on the bus, until dropped.
pub struct Provider {
    connection: gio::DBusConnection,
    registration: Option<gio::RegistrationId>,
}

struct Searching {
    results: Answers,
    told: Box<dyn Fn(Asked)>,
    // each result given since the search began, by its id
    given: RefCell<HashMap<String, Found>>,
}

impl Provider {
    /// Serves the provider under `app`, the app's object path, with the results `results`
    /// answers for a search's terms.
    pub fn serve(
        connection: &gio::DBusConnection,
        app: &str,
        results: impl Fn(&[String]) -> Vec<Found> + 'static,
        told: impl Fn(Asked) + 'static,
    ) -> Result<Provider, glib::Error> {
        let searching = Rc::new(Searching {
            results: Box::new(results),
            told: Box::new(told),
            given: RefCell::default(),
        });
        let registration = connection
            .register_object(&path(app), &resources::interface(INTERFACE))
            .typed_method_call::<Call>()
            .invoke(move |_, _, call, invocation| searching.answer(call, invocation))
            .build()?;
        Ok(Provider {
            connection: connection.clone(),
            registration: Some(registration),
        })
    }
}

impl Drop for Provider {
    fn drop(&mut self) {
        if let Some(registration) = self.registration.take() {
            let _ = self.connection.unregister_object(registration);
        }
    }
}

impl Searching {
    // Answered before the app hears of a choice.
    fn answer(&self, call: Call, invocation: gio::DBusMethodInvocation) {
        match call {
            Call::GetInitialResultSet { terms } => {
                let found = (self.results)(&terms);
                self.given.borrow_mut().clear();
                invocation.return_value(Some(&(self.give(found),).to_variant()));
            }
            Call::GetSubsearchResultSet { previous, terms } => {
                // A subsearch narrows the results before it, so it answers none but theirs.
                let found = (self.results)(&terms)
                    .into_iter()
                    .filter(|found| previous.contains(&found.id))
                    .collect();
                invocation.return_value(Some(&(self.give(found),).to_variant()));
            }
            Call::GetResultMetas { ids } => {
                let given = self.given.borrow();
                let metas: Vec<BTreeMap<String, glib::Variant>> = ids
                    .iter()
                    .filter_map(|id| given.get(id))
                    .map(meta)
                    .collect();
                invocation.return_value(Some(&(metas,).to_variant()));
            }
            Call::ActivateResult { id } => {
                let given = self.given.borrow().contains_key(&id);
                invocation.return_value(None);
                if given {
                    (self.told)(Asked::Activate(id));
                }
            }
            Call::LaunchSearch => {
                invocation.return_value(None);
                (self.told)(Asked::Open);
            }
        }
    }

    fn give(&self, found: Vec<Found>) -> Vec<String> {
        let mut given = self.given.borrow_mut();
        found
            .into_iter()
            .map(|found| {
                let id = found.id.clone();
                given.insert(id.clone(), found);
                id
            })
            .collect()
    }
}

/// The provider's methods, as its interface declares them.
enum Call {
    GetInitialResultSet {
        terms: Vec<String>,
    },
    GetSubsearchResultSet {
        previous: Vec<String>,
        terms: Vec<String>,
    },
    GetResultMetas {
        ids: Vec<String>,
    },
    ActivateResult {
        id: String,
    },
    LaunchSearch,
}

impl DBusMethodCall for Call {
    // GDBus has checked the parameters against the interface before a call reaches here.
    fn parse_call(
        _: &str,
        _: Option<&str>,
        method: &str,
        parameters: glib::Variant,
    ) -> Result<Call, glib::Error> {
        match method {
            "GetInitialResultSet" => parameters
                .get::<(Vec<String>,)>()
                .map(|(terms,)| Call::GetInitialResultSet { terms }),
            "GetSubsearchResultSet" => parameters
                .get::<(Vec<String>, Vec<String>)>()
                .map(|(previous, terms)| Call::GetSubsearchResultSet { previous, terms }),
            "GetResultMetas" => parameters
                .get::<(Vec<String>,)>()
                .map(|(ids,)| Call::GetResultMetas { ids }),
            "ActivateResult" => parameters
                .get::<(String, Vec<String>, u32)>()
                .map(|(id, _, _)| Call::ActivateResult { id }),
            "LaunchSearch" => Some(Call::LaunchSearch),
            _ => None,
        }
        .ok_or_else(|| {
            glib::Error::new(
                gio::DBusError::UnknownMethod,
                &format!("{INTERFACE} has no {method}"),
            )
        })
    }
}

// The icon as GNOME Shell takes it back with `g_icon_deserialize`.
fn meta(found: &Found) -> BTreeMap<String, glib::Variant> {
    let mut meta = BTreeMap::from([
        ("id".to_owned(), found.id.to_variant()),
        ("name".to_owned(), found.name.to_variant()),
        ("description".to_owned(), found.description.to_variant()),
    ]);
    if let Some(icon) = gio::ThemedIcon::new(&found.icon).serialize() {
        meta.insert("icon".to_owned(), icon);
    }
    meta
}
