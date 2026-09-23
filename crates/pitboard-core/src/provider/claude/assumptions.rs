//! Every fact about Claude Code that pitboard stands on, named and dated.
//!
//! The keychain item's name and how the slot is hashed from a directory, the five keys a
//! logout deletes, the write lock and its constants, the one write that skips the lock, the
//! 4032-byte ceiling on a `security` command, the thirty seconds a session caches a
//! credential for. All of it was read out of one build.
//!
//! None of it transfers. The next provider's equivalents have to be read out of its own
//! build the same way, into a list of its own, dated on its own schedule.
//!
//! See [`crate::assumptions`] for what an entry means and how a probe reads one.

use crate::assumptions::Assumption;

/// The build every entry below was read from, unless it says otherwise.
pub const VERIFIED_AGAINST: &str = "2.1.278";

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
