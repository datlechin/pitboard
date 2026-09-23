//! Codex CLI behind the provider boundary.
//!
//! Read from codex-cli 0.154.0: the binary on this machine and the matching public source.
//! Every fact this leans on is dated in [`super::assumptions`].

use super::{api, paths};
use crate::context::Context;
use crate::provider::{
    Adoption, Credential, Expiry, Identity, Isolation, ParkSemantics, Provider, ProviderError,
    ProviderId, jwt,
};
use crate::store::{self, Live, RawStore};
use crate::usage::Snapshot;
use serde_json::Value;

/// The claim namespace OpenAI puts its own facts under in a standard token.
const OPENAI: &str = "https://api.openai.com/auth";

#[derive(Debug, Clone, Copy)]
pub(crate) struct Codex;

/// Where Codex looks for its login, in the order it looks.
///
/// The shipped default is the file, and this machine's Codex uses it. With the keyring
/// backend configured the item comes first, filed under an account derived from
/// `CODEX_HOME`, which is what makes that home isolate the keyring as well as the file.
fn chain(ctx: &Context) -> Live {
    let host = ctx.host();
    let mut backends: Vec<Box<dyn RawStore>> = Vec::new();
    if paths::backend(ctx) != paths::Backend::File
        && let Some(keychain) = host.foreign_keychain(ctx, &paths::keychain_account(ctx))
    {
        backends.push(keychain);
    }
    backends.push(host.file(paths::auth_file(ctx)));
    Live::of(backends)
}

impl Provider for Codex {
    fn id(&self) -> ProviderId {
        ProviderId::Codex
    }

    fn read_live(&self, ctx: &Context) -> Result<Option<Credential>, ProviderError> {
        if paths::backend(ctx) == paths::Backend::Ephemeral {
            return Err(ProviderError::ShapeUnexpected {
                provider: ProviderId::Codex,
                detail: "this machine's Codex keeps its login in memory only \
                         (cli_auth_credentials_store = \"ephemeral\"), so there is nothing \
                         at rest to park"
                    .into(),
            });
        }
        store::read(&chain(ctx), paths::KEYCHAIN_SERVICE)
            .map(|found| found.map(|raw| Credential::new(ProviderId::Codex, raw)))
            .map_err(|e| ProviderError::Network {
                service: "this machine's credential store",
                detail: e.to_string(),
            })
    }

    /// Read out of the login itself, with no network call at all.
    ///
    /// The ID token is a JWT whose claims name the account, the email and the plan. Claude
    /// Code's equivalent costs a round trip on every switch because its local config can
    /// lag the credential by a day; Codex's cannot, because this is the credential.
    fn identify(&self, _ctx: &Context, credential: &Credential) -> Result<Identity, ProviderError> {
        let shape = |detail: &str| ProviderError::ShapeUnexpected {
            provider: ProviderId::Codex,
            detail: detail.to_string(),
        };
        let token = credential.raw["tokens"]["id_token"]
            .as_str()
            .ok_or_else(|| shape("it has no tokens.id_token"))?;
        let claims = jwt::claims(token).ok_or_else(|| shape("its id token is not readable"))?;
        let account_id = jwt::claim(&claims, &[OPENAI, "chatgpt_account_id"])
            .or_else(|| credential.raw["tokens"]["account_id"].as_str())
            .ok_or_else(|| shape("its id token names no account"))?;
        Ok(Identity {
            account_id: account_id.to_string(),
            email: jwt::claim(&claims, &["email"])
                .unwrap_or_default()
                .to_string(),
            // The workspace an account belongs to, where it belongs to one.
            group: jwt::claim(&claims, &[OPENAI, "chatgpt_workspace_id"]).map(str::to_owned),
        })
    }

    fn usage(&self, ctx: &Context, credential: &Credential) -> Result<Snapshot, ProviderError> {
        let shape = |detail: &str| ProviderError::ShapeUnexpected {
            provider: ProviderId::Codex,
            detail: detail.to_string(),
        };
        let tokens = &credential.raw["tokens"];
        let access = tokens["access_token"]
            .as_str()
            .ok_or_else(|| shape("it has no tokens.access_token"))?;
        let account = tokens["account_id"]
            .as_str()
            .ok_or_else(|| shape("it has no tokens.account_id"))?;
        api::usage(ctx, access, account, ctx.now())
    }

