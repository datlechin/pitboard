//! The engine behind Pitboard: parking and restoring a person's own Claude Code, Codex and
//! Claude Desktop logins, and reading what each Claude Code and Codex account has left. It
//! serves Pitboard's own front ends, the command line and the native apps, which reach it
//! through [`service::Pitboard`] with an explicit [`context::Context`].
//!
//! # What is supported
//!
//! This crate is published because the `pitboard` binary depends on it, not because it was
//! designed for other programs to build on. The supported interface is [`service::Pitboard`],
//! [`context::Context`], and the types those two return. Everything else public is what the
//! front ends in this repository use, and may change in any release.
//!
//! What a version promises, for the part that is supported: a code is a name, and names are
//! kept. Adding an error, warning or check code is not a breaking change, which is why every
//! enum a caller reads codes out of is `#[non_exhaustive]` and every such caller needs a
//! fallback arm. Renaming or removing a code is a breaking change: a new minor version of
//! this crate, whose version is its own, apart from Pitboard's.
//! A report a caller reads, such as what `uninstall` returns, may likewise say more in a
//! later release, so those structs are `#[non_exhaustive]` too.
//!
//! What is deliberately not reachable: nothing outside this crate may write Pitboard's index.
//! Every change goes through [`switch`], which records what it is about to do first.

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

pub(crate) mod api;
pub mod app;
pub mod assumptions;
pub mod audit;
pub mod context;
// The Claude Desktop the tests play, reached as `testing::desktop`.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
#[allow(
    clippy::disallowed_methods,
    reason = "it leaves Claude Desktop's files as Claude does"
)]
pub mod desktop_testing;
pub mod doctor;
pub mod duty;
pub mod error;
pub(crate) mod in_use;
pub mod label;
pub mod provider;
pub mod redact;
pub mod release;
pub mod schedule;
pub mod service;
pub(crate) mod settings;
pub mod state;
pub mod status;
pub mod statusline;
pub mod switch;
pub mod time;
pub mod usage;
pub mod watch_service;
pub mod words;

