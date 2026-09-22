//! The engine behind pitboard: parking and restoring a person's own Claude Code logins, and
//! reading what each has left. It serves pitboard's own front ends, the command line and the
//! native apps, which reach it through [`service::Pitboard`] with an explicit
//! [`context::Context`].

#[cfg(not(unix))]
compile_error!(
    "pitboard supports macOS and Linux. Claude Code stores its login differently on \
     Windows, and pitboard has not been written for it."
);

pub mod api;
pub mod audit;
pub mod context;
pub mod doctor;
pub mod error;
pub mod service;
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
pub(crate) mod home;
pub(crate) mod lock;
pub(crate) mod park;
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
    pub use crate::claude::live_service;
    pub use crate::slot::{LIVE_SERVICE, dir_hash, service_for_dir};
    pub use crate::store::{vault_delete, vault_read, vault_write};
}
