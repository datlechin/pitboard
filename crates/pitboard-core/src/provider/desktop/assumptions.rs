// `M` is 2.7032.0's `.vite/build/index.chunk-D3OyLXgG.js`, `P` its `index.pre.js`, `@n` a byte.
// `N` is 2.42304.0's `.vite/build/index.chunk-muI2IV6k.js`; its other chunks go by their hash.
// `R` is 3.6556.2's `.vite/build/index.pre.js`, and `Q` its `index.chunk-jPKPzV-P.js`.
// `S` is 4.29.0's `.vite/build/index.chunk-C7NkbHFq.js`.

use crate::assumptions::OnSystem::{NotRead, Read};
use crate::assumptions::{Assumption, PerSystem};

pub(crate) const VERIFIED_AGAINST: &str = "2.19675.0";
pub(crate) const READ_FROM_THE_ARCHIVE: &str = "2.7032.0";
pub(crate) const READ_FROM_THE_NEWER_BUNDLE: &str = "2.42304.0";
pub(crate) const READ_FROM_CLAUDE_3: &str = "3.6556.2";

const NOT_HERE: &str =
    "Pitboard switches Claude Desktop accounts on macOS only, and nothing of it was read here";

const fn on_a_mac(name: &'static str, build: &'static str) -> PerSystem {
    PerSystem {
        name,
        macos: Read(build),
        linux: NotRead(NOT_HERE),
        windows: NotRead(NOT_HERE),
    }
}

