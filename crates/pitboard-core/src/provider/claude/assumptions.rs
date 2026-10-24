//! Every fact about Claude Code that Pitboard stands on, named and dated.
//!
//! The keychain item's name and how the slot is hashed from a directory, the five keys a
//! logout deletes, the write lock, the refresh lock and their constants, the one write that
//! skips the write lock, the 4032-byte ceiling on a `security` command, the thirty seconds a
//! session caches a credential for. Each was read out of the build its entry names.
//!
//! None of it transfers. The next provider's equivalents have to be read out of its own
//! build the same way, into a list of its own, dated on its own schedule.
//!
//! See [`crate::assumptions`] for what an entry means and how a probe reads one.

use crate::assumptions::OnSystem::{NotRead, Pending, Read};
use crate::assumptions::{Assumption, OnSystem, PerSystem};

/// The build every entry below was read from on macOS and Linux, unless it says otherwise.
pub(crate) const VERIFIED_AGAINST: &str = "2.1.284";

// Both Windows builds of 2.1.303 (commit 07df2ae), compared as bytes on 2026-10-20 with the build
// each reading named before. They embed Bun 1.4.4, whose source is not public, so a fact's Bun
// half stays its entry's reading of bun-v1.4.3.
pub(crate) const WINDOWS_VERIFIED_AGAINST: &str = "2.1.303";

/// The Linux build has no keychain backend, so a keychain fact read from it reports the
/// keychain gone, as it did on every build from 2.1.278 on.
const NO_KEYCHAIN_ON_LINUX: OnSystem =
    NotRead("Linux has no keychain, and Claude Code's Linux build has no keychain backend");

/// Nor has the Windows build. Both Windows builds of 2.1.289 keep code they share with the
/// macOS one: the fallback wrapper's `keychain_locked_skip_fallback` and
/// `primary_transient_skip_fallback`, and a device key store that names
/// `find-generic-password`. What they lack is the backend itself: none holds `exceeds
/// security -i stdin limit; using argv`, `show-keychain-info` or `[keychain] readAsync
/// failed`, which the macOS build holds twice each.
const NO_KEYCHAIN_ON_WINDOWS: OnSystem =
    NotRead("Windows has no keychain, and Claude Code's Windows build has no keychain backend");

/// A fact read on macOS and Linux from the build its entry names, and on Windows from
/// [`WINDOWS_VERIFIED_AGAINST`].
const fn everywhere(name: &'static str, build: &'static str) -> PerSystem {
    PerSystem {
        name,
        macos: Read(build),
        linux: Read(build),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    }
}

// Read on 12 October 2026 from 2.1.296's macOS and Linux builds, and measured on macOS; read as
// bytes on 19 October 2026 from 2.1.303's win32-x64 and win32-arm64 builds.
const fn warm_up(name: &'static str) -> PerSystem {
    PerSystem {
        name,
        macos: Read("2.1.296"),
        linux: Read("2.1.296"),
        windows: Read("2.1.303"),
    }
}

// Anthropic's, not the build's, so the same whatever system sends the request.
const fn of_the_service(name: &'static str) -> PerSystem {
    PerSystem {
        name,
        macos: Read("2.1.296"),
        linux: Read("2.1.296"),
        windows: Read("2.1.296"),
    }
}

