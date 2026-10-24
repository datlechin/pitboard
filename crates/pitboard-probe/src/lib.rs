//! `pitboard-probe` measures the Windows facts Pitboard for Windows is built on, on a
//! machine kept for that: the owner's VM sessions and the runner-facts step every Windows CI
//! leg runs. It is never published and never in a release (`publish = false`, and
//! `release.yml` never builds it).
//!
//! The crate is split in two halves, so the half that holds no Windows call is unit tested
//! on every system:
//!
//! - the **report** half, here in the library: the command line, how a logon, an elevation,
//!   a Credential Manager target, a process image or a manifest is named, what the probe
//!   refuses to touch, how it hides the account in what it prints, the PE reader, and the
//!   report's JSON shape. Every piece of it is pure.
//! - the **measure** half, [`win`], compiled on Windows alone. On any other system it is
//!   absent and every subcommand says the probe is for Windows.
//!
//! What the probe may never do is written into the report half, so a test proves it:
//!
//! - every subcommand that writes refuses unless the Windows account carries the throwaway
//!   marker the owner creates, and refuses a scratch path that is relative or keeps a `..`
//!   (nothing is resolved against the working folder), or that is, in any spelling or by
//!   the identity of a folder it passes through, the real profile's `.claude`,
//!   `.claude.json`, `.codex`, `%LOCALAPPDATA%\Pitboard` or Claude Desktop's folders, or
//!   inside one ([`guard`]);
//! - it writes, reads or deletes only Credential Manager items, tasks, registry keys and
//!   files it made itself under a `pitboard-probe-*` name or in a scratch folder it was
//!   given, and every file it is named (`--out`, `dpapi --file`, `swap`) carries that
//!   prefix; past those it only reads the task Pitboard's watch service registered for the
//!   account; `credman-names` asks Credential Manager only for the live login families and
//!   `pitboard-*` names, and never reads a blob out ([`names`]);
//! - it names processes only when they are the tools', their runtimes', Pitboard's, the
//!   probe's or a browser's ([`images`]);
//! - it prints principals as their relation to the token (self, SYSTEM, Administrators,
//!   other), never a SID, and every path with the profile and the account name replaced
//!   ([`redact`]);
//! - no object in a report holds two keys that differ only in case, which PowerShell's
//!   `ConvertFrom-Json` refuses ([`report`]).

// The probe reads the environment on purpose: `PATH`, `USERPROFILE`, `CODEX_HOME` and the
// rest are what the homes and exe-lookup blocks measure, and it is not Pitboard's engine, so
// the workspace rule to read the environment only through `context::Environment` does not
// apply here.
#![allow(clippy::disallowed_methods)]

pub mod assoc;
pub mod cli;
pub mod console;
pub mod elevation;
pub mod exelookup;
pub mod guard;
pub mod hashes;
pub mod images;
pub mod logon;
pub mod names;
pub mod pe;
pub mod redact;
pub mod replace;
pub mod report;

#[cfg(windows)]
pub mod win;

/// The prefix every Credential Manager item, task, registry key, scratch folder and file the
/// probe makes for itself carries, so its own leavings are told from everything else by
/// name alone.
pub const PROBE_PREFIX: &str = "pitboard-probe-";

/// The marker file the owner drops in a throwaway account's profile folder to let the probe
/// write there.
pub const MARKER_FILE: &str = "pitboard-probe-throwaway.marker";

/// The local stand-in page block F2 opens in the default browser: the only page the probe
/// ever opens, and no site.
pub const STANDIN_PAGE: &str = "pitboard-probe-standin.html";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_marker_is_named_for_the_probe() {
        assert!(MARKER_FILE.starts_with(PROBE_PREFIX));
        assert_eq!(PROBE_PREFIX, PROBE_PREFIX.to_lowercase());
        assert!(guard::is_probe_file_name(STANDIN_PAGE));
    }
}