pub(crate) mod atomic;
pub(crate) mod fault;
#[cfg(test)]
mod helper;
pub mod holder;
pub(crate) mod home;
pub mod host;
pub(crate) mod lock;
pub(crate) mod park;
pub(crate) mod pending;
pub(crate) mod proxy;
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
    pub use crate::desktop_testing as desktop;
    pub use crate::duty::warm::run::{Started, WarmScript};
    pub use crate::host::Ran;
    /// What a test or a fixture does to a file's access, or to make a link, which only this
    /// system's face says how to do: the integration tests and the apps' fixtures do it here,
    /// as the core's own tests do.
    pub use crate::host::fs::testing as fs;
    pub use crate::host::memory::MemoryHost;
    pub use crate::provider::claude::api::Renewed;
    pub use crate::provider::claude::paths::live_service;
    pub use crate::provider::claude::slot::{LIVE_SERVICE, dir_hash, service_for_dir};
    pub use crate::provider::jwt::unsigned as unsigned_token;
    /// The program the integration tests, the model's tests and the fixtures' tests start in
    /// place of `claude`, `codex` and every other program they run, on every system.
    pub use crate::stand_in;
    pub use crate::store::memory::{Fault, MemoryStore};
    pub use crate::store::vault_read;
    pub use crate::switch::enroll::{ScriptedSignIn, SignInScript};
    pub use crate::time::{Clock, FixedClock};

    /// Every variable Pitboard reads from its environment, by name, which a test withholds
    /// from every command it runs unless it means to pass one on.
    pub fn variables() -> impl Iterator<Item = &'static str> {
        crate::context::variables()
    }

    pub fn park_unread(label: &str, path: &std::path::Path, why: &str) -> crate::error::Error {
        crate::store::folder::unread(label, path, why)
    }

    // The one guard every test's folder is refused by.
    pub fn lies_in(path: &std::path::Path, place: &std::path::Path) -> bool {
        crate::host::lies_in(path, place)
    }

    pub fn real_login_places(
        started_in: &crate::context::Context,
    ) -> Result<Vec<std::path::PathBuf>, String> {
        crate::host::real_login_places(Some(started_in))
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

    pub fn may_be_running(pid: u32) -> bool {
        crate::host::proc::may_be_running(pid)
    }

    // Every process's id, its parent's and its program's name, from one snapshot.
    #[cfg(windows)]
    pub fn processes() -> std::io::Result<Vec<(u32, u32, String)>> {
        crate::host::proc::snapshot()
    }

    // A program's version resource, read as Claude Code's is (W17).
    #[cfg(windows)]
    pub mod version_resource {
        use std::path::Path;

        pub fn file_version(program: &Path) -> Option<[u16; 4]> {
            crate::host::proc::file_version(program)
        }

        pub fn product_version(program: &Path) -> Option<[u16; 4]> {
            crate::host::proc::product_version(program)
        }

        pub fn string(program: &Path, name: &str) -> Option<String> {
            crate::host::proc::version_string(program, name)
        }
    }

    /// Claude Code's login in Credential Manager as Pitboard writes, reads and deletes it, under
    /// a test's own service, and each item as Windows keeps it, its blob by length alone.
    #[cfg(windows)]
    pub mod credman {
        use crate::context::Context;
        use crate::store::RawStore;

        pub struct Item {
            pub kind: u32,
            pub persist: u32,
            pub user: String,
            pub comment: Option<String>,
            pub attributes: u32,
            pub bytes: usize,
        }

        fn refused(target: &str) -> std::io::Result<()> {
            match crate::provider::names::refusal(target, &[]) {
                Some(why) => Err(std::io::Error::other(why)),
                None => Ok(()),
            }
        }

        pub fn item(target: &str) -> std::io::Result<Option<Item>> {
            refused(target)?;
            Ok(
                crate::host::windows_credman::item(target)?.map(|held| Item {
                    kind: held.kind,
                    persist: held.persist,
                    user: held.user,
                    comment: held.comment,
                    attributes: held.attributes,
                    bytes: held.blob.len(),
                }),
            )
        }

        // As `Bun.secrets.set` writes one (RF-G3).
        pub fn write_as_bun(target: &str, user: &str, value: &str) -> std::io::Result<()> {
            refused(target)?;
            crate::host::windows_credman::write(target, user, value.as_bytes())
        }

        fn store(ctx: &Context) -> crate::provider::claude::credman::Store {
            crate::provider::claude::credman::Store::of(ctx)
        }

        pub fn write(ctx: &Context, service: &str, contents: &str) -> crate::error::Result<()> {
            let permit = crate::service::gate(ctx)?;
            Ok(store(ctx).write(permit, service, contents)?)
        }

        pub fn read(ctx: &Context, service: &str) -> crate::error::Result<Option<String>> {
            Ok(store(ctx).read(service)?)
        }

        // Every piece too, as Claude Code deletes its login.
        pub fn discard(ctx: &Context, service: &str) -> crate::error::Result<()> {
            let permit = crate::service::gate(ctx)?;
            Ok(store(ctx).delete(permit, service)?)
        }

        // A test's cleanup: every item under `service`, each by name.
        pub fn sweep(ctx: &Context, service: &str) -> Result<(), String> {
            crate::provider::claude::credman::swept(ctx, service)
        }
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
        use crate::provider::LoginKind;
        let document = serde_json::from_str(document).expect("a login a test wrote as JSON");
        match crate::provider::of(which) {
            LoginKind::Document(tool) => {
                crate::switch::planted(ctx, crate::service::gate(ctx)?, tool, document)
            }
            LoginKind::Folder(_) => Err(crate::error::Error::Usage(
                "a Claude Desktop sign-in is played by testing::desktop".into(),
            )),
        }
    }
}
