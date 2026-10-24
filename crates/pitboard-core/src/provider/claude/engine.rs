//! Claude Code behind the provider boundary.
//!
//! Every method here delegates to the code that was already doing the job, so this adds a
//! seam and changes no behaviour. What it proves is that the boundary is one Claude Code
//! can actually sit behind, which is worth knowing before a second tool is written against
//! it rather than after.

use super::api;
use super::{configfile, document, live, paths as claude};
use crate::context::Context;
use crate::host::{OS, Os};
use crate::permit::Permit;
use crate::provider::{
    Adoption, Behind, Credential, Expiry, Held, Identity, Isolation, LiveStore, Owner,
    ParkSemantics, Provider, ProviderError, ProviderId, SignInView,
};
use crate::switch;
use crate::usage;
use serde_json::Value;
use std::path::PathBuf;

/// Claude Code. A zero-sized value: everything it needs comes from the context.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Claude;

/// Claude Code serves its credential from a 30 second cache, so a session already running
/// picks a switch up on its own where nothing sits behind the keychain. Measured against a
/// running session; the three seconds of margin are for the round trip that follows the
/// cache expiring.
const ADOPTION_SECONDS: u32 = switch::ADOPTION_CEILING_SECONDS;

// Every system's answer, so each is tested on every system.
fn adoption_on(os: Os, behind: Option<&Behind>) -> Adoption {
    match (os, behind) {
        // Behind the keychain or Credential Manager, whatever the file holds, a session keeps its
        // login until it renews it, a request is refused, it signs in, or the file's mtime changes
        // (`fallback_file_pins_session_login`).
        (Os::MacOs | Os::Windows, Some(behind)) => Adoption::AtRenewal {
            file: behind.path.clone(),
        },
        // Windows: measured for the file store, not Credential Manager (`windows_credman_cache`).
        (Os::MacOs | Os::Windows, None) => Adoption::PollingWithin(ADOPTION_SECONDS),
        // The file is the store, so nothing is behind it, and a switch changes its mtime.
        (Os::Linux, _) => Adoption::PollingWithin(ADOPTION_SECONDS),
    }
}

/// What `claude auth login` prints to say it reads a code typed back: the start of its
/// prompt `Paste code here if prompted > `, which the register's `sign_in_output` holds
/// whole.
const ASKS_FOR_A_CODE: &str = "Paste code";

/// What `claude auth login` writes when it refuses a code typed back: the start of `Invalid
/// code. Please make sure the full code was copied.`, which the register's
/// `sign_in_takes_another_code` holds whole.
const REFUSES_A_CODE: &str = "Invalid code";

impl Provider for Claude {
    fn id(&self) -> ProviderId {
        ProviderId::Claude
    }

    fn live(&self, ctx: &Context) -> Result<LiveStore, ProviderError> {
        Ok(LiveStore {
            chain: live::chain(ctx),
            service: claude::live_service(ctx),
        })
    }

    /// Asked of Anthropic as Claude Code asks it where an identity decides something, never
    /// read from Claude Code's config, which can lag the credential by a day.
    fn identity(&self, ctx: &Context, credential: &Credential) -> Result<Identity, ProviderError> {
        let token = token(credential)?;
        let validated = api::validate(ctx, token)?;
        if let Some(organization_uuid) = validated.organization_uuid {
            return Ok(Identity {
                account_uuid: validated.account_uuid,
                organization_uuid,
            });
        }
        // The profile names the organisation every enrolled account was recorded under, and
        // Claude Code's sign-in records it there or from the token answer. The token is
        // valid, so neither failing is a refusal of the login. Only a profile refused for good
        // is read past: an outage is asked again.
        match api::owner(ctx, token) {
            Ok(owner) => Ok(owner.identity()),
            Err(error @ ProviderError::Refused { .. }) => self
                .own_record(ctx)
                .filter(|recorded| recorded.account_uuid == validated.account_uuid)
                .map(|recorded| recorded.identity())
                .ok_or(ProviderError::Unnamed {
                    error: Box::new(error),
                }),
            Err(error) => Err(error),
        }
    }

