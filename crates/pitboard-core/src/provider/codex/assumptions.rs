//! Every fact about Codex CLI that Pitboard stands on, named and dated.
//!
//! Read from codex-cli 0.154.0, unless an entry names another build: the binary installed
//! on the machine this was written on, the matching public source at tag `rust-v0.154.0`,
//! and the real `auth.json` that build had written. What runs Codex beside the CLI, the
//! ChatGPT app and the background app server, is read from the builds its entries name.
//!
//! See [`crate::assumptions`] for what an entry means and how a probe reads one.

use crate::assumptions::OnSystem::{NotRead, Pending, Read};
use crate::assumptions::{Assumption, PerSystem};

/// The build every entry below was read from on macOS and Linux, unless it names its own.
pub(crate) const VERIFIED_AGAINST: &str = "0.154.0";

// Both Windows builds of 0.163.0, read as bytes on 2026-10-20 with the source at `rust-v0.163.0`
// against the `rust-v0.160.0` and `rust-v0.162.1` sources each reading named before.
pub(crate) const WINDOWS_VERIFIED_AGAINST: &str = "0.163.0";

/// A fact read on macOS and Linux from the build its entry names, and on Windows from
/// [`WINDOWS_VERIFIED_AGAINST`]: the code each is about has no part that differs on
/// Windows at that tag, outside its tests, and both Windows builds hold every literal the
/// entry probes for.
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

