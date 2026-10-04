//! Every fact about Claude Desktop that Pitboard stands on, named and dated.
//!
//! Read from Claude Desktop 2.19675.0 on a Mac on 3 October 2026: its bundle, its data
//! folder, its cookie database copied aside, its processes and its keychain item's
//! attributes. The experiments of 4 October 2026, run on the same build with two real
//! accounts, settled most of the rest, each entry naming the experiment (E1 to E18 for the
//! app, U-K1 to U-K5 for the keychain). Four facts still wait for a read: whether the
//! session's expiry slides, whether a sign-in replaces a uuid Log out left behind, whether
//! reading the key slows the app, and whether an update keeps the keychain item. Each is dated [`UNVERIFIED`] and names the experiment that will
//! settle it. Code that can choose between two behaviours on one asks
//! [`crate::assumptions::verified`] first; code that cannot work at all without one leans on
//! it as written, and `pitboard doctor` lists it as unverified.
//!
//! Nothing here can be probed for in a build: the app's literals are in `app.asar`, not in
//! its Mach-O binary, so every entry has an empty probe.
//!
//! See [`crate::assumptions`] for what an entry means.

use crate::assumptions::{Assumption, Platform, UNVERIFIED};

/// The build every measured entry below was read from.
pub const VERIFIED_AGAINST: &str = "2.19675.0";

/// Claude Desktop is a Mac app here, so every fact is read from the macOS build.
pub fn read_on(_name: &str) -> &'static [Platform] {
    &[Platform::MacOs]
}

