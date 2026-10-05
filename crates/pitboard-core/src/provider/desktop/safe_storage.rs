//! Claude's key in the keychain, `Claude Safe Storage`, which Chromium encrypts the app's
//! cookies with.
//!
//! The item's access list trusts Claude and nothing else, so reading its password makes
//! macOS ask the person at the Mac whether Pitboard may. That question is asked once, when
//! somebody turns live usage on, and never again from a refresh: a refresh that would have
//! to ask gives up after ten seconds and says approval is needed. Giving up does not take
//! the question away: experiment U-K3 found macOS keeps it on screen after `security` is
//! killed, so a refresh that gave up never asks again for the same key. That holds within
//! one process: `enable` asks every time it runs, and two processes refreshing at once can
//! each ask, so each of those can leave a question behind. After Always Allow a background
//! shell that could not ask before reads the key without asking (experiment U-K1b), so
//! refreshes from there need nobody at the Mac; a read over SSH or from a scheduled job has
//! not been measured. Reading the item's attributes
//! asks nothing (the register's `security_attributes_never_prompt`, experiment U-K5), which
//! is how a key Claude made again is noticed without its password being read.
//!
//! Every read goes through `/usr/bin/security`, like every other keychain read Pitboard
//! makes, and is never written: the item is Claude's. Running it is the host's, in
//! `host/macos/safe_storage.rs`; what a read means is said here.

use crate::context::Context;
use std::time::Duration;
use zeroize::Zeroizing;

pub(crate) use super::types::ItemStamp;

/// The item Claude Desktop keeps its key in.
// Only the macOS host reads the item.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const ITEM_SERVICE: &str = "Claude Safe Storage";
/// The item's account. Experiment E5 read "Claude Key", and Swivel says the same; left
/// unnamed all the same, since the service alone finds the item and naming the account would
/// miss an install that used another.
// Only the macOS host reads the item.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const ITEM_ACCOUNT: Option<&str> = None;

/// Why the password is being read, which decides how long a question may wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyRead {
    /// Somebody turned live usage on and is at the Mac to answer macOS.
    Approve,
    /// A refresh, which nobody is watching.
    Refresh,
}

impl KeyRead {
    /// How long `security` may wait on macOS's question before it is killed. Killing it
    /// does not close the question (experiment U-K3): the dialog stays on screen until
    /// somebody answers it, and an answer then reaches nobody.
    // Only the macOS host runs `security`, so elsewhere only the tests ask.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn limit(self) -> Duration {
        match self {
            KeyRead::Approve => Duration::from_secs(120),
            KeyRead::Refresh => Duration::from_secs(10),
        }
    }
}

/// What reading the item came to, by `security`'s exit status, which is the low byte of
/// the `OSStatus`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum KeyReadError {
    /// `errSecInteractionNotAllowed`: macOS would have to ask, and cannot from here.
    #[error("macOS can only ask about Claude's key on the Mac's own screen")]
    NoGui,
    /// `errSecUserCanceled`: somebody said no, or typed a password that was not accepted
    /// and then chose Allow, which experiment U-K2 found exits 128 as well.
    #[error("macOS did not let Pitboard read Claude's key")]
    Denied,
    /// `errSecAuthFailed`. Experiment U-K2 saw no answer at the question give it, a wrong
    /// password included, which is [`KeyReadError::Denied`]; kept for a keychain that does.
    #[error("macOS did not accept the password given for Claude's key")]
    AuthFailed,
    /// Nobody answered in time. The question may still be on screen (experiment U-K3).
    #[error("macOS's question about Claude's key was not answered in time")]
    TimedOut,
    /// `errSecItemNotFound`.
    #[error("Claude's key is not in the keychain")]
    Missing,
    #[error("Claude's key could not be read: {0}")]
    Other(String),
}

impl KeyReadError {
    /// The stable code live usage records for this, as `LiveUsageNotAllowed` reads it.
    pub(crate) fn reason(&self) -> &'static str {
        match self {
            KeyReadError::NoGui => "no_gui",
            KeyReadError::Denied => "denied",
            KeyReadError::AuthFailed => "auth_failed",
            KeyReadError::TimedOut => "timed_out",
            KeyReadError::Missing => "item_missing",
            KeyReadError::Other(_) => "other",
        }
    }
}

/// Claude's key, as a seam: `security` in every real context, a script in the tests,
/// which never read the real item.
pub(crate) trait SafeStorage: Send + Sync + std::fmt::Debug {
    /// When the item was made and last changed, read without its password.
    fn stamp(&self, ctx: &Context) -> Result<ItemStamp, KeyReadError>;
    /// The item's password, which may make macOS ask.
    fn password(&self, ctx: &Context, how: KeyRead) -> Result<Zeroizing<Vec<u8>>, KeyReadError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refresh_waits_ten_seconds_and_an_approval_two_minutes() {
        assert_eq!(KeyRead::Refresh.limit(), Duration::from_secs(10));
        assert_eq!(KeyRead::Approve.limit(), Duration::from_secs(120));
    }
}
