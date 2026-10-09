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
//!
//! Beside its facts, each register keeps a table, [`PerSystem`], that says of every fact on
//! every system whether it was read there and from which build, why it is not read there,
//! or which pull request of the Windows work reads it there. A fact that behaves
//! differently on one system is a fact of its own for that system, never a second reading
//! under the same name.

use crate::provider::ProviderId;

/// The system a build of a tool is for.
///
/// A tool's builds for different systems do not carry the same code. Claude Code's Linux
/// build has no keychain code at all, so a fact about the macOS keychain, read from it,
/// reports the keychain gone. Measured on 2.1.278, 2.1.281 and 2.1.284, where 1,462 string
/// literals are in the macOS build only and 85 in the Linux build only. Its Windows build of
/// 2.1.289 is built from the same commit as the other two and has no keychain backend
/// either, though it keeps the keychain code the builds share, and a Credential Manager
/// store of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Platform {
    MacOs,
    Linux,
    Windows,
}

impl Platform {
    /// Every system, in the order a report lists them. Kept private: a fact's systems are
    /// the ones its line in [`PerSystem`] reads it on, and no list of systems stands in for
    /// that.
    const EACH: [Platform; 3] = [Platform::MacOs, Platform::Linux, Platform::Windows];

    /// Stable, lower case, as a report names it.
    pub fn code(self) -> &'static str {
        match self {
            Platform::MacOs => "macos",
            Platform::Linux => "linux",
            Platform::Windows => "windows",
        }
    }
}

/// What a register says of one fact on one system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnSystem {
    /// Read from this build of the tool for that system. A conformance run reads the fact
    /// from that system's builds, and only from those.
    Read(&'static str),
    /// Not read there, and why: the system has nothing the fact is about, or its build
    /// carries what the fact rules out for another reason.
    NotRead(&'static str),
    /// Not read there yet. `by` names the pull requests of the Windows work that read it,
    /// `W2` to `W27`, and `reads` what each reads, which is a fact of its own wherever the
    /// tool behaves differently there.
    Pending {
        by: &'static [&'static str],
        reads: &'static str,
    },
}

/// One fact's line in the table beside its register: what the register says of it on each
/// system. A field for each system, so no fact can be left out on one or said twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerSystem {
    /// The fact's name in the register.
    pub name: &'static str,
    pub macos: OnSystem,
    pub linux: OnSystem,
    pub windows: OnSystem,
}

impl PerSystem {
    /// What this line says on one system.
    pub fn on(&self, platform: Platform) -> OnSystem {
        match platform {
            Platform::MacOs => self.macos,
            Platform::Linux => self.linux,
            Platform::Windows => self.windows,
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
    /// The build its macOS and Linux readings name. A fact read on Windows may be read there
    /// from a later build, which the register's table names, so [`verified_on`] is the build
    /// for any one system.
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
    }
}

/// The table beside one provider's register, a line for each of its facts.
///
/// Each register says this beside its facts rather than in them: `Assumption` can be
/// written as a literal outside this crate, and a field added to it would break every such
/// literal.
pub fn per_system(provider: ProviderId) -> &'static [PerSystem] {
    match provider {
        ProviderId::Claude => crate::provider::claude::assumptions::PER_SYSTEM,
        ProviderId::Codex => crate::provider::codex::assumptions::PER_SYSTEM,
    }
}

/// What a provider's register says of one of its facts on one system, or nothing for a
/// name it does not hold.
pub fn on(provider: ProviderId, name: &str, platform: Platform) -> Option<OnSystem> {
    per_system(provider)
        .iter()
        .find(|line| line.name == name)
        .map(|line| line.on(platform))
}

/// The systems whose builds one of a provider's facts is read from.
pub fn read_on(provider: ProviderId, name: &str) -> Vec<Platform> {
    Platform::EACH
        .into_iter()
        .filter(|&platform| verified_on(provider, name, platform).is_some())
        .collect()
}

/// The build one of a provider's facts was read from on one system, or nothing where it is
/// not read there.
///
/// `Assumption::verified_against` is the build its macOS and Linux readings name, and one
/// field could not also say that the same fact was read on Windows from a later build.
pub fn verified_on(provider: ProviderId, name: &str, platform: Platform) -> Option<&'static str> {
    match on(provider, name, platform)? {
        OnSystem::Read(build) => Some(build),
        OnSystem::NotRead(_) | OnSystem::Pending { .. } => None,
    }
}

/// Every fact whose reading on a system waits on a pull request still to come, with that
/// system. The Windows work is done when this is empty.
pub fn pending() -> Vec<(ProviderId, &'static str, Platform)> {
    ProviderId::ALL
        .iter()
        .flat_map(|&provider| {
            per_system(provider).iter().flat_map(move |line| {
                Platform::EACH
                    .into_iter()
                    .filter(|&platform| matches!(line.on(platform), OnSystem::Pending { .. }))
                    .map(move |platform| (provider, line.name, platform))
            })
        })
        .collect()
}

