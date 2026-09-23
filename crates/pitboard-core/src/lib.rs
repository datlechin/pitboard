//! The engine behind pitboard: parking and restoring a person's own Claude Code logins, and
//! reading what each has left. It serves pitboard's own front ends, the command line and the
//! native apps, which reach it through [`service::Pitboard`] with an explicit
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
//! fallback arm. Renaming or removing a code is a breaking change and gets a major version.
//!
//! What is deliberately not reachable: nothing outside this crate may write pitboard's index.
//! Every change goes through [`switch`], which records what it is about to do first and
//! finishes an interrupted one before starting another.

#[cfg(not(unix))]
compile_error!(
    "pitboard supports macOS and Linux. Claude Code stores its login differently on \
     Windows, and pitboard has not been written for it."
);

pub mod api;
pub mod assumptions;
pub mod audit;
pub mod budget;
pub mod context;
pub mod doctor;
pub mod error;
pub mod redact;
pub mod schedule;
pub mod service;
pub mod settings;
pub mod state;
pub mod status;
pub mod statusline;
pub mod switch;
pub mod time;
pub mod usage;

pub(crate) mod atomic;
pub(crate) mod claude;
pub(crate) mod configfile;
pub(crate) mod daemon;
pub(crate) mod fault;
pub mod history;
pub(crate) mod home;
pub(crate) mod lock;
pub(crate) mod park;
pub(crate) mod pending;
#[cfg(target_os = "macos")]
pub(crate) mod process;
pub(crate) mod readings;
pub(crate) mod slot;
pub(crate) mod store;

/// What the integration tests reach into: they plant and inspect parked logins in the real
/// store, and must never touch the credential slot this machine's Claude Code reads.
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub mod testing {
    pub use crate::api::scripted::{Answer, Asked, ScriptedApi, Trouble};
    pub use crate::claude::live_service;
    pub use crate::slot::{LIVE_SERVICE, dir_hash, service_for_dir};
    pub use crate::store::memory::{Fault, MemoryHost, MemoryStore};
    pub use crate::store::{vault_delete, vault_read, vault_write};
    pub use crate::time::{Clock, FixedClock};
}