    /// The profile's email, or, where Anthropic will not give the profile, the one Claude
    /// Code's own record holds for that same account and organisation, which its sign-in
    /// writes from the profile or else from the token answer.
    fn name(
        &self,
        ctx: &Context,
        credential: &Credential,
        identity: &Identity,
    ) -> Result<String, ProviderError> {
        match api::owner(ctx, token(credential)?) {
            Ok(owner) if owner.identity() == *identity => Ok(owner.email),
            Ok(_) => Err(ProviderError::Malformed {
                service: ProviderId::Claude.service(),
                detail: "the profile names another login than the token's".into(),
            }),
            Err(ProviderError::Unauthorized) => Err(ProviderError::Unauthorized),
            Err(refused) => self
                .own_record(ctx)
                .filter(|recorded| recorded.identity() == *identity)
                .map(|recorded| recorded.email)
                .ok_or(refused),
        }
    }

    fn identifies_by_itself(&self) -> bool {
        false
    }

    fn usage(
        &self,
        ctx: &Context,
        credential: &Credential,
    ) -> Result<usage::Snapshot, ProviderError> {
        api::usage(ctx, token(credential)?)
    }

    fn renew(
        &self,
        ctx: &Context,
        permit: Permit,
        credential: &Credential,
    ) -> Result<Credential, ProviderError> {
        let oauth = document::oauth_in(&credential.raw);
        let refresh = oauth["refreshToken"].as_str().unwrap_or_default();
        let mut scopes: Vec<String> = oauth["scopes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        if scopes.is_empty() {
            scopes = switch::renew::DEFAULT_SCOPES.map(str::to_owned).to_vec();
        }
        let client_id = oauth["clientId"].as_str();
        let fresh = api::renew(ctx, permit, refresh, &scopes, client_id)?;
        // Anchored to the answer's own clock where it sent one, so a machine whose clock is
        // wrong does not write an expiry that is wrong with it.
        let at_millis = fresh.at.map_or_else(|| ctx.now_millis(), |at| at * 1000);
        let mut renewed = Credential::new(
            ProviderId::Claude,
            document::renewed(&credential.raw, &fresh, at_millis),
        );
        renewed.named = fresh.owner;
        Ok(renewed)
    }

    fn slot(&self, ctx: &Context) -> String {
        claude::live_service(ctx)
    }

    /// The directory lock Claude Code's own protocol takes around every write to its
    /// credential. A write that did not take it could land between Claude Code's read and
    /// its write of a refreshed token, and one of the two logins would be lost.
    fn write_lock(&self, ctx: &Context) -> Option<PathBuf> {
        Some(PathBuf::from(claude::storage_dir(ctx)).join(".storage-write"))
    }

    /// The lock a renewal takes before it reads the login again and sends its refresh token,
    /// and the legacy one it takes after it (the register's `refresh_lock`). The write lock
    /// does not keep a renewal from spending a refresh token: these do.
    fn refresh_lock(&self, ctx: &Context) -> Option<(PathBuf, PathBuf)> {
        Some((claude::refresh_lock(ctx), claude::legacy_refresh_lock(ctx)))
    }

    /// The account Claude Code's config names, as written there. One that names no
    /// organisation, as a sign-in that could not read the profile leaves it, is named with
    /// none (the register's `config_may_name_no_organisation`).
    fn own_record(&self, ctx: &Context) -> Option<Owner> {
        let config = claude::load_config(ctx).ok()?;
        let found = claude::config_account(&config)?;
        Some(Owner {
            account_uuid: found.account_uuid,
            email: found.email,
            organization_uuid: found.organization_uuid,
        })
    }

    /// Record the new identity in Claude Code's config. Runs after the login is in place,
    /// so the config never names an account before its login is live. It drops
    /// `profileFetchedAt`, so the next Claude Code process to start writes the account of
    /// the login it started with, and a sign-in writes its own. Either can be another login
    /// than the one stored, such as the one a session over SSH takes from a file behind the
    /// keychain (the register's `config_identity_is_the_last_writers`).
    fn after_switch(
        &self,
        ctx: &Context,
        permit: Permit,
        incoming: &crate::state::Account,
        outgoing: &Owner,
    ) -> Result<(), crate::error::Error> {
        let path = claude::config_file(ctx);
        // Claude Code that never ran makes it on its first run, naming the login it starts with.
        if !path.exists() {
            return Ok(());
        }
        configfile::backup(ctx, permit, &path)?;
        configfile::update(ctx, permit, &path, |config| {
            configfile::splice_identity(
                config,
                incoming.claude().map_or(&Value::Null, |c| c.oauth_account),
                &[
                    outgoing.account_uuid.as_str(),
                    outgoing.organization_uuid.as_str(),
                ],
            )
        })
        .map(|_| ())
    }

    fn program(&self, ctx: &Context) -> Option<PathBuf> {
        claude::program(ctx)
    }

