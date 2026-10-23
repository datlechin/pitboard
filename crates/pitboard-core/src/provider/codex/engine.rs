//! Codex CLI behind the provider boundary.
//!
//! Read from codex-cli 0.154.0: the binary on this machine and the matching public source.
//! Every fact this leans on is dated in [`super::assumptions`].

use super::{api, layers, paths};
use crate::context::Context;
use crate::host::Os;
use crate::provider::{
    Adoption, Behind, Credential, Expiry, Identity, Isolation, LiveStore, Owner, ParkSemantics,
    Provider, ProviderError, ProviderId, SignInView, jwt,
};
use crate::service::Permit;
use crate::store::{self, Live, RawStore};
use crate::usage::Snapshot;
use serde_json::Value;

/// The claim namespace OpenAI puts its own facts under in a standard token.
const OPENAI: &str = "https://api.openai.com/auth";

#[derive(Debug, Clone, Copy)]
pub(crate) struct Codex;

// The file store alone, on every system: `live` refuses every other one first.
fn chain(ctx: &Context) -> Live {
    Live::of(vec![Box::new(InPlace(
        ctx.host().file(paths::auth_file(ctx)),
    ))])
}

// Codex truncates `auth.json` and writes it again in place, with no lock, so a read can land
// between the two (`codex_windows_login_location`).
struct InPlace(Box<dyn RawStore>);

impl InPlace {
    const AGAIN: u32 = 10;
    const APART: std::time::Duration = std::time::Duration::from_millis(10);
}

impl RawStore for InPlace {
    fn kind(&self) -> store::Backend {
        self.0.kind()
    }

    fn contains(&self, service: &str) -> Result<bool, store::Error> {
        self.0.contains(service)
    }

    fn read(&self, service: &str) -> Result<Option<String>, store::Error> {
        let mut read = self.0.read(service)?;
        for _ in 0..Self::AGAIN {
            match &read {
                Some(text) if serde_json::from_str::<serde::de::IgnoredAny>(text).is_err() => {
                    std::thread::sleep(Self::APART);
                    read = self.0.read(service)?;
                }
                _ => break,
            }
        }
        Ok(read)
    }

    fn write(&self, permit: Permit, service: &str, contents: &str) -> Result<(), store::Error> {
        self.0.write(permit, service, contents)
    }

    fn delete(&self, permit: Permit, service: &str) -> Result<(), store::Error> {
        self.0.delete(permit, service)
    }

    fn list(&self) -> Result<Option<Vec<String>>, store::Error> {
        self.0.list()
    }

    fn cost(&self, service: &str, contents: &str) -> Option<store::Cost> {
        self.0.cost(service, contents)
    }
}

fn kept_elsewhere(ctx: &Context, store: &layers::Store) -> String {
    let kept = match (crate::host::OS, store.backend) {
        (Os::MacOs | Os::Linux, _) => "in the keychain".into(),
        (Os::Windows, paths::Backend::Secrets) => format!(
            "in {}, encrypted with a key it keeps in Credential Manager",
            paths::home(ctx)
                .join("secrets")
                .join("codex_auth.age")
                .display()
        ),
        (Os::Windows, paths::Backend::Either) => {
            "in Credential Manager, and in auth.json only where that fails".into()
        }
        (Os::Windows, _) => "in Credential Manager".into(),
    };
    format!(
        "this machine's Codex keeps its login {kept} ({}). {}",
        store.setting(),
        layers::file_store_only(crate::host::OS)
    )
}

impl Provider for Codex {
    fn id(&self) -> ProviderId {
        ProviderId::Codex
    }

    /// The file, where every layer Codex reads picks it ([`paths::store`]). Any other store
    /// is refused with the setting that chose it and what would choose the file instead: the
    /// line for the person's own `config.toml`, or that only an administrator can, where a
    /// requirement or a managed layer chose it. A store nobody can tell is refused too.
    fn live(&self, ctx: &Context) -> Result<LiveStore, ProviderError> {
        if let Some(path) = paths::missing_home(ctx) {
            return Err(ProviderError::CodexHomeMissing { path });
        }
        let unsupported = |reason: String| {
            Err(ProviderError::Unsupported {
                provider: ProviderId::Codex,
                reason,
            })
        };
        let store = paths::store(ctx);
        match store.backend {
            paths::Backend::File => Ok(LiveStore {
                chain: chain(ctx),
                // One file, so there is only one name in it.
                service: paths::AUTH_FILE.to_string(),
            }),
            paths::Backend::Ephemeral => unsupported(format!(
                "this machine's Codex keeps its login in memory only ({}), so there is \
                 nothing at rest to park or switch. {}",
                store.setting(),
                store.to_use_the_file(
                    crate::words::spelled(ctx, ProviderId::Codex).of(ProviderId::Codex)
                )
            )),
            paths::Backend::Keyring | paths::Backend::Either | paths::Backend::Secrets => {
                unsupported(format!(
                    "{}. {}",
                    kept_elsewhere(ctx, &store),
                    store.to_use_the_file(
                        crate::words::spelled(ctx, ProviderId::Codex).of(ProviderId::Codex)
                    )
                ))
            }
            paths::Backend::Unknown => unsupported(format!(
                "Pitboard cannot tell where this machine's Codex keeps its login: {}. {}",
                store.why_unknown().unwrap_or_default(),
                store.while_unknown().unwrap_or_default()
            )),
        }
    }

