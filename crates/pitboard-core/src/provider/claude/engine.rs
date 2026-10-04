//! Claude Code behind the provider boundary.
//!
//! Every method here delegates to the code that was already doing the job, so this adds a
//! seam and changes no behaviour. What it proves is that the boundary is one Claude Code
//! can actually sit behind, which is worth knowing before a second tool is written against
//! it rather than after.

use super::{configfile, document, live, paths as claude};
use crate::api::{self, ApiError};
use crate::context::Context;
use crate::provider::{
    Adoption, Credential, Expiry, Identity, Isolation, LiveStore, ParkSemantics, Provider,
    ProviderError, ProviderId, SignInView,
};
use crate::switch;
use crate::usage;
use serde_json::Value;
use std::path::PathBuf;

/// Claude Code. A zero-sized value: everything it needs comes from the context.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Claude;

/// Claude Code serves its credential from a 30 second cache, so a session already running
/// picks a switch up on its own. Measured against a running session; the three seconds of
/// margin are for the round trip that follows the cache expiring.
const ADOPTION_SECONDS: u32 = switch::ADOPTION_CEILING_SECONDS;

/// What `claude auth login` prints to say it reads a code typed back: the start of its
/// prompt `Paste code here if prompted > `, which the register's `sign_in_output` holds
/// whole.
const ASKS_FOR_A_CODE: &str = "Paste code";

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

    /// Asked of Anthropic, never read from Claude Code's config, which can lag the
    /// credential by a day. That round trip is the reason a switch needs the network at
    /// all, and it is deliberate: filing a login under the wrong account is worse than
    /// refusing to file it.
    fn identify(&self, ctx: &Context, credential: &Credential) -> Result<Identity, ProviderError> {
        let token =
            access_token(&credential.raw).ok_or_else(|| ProviderError::ShapeUnexpected {
                provider: ProviderId::Claude,
                detail: "it has no claudeAiOauth.accessToken".into(),
            })?;
        api::owner(ctx, token).map(from_owner).map_err(from_api)
    }

    fn usage(
        &self,
        ctx: &Context,
        credential: &Credential,
    ) -> Result<usage::Snapshot, ProviderError> {
        let token =
            access_token(&credential.raw).ok_or_else(|| ProviderError::ShapeUnexpected {
                provider: ProviderId::Claude,
                detail: "it has no claudeAiOauth.accessToken".into(),
            })?;
        api::usage(ctx, token).map_err(from_api)
    }

    fn renew(&self, ctx: &Context, credential: &Credential) -> Result<Credential, ProviderError> {
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
        let fresh = api::renew(ctx, refresh, &scopes, client_id).map_err(from_api)?;
        // Anchored to the answer's own clock where it sent one, so a machine whose clock is
        // wrong does not write an expiry that is wrong with it.
        let at_millis = fresh.at.map_or_else(|| ctx.now_millis(), |at| at * 1000);
        Ok(Credential::new(
            ProviderId::Claude,
            document::renewed(&credential.raw, &fresh, at_millis),
        ))
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

    /// The identity cached in Claude Code's config, which it refreshes about once a day.
    fn recorded_identity(&self, ctx: &Context) -> Option<Identity> {
        let config = claude::load_config(ctx).ok()?;
        let found = claude::identity(&config)?;
        Some(Identity {
            account_id: found.account_uuid,
            email: found.email,
            group: Some(found.organization_uuid).filter(|o| !o.is_empty()),
        })
    }

    /// Record the new identity in Claude Code's config. Runs after the login is in place,
    /// so the config never names an account before its login is live. Claude Code does not
    /// correct a stale config on its own; it refetches its profile only once a day.
    fn after_switch(
        &self,
        ctx: &Context,
        incoming: &crate::state::Account,
        outgoing: &Identity,
    ) -> Result<(), crate::error::Error> {
        let path = claude::config_file(ctx);
        let outgoing_group = outgoing.group.clone().unwrap_or_default();
        configfile::backup(ctx, &path)?;
        configfile::update(ctx, &path, |config| {
            configfile::splice_identity(
                config,
                incoming.claude().map_or(&Value::Null, |c| c.oauth_account),
                &[outgoing.account_id.as_str(), outgoing_group.as_str()],
            )
        })
        .map(|_| ())
    }

    fn program(&self, ctx: &Context) -> Option<PathBuf> {
        claude::program(ctx)
    }

    /// Measured in 2.1.278: `claude auth login` opens the browser itself and finishes
    /// through a loopback callback, printing progress with `stdout.write`. Read from
    /// 2.1.289, as the register's `sign_in_output` holds: it also reads a pasted code from
    /// stdin, from the start and whether or not the callback is reached. So it needs no
    /// terminal: pipes are enough.
    /// `CLAUDE_SECURESTORAGE_CONFIG_DIR` is taken away because it would pin the credential
    /// slot back to a real one whatever `CLAUDE_CONFIG_DIR` says.
    fn sign_in(&self, ctx: &Context, dir: &std::path::Path) -> std::process::Command {
        let mut command = crate::provider::command(ctx, ProviderId::Claude);
        command
            .args(["auth", "login"])
            .env("CLAUDE_CONFIG_DIR", dir)
            .env_remove("CLAUDE_SECURESTORAGE_CONFIG_DIR");
        command
    }

    fn read_signin(
        &self,
        ctx: &Context,
        dir: &std::path::Path,
    ) -> Result<Option<String>, crate::store::Error> {
        live::read_signin(ctx, dir)
    }

    /// The keychain item Claude Code made for the private directory, which outlives the
    /// directory unless it is deleted by name.
    fn discard_signin(&self, ctx: &Context, dir: &std::path::Path) {
        let _ = live::discard_signin(ctx, dir);
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

    fn overridden_by(&self, ctx: &Context) -> Vec<String> {
        crate::settings::overrides(ctx)
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    fn adoption(&self) -> Adoption {
        Adoption::PollingWithin(ADOPTION_SECONDS)
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

    /// A document with no `claudeAiOauth` is what `/logout` leaves: it deletes the account's
    /// keys and keeps the machine's, such as MCP tokens. That is nobody signed in.
    fn slice(&self, live: &Value) -> Result<Value, ProviderError> {
        if live.is_object() && live.get("claudeAiOauth").is_none() {
            return Err(ProviderError::NoLogin {
                provider: ProviderId::Claude,
            });
        }
        document::slice(live).map_err(|detail| ProviderError::ShapeUnexpected {
            provider: ProviderId::Claude,
            detail,
        })
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

fn access_token(login: &Value) -> Option<&str> {
    document::oauth_in(login)
        .get("accessToken")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
}

fn from_owner(owner: api::Owner) -> Identity {
    Identity {
        account_id: owner.account_uuid,
        email: owner.email,
        // An account outside an organisation reports an empty string, which is not the same
        // as a tool that has no organisations at all.
        group: Some(owner.organization_uuid).filter(|o| !o.is_empty()),
    }
}

fn from_api(error: ApiError) -> ProviderError {
    match error {
        ApiError::Unauthorized => ProviderError::Unauthorized,
        ApiError::RateLimited { retry_after } => ProviderError::RateLimited {
            service: ProviderId::Claude.service(),
            retry_after,
        },
        ApiError::Network(detail) => ProviderError::Network {
            service: ProviderId::Claude.service(),
            detail,
        },
        ApiError::Unexpected { status } | ApiError::Blocked { status, .. } => {
            ProviderError::Unexpected {
                service: ProviderId::Claude.service(),
                status,
            }
        }
        ApiError::Malformed(detail) => ProviderError::Malformed {
            service: ProviderId::Claude.service(),
            detail,
        },
        ApiError::InvalidGrant => ProviderError::InvalidGrant {
            service: ProviderId::Claude.service(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