    // `CLAUDE_SECURESTORAGE_CONFIG_DIR` would pin the slot back to a real one, and
    // `CLAUDE_CODE_FORCE_WINDOWS_CREDMAN` the login into Credential Manager (2.1.300).
    fn sign_in(
        &self,
        ctx: &Context,
        permit: Permit,
        dir: &std::path::Path,
    ) -> std::io::Result<std::process::Command> {
        let mut command = crate::provider::command(ctx, permit, ProviderId::Claude)?;
        command
            .args(["auth", "login"])
            .env("CLAUDE_CONFIG_DIR", dir)
            .env_remove("CLAUDE_SECURESTORAGE_CONFIG_DIR")
            .env_remove("CLAUDE_CODE_FORCE_WINDOWS_CREDMAN");
        Ok(command)
    }

    fn login_from_sign_in(
        &self,
        ctx: &Context,
        dir: &std::path::Path,
    ) -> Result<Option<String>, crate::store::Error> {
        live::login_from_sign_in(ctx, dir)
    }

    /// The keychain item Claude Code made for the private directory, which outlives the
    /// directory unless it is deleted by name.
    fn discard_signin(&self, ctx: &Context, permit: Permit, dir: &std::path::Path) {
        let _ = live::discard_signin(ctx, permit, dir);
    }

    /// Read from 2.1.289. Before it opens the browser, `claude auth login` writes to stdout
    /// `Opening browser to sign in…`, then `If the browser didn't open, visit: ` and an
    /// address, then its prompt with no newline after it, and from then on it reads a
    /// pasted code. So the field is offered with the address. The address it prints is the
    /// manual one, `https`, whose page shows the code to paste; the browser it opens itself
    /// goes to another, which comes back to the loopback callback. It is printed bare, or as
    /// a terminal hyperlink to itself where the environment says the terminal takes them,
    /// and either way the address read is the one it goes to.
    fn read_sign_in(&self, said: &str) -> SignInView {
        SignInView {
            url: crate::provider::https_address(said),
            wants_code: said.contains(ASKS_FOR_A_CODE),
        }
    }

    /// Read from 2.1.289, as the register's `sign_in_takes_another_code` holds: a line typed
    /// back that is not `<code>#<state>` with both halves is refused, with `Invalid code.`
    /// on stderr, and `claude auth login` goes on reading in the same process. It prints no
    /// prompt again, so the field is offered again on the refusal alone.
    fn refused_code(&self, since: &str) -> bool {
        since.contains(REFUSES_A_CODE)
    }

