//! Every fact about Codex CLI that pitboard stands on, named and dated.
//!
//! Read from codex-cli 0.154.0: the binary installed on the machine this was written on,
//! the matching public source at tag `rust-v0.154.0`, and the real `auth.json` that build
//! had written.
//!
//! See [`crate::assumptions`] for what an entry means and how a probe reads one.

use crate::assumptions::Assumption;

/// The build every entry below was read from.
pub const VERIFIED_AGAINST: &str = "0.154.0";

pub const ASSUMPTIONS: &[Assumption] = &[
    Assumption {
        name: "codex_login_location",
        fact: "the login is `$CODEX_HOME/auth.json`, default `~/.codex/auth.json`, mode 0600, \
               and `file` is the packaged default backend (`cli_auth_credentials_store`); the \
               alternatives are `keyring`, `auto` and `ephemeral`",
        read_from: "the auth storage module's file path and the packaged `.codexconfig.toml`",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::codex::paths, and every read and write of the live login",
        probe: &["auth.json", "cli_auth_credentials_store"],
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
        depends: "provider::codex::engine::identify",
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
        depends: "provider::codex::api::usage, and every limit pitboard shows for a Codex \
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
        name: "codex_home_isolates_a_sign_in",
        fact: "`CODEX_HOME` moves everything Codex keeps, and a home with no `config.toml` \
               keeps its login in the file store, so a sign-in with `CODEX_HOME` set to an \
               empty private directory writes `auth.json` there and nowhere else. An empty \
               `CODEX_HOME` means unset and falls back to `~/.codex`, which is why the \
               directory is always set and never empty",
        read_from: "the home resolution and the packaged default store",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::codex::engine's sign_in and read_signin, and Isolation for Codex",
        probe: &["CODEX_HOME", "cli_auth_credentials_store"],
        absent: &[],
    },
    Assumption {
        name: "codex_keychain_stores_are_its_own",
        fact: "the `keyring` and `auto` stores keep the login in a keychain item `Codex Auth` \
               that Codex creates through the Security framework, and `[features] \
               secret_auth_storage` keeps it in `secrets/codex_auth.age` under a keychain key. \
               Neither item trusts `/usr/bin/security`, so pitboard refuses those stores \
               rather than put a permission prompt in front of every read",
        read_from: "the keyring store, the secret auth storage feature and their key names",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::codex::paths::backend and Codex::live's refusal",
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
        depends: "provider::codex::engine::identify, and every account id pitboard records \
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
        name: "codex_login_is_driveable",
        fact: "`codex login` revokes whatever login is stored in its home before signing in, \
               opens the browser itself, prints the address to stderr for when it cannot, \
               listens for the callback on a loopback port and reads nothing from stdin, so \
               it runs piped in a private home with no terminal",
        read_from: "the login command and its local server",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::codex::engine::sign_in, and the watched sign-in the app runs",
        probe: &["Starting local login server"],
        absent: &[],
    },
];
