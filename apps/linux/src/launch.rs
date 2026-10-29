//! Whether a launch starts anything, decided before any `gio::Application` exists.
//!
//! Registering one owns the build's name on the session bus, and from then on D-Bus
//! activation, a link, a notification or a search can reach the program, which runs in the
//! session's environment and so on its real homes. So the model is asked how the launch was
//! started first: a debug build that is not in a fixture quits here, as D-Bus activation,
//! autostart and a link would start one, and so does one in a fixture it cannot make, before
//! anything reaches the bus. Only then is the app made and run.

use std::collections::HashMap;
use std::process::ExitCode;

use gio::prelude::*;
use pitboard_model::{Front, Started};

use crate::application::{Application, Launched};
use crate::config::{self, Build};
use crate::platform::links;

pub const OUTSIDE_A_FIXTURE: &str = "a debug build runs only in the fixture PITBOARD_FIXTURE \
     names, as Run the Linux debug build in CONTRIBUTING.md says: started any other way, it \
     would run on this account's real homes.";

pub fn run() -> ExitCode {
    let environment = environment();
    let build = match config::BUILD {
        Build::Debug => pitboard_model::Build::Debug,
        Build::Release => pitboard_model::Build::Release,
    };
    let launched = match pitboard_model::started(build, environment.clone()) {
        Started::Live => Launched::Live { environment },
        Started::OutsideFixture => return refused(OUTSIDE_A_FIXTURE),
        Started::Fixture { name } => match in_fixture(name) {
            Ok(launched) => launched,
            Err(reason) => return refused(&reason),
        },
    };
    // In every process, since a second launch makes the files it hands the first.
    links::hand_on_as_written(config::LINK_SCHEME);
    let app = Application::new(launched);
    let ran = app.run();
    if app.failed() {
        return ExitCode::FAILURE;
    }
    ExitCode::from(ran.get())
}

#[allow(
    clippy::disallowed_methods,
    reason = "the model reads the app's environment through the core's context, which takes \
              every variable the app was started with, as each app hands it"
)]
fn environment() -> HashMap<String, String> {
    (std::env::vars_os())
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
        .collect()
}

#[cfg(feature = "fixture")]
fn in_fixture(name: String) -> Result<Launched, String> {
    match pitboard_model::fixture_refusal(Front::Gnome, &name) {
        Some(refused) => Err(refused.to_string()),
        None => Ok(Launched::Fixture { name }),
    }
}

#[cfg(not(feature = "fixture"))]
fn in_fixture(name: String) -> Result<Launched, String> {
    let refused = pitboard_model::fixture_refusal(Front::Gnome, &name)
        .expect("a build without the fixtures refuses every one");
    Err(refused.to_string())
}

fn refused(reason: &str) -> ExitCode {
    eprintln!("pitboard-app: {reason}");
    ExitCode::FAILURE
}