/// What each fact below is on each system. On macOS and Linux every fact is read, from the
/// build its entry names, as this register said of every fact before it had a table, but the
/// managed preferences, which only macOS has. The macOS readings are of the codex installed
/// on a Mac, or of the builds an entry names, and the conformance run reads the facts from
/// the macOS, Linux and Windows builds alike.
///
/// Three facts were read on a Mac alone: the ChatGPT app's codex, the app server's parent
/// and the keychain stores. They stay read on Linux, since the code that stands on them,
/// `holders` and the refusal of a keychain store, runs there as it does on macOS. A Linux
/// reading of each of its own is no part of the Windows work.
pub(crate) const PER_SYSTEM: &[PerSystem] = &[
    PerSystem {
        name: "codex_login_location",
        macos: Read("0.160.0"),
        linux: Read("0.160.0"),
        windows: NotRead(
            "Codex sets no mode on Windows and keeps a keyring store's login in the secret store \
             by default, which codex_windows_login_location reads",
        ),
    },
    PerSystem {
        name: "codex_windows_login_location",
        macos: NotRead("where macOS's login is, is codex_login_location's"),
        linux: NotRead("where Linux's login is, is codex_login_location's"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    everywhere("codex_login_shape", VERIFIED_AGAINST),
    everywhere("codex_identity_is_local", VERIFIED_AGAINST),
    everywhere("codex_renewal", VERIFIED_AGAINST),
    everywhere("codex_usage_endpoint", VERIFIED_AGAINST),
    everywhere("codex_revokes_on_its_own_sign_out", VERIFIED_AGAINST),
    everywhere("codex_never_follows_a_switch", VERIFIED_AGAINST),
    later_on_windows(
        "codex_runs_inside_the_chatgpt_app",
        "26.928.31416",
        &["W27"],
        "whether the app codex_windows_app_identity names runs its own codex inside its \
         package, and whether closing its windows leaves it running, session step 10",
    ),
    PerSystem {
        name: "codex_app_server_daemon",
        macos: Read("0.159.3"),
        linux: Read("0.159.3"),
        windows: NotRead(
            "where the app server runs from on Windows, and how it starts there, is \
             codex_windows_app_server's",
        ),
    },
    PerSystem {
        name: "codex_windows_app_server",
        macos: NotRead("where the app server runs from on macOS is codex_app_server_daemon's"),
        linux: NotRead("where the app server runs from on Linux is codex_app_server_daemon's"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "codex_windows_app_identity",
        macos: NotRead(
            "ChatGPT for macOS is a bundle, which codex_runs_inside_the_chatgpt_app reads",
        ),
        linux: NotRead("OpenAI makes no desktop app for Linux"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "codex_home_isolates_a_sign_in",
        macos: Read("0.160.0"),
        linux: Read("0.160.0"),
        windows: NotRead(
            "an empty or unset `CODEX_HOME` is not `~/.codex` there, and is read as \
             codex_windows_home",
        ),
    },
    PerSystem {
        name: "codex_windows_home",
        macos: NotRead("macOS's codex finds its default home in `HOME`"),
        linux: NotRead("Linux's codex finds its default home in `HOME`"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "codex_keychain_stores_are_its_own",
        macos: Read(VERIFIED_AGAINST),
        linux: Read(VERIFIED_AGAINST),
        windows: NotRead(
            "Windows has no keychain; what Codex's other stores keep there is codex_windows_stores'",
        ),
    },
    PerSystem {
        name: "codex_windows_stores",
        macos: NotRead("macOS's are codex_keychain_stores_are_its_own's"),
        linux: NotRead("Linux's are codex_keychain_stores_are_its_own's"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    everywhere("codex_identity_is_the_person", VERIFIED_AGAINST),
    everywhere("codex_refusal_is_invalid_grant", VERIFIED_AGAINST),
    PerSystem {
        name: "codex_install_places",
        macos: Read("0.159.2"),
        linux: Read("0.159.2"),
        windows: NotRead(
            "install.ps1, npm and winget put `codex.exe` elsewhere on Windows, which \
             codex_windows_install_places reads",
        ),
    },
    PerSystem {
        name: "codex_windows_install_places",
        macos: NotRead("where macOS's installers put `codex` is codex_install_places'"),
        linux: NotRead("where Linux's installers put `codex` is codex_install_places'"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "codex_windows_version",
        macos: NotRead("a macOS install names its version in its path or its package.json"),
        linux: NotRead("a Linux install names its version in its path or its package.json"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    // Its static half read from 0.162.1's source: it binds 127.0.0.1:1455, sends `GET /cancel`
    // to a login server already there and falls back to 1457, opens the browser with the `http`
    // association's command as a child of its own with no stdio, and reads nothing typed back.
    later_on_windows(
        "codex_login_is_driveable",
        VERIFIED_AGAINST,
        &["W27"],
        "whether `codex login` runs piped, with no console window, from a program on Windows, \
         session step 9",
    ),
    everywhere("codex_login_prints_its_address", "0.160.0"),
    PerSystem {
        name: "codex_store_layers",
        macos: Read("0.160.0"),
        linux: Read("0.160.0"),
        windows: NotRead(
            "Windows keeps this machine's layers in another folder, reads no \
             managed_config.toml and keeps a keyring store's login in the secret store by \
             default, which codex_windows_store_layers reads",
        ),
    },
    PerSystem {
        name: "codex_windows_store_layers",
        macos: NotRead("macOS's layers are codex_store_layers'"),
        linux: NotRead("Linux's layers are codex_store_layers'"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "codex_managed_preferences",
        macos: Read("0.160.0"),
        linux: NotRead(
            "Linux has no managed preferences, and Codex's Linux build holds neither of their \
             keys",
        ),
        windows: NotRead(
            "Windows has no managed preferences, and neither Windows build of Codex holds \
             their keys; what an administrator sets there is codex_windows_store_layers'",
        ),
    },
    everywhere("codex_login_takes_a_store_override", "0.160.0"),
    PerSystem {
        name: "codex_windows_system_proxy",
        macos: NotRead(
            "Pitboard says nothing of a system proxy on macOS, where Codex fills in the one System \
             Settings names the same way",
        ),
        linux: NotRead("Linux has no system proxy setting Codex reads"),
        windows: Read(WINDOWS_VERIFIED_AGAINST),
    },
    PerSystem {
        name: "codex_wsl",
        macos: NotRead("WSL runs Codex's Linux build"),
        linux: Read("0.163.0"),
        windows: NotRead("WSL runs Codex's Linux build"),
    },
];

pub(crate) const ASSUMPTIONS: &[Assumption] = &[
    Assumption {
        name: "codex_login_location",
        fact: "the login is `$CODEX_HOME/auth.json`, default `~/.codex/auth.json`, mode 0600, \
               and `file` is the packaged default backend (`cli_auth_credentials_store`); the \
               alternatives are `keyring`, `auto` and `ephemeral`. Which of them a machine \
               uses is codex_store_layers'",
        read_from: "get_auth_file and FileAuthStorage's 0o600 in codex-rs/login/src/auth/\
                    storage.rs, AuthCredentialsStoreMode in codex-rs/config/src/types.rs, and \
                    codex-rs/config/defaults.toml, at tag rust-v0.160.0, read on 2026-10-07. \
                    0.154.0 packaged the default as `.codexconfig.toml`",
        verified_against: "0.160.0",
        depends: "provider::codex::paths, and every read and write of the live login",
        probe: &["auth.json", "cli_auth_credentials_store"],
        absent: &[],
    },
    Assumption {
        name: "codex_windows_login_location",
        fact: "on Windows the login is `auth.json` in Codex's home (codex_windows_home), and \
               `file` is the packaged default store; `keyring` and `auto` keep it in the \
               secret store by default there (codex_windows_store_layers), and `ephemeral` in \
               memory. Codex writes `auth.json` in place: it opens it with std's default \
               sharing, read, write and delete, truncates it, writes the JSON and flushes, \
               with no temporary file, no rename, no lock and no access list of its own, so \
               the file takes its folder's. A read can catch it empty or half written, and \
               Codex's own read does not try again",
        read_from: "get_auth_file and FileAuthStorage's load, try_read_auth_json and save, \
                    whose 0o600 is under `cfg(unix)` alone, in \
                    codex-rs/login/src/auth/storage.rs, AuthCredentialsStoreMode in \
                    codex-rs/config/src/types.rs and codex-rs/config/defaults.toml, at tag \
                    rust-v0.163.0, read on 2026-10-16; std's OpenOptions on Windows shares \
                    read, write and delete unless told otherwise. Both win32 builds hold the \
                    file's name and the store's setting",
        verified_against: "0.163.0",
        depends: "provider::codex::paths and engine on Windows, every read and write of the \
                  live login there, and the read that tries again where it catches Codex \
                  writing",
        probe: &["auth.json", "cli_auth_credentials_store"],
        absent: &[],
    },
    Assumption {
        name: "codex_windows_stores",
        fact: "on Windows `keyring` with `secret_auth_storage` on, the default there, keeps \
               the login in `%CODEX_HOME%\\secrets\\codex_auth.age`, encrypted with a \
               passphrase Codex keeps in Credential Manager as the generic target \
               `secrets|<16 hex>.codex`, and deletes `auth.json` once it saved there; `auto` \
               does the same and falls back to `auth.json` where that fails. With the feature \
               off, `keyring` keeps the login itself as `cli|<16 hex>.Codex Auth`, through \
               keyring 3.6.3: a generic target `<user>.<service>`, enterprise persistence, \
               the Comment `keyring v3.6.3`, no attributes and the JSON as UTF-16LE, so at \
               most 1280 UTF-16 units. The 16 hex begin the SHA-256 of the home canonicalised \
               by std, `\\\\?\\` kept, or of the home as given where that fails. MCP \
               logins go under `<server>|<16 hex>.Codex MCP Credentials`, or in \
               `secrets\\mcp_oauth.age` under the same `secrets|` key, so a `secrets|` target \
               can be there for MCP alone",
        read_from: "compute_store_key, DirectKeyringAuthStorage, SecretsKeyringAuthStorage \
                    and AutoAuthStorage in codex-rs/login/src/auth/storage.rs, \
                    compute_keyring_account and KEYRING_SERVICE in \
                    codex-rs/secrets/src/lib.rs and CODEX_AUTH_SECRETS_FILENAME in \
                    codex-rs/secrets/src/local.rs, at tag rust-v0.163.0, and the target, \
                    Comment and UTF-16 blob with its CRED_MAX_CREDENTIAL_BLOB_SIZE check in \
                    src/windows.rs of keyring 3.6.3, the version Codex's Cargo.lock pins; read \
                    on 2026-10-16. Both win32 builds hold every name and both prefixes. `cli|` \
                    stands alone there, shorter than a string the checker reads, so the direct \
                    store is read by its message",
        verified_against: "0.163.0",
        depends: "provider::codex::engine's refusal of `keyring`, `auto` and the secret store \
                  on Windows, and doctor's `codex_credential_manager`, which counts `cli|` and \
                  `secrets|` items by name while Codex keeps the file",
        probe: &[
            "Codex Auth",
            "failed to deserialize CLI auth from keyring",
            "secrets|",
            "codex_auth.age",
            "Codex MCP Credentials",
        ],
        absent: &[],
    },
    Assumption {
        name: "codex_login_shape",
        fact: "the document is `{auth_mode, OPENAI_API_KEY, tokens{id_token, access_token, \
               refresh_token, account_id}, last_refresh}`, and `last_refresh` is an RFC 3339 \
               string. Codex matches on it being present at all: without it a login it would \
               otherwise accept reads as `Token data is not available`",
        read_from: "the AuthDotJson struct and get_token_data's pattern",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::codex::engine's renew, which writes it on every renewal",
        probe: &["last_refresh", "Token data is not available"],
        absent: &[],
    },
    Assumption {
        name: "codex_identity_is_local",
        fact: "the ID token is a JWT whose claims name the account: `email`, and under \
               `https://api.openai.com/auth` the `chatgpt_account_id` and `chatgpt_plan_type`. \
               So who a parked Codex login belongs to costs no network call at all",
        read_from: "the real id_token this machine's Codex had written",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::codex::engine::identify, and provider::codex::engine's plan, which \
                  reads `chatgpt_plan_type` for `pitboard status`, the app and the account's \
                  record",
        probe: &["chatgpt_account_id", "chatgpt_plan_type"],
        absent: &[],
    },
    Assumption {
        name: "codex_renewal",
        fact: "a refresh chain is exchanged at `https://auth.openai.com/oauth/token` with a \
               JSON body `{client_id, grant_type, refresh_token}` and client id \
               `app_EMoamEEZ73f0CkXaXp7hrann`. The answer's `id_token`, `access_token` and \
               `refresh_token` are each written only if present, so rotation is optional",
        read_from: "the auth manager's refresh path",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::codex::api::renew",
        probe: &["app_EMoamEEZ73f0CkXaXp7hrann", "auth.openai.com"],
        absent: &[],
    },
    Assumption {
        name: "codex_usage_endpoint",
        fact: "`GET https://chatgpt.com/backend-api/wham/usage` with `Authorization: Bearer` \
               and `ChatGPT-Account-ID` answers with `plan_type` and a `rate_limit` holding \
               `primary_window` and `secondary_window`, each `{used_percent, \
               limit_window_seconds, reset_after_seconds, reset_at}` and either of them \
               null. No model request and no quota spent",
        read_from: "a live response from this endpoint, not from a description of it: a \
                    parser written from the source had the windows one level up, the length \
                    in minutes and the reset under another name, and returned nothing while \
                    the request succeeded",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::codex::api::usage, and every limit Pitboard shows for a Codex \
                  account",
        probe: &["wham/usage", "ChatGPT-Account-ID", "used_percent"],
        absent: &[],
    },
    Assumption {
        name: "codex_revokes_on_its_own_sign_out",
        fact: "`codex login` and `codex logout` both POST the stored refresh token to \
               `https://auth.openai.com/oauth/revoke` before clearing it, so a parked copy \
               left live beside its twin is a token the person's own next login kills in \
               both places",
        read_from: "clear_existing_auth_before_login and the revoke path",
        verified_against: VERIFIED_AGAINST,
        depends: "ParkSemantics::MoveOnly for Codex, and the read-back before install",
        probe: &["oauth/revoke"],
        absent: &[],
    },
    Assumption {
        name: "codex_never_follows_a_switch",
        fact: "a running Codex caches its login in memory for the life of the process with no \
               expiry and no file watcher, and refuses a reload whose account id has changed, \
               so a switch is invisible until it is started again. A refresh already under \
               way when the file changes writes its own account's tokens over whatever is \
               there, keeping the account id it finds, which leaves one login naming two \
               accounts",
        read_from: "the auth manager's cache and reload_if_account_id_matches",
        verified_against: VERIFIED_AGAINST,
        depends: "Adoption::RestartRequired for Codex, and what a switch tells the person",
        probe: &[
            "Skipping auth reload due to account id mismatch",
            "since logged out or signed in to another account",
        ],
        absent: &[],
    },
    Assumption {
        name: "codex_runs_inside_the_chatgpt_app",
        fact: "OpenAI's ChatGPT app for macOS, bundle id `com.openai.codex`, runs a codex of \
               its own: two processes of `Contents/Resources/codex-cli/CodexCLI.app/Contents/\
               MacOS/codex` inside the app's bundle, children of the app, reading \
               `$CODEX_HOME/auth.json` as any codex does. Closing the app's windows leaves it \
               running; quitting it stops them",
        read_from: "the process list, the app's Info.plist and the bundled codex-package.json \
                    on a Mac running the app on 2026-10-01; what closing and quitting do is \
                    read from the app's own code and not yet watched",
        verified_against: "26.928.31416",
        depends: "provider::codex::holders's chatgpt_app, and the app's offer to quit ChatGPT \
                  before a Codex switch",
        probe: &[],
        absent: &[],
    },
    Assumption {
        name: "codex_app_server_daemon",
        fact: "codex can run on its own as a background app server, from \
               `$CODEX_HOME/packages/app-server-daemon/releases/<version>/bin/codex`, a child \
               of launchd that terminal sessions can share, and `codex app-server daemon \
               restart` starts it again",
        read_from: "the process list on 2026-10-01 and the daemon subcommands of the codex it \
                    ran",
        verified_against: "0.159.3",
        depends: "provider::codex::holders's app_server_daemon",
        probe: &["app-server daemon restart"],
        absent: &[],
    },
    Assumption {
        name: "codex_windows_app_server",
        fact: "on Windows the background app server runs \
               `<CODEX_HOME>\\packages\\app-server-daemon\\current\\bin\\codex.exe`, where \
               `current` is a junction Codex itself points at one of its \
               `releases\\<version>-<target>`. Codex starts it with `DETACHED_PROCESS | \
               CREATE_BREAKAWAY_FROM_JOB`, out of the starting terminal's job, refuses to from \
               an elevated terminal, and `codex app-server daemon restart` starts it again",
        read_from: "package_root and managed_codex_bin in \
                    codex-rs/app-server-daemon/src/managed_install.rs, select_release in \
                    prepare_install_windows.rs, the Windows start in backend/pid_start.rs and \
                    ensure_not_elevated in backend/windows.rs, at tag rust-v0.162.1, read on \
                    2026-10-16; the win32-x64 and win32-arm64 builds hold their messages",
        verified_against: "0.162.1",
        depends: "provider::codex::holders's app_server_daemon on Windows",
        probe: &[
            "app-server daemon restart",
            "failed to create managed daemon junction after native creation was denied",
            "start the Windows daemon from a non-elevated terminal",
        ],
        absent: &[],
    },
    Assumption {
        name: "codex_windows_app_identity",
        fact: "OpenAI's desktop app for Windows, ChatGPT- or Codex-branded, is the Store package \
               of family `OpenAI.Codex_2p2nqsd0c76g0`, so its app's id is \
               `OpenAI.Codex_2p2nqsd0c76g0!App`, the one Start lists as `OpenAI.Codex_*!App`. \
               Codex tells the app's running process by that family, from \
               `GetPackageFamilyName`",
        read_from: "installed_windows_app and process_package_family in \
                    codex-rs/cli/src/doctor/desktop/platform.rs, and codex_app_is_installed in \
                    codex-rs/cli/src/desktop_app/windows.rs, at tags rust-v0.162.1 and \
                    rust-v0.163.0, read on 2026-10-16; the win32-x64 and win32-arm64 builds of \
                    both hold both",
        verified_against: "0.163.0",
        depends: "provider::codex::holders's chatgpt_app on Windows, the app id it hands an \
                  app to quit ChatGPT with, and the Windows app's WindowsApps, which finds \
                  ChatGPT's running process by that family",
        probe: &[
            "OpenAI.Codex_2p2nqsd0c76g0",
            "Get-StartApps | Where-Object AppID -Like 'OpenAI.Codex_*!App'",
        ],
        absent: &[],
    },
    Assumption {
        name: "codex_home_isolates_a_sign_in",
        fact: "`CODEX_HOME` moves everything Codex keeps in a home, so a sign-in with \
               `CODEX_HOME` set to an empty private directory writes `auth.json` there and \
               nowhere else, as long as its store is the file. A home with no `config.toml` \
               does not make it so: `/etc/codex/config.toml` and a trusted project's config \
               are read whatever the home, which is why Pitboard's sign-ins name the file \
               store with `-c` (codex_login_takes_a_store_override). An empty `CODEX_HOME` \
               means unset and falls back to `~/.codex`, which is why the directory is always \
               set and never empty",
        read_from: "find_codex_home_from_env in codex-rs/utils/home-dir/src/lib.rs and the \
                    layers load_config_layers_state reads, at tag rust-v0.160.0, read on \
                    2026-10-07",
        verified_against: "0.160.0",
        depends: "provider::codex::engine's sign_in and read_signin, and Isolation for Codex",
        probe: &["CODEX_HOME", "cli_auth_credentials_store"],
        absent: &[],
    },
    Assumption {
        name: "codex_windows_home",
        fact: "on Windows a `CODEX_HOME` that is set and not empty must name a folder, or Codex \
               does not start, and is canonicalised, `\\\\?\\` taken off. Unset or empty, the \
               home is `.codex` in `FOLDERID_Profile`, which ignores `USERPROFILE` and `HOME`. \
               Codex's install.ps1 puts it in `%USERPROFILE%\\.codex`, so the two part where \
               `USERPROFILE` is moved",
        read_from: "find_codex_home_from_env in codex-rs/utils/home-dir/src/lib.rs, \
                    AbsolutePathBuf::from_absolute_path in codex-rs/utils/absolute-path, and \
                    scripts/install/install.ps1, at tag rust-v0.162.1; dirs 6.0.0's home_dir \
                    and dirs-sys 0.5.0's known_folder_profile; read on 2026-10-15, with the \
                    messages in the win32-x64 and win32-arm64 builds",
        verified_against: "0.162.1",
        depends: "provider::codex::paths::home and missing_home, and Context::read's lookup of \
                  the profile",
        probe: &[
            "CODEX_HOME points to",
            "but that path does not exist",
            "but that path is not a directory",
        ],
        absent: &[],
    },
    Assumption {
        name: "codex_keychain_stores_are_its_own",
        fact: "the `keyring` and `auto` stores keep the login in a keychain item `Codex Auth` \
               that Codex creates through the Security framework, and `[features] \
               secret_auth_storage` keeps it in `secrets/codex_auth.age` under a keychain key. \
               Neither item trusts `/usr/bin/security`, so Pitboard refuses those stores \
               rather than put a permission prompt in front of every read",
        read_from: "the keyring store, the secret auth storage feature and their key names",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::codex::layers and Codex::live's refusal",
        probe: &["Codex Auth", "secret_auth_storage", "codex_auth.age"],
        absent: &[],
    },
    Assumption {
        name: "codex_identity_is_the_person",
        fact: "`chatgpt_account_id` is the ChatGPT plan, which a Team or Business workspace \
               shares between its members, and `chatgpt_user_id` under the same claim \
               namespace is the person. The pair names one login's quota",
        read_from: "the id token claims Codex reads, and the caches it keys on the same pair",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::codex::engine::identify, and every account id Pitboard records \
                  for Codex",
        probe: &["chatgpt_user_id", "chatgpt_account_id"],
        absent: &[],
    },
    Assumption {
        name: "codex_refusal_is_invalid_grant",
        fact: "a refresh token that has been spent or revoked is refused with a 400 whose \
               error is `invalid_grant`, or a 401. Any other 400 is a request the server did \
               not like and is not a dead login",
        read_from: "the refresh error classification",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::codex::api's refused_for_good, which decides when a park is \
                  dropped",
        probe: &["invalid_grant"],
        absent: &[],
    },
    Assumption {
        name: "codex_install_places",
        fact: "the standalone installer links `~/.local/bin/codex`, or `$CODEX_INSTALL_DIR/codex`, \
               to `$CODEX_HOME/packages/standalone/current/bin/codex`; the build names \
               `npm install -g @openai/codex` and `brew upgrade --cask codex` as the other ways \
               it is kept up to date, which put `codex` in npm's and Homebrew's `bin`",
        read_from: "`scripts/install/install.sh` at tag `rust-v0.159.2`, the link a standalone \
                    install of 0.159.2 left on a Mac, read on 2026-10-05, and the update \
                    commands in the macOS and Linux binaries",
        // 0.154.0 names the installers too, but not the standalone package layout.
        verified_against: "0.159.2",
        depends: "codex::paths::install_places, where an app looks for `codex` after the \
                  login shell's PATH",
        probe: &[
            "packages/standalone",
            "codex/install.sh",
            "npm install -g @openai/codex",
            "brew upgrade --cask codex",
        ],
        absent: &[],
    },
    Assumption {
        name: "codex_windows_install_places",
        fact: "on Windows install.ps1 keeps each release in `<CODEX_HOME or \
               %USERPROFILE%\\.codex>\\packages\\standalone\\releases\\<version>-<target>`, \
               points `current` at it by a junction, and makes `CODEX_INSTALL_DIR`, or \
               `%LOCALAPPDATA%\\Programs\\OpenAI\\Codex\\bin` where that is unset or white \
               space, a junction to `current\\bin`, which it puts on the user `Path`. A global \
               npm install's `codex.cmd` in its prefix, `%APPDATA%\\npm` by default, starts \
               `bin/codex.js`, which runs `vendor/<target>/bin/codex.exe` of the platform's \
               package, beside `codex-package.json`. It finds that package as Node finds one \
               from its own `bin`, else uses its own `vendor`, and starts the program with its \
               arguments and every variable it was given, `CODEX_MANAGED_PACKAGE_ROOT` set to \
               its own package's folder and `CODEX_MANAGED_BY_NPM` to `1`, where no other \
               package manager's files are found, the other `CODEX_MANAGED_BY_` ones removed. \
               winget's `OpenAI.Codex` is a zip whose \
               nested portable `codex-<target>.exe` is linked in winget's `Links` as `codex`",
        read_from: "scripts/install/install.ps1 at tag rust-v0.162.1; `bin/codex.js` of \
                    `@openai/codex@0.162.1`, and its win32-x64 and win32-arm64 packages; the \
                    installer manifest of `OpenAI.Codex` 0.161.0, the newest in \
                    microsoft/winget-pkgs; read on 2026-10-15, and `bin/codex.js` again on \
                    2026-10-16. `Programs\\OpenAI\\Codex` is in install.ps1 alone, so the build \
                    is probed for the package's own file and the variable the launcher sets",
        verified_against: "0.162.1",
        depends: "codex::paths::install_places and host::Os::package_bins, where an app looks \
                  for `codex.exe` after the registry's `Path`, and codex::paths::npm_native, \
                  which a sign-in starts in place of npm's `codex.cmd`",
        probe: &["codex-package.json", "CODEX_MANAGED_PACKAGE_ROOT"],
        absent: &[],
    },
    Assumption {
        name: "codex_windows_version",
        fact: "on Windows `codex.exe` carries no version resource at all. The package it runs \
               from, npm's vendored one or a standalone release, holds `codex-package.json` \
               beside its `bin`, `{layoutVersion, version, target, variant: \"codex\", \
               entrypoint: \"bin/codex.exe\"}`, whose `version` is the release",
        read_from: "the win32-x64 and win32-arm64 builds of 0.162.1, which have no resource \
                    directory, their packages' `codex-package.json`, and the build's own check \
                    of a local package, read on 2026-10-15",
        verified_against: "0.162.1",
        depends: "doctor's codex_version, which reads the release there on Windows",
        probe: &["codex-package.json"],
        absent: &[],
    },
    Assumption {
        name: "codex_login_is_driveable",
        fact: "`codex login` revokes whatever login is stored in its home before signing in, \
               opens the browser itself, prints the address to stderr for when it cannot, \
               listens for the callback on a loopback port and reads nothing from stdin, so \
               it runs piped in a private home with no terminal",
        read_from: "the login command and its local server",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::codex::engine's sign_in and read_sign_in, and the watched sign-in \
                  the app runs",
        probe: &["Starting local login server"],
        absent: &[],
    },
    Assumption {
        name: "codex_login_prints_its_address",
        fact: "`codex login` prints to stderr `Starting local login server on \
               http://localhost:<port>.`, then `If your browser did not open, navigate to this \
               URL to authenticate:`, a blank line and the address, bare on a line of its own: \
               `<issuer>/oauth/authorize?…`, with the issuer `https://auth.openai.com`. So the \
               first `https` address it prints is the one to open, and the loopback one before \
               it, where the browser comes back to, is `http`. Nothing on that path reads stdin",
        read_from: "print_login_server_start in cli/src/login.rs, and build_authorize_url and \
                    DEFAULT_ISSUER in login/src/server.rs, at tag rust-v0.160.0, and the same \
                    strings in the macOS and Linux binaries",
        verified_against: "0.160.0",
        depends: "provider::codex::engine's read_sign_in, and the address the app's sign-in \
                  sheet offers",
        probe: &[
            "Starting local login server on http://localhost:",
            "If your browser did not open, navigate to this URL to authenticate:",
            "/oauth/authorize",
        ],
        absent: &[],
    },
    Assumption {
        name: "codex_store_layers",
        fact: "the store is `cli_auth_credentials_store` as the highest layer that sets it \
               says, lowest first: the packaged default `file`, `/etc/codex/config.toml`, an \
               enterprise's cloud config, which `codex login` does not load and a session of \
               the TUI does, and from which nothing strips the store, \
               `$CODEX_HOME/config.toml`, a profile's `$CODEX_HOME/<name>.config.toml` only for \
               a run given `--profile`, which `codex login` and `codex logout` refuse, a \
               trusted project's `.codex/config.toml`, `-c` on the command line, and \
               `/etc/codex/managed_config.toml`. A profile's file is a layer of its own just \
               over `$CODEX_HOME/config.toml`, and `--profile <name>` is refused where \
               `$CODEX_HOME/config.toml` has a `profile = \"<name>\"` line or a \
               `[profiles.<name>]` table of that name. Such a line, which 0.160.0 calls a \
               legacy way to choose a profile, stops 0.160.0 from starting in any layer, \
               once it has read the configuration whole, whether or not a table defines the \
               profile; a project's layer may set neither it nor `profiles`. 0.99.0 has no \
               such refusal: it chooses with the line the profile `[profiles.<name>]` \
               defines, and stops with an error for one no table defines. A profile's table \
               holds no store either build reads: neither one's `ConfigProfile` has \
               `cli_auth_credentials_store`, and each takes the store from the merged top \
               level alone, so one in a profile's table is dropped, or in 0.160.0 refused as \
               a field Codex does not know under `--strict-config`. 0.160.0 applies no \
               profile's `[features]`; 0.99.0 applies them, and none of its features is \
               about the store. \
               `/etc/codex/requirements.toml` pins the store over every layer, and cloud \
               requirements may not set it. `secret_auth_storage`, which makes a keychain \
               store an encrypted file of secrets, is read from `[features]` in the same \
               layers, and from `[features]` or `[feature_requirements]` in requirements, which \
               pin it the same way. It is off by default on macOS and Linux. A layer that is \
               there and is not TOML Codex can read, or that names a store Codex does not \
               have, stops Codex from starting",
        read_from: "load_config_layers_state, load_requirements_from_sources, \
                    ConfigLayerSource::precedence and LOCAL_ONLY_AUTH_REQUIREMENTS in \
                    codex-rs/config/src, codex-rs/config/defaults.toml, apply_to_config and the \
                    cli_auth_credentials_store_mode it feeds and the legacy profile refusal in \
                    load_config_with_layer_stack in codex-rs/core/src/config, \
                    profile_v2_for_subcommand in codex-rs/cli/src/main.rs, the \
                    SecretAuthStorage feature's `cfg!(windows)` default in \
                    codex-rs/features/src/lib.rs, the `alias = \"feature_requirements\"` \
                    ConfigRequirementsToml has and ConfigToml does not in codex-rs/config/src, \
                    strip_cloud_auth_requirements, which strips the store from cloud \
                    requirements alone, and the cloud_config_bundle codex-rs/tui/src/lib.rs \
                    builds its config with, at tag rust-v0.160.0, read on 2026-10-07. For a \
                    profile, read the same day at that tag: ConfigProfile in \
                    codex-rs/config/src/profile_toml.rs, which has no store and does not deny \
                    a field it does not know, the empty profile source \
                    load_config_with_layer_stack hands Features::from_sources, \
                    config_error_from_ignored_toml_value_fields in \
                    codex-rs/config/src/strict_config.rs, PROJECT_LOCAL_CONFIG_DENYLIST and \
                    the check of `--profile` against a legacy profile in \
                    load_config_layers_state, and the precedence 21, over the person's 20, \
                    ConfigLayerSource::precedence gives a User layer with a profile. Whether \
                    a running session's store follows a cloud fragment was not read. The \
                    three `/etc/codex` paths are in the macOS and Linux binaries, and in neither \
                    Windows one; the profile refusal is in all four, three times each in the \
                    macOS and Linux ones. That 0.99.0 chooses a profile with the line is read \
                    from its darwin-arm64 build, as bytes on 2026-10-07: it holds no copy of \
                    the refusal, and holds the error for a chosen profile that is not there, \
                    \"config profile `<name>` not found\". 0.160.0's ConfigToml still has \
                    the `profile` field, documented as the profile to use from the `profiles` \
                    map. How 0.99.0 reads a chosen profile, read at tag rust-v0.99.0 on \
                    2026-10-07: ConfigProfile in codex-rs/core/src/config/profile.rs, which \
                    has no store and denies a field it does not know only in its JSON schema; \
                    Config's load in codex-rs/core/src/config/mod.rs, which reads the \
                    config its layers merge into, takes the profile `--profile` or the \
                    `profile` line names, stops with \"config profile `<name>` not found\" \
                    where `profiles` has none of that name, and sets \
                    cli_auth_credentials_store_mode from cfg.cli_auth_credentials_store \
                    alone, the file by default (AuthCredentialsStoreMode in \
                    codex-rs/core/src/auth/storage.rs); Features::from_config in \
                    codex-rs/core/src/features.rs, which applies a profile's features, none \
                    of them about the store; and load_config_or_exit in \
                    codex-rs/cli/src/login.rs, through which `codex login` loads that config",
        verified_against: "0.160.0",
        depends: "provider::codex::layers, and so every read and write of the live Codex \
                  login and every refusal of a store",
        probe: &[
            "/etc/codex/config.toml",
            "/etc/codex/managed_config.toml",
            "/etc/codex/requirements.toml",
            "cli_auth_credentials_store",
            "secret_auth_storage",
            "` config is no longer supported; use `--profile ",
        ],
        absent: &[],
    },
    Assumption {
        name: "codex_windows_store_layers",
        fact: "on Windows the store is `cli_auth_credentials_store` as the highest layer that \
               sets it says, lowest first: the packaged default `file`, `config.toml` in \
               `OpenAI\\Codex` under `FOLDERID_ProgramData`, or under `C:\\ProgramData` \
               where Windows cannot say that folder, never the `ProgramData` variable, an \
               enterprise's cloud config, `%CODEX_HOME%\\config.toml`, a profile's file, a \
               trusted project's `.codex\\config.toml`, and `-c`. `requirements.toml` in the \
               same folder pins it over every layer, and cloud requirements may not set it. \
               There are no managed preferences and no registry layer, and Codex skips a \
               `managed_config.toml` in its home with a warning at startup. \
               `secret_auth_storage` is read from the same layers and is on by default, so \
               `keyring` and `auto` keep the login in the secret store unless a layer turns it \
               off. A layer that is there and is not TOML Codex can read, or that names a store \
               Codex does not have, stops Codex from starting",
        read_from: "load_config_layers_state, windows_codex_system_dir and \
                    windows_program_data_dir_from_known_folder in \
                    codex-rs/config/src/loader/mod.rs, load_requirements_from_sources in \
                    loader/managed_requirements.rs, load_config_layers_internal and \
                    managed_config_default_path in loader/layer_io.rs, \
                    LOCAL_ONLY_AUTH_REQUIREMENTS and strip_cloud_auth_requirements in \
                    codex-rs/config/src/requirements_layers/layer.rs, and the \
                    SecretAuthStorage feature's `cfg!(windows)` default in \
                    codex-rs/features/src/lib.rs, at tag rust-v0.163.0, read on 2026-10-16. \
                    probe_windows_system_config_namespace only counts who owns the folder for \
                    telemetry. The win32-x64 and win32-arm64 builds hold the folder, the \
                    warning and the fallback's message, and neither holds `/etc/codex`",
        verified_against: "0.163.0",
        depends: "provider::codex::layers on Windows, and so every read and write of the \
                  live Codex login there and every refusal of a store",
        probe: &[
            "OpenAI\\Codex",
            "managed_config.toml is no longer supported on Windows",
            "Failed to resolve ProgramData known folder",
            "cli_auth_credentials_store",
            "secret_auth_storage",
        ],
        absent: &[],
    },
    Assumption {
        name: "codex_managed_preferences",
        fact: "on macOS, Codex also reads what a configuration profile forces for \
               `com.openai.codex`, and only what it forces: `config_toml_base64`, base64 TOML \
               read as configuration over every file, `/etc/codex/managed_config.toml` \
               included, and `requirements_toml_base64`, read as requirements over \
               `/etc/codex/requirements.toml`. A value it forces must be a string of canonical \
               standard base64 whose bytes are UTF-8, or Codex does not start",
        read_from: "codex-rs/config/src/loader/macos.rs at tag rust-v0.160.0, read on \
                    2026-10-07: MANAGED_PREFERENCES_APPLICATION_ID and its two keys, \
                    load_managed_preference_with, which asks CFPreferencesAppValueIsForced \
                    before and after CFPreferencesCopyAppValue, and the BASE64_STANDARD decode; \
                    and where load_requirements_from_sources and load_config_layers_state put \
                    them. The two keys are in the macOS binary, and in neither the Linux nor \
                    the Windows ones",
        verified_against: "0.160.0",
        depends: "host::macos::preferences, and provider::codex::layers's ManagedPreference \
                  and RequiredByPreference",
        probe: &[
            "com.openai.codex",
            "config_toml_base64",
            "requirements_toml_base64",
        ],
        absent: &[],
    },
    Assumption {
        name: "codex_login_takes_a_store_override",
        fact: "`codex -c cli_auth_credentials_store=\"file\" login` signs in to the file \
               store, unless a layer over the session flags or a requirement chooses another. \
               Codex's own command line takes `-c key=value` before a subcommand, and after \
               it, since the flag is global; `login` adds its own after the root's. The value \
               is read as TOML and put in the session-flags layer, over \
               `$CODEX_HOME/config.toml`, a profile's and a project's config. Which layers are \
               over it on each system, and which requirements there are, is \
               codex_store_layers'. `codex login` has no flag of its own for a store",
        read_from: "CliConfigOverrides in codex-rs/utils/cli/src/config_override.rs, \
                    MultitoolCli and the Login arm in codex-rs/cli/src/main.rs, \
                    load_config_or_exit in codex-rs/cli/src/login.rs, and the precedence of \
                    ConfigLayerSource::SessionFlags, at tag rust-v0.160.0, read on 2026-10-07. \
                    The flag's help is in the macOS, Linux and both Windows binaries",
        verified_against: "0.160.0",
        depends: "provider::codex::engine's sign_in, and so where a sign-in Pitboard runs \
                  leaves the login it reads back",
        probe: &[
            "Override a configuration value that would otherwise be loaded from",
            "cli_auth_credentials_store",
        ],
        absent: &[],
    },
    Assumption {
        name: "codex_windows_system_proxy",
        fact: "on Windows Codex sends its requests through the proxy the variables name, and \
               where `HTTPS_PROXY` and `https_proxy` are unset or empty, its https requests \
               through `ProxyServer` in \
               `HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings` where \
               `ProxyEnable` is not 0, whatever `HTTP_PROXY` and `ALL_PROXY` say, exempting \
               `ProxyOverride`'s hosts where neither `NO_PROXY` nor `no_proxy` is set. It reads \
               `ProxyServer` whole as one address: a proxy for each protocol, \
               `http=host:port;https=host:port`, is none it reads, so its requests go out \
               directly, or through `ALL_PROXY`'s, and one entry alone, `https=host:port`, names \
               a host called `https=host`, so they fail. That is reqwest 0.12.28's default \
               proxy, hyper-util 0.1.20's `Matcher::from_system`. The `respect_system_proxy` \
               feature, off by default, asks the system's PAC and WPAD before the variables, and \
               `system_proxy_fallback`, on by default, sends a sign-in's request again through \
               them after a direct one fails",
        read_from: "`win::with_system`, `Builder::build` and `parse_env_uri` in hyper-util \
                    0.1.20's src/client/proxy/matcher.rs, the http crate's authority parser, \
                    which refuses a second colon, `Matcher::system` in reqwest 0.12.28's \
                    src/proxy.rs, and codex-rs/http-client/src/outbound_proxy.rs and \
                    codex-rs/features/src/lib.rs at tag rust-v0.163.0, read on 2026-10-16. Both \
                    win32 builds of 0.163.0 hold the key and its three values",
        verified_against: "0.163.0",
        depends: "doctor's system_proxy note on Windows, which says where Codex's requests go \
                  while HTTPS_PROXY is unset, and where Claude Code's and Pitboard's go",
        probe: &[
            r"Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "ProxyEnable",
            "ProxyServer",
            "system_proxy_fallback",
        ],
        absent: &[],
    },
    Assumption {
        name: "codex_wsl",
        fact: "Codex's Linux build takes itself to run in WSL where `WSL_DISTRO_NAME` is set or \
               `/proc/version` names Microsoft, in any case. There, or where `WSL_INTEROP` is set, \
               it starts no shared background server where the first folder of its home that \
               opens is on a 9p file system, as WSL 2 mounts Windows' drives. It compares paths \
               under `/mnt/<drive>` in any case, and runs its updater with a Windows path \
               `C:\\x` as `/mnt/c/x`",
        read_from: "`is_wsl` in codex-rs/utils/path-utils/src/env.rs, `uses_wsl_drvfs` in \
                    codex-rs/tui/src/daemon_startup.rs and where the TUI and `remote-control` ask \
                    it, and codex-rs/cli/src/wsl_paths.rs, at tag rust-v0.163.0, read on \
                    2026-10-16. The linux-x64 build of 0.163.0 holds both variables, \
                    `/proc/version` and the exclusion's words",
        verified_against: "0.163.0",
        depends: "doctor's wsl check on Linux, which reads WSL as Codex does and says where \
                  Codex's home is on a Windows drive",
        probe: &[
            "WSL_DISTRO_NAME",
            "WSL_INTEROP",
            "/proc/version",
            "a Windows-mounted WSL CODEX_HOME (DrvFS/9p)",
        ],
        absent: &[],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assumptions::Platform;

    // The rest the Windows work reads for Windows on its own.
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
                "codex_windows_login_location",
                "codex_login_shape",
                "codex_identity_is_local",
                "codex_renewal",
                "codex_usage_endpoint",
                "codex_revokes_on_its_own_sign_out",
                "codex_never_follows_a_switch",
                "codex_windows_app_server",
                "codex_windows_app_identity",
                "codex_windows_home",
                "codex_windows_stores",
                "codex_identity_is_the_person",
                "codex_refusal_is_invalid_grant",
                "codex_windows_install_places",
                "codex_windows_version",
                "codex_login_prints_its_address",
                "codex_windows_store_layers",
                "codex_login_takes_a_store_override",
                "codex_windows_system_proxy",
            ]
        );
    }

    // The conformance run checks a build of each system for them.
    #[test]
    fn macos_and_linux_read_every_fact() {
        let windows = [
            "codex_windows_login_location",
            "codex_windows_stores",
            "codex_windows_app_server",
            "codex_windows_app_identity",
            "codex_windows_home",
            "codex_windows_install_places",
            "codex_windows_version",
            "codex_windows_store_layers",
            "codex_windows_system_proxy",
        ];
        for line in PER_SYSTEM
            .iter()
            .filter(|l| !windows.contains(&l.name) && l.name != "codex_wsl")
        {
            assert!(matches!(line.on(Platform::MacOs), Read(_)), "{}", line.name);
            assert_eq!(
                matches!(line.on(Platform::Linux), Read(_)),
                line.name != "codex_managed_preferences",
                "{} on linux",
                line.name
            );
        }
        let wsl = PER_SYSTEM.iter().find(|l| l.name == "codex_wsl").unwrap();
        assert_eq!(wsl.on(Platform::Linux), Read("0.163.0"));
        assert!(matches!(wsl.on(Platform::MacOs), NotRead(_)));
    }

    /// Each fact re-read from 0.160.0 for which store Codex keeps its login in, and the two
    /// read for it, name that build on macOS and Linux.
    #[test]
    fn the_store_facts_are_read_from_0_160_0() {
        for name in [
            "codex_login_location",
            "codex_home_isolates_a_sign_in",
            "codex_store_layers",
            "codex_managed_preferences",
            "codex_login_takes_a_store_override",
        ] {
            let line = PER_SYSTEM.iter().find(|l| l.name == name).expect(name);
            assert_eq!(line.on(Platform::MacOs), Read("0.160.0"), "{name}");
        }
    }
}