/// The build one provider's register was read from on one system, unless an entry names
/// its own.
pub fn verified_against(provider: ProviderId, platform: Platform) -> &'static str {
    use crate::provider::{claude, codex};
    match (provider, platform) {
        (ProviderId::Claude, Platform::MacOs | Platform::Linux) => {
            claude::assumptions::VERIFIED_AGAINST
        }
        (ProviderId::Claude, Platform::Windows) => claude::assumptions::WINDOWS_VERIFIED_AGAINST,
        (ProviderId::Codex, Platform::MacOs | Platform::Linux) => {
            codex::assumptions::VERIFIED_AGAINST
        }
        (ProviderId::Codex, Platform::Windows) => codex::assumptions::WINDOWS_VERIFIED_AGAINST,
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
                [Platform::MacOs],
                "{name}"
            );
        }
        assert_eq!(
            read_on(ProviderId::Claude, "no_keyring_off_macos"),
            [Platform::Linux]
        );
    }

    /// Each register's table has a line for every fact it holds and for nothing else, and
    /// a line says one thing on each system. A fact with no line would be read nowhere and
    /// nothing would say so; a line naming no fact would be a reading of nothing.
    #[test]
    fn every_fact_has_one_line_and_every_line_is_a_fact() {
        for &provider in ProviderId::ALL {
            let mut facts: Vec<&str> = of(provider).iter().map(|a| a.name).collect();
            let mut lines: Vec<&str> = per_system(provider).iter().map(|l| l.name).collect();
            facts.sort_unstable();
            lines.sort_unstable();
            let before = lines.len();
            lines.dedup();
            assert_eq!(lines.len(), before, "{provider:?}: a fact has two lines");
            assert_eq!(lines, facts, "{provider:?}");
            for a in of(provider) {
                for platform in Platform::EACH {
                    assert!(on(provider, a.name, platform).is_some(), "{}", a.name);
                }
            }
        }
    }

    /// Claude Code's Credential Manager store calls `Bun.secrets`, so the Windows build
    /// carries it 25 times in 2.1.289, x64 and arm64 both. A fact that rules it out, read
    /// there, would report a keyring arrived on every build.
    #[test]
    fn no_fact_ruling_out_bun_secrets_is_read_on_windows() {
        for &provider in ProviderId::ALL {
            for a in of(provider) {
                if a.absent.contains(&"Bun.secrets") {
                    assert_eq!(
                        verified_on(provider, a.name, Platform::Windows),
                        None,
                        "{}",
                        a.name
                    );
                    assert!(
                        matches!(
                            on(provider, a.name, Platform::Windows),
                            Some(OnSystem::NotRead(_))
                        ),
                        "{} must say why it is not read on Windows",
                        a.name
                    );
                }
            }
        }
    }

    /// A reading still to come names the pull request of the Windows work that takes it,
    /// `W2` to `W27`, and says what that reads. Nothing on macOS or Linux waits on one.
    #[test]
    fn every_pending_reading_names_a_pull_request_of_the_windows_work() {
        for &provider in ProviderId::ALL {
            for line in per_system(provider) {
                for platform in Platform::EACH {
                    let OnSystem::Pending { by, reads } = line.on(platform) else {
                        continue;
                    };
                    assert_eq!(platform, Platform::Windows, "{}", line.name);
                    assert!(!by.is_empty(), "{} waits on nothing", line.name);
                    assert!(
                        !reads.is_empty(),
                        "{} says nothing of what is read",
                        line.name
                    );
                    for pr in by {
                        let number = pr.strip_prefix('W').and_then(|n| n.parse::<u32>().ok());
                        assert!(
                            matches!(number, Some(2..=27)),
                            "{}: `{pr}` is not a pull request of the Windows work",
                            line.name
                        );
                    }
                }
            }
        }
    }

    /// The readings still to come are listed in one place, so the last pull request of the
    /// Windows work can require the list to be empty. Today it is the facts of each tool that
    /// Windows reads differently or later: eight of Claude Code's, and eight of Codex's, with
    /// the layers its store is read from.
    #[test]
    fn the_readings_still_to_come_are_listed() {
        let waiting = pending();
        assert!(waiting.iter().all(|&(_, _, p)| p == Platform::Windows));
        for (provider, count) in [(ProviderId::Claude, 8), (ProviderId::Codex, 8)] {
            let names: Vec<&str> = waiting
                .iter()
                .filter(|&&(p, _, _)| p == provider)
                .map(|&(_, name, _)| name)
                .collect();
            assert_eq!(names.len(), count, "{provider:?}: {names:?}");
            for name in names {
                assert!(matches!(
                    on(provider, name, Platform::Windows),
                    Some(OnSystem::Pending { .. })
                ));
            }
        }
    }

    /// `verified_against` is the build a fact's macOS and Linux readings name, so a report
    /// of either system and the fact itself cannot disagree.
    #[test]
    fn the_field_names_the_build_of_the_macos_and_linux_readings() {
        for &provider in ProviderId::ALL {
            for a in of(provider) {
                for platform in [Platform::MacOs, Platform::Linux] {
                    if let Some(build) = verified_on(provider, a.name, platform) {
                        assert_eq!(
                            build,
                            a.verified_against,
                            "{} on {}",
                            a.name,
                            platform.code()
                        );
                    }
                }
            }
        }
    }

    /// A reading names a version, and a fact not read somewhere says why.
    #[test]
    fn every_line_says_a_build_or_a_reason() {
        for &provider in ProviderId::ALL {
            for line in per_system(provider) {
                for platform in Platform::EACH {
                    match line.on(platform) {
                        OnSystem::Read(build) => assert_eq!(
                            build.split('.').count(),
                            3,
                            "{} on {}: `{build}` is not a version",
                            line.name,
                            platform.code()
                        ),
                        OnSystem::NotRead(why) => {
                            assert!(!why.is_empty(), "{} on {}", line.name, platform.code());
                        }
                        OnSystem::Pending { .. } => {}
                    }
                }
            }
        }
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
