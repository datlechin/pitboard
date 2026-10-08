//! Codex CLI behind the provider boundary.
//!
//! Read from codex-cli 0.154.0: the binary on this machine and the matching public source.
//! Every fact this leans on is dated in [`super::assumptions`].

use super::{api, paths};
use crate::context::Context;
use crate::host::Os;
use crate::provider::{
    Adoption, Behind, Credential, Expiry, Identity, Isolation, LiveStore, ParkSemantics, Provider,
    ProviderError, ProviderId, SignInView, jwt,
};
use crate::service::Permit;
use crate::store::{self, Backend, Live, Unbuilt};
use crate::usage::Snapshot;
use serde_json::Value;

/// The claim namespace OpenAI puts its own facts under in a standard token.
const OPENAI: &str = "https://api.openai.com/auth";

#[derive(Debug, Clone, Copy)]
pub(crate) struct Codex;

/// Where Codex keeps its login, when Pitboard can act on it.
///
/// Only the default store: the file. Codex's keyring store files the login in a keychain
/// item that Codex created through the Security framework, whose access list trusts the
/// `codex` binary alone, so every read Pitboard made would put a keychain prompt in front
/// of the person, from `status` as much as from a switch. Pressing Always Allow would
/// change Codex's own item. The honest answer is to say Pitboard does not handle that
/// store, rather than to prompt on every refresh of a menu bar.
///
/// On Windows it is one store that refuses every call, until W21 reads which store Codex
/// keeps its login in there, from layers of its own and with `secret_auth_storage` on by
/// default (the register's pending `codex_store_layers` and `codex_login_location`), and
/// switches it where that is the file.
fn chain(ctx: &Context) -> Live {
    match crate::host::OS {
        Os::MacOs | Os::Linux => Live::of(vec![ctx.host().file(paths::auth_file(ctx))]),
        Os::Windows => Live::of(vec![Box::new(Unbuilt::new(
            Backend::Unknown,
            "Pitboard does not switch Codex on Windows yet",
        ))]),
    }
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
                store.to_use_the_file()
            )),
            paths::Backend::Keyring | paths::Backend::Either | paths::Backend::Secrets => {
                unsupported(format!(
                    "this machine's Codex keeps its login in the keychain ({}). Pitboard \
                     handles Codex's file store, auth.json, and does not read an item Codex \
                     created for itself, because every read would ask you for permission. {}",
                    store.setting(),
                    store.to_use_the_file()
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
    fn identify(&self, _ctx: &Context, credential: &Credential) -> Result<Identity, ProviderError> {
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
        Ok(Identity {
            account_id: match person {
                Some(person) => format!("{chatgpt}_{person}"),
                None => chatgpt.to_string(),
            },
            email: jwt::claim(&claims, &["email"])
                .unwrap_or_default()
                .to_string(),
            group: Some(chatgpt.to_string()),
        })
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
    fn renew(
        &self,
        ctx: &Context,
        permit: Permit,
        credential: &Credential,
    ) -> Result<Credential, ProviderError> {
        let refresh = credential.raw["tokens"]["refresh_token"]
            .as_str()
            .ok_or_else(|| ProviderError::ShapeUnexpected {
                provider: ProviderId::Codex,
                detail: "it has no tokens.refresh_token".into(),
            })?;
        let fresh = api::renew(ctx, permit, refresh)?;
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
        let found = self.identify(ctx, credential)?;
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

    /// The login is its own record: its ID token names the account, and Codex keeps no
    /// other file saying who is signed in.
    fn recorded_identity(&self, ctx: &Context) -> Option<Identity> {
        let live = self.read_live(ctx).ok()??;
        self.identify(ctx, &live).ok()
    }

    /// Nothing to correct: Codex caches no identity apart from the login itself.
    fn after_switch(
        &self,
        _ctx: &Context,
        _: Permit,
        _incoming: &crate::state::Account,
        _outgoing: &Identity,
    ) -> Result<(), crate::error::Error> {
        Ok(())
    }

    fn program(&self, ctx: &Context) -> Option<std::path::PathBuf> {
        crate::provider::program_of(ctx, ProviderId::Codex)
    }

    /// `codex -c cli_auth_credentials_store="file" login` with `CODEX_HOME` pointed at the
    /// private directory.
    ///
    /// Read from 0.154.0. It revokes whatever login is stored in the home it is given
    /// before it signs in, which in an empty directory is nothing; run against the real
    /// home it would end the account in use, which is why the directory is always set and
    /// never empty. It opens the browser itself, prints the address to stderr for when it
    /// cannot, listens for the callback on a loopback port, and reads nothing from stdin.
    ///
    /// The `-c` names the file store, read from 0.160.0 (`codex_login_takes_a_store_override`
    /// in the register): given before `login`, where Codex's own command line takes it, it is
    /// over the private home's config, which holds none, over `/etc/codex/config.toml` and over
    /// a trusted project's `.codex/config.toml`. Without it, either could send the new login to
    /// the keychain, where this could not read it back. Only a requirement or a managed layer
    /// is over it, and one that chooses another store chooses the live store too, which
    /// [`Codex::live`] refuses before a sign-in starts.
    ///
    /// Started from inside the directory too, so no project's config is read at all.
    fn sign_in(
        &self,
        ctx: &Context,
        permit: Permit,
        dir: &std::path::Path,
    ) -> std::process::Command {
        let mut command = crate::provider::command(ctx, permit, ProviderId::Codex);
        command
            .args(["-c", super::layers::FILE_STORE, "login"])
            .env("CODEX_HOME", dir)
            .current_dir(dir);
        command
    }

    fn read_signin(
        &self,
        ctx: &Context,
        dir: &std::path::Path,
    ) -> Result<Option<String>, crate::store::Error> {
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
        // not one account's login. Codex leaves exactly that when a session still running
        // from before a switch refreshes during one: it re-reads the file, keeps the account
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
        assert_eq!(found.account_id, "acc-1");
        assert_eq!(found.email, "a@b.c");
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
        assert_ne!(one.account_id, two.account_id);
        assert_eq!(one.group.as_deref(), Some("team"));
        assert_eq!(one.account_id, "team_user-1");
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

    /// The sign-in runs Codex's own login against the private directory, from inside it,
    /// and never against the real home: `codex login` revokes whatever it finds stored in
    /// the home it is given before it signs in. It names the file store before `login`, where
    /// Codex's own command line takes a `-c`, so no setting of `/etc/codex` or of a project
    /// sends the new login where it cannot be read back.
    #[test]
    fn a_sign_in_is_codex_login_in_the_private_directory() {
        let dir = std::path::Path::new("/tmp/pitboard-signin-scratch");
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"))
            .with_codex_program(std::path::PathBuf::from("/opt/codex/bin/codex"));
        let command = Codex.sign_in(&ctx, Permit::for_a_test(), dir);
        assert_eq!(command.get_program(), "/opt/codex/bin/codex");
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
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
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

    /// A keychain store is refused with a reason rather than read: every read of an item
    /// Codex made for itself would put a permission prompt in front of the person.
    #[test]
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
    fn a_keychain_store_is_refused_with_a_reason_and_never_read() {
        let dir = std::env::temp_dir().join(format!(
            "pitboard-codex-keyring-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("a scratch home");
        std::fs::write(
            dir.join("config.toml"),
            "cli_auth_credentials_store = \"keyring\"\n",
        )
        .expect("a config");
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"))
            .with_codex_home(dir.to_string_lossy().into_owned());
        let refused = Codex.live(&ctx).err().expect("refused");
        assert!(matches!(refused, ProviderError::Unsupported { .. }));
        let config = dir.join("config.toml");
        assert_eq!(
            refused.to_string(),
            format!(
                "this machine's Codex keeps its login in the keychain \
                 (`cli_auth_credentials_store = \"keyring\"` in {config}). Pitboard handles \
                 Codex's file store, auth.json, and does not read an item Codex created for \
                 itself, because every read would ask you for permission. To use the file \
                 store, set `cli_auth_credentials_store = \"file\"` in {config}, then sign in \
                 again with `codex login`",
                config = config.display()
            )
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A store pinned by `/etc/codex/requirements.toml` is refused naming that file, since no
    /// line in the person's own config can change it, and so is a sign-in under it. One that
    /// cannot be read is refused rather than taken for the file.
    #[test]
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
    fn a_store_an_administrator_set_is_refused_naming_where() {
        let host = crate::host::memory::MemoryHost::new();
        let ctx = Context::for_unit_test().with_memory_stores(host.clone());
        assert!(Codex.live(&ctx).is_ok(), "nothing set is the file");
        host.administers(
            "/etc/codex/requirements.toml",
            "cli_auth_credentials_store = \"ephemeral\"\n",
        );
        let refused = Codex.live(&ctx).err().expect("refused").to_string();
        assert_eq!(
            refused,
            "this machine's Codex keeps its login in memory only (pinned to `ephemeral` by \
             /etc/codex/requirements.toml), so there is nothing at rest to park or switch. No \
             line in your own config.toml can change it: /etc/codex/requirements.toml pins \
             it, and only an administrator can change that"
        );
        assert!(matches!(
            Codex.private_signin_isolation(&ctx),
            Isolation::NotIsolated { reason } if reason.contains("/etc/codex/requirements.toml")
        ));

        host.administers("/etc/codex/requirements.toml", "[[");
        let refused = Codex.live(&ctx).err().expect("refused").to_string();
        assert_eq!(
            refused,
            "Pitboard cannot tell where this machine's Codex keeps its login: \
             /etc/codex/requirements.toml is not TOML Codex can read, at line 1. Codex \
             0.160.0 does not start until that is put right"
        );
    }

    /// A `profile = "<name>"` line chooses its profile, whose table holds no store Codex
    /// reads, so where the layers choose the file the live login is the file's, where it was
    /// refused for the line. A line naming a profile no table defines is refused, saying
    /// what each Codex does with it.
    #[test]
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
    fn a_profile_line_is_read_as_choosing_its_profile() {
        let host = crate::host::memory::MemoryHost::new();
        let ctx = Context::for_unit_test().with_memory_stores(host.clone());
        host.administers(
            "/etc/codex/config.toml",
            "profile = \"work\"\n[profiles.work]\ncli_auth_credentials_store = \"keyring\"\n",
        );
        assert!(
            Codex.live(&ctx).is_ok(),
            "a chosen profile, its store not read"
        );

        host.administers("/etc/codex/config.toml", "profile = \"work\"\n");
        let refused = Codex.live(&ctx).err().expect("refused").to_string();
        assert_eq!(
            refused,
            "Pitboard cannot tell where this machine's Codex keeps its login: \
             /etc/codex/config.toml sets `profile = \"work\"`, a profile no `[profiles.work]` \
             table defines. Codex 0.160.0 does not start with that line, and an older Codex, \
             such as 0.99.0, which chooses a profile with it, refuses one no table defines: \
             \"config profile `work` not found\""
        );
    }

    /// Where `/etc/codex/managed_config.toml` or `/etc/codex/requirements.toml` chooses the
    /// file over a keychain or memory store in the person's own `config.toml`, Codex keeps
    /// its login in `auth.json`, so the account is switched there rather than refused, as it
    /// was while Pitboard read the person's `config.toml` alone.
    #[test]
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
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
            for over in [
                "/etc/codex/managed_config.toml",
                "/etc/codex/requirements.toml",
            ] {
                let host = crate::host::memory::MemoryHost::new();
                let ctx = ctx_for(&host);
                assert!(Codex.live(&ctx).is_err(), "{own} alone is refused");
                host.administers(over, "cli_auth_credentials_store = \"file\"\n");
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
