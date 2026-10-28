//! What the app asks of the desktop for the model: notifications, start at sign-in, waking
//! from sleep, links, the status item, and what Uninstall takes back of the app; and what the
//! desktop asks of the app: GNOME's search and KRunner.
//!
//! Each takes and gives plain values of the glue's own, so nothing here links the model. A
//! service a fixture must keep off the desktop is a trait, which the desktop's own and
//! [`stand_in`]'s each implement: in a fixture, the app posts, writes, shows and opens nothing
//! outside the fixture's folder.

pub mod autostart;
pub mod krunner;
pub mod leftovers;
pub mod links;
pub mod logind;
pub mod notifications;
pub mod search;
pub mod stand_in;
pub mod tray;

use std::fmt;

/// What a service of the desktop's could not do, in a sentence the model can carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failed(pub String);

impl fmt::Display for Failed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Failed {}