pub(crate) const ASSUMPTIONS: &[Assumption] = &[
    Assumption {
        name: "desktop_app",
        fact: "Claude Desktop is `com.anthropic.claudefordesktop`. Its main program is \
               `Contents/MacOS/Claude`; `Claude Helper`, `Claude Helper (GPU)`, `Claude Helper \
               (Plugin)`, `Claude Helper (Renderer)` and `chrome_crashpad_handler` run from \
               inside its bundle.",
        read_from: "The contributor's listing of the bundle and `ps -x -o pid=,ppid=,comm=` with \
                    Claude open, 3 October 2026; the bundle id read again from 2.7032.0's \
                    `Info.plist`, 11 October 2026, and with `CFBundleExecutable` `Claude` from \
                    `/Applications/Claude.app/Contents/Info.plist` of 2.7032.0, 13 October 2026",
        verified_against: VERIFIED_AGAINST,
        depends: "`desktop::running` counts every program inside the bundle with this id, \
                  whatever the bundle is named; where a bundle's id does not read, every \
                  program inside a `Claude.app`, and the main program and `Claude Helper` \
                  apps at these paths in any bundle; `desktop::HOLDER` names the bundle id the \
                  app asks to quit",
        probe: &["com.anthropic.claudefordesktop"],
        absent: &[],
    },
    Assumption {
        name: "desktop_native_host_is_chromes",
        fact: "`Contents/Helpers/chrome-native-host` runs as Chrome's child, for Claude's \
               browser extension, not as Claude's.",
        read_from: "The contributor's `ps` sample with Chrome and Claude open, 3 October 2026, \
                    kept as the test fixture of `desktop::running`",
        verified_against: VERIFIED_AGAINST,
        depends: "`is_the_app` counts `chrome-native-host` only where its parent runs from \
                  inside Claude's bundle",
        probe: &["chrome-native-host"],
        absent: &[],
    },
    Assumption {
        name: "desktop_data_folder",
        fact: "Claude keeps its data in `Library/Application Support/Claude` under the home the \
               passwd entry names, as `os.userInfo().homedir`; a packaged build deletes \
               `CLAUDE_USER_DATA_DIR` from its environment.",
        read_from: "This repository's static read of `app.asar`, 11 October 2026, `P` @1028993, \
                    @1029336",
        verified_against: READ_FROM_THE_ARCHIVE,
        depends: "`desktop::paths::root` takes the folder from the passwd home, and leaves the \
                  tool absent where `HOME` names another folder",
        probe: &["CLAUDE_USER_DATA_DIR", "userData relocated"],
        absent: &[],
    },
    Assumption {
        name: "desktop_third_party_folder",
        fact: "Claude runs from `Claude-3p`, the data folder's name with `-3p` after it, and keeps \
               its settings there, once its managed configuration turns on a third-party \
               deployment. The person can then pick claude.ai in its chooser, which writes \
               `deploymentMode` `1p` in `Claude-3p/claude_desktop_config.json`, and Claude runs \
               from `Claude` again, unless the configuration turns claude.ai sign-in off. A \
               `Claude-3p` can hold that file alone with no deployment, as an earlier build \
               left it.",
        read_from: "This repository's static read of `app.asar`, 11 October 2026, `P` @965537, \
                    @1030355; of 3.6556.2's, 23 October 2026, `R` @877849 to @878100 for the \
                    chooser, @975562 for the data folder, `Q` @544813 for the settings; a \
                    person's report of a `Claude-3p` holding only that file, on a Mac with no \
                    deployment, issue 271, 21 October 2026",
        verified_against: READ_FROM_CLAUDE_3,
        depends: "`Folder::refuses` refuses a root whose `Claude-3p` holds `config.json`, unless \
                  its `claude_desktop_config.json` names `deploymentMode` `1p`; a configuration \
                  that turns claude.ai sign-in off is not read, since no account then signs in \
                  to switch",
        probe: &[
            "[custom-3p] userData mkdir failed",
            "[custom-3p] deploymentMode written",
        ],
        absent: &[],
    },
    Assumption {
        name: "desktop_account_items",
        fact: "`Cookies`, `Cookies-journal`, `Local Storage`, `Session Storage`, \
               `IndexedDB/https_claude.ai_0.indexeddb.leveldb`, \
               `IndexedDB/https_claude.ai_0.indexeddb.blob`, `WebStorage` and `File System`, any \
               of which may be absent, with the three account keys of `config.json`, move an \
               account whole, both ways.",
        read_from: "The contributor's experiment E2, 4 October 2026: two accounts, one switch \
                    each way, name, conversations, Code tab, Cowork and connectors checked by eye",
        verified_against: VERIFIED_AGAINST,
        depends: "`Desktop::items` starts with these, the jar first",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_account_keys",
        fact: "`config.json`'s `oauth:tokenCache`, `oauth:tokenCacheV2` and \
               `lastKnownAccountUuid` belong to the account signed in: the token caches are \
               keyed `acct:<lastKnownAccountUuid>|…` and hold its OAuth tokens.",
        read_from: "The contributor's experiments E2 and E9, 4 October 2026; read again in \
                    `app.asar`, 11 October 2026, `M` @650532, @2938290, @2956446",
        verified_against: VERIFIED_AGAINST,
        depends: "`Desktop::keys`, `naming_key`: the splice moves these three, and \
                  `custom3pMcpOAuth` of `desktop_cleared_for_another_account`",
        probe: &[
            "lastKnownAccountUuid",
            "oauth:tokenCacheV2",
            "oauth:tokenCache",
        ],
        absent: &[],
    },
    Assumption {
        name: "desktop_names_stay",
        fact: "Claude keeps every name in its data folder across another account's sign-in but \
               those `desktop_cleared_for_another_account` names. 2.42304.0, 3.1623.0 and \
               3.6556.2 run the same seventeen named resets on that change, while each build \
               makes names of its own there, such as `design`, Claude Design's window, from \
               3.1623.0.",
        read_from: "This repository's static read of 2.42304.0's `app.asar`, 19 October 2026, \
                    `N` @2937198 to @2942600, and of 3.1623.0's and 3.6556.2's, 23 October \
                    2026, `Q` @3095967 to @3098442, each reset compared by its name and body; \
                    `design` at `Q` @4911399",
        verified_against: READ_FROM_CLAUDE_3,
        depends: "`Desktop::unread_part`: a switch carries `Desktop::items` and leaves every \
                  other name where Claude keeps it",
        probe: &[
            "project-upload-folder-grants",
            "handoff-invalidate",
            "sp-section-prompts",
        ],
        absent: &[],
    },
    Assumption {
        name: "desktop_store_parts",
        fact: "SQLite keeps a database's `-journal`, `-wal` and `-shm` beside it, and Claude's \
               stores keep a `.journal`, each named for the file and part of it. Chromium names \
               claude.ai's IndexedDB stores for its origin, `https_claude.ai_0`. Electron keeps \
               the jar at the data folder's first level, and in `Network` once Chromium moves \
               its network service's files, which on macOS it does only under the \
               `TriggerNetworkDataMigration` feature.",
        read_from: "SQLite's \"Temporary Files Used By SQLite\"; 2.42304.0's `app.asar`, 19 \
                    October 2026, `N` @493566, for the stores' journals; the items E2 moved, \
                    for the IndexedDB stores; Electron v44.9.0's and v44.9.3's \
                    `shell/browser/net/network_context_service.cc`, \
                    `ShouldTriggerNetworkDataMigration`, the versions 3.1623.0 and 3.6556.2 run, \
                    23 October 2026",
        verified_against: READ_FROM_CLAUDE_3,
        depends: "`Desktop::unread_part`: such a part that no item names stops the start of a \
                  move, since the move would split it from its store",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_cleared_for_another_account",
        fact: "When the account that signs in is not `lastKnownAccountUuid`, Claude empties \
               its preview browsers, `launch-preview-static`, `launch-preview-cowork-shared` \
               and one per workspace under `Partitions`, and `chrome-import-sites.json`, and \
               sets `launchChromeImportPrompt` in `claude_desktop_config.json` back to \
               pending. Where a policy sets up its Microsoft 365 connector, it also drops \
               the grants in `config.json`'s `custom3pMcpOAuth` and the sign-ins in \
               `builtin-mcp-auth` and `custom3p-builtin-mcp.json`, and signs out of the \
               system's Microsoft broker, which no switch moves. None of these names its \
               account. Its other resets on that change clear memory, or what names its \
               account or session. A first sign-in with no `lastKnownAccountUuid` runs the \
               preview resets too, and empties `preview-last-frames`.",
        read_from: "This repository's static read of 2.42304.0's `app.asar`, 19 October 2026: \
                    the account remembered at `N` @2937198, the resets @2938000 to @2942600, \
                    a first sign-in @2939820, the preview browsers @4857844, the broker \
                    @1454853, the connectors @2877456 and their files in \
                    a claude.ai sign-in @903626 to @904139, the preferences file @1684323",
        verified_against: READ_FROM_THE_NEWER_BUNDLE,
        depends: "`Desktop::items` and `Desktop::keys` carry each with the login, since a \
                  switch puts the arriving account's `lastKnownAccountUuid` in and Claude \
                  sees no change; `launchChromeImportPrompt` stays with the preferences",
        probe: &[
            "resetAllPreviewIsolationSurfaces",
            "disconnectConnectorsForNewPerson",
            "custom3pMcpOAuth",
            "chrome-import-sites.json",
        ],
        absent: &[],
    },
    Assumption {
        name: "desktop_site_stores",
        fact: "Chromium keeps `Service Worker`, `Platform Notifications`, `CdmStorage.db` and \
               `declarative_performance_observer.db`, the last two with a `-journal`, for \
               each site in the data folder. claude.ai's are the signed-in account's.",
        read_from: "Chromium 152.0.7977.149's `content/browser/storage_partition_impl.cc` and \
                    `content/browser/declarative_performance_observer/\
                    declarative_performance_observer_store.cc`, the version 2.42304.0's \
                    framework names, 19 October 2026; `N` @6788047 grants claude.ai \
                    notifications from a service worker. That claude.ai registers a service \
                    worker or shows a notification was not read",
        verified_against: READ_FROM_THE_NEWER_BUNDLE,
        depends: "`Desktop::items` carries each with the login",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_account_files",
        fact: "`session-popouts.json` holds the open session windows' ids and titles with no \
               account in it, and Claude opens them again at launch. `local-sessions.json`, \
               which only earlier builds write, is taken in by the next account to sign in.",
        read_from: "This repository's static read of 2.42304.0's `app.asar`, 19 October 2026: \
                    `N` @2989501 and @2990962; `migrateLegacySessions` at `CFJssAFf` @1526454 \
                    and `CB6cNSrW` @474151",
        verified_against: READ_FROM_THE_NEWER_BUNDLE,
        depends: "`Desktop::items` carries both with the login",
        probe: &["session-popouts.json", "migrateLegacySessions"],
        absent: &[],
    },
    Assumption {
        name: "desktop_cookie_jar",
        fact: "`Cookies` is SQLite at `meta.version` 24; every value starts `v10`; `sessionKey` \
               and `sessionKeyV3` hold one ciphertext; a session cookie expires about four \
               weeks after it is set; with Claude open, `Cookies-journal` is empty between \
               writes and there is no `Cookies-wal`.",
        read_from: "The contributor's read of a copy of the jar with Claude closed, and two \
                    `stat` readings with it open, 3 October 2026",
        verified_against: VERIFIED_AGAINST,
        depends: "`host/macos/sqlite.rs` and `desktop::jar`: compatibility above 24, a value not \
                  `v10`, a journal not empty refuse",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_cookie_host_hash",
        fact: "At `meta.version` 24 Chromium puts the SHA-256 of a cookie's host before its \
               value and encrypts both, so one session stored for both hosts is two different \
               stored values. Claude's importer of Chrome's cookies checks and strips the same \
               hash.",
        read_from: "Chromium 152.0.7977.130's \
                    `net/extras/sqlite/sqlite_persistent_cookie_store.cc`, its version 24 note \
                    and `DoCommit`, the version 2.7032.0's framework names; this repository's \
                    static read of `app.asar`, 13 October 2026, \
                    `.vite/build/index.chunk-o_ZG2Orq.js` @9530, and @10473, which hands it the \
                    row's `host_key`",
        verified_against: READ_FROM_THE_ARCHIVE,
        depends: "`desktop::jar`: a `sessionKey` for each host refuses whatever the values",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_session_cookie_names",
        fact: "The session cookies are `sessionKey`, `sessionKeyV2`, `sessionKeyV3`, \
               `sessionKeyLC` and `sessionKeyV3LC`, for `claude.ai` and `.claude.ai`.",
        read_from: "This repository's static read of `app.asar`, 11 October 2026, `M` @2929634 \
                    to @2930389",
        verified_against: READ_FROM_THE_ARCHIVE,
        depends: "signed out needs none of the five names",
        probe: &["sessionKeyV3LC", "sessionKeyLC", "sessionKeyV2"],
        absent: &[],
    },
    Assumption {
        name: "desktop_session_same_across_quit",
        fact: "A `sessionKey`'s stored value is the same after a quit and an open.",
        read_from: "The contributor's experiment E3, 4 October 2026: two accounts, one cycle \
                    each",
        verified_against: VERIFIED_AGAINST,
        depends: "a session's fingerprint names its login from one change to the next",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_session_replaced_in_place",
        fact: "Claude can replace an account's session while it stays signed in: a session \
               Pitboard had put back met `session_stale_relogin` one second after launch, and \
               66 seconds later Claude stored a new `sessionKey` under the same \
               `lastKnownAccountUuid`, with no login transition logged.",
        read_from: "The contributor's reading of `main.log` and the jar, 5 October 2026, while \
                    the contributor's build may have been asking claude.ai for usage; the latch \
                    read in `app.asar`, 11 October 2026, `M` @2965245",
        verified_against: VERIFIED_AGAINST,
        depends: "a session not seen, under the recorded name, is taken as Claude names it",
        probe: &["session_stale_relogin"],
        absent: &[],
    },
    Assumption {
        name: "desktop_logout_revokes",
        fact: "Log out in Claude ends the session at claude.ai: a copy of it put back is \
               refused.",
        read_from: "The contributor's experiment E1, 4 October 2026, one account",
        verified_against: VERIFIED_AGAINST,
        depends: "an add parks the account in use and never logs it out; no sentence sends a \
                  person to Log out",
        probe: &["Navigated to /logout"],
        absent: &[],
    },
    Assumption {
        name: "desktop_logout_keeps_name",
        fact: "After Log out, `lastKnownAccountUuid` still names the account that left, the \
               `sessionKey` row is gone, and both token caches hold a 28-character emptied \
               value.",
        read_from: "The contributor's experiment E1b, 4 October 2026; read again in `app.asar`, \
                    11 October 2026, `M` @2977015, @2945841",
        verified_against: VERIFIED_AGAINST,
        depends: "`Desktop::emptied`; signed out with the name kept is certain; a session \
                  Pitboard has not seen beside caches that all hold the emptied value is a \
                  sign-in not finished, never taken under the name Claude kept",
        probe: &["clearing oauth cache"],
        absent: &[],
    },
    Assumption {
        name: "desktop_token_caches_on_demand",
        fact: "Claude writes a token cache only as something asks it for a token: \
               `oauth:tokenCache` for its telemetry and the API token of its own deployment, \
               `oauth:tokenCacheV2` for the Code tab's settings and credential. A sign-in writes \
               the account to `lastKnownAccountUuid` before either cache takes a token for it, \
               and empties both unless it brings back the account Claude last named or, behind \
               a feature gate, one its account switcher holds. So one cache can stay emptied \
               while the other holds the account's tokens.",
        read_from: "This repository's static read of 2.7032.0's, 3.6556.2's and 4.29.0's \
                    `app.asar`, 28 October 2026: the caches `M` @2938294 and @2956450, `Q` \
                    @3054888 and @3074896, `S` @2993414 and @3013803; the telemetry token `M` \
                    @2989080, `Q` @3110636, `S` @3048100; the deployment's token `M` @2918671, \
                    `Q` @3033590, `S` @2971717; the Code tab's `M` @6000088, `Q` @6260565, `S` \
                    @6333736; the sign-in `M` @2974715, `Q` @3095957, `S` @3035027, and its \
                    name `M` @2976997, `Q` @3098664, `S` @3037755. Issue #312: \
                    3.6556.2 signed in, its `oauth:tokenCache` 28 characters and \
                    `oauth:tokenCacheV2` 2244",
        verified_against: READ_FROM_CLAUDE_3,
        depends: "`switch::folder::verdict::holds`: a sign-in is unfinished only while every \
                  token cache there holds the emptied value",
        probe: &[
            "[EventLogging] Telemetry token obtained",
            "[ccdCredential] base-scope token unavailable",
        ],
        absent: &[],
    },
    Assumption {
        name: "desktop_name_after_sign_in",
        fact: "On a sign-in with no name recorded, Claude wrote `lastKnownAccountUuid` 15 to 35 \
               seconds after launch, and the session reached the jar on disk 31 to 63 seconds \
               after it; the jar is written about every 30 seconds.",
        read_from: "The contributor's experiment E4, 4 October 2026, two sign-ins",
        verified_against: VERIFIED_AGAINST,
        depends: "a session with no name is a sign-in not finished",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_name_written_on_report",
        fact: "Claude writes `lastKnownAccountUuid` whenever the account its page reports \
               differs from the one it holds; a report of nobody signed in carries none.",
        read_from: "This repository's static read of `app.asar`, 11 October 2026, `M` @2977015, \
                    @2972079",
        verified_against: READ_FROM_THE_ARCHIVE,
        depends: "the settings' name is Claude's word, taken in one case only",
        probe: &["Login-state transition"],
        absent: &[],
    },
    Assumption {
        name: "desktop_writes_at_quit",
        fact: "As it quits, Claude writes `Cookies`, `config.json`, `Local Storage`, \
               `Session Storage`, `WebStorage` and `Preferences`.",
        read_from: "The contributor's experiment E8, 4 October 2026, modification times to the \
                    second; read again in `app.asar`, 11 October 2026, `M` @3021507",
        verified_against: VERIFIED_AGAINST,
        depends: "the jar is read only after Claude has quit",
        probe: &["flush-web-storage"],
        absent: &[],
    },
    Assumption {
        name: "desktop_no_singleton_lock",
        fact: "Claude takes its single-instance lock only while it runs Reset App Data, and \
               otherwise writes no `SingletonLock`; its main process holds the `LOCK` files of \
               its leveldb stores.",
        read_from: "The contributor's experiment E14, 4 October 2026, a listing and `lsof`; the \
                    absence read again in 2.7032.0's archive, 11 October 2026; 3.6556.2's \
                    `app.asar`, 23 October 2026, `R` @960795, where a start that finds no \
                    reset asked for returns before the lock at @961471",
        verified_against: READ_FROM_CLAUDE_3,
        depends: "the running check is the process list",
        probe: &["data folder's single-instance lock"],
        absent: &[],
    },
    Assumption {
        name: "desktop_reset_app_data",
        fact: "From 3.1623.0, Reset App Data writes `reset-app-data` in the data folder and \
               opens Claude again, which deletes what the folder holds before it shows a \
               window, and removes the file once done. Beside `reset-app-data.given-up`, \
               Claude has given the reset up and deletes nothing.",
        read_from: "This repository's static read of 3.1623.0's and 3.6556.2's `app.asar`, 23 \
                    October 2026: the names at `R` @850745, the start that deletes at @960795",
        verified_against: READ_FROM_CLAUDE_3,
        depends: "`Folder::refuses` refuses a data folder holding `reset-app-data` and no \
                  `reset-app-data.given-up`: a login moved in would be deleted",
        probe: &["reset-app-data.given-up"],
        absent: &[],
    },
    Assumption {
        name: "desktop_helpers_exit_with_main",
        fact: "The helpers and `chrome_crashpad_handler` are gone once the main program has \
               exited.",
        read_from: "The contributor's experiment E18, 4 October 2026",
        verified_against: VERIFIED_AGAINST,
        depends: "the app's quit waits for the core's holders inside its 30 seconds",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_stays_quit",
        fact: "Claude does not open itself again after a quit, checked at 0, 2, 5 and 30 \
               seconds.",
        read_from: "The contributor's experiment E12, 4 October 2026, one quit",
        verified_against: VERIFIED_AGAINST,
        depends: "a quit lasts through a move; the look after the last write still guards",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_quit_can_wait",
        fact: "A quit can be refused by Claude's own question while Code or Cowork sessions \
               run, and otherwise takes up to 75 + 5 + 15 seconds.",
        read_from: "This repository's static read of `app.asar`, 11 October 2026, `M` @2330101, \
                    @3023650, @2790204",
        verified_against: READ_FROM_THE_ARCHIVE,
        depends: "the app gives a quit 30 seconds and changes nothing after; the core never \
                  waits for a quit",
        probe: &["vetoed by before-quit interceptor", "reached maxtimeout"],
        absent: &[],
    },
    Assumption {
        name: "desktop_settings_journal",
        fact: "A write of `config.json` goes first to `config.json.journal`: a line of three \
               fields, the SHA-256 of the body, of `config.json` on disk (`-` where absent) and \
               of the write under way (`.` where none), then the body. At start Claude writes a \
               journal's body in where the body matches field 1 and the file field 2 or 3, and \
               drops it otherwise.",
        read_from: "This repository's static read of `app.asar`, 11 October 2026, `M` @615321, \
                    @615852",
        verified_against: READ_FROM_THE_ARCHIVE,
        depends: "`Desktop::settings` reads a valid sidecar as the settings and ignores a stale \
                  one; a header it cannot read refuses every change",
        probe: &[
            "torn journal",
            "journal was not written over this store file",
        ],
        absent: &[],
    },
    Assumption {
        name: "desktop_settings_reset",
        fact: "A `config.json` that does not parse is reset to defaults, or renamed \
               `config.json.corrupt-<milliseconds>` and reset.",
        read_from: "This repository's static read of `app.asar`, 11 October 2026, `M` @652152",
        verified_against: READ_FROM_THE_ARCHIVE,
        depends: "Pitboard never writes a settings file it could not parse; \
                  `config.json.corrupt-<digits>` passes",
        probe: &["Config file corrupted, resetting to defaults"],
        absent: &[],
    },
    Assumption {
        name: "desktop_running_instance_record",
        fact: "Claude records itself running as `updaterRunningInstance` in `config.json` and \
               removes it through the journal as it quits. One left behind marks the last exit \
               unclean and arms an install of a staged update within 30 minutes by default.",
        read_from: "This repository's static read of `app.asar`, 11 October 2026, `M` @3009016, \
                    @3009548, @4141231, @4144680",
        verified_against: READ_FROM_THE_ARCHIVE,
        depends: "Pitboard writes a valid sidecar's body in before it splices, so its own write \
                  never leaves the record behind",
        probe: &[
            "updaterRunningInstance",
            "Previous run did not exit cleanly",
        ],
        absent: &[],
    },
    Assumption {
        name: "desktop_own_switcher",
        fact: "An account switcher of Claude's own sits behind server gate `96101707` and the \
               policy `disableMultiAccount`; with it on, one jar holds several accounts and \
               `lastKnownAccountUuid` names the active one.",
        read_from: "This repository's static read of `app.asar`, 11 October 2026, `M` @2969996, \
                    @2970037, `P` @984528",
        verified_against: READ_FROM_THE_ARCHIVE,
        depends: "two rows of one session name for one host, or a `sessionKey` for each host, \
                  refuse; the recorded session under another name is doubt",
        probe: &["disableMultiAccount", "96101707"],
        absent: &[],
    },
];

