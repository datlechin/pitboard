//! What Pitboard does by itself, for a front end somebody asked to: the app with its setting
//! on, or `pitboard watch` running in a terminal.
//!
//! Each thing it does is a duty: a rule that judges from what Pitboard already knows, a look
//! at its files that waits on nobody, and an act under the lock every change takes. The
//! automatic switch ([`crate::duty::autoswitch`]) and the warm-up ([`crate::duty::warm`]) are two. What
//! every duty shares is here: how often a front end decides, how long something that came to
//! nothing waits before it is tried again, how what was tried is kept between runs, and when
//! another front end wrote what a duty decides from ([`crate::duty::written`]). What each duty
//! decides, and what it does, is its own.

pub mod autoswitch;
pub mod cadence;
pub(crate) mod ledger;
pub mod warm;
pub mod written;

/// What a duty's look at the files alone came to.
#[derive(Debug)]
pub enum Look<T> {
    /// This stands, and nothing is to be done under the lock.
    Stands(Box<T>),
    /// Something may be due: only a decision under the lock can say.
    Act,
}
