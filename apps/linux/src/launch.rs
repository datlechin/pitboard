//! Whether a launch starts anything, decided before any `gio::Application` exists.
//!
//! Registering one owns the build's name on the session bus, and from then on D-Bus
//! activation, a link, a notification or a search can reach the program, which runs in the
//! session's environment and so on its real homes. With no model linked yet, every launch is
//! refused here, before anything reaches the bus.

use std::process::ExitCode;

pub const NOTHING_TO_RUN: &str =
    "this build of the Linux app has no model to run yet, so it starts nothing.";

pub fn run() -> ExitCode {
    eprintln!("pitboard-app: {NOTHING_TO_RUN}");
    ExitCode::FAILURE
}
