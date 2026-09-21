//! pitboard's internals, exposed so integration tests exercise the real code paths.

pub mod atomic;
pub mod claude;
pub mod configfile;
pub mod doctor;
pub mod error;
pub mod hex;
pub mod home;
pub mod lock;
pub mod park;
pub mod slot;
pub mod state;
pub mod status;
pub mod store;
pub mod switch;
pub mod time;
pub mod usage;
