//! How Pitboard writes down what it believes about somebody else's software.
//!
//! Every load-bearing fact about a tool Pitboard parks logins for was read out of one build
//! of that tool, and those tools ship several times a week. When such a fact moves, Pitboard
//! does not fail loudly: it parks a login under the wrong account, or writes to an item
//! nobody reads, or leaves the outgoing account's device token in place for the incoming
//! one. The 0.1.4 changelog records this class of bug happening once already, found by hand.
//!
//! So the facts are a list rather than a comment. Each one says what it is, where in the
//! tool it was read, which build it was last verified against, and what in this crate falls
//! over if it moves.
//!
//! Each provider keeps its own list, dated on its own schedule, because these are facts
//! about different binaries with nothing to do with each other:
//! [`crate::provider::claude::assumptions`] is Claude Code's. This module is the shape they
//! share and the machinery that probes them.
//!
//! This is a register, not a check. Naming a fact does not verify it, and the list says so
//! by dating every entry.

use crate::provider::ProviderId;

/// The system a build of a tool is for.
///
/// A tool's builds for different systems do not carry the same code. Claude Code's Linux
/// build has no keychain code at all, so a fact about the macOS keychain, read from it,
/// reports the keychain gone. Measured on 2.1.278, 2.1.281 and 2.1.284, where 1,462 string
/// literals are in the macOS build only and 85 in the Linux build only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Platform {
    MacOs,
    Linux,
}

impl Platform {
    /// Both, for a fact that holds the same on either.
    pub const ALL: &'static [Platform] = &[Platform::MacOs, Platform::Linux];

    /// Stable, lower case, as a report names it.
    pub fn code(self) -> &'static str {
        match self {
            Platform::MacOs => "macos",
            Platform::Linux => "linux",
        }
    }
}

/// One thing Pitboard believes about a tool it parks logins for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assumption {
    /// Stable, snake_case, safe for a program to branch on.
    pub name: &'static str,
    /// What Pitboard believes.
    pub fact: &'static str,
    /// Where in Claude Code it was read, so it can be read again.
    pub read_from: &'static str,
    /// The build it was last verified against, or [`UNVERIFIED`] while it has not been.
    pub verified_against: &'static str,
    /// What in this crate stops being true if it moves.
    pub depends: &'static str,
    /// Literals that must be present in a Claude Code build for this fact to still be
    /// readable there. Empty where the fact cannot be read out of a build at all, which is
    /// every fact that is about behaviour rather than about a name.
    ///
    /// These are a cheap and shallow check. A literal being present does not prove the
    /// behaviour around it is unchanged; a literal disappearing does prove something moved.
    /// Read from the builds [`read_on`] names, and only from those.
    pub probe: &'static [&'static str],
    /// Literals whose *arrival* would disprove the fact.
    ///
    /// Some of what Pitboard stands on is an absence: Claude Code has no Linux keyring
    /// backend, so on Linux its login is a file, so Pitboard's own store there is a file
    /// too. A fact like that cannot be probed for by looking for something. Nothing being
    /// there is not evidence a check is running, which is exactly how an absence stops
    /// being true without anybody noticing, so the absence is written down and looked for.
    ///
    /// Needles here must be specific to the thing being ruled out. `secret-tool` and
    /// `kwallet-query` both appear in the build already, in the list of credential helpers
    /// its sandbox excludes from a shell, and either would report a keyring backend that is
    /// not there.
    pub absent: &'static [&'static str],
}

/// One provider's register.
pub fn of(provider: ProviderId) -> &'static [Assumption] {
    match provider {
        ProviderId::Claude => crate::provider::claude::assumptions::ASSUMPTIONS,
        ProviderId::Codex => crate::provider::codex::assumptions::ASSUMPTIONS,
        ProviderId::Desktop => crate::provider::desktop::assumptions::ASSUMPTIONS,
    }
}