    fn overridden_by(&self, ctx: &Context) -> Vec<String> {
        crate::settings::overrides(ctx)
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    // Where several managed sources set it, Claude Code takes one; a login none of them names
    // is refused whichever it takes.
    fn refuses(&self, ctx: &Context, _document: &Value, owner: &Owner) -> Option<String> {
        let organisation = &owner.organization_uuid;
        let pins = crate::settings::organisation_pins(ctx);
        if organisation.is_empty() || pins.iter().any(|(_, ids)| ids.contains(organisation)) {
            return None;
        }
        let (layer, ids) = pins.first()?;
        Some(format!(
            "{layer} sets `forceLoginOrgUUID` to {}, and this login is of `{organisation}`, so \
             Claude Code refuses it as a session starts",
            crate::words::listed_values(ids)
        ))
    }

    fn behind(&self, ctx: &Context) -> Option<Behind> {
        let held = match live::login_behind(ctx)? {
            Ok(Some(_)) => Held::Login,
            Ok(None) => Held::NoLogin,
            Err(_) => Held::Unreadable,
        };
        Some(Behind {
            path: live::credential_file(ctx),
            held,
        })
    }

    fn adoption(&self, behind: Option<&Behind>) -> Adoption {
        adoption_on(OS, behind)
    }

    /// A copy may sit in the vault while the same login is still signed in: nothing of
    /// Claude Code's revokes for presenting either, and the live document holds the
    /// machine's other keys, which have to stay where they are.
    fn park_semantics(&self) -> ParkSemantics {
        ParkSemantics::CopyWhileLive
    }

    /// `CLAUDE_CONFIG_DIR` selects the keychain item by hashing the directory, so a scratch
    /// directory's item is one nothing else reads. There is no second backend that escapes
    /// it.
    fn private_signin_isolation(&self, _ctx: &Context) -> Isolation {
        Isolation::Isolated
    }

    fn slice(&self, live: &Value) -> Result<Value, ProviderError> {
        if document::signed_out(live) {
            return Err(ProviderError::NoLogin {
                provider: ProviderId::Claude,
            });
        }
        document::slice(live).map_err(|detail| ProviderError::ShapeUnexpected {
            provider: ProviderId::Claude,
            detail,
        })
    }

    fn plan(&self, document: &Value) -> Option<String> {
        document::plan(document)
    }

    fn splice(&self, live: &Value, incoming: &Value) -> Result<Value, ProviderError> {
        document::splice(live, incoming).map_err(|detail| ProviderError::ShapeUnexpected {
            provider: ProviderId::Claude,
            detail,
        })
    }

    fn fingerprint(&self, slice: &Value) -> String {
        document::fingerprint_of(slice)
    }

    fn expiry(&self, slice: &Value) -> Expiry {
        let oauth = document::oauth_in(slice);
        // Claude Code records both in epoch milliseconds.
        let at = |key: &str| oauth.get(key).and_then(Value::as_i64).map(|ms| ms / 1000);
        Expiry {
            access_expires_at: at("expiresAt"),
            refresh_expires_at: at("refreshTokenExpiresAt"),
        }
    }
}

fn token(credential: &Credential) -> Result<&str, ProviderError> {
    access_token(&credential.raw).ok_or_else(|| ProviderError::ShapeUnexpected {
        provider: ProviderId::Claude,
        detail: "it has no claudeAiOauth.accessToken".into(),
    })
}

pub(super) fn access_token(login: &Value) -> Option<&str> {
    document::oauth_in(login)
        .get("accessToken")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;

    // Never run: a program that is there on every system.
    #[test]
    fn a_sign_in_withholds_what_would_store_its_login_elsewhere() {
        let ctx = Context::new(PathBuf::from("/nowhere"))
            .with_claude_program(std::env::current_exe().expect("this test"));
        let dir = std::path::Path::new("/tmp/pitboard-signin-scratch");
        let command = Claude
            .sign_in(&ctx, Permit::for_a_test(), dir)
            .expect("found");
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["auth", "login"]);
        // As given: Claude Code hashes the string itself for its slot, as login_from_sign_in does.
        assert!(
            command
                .get_envs()
                .any(|(name, value)| name == "CLAUDE_CONFIG_DIR" && value == Some(dir.as_os_str()))
        );
        for withheld in [
            "CLAUDE_SECURESTORAGE_CONFIG_DIR",
            "CLAUDE_CODE_FORCE_WINDOWS_CREDMAN",
        ] {
            assert!(
                command
                    .get_envs()
                    .any(|(name, value)| name == withheld && value.is_none()),
                "{withheld}"
            );
        }
    }

    /// The manual address 2.1.289 prints, with its values made up.
    const ADDRESS: &str = "https://claude.com/cai/oauth/authorize?code=true\
        &client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e&response_type=code\
        &redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback\
        &scope=org%3Acreate_api_key+user%3Aprofile&code_challenge=c&code_challenge_method=S256\
        &state=s";

    /// What `claude auth login` 2.1.289 writes before it opens the browser, piped, with the
    /// address as its hyperlink helper wrote it: at the end of a line, and the prompt with no
    /// newline after it.
    fn said(address: &str) -> String {
        format!(
            "Opening browser to sign in\u{2026}\n\
             If the browser didn't open, visit: {address}\n\
             Paste code here if prompted > "
        )
    }

    /// The address and the code field come together, because the tool prints both before
    /// it opens the browser: the field is for the page the printed address leads to, which
    /// shows a code.
    #[test]
    fn a_sign_in_offers_its_address_and_a_field_for_the_code_together() {
        let said = said(ADDRESS);
        let read = Claude.read_sign_in(&said);
        assert_eq!(read.url.as_deref(), Some(ADDRESS));
        assert!(read.wants_code);

        let opening = &said[..said.find('\n').expect("a first line")];
        assert_eq!(Claude.read_sign_in(opening), SignInView::default());
    }

    /// A code typed back is refused by what the tool says after it: 2.1.289 writes its
    /// refusal to stderr and goes on reading. What it said before, its prompt among it, says
    /// nothing of the code, and a refusal is read once all of it has come, in whatever
    /// pieces.
    #[test]
    fn a_code_is_refused_by_what_is_said_after_it() {
        let refused = "Invalid code. Please make sure the full code was copied.\n";
        assert!(Claude.refused_code(refused));
        assert!(!Claude.refused_code(""));
        assert!(!Claude.refused_code(&said(ADDRESS)));
        assert!(!Claude.refused_code("Inval"));
        assert!(Claude.refused_code(&["Inval", "id code. Please"].concat()));
    }