    /// Fresh tokens, folded back in the way Codex stores its own.
    ///
    /// `last_refresh` is written on every renewal, not only when the refresh token rotated.
    /// Codex matches on the field being present at all: without it, a login it would
    /// otherwise accept reads as "Token data is not available". It is an RFC 3339 string,
    /// not a number of milliseconds.
    fn renew(&self, ctx: &Context, credential: &Credential) -> Result<Credential, ProviderError> {
        let refresh = credential.raw["tokens"]["refresh_token"]
            .as_str()
            .ok_or_else(|| ProviderError::ShapeUnexpected {
                provider: ProviderId::Codex,
                detail: "it has no tokens.refresh_token".into(),
            })?;
        let fresh = api::renew(ctx, refresh)?;
        let mut next = credential.raw.clone();
        let tokens =
            next["tokens"]
                .as_object_mut()
                .ok_or_else(|| ProviderError::ShapeUnexpected {
                    provider: ProviderId::Codex,
                    detail: "its tokens are not an object".into(),
                })?;
        for (key, value) in [
            ("id_token", fresh.id_token),
            ("access_token", fresh.access_token),
            ("refresh_token", fresh.refresh_token),
        ] {
            if let Some(value) = value {
                tokens.insert(key.into(), Value::from(value));
            }
        }
        // Anchored to the clock of the answer that carried the tokens, where it sent one,
        // for the same reason the Claude Code side is: a machine running fast would
        // otherwise write a date that makes a fresh login read as stale.
        let at = fresh.at.unwrap_or_else(|| ctx.now());
        next["last_refresh"] = Value::from(stamp(at));
        Ok(Credential::new(ProviderId::Codex, next))
    }

    fn install_live(&self, ctx: &Context, credential: &Credential) -> Result<(), ProviderError> {
        let body = serde_json::to_string(&credential.raw)
            .expect("a credential document stays serialisable");
        store::write_raw(&chain(ctx), paths::KEYCHAIN_SERVICE, &body).map_err(|e| {
            ProviderError::Network {
                service: "this machine's credential store",
                detail: e.to_string(),
            }
        })
    }

    /// Nothing follows on its own.
    ///
    /// A running Codex holds its login in memory for the life of the process, watches no
    /// file, and refuses a reload whose account id has changed. There is no cache to expire
    /// and no window to wait out: the only way a session sees a switch is to be started
    /// again.
    fn adoption(&self) -> Adoption {
        Adoption::RestartRequired { program: "codex" }
    }

    /// A park may never be a copy.
    ///
    /// `codex login` and `codex logout` both revoke the stored refresh token at OpenAI
    /// before clearing it. A copy left live beside its twin in the vault is a token the
    /// person's own next login kills in both places at once.
    fn park_semantics(&self) -> ParkSemantics {
        ParkSemantics::MoveOnly
    }

    /// `CODEX_HOME` moves the file, and the keyring account is derived from that same home,
    /// so a scratch directory isolates both backends.
    fn private_signin_isolation(&self, ctx: &Context) -> Isolation {
        match paths::backend(ctx) {
            paths::Backend::Ephemeral => Isolation::NotIsolated {
                reason: "this machine's Codex keeps its login in memory only, so a sign-in \
                         here would leave nothing to enrol."
                    .into(),
            },
            _ => Isolation::Isolated,
        }
    }

    /// One account per file, so an account's share is the whole of it.
    ///
    /// Nothing in a Codex login belongs to the machine rather than the account: the tokens,
    /// the mode, the API key obtained during the same sign-in and the refresh stamp are all
    /// that account's. Keeping the whole document also means nothing pitboard does not
    /// recognise is ever dropped.
    fn slice(&self, live: &Value) -> Result<Value, ProviderError> {
        if !live.is_object() {
            return Err(ProviderError::ShapeUnexpected {
                provider: ProviderId::Codex,
                detail: "it is not a JSON object".into(),
            });
        }
        Ok(live.clone())
    }

    fn splice(&self, _live: &Value, incoming: &Value) -> Result<Value, ProviderError> {
        self.slice(incoming)
    }

    fn fingerprint(&self, slice: &Value) -> String {
        slice["tokens"]["refresh_token"]
            .as_str()
            .map(store::fingerprint)
            .unwrap_or_default()
    }

    /// When the access token expires, from its own claims.
    ///
    /// Codex states no refresh-token lifetime anywhere, so there is none to report.
    /// `None` reads as "no deadline known", which is what a park with an unknown refresh
    /// expiry should be treated as.
    fn expiry(&self, slice: &Value) -> Expiry {
        Expiry {
            access_expires_at: slice["tokens"]["access_token"]
                .as_str()
                .and_then(jwt::expires_at),
            refresh_expires_at: None,
        }
    }
}

