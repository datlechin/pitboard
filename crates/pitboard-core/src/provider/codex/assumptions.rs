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
               so a switch is invisible until it is started again",
        read_from: "the auth manager's cache and reload_if_account_id_matches",
        verified_against: VERIFIED_AGAINST,
        depends: "Adoption::RestartRequired for Codex, and what a switch tells the person",
        probe: &["Skipping auth reload due to account id mismatch"],
        absent: &[],
    },
    Assumption {
        name: "codex_home_isolates_both_backends",
        fact: "`CODEX_HOME` moves the auth file, and the keyring backend's account is \
               `cli|` plus the first sixteen hex characters of the SHA-256 of the canonical \
               home, so a scratch home isolates a private sign-in whichever backend is in use",
        read_from: "the home resolution and compute_store_key",
        verified_against: VERIFIED_AGAINST,
        depends: "provider::codex::paths::keychain_account, and Isolation for Codex",
        probe: &["CODEX_HOME", "Codex Auth"],
        absent: &[],
    },
];
