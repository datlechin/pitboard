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
    },
    Assumption {
        name: "keychain_account_name",
        fact: "the keychain account is $USER when it matches /^[a-zA-Z0-9._-]+$/, and \
               `claude-code-user` when it does not",
        read_from: "the secure storage module's account name",
        verified_against: VERIFIED_AGAINST,
        depends: "slot::account_name",
    },
    Assumption {
        name: "live_chain_order",
        fact: "the login is read from the keychain first and a plaintext `.credentials.json` \
               second; the successor backend replaces the fallback half and only for a caller \
               that hands a backend in, which an ordinary `claude` does not",
        read_from: "the keychain-with-plaintext-fallback store",
        verified_against: VERIFIED_AGAINST,
        depends: "store::resolve and everything that reads or writes through it",
    },
    Assumption {
        name: "keychain_write_route",
        fact: "a write is `security -i` while the command is at most 4032 bytes and \
               `add-generic-password -U -a <account> -s <service> -X <hex>` above it, with a \
               2 second timeout, and only a timeout counts as retryable",
        read_from: "the keychain backend's update path",
        verified_against: VERIFIED_AGAINST,
        depends: "store::keychain::MAX_COMMAND_BYTES and the write path",
    },
    Assumption {
        name: "keychain_absence_codes",
        fact: "`find-generic-password` exiting 0 with no output, or 44, means absent; 36 means \
               the keychain is locked and says nothing about what is there",
        read_from: "the keychain backend's read path",
        verified_against: VERIFIED_AGAINST,
        depends: "store::keychain::classify",
    },
    Assumption {
        name: "write_lock",
        fact: "every credential write takes proper-lockfile's directory lock at \
               `<storage dir>/.storage-write`, stale 15000ms, ten retries, 100ms to 1000ms of \
               backoff, and re-reads the credential inside the lock before changing it",
        read_from: "the secure storage module's write wrapper",
        verified_against: VERIFIED_AGAINST,
        depends: "lock.rs and the whole switch",
    },
    Assumption {
        name: "logout_skips_the_lock",
        fact: "`/logout` retries for its own 7.5 seconds and then deletes the credential with \
               no lock held at all",
        read_from: "the secure storage module's already-locked escape hatch",
        verified_against: VERIFIED_AGAINST,
        depends: "the slot re-read in switch, which exists for this",
    },
    Assumption {
        name: "account_scoped_keys",
        fact: "a logout deletes claudeAiOauth, organizationUuid, trustedDeviceToken, \
               enterpriseGateway and designOauth, so those belong to the account",
        read_from: "the logout path",
        verified_against: VERIFIED_AGAINST,
        depends: "switch::ACCOUNT_SCOPED",
    },
    Assumption {
        name: "credential_cache",
        fact: "a running session serves the credential from a 30 second cache, so a swap is \
               picked up within about 33 seconds",
        read_from: "the keychain backend's cache, and measured against a running session",
        verified_against: VERIFIED_AGAINST,
        depends: "switch::ADOPTION_CEILING_SECONDS",
    },
    Assumption {
        name: "config_file_location",
        fact: "a legacy `<config dir>/.config.json` wins when present; otherwise \
               `<$CLAUDE_CONFIG_DIR or $HOME>/.claude<suffix>.json`",
        read_from: "the config path resolution",
        verified_against: VERIFIED_AGAINST,
        depends: "claude::config_file",
    },
    Assumption {
        name: "oauth_client",
        fact: "every login Claude Code stores was issued to client 9d1c250a-e61b-44d9-88ed-5944d1962f5e, \
               and a refresh answer without a refresh-token lifetime keeps the one it had",
        read_from: "the OAuth client id and the token refresh path",
        verified_against: VERIFIED_AGAINST,
        depends: "api::CLIENT_ID and park::renewed",
    },
    Assumption {
        name: "supervisor_daemon",
        fact: "a supervisor daemon outlives the session that started it, refreshes the login on \
               a timer of roughly eight hours, records itself in `<config dir>/daemon.lock`, and \
               writes through the same lock as everything else",
        read_from: "the daemon's auth scheduler and its lock file",
        verified_against: VERIFIED_AGAINST,
        depends: "daemon.rs, and the lock discipline the switch relies on",
    },
];

/// The assumption of that name, for a check or a probe that wants to speak about one.
pub fn named(name: &str) -> Option<&'static Assumption> {
    ASSUMPTIONS.iter().find(|a| a.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn an_assumption_can_be_looked_up_by_name() {
        assert_eq!(
            named("write_lock").expect("it is listed").name,
            "write_lock"
        );
        assert_eq!(named("nothing_like_this"), None);
    }
}