/// An instant as Codex writes `last_refresh`: RFC 3339, UTC.
fn stamp(seconds: i64) -> String {
    jiff::Timestamp::from_second(seconds)
        .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(payload: &Value) -> String {
        // The same encoder the jwt tests use, kept local so this file needs nothing of
        // theirs.
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let part = |bytes: &[u8]| {
            let mut out = String::new();
            for chunk in bytes.chunks(3) {
                let mut held = 0u32;
                for (at, byte) in chunk.iter().enumerate() {
                    held |= u32::from(*byte) << (16 - 8 * at);
                }
                for at in 0..chunk.len().saturating_mul(8).div_ceil(6) {
                    out.push(char::from(
                        ALPHABET[((held >> (18 - 6 * at)) & 0x3f) as usize],
                    ));
                }
            }
            out
        };
        format!(
            "{}.{}.{}",
            part(b"{}"),
            part(payload.to_string().as_bytes()),
            part(b"sig")
        )
    }

    fn login() -> Value {
        serde_json::json!({
            "auth_mode": "chatgpt",
            "OPENAI_API_KEY": null,
            "tokens": {
                "id_token": token(&serde_json::json!({
                    "email": "a@b.c",
                    "exp": 1_789_935_600,
                    OPENAI: {"chatgpt_account_id": "acc-1", "chatgpt_plan_type": "pro"}
                })),
                "access_token": token(&serde_json::json!({"exp": 1_790_800_000i64})),
                "refresh_token": "a-refresh-token",
                "account_id": "acc-1"
            },
            "last_refresh": "2026-09-15T05:05:11.289384Z"
        })
    }

    #[test]
    fn the_account_is_read_out_of_the_login_with_no_network_call() {
        let found = Codex
            .identify(
                &Context::new(std::path::PathBuf::from("/nowhere")),
                &Credential::new(ProviderId::Codex, login()),
            )
            .expect("a login names its own account");
        assert_eq!(found.account_id, "acc-1");
        assert_eq!(found.email, "a@b.c");
    }

    /// One account per file, so nothing is filtered and nothing pitboard does not recognise
    /// is dropped on the way through.
    #[test]
    fn an_account_share_is_the_whole_file() {
        let live = login();
        assert_eq!(Codex.slice(&live).unwrap(), live);
        let incoming = serde_json::json!({"tokens": {"refresh_token": "another"}});
        assert_eq!(
            Codex.splice(&live, &incoming).unwrap(),
            incoming,
            "nothing of the outgoing account survives a switch"
        );
    }

    #[test]
    fn a_login_that_is_not_one_is_refused_rather_than_parked() {
        for bad in [
            serde_json::json!("a string"),
            serde_json::json!([1, 2, 3]),
            serde_json::json!(null),
        ] {
            assert!(Codex.slice(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_expiry_comes_from_the_access_token_and_there_is_no_refresh_deadline() {
        let expiry = Codex.expiry(&login());
        assert_eq!(expiry.access_expires_at, Some(1_790_800_000));
        assert_eq!(
            expiry.refresh_expires_at, None,
            "Codex states no refresh-token lifetime, so there is none to report"
        );
    }

    #[test]
    fn a_handle_on_the_refresh_token_is_stable_and_is_not_the_token() {
        let handle = Codex.fingerprint(&login());
        assert_eq!(handle.len(), 16);
        assert_eq!(handle, Codex.fingerprint(&login()));
        assert!(!handle.contains("a-refresh-token"));
        assert_eq!(
            Codex.fingerprint(&serde_json::json!({})),
            "",
            "a document with no refresh token has no handle, rather than a made up one"
        );
    }

    /// The two facts that make Codex different from Claude Code, asserted so a change to
    /// either is a change to a test.
    #[test]
    fn nothing_follows_a_codex_switch_and_a_park_is_never_a_copy() {
        assert_eq!(
            Codex.adoption(),
            Adoption::RestartRequired { program: "codex" }
        );
        assert_eq!(Codex.park_semantics(), ParkSemantics::MoveOnly);
    }

    /// `last_refresh` is a timestamp, not a number of milliseconds. Writing the wrong one
    /// leaves a login Codex reads as having no token data at all.
    #[test]
    fn the_refresh_stamp_is_written_the_way_codex_writes_it() {
        let written = stamp(1_789_935_600);
        assert!(written.ends_with('Z'), "{written}");
        assert_eq!(
            written.parse::<jiff::Timestamp>().unwrap().as_second(),
            1_789_935_600
        );
    }
}