    /// Read out of the login itself, with no network call at all.
    ///
    /// The ID token is a JWT whose claims name the account, the email and the plan. Claude
    /// Code's equivalent costs a round trip on every switch because its local config can
    /// lag the credential by a day; Codex's cannot, because this is the credential.
    fn identify(&self, _ctx: &Context, credential: &Credential) -> Result<Owner, ProviderError> {
        let shape = |detail: &str| ProviderError::ShapeUnexpected {
            provider: ProviderId::Codex,
            detail: detail.to_string(),
        };
        let token = credential.raw["tokens"]["id_token"]
            .as_str()
            .ok_or_else(|| shape("it has no tokens.id_token"))?;
        let claims = jwt::claims(token).ok_or_else(|| shape("its id token is not readable"))?;
        let chatgpt = jwt::claim(&claims, &[OPENAI, "chatgpt_account_id"])
            .or_else(|| credential.raw["tokens"]["account_id"].as_str())
            .ok_or_else(|| shape("its id token names no account"))?;
        // The ChatGPT account is the plan, and a Team or Business plan is shared: two people
        // in one workspace carry the same one. The person is the user id inside it, and one
        // person with a personal plan and a workspace has the same user id in both. Only the
        // pair names one login's quota, so the pair is the identity.
        //
        // Joined with `_`, which appears in neither half and is safe everywhere an account
        // id goes, such as a park's name, which on Linux is a file name the vault checks. A
        // `:` there made every real Codex park unwritable on Linux.
        let person = jwt::claim(&claims, &[OPENAI, "chatgpt_user_id"])
            .or_else(|| jwt::claim(&claims, &[OPENAI, "user_id"]));
        Ok(Owner {
            account_uuid: match person {
                Some(person) => format!("{chatgpt}_{person}"),
                None => chatgpt.to_string(),
            },
            email: jwt::claim(&claims, &["email"])
                .unwrap_or_default()
                .to_string(),
            organization_uuid: chatgpt.to_string(),
        })
    }

    fn identity(&self, ctx: &Context, credential: &Credential) -> Result<Identity, ProviderError> {
        self.identify(ctx, credential).map(|owner| owner.identity())
    }

    fn name(
        &self,
        ctx: &Context,
        credential: &Credential,
        _identity: &Identity,
    ) -> Result<String, ProviderError> {
        self.identify(ctx, credential).map(|owner| owner.email)
    }

    /// The ID token names the account, read with no network call.
    fn identifies_by_itself(&self) -> bool {
        true
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
        let account_id = tokens["account_id"]
            .as_str()
            .ok_or_else(|| shape("it has no tokens.account_id"))?;
        let claims = tokens["id_token"].as_str().and_then(jwt::claims);
        let user_id = claims.as_ref().and_then(|claims| {
            jwt::claim(claims, &[OPENAI, "chatgpt_user_id"])
                .or_else(|| jwt::claim(claims, &[OPENAI, "user_id"]))
        });
        api::usage(
            ctx,
            access,
            api::Login {
                account_id,
                user_id,
            },
            ctx.now(),
        )
    }

    // At the issuer and as the client the login records, every field kept (`codex_renewal`).
    // `last_refresh` is written each time, as Codex reads a login without it as having no
    // token data.
    fn renew(
        &self,
        ctx: &Context,
        permit: Permit,
        credential: &Credential,
    ) -> Result<Credential, ProviderError> {
        let shape = |detail: String| ProviderError::ShapeUnexpected {
            provider: ProviderId::Codex,
            detail,
        };
        let tokens = &credential.raw["tokens"];
        let refresh = tokens["refresh_token"]
            .as_str()
            .ok_or_else(|| shape("it has no tokens.refresh_token".into()))?;
        let authority = api::Authority::of(tokens).map_err(shape)?;
        let fresh = api::renew(ctx, permit, refresh, &authority).map_err(|e| match e {
            ProviderError::InvalidGrant { .. } => other_client(tokens, &authority).unwrap_or(e),
            other => other,
        })?;
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

    /// The ID token names the account whether or not OpenAI still accepts the login, so
    /// that is asked separately, with the cheapest request that answers it: the same usage
    /// read `status` makes, which spends no quota.
    fn verify(&self, ctx: &Context, credential: &Credential) -> Result<Identity, ProviderError> {
        let found = self.identity(ctx, credential)?;
        self.usage(ctx, credential)?;
        Ok(found)
    }

    fn slot(&self, ctx: &Context) -> String {
        paths::auth_file(ctx).to_string_lossy().into_owned()
    }

    /// Codex writes its login with a plain truncating write and takes no lock of any kind,
    /// so there is none for Pitboard to share.
    fn write_lock(&self, _ctx: &Context) -> Option<std::path::PathBuf> {
        None
    }

    /// Nor does a running `codex` take one to renew its login.
    fn refresh_lock(&self, _ctx: &Context) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
        None
    }