// What Anthropic answers and how Claude Code reads it, in 2.1.301's builds and Windows' 2.1.303.
const fn answers(name: &'static str) -> PerSystem {
    PerSystem {
        name,
        macos: Read("2.1.301"),
        linux: Read("2.1.301"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    }
}

// Read as bytes on 2026-10-23 from the darwin-arm64, linux-x64, win32-x64 and win32-arm64
// builds of 2.1.309.
const fn read_from_2_1_309(name: &'static str) -> PerSystem {
    PerSystem {
        name,
        macos: Read("2.1.309"),
        linux: Read("2.1.309"),
        windows: Read("2.1.309"),
    }
}

/// What each fact below is on each system. The Windows readings are a reading of the
/// Windows builds' code for each fact, and of every literal its entry probes for, which
/// both Windows builds hold. Where Claude Code does something else on Windows, that is a
/// fact of its own, read by the pull request named.
pub(crate) const PER_SYSTEM: &[PerSystem] = &[
    PerSystem {
        name: "credential_service_name",
        macos: Read(VERIFIED_AGAINST),
        linux: NO_KEYCHAIN_ON_LINUX,
        windows: NotRead(
            "the Windows build keeps no keychain item; the Credential Manager item it keeps the \
             login in under the same service is credman_target's",
        ),
    },
    PerSystem {
        name: "keychain_account_name",
        macos: Read(VERIFIED_AGAINST),
        linux: NO_KEYCHAIN_ON_LINUX,
        windows: NotRead(
            "the Windows build names its Credential Manager item's user with a literal, which \
             credman_account_name reads",
        ),
    },
    PerSystem {
        name: "live_chain_order",
        macos: Read(VERIFIED_AGAINST),
        linux: Read(VERIFIED_AGAINST),
        windows: NotRead(
            "which store Claude Code keeps the login in on Windows is windows_backend_choice's, \
             and how it moves the login into Credential Manager is credman_migration's",
        ),
    },
    PerSystem {
        name: "keychain_write_route",
        macos: Read("2.1.309"),
        linux: NO_KEYCHAIN_ON_LINUX,
        windows: NO_KEYCHAIN_ON_WINDOWS,
    },
    PerSystem {
        name: "keychain_absence_codes",
        macos: Read("2.1.309"),
        linux: NO_KEYCHAIN_ON_LINUX,
        windows: NO_KEYCHAIN_ON_WINDOWS,
    },
    // Read again in both Windows builds of 2.1.300 and 2.1.303. Under Bun 1.4.3, proper-lockfile
    // 4.1.2 and Pitboard's lock see each other's lock with this timing on both runner images
    // (RF-C5, run 38693947329).
    PerSystem {
        name: "write_lock",
        macos: Read(VERIFIED_AGAINST),
        linux: Read(VERIFIED_AGAINST),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    everywhere("logout_skips_the_lock", VERIFIED_AGAINST),
    PerSystem {
        name: "account_scoped_keys",
        macos: Read("2.1.303"),
        linux: Read("2.1.303"),
        windows: Read("2.1.303"),
    },
    PerSystem {
        name: "credential_cache",
        macos: Read("2.1.278"),
        linux: Read("2.1.278"),
        windows: NotRead(
            "how long a session keeps a login read from Credential Manager is \
             windows_credman_cache's, and how soon it takes a new `.credentials.json` \
             windows_file_adoption's",
        ),
    },
    PerSystem {
        name: "config_file_location",
        macos: Read("2.1.301"),
        linux: Read("2.1.301"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "settings_byte_order_mark",
        macos: Read("2.1.302"),
        linux: Read("2.1.302"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    everywhere("oauth_client", VERIFIED_AGAINST),
    PerSystem {
        name: "supervisor_daemon",
        macos: Read("2.1.309"),
        linux: Read("2.1.309"),
        windows: NotRead(
            "which process the lock names on Windows is windows_daemon_identity's, read from \
             the Windows builds",
        ),
    },
    PerSystem {
        name: "no_keyring_off_macos",
        macos: NotRead(
            "this is about the systems without a keychain, and macOS keeps the login in one",
        ),
        linux: Read(VERIFIED_AGAINST),
        windows: NotRead(
            "Claude Code's Credential Manager store on Windows calls `Bun.secrets`, which this \
             fact rules out, so read from the Windows build it reports a keyring arrived",
        ),
    },
    // Read again in both Windows builds of 2.1.300, 2.1.303 and 2.1.309, where a line is read with
    // readline and trimmed, so one typed back with `\r\n` is one with `\n`.
    read_from_2_1_309("sign_in_output"),
    PerSystem {
        name: "sign_in_takes_another_code",
        macos: Read("2.1.289"),
        linux: Read("2.1.289"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "project_settings_folder",
        macos: Read("2.1.310"),
        linux: Read("2.1.310"),
        windows: Read("2.1.310"),
    },
    PerSystem {
        name: "install_places",
        macos: Read("2.1.289"),
        linux: Read("2.1.289"),
        windows: NotRead(
            "the native installer, npm and winget put `claude.exe` elsewhere on Windows, which \
             windows_install_layout reads",
        ),
    },
    PerSystem {
        name: "plaintext_credential_mode",
        macos: Read(VERIFIED_AGAINST),
        linux: Read(VERIFIED_AGAINST),
        windows: NotRead(
            "a mode keeps nobody out of a file on Windows; how Claude Code writes \
             `.credentials.json` there is windows_plaintext_write's",
        ),
    },
    PerSystem {
        name: "locked_keychain_keeps_last_login",
        macos: Read("2.1.294"),
        linux: NO_KEYCHAIN_ON_LINUX,
        windows: NO_KEYCHAIN_ON_WINDOWS,
    },
    PerSystem {
        name: "locked_sign_in_writes_fallback",
        macos: Read("2.1.294"),
        linux: NO_KEYCHAIN_ON_LINUX,
        windows: NotRead(
            "Windows has no keychain; a session out of Credential Manager's reach writes the \
             file, credman_unreachable_writes_file's",
        ),
    },
    PerSystem {
        name: "fallback_outlives_keychain_writes",
        macos: Read("2.1.294"),
        linux: NO_KEYCHAIN_ON_LINUX,
        windows: NO_KEYCHAIN_ON_WINDOWS,
    },
    // Read again in both Windows builds of 2.1.301 and 2.1.303, where the file sits behind
    // Credential Manager wherever that store is on.
    PerSystem {
        name: "fallback_file_pins_session_login",
        macos: Read("2.1.294"),
        linux: NO_KEYCHAIN_ON_LINUX,
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "login_is_one_organisation",
        macos: Read("2.1.294"),
        linux: Read("2.1.294"),
        windows: Read("2.1.294"),
    },
    PerSystem {
        name: "config_may_name_no_organisation",
        macos: Read("2.1.294"),
        linux: Read("2.1.294"),
        windows: Read("2.1.294"),
    },
    read_from_2_1_309("usage_cache_stamp_is_the_configs"),
    read_from_2_1_309("status_reads_the_config_usage_the_token"),
    read_from_2_1_309("config_identity_is_the_last_writers"),
    PerSystem {
        name: "status_line_input_names_no_account",
        macos: Read("2.1.294"),
        linux: Read("2.1.294"),
        windows: Read("2.1.294"),
    },
    // Read again in both Windows builds of 2.1.300, 2.1.303 and 2.1.309, and measured with this
    // timing as the write lock was in 2.1.303 (RF-C5r, run 38693947329). From 2.1.301 it takes a
    // synchronous `fs`.
    read_from_2_1_309("refresh_lock"),
    read_from_2_1_309("login_carries_its_plan"),
    PerSystem {
        name: "windows_home_folder",
        macos: NotRead("macOS keeps the home in `HOME`, which config_file_location reads"),
        linux: NotRead("Linux keeps the home in `HOME`, which config_file_location reads"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "windows_open_sharing",
        macos: NotRead("a rename lands over a file open on macOS whatever opened it"),
        linux: NotRead("a rename lands over a file open on Linux whatever opened it"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "windows_install_layout",
        macos: NotRead("where macOS's installers put `claude` is install_places'"),
        linux: NotRead("where Linux's installers put `claude` is install_places'"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "windows_version_resource",
        macos: NotRead("a macOS program carries no version resource"),
        linux: NotRead("a Linux program carries no version resource"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "windows_managed_settings",
        macos: NotRead(
            "macOS has no registry: its managed folder is settings.rs's, and its profile's plist \
             macos_managed_preferences'",
        ),
        linux: NotRead("Linux has no registry, and its managed folder is settings.rs's"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "windows_daemon_identity",
        macos: NotRead("off Windows the daemon records `procStart`, which Pitboard does not read"),
        linux: NotRead("off Windows the daemon records `procStart`, which Pitboard does not read"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "windows_backend_choice",
        macos: NotRead("macOS's chain is live_chain_order's"),
        linux: NotRead("Linux's store is the file alone, which no_keyring_off_macos reads"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "windows_plaintext_strict_read",
        macos: NotRead(
            "off Windows no reader keeps another out of a file, and a refused read is no login",
        ),
        linux: NotRead(
            "off Windows no reader keeps another out of a file, and a refused read is no login",
        ),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "windows_plaintext_write",
        macos: NotRead("how macOS's file is written is plaintext_credential_mode's"),
        linux: NotRead("how Linux's file is written is plaintext_credential_mode's"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "windows_file_adoption",
        macos: NotRead(
            "on macOS the file sits behind the keychain, fallback_file_pins_session_login's",
        ),
        linux: NotRead("how soon a session takes a new login off Windows is credential_cache's"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "credman_account_name",
        macos: NotRead("macOS's account is keychain_account_name's"),
        linux: NotRead("Linux keeps the login in a file, which has no user name"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "credman_target",
        macos: NotRead("macOS's item is credential_service_name's"),
        linux: NotRead("Linux keeps the login in a file"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "credman_chunking",
        macos: NotRead("a keychain item takes a login of any size, keychain_write_route's"),
        linux: NotRead("Linux keeps the login in a file"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "credman_probe",
        macos: NotRead("macOS's chain is live_chain_order's"),
        linux: NotRead("Linux keeps the login in a file"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "credman_migration",
        macos: NotRead("macOS's chain is live_chain_order's"),
        linux: NotRead("Linux keeps the login in a file"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "credman_move_lost_a_piece",
        macos: NotRead("Linux and macOS keep no login in Credential Manager"),
        linux: NotRead("Linux and macOS keep no login in Credential Manager"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "credman_unreachable_writes_file",
        macos: NotRead("macOS's counterpart is locked_sign_in_writes_fallback"),
        linux: NotRead("Linux keeps the login in a file"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "credman_attributes",
        macos: NotRead("macOS's item is written by `security`, keychain_write_route's"),
        linux: NotRead("Linux keeps the login in a file"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "windows_credman_cache",
        macos: NotRead("how soon a session takes a new login on macOS is credential_cache's"),
        linux: NotRead("how soon a session takes a new login on Linux is credential_cache's"),
        windows: Pending {
            by: &["W27"],
            reads: "how soon a running session takes a login a switch wrote to Credential \
                    Manager, session step 7, which the session of 2026-10-18 did not reach: its \
                    switches wrote `.credentials.json` past a settings file Pitboard read no \
                    `env` from (settings_byte_order_mark), then refused over the pieces Claude \
                    Code had lost a login in (credman_move_lost_a_piece)",
        },
    },
    PerSystem {
        name: "windows_browser_open",
        macos: NotRead(
            "off Windows a cancel stops the tool alone, so how it opens a browser decides nothing",
        ),
        linux: NotRead(
            "off Windows a cancel stops the tool alone, so how it opens a browser decides nothing",
        ),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "windows_token_handover",
        macos: NotRead("macOS and Linux hand a descriptor as headless_token_descriptor says"),
        linux: NotRead("macOS and Linux hand a descriptor as headless_token_descriptor says"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "windows_token_env",
        macos: NotRead("macOS and Linux hand a descriptor as headless_token_descriptor says"),
        linux: NotRead("macOS and Linux hand a descriptor as headless_token_descriptor says"),
        windows: Read("2.1.310"),
    },
    PerSystem {
        name: "windows_statusline_shell",
        macos: NotRead(
            "off Windows a status line runs in the person's shell, which finds pitboard",
        ),
        linux: NotRead(
            "off Windows a status line runs in the person's shell, which finds pitboard",
        ),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "windows_statusline_powershell",
        macos: NotRead("only Windows runs a status line in PowerShell"),
        linux: NotRead("only Windows runs a status line in PowerShell"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "proxy_from_variables",
        macos: Read("2.1.301"),
        linux: Read("2.1.301"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    everywhere("remote_token_files", "2.1.303"),
    PerSystem {
        name: "headless_token_descriptor",
        macos: Read("2.1.296"),
        linux: Read("2.1.296"),
        windows: NotRead(
            "Claude Code on Windows reads no token handed on a descriptor \
             (windows_token_handover), so a warm-up there hands it as windows_token_env says",
        ),
    },
    warm_up("headless_print_mode"),
    warm_up("headless_result"),
    of_the_service("request_starts_the_five_hour_period"),
    of_the_service("weekly_period_is_a_fixed_cycle"),
    of_the_service("used_up_limit_refuses_every_request"),
    answers("refusals_are_final"),
    answers("account_refusals"),
    answers("token_refusals"),
    answers("validate_names_the_login"),
    answers("token_answer_names_the_login"),
    // Read as bytes on 2026-10-24 from the darwin-arm64, linux-x64, win32-x64 and win32-arm64
    // builds of 2.1.309.
    PerSystem {
        name: "refresh_token_end",
        macos: Read("2.1.309"),
        linux: Read("2.1.309"),
        windows: Read("2.1.309"),
    },
    read_from_2_1_309("auth_precedence"),
    read_from_2_1_309("anthropic_profiles"),
    PerSystem {
        name: "profile_folder",
        macos: Read("2.1.309"),
        linux: Read("2.1.309"),
        windows: NotRead(
            "the Windows build looks in `APPDATA`, which windows_profile_folder reads",
        ),
    },
    PerSystem {
        name: "windows_profile_folder",
        macos: NotRead("where macOS's profiles are is profile_folder's"),
        linux: NotRead("where Linux's profiles are is profile_folder's"),
        windows: Read("2.1.309"),
    },
    read_from_2_1_309("organisation_pin"),
    // Read as bytes on 2026-10-24 from the darwin-arm64, linux-x64, win32-x64 and win32-arm64
    // builds of 2.1.310.
    PerSystem {
        name: "login_method_pin",
        macos: Read("2.1.310"),
        linux: Read("2.1.310"),
        windows: Read("2.1.310"),
    },
    PerSystem {
        name: "managed_login_pins",
        macos: Read("2.1.310"),
        linux: Read("2.1.310"),
        windows: Read("2.1.310"),
    },
    PerSystem {
        name: "macos_managed_preferences",
        macos: Read("2.1.309"),
        linux: NotRead("Linux has no managed preferences, and its managed folder is settings.rs's"),
        windows: NotRead(
            "Windows has no managed preferences, and its policy keys are \
                          windows_managed_settings'",
        ),
    },
];

pub(crate) const ASSUMPTIONS: &[Assumption] = &[
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
        fact: "on macOS the login is read from the keychain first and a plaintext \
               `.credentials.json` second, and a write moves it to the file only when the \
               keychain write fails for good; on Linux the file is the only store. The \
               successor backend replaces the fallback half and only for a caller that hands a \
               backend in, which an ordinary `claude` does not",
        read_from: "the keychain-with-plaintext-fallback store and `getSecureStorage`",
        verified_against: VERIFIED_AGAINST,
        depends: "claude::live::chain, store::holder, read_raw and write_raw, and everything \
                  that reads or writes through them",
        probe: &[".credentials.json", "-with-", "-fallback"],
        absent: &[],
    },
    Assumption {
        name: "keychain_write_route",
        fact: "a write gives `security -i` the line `add-generic-password -U -a \"<account>\" \
               -s \"<service>\" -X \"<hex>\"` on stdin while that line is at most 4032 bytes, \
               and the same command on the argument line above that; it never refuses or \
               splits a login. Each call has a 2 second timeout. A failed write is transient, \
               and skips the plaintext fallback, when it timed out or, from 2.1.281, when it \
               exited 36 after this process had seen the item; any other failure moves the \
               login to `.credentials.json`",
        read_from: "the keychain backend's update path and the fallback wrapper around it",
        // Read again on 2026-10-23 from the darwin-arm64 build of 2.1.309.
        verified_against: "2.1.309",
        depends: "host::macos::keychain::MAX_COMMAND_BYTES and the write path",
        probe: &[
            "exceeds security -i stdin limit; using argv",
            "primary_transient_skip_fallback",
            "keychain_locked_skip_fallback",
        ],
        absent: &[],
    },
    Assumption {
        name: "keychain_absence_codes",
        fact: "`find-generic-password -a <account> -w -s <service>`, with a 2 second timeout: \
               exit 0 with output is the login; exit 0 with nothing, exit 44, or output that is \
               not JSON is absent. Exit 36, a locked keychain, is absent to an ordinary read, a \
               failed read to a write once this process has seen the item \
               (`failureIfTransient`, from 2.1.281), and a failed read outright only when a \
               caller asks for that. Any other exit is a failed read. `show-keychain-info` \
               exiting 36 only adds an unlock hint. Pitboard reads 36 as a locked keychain, \
               never as absent, the strict end of that",
        read_from: "the keychain backend's read path",
        // Read again on 2026-10-23 from the darwin-arm64 build of 2.1.309.
        verified_against: "2.1.309",
        depends: "host::macos::keychain::classify",
        probe: &[
            "failureIfTransient",
            "[keychain] readAsync failed; not caching a null",
            "show-keychain-info",
        ],
        absent: &[],
    },
    Assumption {
        name: "write_lock",
        fact: "every credential write takes proper-lockfile's directory lock at \
               `<storage dir>/.storage-write`, stale 15000ms, ten retries, 100ms to 1000ms of \
               backoff, and re-reads the credential inside the lock before changing it. A \
               re-read that fails abandons the write (`read_failed_skip_write`); from 2.1.281 a \
               locked keychain counts as a failed re-read once this process has seen the item, \
               where before it read as empty",
        read_from: "the secure storage module's write wrapper",
        verified_against: VERIFIED_AGAINST,
        depends: "lock.rs, the whole switch, and `pitboard stow`, which takes it once before it \
                  renews a login left in `.credentials.json`, and holds it while it writes that \
                  login back renewed and while it reads the file and the login stored a last time \
                  and deletes the file",
        probe: &[
            ".storage-write",
            "[secureStorage] write lock compromised: ",
            "read_failed_skip_write",
            "failureIfTransient",
        ],
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
        name: "refresh_lock",
        fact: "a renewal of the login takes proper-lockfile's directory lock \
               `<storage dir>/.oauth_refresh.lock`, stale 60000ms and touched every 5000ms, then \
               the legacy `<storage dir, links resolved>.lock` the same way, letting go of the \
               first where the second is held and going without the second where it cannot be \
               made. Under them it reads the login again, sends its refresh token, and saves the \
               answer through the write lock only where the login stored still holds the token \
               it sent, writing nothing otherwise. So the write lock does not keep a renewal \
               from spending a refresh token: this lock does. A holder is taken over before \
               the lock is stale only where its owner record, \
               `<storage dir>/.oauth_refresh.lock.owner`, proves it gone",
        read_from: "the refresh lock's options and the function taking both locks, the token \
                    refresh and the scope expansion, which take it before they read the login \
                    again and send the refresh token, the refresh's compare-and-set save through \
                    the storage write wrapper, and the dead holder takeover, which reads the \
                    owner record",
        // Read on 2026-10-09 from the macOS, Linux x64, Windows x64 and Windows arm64 builds of
        // 2.1.294, whose code here is the same, and again on 2026-10-23 from the four builds of
        // 2.1.309.
        verified_against: "2.1.309",
        depends: "lock::REFRESH, a switch, which holds it from before it reads the outgoing login \
                  a last time until it has recorded the login installed, so no session spends the \
                  refresh token of the copy it parks, `pitboard stow`, which holds it from its \
                  last reading of a login left in `.credentials.json` until the file is gone, so \
                  no session spends the refresh token of the login it parks, drops or renews \
                  meanwhile, and switch::refresh, which renews a login itself as this renewal \
                  does, the stored login where a change or a read must tell whose it is and its \
                  access token has lapsed, and the file's for `pitboard stow`: it reads the login \
                  again under the lock, tells one a session renewed meanwhile as it is, and saves \
                  the one it renewed as this save does, where the store still holds the token it \
                  sent, so a session that renews later reads the renewed login and sends its \
                  refresh token. None writes an owner record, so no session takes the lock over",
        probe: &[
            "\".oauth_refresh.lock\"),realpath:!1,stale:60000,update:5000",
            "tengu_oauth_refresh_legacy_lock_contended",
            "tengu_oauth_refresh_save_adopted_newer_write",
        ],
        absent: &[],
    },
    Assumption {
        name: "account_scoped_keys",
        fact: "signing in again deletes claudeAiOauth, organizationUuid, trustedDeviceToken, \
               trustedDeviceEnrolledAfterRefusalAtMs, enterpriseGateway, designOauth and \
               cloudSessionsOauth, and `/logout` clears the whole document but writes back \
               `coworkRemoteDevice`, so those seven belong to the account. 2.1.296 deleted \
               five, without trustedDeviceEnrolledAfterRefusalAtMs and cloudSessionsOauth",
        read_from: "the re-login prune and the logout path, which also revokes \
                    cloudSessionsOauth's refresh token, in the darwin-arm64, linux-x64, \
                    win32-x64 and win32-arm64 builds of 2.1.303, read as bytes on 2026-10-20; \
                    the win32-x64 builds of 2.1.300 to 2.1.302 prune the same seven",
        verified_against: "2.1.303",
        depends: "document::ACCOUNT_SCOPED, and what a park holds",
        probe: &[
            "claudeAiOauth",
            "organizationUuid",
            "trustedDeviceToken",
            "trustedDeviceEnrolledAfterRefusalAtMs",
            "enterpriseGateway",
            "designOauth",
            "cloudSessionsOauth",
        ],
        absent: &[],
    },
    Assumption {
        name: "credential_cache",
        fact: "a running session serves the credential from a 30 second cache, so a swap is \
               picked up within about 33 seconds, only where no `.credentials.json` is behind \
               the keychain; see `fallback_file_pins_session_login`. The 30 seconds are \
               unchanged in 2.1.284; its re-checks after 1, 3 and 10 seconds, behind \
               `tengu_streamed_thimble` from 2.1.281, can only shorten that",
        read_from: "the keychain backend's cache, and measured against a running session",
        // The 33 seconds were measured against a running 2.1.278; nothing later was run.
        verified_against: "2.1.278",
        depends: "switch::ADOPTION_CEILING_SECONDS and Claude's adoption where nothing is \
                  behind the keychain, and autoswitch, which switches before a limit rather \
                  than at it so a session already running follows in time",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "config_file_location",
        fact: "the config file is `<$CLAUDE_CONFIG_DIR or the home folder>/.claude<suffix>.json`, \
               but a legacy `<config dir>/.config.json` wins where it is a file, links followed, \
               that is not empty, or is empty and not read-only while `.claude<suffix>.json` is \
               missing. The two read `CLAUDE_CONFIG_DIR` differently: the \
               `.claude<suffix>.json` base is `CLAUDE_CONFIG_DIR || homedir()`, so an empty \
               value is the home folder, while the config dir is \
               `(CLAUDE_CONFIG_DIR ?? join(homedir(), \".claude\")).normalize(\"NFC\")`, so an \
               empty value makes it the empty path and `.config.json` is looked for in the \
               working directory",
        read_from: "the config path resolution, and the config dir the secure storage module \
                    shares",
        // Read on 2026-10-15 from the darwin-arm64, linux-x64, win32-x64 and win32-arm64 builds
        // of 2.1.299 and 2.1.300, and on 2026-10-16 from those of 2.1.301, whose code for both
        // paths is the same.
        verified_against: "2.1.301",
        depends: "claude::config_file and claude::config_dir, which read CLAUDE_CONFIG_DIR \
                  the two ways, home::check_absolute, which refuses an empty one, and \
                  claude::credman's read of the cached feature, from the file config_file names",
        probe: &[".config.json", "CLAUDE_CONFIG_DIR"],
        absent: &[],
    },
    Assumption {
        name: "settings_byte_order_mark",
        fact: "Claude Code reads a settings file, the person's `settings.json` and a managed \
               one alike, as UTF-16LE where it starts with FF FE and as UTF-8 otherwise, and \
               drops one U+FEFF at its start before it parses it, so the byte order mark \
               Windows PowerShell 5.1's `Set-Content -Encoding utf8` writes leaves its `env` \
               read. Whitespace before the mark, or a second mark, leaves the file unread. Its \
               config file's loader reads UTF-8 and drops one mark the same way, but the \
               Credential Manager gate (windows_backend_choice) parses that file as it is, so a \
               config that starts with a mark reads `tengu_windows_credman` as off while its \
               `env` still counts. A `.credentials.json` that starts with one is no login",
        read_from: "the settings file reader and its encoding sniff, safeParseJSON, the config \
                    loader, the backend gate's read of the config file and the plaintext \
                    store's read, in the darwin-arm64, linux-x64 and win32-x64 builds of \
                    2.1.302, read as bytes on 2026-10-18. Measured the same day on the session's \
                    PC, Windows 11 build 26300 on ARM64: with `CLAUDE_CODE_FORCE_WINDOWS_CREDMAN` \
                    1 in the `env` of a `settings.json` written that way, a new session of \
                    2.1.302 moved its login into Credential Manager",
        verified_against: "2.1.302",
        depends: "settings::own and the managed settings it reads, which drop the mark as \
                  Claude Code does and read the person's own UTF-16LE file, \
                  claude::paths::load_config and configfile::update, and credman::turned_on, \
                  which reads the config's `env` past a mark and its cached feature only from a \
                  config with none",
        probe: &[
            "safeParseJSON: invalid JSON (",
            r#"return"utf16le""#,
            r#"startsWith("\uFEFF")?e.slice(1):e}"#,
        ],
        absent: &[],
    },
    Assumption {
        name: "oauth_client",
        fact: "every login Claude Code stores was issued to client 9d1c250a-e61b-44d9-88ed-5944d1962f5e",
        read_from: "the OAuth client id and the token refresh path",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::claude::api::CLIENT_ID, and the renewal `pitboard stow` makes of a \
                  login left in `.credentials.json` whose access token has expired",
        probe: &["9d1c250a-e61b-44d9-88ed-5944d1962f5e"],
        absent: &[],
    },
    Assumption {
        name: "supervisor_daemon",
        fact: "a supervisor daemon outlives the session that started it, refreshes the login on \
               a timer of roughly eight hours, records itself in `<config dir>/daemon.lock`, and \
               writes through the same lock as everything else",
        read_from: "the daemon's auth scheduler and its lock file",
        // Read again on 2026-10-23 from the darwin-arm64 and linux-x64 builds of 2.1.309.
        verified_against: "2.1.309",
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
        fact: "Claude Code's code has no Secret Service, libsecret, gnome-keyring or KWallet \
               backend. Its storage backends are `keychain`, `plaintext` and `windows-credman`, \
               the last behind the `tengu_windows_credman` flag, and on Linux it uses \
               `plaintext` alone, so the login is the file. The `libsecret` in the Linux binary \
               belongs to the Bun runtime it ships in, behind `Bun.secrets`, which Claude \
               Code's code never calls",
        read_from: "the secure storage module's backend list and `getSecureStorage`, and the \
                    whole build searched for every Linux keyring name",
        verified_against: VERIFIED_AGAINST,
        depends: "host::linux, and Pitboard's claim that a parked login on Linux is no \
                  less protected than the live one",
        probe: &[
            "tengu_windows_credman",
            "CLAUDE_CODE_FORCE_WINDOWS_CREDMAN",
            r#"["keychain","plaintext","windows-credman"]"#,
        ],
        absent: &[
            "Bun.secrets",
            "org.freedesktop.secrets",
            "gnome-keyring",
            "SecretService",
        ],
    },
    Assumption {
        name: "sign_in_output",
        fact: "`claude auth login` writes `Opening browser to sign in…`, then `If the browser \
               didn't open, visit: <address>`, then `Paste code here if prompted > ` with no \
               newline, all to stdout and before it opens the browser, and from then on reads \
               a pasted `<code>#<state>` line from stdin. The address is the manual one: \
               `https`, on claude.com for a claude.ai login and platform.claude.com for a \
               Console one, coming back to platform.claude.com's page that shows the code. The \
               browser it opens goes to another address, which comes back to the loopback. \
               The address goes through a hyperlink helper, which writes it bare, or, where \
               its check says the terminal takes hyperlinks, as an OSC 8 hyperlink: \
               `ESC ] 8 ; ;`, the address, BEL, the address again as the link's text, bright \
               blue where colour is on, then `ESC ] 8 ; ;` and BEL. Piped, the check says yes \
               when `FORCE_HYPERLINK` is set to anything but 0 or nothing, which decides it \
               when set; otherwise when `NETLIFY` is set, `TERM_PROGRAM` or `LC_TERMINAL` is \
               ghostty, Hyper, kitty, alacritty, iTerm.app, iTerm2 or WarpTerminal, \
               `TERMINAL_EMULATOR` is JetBrains-JediTerm, `WT_SESSION` is set outside tmux, \
               `TERM_PROGRAM` is tmux 3.4 or later, or `TERM` contains kitty. A terminal \
               attached to a background session answers for the check instead, and \
               `auth login` has none. Only where its stdin is a terminal and the browser cannot \
               come back to the loopback does it write `Sign in at: <address>` and `Paste code \
               here > ` instead, and Pitboard's sign-in hands it a pipe",
        read_from: "the `auth login` command's OAuth flow and `startOAuthFlow`, the authorize \
                    address builder and its constants, the hyperlink helper the address is \
                    printed through with `assumeSupport`, and the supports-hyperlinks check \
                    it asks",
        // Read from 2.1.289, and again on 2026-10-23 from the four builds of 2.1.309.
        verified_against: "2.1.309",
        depends: "provider::claude::engine's read_sign_in, provider::printed, which reads \
                  the hyperlink, and the address and code field the app's sign-in sheet \
                  offers",
        probe: &[
            "If the browser didn't open, visit: ",
            "Paste code here if prompted > ",
            r#"CLAUDE_AI_AUTHORIZE_URL:"https://"#,
            r#"CONSOLE_AUTHORIZE_URL:"https://"#,
            r#"MANUAL_REDIRECT_URL:"https://"#,
            // A sign-in address going through the hyperlink helper with `assumeSupport`, from
            // after the helper's name, which the minifier chooses. `auth login` makes one of
            // the three such calls in 2.1.289; 2.1.110 has none.
            "{assumeSupport:!0})}",
            // The other words, only for a terminal on stdin.
            "...process.stdin.isTTY&&{onCallbackListenerUnavailable:",
        ],
        absent: &[],
    },
    Assumption {
        name: "sign_in_takes_another_code",
        fact: "`claude auth login` reads each line typed back while it waits, trims it and \
               splits it at `#`. A line that is not `<code>#<state>` with both halves it \
               refuses by writing `Invalid code. Please make sure the full code was copied.` \
               and a newline to stderr, and it goes on reading in the same process, printing \
               no prompt again, so another line can be typed back. The first line with both \
               halves it takes, whatever its state half says. After that it still refuses a \
               line without both halves, and ignores one with them; a code the token exchange \
               then refuses ends the sign-in with `Login failed: ` on stderr and exit status 1",
        read_from: "the `auth login` command's handler for each line read from stdin, which \
                    trims and splits the line and refuses it before it looks at whether a code \
                    was taken, returns after the refusal and leaves the line reader open, and \
                    the OAuth service's `waitForAuthorizationCode` and \
                    `handleManualAuthCodeInput`, which takes a code only while its resolver is \
                    set, hands on the code half alone and clears the resolver",
        // Read on 2026-10-05 from the Windows arm64 build's sources and the macOS and Linux
        // builds of 2.1.289, all of which hold every literal below; 2.1.110 holds none.
        verified_against: "2.1.289",
        depends: "provider::claude::engine's refused_code, and the app model offering the \
                  code field again once Claude Code has refused a code",
        probe: &[
            // The refusal, then the handler returning rather than ending the process.
            "Invalid code. Please make sure the full code was copied.\n`);return}",
            "if(this.manualAuthCodeResolver)this.authorizationCodeReceived=!0",
        ],
        absent: &[],
    },
    Assumption {
        name: "project_settings_folder",
        fact: "Claude Code takes a project's settings from `.claude/settings.json` in the folder \
               it starts in, its working folder, resolved to its real path on macOS and Windows \
               and taken as it is on Linux, and local settings from \
               `.claude/settings.local.json` at that folder's git root, or in the folder itself \
               where the root is the home folder or another user's, and always on Windows. \
               `--project-config-root` moves both. The settings every command composes, \
               `claude auth login`'s among them, take both",
        read_from: "the settings sources' files, the project and local folders' resolvers with \
                    the git root's home and owner checks, and the working folder a run starts \
                    with, `process.cwd()` through `realpathSync` in the darwin-arm64, win32-x64 \
                    and win32-arm64 builds and as it is in the linux-x64 one, in those four \
                    builds of 2.1.310, read as bytes on 2026-10-24 and 2026-10-25",
        verified_against: "2.1.310",
        depends: "Claude's Provider::sign_in, which starts `claude auth login` in its private \
                  folder, as Codex's starts `codex login`, so a project's settings in the folder \
                  Pitboard runs from do not reach it, and local ones only from a git repository \
                  other than the home folder that Pitboard's own folder is in",
        probe: &[
            r#"case"projectSettings":return pe(n.projectConfigDir??"#,
            r#"return fe(".claude","settings.json");case"localSettings":return fe(".claude","settings.local.json")"#,
            "localSettings: not canonicalizing the consent store to ",
            r#"typeof process.cwd==="function"&&typeof t==="function")try{let r=e();try{n="#,
        ],
        absent: &[],
    },
    Assumption {
        name: "install_places",
        fact: "the native installer puts its launcher at `~/.local/bin/claude`, and says so \
               when that directory is not on `PATH`; a global npm install puts `claude` in \
               npm's global `bin`, which is in Homebrew's prefix, `/opt/homebrew` or \
               `/usr/local`, or in `/usr/local` where nodejs.org's installer put Node, and \
               Claude Code tells it is one by `/node_modules/@anthropic-ai/` in the path the \
               running program resolves to; one whose path runs through a Homebrew \
               `Caskroom` is the Homebrew cask's",
        read_from: "the installation-method detection, which returns `npm-global` for an \
                    `execPath` holding `/node_modules/@anthropic-ai/` and lists \
                    `/opt/homebrew/bin` and `/usr/local/bin` among npm's places, \
                    `getHomebrewCaskName`, and the native install's PATH advice",
        // Read on 2026-10-05 from the macOS builds of 2.1.283 and 2.1.289 and the Linux build
        // of 2.1.289, all of which hold every literal below.
        verified_against: "2.1.289",
        depends: "claude::paths::install_places, where an app looks for `claude` after the \
                  login shell's PATH",
        probe: &[
            "Native installation exists but ~/.local/bin is not in your PATH",
            ".local/bin/claude",
            "/node_modules/@anthropic-ai/",
            "Detected Homebrew cask installation: ",
        ],
        absent: &[],
    },
    Assumption {
        name: "plaintext_credential_mode",
        fact: "the plaintext credential is written and then chmod'd to 0600, in its storage \
               directory, under the fixed name `.credentials.json`",
        read_from: "the plaintext backend's write path, which chmods 384 after writing",
        verified_against: VERIFIED_AGAINST,
        depends: "store::file, slot::CRED_FILE, and atomic::Perms::Secret matching what \
                  Claude Code itself writes",
        probe: &[
            ".credentials.json",
            "Warning: Storing credentials in plaintext.",
        ],
        absent: &[],
    },
    Assumption {
        name: "locked_keychain_keeps_last_login",
        fact: "a keychain read that fails, as one of a locked keychain does, leaves a session \
               serving the login it last read, cached again for another 30 seconds; a session \
               that has read none reads the keychain as empty and the file behind it. So while \
               the keychain is locked a running session keeps its account and follows no \
               switch, and one started then signs in with `.credentials.json` or is signed out",
        read_from: "the keychain backend's `read`, which logs the probe and serves its cache \
                    when `security` fails, and its `readAsync`, which keeps the cached login \
                    when `find-generic-password` exits 36 and answers absent",
        // Read on 2026-10-08 from the macOS builds of 2.1.291 to 2.1.294, whose code here is
        // the same.
        verified_against: "2.1.294",
        depends: "doctor's credential check, which says what a session does while the \
                  keychain is locked, and what `pitboard stow` says of a session that signed in \
                  with `.credentials.json`: signed out once the file is gone",
        probe: &["[keychain] read failed; serving stale cache"],
        absent: &[],
    },
    Assumption {
        name: "locked_sign_in_writes_fallback",
        fact: "a sign-in in a session that cannot read the keychain, and has not read its item, \
               fails the keychain write with exit 36, which is not transient there, writes the \
               login to `.credentials.json`, and leaves the keychain item as it was, because \
               the keychain read as empty before the write. The keychain then holds one login \
               and the file another",
        read_from: "the fallback wrapper's `update`, which deletes the primary after writing \
                    the fallback only when the primary read non-empty first, and the keychain \
                    backend's `update`, which calls exit 36 transient only once this process \
                    has seen the item; and measured on 2026-10-08 with 2.1.294 under a herdr \
                    server started over SSH: `/login` made a `.credentials.json` of mode 0600 \
                    twice, and the app's two switches after the first went on reading and \
                    writing the keychain item",
        verified_against: "2.1.294",
        depends: "doctor's fallback_login check, the warning every read and every change give \
                  about it, and `pitboard stow`, which puts that login away",
        probe: &["plaintext_fallback_used"],
        absent: &[],
    },
    Assumption {
        name: "fallback_outlives_keychain_writes",
        fact: "a keychain write that lands deletes `.credentials.json` only where the keychain \
               held nothing before it. Where both hold a login, every later keychain write, a \
               token refresh, a sign-in from a desktop session and a switch alike, leaves the \
               file's in place",
        read_from: "the fallback wrapper's `update`: after a primary write that lands it \
                    deletes the fallback only when its read of the primary before the write was \
                    null",
        verified_against: "2.1.294",
        depends: "doctor's fallback_login check and the warning every read and every change \
                  give while it is there, both of which name `pitboard stow`, the one thing \
                  that deletes the file",
        // Behaviour, with no literal of its own.
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "fallback_file_pins_session_login",
        fact: "before a session makes a request it looks at `.credentials.json` in its storage \
               directory, by its modification time alone. Where the file is not there, or the \
               look fails, it drops the login it holds and reads the store in front of the file, \
               the keychain or Credential Manager, through its 30 second cache. Where it is \
               there, whatever it holds and whether or not it can be read, the session keeps \
               the login it holds while that login is usable: it reads again where the file's \
               modification time has changed, at most every 30 seconds where that login is gone \
               or its refresh token is empty or known dead, after a 401, and at its own sign-in, \
               and 5 minutes before the login expires it reads the store and takes the login \
               there where it differs. So while the file sits behind that store, a session \
               already running keeps the account it is on after a switch until its login is \
               next renewed, or until it is started again. Read, not measured against a running \
               session",
        read_from: "the check before each API client, which stats the file, compares its \
                    modification time with the one it last saw, reads through the store's \
                    cache where the stat fails, and otherwise keeps a usable login, reading \
                    again at most every 30 seconds for one that is not; the renewal check 5 \
                    minutes before expiry, which reads the store first and takes its login \
                    where it differs; and the 401 handlers",
        // Read on 2026-10-08 from the macOS build and on 2026-10-09 from the Linux x64,
        // Windows x64 and Windows arm64 builds of 2.1.294, whose code here is the same, and
        // again on 2026-10-16 from both Windows builds of 2.1.301. On Linux the file is the
        // only store, and on Windows it is unless Credential Manager is turned on
        // (`windows_backend_choice`).
        verified_against: "2.1.294",
        depends: "Claude's adoption, by which a switch says that running sessions take it at \
                  their login's next renewal while the file is behind the keychain or \
                  Credential Manager; Claude's behind, which tells the file by a look and says \
                  nothing where the look fails; the fallback_login warning and check, which say \
                  so of a file with no login in it, or one Pitboard cannot read, too; and \
                  `pitboard stow`, which says sessions already running follow a switch within 33 \
                  seconds again once the file is gone",
        probe: &["lastCredentialsMtimeMs", "lastUnusableTokenRecheckAt"],
        absent: &[],
    },
    Assumption {
        name: "login_is_one_organisation",
        fact: "a login is one account in one organisation. Anthropic's account uuid is the \
               person, and a sign-in is made for one of their organisations: the authorize \
               address takes `orgUUID`, and with `forceLoginOrgUUID` set a token whose \
               organisation `/api/oauth/validate` names otherwise is refused. Claude Code tells \
               logins apart by `accountUuid` and `organizationUuid` together, so a sign-in to \
               another organisation of the same person is an `account_switch`. Usage is asked \
               with the token alone, so it is that organisation's",
        read_from: "the sign-in's authorize address and its forceLoginOrgUUID check, the \
                    comparison a sign-in and `/login` make of the account before and after, \
                    and the usage request's headers",
        verified_against: "2.1.294",
        depends: "state::Account::owned_by and state::new_id: which enrolled account a login \
                  is, and what a new one is filed under",
        probe: &[
            "searchParams.append(\"orgUUID\"",
            "Your authentication token belongs to organization",
            "\"same_account\":\"account_switch\"",
            "Account changed via /login",
        ],
        absent: &[],
    },
    Assumption {
        name: "config_may_name_no_organisation",
        fact: "`oauthAccount` names the organisation of the login of the process that wrote \
               it, from the profile, whose shape requires `organization.uuid`. A sign-in that \
               could not read the profile writes it from the token's own account instead, \
               whose organisation can be absent, and Claude Code then reads none",
        read_from: "the sign-in's fallback to `tokenAccount` when the profile cannot be \
                    fetched, and the readers that stand `acct:` in for a missing organisation",
        verified_against: "2.1.294",
        depends: "Claude's own_record, which names such an account with no organisation, as \
                  the config wrote it, so in_use::known tells a change of it by the same id; \
                  and Claude's name, which takes the email the record holds for the account \
                  and organisation validate named, where the profile is refused",
        probe: &["tokenAccount", "acct:${"],
        absent: &[],
    },
    Assumption {
        name: "usage_cache_stamp_is_the_configs",
        fact: "`cachedUsageUtilization` is written after `GET /api/oauth/usage`, which a \
               session asks with the login it holds, and stamped with the config's \
               `accountUuid`, read before the request and again before the write. Nothing \
               compares the stamp with the login: a session still holding the login it had \
               before a switch writes that login's numbers under the account the config names \
               since. Claude Code's own readers compare the stamp with the config alone, and \
               clear the cache where the two differ. A sign-in clears it, as a logout does",
        read_from: "the usage read, which takes the config's `accountUuid` before it asks with \
                    the session's credentials, the cache's writer, which writes only where the \
                    config still names that account, the cache's two readers, and the reset \
                    a sign-in and a logout both run",
        // Read on 2026-10-08 from the macOS build and on 2026-10-09 from the Linux x64,
        // Windows x64 and Windows arm64 builds of 2.1.294, whose code here is the same, and
        // again on 2026-10-23 from the four builds of 2.1.309.
        verified_against: "2.1.309",
        depends: "status, which takes no usage from it, and the automatic switch, which \
                  decides from status's offline rows",
        probe: &[
            "cachedUsageUtilization:{fetchedAtMs:Date.now(),",
            "cachedUsageUtilization=void 0",
        ],
        absent: &[],
    },
    Assumption {
        name: "status_reads_the_config_usage_the_token",
        fact: "`/status` prints the config's `oauthAccount` email and organisation, and a \
               running session reads the config again within about a second of a change. \
               `/usage` shows the session's header snapshot, or without one the config's usage \
               cache where its stamp is the config's account and it is under an hour old; it \
               then answers from that cache where it is under 60 seconds old and newer than \
               the session's last header reading, and otherwise asks with the login the \
               session holds. So a session that has not taken a switch names the account \
               switched to in `/status`, while `/usage` and every request go on with the login \
               it holds",
        read_from: "`/status`'s account rows, which read the config's `oauthAccount`; the \
                    config's freshness watch and its 1000 ms poll; the `/usage` screen, which \
                    seeds itself from the session's header snapshot or the cache, and the \
                    usage read it starts",
        // Read on 2026-10-08 from the macOS build and on 2026-10-09 from the Linux x64,
        // Windows x64 and Windows arm64 builds of 2.1.294, whose code here is the same, and
        // again on 2026-10-23 from the four builds of 2.1.309.
        verified_against: "2.1.309",
        depends: "what a switch says of sessions already running while a file is behind the \
                  keychain, which names the file and no account: `/status` in one of them \
                  names the account switched to",
        probe: &["Usage read answered from a snapshot"],
        absent: &[],
    },
    Assumption {
        name: "config_identity_is_the_last_writers",
        fact: "`oauthAccount`'s account, email and organisation are written by a sign-in, from \
               the profile or else the token's own account, and at a process start where \
               `profileFetchedAt` is missing or over 24 hours old or a profile field is \
               missing, from the profile of that process's own token. Every start of a \
               process signed in to claude.ai also writes the email and organisation that \
               `/api/claude_cli/bootstrap` gives for that process's token, where it names the \
               config's account or none. A token refresh writes no identity field. So the \
               config can name another account than the login stored: a process started on \
               another login writes that login's account where the profile was missing, \
               incomplete or a day old, and one started on another organisation of the same \
               person writes that organisation",
        read_from: "the sign-in's profile and `tokenAccount` write, the start-up profile fetch \
                    and its 24 hour check, the bootstrap merge into `oauthAccount`, and the \
                    refresh's profile update",
        // Read on 2026-10-08 from the macOS build and on 2026-10-09 from the Linux x64,
        // Windows x64 and Windows arm64 builds of 2.1.294, whose code here is the same, and
        // again on 2026-10-23 from the four builds of 2.1.309.
        verified_against: "2.1.309",
        depends: "in_use::known, which takes the config naming another account than when \
                  Anthropic last named the login stored as a sign that something signed in, \
                  never as whose the login is; in_use::Known::unconfirmed, which doctor's \
                  in_use check and status's in_use_unconfirmed go by, and which keeps a record \
                  never checked against the store unconfirmed whatever the config names; and \
                  every reader of the account in use, which takes it from that record and not \
                  from the config",
        probe: &["profileFetchedAt:Date.now()", "organization_uuid!=null)"],
        absent: &[],
    },
    Assumption {
        name: "status_line_input_names_no_account",
        fact: "the status line's input holds `rate_limits.five_hour` and `seven_day`, each \
               `used_percentage` and `resets_at`, taken whole from one response's headers and \
               only for windows whose reset is ahead, and no account, organisation or email. \
               It is emptied only by that process's own account change, a sign-in or \
               sign-out, by the end of a remote attach after one, or by a response while the \
               process holds no claude.ai login. A response to a request sent before the \
               process's account changed is dropped, and so is a reading older than the last \
               one applied. So a session passes the numbers of the login it holds, whatever the \
               config names, and both windows it passes are of one response",
        read_from: "the status line's input and the hook fields common to every hook; the \
                    session's header snapshot, which keeps only windows whose reset is ahead; \
                    the limits' `applyWindowReadings`, which sets that snapshot whole from one \
                    response; `resetCurrentLimits` and both its callers; and the header \
                    reader, which drops a response from before an account change and empties \
                    the snapshot without a claude.ai login",
        // Read on 2026-10-08 from the macOS build and on 2026-10-09 from the Linux x64,
        // Windows x64 and Windows arm64 builds of 2.1.294, whose code here is the same.
        verified_against: "2.1.294",
        depends: "statusline::read, which takes a session's numbers to be the account's whose \
                  reading, as Anthropic answered it, holds their windows, never the account \
                  Claude Code's config names, and takes a limit's next window on the word of \
                  the other window passed with it",
        probe: &[
            ".spend_limit)&&{rate_limits:",
            "applyWindowReadings",
            "resetCurrentLimits",
        ],
        absent: &[],
    },
    Assumption {
        name: "login_carries_its_plan",
        fact: "the stored login keeps its plan in `claudeAiOauth`, beside the tokens: \
               `subscriptionType`, which a sign-in takes from the profile's \
               `organization.organization_type`, so `claude_max`, `claude_pro`, `claude_team` \
               and `claude_enterprise` are stored as `max`, `pro`, `team` and `enterprise`, and \
               `rateLimitTier`, the profile's `organization.rate_limit_tier` as Anthropic gives \
               it, such as `default_claude_max_20x` or `default_claude_max_5x`. Each is null \
               where neither the profile nor the login stored before says one. From 2.1.309 \
               `subscriptionTypeSource`, `profile` or `sign_in`, and \
               `subscriptionTypeVerifiedAt` are kept beside them, and a save takes the \
               profile's plan over the stored one only where the profile's answer is stamped \
               as Anthropic's own and the stored plan came from a profile, is old, or ranks \
               differently. A token refresh asks for the profile again only where the login \
               lacks either or the config's `oauthAccount` lacks one of its profile fields. So \
               a plan changed since the login's last sign-in can go unseen in it",
        read_from: "the profile read, which maps `organization_type` through a table and takes \
                    `rate_limit_tier` whole; the function every save of the login into \
                    `claudeAiOauth` goes through, which writes both beside the tokens, and from \
                    2.1.309 the one that chooses the stored or the profile's plan; the \
                    sign-in's save; and the refresh's check of whether to ask for the profile. \
                    Measured on 2026-10-10 in the stored logins of five accounts on one Mac: \
                    `max` with `default_claude_max_20x`, and `team` with `default_claude_max_5x`",
        // Read on 2026-10-11 from macOS 2.1.294 to 2.1.296, and Linux x64 and Windows 2.1.296,
        // and on 2026-10-23 from the four builds of 2.1.309, where the save's plan moved.
        verified_against: "2.1.309",
        depends: "provider::claude::document::plan, which reads the plan off a login or a park \
                  of it for `pitboard status`, the app and the account's record, as \
                  `subscriptionType` capitalised and the multiple `rateLimitTier` ends with",
        probe: &[
            r#"[["claude_max","max"],["claude_pro","pro"],["claude_enterprise","enterprise"],["claude_team","team"]]"#,
            "organization?.rate_limit_tier??null",
            r#"subscriptionTypeSource:"profile",subscriptionTypeVerifiedAt:Date.now()"#,
        ],
        absent: &[],
    },
    Assumption {
        name: "windows_home_folder",
        fact: "on Windows the home `~/.claude` and `~/.claude.json` are in is `os.homedir()`: \
               `USERPROFILE` where it is set, and `GetUserProfileDirectoryW` where it is not. \
               A `USERPROFILE` shorter than three bytes makes it throw. `HOME`, which Git Bash \
               sets, is not read for it",
        read_from: "the config dir, `(CLAUDE_CONFIG_DIR ?? join(homedir(), \".claude\"))` in NFC \
                    with `homedir` from `os`, in the win32-x64 and win32-arm64 builds; Bun \
                    1.4.3's `os.homedir` in src/runtime/node/node_os.rs, as embedded in Claude \
                    Code 2.1.300, which calls libuv's `uv_os_homedir`, in src/win/util.c at \
                    oven-sh/libuv 8023581; read on 2026-10-15",
        verified_against: "2.1.300",
        depends: "context::Environment::home and host::windows::user::home, which every \
                  default home on Windows is made from",
        probe: &[
            "import{homedir as",
            "USERPROFILE",
            "GetUserProfileDirectoryW",
        ],
        absent: &[],
    },
    Assumption {
        name: "windows_open_sharing",
        fact: "on Windows Claude Code opens its files sharing their reading, writing and \
               deletion, so a POSIX rename lands over one it holds open. It writes \
               `.credentials.json` by staging `<file>.tmp.<8 hex>` beside it and renaming that \
               over, trying the rename four times 50ms apart on EPERM, EBUSY and EACCES, and \
               then rewriting the file in place on EXDEV, EPERM, EEXIST or EBUSY. What its \
               strict read makes of a file held without sharing is \
               windows_plaintext_strict_read's",
        read_from: "Bun 1.4.3's `openat_windows`, which calls `NtCreateFile` with \
                    `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE`, in src/sys/lib.rs \
                    at bun-v1.4.3, its errno table, which makes a sharing violation EBUSY, in \
                    src/errno/windows_errno.rs, and libuv's `uv_fs_open`, which shares all three but under \
                    `UV_FS_O_EXLOCK`, in src/win/fs.c at oven-sh/libuv 8023581, as embedded in \
                    Claude Code 2.1.300; the file write's staging, its retries and its \
                    in-place arm, in the win32-x64 and win32-arm64 builds; read on 2026-10-15. \
                    Measured with Process Monitor and 2.1.302 on the session's PC, Windows 11 \
                    build 26300 on ARM64, on 2026-10-18: `claude.exe` opened `.credentials.json` \
                    and `.claude.json` sharing all three, renamed `.claude.json.tmp.<pid>.<hex>` \
                    over `.claude.json`, and no open of a file in `.claude` met a sharing \
                    violation or access denied while the app and the command line switched",
        verified_against: "2.1.300",
        depends: "host::windows::fs::rename, which renames over Claude Code's files the POSIX \
                  way where the volume has it and waits out a file held without sharing its \
                  deletion, and Pitboard's reads of Claude Code's files, which share all three \
                  as std opens a file and are never held across Claude Code's lock: a read \
                  not sharing deletion would turn Claude Code's write into an in-place one",
        probe: &[
            r#"["EXDEV","EPERM","EEXIST","EBUSY"]"#,
            r#"["EPERM","EBUSY","EACCES"]"#,
            "refusing the in-place arm on a non-regular target",
        ],
        absent: &[],
    },
    Assumption {
        name: "windows_install_layout",
        fact: "on Windows the native installer keeps its launcher as a copy, `claude.exe` in \
               `(HOME ?? homedir())\\.local\\bin`, put in place by a rename once its checksum \
               matches, and each version in `claude\\versions` of `XDG_DATA_HOME`, else of \
               `<home>\\.local\\share`. A global npm install's program is `bin/claude.exe`, which npm's \
               cmd-shim starts as it is from `claude.cmd` in its prefix, `%APPDATA%\\npm` by \
               default, and which postinstall links or copies from \
               `@anthropic-ai/claude-code-win32-<arch>/claude.exe`; without that it is a stub \
               that prints how to install. winget's `Anthropic.ClaudeCode` is the bare \
               portable `claude.exe`, linked in winget's `Links`",
        read_from: "the native installer's paths, its launcher's update and its `claude.exe` on \
                    win32, in the win32-x64 and win32-arm64 builds; `install.cjs` and \
                    `package.json` of `@anthropic-ai/claude-code@2.1.300`; the installer \
                    manifest of `Anthropic.ClaudeCode` 2.1.292, the newest in \
                    microsoft/winget-pkgs; read on 2026-10-15",
        verified_against: "2.1.300",
        depends: "claude::paths::install_places and host::Os::package_bins, where an app looks \
                  for `claude.exe` after the registry's `Path`",
        probe: &["Refusing to install a launcher copy of "],
        absent: &[],
    },
    Assumption {
        name: "windows_version_resource",
        fact: "on Windows `claude.exe` carries a version resource whose product version, fixed \
               and as text, is the release with a fourth part of 0, `2.1.300.0` for 2.1.300, \
               with the product name `Claude Code`",
        read_from: "the version resource of the win32-x64 and win32-arm64 builds of 2.1.300, \
                    read as bytes on 2026-10-15",
        verified_against: "2.1.300",
        depends: "claude::paths::installed_version, which reads the release there on Windows",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "windows_managed_settings",
        fact: "on Windows Claude Code takes managed settings from `C:\\Program Files\\ClaudeCode`, \
               its `managed-settings.json` and the `.json` files of `managed-settings.d`, and \
               from the value `Settings` of `HKLM\\SOFTWARE\\Policies\\ClaudeCode`, a `REG_SZ` or \
               a `REG_EXPAND_SZ` as `reg.exe query` prints it, its end trimmed, which it takes \
               over the folder unless `managedSourcesBehavior` merges them. The same value \
               under `HKCU` it reads only where neither holds a policy, and there it ignores \
               what only an administrator may set. A source that holds nothing but \
               `managedSourcesBehavior` or `wslInheritsWindowsSettings` is not a policy and \
               keeps `HKCU` out of nothing (its `pt`). A failed `HKLM` `reg.exe query` it treats \
               as absent (its `vYn` nulls the machine output on a non-zero exit) and still reads \
               `HKCU`, while an unreadable managed file keeps `HKCU` out. From 2.1.301 a \
               diskless launch reads no `HKCU` value",
        read_from: "the managed folder, the policy keys, their reading through \
                    `%SystemRoot%\\System32\\reg.exe query <key> /v Settings` and the managed \
                    sources' order in the win32-x64 and win32-arm64 builds of 2.1.300, read as \
                    bytes on 2026-10-15; the `HKCU` read's diskless check, in the win32-x64 \
                    builds of 2.1.301 to 2.1.303, read as bytes on 2026-10-20",
        verified_against: "2.1.300",
        depends: "settings::overrides, custom_oauth and overrides_of_a_run_in, through \
                  host::Host::policy and administered_folder, where holds_policy and managed \
                  keep HKCU out",
        probe: &[
            "C:\\Program Files\\ClaudeCode",
            "SOFTWARE\\Policies\\ClaudeCode",
            "managedSourcesBehavior",
            "wslInheritsWindowsSettings",
        ],
        absent: &[],
    },
    Assumption {
        name: "windows_daemon_identity",
        fact: "on Windows the supervisor daemon records in `daemon.lock`, beside its pid, \
               `procStartFt`: its own creation time from `GetProcessTimes`, called through \
               `bun:ffi` with `PROCESS_QUERY_LIMITED_INFORMATION`, the FILETIME as a decimal \
               string. It takes a daemon for running only while its pid's creation time, read \
               the same way, is the same text, so a pid Windows has given another process \
               reads as gone. Where `bun:ffi` does not load it writes `procStart`, \
               PowerShell's `CreationDate.Ticks`, which it never compares with a FILETIME. \
               `daemon.lock` holds no `pidDomain`, which only the session records carry, as \
               `win32:<host name>`. Its pipes are `\\\\.\\pipe\\cc-daemon-<the 16 hex of \
               daemon\\pipe.key>-<name>`",
        read_from: "the daemon lock's writer and its liveness check, the process start module \
                    and the daemon's pipe names, in the win32-x64 and win32-arm64 builds of \
                    2.1.300, read as bytes on 2026-10-16; 2.1.301's win32-x64 build is the same",
        verified_against: "2.1.300",
        depends: "claude::daemon's running, through host::proc::started",
        probe: &["procStartFt", "\\\\.\\pipe\\cc-daemon-"],
        absent: &[],
    },
    Assumption {
        name: "windows_backend_choice",
        fact: "on Windows Claude Code keeps its login in Credential Manager, with \
               `.credentials.json` behind it, where `CLAUDE_CODE_FORCE_WINDOWS_CREDMAN` is \
               exactly `1` in its environment, which it asks on every store access, or where its \
               config file caches `tengu_windows_credman` as true, which it reads once a process: \
               a non-empty `<config dir>\\.config.json`, else `<CLAUDE_CONFIG_DIR || home>\\.claude.json` \
               where `.config.json` is missing, empty, a folder or a link loop, any other error \
               or a file that is not JSON reading as false. Otherwise the file alone is its \
               store. The variable reaches a session from the environment it starts with, which \
               Windows makes for one started from Explorer from `HKCU\\Environment` over the \
               machine's `Session Manager\\Environment`, and from \
               the `env` of the config file, of the person's `settings.json` and of managed \
               settings, assigned in that order at start-up, each value coerced to a string. A \
               project's or local settings' `env` passes only a safe list then, which leaves the \
               variable out, but is assigned whole once the folder is trusted, or in print \
               mode, so a project can turn the store on for its own sessions from then on. From \
               2.1.302 a session started with `--session-tunnel` also takes the `env` of the \
               line of JSON it reads at start-up, ahead of the config file's",
        read_from: "`getSecureStorage`, the backend gate beside it with its memo and its read \
                    of the cached feature, the backend list, and \
                    applySafeConfigEnvironmentVariables, applyConfigEnvironmentVariables, the \
                    project-scope filter and the safe list, in the win32-x64 and win32-arm64 \
                    builds of 2.1.301, read as bytes on 2026-10-16; the session tunnel's line and \
                    `takenFromToolServerLaunchEnv`, in both Windows builds of 2.1.303, read as \
                    bytes on 2026-10-20",
        verified_against: "2.1.301",
        depends: "claude::credman::turned_on, which reads every signal but a project's, a \
                  `--settings` file's and a session tunnel's, managed settings only from the \
                  sources a session takes (settings::env_settings), and live::chain on Windows, \
                  which is the file only where none is on and Credential Manager holds nothing \
                  at the live target, so a session a project turned the store on for is caught \
                  once it wrote there",
        probe: &[
            "tengu_windows_credman",
            "CLAUDE_CODE_FORCE_WINDOWS_CREDMAN",
            r#"["keychain","plaintext","windows-credman"]"#,
            "applySafeConfigEnvironmentVariables",
        ],
        absent: &[],
    },
    Assumption {
        name: "windows_plaintext_strict_read",
        fact: "on Windows Claude Code reads `.credentials.json` strictly before it writes: a \
               missing file, a folder in its place or a path through a file is no login, a \
               file that is not JSON is no login either, and any other error is a failed read, \
               EACCES, EPERM and a sharing violation's EBUSY included, which skips its write \
               (`read_failed_skip_write`). So a read that held the file without sharing it \
               would have Claude Code drop a renewal",
        read_from: "the plaintext store's strict read and its error mapping, called with \
                    `win32`, and the store mutation that skips its write on a failed read, in \
                    the win32-x64 and win32-arm64 builds of 2.1.301, read as bytes on 2026-10-16",
        verified_against: "2.1.301",
        depends: "Pitboard's reads of Claude Code's files on Windows, which share reading, \
                  writing and deletion as std opens a file and are never held across Claude \
                  Code's lock (windows_open_sharing)",
        probe: &["read_failed_skip_write"],
        absent: &[],
    },
    Assumption {
        name: "windows_plaintext_write",
        fact: "on Windows Claude Code writes `.credentials.json` in its storage folder, making \
               the folder first, by a temporary file renamed over it (windows_open_sharing), \
               then `chmod` 0600, which there only clears the read-only attribute. It sets no \
               access list, so a file it makes takes its folder's inherited one. Its rename is \
               `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`, which fails over a read-only \
               file, and its in-place arm cannot open one to write, so its write fails there. \
               The file's modification time changes with every write",
        read_from: "the plaintext store's write, which calls the file write helper with 384 \
                    and then chmod 384, in the win32-x64 and win32-arm64 builds of 2.1.301; Bun \
                    1.4.3's `chmod` and `rename` in src/sys/sys_uv.rs and \
                    src/runtime/node/node_fs.rs, which call libuv's `uv_fs_chmod`, \
                    `_wchmod`, and `uv_fs_rename`, `MoveFileExW`, in src/win/fs.c at \
                    oven-sh/libuv 8023581, as embedded in Claude Code 2.1.301; read on \
                    2026-10-16",
        verified_against: "2.1.301",
        depends: "store::file on Windows, which writes the file by a temporary file renamed \
                  over it, keeps the access list it had (host::windows::fs::copy_access) and \
                  refuses a read-only one, as Claude Code's own write fails there",
        probe: &[
            ".credentials.json",
            "Warning: Storing credentials in plaintext.",
        ],
        absent: &[],
    },
    Assumption {
        name: "windows_file_adoption",
        fact: "on Windows, where `.credentials.json` is Claude Code's store, a running session \
               looks at its modification time before each request and reads it again where it \
               has changed, so a switch, which changes it, is taken at the session's next \
               request",
        read_from: "the check before each API client, which compares the file's modification \
                    time with the one it last saw, in the win32-x64 and win32-arm64 builds of \
                    2.1.301, read as bytes on 2026-10-16. Measured with 2.1.302 on the session's \
                    PC, Windows 11 build 26300 on ARM64, on 2026-10-18: a running `claude` \
                    answered its next prompt as the account switched to, 11 s after a switch from \
                    the app's menu, and 21 s after one from a terminal, its answer at 11 s still \
                    under way",
        verified_against: "2.1.302",
        depends: "Claude's adoption on Windows, which says sessions follow within 33 seconds, \
                  as on Linux",
        probe: &["lastCredentialsMtimeMs"],
        absent: &[],
    },
    Assumption {
        name: "credman_account_name",
        fact: "on Windows Claude Code names its Credential Manager item's user \
               `claude-code-user` whoever runs it, so its item is \
               `Claude Code-credentials[-<8 hex>]/claude-code-user` as `Bun.secrets` names one, \
               and the pieces of one too large for an item `#<k>`, `#m` and `#p` after it",
        read_from: "the secure storage module's account name, a literal in the win32-x64 and \
                    win32-arm64 builds of 2.1.301 where the others read `USER`, and the store \
                    that passes it to `Bun.secrets` with the service name; Bun 1.4.3's \
                    `Bun.secrets` in src/jsc/bindings/SecretsWindows.cpp, which names a \
                    generic item `<service>/<name>`, as embedded in Claude Code 2.1.301; read on \
                    2026-10-16",
        verified_against: "2.1.301",
        depends: "slot::account_name on Windows, and claude::credman's look at the live target \
                  and its manifest",
        probe: &["claude-code-user", "Bun.secrets.get"],
        absent: &[],
    },
    Assumption {
        name: "credman_target",
        fact: "on Windows Claude Code keeps its login in the Credential Manager item \
               `<service>/claude-code-user`, the service named as on macOS: \
               `Claude Code${OAUTH_FILE_SUFFIX}-credentials`, with \
               `-<first 8 hex of sha256>` of `CLAUDE_SECURESTORAGE_CONFIG_DIR`, else of the NFC \
               config dir, appended unless the one asked for is unset. The string is hashed as \
               given, so `C:\\x`, `C:/x`, `c:\\x` and `C:\\x\\` name four items for one folder, and \
               any `CLAUDE_CONFIG_DIR` set names one of its own",
        read_from: "the secure storage module's service name and slot derivation, and the \
                    Credential Manager store that hands it to `Bun.secrets` with the user name, \
                    in the win32-x64 and win32-arm64 builds of 2.1.301, read as bytes on \
                    2026-10-16",
        verified_against: "2.1.301",
        depends: "slot::service_for_dir and slot::dir_hash, which hash the string as given, \
                  claude::paths::live_service, and claude::credman::Store",
        probe: &[
            "-credentials",
            "OAUTH_FILE_SUFFIX",
            "CLAUDE_SECURESTORAGE_CONFIG_DIR",
        ],
        absent: &[],
    },
    Assumption {
        name: "credman_chunking",
        fact: "a login of up to 2400 bytes is the item's whole value. Above that it is base64 in \
               pieces of 2400 characters, `#0` to `#n-1` after the user name, at most 256, with \
               `#m` holding `{\"n\":n,\"l\":length}` and `#p` the number of pieces a write may \
               leave. A write of pieces sets `#p`, each piece, then `#m`, then deletes the item \
               and any piece past the last, then `#p`; a write of one item over pieces sets the \
               item, `#p`, deletes `#m`, each piece, then `#p`. A read takes `#m` only where its \
               `n` and `l` are integers with 0 < n <= 256 and 0 < l <= n * 2400, else reads the \
               item, never reads `#p`, and fails where a piece is missing or the pieces' length \
               is not `l`. More than 256 pieces, 460800 bytes, fails the write \
               (`credman_oversize`). A delete takes the item, the pieces `#m` names and `#m`, \
               then as many as `#p` says, or where it says none each piece found from `#0` on. \
               An item holding nothing is none. Pieces replaced in place under a manifest of \
               the same length can be read half old and half new",
        read_from: "the Credential Manager store's write, read, delete and manifest check, in the \
                    win32-x64 and win32-arm64 builds of 2.1.301, read as bytes on 2026-10-16",
        verified_against: "2.1.301",
        depends: "claude::pieces, which writes in the same order, adds a manifest no base64 \
                  length matches while pieces are replaced in place, and reads a manifest Claude \
                  Code would pass over, or `#p` beside one, as unreadable; claude::credman::Store, \
                  and the ceiling a switch and a renewal check before they write",
        probe: &[
            "SecureStorageData too large for Credential Manager (",
            "chunked read inconsistent: got ",
            "credman_oversize",
        ],
        absent: &[],
    },
    Assumption {
        name: "credman_probe",
        fact: "a session asks `Bun.secrets` for its item once as it starts, for at most 10 \
               seconds. Where that fails or takes longer, Credential Manager is out of its reach \
               for the session's life: it reads only `.credentials.json` and every write to the \
               store fails, which its fallback wrapper answers by writing the file. Each later \
               call has a 2 second timeout",
        read_from: "the startup probe, its 10 second timeout and the flag every Credential \
                    Manager read and write asks, in the win32-x64 and win32-arm64 builds of \
                    2.1.301, read as bytes on 2026-10-16",
        verified_against: "2.1.301",
        depends: "credman_unreachable_writes_file, by which such a session leaves a login in \
                  the file behind Credential Manager",
        probe: &["probe timeout", "[windows-credman] load failed: "],
        absent: &[],
    },
    Assumption {
        name: "credman_migration",
        fact: "with Credential Manager on, every write goes there first. Where it took the login \
               and held none before, `.credentials.json` is deleted, a failure to delete \
               ignored; where it held one, the file is left. Where Credential Manager fails, \
               the login is written to the file (`plaintext_fallback_used`), and where it held \
               a login it cannot then delete, the write fails though the file took it \
               (`plaintext_fallback_primary_not_cleared`); both failing is \
               `primary_and_fallback_failed`. A strict read takes an entry that is not JSON as a \
               failed read. The successor backend behind `tengu_hover_rest` is not used where no \
               backend is handed in, as an ordinary `claude` hands none",
        read_from: "the fallback wrapper's update, delete and strict read around the Credential \
                    Manager and plaintext stores, and `getSecureStorage`, in the win32-x64 and \
                    win32-arm64 builds of 2.1.301, read as bytes on 2026-10-16",
        verified_against: "2.1.301",
        depends: "store::Live::taken_over_by_the_first, which writes Credential Manager and \
                  deletes the file only where it held nothing, and never writes the file where \
                  Credential Manager fails: the journal's recovery takes that",
        probe: &[
            "plaintext_fallback_used",
            "primary_and_fallback_failed",
            "plaintext_fallback_primary_not_cleared",
        ],
        absent: &[],
    },
    Assumption {
        name: "credman_move_lost_a_piece",
        fact: "Claude Code's move of a login into Credential Manager can leave pieces no read \
               can join, and the login with them. Measured with 2.1.302 on 2026-10-18 on the \
               session's PC, Windows 11 build 26300 on ARM64: `CLAUDE_CODE_FORCE_WINDOWS_CREDMAN` \
               turned on, a session moved a 524-byte `.credentials.json` login there as `#m`, 16 \
               bytes naming 3 pieces, beside `#0` and `#1` of 2400 bytes each, with no `#2`, no \
               `#p` and no item of its own, and deleted the file; a new session said \
               \"Not logged in\". With those items there, `claude auth login` said it was \
               logged in and wrote nothing. `claude auth logout` deleted every item, and a \
               sign-in after it wrote one item of 524 bytes and no file. Read in the code: what \
               it moves is its whole storage object, written whole up to 2400 bytes and above \
               that as base64 pieces; no single write names a piece it lacks, while the 2-second \
               timeout on each call and its lock's stale and re-entered holds can let two \
               overlap; its login skips its write after a strict read that failed, and its \
               logout deletes the pieces a manifest names and every one found from `#0` on",
        read_from: "the Credential Manager store's write, read and delete, the fallback wrapper's \
                    update, `claude auth login`'s save and `claude auth logout`, in the \
                    win32-x64 build of 2.1.302, read as bytes on 2026-10-18, and the session's \
                    `credman-names` reports of steps 7 and 8",
        verified_against: "2.1.302",
        depends: "claude::pieces::read, which takes pieces no read can join for a broken login \
                  (store::Error::Broken), and doctor's credential check and every refusal over \
                  it, which say Claude Code cannot read it either and that `claude auth logout`, \
                  then a sign-in, clears it",
        probe: &[
            "logout: secure storage could not be read",
            "Couldn't save your login. Try logging in again.",
        ],
        absent: &[],
    },
    Assumption {
        name: "credman_unreachable_writes_file",
        fact: "with Credential Manager on, a session whose start-up look at `Bun.secrets` \
               failed, as every call fails in a network logon, reads no login there and fails \
               every write there, so its fallback wrapper writes the login, a sign-in's or a \
               renewal's, to `.credentials.json` (`plaintext_fallback_used`) and leaves the \
               Credential Manager item as it was: it deletes that only where its read before \
               the write held a login. Credential Manager then holds one login and the file \
               another, and a session that reads only the file signs in with the file's",
        read_from: "the Credential Manager store's read and update, which answer no login and a \
                    failed write while the start-up probe has not passed, the probe, and the \
                    fallback wrapper's update and read, in the win32-x64 and win32-arm64 builds \
                    of 2.1.301, read as bytes on 2026-10-16",
        verified_against: "2.1.301",
        depends: "Claude's behind on Windows, doctor's fallback_login check, the warning every \
                  read and every change give while the file is there, and `pitboard stow`, \
                  which puts that login away",
        // Behaviour, with no literal of its own.
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "credman_attributes",
        fact: "`Bun.secrets` writes each item generic, named `<service>/<name>`, the name its \
               user name, the value's UTF-8 bytes its blob, with enterprise persistence, no \
               comment and no attributes: every logon of the person on that PC reads it, and a \
               roaming profile takes it to others. It reads an item holding nothing as one that \
               is there",
        read_from: "Bun 1.4.3's `Bun.secrets` in src/jsc/bindings/SecretsWindows.cpp, as embedded \
                    in Claude Code 2.1.301, read on 2026-10-16, and RF-G3, which wrote one as \
                    Claude Code 2.1.300 calls it (run 38704825885)",
        verified_against: "2.1.301",
        depends: "host::windows::credman, which writes each item the same way, \
                  `credman_write_is_harmless`, and doctor's credential store line",
        probe: &["Bun.secrets.set", "Bun.secrets.delete"],
        absent: &[],
    },
    Assumption {
        name: "windows_credman_cache",
        fact: "a running session serves the login it read from Credential Manager for 30 \
               seconds before it reads it again, and keeps the last one it read where a read \
               fails. Read, not measured against a running session",
        read_from: "the Credential Manager store's cached read, in the win32-x64 and \
                    win32-arm64 builds of 2.1.301, read as bytes on 2026-10-16",
        verified_against: "2.1.301",
        depends: "Claude's adoption on Windows, 33 seconds as on macOS until session step 7 \
                  measures it",
        probe: &["[windows-credman] strict load failed: ", "readInFlight"],
        absent: &[],
    },
    Assumption {
        name: "windows_browser_open",
        fact: "on Windows `claude auth login` opens its sign-in page once it has printed its \
               prompt by starting `rundll32 url,OpenURL <address>`, a child of its own, which \
               hands the address to the shell; where its settings or `BROWSER` name a program, \
               it starts that with the address instead. A browser that was not running starts \
               from inside whatever job Claude Code runs in",
        read_from: "the opener `startOAuthFlow` calls after its prompt, which checks the address \
                    is `http` or `https` and starts `rundll32` on win32, in the win32-x64 and \
                    win32-arm64 builds of 2.1.300, read as bytes on 2026-10-16",
        verified_against: "2.1.300",
        depends: "host::windows::job, which ends only the tool's own programs on a cancel, so a \
                  browser its sign-in opened keeps running (RF-F2)",
        probe: &["[\"url,OpenURL\","],
        absent: &[],
    },
    Assumption {
        name: "windows_token_handover",
        fact: "on Windows Claude Code reads no token handed to it on a descriptor. Its reader of \
               `CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR` reads the descriptor directly only where \
               `fstat` calls it a socket; otherwise it opens `/proc/self/fd/<n>`, and where that \
               fails it reads the well-known file `/home/claude/.claude/remote/.oauth_token` \
               before it reads directly, which it does only on EACCES or EPERM. It reads that \
               file, a regular file alone, where the variable is unset too, unless it runs as \
               `mcp serve --transport http`, and a token from it comes before `apiKeyHelper` and \
               the stored login, as `CCR_OAUTH_TOKEN_FILE`. That file is no route on Windows: a \
               path with no drive names a folder at a drive's root, which every account on the \
               PC shares, so a token there would sign in every Claude Code run on that drive: \
               Bun 1.4.3 takes the drive of the working folder (RF-T7, runs 39310064516 and \
               39320680285), as remote_token_files says. Bun 1.4.3 calls a pipe or a \
               socket handed as descriptor 3 through `STARTUPINFOW.lpReserved2` a FIFO, and \
               opening `/proc/self/fd/3` fails with ENOENT: RF-WW1, runs 38797729796 and \
               38821312770",
        read_from: "the descriptor reader, its socket check and its well-known file read, and \
                    the order of the token sources it names, in the win32-x64 and win32-arm64 \
                    builds of 2.1.301, which embed Bun 1.4.3, read as bytes on 2026-10-16. The \
                    path is fixed when a build is made: the macOS build of 2.1.296 opens \
                    `/dev/fd/<n>`, which takes a pipe",
        verified_against: "2.1.301",
        depends: "Claude Code's warm-up command, which on Windows hands a run the token as \
                  windows_token_env says instead",
        probe: &[
            "descriptor_by_path",
            "file_after_socket",
            "file_after_failed_read",
            "/home/claude/.claude/remote",
        ],
        absent: &[],
    },
    Assumption {
        name: "windows_token_env",
        fact: "on Windows Claude Code signs in with the access token `CLAUDE_CODE_OAUTH_TOKEN` \
               holds wherever it is set: right after `ANTHROPIC_AUTH_TOKEN`, beside a token the Claude desktop app hands a \
               session it starts, and before a descriptor or its well-known file, `apiKeyHelper`, a profile and the stored login. \
               It takes it as an access token alone, with no refresh token and no expiry, so \
               nothing renews it, scoped `user:inference` unless `CLAUDE_CODE_OAUTH_SCOPES` names \
               others. Print mode sends its requests with `ANTHROPIC_API_KEY` instead wherever \
               that is set. `CLAUDE_CODE_SUBPROCESS_ENV_SCRUB` set to 1 takes \
               `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN` and the \
               other credentials it names out of the environment Claude Code builds for a \
               program it starts. A program started with its own environment whole keeps them: \
               `where.exe`, which windows_statusline_shell's Git Bash finder asks for `git` at \
               every start where neither `CLAUDE_CODE_GIT_BASH_PATH` nor a Program Files path \
               names a bash, `git worktree list`, and a `proxyAuthHelper`. \
               code.claude.com's page of environment variables documents both variables. Read, \
               not measured: session reading M9 measures a warm-up made so, and that it writes \
               nothing to its private home or that home's Credential Manager slot",
        read_from: "the two token source checks, each of which takes the variable right after \
                    `ANTHROPIC_AUTH_TOKEN`, the login getter, which takes it first, the API \
                    key's source in print mode, the variables the scrub takes out of a child's \
                    environment and what turns it on, and the `where.exe` lookup the Git Bash \
                    finder falls back to, started with `env:process.env`, in the win32-x64 and \
                    win32-arm64 builds of 2.1.303, read as bytes on 2026-10-19 and 2026-10-20, \
                    and again in those of 2.1.309 on 2026-10-23 and of 2.1.310 on 2026-10-24",
        verified_against: "2.1.310",
        depends: "Claude Code's warm-up command on Windows, which hands a run the access token \
                  alone in `CLAUDE_CODE_OAUTH_TOKEN`, with the scrub on and no other variable \
                  that signs it in, and names in `CLAUDE_CODE_GIT_BASH_PATH` the Git Bash \
                  Pitboard finds, so no `where.exe` is asked for `git` where it finds one",
        probe: &[
            r#"return{source:"ANTHROPIC_AUTH_TOKEN",hasToken:!0};if(i.CLAUDE_CODE_OAUTH_TOKEN||gM()!==null)return{source:"CLAUDE_CODE_OAUTH_TOKEN",hasToken:!0}"#,
            "(){if(i.CLAUDE_CODE_OAUTH_TOKEN)return bp(i.CLAUDE_CODE_OAUTH_TOKEN);",
            "{accessToken:e,refreshToken:null,expiresAt:null,scopes:eA(),",
            r#""ANTHROPIC_API_KEY","CLAUDE_CODE_OAUTH_TOKEN","CLAUDE_CODE_ARTIFACTS_API_TOKEN""#,
            "process.env.CLAUDE_CODE_SUBPROCESS_ENV_SCRUB",
            "windowsHide:!0,env:process.env}).trim().split(",
        ],
        absent: &[],
    },
    Assumption {
        name: "windows_statusline_shell",
        fact: "on Windows Claude Code runs a `statusLine` command as it runs a hook that names no \
               shell: with `bash -c` in Git Bash, the bash's folder put first on the command's \
               `PATH`, where it finds one, and otherwise in PowerShell, `-NoProfile \
               -NonInteractive -ExecutionPolicy Bypass -Command`, saying once a session that a \
               hook ran in PowerShell. Each starts hidden. The command is `statusLine.command` of \
               its settings. It finds Git Bash at `CLAUDE_CODE_GIT_BASH_PATH` where that names a \
               file there whose name, in lower case, is `bash.exe`, `sh.exe`, `bash` or `sh`, then at \
               `C:\\Program Files\\Git\\bin\\bash.exe`, then at \
               `C:\\Program Files (x86)\\Git\\bin\\bash.exe`, then at `..\\..\\bin\\bash.exe` from \
               the first `git` that `where.exe` finds outside the working folder. It hands bash \
               the command as written, backslashes and all, so the bare `pitboard statusline` \
               runs the same in either shell",
        read_from: "the Git Bash finder, the hook executor the status line's run goes through, \
                    its default shell, the notice it posts falling back to PowerShell, and the \
                    settings' `statusLine` and the runner that reads its `command`, in the \
                    win32-x64 and win32-arm64 builds of 2.1.301, read as bytes on 2026-10-16",
        verified_against: "2.1.301",
        depends: "doctor's status_line check, which says on Windows which shell runs the status \
                  line and whether a `pitboard` is on the PATH it is handed, and warns where the \
                  settings Pitboard reads run a bare `pitboard` and none is there",
        probe: &[
            r#"["bash.exe","sh.exe","bash","sh"]"#,
            ".CLAUDE_CODE_GIT_BASH_PATH).toLowerCase()",
            r#""C:\\Program Files\\Git\\bin\\bash.exe","C:\\Program Files (x86)\\Git\\bin\\bash.exe""#,
            r#""..","..","bin","bash.exe""#,
            "ran in PowerShell. Commands written for bash can fail there.",
            ".statusLine?.command",
        ],
        absent: &[],
    },
    Assumption {
        name: "windows_statusline_powershell",
        fact: "where it finds no Git Bash, Claude Code runs a status line in `pwsh` on `PATH`, \
               then `pwsh.exe` in `%ProgramFiles%\\PowerShell\\7`, \
               `%LOCALAPPDATA%\\Microsoft\\WindowsApps`, there only as an app execution alias \
               that leads to a file, or `%USERPROFILE%\\.dotnet\\tools`, then `powershell` on \
               `PATH`, then `%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe`, \
               `SystemRoot` taken as `C:\\Windows` where unset. A program it finds named `pwsh` \
               is PowerShell 7, and any other Windows PowerShell",
        read_from: "the PowerShell finder the hook executor asks, and how it tells PowerShell 7 \
                    from Windows PowerShell, in the win32-x64 and win32-arm64 builds of 2.1.301, \
                    read as bytes on 2026-10-16, and in the win32-x64 build of 2.1.302 on \
                    2026-10-18. Measured on the session's PC, Windows 11 build 26300 on ARM64, \
                    on 2026-10-18 with 2.1.302: with neither Git nor PowerShell 7 installed it \
                    ran the line in powershell.exe, with PowerShell 7.6.7 in pwsh.exe, and with \
                    Git 2.55.0 in Git's bash.exe",
        verified_against: "2.1.302",
        depends: "claude::paths::powershell, by which doctor's status_line check names the \
                  PowerShell that runs the line, and the Windows tests that run `pitboard \
                  statusline` under Windows PowerShell 5.1 and under pwsh 7, either of which may \
                  be the one",
        probe: &[
            r#""PowerShell","7","pwsh.exe""#,
            r#""Microsoft","WindowsApps","pwsh.exe""#,
            r#"".dotnet","tools","pwsh.exe""#,
            r#"==="pwsh"?"core":"desktop""#,
            "fell_back_to_powershell_5",
        ],
        absent: &[],
    },
    Assumption {
        name: "proxy_from_variables",
        fact: "Claude Code sends its requests through the proxy the first of `https_proxy`, \
               `HTTPS_PROXY`, `http_proxy` and `HTTP_PROXY` that is set names, and exempts the \
               hosts `no_proxy` and `NO_PROXY` name, both where both are set. It reads no proxy \
               the system names: on Windows neither `Internet Settings` nor WinHTTP, so a proxy \
               only Windows' settings name carries none of its requests",
        read_from: "the proxy module's variable list, `getProxyUrlWithSource`, `getNoProxy` and \
                    `noProxyUnion`, in the darwin-arm64, linux-x64, win32-x64 and win32-arm64 \
                    builds of 2.1.301, read as bytes on 2026-10-16. None of the four holds \
                    `Internet Settings`, `ProxyEnable` or any `WinHttp` call",
        verified_against: "2.1.301",
        depends: "doctor's system_proxy note on Windows, which says Claude Code's requests go out \
                  directly where only Windows' settings name a proxy",
        probe: &[
            r#""https_proxy","HTTPS_PROXY","http_proxy","HTTP_PROXY""#,
            "Proxy settings must be a complete URL including the scheme",
        ],
        absent: &["Internet Settings", "WinHttpOpen"],
    },
    Assumption {
        name: "remote_token_files",
        fact: "where `CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR` is unset, Claude Code reads an \
               OAuth token from `/home/claude/.claude/remote/.oauth_token`, and where \
               `CLAUDE_CODE_API_KEY_FILE_DESCRIPTOR` is unset, an API key from `.api_key` \
               beside it: a regular file alone, of at most 65,536 bytes, trimmed, one that \
               holds nothing passed over, unless it runs as `mcp serve --transport http`. The \
               token is `CCR_OAUTH_TOKEN_FILE`, after `ANTHROPIC_AUTH_TOKEN` and \
               `CLAUDE_CODE_OAUTH_TOKEN` and before `apiKeyHelper` and the stored login; the \
               key is taken as `ANTHROPIC_API_KEY`, before `apiKeyHelper`, and a key of that \
               source turns off a sign-in by OAuth token, one in `CLAUDE_CODE_OAUTH_TOKEN` or \
               on a descriptor included. On Windows the path names no drive, so it leads to a \
               folder at the root of a drive, which every account on the PC shares: Bun 1.4.3 \
               takes the drive of the working folder",
        read_from: "the well-known file reader, its byte limit and its two callers, the token's \
                    place in the sources a session names and the key's in the API key's, the \
                    check that turns a claude.ai sign-in off for an API key, and the `mcp \
                    serve` check, in the darwin-arm64, linux-x64, win32-x64 and win32-arm64 \
                    builds of 2.1.303, read as bytes on 2026-10-20; and RF-T7, whose Bun 1.4.3 \
                    read a dummy there from a folder on that drive and got ENOENT from one on \
                    C: (windows-facts runs 39310064516 and 39320680285)",
        verified_against: "2.1.303",
        depends: "settings::overrides, so doctor's auth_source check and a switch's warning \
                  name either file where it holds something: at `/` off Windows, and at the \
                  root of each fixed drive on Windows, read as Claude Code reads them; and \
                  duty::warm::overridden, through overrides_of_a_run_in, so a warm-up holds \
                  back while `.api_key` holds one",
        probe: &[
            "\"/home/claude/.claude/remote\"",
            "/.oauth_token",
            "/.api_key",
            "CCR_OAUTH_TOKEN_FILE",
            "miss:\"empty\"",
            "readBoundedSync: content exceeds byte limit",
        ],
        absent: &[],
    },
    Assumption {
        name: "headless_token_descriptor",
        fact: "where `CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR` names a descriptor, Claude Code \
               reads an OAuth access token from it, an inherited pipe until a newline or its \
               end, and signs in with that token alone: no refresh token and no expiry, so it \
               is never renewed, and never written to the credential store. It is written to \
               disk only under `CLAUDE_CODE_REMOTE`. An ordinary sign-in's access token \
               serves, as a token from `claude setup-token` does: measured on 12 October 2026 \
               with the parked logins of two accounts. No keychain item was made",
        read_from: "the descriptor reader, the login it makes of the token it reads, and the \
                    disk handoff only a remote session makes; and measured in a private \
                    `CLAUDE_CONFIG_DIR` and `HOME`",
        verified_against: "2.1.296",
        depends: "Claude Code's warm-up command, which hands a run the access token alone, on \
                  descriptor 3, so a run can spend no refresh token a park or Claude Code \
                  still holds",
        probe: &[
            "CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR",
            "(until newline or end-of-stream)",
            "refreshToken:null,expiresAt:null",
        ],
        absent: &[],
    },
    Assumption {
        name: "headless_print_mode",
        fact: "`-p` answers one prompt and exits; `--model haiku` takes the smallest model; \
               `--tools \"\"` offers none; `--strict-mcp-config` starts no MCP server but \
               those named on the command line; `--disable-slash-commands` loads no skill; \
               `--no-session-persistence` keeps no session on disk; `--output-format json` \
               prints one JSON result. Print mode adds what it reads on standard input to the \
               prompt. Run so on 12 October 2026, a request cost 3,086 tokens written to the \
               cache and 6 others, and 7 to 15 seconds",
        read_from: "the command line's options and their help, and measured",
        verified_against: "2.1.296",
        depends: "Claude Code's warm-up command, its arguments, and the token kept off \
                  standard input",
        probe: &[
            "\"-p, --print\"",
            "\"--tools <tools...>\"",
            "\"--no-session-persistence\"",
            "\"--strict-mcp-config\"",
            "\"--disable-slash-commands\"",
            "\"--output-format <format>\"",
        ],
        absent: &[],
    },
    Assumption {
        name: "headless_result",
        fact: "print mode's JSON result holds `is_error`, `result`, the answer or what went \
               wrong in Claude Code's words, and where the service refused the request, \
               `api_error_status` and `api_error`: 429 with `usage_limit_reached` where a \
               limit of the account is used up, and exit status 1",
        read_from: "the result print mode prints, measured on 12 October 2026 on an account \
                    with its weekly limit used up and on two with room",
        verified_against: "2.1.296",
        depends: "Claude Code's warm-up verdict: what a run came to",
        probe: &["api_error_status", "usage_limit_reached"],
        absent: &[],
    },
    Assumption {
        name: "request_starts_the_five_hour_period",
        fact: "an account's five-hour period starts with its first request after the last one \
               reset, print mode's on the smallest model included, and counts on the plan, not \
               the monthly credit. It starts at the start of the ten minutes the request fell \
               in: a request at 17:52 gave a reset at 22:50, one at 18:02 gave 23:00. Until \
               then the usage answer gives the limit's row, `session`, at 0% with no reset, \
               and `five_hour` with no reset",
        read_from: "measured on 12 October 2026 on a Team 5x and a Max 20x account: the usage \
                    answer before a request and 20 seconds after it",
        verified_against: "2.1.296",
        depends: "warm::decide, which reads a period not running as one with no reset or a \
                  passed one, and counts a warm-up the last reading did not see as a period \
                  running from the start of its ten minutes",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "weekly_period_is_a_fixed_cycle",
        fact: "the weekly limit, and a model's weekly limit beside it, runs on a fixed cycle: \
               at its reset the next week starts at once, with no request, at 0%",
        read_from: "measured on 12 October 2026 on a Max 20x account whose weekly limit reset \
                    at 18:00: at 18:02, before any request, the answer gave the next week's \
                    reset at 0%",
        verified_against: "2.1.296",
        depends: "warm::decide, which starts the five-hour period alone",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "used_up_limit_refuses_every_request",
        fact: "while a limit of the account's own, not a model's, is used up, the service \
               refuses a request, which spends nothing and starts no five-hour period. Not \
               measured with only a model's limit used up",
        read_from: "measured on 12 October 2026 on a Max 20x account with its weekly limit \
                    used up: the request was refused with `usage_limit_reached`, 0 tokens, and \
                    the answer after it still had no five-hour period running",
        verified_against: "2.1.296",
        depends: "warm::decide, which warms up no account with a limit used up",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "refusals_are_final",
        fact: "Claude Code asks Anthropic again after a network failure, a timeout, a 5xx, a \
               408, a 409 or a 429, and takes any other 4xx as final. An error answer from \
               `api.anthropic.com` is `{\"type\":\"error\",\"error\":{\"type\",\"message\", \
               \"details\"},\"request_id\"}`, and Claude Code shows `error.message` where \
               `details.error_visibility` is `user_facing`",
        read_from: "the retry rule the OAuth requests share and the profile, validate and \
                    roles requests' error handling; the envelope measured on 16 October 2026 \
                    from a 401 and a 405 of `/api/oauth/profile`",
        verified_against: "2.1.301",
        depends: "Refusal::is_final, which makes a final 4xx a refusal that is not worth \
                  retrying and keeps its message and request id",
        probe: &["error_visibility", "user_facing"],
        absent: &[],
    },
    Assumption {
        name: "account_refusals",
        fact: "a 403 `permission_error` whose `details.error_code` is `verification_required`, \
               and a 401 or 403 whose message says `OAuth authentication is currently not \
               allowed for this organization`, refuse the account, not an expired token: \
               Claude Code shows the message and does not renew",
        read_from: "the verification check of the API key and API errors, and the \
                    organisation check of the API errors",
        verified_against: "2.1.301",
        depends: "the Claude provider's reading of a 401 or 403, which takes these two as \
                  refusals and every other as a token to renew",
        probe: &[
            "verification_required",
            "OAuth authentication is currently not allowed for this organization",
        ],
        absent: &[],
    },
    Assumption {
        name: "token_refusals",
        fact: "the token endpoint's error answer is `{\"error\", \"error_description\", \
               \"error_uri\"}`, `error` a string or `{type}`. A 400, 401 or 403 whose `error` is \
               `invalid_grant` or `access_denied` and whose description is `account_on_hold` is \
               an account on hold: Claude Code keeps the login and shows the `error_uri`, only \
               an https page on `portal.anthropic.com`, `claude.ai` or `platform.claude.com`. \
               A 400 or 401 `invalid_grant` otherwise is a refresh token spent for good. \
               Descriptions it knows: `Refresh token expired`, `Refresh token not found or \
               invalid`, `No organization associated with this token`, `No account \
               associated with this token`",
        read_from: "the refresh's error readers: the on-hold check, the dead-token check, the \
                    invalid-scope fallback and the descriptions its failure event names, and \
                    the link check of the on-hold notice",
        verified_against: "2.1.301",
        depends: "the Claude provider's renewal, which keeps a park of an account on hold and \
                  drops one only for `invalid_grant`",
        probe: &[
            "account_on_hold",
            "No organization associated with this token",
            "portal.anthropic.com",
        ],
        absent: &[],
    },
    Assumption {
        name: "validate_names_the_login",
        fact: "`POST /api/oauth/validate` with an access token as `Authorization: Bearer`, \
               `Content-Type: application/json` and no body answers `{\"valid\", \
               \"account_uuid\", \"organization_uuid\", \"scopes\"}` for a token with \
               `user:profile`, `user:office` or `user:ccr_inference`. Claude Code asks it where \
               an identity decides something: whether a login is of an organisation managed \
               settings require, and which account the browser bridge serves, over the \
               config's. Its sign-in identifies a login by the profile or else the token \
               answer, and signs in where the profile cannot be read",
        read_from: "the validate request and its retry list, the organisation pin's check, the \
                    browser bridge's account, and the sign-in's identity; the answer's keys \
                    measured on 16 October 2026 from a Max login",
        verified_against: "2.1.301",
        depends: "the Claude provider's identity, which every switch, read and sign-in asks, \
                  and which needs no profile for an account Pitboard knows",
        probe: &[
            "/api/oauth/validate",
            "resolves to a different claude.ai account than the persisted Claude Code login",
        ],
        absent: &[],
    },
    Assumption {
        name: "token_answer_names_the_login",
        fact: "the token endpoint's answer to a renewal carries `account.uuid`, \
               `account.email_address` and, where the login has one, `organization.uuid`, \
               which Claude Code keeps as the login's `tokenAccount`",
        read_from: "the refresh's answer reader",
        verified_against: "2.1.301",
        depends: "the Claude provider's renewal, whose answer names the login renewed where it \
                  names all three, so nothing is asked about it after",
        probe: &["account.email_address", "tokenAccount:"],
        absent: &[],
    },
    Assumption {
        name: "refresh_token_end",
        fact: "a sign-in sets `refreshTokenExpiresAt` from its answer's \
               `refresh_token_expires_in`, or 30 days on where the answer has none. A renewal \
               sets it from that field only where its answer has one, and otherwise keeps the \
               date the login had. Within 3 days of that date a claude.ai login is told `Your \
               login expires in <n> days · run /login to renew`: Claude Code sends its owner to \
               sign in again, not to a renewal",
        read_from: "the sign-in's tokens, the token refresh's answer reader and the save that \
                    merges it into the login stored, and the `oauth-expiry` notice",
        verified_against: "2.1.309",
        depends: "provider::claude::document::renewed, which keeps the date as Claude Code does, \
                  and status::Row::standing's Expiring, from doctor::RENEW_WITHIN's 3 days, \
                  which `pitboard status`, doctor's parked login check and both apps answer \
                  with a sign-in",
        probe: &[
            "Your login expires in ",
            "run /login to renew",
            ".refresh_token_expires_in,!1)",
            "refreshTokenExpiresAt:n.refreshTokenExpiresAt??e?.refreshTokenExpiresAt",
        ],
        absent: &[],
    },
    Assumption {
        name: "auth_precedence",
        fact: "a session signs in with the stored claude.ai login only where nothing before it \
               names another way: `ANTHROPIC_API_KEY`, then for a token `ANTHROPIC_AUTH_TOKEN`, \
               `CLAUDE_CODE_OAUTH_TOKEN`, a token on a descriptor or in its well-known file, \
               `apiKeyHelper` and an Anthropic profile (anthropic_profiles). It does so only while \
               its provider is Anthropic's own: any of `CLAUDE_CODE_USE_BEDROCK`, \
               `CLAUDE_CODE_USE_FOUNDRY`, `CLAUDE_CODE_USE_ANTHROPIC_AWS`, \
               `CLAUDE_CODE_USE_ANTHROPIC_GOOGLE_CLOUD`, `CLAUDE_CODE_USE_MANTLE` and \
               `CLAUDE_CODE_USE_VERTEX` set chooses another, and the Cloud gateway is chosen by a \
               gateway sign-in held, by `CLAUDE_CODE_USE_GATEWAY`, or by managed settings that set \
               `forceLoginMethod` to `gateway` or name a `forceLoginGatewayUrl`. The person's own \
               settings with those two only aim `/login` at a gateway, and the sign-in it makes \
               is kept as `enterpriseGateway`, which account_scoped_keys moves with the account",
        read_from: "the token source check and the session's provider choice, the managed \
                    sign-in pins the gateway check reads, the pre-fill that takes the person's \
                    own `forceLoginGatewayUrl`, and the gateway sign-in's save and restore, in the \
                    darwin-arm64, linux-x64, win32-x64 and win32-arm64 builds of 2.1.309, read as \
                    bytes on 2026-10-23",
        verified_against: "2.1.309",
        depends: "settings::OVERRIDING_ENV and settings::overrides, so doctor's auth_source \
                  check, a switch's auth_overridden warning and the automatic switch name each, \
                  and overrides_of_a_run_in, so a warm-up holds back under managed settings that \
                  require the gateway",
        probe: &[
            r#"mantle:"CLAUDE_CODE_USE_MANTLE""#,
            r#"because managed settings on this machine require the gateway sign-in (forceLoginMethod "gateway" / forceLoginGatewayUrl)."#,
            "Honored from admin-controlled managed settings (MDM / managed-settings.json / \
             policy helper) and, on a machine with none of those, from your own user settings",
        ],
        absent: &[],
    },
    Assumption {
        name: "anthropic_profiles",
        fact: "an Anthropic profile is `configs/<name>.json` in the profile folder \
               (profile_folder, windows_profile_folder), its mode `authentication.type`, and a \
               session takes one before the stored login. Where `ANTHROPIC_PROFILE` is set it \
               takes the profile that names where its mode is `oidc_federation`, or \
               `user_oauth` with a login in its credentials file, \
               `authentication.credentials_path` or `credentials/<name>.json`, and looks at \
               nothing else. Without it, `ANTHROPIC_FEDERATION_RULE_ID` and \
               `ANTHROPIC_ORGANIZATION_ID` both set are a federation of their own. Without \
               those, it takes the profile `active_config` names, or `default`, where its mode is \
               `oidc_federation`; a `user_oauth` one comes after a claude.ai login. Each variable \
               and `ANTHROPIC_CONFIG_DIR` is trimmed, and read from the process's environment, \
               which a session sets as it starts from the `env` of its config file, then of each \
               settings source over it, the managed ones highest: the person's, a project's, a \
               local one, a `--settings` file's and the managed sources'. A number or a boolean \
               is taken as its text and any other value dropped, and a project's or a local \
               one's `ANTHROPIC_CONFIG_DIR` is dropped",
        read_from: "the profile precedence, the profile's mode and credentials file readers, the \
                    active profile reader, and the token source check that takes a profile after \
                    `apiKeyHelper`, in the darwin-arm64, linux-x64, win32-x64 and win32-arm64 \
                    builds of 2.1.309, read as bytes on 2026-10-23; the config file's and the \
                    settings' `env` a session applies to its environment at start, before it \
                    looks for a profile, and the filters that drop a name from it, read in the \
                    same builds on 2026-10-24",
        verified_against: "2.1.309",
        depends: "settings::Profiles and settings::overrides, which read each variable from the \
                  managed sources' `env`, the person's settings' and the config file's before the \
                  environment, and from every managed source they can read where one cannot be, \
                  so doctor's auth_source check, a switch's auth_overridden warning and the \
                  automatic switch name the profile. They read no project's, local or \
                  `--settings` file. They take a credentials file that is there and not empty \
                  for one that holds a login, and never read it",
        probe: &[
            r#"["ANTHROPIC_FEDERATION_RULE_ID","ANTHROPIC_ORGANIZATION_ID"]"#,
            r#"this.filterSettingsEnv(this.getLegacyEnvMap(),"globalConfig")"#,
            "An Anthropic profile (~/.config/anthropic) is configured, but a claude.ai login \
             exists",
            r#"{source:"profile",hasToken:!0}"#,
        ],
        absent: &[],
    },
    Assumption {
        name: "profile_folder",
        fact: "the profile folder is `ANTHROPIC_CONFIG_DIR`, else `anthropic` in \
               `XDG_CONFIG_HOME`, else `.config/anthropic` in `HOME`, each trimmed",
        read_from: "the profile folder's resolver in the darwin-arm64 and linux-x64 builds of \
                    2.1.309, read as bytes on 2026-10-23",
        verified_against: "2.1.309",
        depends: "settings::Profiles::read off Windows",
        probe: &[
            ".ANTHROPIC_CONFIG_DIR?.trim();if(",
            r#".config","anthropic"),space:"home"}"#,
        ],
        absent: &[],
    },
    Assumption {
        name: "windows_profile_folder",
        fact: "on Windows the profile folder is `ANTHROPIC_CONFIG_DIR`, else `Anthropic` in \
               `APPDATA`, else `AppData\\Roaming\\Anthropic` in `USERPROFILE`, each trimmed",
        read_from: "the profile folder's resolver in the win32-x64 and win32-arm64 builds of \
                    2.1.309, read as bytes on 2026-10-23",
        verified_against: "2.1.309",
        depends: "settings::Profiles::read on Windows",
        probe: &[r#""AppData","Roaming","Anthropic"),space:"home"}"#],
        absent: &[],
    },
    Assumption {
        name: "organisation_pin",
        fact: "`forceLoginOrgUUID`, one organisation's uuid or a list of them, any one \
               permitted, from the managed sources a session takes, makes Claude Code refuse a \
               claude.ai login of an organisation it does not name, as a session starts and at a \
               sign-in. A session it refuses stops with the reason and leaves the login stored; a \
               sign-in it refuses puts back the login before it. An empty list, or a managed \
               source it cannot read, refuses every login. The person's own settings with it \
               only pre-fill the sign-in",
        read_from: "the organisation check a session runs as it starts and a sign-in runs on \
                    the login it makes, the schema's description, and the restore a refused \
                    sign-in runs, in the darwin-arm64, linux-x64, win32-x64 and win32-arm64 \
                    builds of 2.1.309, read as bytes on 2026-10-23",
        verified_against: "2.1.309",
        depends: "settings::organisation_pins, through the managed sources settings::managed_pins \
                  takes it from (managed_login_pins), the profile's plist on macOS \
                  (macos_managed_preferences) among them, and Claude's Provider::refuses, so a \
                  switch and a sign-in that would replace the login in use refuse with \
                  `login_restricted` an account of an organisation no managed source names, and \
                  doctor's login_restrictions names the setting and those accounts",
        probe: &[
            "Organization UUID to require for OAuth login. Accepts a single UUID string or an \
             array of UUIDs (any one is permitted).",
            "forceLoginOrgUUID in managed settings is set to an empty array.",
        ],
        absent: &[],
    },
    Assumption {
        name: "login_method_pin",
        fact: "`forceLoginMethod` `claudeai` or `console`, from the highest settings source \
               that sets it, a managed one (managed_login_pins), the person's own, a project's \
               or the command line's, pins only how Claude Code signs in, after a gateway \
               sign-in that managed settings require (auth_precedence), which comes first: \
               `/login` and `claude auth login` sign in that way whatever `--claudeai` or \
               `--console` asks, and under `console` a claude.ai sign-in from the SDK, \
               `setup-token`, Claude Desktop or a cloud session is refused. A Console sign-in \
               stores no claude.ai login: it makes an API key with `create_api_key` and keeps \
               it, on macOS in a keychain item named as credential_service_name's without \
               `-credentials`, elsewhere as `primaryApiKey` in the config file, and says \
               `Login successful.` A session that starts with a claude.ai login stored takes it \
               whatever the setting says, and renews it unchecked",
        read_from: "the method pin's reader, which asks the managed gateway check first and then \
                    takes the merged settings, and its check; `/login`'s and `claude auth \
                    login`'s choice of sign-in, and the SDK's `claude_authenticate`, \
                    `setup-token`, the desktop app's sign-in and a cloud session's sign-in, which \
                    refuse under it; the sign-in's token save, which keeps only a login with \
                    `user:inference`, and the API key it makes and saves for any other; the \
                    person's settings file, `settings.json` in the config folder \
                    `CLAUDE_CONFIG_DIR` names; and the session's start, its token source and its \
                    renewal, none of which reads it, in the darwin-arm64, linux-x64, win32-x64 \
                    and win32-arm64 builds of 2.1.310, read as bytes on 2026-10-24",
        verified_against: "2.1.310",
        depends: "settings::overrides, which leaves `forceLoginMethod` out where it is not \
                  `gateway`, so a switch under `console` warns of nothing and the automatic \
                  switch goes on: a session takes the login a switch puts in place. \
                  settings::console_pin, so a sign-in under a managed `console` and no managed \
                  gateway, which would make an API key and leave nothing to enrol, refuses with \
                  `sign_in_pinned_to_console` before `claude auth login` starts, and doctor's \
                  login_restrictions names it. The person's own settings and a project's \
                  `.claude/settings.json` do not reach a sign-in in Pitboard's private folder \
                  (project_settings_folder)",
        probe: &[
            "forceLoginMethod is 'console' in ",
            "; log in with an Anthropic Console account instead.",
            "Signing in isn't available here: a cloud session needs a claude.ai sign-in, and the \
             forceLoginMethod setting on this machine requires a different sign-in method.",
            "tengu_oauth_tokens_not_claude_ai",
            "Unable to create API key. The server accepted the request but did not return a key.",
            r#"?.forceLoginMethod;return e==="gateway"?void 0:e}"#,
        ],
        absent: &[],
    },
    Assumption {
        name: "managed_login_pins",
        fact: "Claude Code takes `forceLoginOrgUUID`, `forceLoginMethod` and \
               `forceLoginGatewayUrl` from the managed settings it composes. Where \
               `managedSourcesBehavior` merges the managed folder into the configuration \
               profile's plist on macOS or `HKLM` on Windows, it takes them from that first \
               source alone and drops the folder's. In the folder a later file's value overrides \
               an earlier one's. Its schema drops a `forceLoginMethod` other than `claudeai`, \
               `console` or `gateway`, and a `forceLoginGatewayUrl` that is not a string or is \
               empty",
        read_from: "the keys the managed sources' merge takes from the first source alone and \
                    drops from the others, the merge, the folder's merge of \
                    `managed-settings.json` and `managed-settings.d`, the machine's login pins \
                    and gateway check, which read the composed managed settings, and the \
                    settings schema, in the darwin-arm64, linux-x64, win32-x64 and win32-arm64 \
                    builds of 2.1.310, read as bytes on 2026-10-24. The merge's list is the same \
                    in the darwin-arm64 build of 2.1.309",
        verified_against: "2.1.310",
        depends: "settings::managed_pins, so organisation_pins and console_pin leave out a \
                  folder the first source merges, and console_pin, which takes the folder's last \
                  `forceLoginMethod` its schema keeps",
        probe: &[
            "WINNER_ONLY_UNDER_MERGE",
            r#""forceLoginOrgUUID","forceLoginMethod","forceLoginGatewayUrl","gatewayInternalNetworks""#,
            r#"["claudeai","console","gateway"]).optional().catch(void 0)"#,
        ],
        absent: &[],
    },
    Assumption {
        name: "macos_managed_preferences",
        fact: "on macOS Claude Code also takes managed settings from a configuration profile's \
               plist: `/Library/Managed Preferences/<user>/com.anthropic.claudecode.plist`, \
               `<user>` the name this user's passwd entry gives, then \
               `/Library/Managed Preferences/com.anthropic.claudecode.plist`, the first it can \
               read, converted with `/usr/bin/plutil -convert json -o - --` within 5 seconds. \
               One `plutil -lint` takes but that cannot be converted, holding a date or data, is \
               unreadable; one it does not take for a plist is empty. Where that plist holds a \
               policy it is taken over the managed folder, as `HKLM` is on Windows, unless \
               `managedSourcesBehavior` merges them",
        read_from: "the plists' paths and labels, their reading through `plutil`, and the managed \
                    sources' order, whose second tier is the plist on macOS and `HKLM` on \
                    Windows, in the darwin-arm64 build of 2.1.309, read as bytes on 2026-10-23",
        verified_against: "2.1.309",
        depends: "settings::overrides, custom_oauth, env_settings, organisation_pins and \
                  console_pin, through host::Host::managed_plist and settings::managed_taken and \
                  managed_pins",
        probe: &[
            "/Library/Managed Preferences/",
            "per-user managed preferences",
            "device-level managed preferences",
            "it holds a value plutil cannot convert to JSON (a date or data value)",
        ],
        absent: &[],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assumptions::Platform;

    // Read there, the keychain facts would report the keychain gone, and no_keyring_off_macos a
    // keyring arrived.
    #[test]
    fn windows_reads_the_facts_its_build_was_read_for() {
        let read: Vec<&str> = PER_SYSTEM
            .iter()
            .filter(|line| matches!(line.on(Platform::Windows), Read(_)))
            .map(|line| line.name)
            .collect();
        assert_eq!(
            read,
            [
                "write_lock",
                "logout_skips_the_lock",
                "account_scoped_keys",
                "config_file_location",
                "settings_byte_order_mark",
                "oauth_client",
                "sign_in_output",
                "sign_in_takes_another_code",
                "project_settings_folder",
                "fallback_file_pins_session_login",
                "login_is_one_organisation",
                "config_may_name_no_organisation",
                "usage_cache_stamp_is_the_configs",
                "status_reads_the_config_usage_the_token",
                "config_identity_is_the_last_writers",
                "status_line_input_names_no_account",
                "refresh_lock",
                "login_carries_its_plan",
                "windows_home_folder",
                "windows_open_sharing",
                "windows_install_layout",
                "windows_version_resource",
                "windows_managed_settings",
                "windows_daemon_identity",
                "windows_backend_choice",
                "windows_plaintext_strict_read",
                "windows_plaintext_write",
                "windows_file_adoption",
                "credman_account_name",
                "credman_target",
                "credman_chunking",
                "credman_probe",
                "credman_migration",
                "credman_move_lost_a_piece",
                "credman_unreachable_writes_file",
                "credman_attributes",
                "windows_browser_open",
                "windows_token_handover",
                "windows_token_env",
                "windows_statusline_shell",
                "windows_statusline_powershell",
                "proxy_from_variables",
                "remote_token_files",
                "headless_print_mode",
                "headless_result",
                "request_starts_the_five_hour_period",
                "weekly_period_is_a_fixed_cycle",
                "used_up_limit_refuses_every_request",
                "refusals_are_final",
                "account_refusals",
                "token_refusals",
                "validate_names_the_login",
                "token_answer_names_the_login",
                "refresh_token_end",
                "auth_precedence",
                "anthropic_profiles",
                "windows_profile_folder",
                "organisation_pin",
                "login_method_pin",
                "managed_login_pins",
            ]
        );
        for name in [
            "keychain_write_route",
            "keychain_absence_codes",
            "no_keyring_off_macos",
            "headless_token_descriptor",
        ] {
            let line = PER_SYSTEM.iter().find(|l| l.name == name).unwrap();
            assert!(matches!(line.on(Platform::Windows), NotRead(_)), "{name}");
        }
    }
}