pub const ASSUMPTIONS: &[Assumption] = &[
    Assumption {
        name: "desktop_bundle",
        fact: "the app is `/Applications/Claude.app`, bundle id `com.anthropic.claudefordesktop`, signed by team `Q6L2SF6YDW`, with ElectronAsarIntegrity in its Info.plist",
        read_from: "the app's Info.plist and `codesign -dv`",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::desktop's BUNDLE_ID and BUNDLE_NAME, and the app Pitboard quits and opens",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_support_dir",
        fact: "the app keeps its data in `~/Library/Application Support/Claude`",
        read_from: "the data folder on a Mac running it",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::desktop::support_dir, and every item a switch moves",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_cookie_format",
        fact: "`Cookies` is a SQLite database with `meta.version` 24, and every value in it starts `v10`",
        read_from: "sqlite3 run on a copy of the database",
        verified_against: VERIFIED_AGAINST,
        depends: "the cookie reader, which refuses any other version or prefix",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_cookie_journal_idle",
        fact: "while the app runs, `Cookies-journal` is empty between writes, truncated in the same moment the jar is written, and there is no `Cookies-wal`",
        read_from: "the size and mtime of `Cookies` and `Cookies-journal` beside a running app, read twice on 3 October 2026 without opening either",
        verified_against: VERIFIED_AGAINST,
        depends: "the cookie reader, which opens the jar immutable and refuses it while its journal or log holds anything",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_session_twins",
        fact: "`sessionKey` and `sessionKeyV3` hold ciphertexts with the same SHA-256",
        read_from: "the cookie database, measured on 3 October 2026",
        verified_against: VERIFIED_AGAINST,
        depends: "the session fingerprint, which is taken from `sessionKey`",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_session_lifetime",
        fact: "the session cookie lives about four weeks: one set on 2 October 2026 expires on 30 October 2026",
        read_from: "the cookie's `expires_utc`",
        verified_against: VERIFIED_AGAINST,
        depends: "when a parked Claude Desktop login is said to expire",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_processes",
        fact: "the app runs a main process, `Claude Helper` processes and a `chrome_crashpad_handler` with `--user-data-dir`, all inside the bundle; `Contents/Helpers/chrome-native-host` is a child of Chrome, not of the app. The main program is `Contents/MacOS/Claude` (the Info.plist's `CFBundleExecutable`) and the helpers are `Contents/Frameworks/Claude Helper.app`, `Claude Helper (GPU).app`, `Claude Helper (Plugin).app` and `Claude Helper (Renderer).app`, none named for the bundle, so a copy renamed in the Finder runs the same programs",
        read_from: "`ps` on a Mac running the app; the bundle's `Contents/MacOS` and `Contents/Frameworks` listed, and `CFBundleExecutable` read with `plutil`, on 3 October 2026",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::desktop's EXCLUDED, the check that the app is closed, which finds a renamed copy by these programs, and the lock check, which counts only a process running `Claude`",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_safe_storage_item",
        fact: "the cookie key is the keychain item `Claude Safe Storage`, whose `cdat` equals its `mdat` from when the app was installed",
        read_from: "`security find-generic-password` without `-w`",
        verified_against: VERIFIED_AGAINST,
        depends: "the stamp live usage compares before reading the key",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_keychain_needs_gui",
        fact: "reading the key from a shell that cannot show macOS's question exits 36 until Pitboard has been answered Always Allow; after that the same read succeeds without asking",
        read_from: "`security find-generic-password -w` from a background shell of a Claude Code session on 3 October 2026, not over ssh; the second half from experiment U-K1b on 4 October 2026, the same shell reading the key after Always Allow",
        verified_against: VERIFIED_AGAINST,
        depends: "live usage's `no_gui` reason, and its refreshes with nobody at the Mac",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_usage_history_shape",
        fact: "`plan-usage-history.json` is `{version: 2, samples: [{t, org, u: {fh, sd}}]}`",
        read_from: "the file in the data folder",
        verified_against: VERIFIED_AGAINST,
        depends: "the usage read from the app's own history",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_config_account_keys",
        fact: "`oauth:tokenCache`, `oauth:tokenCacheV2` and `lastKnownAccountUuid` in `config.json` belong to the account, and these three, with the items on the list, are enough to move it whole",
        // Enough, not all: nobody went through the file's other keys one by one, and some,
        // such as `dxt:allowlistCache:<org>`, are named for an organisation.
        read_from: "experiments E2 and E9 on 4 October 2026: both token caches are keyed `acct:<lastKnownAccountUuid>|...`, and a switch splicing these three keys, with the items on the list, moved each of two accounts whole",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::desktop's CONFIG_KEYS, which a switch splices",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_cookie_encryption",
        fact: "a cookie is AES-128-CBC with a key from PBKDF2-SHA1 of the item's password, salt `saltysalt`, 1003 rounds, 16 bytes, an IV of sixteen 0x20 bytes, and from meta version 24 the plaintext starts with the 32-byte SHA-256 of the cookie's host",
        read_from: "experiment E5 on 4 October 2026: a `sessionKey` decrypted this way was accepted by claude.ai",
        verified_against: VERIFIED_AGAINST,
        depends: "the cookie decryption live usage needs",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_usage_history_meaning",
        fact: "`fh` is the five-hour window's percentage used and `sd` the seven-day window's",
        read_from: "experiment E10 on 4 October 2026: the history matched claude.ai's answer for two accounts, 0% and 1% against 0% and 1%, and 30% and 5% against 30% and 5%",
        verified_against: VERIFIED_AGAINST,
        depends: "whether usage read from the app's own history is marked verified",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_usage_endpoint",
        fact: "`GET claude.ai/api/organizations/<org>/usage` with the `sessionKey` cookie answers what is left, with the organisation from `lastActiveOrg`",
        read_from: "experiment E10 on 4 October 2026: 200, with `five_hour` and `seven_day` each carrying `utilization` and `resets_at`",
        verified_against: VERIFIED_AGAINST,
        depends: "live usage's request to claude.ai",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_signout_revokes",
        fact: "signing out in the app revokes `sessionKey` at claude.ai, so a copy of the session taken before cannot be used again",
        read_from: "experiment E1 on 4 October 2026: a byte copy of the listed items and config keys, taken before Log out and put back after, was refused; within 5 seconds of launch the app deleted `sessionKey`, emptied `oauth:tokenCacheV2` and showed its sign-in screen",
        verified_against: VERIFIED_AGAINST,
        depends: "whether a park of a signed-out account can ever be used again, and the advice never to use Log out to switch",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_signout_keeps_uuid",
        fact: "after Log out `lastKnownAccountUuid` still names the account that left, while the `sessionKey` row is gone and `oauth:tokenCacheV2` holds the same 28-character empty value as `oauth:tokenCache`",
        read_from: "experiment E1b on 4 October 2026, read from the data folder after Log out",
        verified_against: VERIFIED_AGAINST,
        depends: "identify_tree, which reads a jar with no `sessionKey` as signed out whatever the uuid says",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_allowlist_complete",
        fact: "the items on provider::desktop's list are enough to move one account whole, and an item on it may be absent, as `File System` was for one account",
        read_from: "experiment E2 on 4 October 2026: after a switch each way, the name, conversations, Code tab, Cowork and connectors were the right account's, with nothing of the other",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::desktop's ITEMS, and every switch",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_fingerprint_stable",
        fact: "the `sessionKey` ciphertext stays the same across opening and quitting the app",
        read_from: "experiment E3 on 4 October 2026: the fingerprints of two accounts did not change across a quit and an open; only the analytics cookie `_dd_s_v2` did",
        verified_against: VERIFIED_AGAINST,
        depends: "telling an enrolled account's login from a new one by its fingerprint",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_uuid_written_at_signin",
        fact: "where `config.json` names no account, signing in writes the account's uuid to `lastKnownAccountUuid` before the new `sessionKey` reaches the jar",
        read_from: "experiment E4 on 4 October 2026: on two sign-ins, each starting with no `lastKnownAccountUuid`, the uuid was written 35 and 15 seconds after launch, and `sessionKey` reached disk at 63 and 31 seconds; the jar is flushed about every 30 seconds",
        verified_against: VERIFIED_AGAINST,
        depends: "identify_tree, which reads a session with no uuid beside it as a sign-in still finishing (`desktop_sign_in_incomplete`)",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_uuid_tracks_signin",
        fact: "where `lastKnownAccountUuid` still names an account that left, as Log out leaves it, another account signing in replaces it before its `sessionKey` reaches the jar",
        // E4's sign-ins both started with no uuid at all, so they say nothing of a uuid
        // being replaced.
        read_from: "not measured yet; experiment E19 settles it: Log out of one account in the app, sign in to another, and read the uuid until the new `sessionKey` reaches the jar",
        verified_against: UNVERIFIED,
        depends: "whether a session Pitboard has not seen, under a uuid it knows, is taken as that account; until then it is refused with `desktop_identity_unconfirmed`, and enrolling that label confirms it",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_safe_storage_account",
        fact: "the key's keychain account is `Claude Key`, and after Always Allow a background shell that could not ask before reads it without asking",
        read_from: "experiment E5 on 4 October 2026, the item's attributes read without `-w`; the second half from experiment U-K1b",
        verified_against: VERIFIED_AGAINST,
        depends: "how live usage names the keychain item it reads",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_writes_on_quit",
        fact: "the app writes `Cookies` and `config.json` as it quits, with Local Storage, Session Storage, WebStorage and Preferences",
        read_from: "experiment E8 on 4 October 2026: each had the mtime of the second the main process exited",
        verified_against: VERIFIED_AGAINST,
        depends: "how long a switch waits after the app has quit",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_token_cache_purpose",
        fact: "`oauth:tokenCache` and `oauth:tokenCacheV2` belong to the web account signed in",
        read_from: "experiment E9 on 4 October 2026: decrypted, each is keyed `acct:<lastKnownAccountUuid>|...:<org>:https://api.anthropic.com:<scopes>` and holds `token`, `refreshToken`, `expiresAt`, `subscriptionType` and `rateLimitTier`; 1668 and 1732 characters for one account, 28 and 1500 for the other",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::desktop's CONFIG_KEYS",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_session_sliding",
        fact: "the session cookie's expiry slides forward while it is used",
        // Both sessions read on 4 October 2026 expire on 1 November 2026; the read that
        // settles this has to come after that.
        read_from: "not measured yet; experiment E11/E13 settles it",
        verified_against: UNVERIFIED,
        depends: "when a parked login is said to expire, and whether a park can be renewed",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_relaunches_itself",
        fact: "the app does not open itself again after it has been quit",
        read_from: "experiment E12 on 4 October 2026: nothing ran from the bundle 0, 2, 5 and 30 seconds after a quit but Chrome's `chrome-native-host`",
        verified_against: VERIFIED_AGAINST,
        depends: "the check that the app is closed before anything is moved",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_singleton_lock",
        fact: "the app keeps no `SingletonLock`, in its data folder or in `$TMPDIR`, while it runs; its main process holds the `LOCK` files of its leveldb stores instead",
        read_from: "experiment E14 on 4 October 2026: the data folder and `$TMPDIR` listed, and `lsof` on the main process",
        verified_against: VERIFIED_AGAINST,
        depends: "the lock check, which finds no lock and leaves the process scan as the only sign the app is open",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_web_user_agent",
        fact: "claude.ai answers a request whose user agent is `pitboard/<version>`",
        read_from: "experiment E15 on 4 October 2026: 200, with no bot check",
        verified_against: VERIFIED_AGAINST,
        depends: "live usage's request to claude.ai",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_email_endpoint",
        fact: "`GET claude.ai/api/account` with the `sessionKey` cookie answers the signed-in account's `email_address`, with its `uuid`, `full_name`, `display_name` and `memberships`",
        read_from: "experiment E17 on 4 October 2026: 200",
        verified_against: VERIFIED_AGAINST,
        depends: "nothing yet: an enrolled Claude Desktop account shows no email, because Pitboard does not ask",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_crashpad_exits_on_quit",
        fact: "`chrome_crashpad_handler` exits within five seconds of the app quitting",
        read_from: "experiment E18 on 4 October 2026: no crashpad handler or helper was left when the main process exited",
        verified_against: VERIFIED_AGAINST,
        depends: "the wait after asking the app to quit, and provider::desktop's EXCLUDED",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "security_reads_do_not_slow_claude",
        fact: "letting Pitboard read the key with Always Allow does not slow the app down",
        // U-K1 ran on 4 October 2026 and read only the item's stamp, not the app's speed.
        read_from: "not measured yet; experiment U-K6 settles it: time the app's launch and a conversation's first answer with live usage off, then on after Always Allow",
        verified_against: UNVERIFIED,
        depends: "whether live usage is offered at all",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "security_exit_codes",
        fact: "`security` exits 128 when the person chooses Deny, and 128 as well when a wrong password is typed and Allow chosen; no answer was seen to give 51",
        read_from: "experiment U-K2 on 4 October 2026, on a keychain item made for it",
        verified_against: VERIFIED_AGAINST,
        depends: "live usage's `denied` reason, which covers a wrong password too; `auth_failed` stays for exit 51",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "security_kill_leaves_prompt",
        fact: "killing `security` with SIGTERM (exit 143) leaves the keychain question it opened on screen",
        read_from: "experiment U-K3 on 4 October 2026, on a keychain item made for it",
        verified_against: VERIFIED_AGAINST,
        depends: "the timeout on reading the key, after which a refresh does not ask again for the same stamp; `enable` always asks, and two processes reading at once each ask, so each of those can leave a question behind",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_update_keeps_item",
        fact: "updating the app keeps the `Claude Safe Storage` item rather than making it again",
        read_from: "not measured yet; experiment U-K4 settles it",
        verified_against: UNVERIFIED,
        depends: "whether live usage has to be approved again after an update",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "security_attributes_never_prompt",
        fact: "reading the key's attributes without `-w` never prompts",
        read_from: "experiment U-K5 on 4 October 2026: read many times from a background shell, with no question",
        verified_against: VERIFIED_AGAINST,
        depends: "the stamp live usage reads before it reads the key",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_always_allow_keeps_mdat",
        fact: "answering Always Allow leaves the key's `mdat` as it was",
        read_from: "experiment U-K1 on 4 October 2026: `cdat` and `mdat` were 20260727084307Z before and after",
        verified_against: VERIFIED_AGAINST,
        depends: "nothing: live usage keeps the stamp it reads after the answer either way",
        probe: &[],
        absent: &[],
    },
];
