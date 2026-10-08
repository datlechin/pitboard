//! The engine behind Pitboard: parking and restoring a person's own Claude Code and Codex
//! logins, and reading what each has left. It serves Pitboard's own front ends, the command
//! line and the native apps, which reach it through [`service::Pitboard`] with an explicit
//! [`context::Context`].
//!
//! # What is supported
//!
//! This crate is published because the `pitboard` binary depends on it, not because it was
//! designed for other programs to build on. The supported interface is [`service::Pitboard`],
//! [`context::Context`], and the types those two return. Everything else is reachable so the
//! front ends in this repository can reach it, and may change in any release.
//!
//! What a version promises, for the part that is supported: a code is a name, and names are
//! kept. Adding an error, warning or check code is not a breaking change, which is why every
//! enum a caller reads codes out of is `#[non_exhaustive]` and every such caller needs a
//! fallback arm. Renaming or removing a code is a breaking change: a new minor version
//! while Pitboard is at 0.x, as Cargo reads one, and a new major version after 1.0.
//! A report a caller reads, such as what `uninstall` returns, may likewise say more in a
//! later release, so those structs are `#[non_exhaustive]` too.
//!
//! What is deliberately not reachable: nothing outside this crate may write Pitboard's index.
//! Every change goes through [`switch`], which records what it is about to do first and
//! finishes an interrupted one before starting another.

// Pitboard for Windows is being built, and reaches people only once the command line, the
// app, their installers and their docs are done. Until then a Windows build of a release
// compiles and refuses everything at run time (`release`). It compiles only for the two
// targets its builds are made and tested on: an x64 or ARM64 PC with the MSVC toolchain.
// A 32-bit, GNU, UWP or Arm64EC build is another program nobody has run.
#[cfg(all(
    windows,
    not(all(
        target_vendor = "pc",
        target_env = "msvc",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))
))]
compile_error!(
    "Pitboard for Windows is built for x86_64-pc-windows-msvc and aarch64-pc-windows-msvc \
     alone, the targets it is built and tested on."
);

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
compile_error!(
    "Pitboard runs on macOS, Linux and Windows. Another system needs a host of its own in \
     `host/`, saying where its stores, processes and scheduler are."
);

pub mod api;
pub mod app;
pub mod assumptions;
pub mod audit;
pub mod autoswitch;
pub mod budget;
pub mod context;
pub mod doctor;
pub mod error;
pub mod in_use;
pub mod label;
pub mod pace;
pub mod provider;
pub mod redact;
pub mod release;
pub mod schedule;
pub mod service;
pub mod settings;
pub mod state;
pub mod status;
pub mod statusline;
pub mod switch;
pub mod time;
pub mod usage;
pub mod words;

pub(crate) mod atomic;
pub(crate) mod fault;
pub mod holder;
pub(crate) mod home;
pub mod host;
pub(crate) mod lock;
pub(crate) mod park;
pub(crate) mod pending;
pub(crate) mod proxy;
pub(crate) mod readings;
pub(crate) mod sessions;
/// The program the tests start in place of a tool, reached as `testing::stand_in`. Its own
/// tests run with the core's.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub mod stand_in;
pub(crate) mod store;

/// What the integration tests reach into: they plant and inspect parked logins in the real
/// store, and must never touch the credential slot this machine's Claude Code reads.
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub mod testing {
    pub use crate::api::scripted::{Answer, Asked, ScriptedApi, Trouble};
    /// What a test or a fixture does to a file's access, or to make a link, which only this
    /// system's face says how to do: the integration tests and the apps' fixtures do it here,
    /// as the core's own tests do.
    pub use crate::host::fs::testing as fs;
    pub use crate::host::memory::MemoryHost;
    pub use crate::provider::claude::paths::live_service;
    pub use crate::provider::claude::slot::{LIVE_SERVICE, dir_hash, service_for_dir};
    /// The program the integration tests, the model's tests and the fixtures' tests start in
    /// place of `claude`, `codex` and every other program they run, on every system.
    pub use crate::stand_in;
    pub use crate::store::memory::{Fault, MemoryStore};
    pub use crate::store::vault_read;
    pub use crate::switch::{ScriptedSignIn, SignInScript};
    pub use crate::time::{Clock, FixedClock};

    /// Every variable Pitboard reads from its environment, by name, which a test withholds
    /// from every command it runs unless it means to pass one on.
    pub fn variables() -> impl Iterator<Item = &'static str> {
        crate::context::variables()
    }

    pub fn real_login_places(
        started_in: &crate::context::Context,
    ) -> Result<Vec<std::path::PathBuf>, String> {
        use crate::host::user;
        let home = user::accounts_own_home().ok_or("this system names no home for this account")?;
        let pitboard = user::accounts_own_pitboard_home()
            .ok_or("this system names no Pitboard folder for this account")?;
        let own = crate::context::Context::new(home).with_pitboard_home(pitboard);
        Ok([&own, started_in]
            .into_iter()
            .flat_map(|ctx| {
                [
                    crate::provider::claude::paths::config_dir(ctx),
                    crate::provider::claude::paths::config_file(ctx),
                    crate::provider::codex::paths::home(ctx),
                    ctx.pitboard_home.clone(),
                ]
            })
            .filter(|place| place.is_absolute())
            .collect())
    }

    /// Parks `contents` under `service` in this context's vault, as a change does, once the
    /// gate every change passes has let it: a test that runs as root is refused here too.
    pub fn vault_write(
        ctx: &crate::context::Context,
        service: &str,
        contents: &str,
    ) -> crate::error::Result<()> {
        let permit = crate::service::gate(ctx)?;
        Ok(crate::store::vault_write(ctx, permit, service, contents)?)
    }

    /// Deletes what is parked under `service`, once the gate has let it.
    pub fn vault_delete(ctx: &crate::context::Context, service: &str) -> crate::error::Result<()> {
        let permit = crate::service::gate(ctx)?;
        Ok(crate::store::vault_delete(ctx, permit, service)?)
    }

    /// A sign-in to `which`'s tool that finished with `document`, the login the tool would
    /// have stored, as JSON, and never ran the tool: what enrolling a second account by
    /// signing in privately needs, in a test that may run no tool. Made once the gate has let
    /// it, as every sign-in is.
    pub fn signed_in(
        ctx: &crate::context::Context,
        which: crate::provider::ProviderId,
        document: &str,
    ) -> crate::error::Result<crate::switch::SignIn> {
        let document = serde_json::from_str(document).expect("a login a test wrote as JSON");
        crate::switch::planted(ctx, crate::service::gate(ctx)?, which, document)
    }
}