pub(crate) const PER_SYSTEM: &[PerSystem] = &[
    on_a_mac("desktop_app", VERIFIED_AGAINST),
    on_a_mac("desktop_native_host_is_chromes", VERIFIED_AGAINST),
    on_a_mac("desktop_data_folder", READ_FROM_THE_ARCHIVE),
    on_a_mac("desktop_third_party_folder", READ_FROM_CLAUDE_3),
    on_a_mac("desktop_account_items", VERIFIED_AGAINST),
    on_a_mac("desktop_account_keys", VERIFIED_AGAINST),
    on_a_mac("desktop_names_stay", READ_FROM_CLAUDE_3),
    on_a_mac("desktop_store_parts", READ_FROM_CLAUDE_3),
    on_a_mac(
        "desktop_cleared_for_another_account",
        READ_FROM_THE_NEWER_BUNDLE,
    ),
    on_a_mac("desktop_site_stores", READ_FROM_THE_NEWER_BUNDLE),
    on_a_mac("desktop_account_files", READ_FROM_THE_NEWER_BUNDLE),
    on_a_mac("desktop_cookie_jar", VERIFIED_AGAINST),
    on_a_mac("desktop_cookie_host_hash", READ_FROM_THE_ARCHIVE),
    on_a_mac("desktop_session_cookie_names", READ_FROM_THE_ARCHIVE),
    on_a_mac("desktop_session_same_across_quit", VERIFIED_AGAINST),
    on_a_mac("desktop_session_replaced_in_place", VERIFIED_AGAINST),
    on_a_mac("desktop_logout_revokes", VERIFIED_AGAINST),
    on_a_mac("desktop_logout_keeps_name", VERIFIED_AGAINST),
    on_a_mac("desktop_token_caches_on_demand", READ_FROM_CLAUDE_3),
    on_a_mac("desktop_name_after_sign_in", VERIFIED_AGAINST),
    on_a_mac("desktop_name_written_on_report", READ_FROM_THE_ARCHIVE),
    on_a_mac("desktop_writes_at_quit", VERIFIED_AGAINST),
    on_a_mac("desktop_no_singleton_lock", READ_FROM_CLAUDE_3),
    on_a_mac("desktop_reset_app_data", READ_FROM_CLAUDE_3),
    on_a_mac("desktop_helpers_exit_with_main", VERIFIED_AGAINST),
    on_a_mac("desktop_stays_quit", VERIFIED_AGAINST),
    on_a_mac("desktop_quit_can_wait", READ_FROM_THE_ARCHIVE),
    on_a_mac("desktop_settings_journal", READ_FROM_THE_ARCHIVE),
    on_a_mac("desktop_settings_reset", READ_FROM_THE_ARCHIVE),
    on_a_mac("desktop_running_instance_record", READ_FROM_THE_ARCHIVE),
    on_a_mac("desktop_own_switcher", READ_FROM_THE_ARCHIVE),
];
