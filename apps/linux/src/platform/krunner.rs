//! KRunner, as `org.kde.krunner1` asks the app at `<app's path>/Runner`, on the app's own
//! connection: KRunner follows each registered name its metadata's `com.usepitboard.Pitboard*`
//! prefixes, and asks only those (docs/facts/linux-app.md, Search and KRunner). A query's words
//! are the terms the app answers results for, as GNOME Shell's are.
//!
//! A run is told only of a match given since the session began, with the activation token
//! KRunner set just before it.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use gio::prelude::*;

use super::search::{Answers, Found};
use crate::resources;

const INTERFACE: &str = "org.kde.krunner1";

/// Where under the app's object path the runner answers.
pub const OBJECT: &str = "Runner";

// KRunner 6's Moderate, below the applications Plasma 6.3.6 finds by their name
// (`runners/services/servicerunner.cpp:255-290`).
const CATEGORY_RELEVANCE: i32 = 50;

pub fn path(app: &str) -> String {
    format!("{app}/{OBJECT}")
}

/// A match KRunner ran, by its id, with the token KRunner set for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub id: String,
    pub token: Option<String>,
}

/// The runner on the bus, until dropped.
pub struct Runner {
    connection: gio::DBusConnection,
    registration: Option<gio::RegistrationId>,
}

struct Running {
    results: Answers,
    told: Box<dyn Fn(Run)>,
    // each match given since the session began
    given: RefCell<HashSet<String>>,
    token: RefCell<Option<String>>,
}

impl Runner {
    /// Serves the runner under `app`, the app's object path, with the results `results`
    /// answers for a query's terms.
    pub fn serve(
        connection: &gio::DBusConnection,
        app: &str,
        results: impl Fn(&[String]) -> Vec<Found> + 'static,
        told: impl Fn(Run) + 'static,
    ) -> Result<Runner, glib::Error> {
        let running = Rc::new(Running {
            results: Box::new(results),
            told: Box::new(told),
            given: RefCell::default(),
            token: RefCell::default(),
        });
        let registration = connection
            .register_object(&path(app), &resources::interface(INTERFACE))
            .typed_method_call::<Call>()
            .invoke(move |_, _, call, invocation| running.answer(call, invocation))
            .build()?;
        Ok(Runner {
            connection: connection.clone(),
            registration: Some(registration),
        })
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        if let Some(registration) = self.registration.take() {
            let _ = self.connection.unregister_object(registration);
        }
    }
}

type Match = (
    String,
    String,
    String,
    i32,
    f64,
    BTreeMap<String, glib::Variant>,
);

impl Running {
    // Answered before the app hears of a run.
    fn answer(&self, call: Call, invocation: gio::DBusMethodInvocation) {
        match call {
            Call::Match { query } => {
                let terms: Vec<String> = query.split_whitespace().map(String::from).collect();
                let found = (self.results)(&terms);
                let count = found.len();
                let mut given = self.given.borrow_mut();
                let matches: Vec<Match> = found
                    .into_iter()
                    .enumerate()
                    .map(|(at, found)| {
                        given.insert(found.id.clone());
                        let subtext = BTreeMap::from([(
                            "subtext".to_owned(),
                            found.description.to_variant(),
                        )]);
                        let relevance = relevance(at, count);
                        let Found { id, name, icon, .. } = found;
                        (id, name, icon, CATEGORY_RELEVANCE, relevance, subtext)
                    })
                    .collect();
                invocation.return_value(Some(&(matches,).to_variant()));
            }
            // Pitboard offers nothing beside a match's own run.
            Call::Actions => {
                let actions = Vec::<(String, String, String)>::new();
                invocation.return_value(Some(&(actions,).to_variant()));
            }
            Call::Config => {
                let config = BTreeMap::<String, glib::Variant>::new();
                invocation.return_value(Some(&(config,).to_variant()));
            }
            Call::Teardown => {
                self.given.borrow_mut().clear();
                self.token.take();
                invocation.return_value(None);
            }
            Call::SetActivationToken { token } => {
                self.token.replace(Some(token));
                invocation.return_value(None);
            }
            Call::Run { id, action } => {
                let given = action.is_empty() && self.given.borrow().contains(&id);
                invocation.return_value(None);
                if given {
                    let token = self.token.take();
                    (self.told)(Run { id, token });
                }
            }
        }
    }
}

/// The runner's methods, as its interface declares them.
enum Call {
    Match { query: String },
    Actions,
    Config,
    Teardown,
    SetActivationToken { token: String },
    Run { id: String, action: String },
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
            "Match" => parameters
                .get::<(String,)>()
                .map(|(query,)| Call::Match { query }),
            "Actions" => Some(Call::Actions),
            "Config" => Some(Call::Config),
            "Teardown" => Some(Call::Teardown),
            "SetActivationToken" => parameters
                .get::<(String,)>()
                .map(|(token,)| Call::SetActivationToken { token }),
            "Run" => parameters
                .get::<(String, String)>()
                .map(|(id, action)| Call::Run { id, action }),
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

// KRunner sorts a category's matches by their relevance, from 0 to 1, so the results keep the
// order they came in.
fn relevance(at: usize, count: usize) -> f64 {
    (count - at) as f64 / count as f64
}
