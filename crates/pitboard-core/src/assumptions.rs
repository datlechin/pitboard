//! Every fact about Claude Code that pitboard stands on, named and dated.
//!
//! All of it was read out of one build. The keychain item's name and how the slot is hashed
//! from a directory, the five keys a logout deletes, the write lock and its constants, the
//! one write that skips the lock, the 4032-byte ceiling on a `security` command, the thirty
//! seconds a session caches a credential for. Claude Code ships several times a week. When
//! one of those facts moves, pitboard does not fail loudly: it parks a login under the
//! wrong account, or writes to an item nobody reads, or leaves the outgoing account's
//! device token in place for the incoming one. The 0.1.4 changelog records this class of
//! bug happening once already, found by hand.
//!
//! So the facts are a list rather than a comment. Each one says what it is, where in Claude
//! Code it was read, which build it was last verified against, and what in this crate falls
//! over if it moves. `doctor` reads the list; so does anything that re-measures it.
//!
//! This is a register, not a check. Naming a fact does not verify it, and the list says so
//! by dating every entry.

/// The build every entry below was read from, unless it says otherwise.
pub const VERIFIED_AGAINST: &str = "2.1.278";

/// One thing pitboard believes about Claude Code.
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

pub const ASSUMPTIONS: &[Assumption] = &[
    Assumption {
        name: "credential_service_name",
        fact: "the login lives in a keychain item named `Claude Code${OAUTH_FILE_SUFFIX}-credentials`, \
               with `-<first 8 hex of sha256 of the NFC-normalised storage directory>` appended for \
               any directory but the default",
        read_from: "the secure storage module's service name and slot derivation",
        verified_against: VERIFIED_AGAINST,
        depends: "slot.rs, and every read and write of the live credential",
        probe: &["-credentials", "OAUTH_FILE_SUFFIX"],
        absent: &[],
    },
    Assumption {
        name: "keychain_account_name",
        fact: "the keychain account is $USER when it matches /^[a-zA-Z0-9._-]+$/, and \
               `claude-code-user` when it does not",
        read_from: "the secure storage module's account name",
        verified_against: VERIFIED_AGAINST,
        depends: "slot::account_name",
        probe: &["claude-code-user"],
        absent: &[],
    },
    Assumption {
        name: "live_chain_order",
        fact: "the login is read from the keychain first and a plaintext `.credentials.json` \
               second; the successor backend replaces the fallback half and only for a caller \
               that hands a backend in, which an ordinary `claude` does not",
        read_from: "the keychain-with-plaintext-fallback store",
        verified_against: VERIFIED_AGAINST,
        depends: "store::resolve and everything that reads or writes through it",
        probe: &[".credentials.json", "-with-", "-fallback"],
        absent: &[],
    },
    Assumption {
        name: "keychain_write_route",
        fact: "a write is `security -i` while the command is at most 4032 bytes and \
               `add-generic-password -U -a <account> -s <service> -X <hex>` above it, with a \
               2 second timeout, and only a timeout counts as retryable",
        read_from: "the keychain backend's update path",
        verified_against: VERIFIED_AGAINST,
        depends: "store::keychain::MAX_COMMAND_BYTES and the write path",
        probe: &[
            "add-generic-password",
            "exceeds security -i stdin limit; using argv",
        ],
        absent: &[],
    },
    Assumption {
        name: "keychain_absence_codes",
        fact: "`find-generic-password` exiting 0 with no output, or 44, means absent; 36 means \
               the keychain is locked and says nothing about what is there",
        read_from: "the keychain backend's read path",
        verified_against: VERIFIED_AGAINST,
        depends: "store::keychain::classify",
        probe: &[
            "errsecitemnotfound",
            "errsecinteractionnotallowed",
            "show-keychain-info",
        ],
        absent: &[],
    },
    Assumption {
        name: "write_lock",
        fact: "every credential write takes proper-lockfile's directory lock at \
               `<storage dir>/.storage-write`, stale 15000ms, ten retries, 100ms to 1000ms of \
               backoff, and re-reads the credential inside the lock before changing it",
        read_from: "the secure storage module's write wrapper",
        verified_against: VERIFIED_AGAINST,
        depends: "lock.rs and the whole switch",
        probe: &[".storage-write", "[secureStorage] write lock compromised: "],
        absent: &[],
    },
    Assumption {
        name: "logout_skips_the_lock",
        fact: "`/logout` retries for its own 7.5 seconds and then deletes the credential with \
               no lock held at all",
        read_from: "the secure storage module's already-locked escape hatch",
        verified_against: VERIFIED_AGAINST,
        depends: "the slot re-read in switch, which exists for this",
        probe: &["secureStorage.READ_FAILED"],
        absent: &[],
    },
    Assumption {
        name: "account_scoped_keys",
        fact: "a logout deletes claudeAiOauth, organizationUuid, trustedDeviceToken, \
               enterpriseGateway and designOauth, so those belong to the account",
        read_from: "the logout path",
        verified_against: VERIFIED_AGAINST,
        depends: "switch::ACCOUNT_SCOPED, and what a park holds",
        probe: &[
            "claudeAiOauth",
            "organizationUuid",
            "trustedDeviceToken",
            "enterpriseGateway",
            "designOauth",
        ],
        absent: &[],
    },
    Assumption {
        name: "credential_cache",
        fact: "a running session serves the credential from a 30 second cache, so a swap is \
               picked up within about 33 seconds",
        read_from: "the keychain backend's cache, and measured against a running session",
        verified_against: VERIFIED_AGAINST,
        depends: "switch::ADOPTION_CEILING_SECONDS",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "config_file_location",
        fact: "a legacy `<config dir>/.config.json` wins when present; otherwise \
               `<$CLAUDE_CONFIG_DIR or $HOME>/.claude<suffix>.json`",
        read_from: "the config path resolution",
        verified_against: VERIFIED_AGAINST,
        depends: "claude::config_file",
        probe: &[".config.json", "CLAUDE_CONFIG_DIR"],
        absent: &[],
    },
    Assumption {
        name: "oauth_client",
        fact: "every login Claude Code stores was issued to client 9d1c250a-e61b-44d9-88ed-5944d1962f5e, \
               and a refresh answer without a refresh-token lifetime keeps the one it had",
        read_from: "the OAuth client id and the token refresh path",
        verified_against: VERIFIED_AGAINST,
        depends: "api::CLIENT_ID and park::renewed",
        probe: &["9d1c250a-e61b-44d9-88ed-5944d1962f5e"],
        absent: &[],
    },
    Assumption {
        name: "supervisor_daemon",
        fact: "a supervisor daemon outlives the session that started it, refreshes the login on \
               a timer of roughly eight hours, records itself in `<config dir>/daemon.lock`, and \
               writes through the same lock as everything else",
        read_from: "the daemon's auth scheduler and its lock file",
        verified_against: VERIFIED_AGAINST,
        depends: "daemon.rs, and the lock discipline the switch relies on",
        probe: &[
            "daemon.lock",
            "daemon.status.json",
            "auth: scheduling proactive refresh in ",
        ],
        absent: &[],
    },
    Assumption {
        name: "no_keyring_off_macos",
        fact: "Claude Code has no Secret Service, libsecret, gnome-keyring or KWallet backend.                Its only guarded stores are the macOS keychain and, behind the                `tengu_windows_credman` flag, the Windows credential manager. Everywhere else,                Linux included, the keychain backend's `security` call simply fails and the                plaintext file is what holds the login",
        read_from: "the secure storage module's backend list, and the whole build searched for                     every Linux keyring name",
        verified_against: VERIFIED_AGAINST,
        depends: "store::PlainUnix, and pitboard's claim that a parked login on Linux is no                   less protected than the live one",
        probe: &["tengu_windows_credman", "CLAUDE_CODE_FORCE_WINDOWS_CREDMAN"],
        absent: &[
            "libsecret",
            "org.freedesktop.secrets",
            "gnome-keyring",
            "SecretService",
        ],
    },
    Assumption {
        name: "plaintext_credential_mode",
        fact: "the plaintext credential is written and then chmod'd to 0600, in its storage                directory, under the fixed name `.credentials.json`",
        read_from: "the plaintext backend's write path, which chmods 384 after writing",
        verified_against: VERIFIED_AGAINST,
        depends: "store::file, slot::CRED_FILE, and atomic::Perms::Secret matching what                   Claude Code itself writes",
        probe: &[
            ".credentials.json",
            "Warning: Storing credentials in plaintext.",
        ],
        absent: &[],
    },
];

/// The assumption of that name, for a check or a probe that wants to speak about one.
pub fn named(name: &str) -> Option<&'static Assumption> {
    ASSUMPTIONS.iter().find(|a| a.name == name)
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
        for a in ASSUMPTIONS {
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
            verified_against: VERIFIED_AGAINST,
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
        let mut names: Vec<&str> = ASSUMPTIONS.iter().map(|a| a.name).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before, "two assumptions share a name");

        for a in ASSUMPTIONS {
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