    /// Codex keeps no record apart from its login, whose ID token names the account.
    fn own_record(&self, _ctx: &Context) -> Option<Owner> {
        None
    }

    /// Nothing to correct: Codex caches no identity apart from the login itself.
    fn after_switch(
        &self,
        _ctx: &Context,
        _: Permit,
        _incoming: &crate::state::Account,
        _outgoing: &Owner,
    ) -> Result<(), crate::error::Error> {
        Ok(())
    }

    fn program(&self, ctx: &Context) -> Option<std::path::PathBuf> {
        crate::provider::program_of(ctx, ProviderId::Codex)
    }

    // The `-c` keeps the new login in the file, over every layer but a requirement or a managed
    // one, which `Codex::live` refuses first (`codex_login_takes_a_store_override`).
    fn sign_in(
        &self,
        ctx: &Context,
        permit: Permit,
        dir: &std::path::Path,
    ) -> std::io::Result<std::process::Command> {
        let mut command = crate::provider::command(ctx, permit, ProviderId::Codex)?;
        command
            .args(["-c", layers::FILE_STORE, "login"])
            .env("CODEX_HOME", dir)
            .current_dir(dir);
        Ok(command)
    }

    fn read_signin(
        &self,
        ctx: &Context,
        dir: &std::path::Path,
    ) -> Result<Option<String>, store::Error> {
        let private = ctx
            .clone()
            .with_codex_home(dir.to_string_lossy().into_owned());
        store::read_raw(&chain(&private), paths::AUTH_FILE)
    }

    /// Everything a sign-in writes goes inside its home, so the directory is all there is.
    fn discard_signin(&self, _ctx: &Context, _: Permit, _dir: &std::path::Path) {}

    /// Read from 0.160.0. `codex login` prints to stderr where its loopback server listens,
    /// `http://localhost:<port>`, and then, bare on a line of its own, the `https` address
    /// on auth.openai.com to open when the browser did not. So the first `https` address is
    /// the one to open. It reads nothing typed back, whatever it prints.
    fn read_sign_in(&self, said: &str) -> SignInView {
        SignInView {
            url: crate::provider::https_address(said),
            wants_code: false,
        }
    }

    /// It reads nothing typed back, so it refuses nothing, whatever it prints.
    fn refused_code(&self, _since: &str) -> bool {
        false
    }

    /// Nothing measured yet. Codex reads an API key from its own login document, which moves
    /// with the account, and whether an environment key takes precedence over a ChatGPT
    /// login in 0.154.0 has not been read closely enough to warn about.
    fn overridden_by(&self, _ctx: &Context) -> Vec<String> {
        Vec::new()
    }

    fn refuses(&self, ctx: &Context, document: &Value, _owner: &Owner) -> Option<String> {
        let tokens = &document["tokens"];
        let claimed = tokens["id_token"]
            .as_str()
            .and_then(jwt::claims)
            .and_then(|claims| {
                jwt::claim(&claims, &[OPENAI, "chatgpt_account_id"]).map(str::to_owned)
            });
        paths::restrictions(ctx).refuses(claimed.as_deref(), tokens["account_id"].as_str())
    }

    /// Codex keeps its login in one store, with nothing behind it.
    fn behind(&self, _ctx: &Context) -> Option<Behind> {
        None
    }

