// `M` is 2.7032.0's `.vite/build/index.chunk-D3OyLXgG.js`, `P` its `index.pre.js`, `@n` a byte.

use crate::assumptions::OnSystem::{NotRead, Read};
use crate::assumptions::{Assumption, PerSystem};

pub(crate) const VERIFIED_AGAINST: &str = "2.19675.0";
pub(crate) const READ_FROM_THE_ARCHIVE: &str = "2.7032.0";
pub(crate) const READ_FROM_THE_FRAMEWORK: &str = "2.42304.0";

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
        fact: "With a third-party deployment, Claude keeps its data in the data folder's name \
               with `-3p` after it, `Claude-3p`.",
        read_from: "This repository's static read of `app.asar`, 11 October 2026, `P` @965537, \
                    @1030355",
        verified_against: READ_FROM_THE_ARCHIVE,
        depends: "`Folder::refuses` refuses a root with `Claude-3p` beside it",
        probe: &["[custom-3p] userData mkdir failed"],
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
        depends: "`Desktop::items`: a leave and an arrive move exactly these",
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
        depends: "`Desktop::keys`, `naming_key`: the splice moves exactly these three",
        probe: &[
            "lastKnownAccountUuid",
            "oauth:tokenCacheV2",
            "oauth:tokenCache",
        ],
        absent: &[],
    },
    Assumption {
        name: "desktop_first_level_entries",
        fact: "The data folder's first level holds the names `desktop::layout` lists, each the \
               account's, staying or passing: those a listing under 2.7032.0 held, the items \
               E2 moved, `Claude Extensions`, `Claude Extensions Settings` and \
               `extensions-installations.json`, which 2.7032.0's code makes for installed \
               extensions, and the GPU caches of `desktop_gpu_caches`. Every other name its \
               code makes is unread.",
        read_from: "A listing of a data folder under 2.7032.0, names only, 11 October 2026; the \
                    contributor's experiment E2, 4 October 2026; the three extension names read \
                    in `app.asar`, 11 October 2026, `P` @965893",
        verified_against: READ_FROM_THE_ARCHIVE,
        depends: "`Desktop::classify`: a name it does not classify stops the start of a move",
        probe: &[
            "config.json.journal",
            "Claude Extensions Settings",
            "extensions-installations.json",
        ],
        absent: &[],
    },
    Assumption {
        name: "desktop_gpu_caches",
        fact: "From Electron 44.4.4, Claude keeps Chromium's GPU caches `ShaderCache`, \
               `GrShaderCache`, `GraphiteDawnCache` and `GPUPersistentCache` in its data \
               folder, beside `GPUCache`. They hold compiled shaders, nothing of an account.",
        read_from: "Electron's pull request 54161, released in 44.4.4, which gives \
                    `ElectronBrowserClient` the four folders under `sessionData`; Chromium's \
                    `GpuProcessHost` and `PersistentCacheSandboxedFileFactory` for what \
                    `GPUPersistentCache` holds; the four names side by side in 2.42304.0's \
                    `Electron Framework`, Electron 44.7.2 on Chromium 152.0.7977.149, 19 \
                    October 2026, and neither `GraphiteDawnCache` nor `GPUPersistentCache` in \
                    2.7032.0's, Electron 44.4.3; a person's report of a data folder holding \
                    `GPUPersistentCache`, 19 October 2026",
        verified_against: READ_FROM_THE_FRAMEWORK,
        depends: "`Desktop::classify` gives each `Stays`",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "desktop_kept_by_account",
        fact: "`claude-code-sessions/<account>/<org>` and `local-agent-mode-sessions/<account>/\
               <org>` carry the account in their path, `spaces-present` is keyed by eight hex \
               digits of it, `fcache` is dropped for another organisation, and \
               `cowork-enabled-cli-ops.json` carries its owner, so each stays in the data \
               folder.",
        read_from: "This repository's static read of `app.asar`, 11 October 2026, `M` @5333662, \
                    @2776214, @2758122, @2209405, @5647316",
        verified_against: READ_FROM_THE_ARCHIVE,
        depends: "`Desktop::classify` gives each `Stays`",
        probe: &[
            "claude-code-sessions",
            "local-agent-mode-sessions",
            "spaces-present",
            "ownerAccountId",
        ],
        absent: &[],
    },
    Assumption {
        name: "desktop_chromium_temporary",
        fact: "Chromium writes a file through a temporary beside it named `.<base bundle \
               id>.<file>.XXXXXX`, and Electron makes Claude's base bundle id its \
               `ElectronTeamID` and bundle id: `.Q6L2SF6YDW.com.anthropic.claudefordesktop.\
               <file>.` and six letters or digits.",
        read_from: "Electron v44.4.3's `shell/app/electron_main_delegate_mac.mm` \
                    (`SetUpBundleOverrides`) and Chromium 152.0.7977.130's \
                    `base/files/file_util_posix.cc` (`FormatTemporaryFileName`) and \
                    `base/files/important_file_writer.cc`, the versions 2.7032.0's framework \
                    names; `ElectronTeamID` and `CFBundleIdentifier` from \
                    `/Applications/Claude.app/Contents/Info.plist` of 2.7032.0, 13 October \
                    2026. No listing has held one",
        verified_against: READ_FROM_THE_ARCHIVE,
        depends: "`Desktop::classify` passes such a name where `<file>` stays; any other \
                  temporary of Chromium's is unread and stops the start of a move",
        probe: &[],
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
                  Pitboard has not seen beside an emptied cache is a sign-in not finished, \
                  never taken under the name Claude kept",
        probe: &["clearing oauth cache"],
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
        fact: "Claude takes no single-instance lock and writes no `SingletonLock`; its main \
               process holds the `LOCK` files of its leveldb stores.",
        read_from: "The contributor's experiment E14, 4 October 2026, a listing and `lsof`; the \
                    absence read again in 2.7032.0's archive, 11 October 2026",
        verified_against: VERIFIED_AGAINST,
        depends: "the running check is the process list; a `Singleton*` name in the folder is \
                  unread and stops a move",
        probe: &[],
        absent: &["requestSingleInstanceLock"],
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
    on_a_mac("desktop_third_party_folder", READ_FROM_THE_ARCHIVE),
    on_a_mac("desktop_account_items", VERIFIED_AGAINST),
    on_a_mac("desktop_account_keys", VERIFIED_AGAINST),
    on_a_mac("desktop_first_level_entries", READ_FROM_THE_ARCHIVE),
    on_a_mac("desktop_gpu_caches", READ_FROM_THE_FRAMEWORK),
    on_a_mac("desktop_kept_by_account", READ_FROM_THE_ARCHIVE),
    on_a_mac("desktop_chromium_temporary", READ_FROM_THE_ARCHIVE),
    on_a_mac("desktop_cookie_jar", VERIFIED_AGAINST),
    on_a_mac("desktop_cookie_host_hash", READ_FROM_THE_ARCHIVE),
    on_a_mac("desktop_session_cookie_names", READ_FROM_THE_ARCHIVE),
    on_a_mac("desktop_session_same_across_quit", VERIFIED_AGAINST),
    on_a_mac("desktop_session_replaced_in_place", VERIFIED_AGAINST),
    on_a_mac("desktop_logout_revokes", VERIFIED_AGAINST),
    on_a_mac("desktop_logout_keeps_name", VERIFIED_AGAINST),
    on_a_mac("desktop_name_after_sign_in", VERIFIED_AGAINST),
    on_a_mac("desktop_name_written_on_report", READ_FROM_THE_ARCHIVE),
    on_a_mac("desktop_writes_at_quit", VERIFIED_AGAINST),
    on_a_mac("desktop_no_singleton_lock", VERIFIED_AGAINST),
    on_a_mac("desktop_helpers_exit_with_main", VERIFIED_AGAINST),
    on_a_mac("desktop_stays_quit", VERIFIED_AGAINST),
    on_a_mac("desktop_quit_can_wait", READ_FROM_THE_ARCHIVE),
    on_a_mac("desktop_settings_journal", READ_FROM_THE_ARCHIVE),
    on_a_mac("desktop_settings_reset", READ_FROM_THE_ARCHIVE),
    on_a_mac("desktop_running_instance_record", READ_FROM_THE_ARCHIVE),
    on_a_mac("desktop_own_switcher", READ_FROM_THE_ARCHIVE),
];
