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
pub const VERIFIED_AGAINST: &str = "2.1.284";

/// The build the facts read on Windows were read from: `@anthropic-ai/claude-code-win32-x64`
/// and `-win32-arm64` 2.1.289, both built from commit 736d26e, as that version's macOS and
/// Linux builds are. Read as bytes on a Mac on 2026-10-06, and never run.
pub const WINDOWS_VERIFIED_AGAINST: &str = "2.1.289";

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

/// A fact read on macOS and Linux from the build its entry names, whose Windows reading
/// waits on the Windows work.
const fn later_on_windows(
    name: &'static str,
    build: &'static str,
    by: &'static [&'static str],
    reads: &'static str,
) -> PerSystem {
    PerSystem {
        name,
        macos: Read(build),
        linux: Read(build),
        windows: Pending { by, reads },
    }
}

/// What each fact below is on each system. The Windows readings are a reading of the
/// Windows builds' code for each fact, and of every literal its entry probes for, which
/// both Windows builds hold. Where Claude Code does something else on Windows, that is a
/// fact of its own, read by the pull request named.
pub const PER_SYSTEM: &[PerSystem] = &[
    PerSystem {
        name: "credential_service_name",
        macos: Read(VERIFIED_AGAINST),
        linux: NO_KEYCHAIN_ON_LINUX,
        windows: Pending {
            by: &["W23"],
            reads: "credman_target: the Credential Manager target Claude Code keeps the login \
                    under on Windows, once it uses that store",
        },
    },
    PerSystem {
        name: "keychain_account_name",
        macos: Read(VERIFIED_AGAINST),
        linux: NO_KEYCHAIN_ON_LINUX,
        windows: Pending {
            by: &["W22"],
            reads: "credman_account_name: the user name on Claude Code's Credential Manager \
                    item",
        },
    },
    later_on_windows(
        "live_chain_order",
        VERIFIED_AGAINST,
        &["W22", "W23"],
        "windows_backend_choice (W22): which store Claude Code keeps the login in on Windows, \
         and credman_migration (W23): how it moves the login into Credential Manager",
    ),
    PerSystem {
        name: "keychain_write_route",
        macos: Read(VERIFIED_AGAINST),
        linux: NO_KEYCHAIN_ON_LINUX,
        windows: NO_KEYCHAIN_ON_WINDOWS,
    },
    PerSystem {
        name: "keychain_absence_codes",
        macos: Read(VERIFIED_AGAINST),
        linux: NO_KEYCHAIN_ON_LINUX,
        windows: NO_KEYCHAIN_ON_WINDOWS,
    },
    everywhere("write_lock", VERIFIED_AGAINST),
    everywhere("logout_skips_the_lock", VERIFIED_AGAINST),
    everywhere("account_scoped_keys", VERIFIED_AGAINST),
    later_on_windows(
        "credential_cache",
        "2.1.278",
        &["W22", "W23"],
        "windows_file_adoption (W22): how soon a running session takes a new \
         `.credentials.json` on Windows, and windows_credman_cache (W23): how long it keeps a \
         login read from Credential Manager",
    ),
    everywhere("config_file_location", "2.1.289"),
    everywhere("oauth_client", VERIFIED_AGAINST),
    later_on_windows(
        "supervisor_daemon",
        VERIFIED_AGAINST,
        &["W18"],
        "windows_daemon_identity: how the supervisor daemon is told apart from other \
         processes on Windows",
    ),
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
    everywhere("sign_in_output", "2.1.289"),
    everywhere("sign_in_takes_another_code", "2.1.289"),
    later_on_windows(
        "install_places",
        "2.1.289",
        &["W17"],
        "windows_install_layout: where the native installer and npm put `claude.exe` on \
         Windows",
    ),
    later_on_windows(
        "plaintext_credential_mode",
        VERIFIED_AGAINST,
        &["W22"],
        "windows_plaintext_write: how Claude Code writes `.credentials.json` on Windows, and \
         what keeps others out of it there",
    ),
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
        windows: NO_KEYCHAIN_ON_WINDOWS,
    },
    PerSystem {
        name: "fallback_outlives_keychain_writes",
        macos: Read("2.1.294"),
        linux: NO_KEYCHAIN_ON_LINUX,
        windows: NO_KEYCHAIN_ON_WINDOWS,
    },
    PerSystem {
        name: "fallback_file_pins_session_login",
        macos: Read("2.1.294"),
        linux: NO_KEYCHAIN_ON_LINUX,
        windows: Pending {
            by: &["W22"],
            reads: "windows_file_adoption: how soon a running session takes a new \
                    `.credentials.json` on Windows, where it is the store and where it sits \
                    behind Credential Manager",
        },
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
    PerSystem {
        name: "usage_cache_stamp_is_the_configs",
        macos: Read("2.1.294"),
        linux: Read("2.1.294"),
        windows: Read("2.1.294"),
    },
    PerSystem {
        name: "status_reads_the_config_usage_the_token",
        macos: Read("2.1.294"),
        linux: Read("2.1.294"),
        windows: Read("2.1.294"),
    },
    PerSystem {
        name: "config_identity_is_the_last_writers",
        macos: Read("2.1.294"),
        linux: Read("2.1.294"),
        windows: Read("2.1.294"),
    },
    PerSystem {
        name: "status_line_input_names_no_account",
        macos: Read("2.1.294"),
        linux: Read("2.1.294"),
        windows: Read("2.1.294"),
    },
    PerSystem {
        name: "refresh_lock",
        macos: Read("2.1.294"),
        linux: Read("2.1.294"),
        windows: Read("2.1.294"),
    },
];

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
        fact: "on macOS the login is read from the keychain first and a plaintext \
               `.credentials.json` second, and a write moves it to the file only when the \
               keychain write fails for good; on Linux the file is the only store. The \
               successor backend replaces the fallback half and only for a caller that hands a \
               backend in, which an ordinary `claude` does not",
        read_from: "the keychain-with-plaintext-fallback store and `getSecureStorage`",
        verified_against: VERIFIED_AGAINST,
        depends: "store::resolve and everything that reads or writes through it",
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
        verified_against: VERIFIED_AGAINST,
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
        verified_against: VERIFIED_AGAINST,
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
        depends: "lock.rs, the whole switch, and `pitboard stow`, which holds it while it writes \
                  a login left in `.credentials.json` back renewed and while it reads the file and \
                  the login stored a last time and deletes the file",
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
        // 2.1.294, whose code here is the same.
        verified_against: "2.1.294",
        depends: "lock::REFRESH, and `pitboard stow`, which holds it from its last reading of \
                  a login left in `.credentials.json` until the file is gone, so no session \
                  spends the refresh token of the login it parks, drops or renews meanwhile, and \
                  writes no owner record, so no session takes it over",
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
               enterpriseGateway and designOauth, and `/logout` clears the whole document but \
               writes back `coworkRemoteDevice`, so those five belong to the account",
        read_from: "the re-login prune and the logout path",
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
        fact: "a legacy `<config dir>/.config.json` wins when present; otherwise \
               `<$CLAUDE_CONFIG_DIR or the home folder>/.claude<suffix>.json`. The two read \
               `CLAUDE_CONFIG_DIR` differently: the file's base is `CLAUDE_CONFIG_DIR || \
               homedir()`, so an empty value is the home folder, while the config dir is \
               `(CLAUDE_CONFIG_DIR ?? join(homedir(), \".claude\")).normalize(\"NFC\")`, so an \
               empty value makes it the empty path and `.config.json` is looked for in the \
               working directory",
        read_from: "the config path resolution, and the config dir the secure storage module \
                    shares",
        // Read on 2026-10-06 from the macOS, Linux and Windows builds of 2.1.289, x64 and
        // arm64, whose code for both paths is the same.
        verified_against: "2.1.289",
        depends: "claude::config_file and claude::config_dir, which read CLAUDE_CONFIG_DIR \
                  the two ways, and home::check_absolute, which refuses an empty one",
        probe: &[".config.json", "CLAUDE_CONFIG_DIR"],
        absent: &[],
    },
    Assumption {
        name: "oauth_client",
        fact: "every login Claude Code stores was issued to client 9d1c250a-e61b-44d9-88ed-5944d1962f5e, \
               and a refresh answer without a refresh-token lifetime keeps the one it had",
        read_from: "the OAuth client id and the token refresh path",
        verified_against: VERIFIED_AGAINST,
        depends: "api::CLIENT_ID and park::renewed, and the renewal `pitboard stow` makes of a \
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
               `auth login` has none",
        read_from: "the `auth login` command's OAuth flow and `startOAuthFlow`, the authorize \
                    address builder and its constants, the hyperlink helper the address is \
                    printed through with `assumeSupport`, and the supports-hyperlinks check \
                    it asks",
        // Read from a newer build than the rest of this register.
        verified_against: "2.1.289",
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
               look fails, it drops the login it holds and reads the keychain through its 30 \
               second cache. Where it is there, whatever it holds and whether or not it can be \
               read, the session keeps the login it holds while that login is usable: it reads \
               again where the file's modification time has changed, at most every 30 seconds \
               where that login is gone or its refresh token is empty or known dead, after a \
               401, and at its own sign-in, and 5 minutes before the login expires it reads \
               the store and takes the login there where it differs. So while the file sits \
               behind the keychain, a session already running keeps the account it is on \
               after a switch until its login is next renewed, or until it is started again. \
               Read, not measured against a running session",
        read_from: "the check before each API client, which stats the file, compares its \
                    modification time with the one it last saw, reads through the keychain \
                    cache where the stat fails, and otherwise keeps a usable login, reading \
                    again at most every 30 seconds for one that is not; the renewal check 5 \
                    minutes before expiry, which reads the store first and takes its login \
                    where it differs; and the 401 handlers",
        // Read on 2026-10-08 from the macOS build and on 2026-10-09 from the Linux x64,
        // Windows x64 and Windows arm64 builds of 2.1.294, whose code here is the same. On
        // Linux the file is the only store, and on Windows it is unless Credential Manager
        // is turned on, by `tengu_windows_credman` or `CLAUDE_CODE_FORCE_WINDOWS_CREDMAN`.
        verified_against: "2.1.294",
        depends: "Claude's adoption, by which a switch says that running sessions take it at \
                  their login's next renewal while the file is behind the keychain; Claude's \
                  behind, which tells the file by a look and says nothing where the look fails; \
                  the fallback_login warning and check, which say so of a file with no login in \
                  it, or one Pitboard cannot read, too; and `pitboard stow`, which says sessions \
                  already running follow a switch within 33 seconds again once the file is gone",
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
                  the config wrote it, so in_use::known tells a change of it by the same id",
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
        // Windows x64 and Windows arm64 builds of 2.1.294, whose code here is the same.
        verified_against: "2.1.294",
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
        // Windows x64 and Windows arm64 builds of 2.1.294, whose code here is the same.
        verified_against: "2.1.294",
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
        // Windows x64 and Windows arm64 builds of 2.1.294, whose code here is the same.
        verified_against: "2.1.294",
        depends: "in_use::known, which takes the config naming another account than when \
                  Anthropic last named the login stored as a sign that something signed in, \
                  never as whose the login is; and every reader of the account in use, which \
                  takes it from that record and not from the config",
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
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assumptions::Platform;

    /// The facts the Windows builds were read for, and those they were not, by name. Read on Windows, the keychain facts would report the keychain gone, and
    /// `no_keyring_off_macos` a keyring arrived.
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
                "oauth_client",
                "sign_in_output",
                "sign_in_takes_another_code",
                "login_is_one_organisation",
                "config_may_name_no_organisation",
                "usage_cache_stamp_is_the_configs",
                "status_reads_the_config_usage_the_token",
                "config_identity_is_the_last_writers",
                "status_line_input_names_no_account",
                "refresh_lock",
            ]
        );
        for name in [
            "keychain_write_route",
            "keychain_absence_codes",
            "no_keyring_off_macos",
        ] {
            let line = PER_SYSTEM.iter().find(|l| l.name == name).unwrap();
            assert!(matches!(line.on(Platform::Windows), NotRead(_)), "{name}");
        }
    }
}