/// The systems whose builds one of a provider's facts is read from.
///
/// Each register says this beside its facts rather than in them: `Assumption` can be
/// written as a literal outside this crate, and a field added to it would break every such
/// literal.
pub fn read_on(provider: ProviderId, name: &str) -> &'static [Platform] {
    match provider {
        ProviderId::Claude => crate::provider::claude::assumptions::read_on(name),
        ProviderId::Codex => Platform::ALL,
        ProviderId::Desktop => crate::provider::desktop::assumptions::read_on(name),
    }
}

/// The build one provider's register was read from.
pub fn verified_against(provider: ProviderId) -> &'static str {
    match provider {
        ProviderId::Claude => crate::provider::claude::assumptions::VERIFIED_AGAINST,
        ProviderId::Codex => crate::provider::codex::assumptions::VERIFIED_AGAINST,
        ProviderId::Desktop => crate::provider::desktop::assumptions::VERIFIED_AGAINST,
    }
}

/// What an entry is dated against while nobody has measured it yet.
///
/// Such an entry is written down so the code that leans on it can be found, and it names
/// the experiment that settles it. Code whose behaviour turns on it asks [`verified`]
/// rather than assuming either way, and dating it against a build changes that behaviour.
pub const UNVERIFIED: &str = "unverified";

/// Whether one of a provider's facts has been measured against a build. A fact the
/// register does not name has not been.
pub fn verified(provider: ProviderId, name: &str) -> bool {
    of(provider)
        .iter()
        .find(|a| a.name == name)
        .is_some_and(|a| a.verified_against != UNVERIFIED)
}

/// Every provider's register, in one list.
///
/// A provider whose register is missing from here is one nothing checks, and nothing would
/// say so, which is the same failure the registers exist to prevent.
pub fn all() -> Vec<&'static Assumption> {
    ProviderId::ALL.iter().flat_map(|&p| of(p)).collect()
}

/// The assumption of that name, for a check or a probe that wants to speak about one.
pub fn named(name: &str) -> Option<&'static Assumption> {
    all().into_iter().find(|a| a.name == name)
}

