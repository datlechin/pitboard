//! pitboard's internals, exposed so integration tests exercise the real code paths.

pub mod api;
pub mod atomic;
pub mod audit;
pub mod claude;
pub mod configfile;
pub mod doctor;
pub mod error;
pub mod home;
pub mod lock;
pub mod park;
pub mod readings;
pub mod slot;
pub mod state;
pub mod status;
pub mod statusline;
pub mod store;
pub mod switch;
pub mod time;
pub mod ui;
pub mod usage;