    /// Nothing follows on its own.
    ///
    /// A running Codex holds its login in memory for the life of the process, watches no
    /// file, and refuses a reload whose account id has changed. There is no cache to expire
    /// and no window to wait out: the only way a session sees a switch is to be started
    /// again, and what starts it again depends on where it runs, which is [`holders`].
    ///
    /// [`holders`]: super::holders::HOLDERS
    fn adoption(&self, _behind: Option<&Behind>) -> Adoption {
        Adoption::RestartRequired {
            program: "codex",
            holders: super::holders::HOLDERS,
        }
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
    ///
    /// What is asked is the store the sign-in itself ends up with ([`paths::sign_in_store`]):
    /// its `-c` names the file over every setting of the person's own, so only an
    /// administrator's can keep its login in memory, where it would leave nothing to enrol.
    fn private_signin_isolation(&self, ctx: &Context) -> Isolation {
        let store = paths::sign_in_store(ctx);
        match store.backend {
            paths::Backend::Ephemeral => Isolation::NotIsolated {
                reason: format!(
                    "this machine's Codex keeps its login in memory only ({}), so a sign-in \
                     here would leave nothing to enrol.",
                    store.setting()
                ),
            },
            _ => Isolation::Isolated,
        }
    }

    /// `chatgpt_plan_type` out of the ID token: `pro` is "Pro".
    fn plan(&self, document: &Value) -> Option<String> {
        let claims = jwt::claims(document["tokens"]["id_token"].as_str()?)?;
        Some(jwt::claim(&claims, &[OPENAI, "chatgpt_plan_type"])?.trim())
            .filter(|kind| !kind.is_empty())
            .map(crate::holder::capitalised)
    }

    /// One account per file, so an account's share is the whole of it.
    ///
    /// Nothing in a Codex login belongs to the machine rather than the account: the tokens,
    /// the mode, the API key obtained during the same sign-in and the refresh stamp are all
    /// that account's. Keeping the whole document also means nothing Pitboard does not
    /// recognise is ever dropped.
    fn slice(&self, live: &Value) -> Result<Value, ProviderError> {
        let shape = |detail: &str| {
            Err(ProviderError::ShapeUnexpected {
                provider: ProviderId::Codex,
                detail: detail.to_string(),
            })
        };
        if !live.is_object() {
            return shape("it is not a JSON object");
        }
        // Signed in with an API key rather than a ChatGPT account: there is no account and
        // no refresh chain, so nothing to park, renew or switch.
        if !live["tokens"].is_object() {
            return Err(ProviderError::Unsupported {
                provider: ProviderId::Codex,
                reason: "Codex is signed in with an API key rather than a ChatGPT account, \
                         so there is no account login to park or switch"
                    .into(),
            });
        }
        // A login whose tokens belong to one account and whose account id names another is
        // not one account's login. Codex until 0.163.0 leaves exactly that when a session still
        // running from before a switch refreshes during one: it re-reads the file, keeps the account
        // id it finds and writes its own account's tokens over the rest. Parking it, or
        // taking it as proof a switch held, would file one account's tokens under the other.
        let stored = live["tokens"]["account_id"].as_str();
        let claimed = live["tokens"]["id_token"]
            .as_str()
            .and_then(jwt::claims)
            .and_then(|claims| {
                jwt::claim(&claims, &[OPENAI, "chatgpt_account_id"]).map(str::to_owned)
            });
        if let (Some(stored), Some(claimed)) = (stored, claimed)
            && stored != claimed
        {
            return shape(
                "its tokens belong to one ChatGPT account and its account id names another, \
                 which is what a codex still running from before a switch leaves when it \
                 refreshes in the middle of one",
            );
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

// Where OpenAI refused a login for good and its access token names another client than the
// one asked, which only a login that records none can be renewed as.
fn other_client(tokens: &Value, asked: &api::Authority) -> Option<ProviderError> {
    let claims = jwt::claims(tokens["access_token"].as_str()?)?;
    let named = jwt::claim(&claims, &["client_id"]).or_else(|| jwt::claim(&claims, &["azp"]))?;
    (named != asked.client_id).then(|| ProviderError::OtherClient {
        detail: format!(
            "OpenAI refused to renew it as the client `{}`, and its access token names the \
             client `{named}`",
            asked.client_id
        ),
    })
}

/// An instant as Codex writes `last_refresh`: RFC 3339, UTC.
fn stamp(seconds: i64) -> String {
    jiff::Timestamp::from_second(seconds)
        .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
        .to_string()
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;

    fn token(payload: &Value) -> String {
        jwt::unsigned(payload)
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
        assert_eq!(found.account_uuid, "acc-1");
        assert_eq!(found.email, "a@b.c");
    }

    #[test]
    fn the_plan_is_read_out_of_the_id_token() {
        assert_eq!(Codex.plan(&login()).as_deref(), Some("Pro"));
        assert_eq!(Codex.plan(&serde_json::json!({"tokens": {}})), None);
    }

    /// One account per file, so nothing is filtered and nothing Pitboard does not recognise
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

    /// Two people in one Business workspace carry the same ChatGPT account id. Identified
    /// by it alone, the second would be refused as already enrolled and their readings
    /// merged; the person inside the workspace is what tells them apart.
    #[test]
    fn two_people_in_one_workspace_are_two_accounts() {
        let person = |user: &str| {
            let mut login = login();
            login["tokens"]["id_token"] = Value::from(token(&serde_json::json!({
                "email": format!("{user}@b.c"),
                OPENAI: {"chatgpt_account_id": "team", "chatgpt_user_id": user},
            })));
            login["tokens"]["account_id"] = Value::from("team");
            Codex
                .identify(
                    &Context::new(std::path::PathBuf::from("/nowhere")),
                    &Credential::new(ProviderId::Codex, login),
                )
                .expect("identified")
        };
        let (one, two) = (person("user-1"), person("user-2"));
        assert_ne!(one.account_uuid, two.account_uuid);
        assert_eq!(one.organization_uuid, "team");
        assert_eq!(one.account_uuid, "team_user-1");
    }

    /// A login whose tokens name one account and whose account id names another is what a
    /// stale codex leaves when it refreshes during a switch. It is not one account's login.
    #[test]
    fn a_login_mixing_two_accounts_is_not_an_accounts_share() {
        let mut mixed = login();
        mixed["tokens"]["account_id"] = Value::from("someone-else");
        let refused = Codex.slice(&mixed).expect_err("refused");
        assert!(refused.to_string().contains("two") || refused.to_string().contains("another"));
        assert!(Codex.slice(&login()).is_ok());
    }

    // `codex login` revokes whatever its home holds, so never the real one.
    #[test]
    fn a_sign_in_is_codex_login_in_the_private_directory() {
        let dir = std::path::Path::new("/tmp/pitboard-signin-scratch");
        // Never run: a program that is there on every system.
        let codex = std::env::current_exe().expect("this test");
        let ctx =
            Context::new(std::path::PathBuf::from("/nowhere")).with_codex_program(codex.clone());
        let command = Codex
            .sign_in(&ctx, Permit::for_a_test(), dir)
            .expect("found");
        assert_eq!(command.get_program(), codex.as_os_str());
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["-c", "cli_auth_credentials_store=\"file\"", "login"]
        );
        assert_eq!(command.get_current_dir(), Some(dir));
        let home = command
            .get_envs()
            .find(|(name, _)| *name == "CODEX_HOME")
            .and_then(|(_, value)| value);
        assert_eq!(home, Some(dir.as_os_str()), "always set, never empty");
    }

    /// What a sign-in left is read from the private directory's own `auth.json`, whatever
    /// `CODEX_HOME` this process has.
    #[test]
    fn a_sign_in_is_read_back_from_the_private_directory() {
        let dir = std::env::temp_dir().join(format!(
            "pitboard-codex-readback-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("a scratch home");
        std::fs::write(dir.join("auth.json"), login().to_string()).expect("a login");
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"))
            .with_codex_home("/somewhere/else".into());
        let read = Codex.read_signin(&ctx, &dir).expect("readable");
        assert_eq!(
            read.map(|raw| serde_json::from_str::<Value>(&raw).unwrap()),
            Some(login())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A file caught empty `torn` times, then whole.
    struct CaughtPartWay {
        torn: u32,
        reads: std::sync::atomic::AtomicU32,
    }

    impl RawStore for CaughtPartWay {
        fn kind(&self) -> store::Backend {
            store::Backend::File
        }

        fn contains(&self, _: &str) -> Result<bool, store::Error> {
            Ok(true)
        }

        fn read(&self, _: &str) -> Result<Option<String>, store::Error> {
            let read = self
                .reads
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(Some(if read < self.torn {
                String::new()
            } else {
                login().to_string()
            }))
        }

        fn write(&self, _: Permit, _: &str, _: &str) -> Result<(), store::Error> {
            unreachable!("only read")
        }

        fn delete(&self, _: Permit, _: &str) -> Result<(), store::Error> {
            unreachable!("only read")
        }
    }

    #[test]
    fn a_login_caught_part_way_is_read_again_ten_times_ten_ms_apart() {
        for (torn, whole) in [(0, true), (1, true), (10, true), (11, false)] {
            let started = std::time::Instant::now();
            let read = InPlace(Box::new(CaughtPartWay {
                torn,
                reads: std::sync::atomic::AtomicU32::new(0),
            }))
            .read("auth.json")
            .expect("read")
            .expect("there");
            assert_eq!(
                serde_json::from_str::<Value>(&read).ok() == Some(login()),
                whole,
                "{torn}"
            );
            let waited = started.elapsed();
            assert!(
                waited >= InPlace::APART * torn.min(InPlace::AGAIN),
                "{torn}: {waited:?}"
            );
        }
    }

    // As Codex 0.163.0 writes it: truncated, then written and flushed in place, with std's
    // default sharing, here with a pause in the middle that a real write leaves too short to see.
    // On a busy machine a read can still catch it part way eleven times running, which fails
    // here only where that outnumbers the whole reads; a torn login always fails.
    #[test]
    fn a_login_codex_is_writing_in_place_is_never_read_torn() {
        use std::io::Write as _;
        use std::sync::atomic::{AtomicBool, Ordering};
        let home = tempfile::tempdir().expect("a scratch Codex home");
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"))
            .with_codex_home(home.path().to_string_lossy().into_owned());
        let file = paths::auth_file(&ctx);
        let logins: Vec<Value> = ["one", "two"]
            .iter()
            .map(|refresh| {
                let mut login = login();
                login["tokens"]["refresh_token"] = Value::from(*refresh);
                login
            })
            .collect();
        let texts: Vec<String> = logins
            .iter()
            .map(|login| serde_json::to_string_pretty(login).expect("JSON"))
            .collect();
        std::fs::write(&file, &texts[0]).expect("a login");
        let writing = std::sync::Arc::new(AtomicBool::new(true));
        let writer = {
            let (file, texts, writing) = (file.clone(), texts.clone(), writing.clone());
            std::thread::spawn(move || {
                for round in 0..200 {
                    let text = texts[round % 2].as_bytes();
                    let mut open = std::fs::OpenOptions::new()
                        .truncate(true)
                        .write(true)
                        .create(true)
                        .open(&file)
                        .expect("Codex's file");
                    open.write_all(&text[..text.len() / 2]).expect("half");
                    open.flush().expect("flushed");
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    open.write_all(&text[text.len() / 2..]).expect("the rest");
                    open.flush().expect("flushed");
                    drop(open);
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                writing.store(false, Ordering::Release);
            })
        };
        let (mut whole, mut unread) = (0, Vec::new());
        while writing.load(Ordering::Acquire) {
            match Codex.read_live(&ctx) {
                Ok(Some(read)) => {
                    assert!(logins.contains(&read.raw), "a torn login: {}", read.raw);
                    whole += 1;
                }
                Ok(None) => panic!("Codex never takes its file away"),
                Err(e) => unread.push(e.to_string()),
            }
        }
        writer.join().expect("the writer");
        assert!(whole > 0);
        assert!(
            unread.len() < whole,
            "{} reads caught it part way, the first: {:?}",
            unread.len(),
            unread.first()
        );
    }

    // Codex does not start there, so nothing under it is read.
    #[test]
    fn a_codex_home_that_names_no_folder_is_refused_on_windows() {
        use crate::context::{Context as Read, Environment};
        let missing = std::env::temp_dir().join("pitboard-codex-never-made");
        let named = missing.to_string_lossy().into_owned();
        let env = Environment::on(
            Os::Windows,
            [("USERPROFILE", "/nowhere"), ("CODEX_HOME", named.as_str())],
        );
        let refused = Codex.live(&Read::read(&env, "cli")).err().expect("refused");
        let ProviderError::CodexHomeMissing { path } = &refused else {
            panic!("refused otherwise: {refused:?}");
        };
        assert_eq!(path, &missing);
        assert_eq!(
            refused.to_string(),
            format!(
                "CODEX_HOME is {}, which is no folder, so Codex does not start. Make that \
                 folder, or unset CODEX_HOME",
                missing.display()
            )
        );
    }

    /// A keychain store is refused with a reason rather than read: every read of an item
    /// Codex made for itself would put a permission prompt in front of the person. On Windows
    /// it is a store Codex keeps its login in only when told to, which Pitboard does not follow.
    #[test]
    fn a_keychain_store_is_refused_with_a_reason_and_never_read() {
        let dir = std::env::temp_dir().join(format!(
            "pitboard-codex-keyring-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("a scratch home");
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"))
            .with_codex_home(dir.to_string_lossy().into_owned());
        let config = paths::config_file(&ctx);
        let refused = |written: &str| {
            std::fs::write(dir.join("config.toml"), written).expect("a config");
            let refused = Codex.live(&ctx).err().expect("refused");
            assert!(matches!(refused, ProviderError::Unsupported { .. }));
            refused.to_string()
        };
        let keyring = "cli_auth_credentials_store = \"keyring\"\n";
        let to_use_the_file = format!(
            "To use the file store, set `cli_auth_credentials_store = \"file\"` in {}, then \
             sign in again with `codex login`",
            config.display()
        );
        let setting = format!(
            "`cli_auth_credentials_store = \"keyring\"` in {}",
            config.display()
        );
        match crate::host::OS {
            Os::MacOs | Os::Linux => assert_eq!(
                refused(keyring),
                format!(
                    "this machine's Codex keeps its login in the keychain ({setting}). Pitboard \
                     handles Codex's file store, auth.json, and does not read an item Codex \
                     created for itself, because every read would ask you for permission. \
                     {to_use_the_file}"
                )
            ),
            Os::Windows => {
                let not_followed = "Pitboard handles Codex's file store, auth.json, and does \
                                    not follow Codex into a store it uses only when told to";
                assert_eq!(
                    refused(keyring),
                    format!(
                        "this machine's Codex keeps its login in {}, encrypted with a key it \
                         keeps in Credential Manager ({setting}, with `secret_auth_storage` on \
                         by default). {not_followed}. {to_use_the_file}",
                        paths::home(&ctx)
                            .join("secrets")
                            .join("codex_auth.age")
                            .display()
                    )
                );
                let off = "[features]\nsecret_auth_storage = false\n";
                assert_eq!(
                    refused(&format!("{keyring}{off}")),
                    format!(
                        "this machine's Codex keeps its login in Credential Manager ({setting}). \
                         {not_followed}. {to_use_the_file}"
                    )
                );
                assert!(
                    refused(&format!("cli_auth_credentials_store = \"auto\"\n{off}")).starts_with(
                        "this machine's Codex keeps its login in Credential Manager, and in \
                             auth.json only where that fails ("
                    )
                );
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A store pinned by this machine's `requirements.toml` is refused naming that file, since
    /// no line in the person's own config can change it, and so is a sign-in under it. One
    /// that cannot be read is refused rather than taken for the file.
    #[test]
    fn a_store_an_administrator_set_is_refused_naming_where() {
        let host = crate::host::memory::MemoryHost::new();
        let home = tempfile::tempdir().expect("a Codex home, which Windows' Codex needs");
        let ctx = Context::for_unit_test()
            .with_codex_home(home.path().to_string_lossy().into_owned())
            .with_memory_stores(host.clone());
        let required = layers::system_file(&ctx, "requirements.toml");
        let named = required.display().to_string();
        assert!(Codex.live(&ctx).is_ok(), "nothing set is the file");
        host.administers(
            required.clone(),
            "cli_auth_credentials_store = \"ephemeral\"\n",
        );
        let refused = Codex.live(&ctx).err().expect("refused").to_string();
        assert_eq!(
            refused,
            format!(
                "this machine's Codex keeps its login in memory only (pinned to `ephemeral` by \
                 {named}), so there is nothing at rest to park or switch. No line in your own \
                 config.toml can change it: {named} pins it, and only an administrator can \
                 change that"
            )
        );
        assert!(matches!(
            Codex.private_signin_isolation(&ctx),
            Isolation::NotIsolated { reason } if reason.contains(&named)
        ));

        host.administers(required, "[[");
        let refused = Codex.live(&ctx).err().expect("refused").to_string();
        assert_eq!(
            refused,
            format!(
                "Pitboard cannot tell where this machine's Codex keeps its login: {named} is \
                 not TOML Codex can read, at line 1. Codex 0.160.0 does not start until that is \
                 put right"
            )
        );
    }

    /// A `profile = "<name>"` line chooses its profile, whose table holds no store Codex
    /// reads, so where the layers choose the file the live login is the file's, where it was
    /// refused for the line. A line naming a profile no table defines is refused, saying
    /// what each Codex does with it.
    #[test]
    fn a_profile_line_is_read_as_choosing_its_profile() {
        let host = crate::host::memory::MemoryHost::new();
        let home = tempfile::tempdir().expect("a Codex home, which Windows' Codex needs");
        let ctx = Context::for_unit_test()
            .with_codex_home(home.path().to_string_lossy().into_owned())
            .with_memory_stores(host.clone());
        let system = layers::system_file(&ctx, "config.toml");
        host.administers(
            system.clone(),
            "profile = \"work\"\n[profiles.work]\ncli_auth_credentials_store = \"keyring\"\n",
        );
        assert!(
            Codex.live(&ctx).is_ok(),
            "a chosen profile, its store not read"
        );

        host.administers(system.clone(), "profile = \"work\"\n");
        let refused = Codex.live(&ctx).err().expect("refused").to_string();
        assert_eq!(
            refused,
            format!(
                "Pitboard cannot tell where this machine's Codex keeps its login: {} sets \
                 `profile = \"work\"`, a profile no `[profiles.work]` table defines. Codex \
                 0.160.0 does not start with that line, and an older Codex, such as 0.99.0, \
                 which chooses a profile with it, refuses one no table defines: \"config \
                 profile `work` not found\"",
                system.display()
            )
        );
    }

    /// Where `/etc/codex/managed_config.toml` or this machine's `requirements.toml` chooses
    /// the file over a keychain or memory store in the person's own `config.toml`, Codex keeps
    /// its login in `auth.json`, so the account is switched there rather than refused, as it
    /// was while Pitboard read the person's `config.toml` alone. Windows reads no
    /// `managed_config.toml`.
    #[test]
    fn a_file_store_an_administrator_set_is_over_the_persons_own() {
        let dir = std::env::temp_dir().join(format!(
            "pitboard-codex-over-own-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("a scratch home");
        let ctx_for = |host: &std::sync::Arc<crate::host::memory::MemoryHost>| {
            Context::for_unit_test()
                .with_codex_home(dir.to_string_lossy().into_owned())
                .with_memory_stores(host.clone())
        };
        for own in ["keyring", "auto", "ephemeral"] {
            std::fs::write(
                dir.join("config.toml"),
                format!("cli_auth_credentials_store = \"{own}\"\n"),
            )
            .expect("a config");
            let overs: &[&str] = match crate::host::OS {
                Os::MacOs | Os::Linux => &["managed_config.toml", "requirements.toml"],
                Os::Windows => &["requirements.toml"],
            };
            for over in overs {
                let host = crate::host::memory::MemoryHost::new();
                let ctx = ctx_for(&host);
                assert!(Codex.live(&ctx).is_err(), "{own} alone is refused");
                host.administers(
                    layers::system_file(&ctx, over),
                    "cli_auth_credentials_store = \"file\"\n",
                );
                let live = Codex.live(&ctx).expect("the file, which Pitboard switches");
                assert_eq!(live.service, paths::AUTH_FILE, "{own} under {over}");
                assert_eq!(Codex.private_signin_isolation(&ctx), Isolation::Isolated);
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A store of the person's own does not reach a sign-in, whose `-c` names the file over
    /// it, so the sign-in is isolated where the live login is refused.
    #[test]
    fn a_store_of_the_persons_own_does_not_reach_a_sign_in() {
        let dir = std::env::temp_dir().join(format!(
            "pitboard-codex-own-ephemeral-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("a scratch home");
        std::fs::write(
            dir.join("config.toml"),
            "cli_auth_credentials_store = \"ephemeral\"\n",
        )
        .expect("a config");
        let ctx = Context::for_unit_test().with_codex_home(dir.to_string_lossy().into_owned());
        assert!(Codex.live(&ctx).is_err());
        assert_eq!(Codex.private_signin_isolation(&ctx), Isolation::Isolated);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The two facts that make Codex different from Claude Code, asserted so a change to
    /// either is a change to a test.
    #[test]
    fn nothing_follows_a_codex_switch_and_a_park_is_never_a_copy() {
        assert!(matches!(
            Codex.adoption(None),
            Adoption::RestartRequired {
                program: "codex",
                ..
            }
        ));
        assert_eq!(Codex.park_semantics(), ParkSemantics::MoveOnly);
    }

    /// What `codex login` 0.160.0 prints to stderr once its loopback server listens, with
    /// fewer of the address's parameters.
    const SAID: &str = "Starting local login server on http://localhost:1455.\n\
        If your browser did not open, navigate to this URL to authenticate:\n\n\
        https://auth.openai.com/oauth/authorize?response_type=code\
        &client_id=app_EMoamEEZ73f0CkXaXp7hrann\
        &redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback&state=s\n\n\
        On a remote or headless machine? Use `codex login --device-auth` instead.\n";

    /// The address to open is the one on auth.openai.com, not the loopback one printed
    /// first, and no code is ever asked for.
    #[test]
    fn a_sign_in_offers_the_address_to_open_and_never_a_code() {
        let read = Codex.read_sign_in(SAID);
        assert_eq!(
            read.url.as_deref(),
            Some(
                "https://auth.openai.com/oauth/authorize?response_type=code\
                 &client_id=app_EMoamEEZ73f0CkXaXp7hrann\
                 &redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback&state=s"
            )
        );
        assert!(!read.wants_code);
        assert!(
            !Codex
                .read_sign_in(&format!("{SAID}Paste code here if prompted > "))
                .wants_code
        );
        assert!(!Codex.refused_code("Invalid code. Please make sure the full code was copied.\n"));
    }

    /// The loopback line the conformance run looks for is one this passes over, so a build
    /// that printed its loopback as `https` first would be reported rather than offered.
    #[test]
    fn the_loopback_address_in_the_register_is_not_one_to_open() {
        let fact = crate::assumptions::named("codex_login_prints_its_address").expect("listed");
        let loopback = fact
            .probe
            .iter()
            .find(|p| p.starts_with("Starting local login server"))
            .expect("the loopback line is probed for");
        assert_eq!(
            crate::provider::https_address(&format!("{loopback}1455.")),
            None
        );
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