    /// What the register says 2.1.289 writes on refusing a code is read as a refusal, so a
    /// build that words it otherwise is one the conformance run reports.
    #[test]
    fn the_refusal_the_register_holds_is_read_as_one() {
        let fact = crate::assumptions::named("sign_in_takes_another_code").expect("listed");
        let written = fact
            .probe
            .iter()
            .find(|probe| probe.contains("make sure the full code"))
            .expect("the refusal is probed for");
        let refusal = &written[written.find(REFUSES_A_CODE).expect("the refusal")..];
        assert!(Claude.refused_code(refusal));
        assert!(refusal.starts_with("Invalid code. Please make sure the full code was copied."));
    }

    /// 2.1.289's hyperlink helper, which the address is printed through, writes it as an
    /// OSC 8 hyperlink when its check says the terminal takes them, which it can say piped.
    /// That is `ESC ] 8 ; ;`, the address and BEL, then the address again as the link's
    /// text, then `ESC ] 8 ; ;` and BEL to end the link. The text is bright blue where colour
    /// is on, as `FORCE_COLOR` turns it on piped. The address to open is the one the link
    /// goes to, with nothing of the sequence or the text in it.
    #[test]
    fn an_address_printed_as_a_terminal_hyperlink_is_the_one_it_links_to() {
        for text in [ADDRESS.to_owned(), format!("\u{1b}[94m{ADDRESS}\u{1b}[39m")] {
            let printed = format!("\u{1b}]8;;{ADDRESS}\u{7}{text}\u{1b}]8;;\u{7}");
            let read = Claude.read_sign_in(&said(&printed));
            assert_eq!(read.url.as_deref(), Some(ADDRESS), "{printed:?}");
            assert!(read.wants_code);
        }
    }

    /// Claude Code keeps the token's own account when the profile could not be read, and
    /// that can name no organisation. The config names that account as written, which is
    /// what a change of it since the last answer is told by.
    #[test]
    fn a_config_naming_no_organisation_names_its_account_with_no_group() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().to_path_buf();
        let ctx = Context::new(home.clone());
        let config = |account: Value| {
            std::fs::write(
                home.join(".claude.json"),
                serde_json::json!({ "oauthAccount": account }).to_string(),
            )
            .unwrap();
        };

        config(serde_json::json!({"accountUuid": "acc", "emailAddress": "a@b.c"}));
        assert_eq!(
            Claude.own_record(&ctx),
            Some(Owner {
                account_uuid: "acc".into(),
                email: "a@b.c".into(),
                organization_uuid: String::new(),
            })
        );

        config(serde_json::json!({
            "accountUuid": "acc", "emailAddress": "a@b.c", "organizationUuid": "org"
        }));
        assert_eq!(
            Claude.own_record(&ctx),
            Some(Owner {
                account_uuid: "acc".into(),
                email: "a@b.c".into(),
                organization_uuid: "org".into(),
            })
        );
    }

    // Behind the keychain on macOS, or Credential Manager on Windows, whatever the file holds and
    // whether or not it can be read, a session takes a switch at its login's next renewal. On
    // Linux the file is the store, and a switch writes it.
    #[test]
    fn sessions_follow_within_33_seconds_unless_a_file_is_behind_the_store() {
        let file = PathBuf::from("/Users/x/.claude/.credentials.json");
        let holding = |held: Held| Behind {
            path: file.clone(),
            held,
        };
        for os in [Os::MacOs, Os::Linux, Os::Windows] {
            assert_eq!(adoption_on(os, None), Adoption::PollingWithin(33), "{os:?}");
        }
        for held in [Held::Login, Held::NoLogin, Held::Unreadable] {
            for os in [Os::MacOs, Os::Windows] {
                assert_eq!(
                    adoption_on(os, Some(&holding(held))),
                    Adoption::AtRenewal { file: file.clone() },
                    "{os:?} {held:?}"
                );
            }
            assert_eq!(
                adoption_on(Os::Linux, Some(&holding(held))),
                Adoption::PollingWithin(33),
                "{held:?}"
            );
        }
    }

    /// The prompt looked for is one the conformance run reads out of every build, so a
    /// build that words it differently is reported, rather than leaving the field unoffered.
    #[test]
    fn the_prompt_looked_for_is_in_the_register() {
        let fact = crate::assumptions::named("sign_in_output").expect("listed");
        assert!(
            fact.probe.iter().any(|p| p.starts_with(ASKS_FOR_A_CODE)),
            "{:?}",
            fact.probe
        );
    }
}