/// What a probe found in one Claude Code build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reading {
    /// Every literal this fact is readable by is there, and nothing that would disprove it
    /// has turned up.
    Holds,
    /// This fact cannot be read out of a build at all; it is about behaviour, not a name.
    NotReadable,
    /// Something moved. These literals are gone.
    Moved(Vec<&'static str>),
    /// Something arrived that this fact said would not be there. An absence that stopped
    /// being an absence: a keyring backend where Pitboard is relying on there being none.
    Appeared(Vec<&'static str>),
}

/// Check one assumption against the printable strings of a Claude Code build.
///
/// Shallow on purpose. A literal being present does not prove the behaviour around it is
/// unchanged, and this never claims it does; a literal disappearing does prove something
/// moved, which is the only thing worth waking somebody for.
pub fn read_from_build(assumption: &Assumption, strings: &str) -> Reading {
    // An arrival is reported before a disappearance: a fact that rests on nothing being
    // there is wrong the moment something is, whatever else still reads the same.
    let arrived: Vec<&'static str> = assumption
        .absent
        .iter()
        .filter(|needle| strings.contains(**needle))
        .copied()
        .collect();
    if !arrived.is_empty() {
        return Reading::Appeared(arrived);
    }
    if assumption.probe.is_empty() {
        return if assumption.absent.is_empty() {
            Reading::NotReadable
        } else {
            // Nothing to look for, and nothing that should not be there was found.
            Reading::Holds
        };
    }
    let gone: Vec<&'static str> = assumption
        .probe
        .iter()
        .filter(|needle| !strings.contains(**needle))
        .copied()
        .collect();
    if gone.is_empty() {
        Reading::Holds
    } else {
        Reading::Moved(gone)
    }
}

/// Every printable run of `least` bytes or more, which is all a probe needs of a binary and
/// is the one thing a compiled bundle reliably gives up.
pub fn printable_runs(bytes: &[u8], least: usize) -> String {
    let mut out = String::new();
    let mut run = Vec::new();
    for &b in bytes {
        if (0x20..0x7f).contains(&b) || b == b'\t' {
            run.push(b);
            continue;
        }
        if run.len() >= least {
            out.push_str(&String::from_utf8_lossy(&run));
            out.push('\n');
        }
        run.clear();
    }
    if run.len() >= least {
        out.push_str(&String::from_utf8_lossy(&run));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one fact here that rests on an absence. A keyring backend arriving in Claude
    /// Code would make Pitboard's Linux store the wrong shape without anything Pitboard
    /// reads going missing, so it is looked for rather than waited for.
    #[test]
    fn a_keyring_arriving_where_there_was_none_is_reported() {
        let no_keyring = named("no_keyring_off_macos").unwrap();
        let backends = r#"tengu_windows_credman CLAUDE_CODE_FORCE_WINDOWS_CREDMAN ["keychain","plaintext","windows-credman"]"#;
        assert_eq!(read_from_build(no_keyring, backends), Reading::Holds);
        assert_eq!(
            read_from_build(no_keyring, &format!("{backends} Bun.secrets.get")),
            Reading::Appeared(vec!["Bun.secrets"])
        );
    }

    /// `libsecret` is in every Linux build of Claude Code since at least 2.1.278, in the
    /// Bun runtime it ships inside, and Claude Code's own code never reaches it. As a needle
    /// it reported a keyring backend that was not there, from the first run that read a
    /// Linux build.
    #[test]
    fn the_bundled_runtime_is_not_taken_for_a_keyring() {
        let no_keyring = named("no_keyring_off_macos").unwrap();
        assert!(!no_keyring.absent.contains(&"libsecret"));
        let backends = r#"tengu_windows_credman CLAUDE_CODE_FORCE_WINDOWS_CREDMAN ["keychain","plaintext","windows-credman"]"#;
        assert_eq!(
            read_from_build(
                no_keyring,
                &format!("{backends} libsecret not available. libsecret-1.so.0")
            ),
            Reading::Holds
        );
    }

    /// Every fact is read from at least one build. One read from none would never be
    /// checked, and nothing would say so.
    #[test]
    fn every_fact_is_read_from_some_build() {
        for &provider in ProviderId::ALL {
            for a in of(provider) {
                assert!(
                    !read_on(provider, a.name).is_empty(),
                    "{} is read from no build",
                    a.name
                );
            }
        }
    }

    /// The keychain facts are read from a macOS build, and the fact about Linux having no
    /// keyring from a Linux one. Read from the wrong build, each reported drift that was
    /// not there.
    #[test]
    fn each_keychain_fact_is_read_where_the_keychain_code_is() {
        for name in ["keychain_write_route", "keychain_absence_codes"] {
            assert_eq!(
                read_on(ProviderId::Claude, name),
                &[Platform::MacOs],
                "{name}"
            );
        }
        assert_eq!(
            read_on(ProviderId::Claude, "no_keyring_off_macos"),
            &[Platform::Linux]
        );
    }

    /// `secret-tool` and `kwallet-query` are both in a shipping build already, in the list
    /// of credential helpers its sandbox keeps out of a shell. Either as a needle would
    /// report a keyring backend on every build there has ever been.
    #[test]
    fn nothing_already_in_a_build_is_used_to_rule_a_backend_out() {
        for a in all() {
            for needle in a.absent {
                assert!(
                    !["secret-tool", "kwallet-query", "keytar", "keyring"].contains(needle),
                    "{}: `{needle}` is in the build for other reasons",
                    a.name
                );
                assert!(
                    needle.len() >= 8,
                    "{}: `{needle}` is too short to mean one thing",
                    a.name
                );
            }
        }
    }

    /// An absence with nothing to read holds until something turns up. Without this it
    /// would report as unreadable, which is what a fact nobody is checking looks like.
    #[test]
    fn a_fact_that_is_only_an_absence_still_reads() {
        let only_absent = Assumption {
            name: "x",
            fact: "x",
            read_from: "x",
            verified_against: "9.9.9",
            depends: "x",
            probe: &[],
            absent: &["a_thing_that_should_not_be_here"],
        };
        assert_eq!(
            read_from_build(&only_absent, "nothing to see"),
            Reading::Holds
        );
        assert_eq!(
            read_from_build(&only_absent, "a_thing_that_should_not_be_here"),
            Reading::Appeared(vec!["a_thing_that_should_not_be_here"])
        );
    }

    #[test]
    fn every_assumption_is_named_once_and_says_all_four_things() {
        let mut names: Vec<&str> = all().iter().map(|a| a.name).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before, "two assumptions share a name");

        for a in all() {
            assert!(!a.fact.is_empty(), "{} says nothing", a.name);
            assert!(!a.read_from.is_empty(), "{} says nowhere", a.name);
            assert!(!a.depends.is_empty(), "{} costs nothing", a.name);
            assert!(
                a.verified_against.split('.').count() == 3 || a.verified_against == UNVERIFIED,
                "{} is dated against `{}`, which is neither a version nor unverified",
                a.name,
                a.verified_against
            );
            assert!(
                a.name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'_' || b.is_ascii_digit()),
                "{} is not a stable code",
                a.name
            );
        }
    }

    /// A fact nobody has measured yet is only honest while it says how it will be. Every
    /// such entry names the experiment that settles it, `E<n>` or `U-K<n>`, so flipping it
    /// to a build is a matter of running that and nothing else.
    #[test]
    fn every_unverified_fact_names_its_experiment() {
        fn names_an_experiment(read_from: &str) -> bool {
            let words = read_from.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'));
            words.into_iter().any(|word| {
                let number = word
                    .strip_prefix("U-K")
                    .or_else(|| word.strip_prefix('E'))
                    .unwrap_or_default();
                !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())
            })
        }
        let unverified: Vec<&Assumption> = all()
            .into_iter()
            .filter(|a| a.verified_against == UNVERIFIED)
            .collect();
        assert!(
            !unverified.is_empty(),
            "Claude Desktop's register has facts still to be measured"
        );
        for a in unverified {
            assert!(
                names_an_experiment(a.read_from),
                "{} is unverified and names no experiment in `{}`",
                a.name,
                a.read_from
            );
            assert!(!verified(ProviderId::Desktop, a.name), "{}", a.name);
        }
        assert!(verified(ProviderId::Desktop, "desktop_bundle"));
        assert!(!verified(ProviderId::Desktop, "no_such_fact"));
        assert!(!names_an_experiment("ps on a Mac"));
        assert!(names_an_experiment("E11/E13"));
        assert!(names_an_experiment("U-K4"));
    }

    #[test]
    fn a_probe_reads_what_is_there_and_names_what_is_not() {
        let write_lock = named("write_lock").expect("listed");
        let whole = write_lock.probe.join(" and also ");
        assert_eq!(read_from_build(write_lock, &whole), Reading::Holds);

        let moved = read_from_build(write_lock, "nothing of the sort");
        assert_eq!(moved, Reading::Moved(write_lock.probe.to_vec()));

        // A fact about behaviour cannot be read out of a build, and says so rather than
        // pretending either way.
        let cache = named("credential_cache").expect("listed");
        assert_eq!(read_from_build(cache, ""), Reading::NotReadable);
    }

    #[test]
    fn printable_runs_finds_the_strings_and_nothing_else() {
        let bytes = b"\x00\x01hello there\x00\x02tiny\x00wide load\xff";
        let found = printable_runs(bytes, 6);
        assert!(found.contains("hello there"));
        assert!(found.contains("wide load"));
        assert!(
            !found.contains("tiny"),
            "a run shorter than asked for is not a string"
        );
    }

    #[test]
    fn an_assumption_can_be_looked_up_by_name() {
        assert_eq!(
            named("write_lock").expect("it is listed").name,
            "write_lock"
        );
        assert_eq!(named("nothing_like_this"), None);
    }
}
