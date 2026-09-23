//! How pitboard writes down what it believes about somebody else's software.
//!
//! Every load-bearing fact about a tool pitboard parks logins for was read out of one build
//! of that tool, and those tools ship several times a week. When such a fact moves, pitboard
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

/// One thing pitboard believes about a tool it parks logins for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assumption {
    /// Stable, snake_case, safe for a program to branch on.
    pub name: &'static str,
    /// What pitboard believes.
    pub fact: &'static str,
    /// Where in Claude Code it was read, so it can be read again.
    pub read_from: &'static str,
    /// The build it was last verified against.
    pub verified_against: &'static str,
    /// What in this crate stops being true if it moves.
    pub depends: &'static str,
    /// Literals that must be present in a Claude Code build for this fact to still be
    /// readable there. Empty where the fact cannot be read out of a build at all, which is
    /// every fact that is about behaviour rather than about a name.
    ///
    /// These are a cheap and shallow check. A literal being present does not prove the
    /// behaviour around it is unchanged; a literal disappearing does prove something moved.
    /// Measured across six builds: the set below holds from 2.1.273 onwards, and correctly
    /// goes red on 2.1.124, which predates the write lock, the two extra account-scoped
    /// keys and the keychain error classification.
    pub probe: &'static [&'static str],
    /// Literals whose *arrival* would disprove the fact.
    ///
    /// Some of what pitboard stands on is an absence: Claude Code has no Linux keyring
    /// backend, so on Linux its login is a file, so pitboard's own store there is a file
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
    }
}

/// The build one provider's register was read from.
pub fn verified_against(provider: ProviderId) -> &'static str {
    match provider {
        ProviderId::Claude => crate::provider::claude::assumptions::VERIFIED_AGAINST,
    }
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
    /// being an absence: a keyring backend where pitboard is relying on there being none.
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
    /// Code would make pitboard's Linux store the wrong shape without anything pitboard
    /// reads going missing, so it is looked for rather than waited for.
    #[test]
    fn a_keyring_arriving_where_there_was_none_is_reported() {
        let no_keyring = named("no_keyring_off_macos").unwrap();
        assert_eq!(
            read_from_build(
                no_keyring,
                "tengu_windows_credman CLAUDE_CODE_FORCE_WINDOWS_CREDMAN"
            ),
            Reading::Holds
        );
        assert_eq!(
            read_from_build(
                no_keyring,
                "tengu_windows_credman CLAUDE_CODE_FORCE_WINDOWS_CREDMAN libsecret_password_store"
            ),
            Reading::Appeared(vec!["libsecret"])
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
                a.verified_against.split('.').count() == 3,
                "{} is dated against `{}`, which is not a version",
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
