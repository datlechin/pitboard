//! `pitboard status`: who is signed in, what each account has left, and which accounts can
//! be switched to.
//!
//! Numbers are asked of Anthropic rather than read from Claude Code's cache, which only moves
//! when Claude Code itself asks. Every account is asked at once, so the command costs one
//! round trip, not one per account.

use crate::api::{self, ApiError, Owner};
use crate::context::Context;
use crate::error::Cause;
use crate::provider::ProviderId;
use crate::provider::claude::paths as claude;
use crate::state::{Key, Park, State};
use crate::usage::{Snapshot, Source};
use crate::{budget, park, readings};
use serde_json::Value;

/// Why a reading is not live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Stale {
    NothingSignedIn,
    /// Claude Code's own session has expired; its next call renews it.
    SessionExpired,
    /// A parked login's access token has expired and could not be renewed this time.
    ParkedAccessExpired,
    NothingParked,
    ParkUnreadable,
    RateLimited,
    Unreachable,
    /// Anthropic answered badly and may answer well later.
    ServerError,
    /// Anthropic's answer was not in a shape pitboard understands, which means the shape
    /// moved. Asking again produces the same thing.
    AnswerNotUnderstood,
    /// Anthropic will not accept this parked login again.
    LoginRefused,
    /// Asked recently enough that the answer cannot have moved by a percentage point, so
    /// the number shown is the one already measured rather than a new request.
    AskedRecently,
    /// The thread that was asking stopped before it answered. Nothing to do with
    /// Anthropic, which is why it used to be filed under the same code as a bad answer.
    Interrupted,
    /// Nobody was asked: this reading was taken without touching the network.
    NotAsked,
}

impl Stale {
    /// Three answers used to arrive here as one. A front end deciding whether to ask
    /// again needs to tell a bad morning at Anthropic from an answer whose shape moved
    /// from a login that is finished, and could not.
    fn of(error: &ApiError, signed_in: bool) -> Stale {
        match Cause::of(error) {
            Cause::TokenExpired if signed_in => Stale::SessionExpired,
            Cause::TokenExpired => Stale::ParkedAccessExpired,
            Cause::RateLimited => Stale::RateLimited,
            Cause::Unreachable => Stale::Unreachable,
            Cause::ServerError => Stale::ServerError,
            Cause::AnswerNotUnderstood => Stale::AnswerNotUnderstood,
            Cause::LoginRefused => Stale::LoginRefused,
        }
    }

    /// Stable, for a program to branch on; the same as its JSON form.
    pub fn code(self) -> &'static str {
        match self {
            Stale::NothingSignedIn => "nothing_signed_in",
            Stale::SessionExpired => "session_expired",
            Stale::ParkedAccessExpired => "parked_access_expired",
            Stale::NothingParked => "nothing_parked",
            Stale::ParkUnreadable => "park_unreadable",
            Stale::RateLimited => "rate_limited",
            Stale::Unreachable => "unreachable",
            Stale::ServerError => "server_error",
            Stale::AnswerNotUnderstood => "answer_not_understood",
            Stale::LoginRefused => "login_refused",
            Stale::AskedRecently => "asked_recently",
            Stale::Interrupted => "interrupted",
            Stale::NotAsked => "not_asked",
        }
    }

    /// Only what is worth a word: a parked login going quiet is how parking works.
    pub fn explanation(self) -> Option<&'static str> {
        match self {
            Stale::NothingSignedIn => Some("nothing is signed in"),
            Stale::SessionExpired => Some("Claude Code's session has expired; `claude` renews it"),
            Stale::ParkedAccessExpired | Stale::NothingParked => None,
            Stale::ParkUnreadable => Some("its parked login cannot be read; run `pitboard doctor`"),
            Stale::RateLimited => Some("Anthropic is rate limiting usage checks"),
            Stale::Unreachable => Some("Anthropic could not be reached"),
            Stale::ServerError => Some("Anthropic answered with an error; try again later"),
            Stale::AnswerNotUnderstood => Some("Anthropic's answer was not understood"),
            Stale::LoginRefused => Some("its parked login is no longer accepted; sign in again"),
            // Not worth a word: it is the ordinary state of a number that is
            // already as true as asking again would make it.
            Stale::AskedRecently => None,
            Stale::Interrupted => Some("the check did not finish"),
            Stale::NotAsked => Some("read without asking Anthropic"),
        }
    }
}

pub struct Row {
    /// `None` for an account that is signed in but not enrolled.
    pub label: Option<String>,
    pub email: String,
    pub account_uuid: String,
    pub signed_in: bool,
    pub parked: Option<Park>,
    pub usage: Option<Snapshot>,
    pub stale: Option<Stale>,
    /// How long this account lasts, from what its limits have been doing.
    ///
    /// The question this whole tool exists to answer is which account to use next, and two
    /// instantaneous percentages do not answer it: 73% of a weekly limit means nothing
    /// without knowing whether it was 40% this morning.
    pub runway: crate::history::Runway,
}

impl Row {
    /// Whether `pitboard use` would switch to it now.
    pub fn switchable(&self, now: i64) -> bool {
        !self.signed_in && self.parked.as_ref().is_some_and(|p| p.restorable_at(now))
    }
}

pub struct Report {
    pub now: i64,
    pub rows: Vec<Row>,
    /// Who the live login belongs to, as Anthropic says: Claude Code's config can be a day
    /// behind.
    pub signed_in: Result<Owner, String>,
    /// Which credential slot this report speaks for.
    ///
    /// `CLAUDE_CONFIG_DIR` selects a different keychain item, so "who is signed in" is a
    /// fact about one slot and not about the machine. Accounts and their parked logins
    /// belong to the machine; what is in use does not. This report used to name accounts
    /// without ever saying which slot it was speaking for, so on a machine with a second
    /// config directory it was silently answering about one of them.
    pub slot: Slot,
}

/// A credential slot, named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    /// The keychain item, which is what actually selects the login.
    pub service: String,
    /// Whether this is the one a `claude` with no `CLAUDE_CONFIG_DIR` reads.
    pub default: bool,
}

/// What is true of one tool's live login.
///
/// Per tool, because there is no such thing as "the account signed in on this machine": a
/// Claude Code login and a Codex login are different programs reading different stores, and
/// neither signs the other out.
#[derive(Default)]
struct LiveLogin {
    /// `None` means nobody is signed in to this tool, but only when [`Facts::asked`] is
    /// true: unasked and absent are different things, and reading one as the other says the
    /// account in use is gone.
    signed_in: Option<Result<Owner, ApiError>>,
    /// The account the tool's own local record names, where it keeps one. It can be behind
    /// the login it describes, so it never decides a switch; it keeps the row that is
    /// signed in from reading as though nobody is, when the service cannot be asked.
    config_uuid: Option<String>,
    usage: Option<Result<Snapshot, Stale>>,
}

impl LiveLogin {
    fn usage(&self) -> Result<Snapshot, Stale> {
        self.usage.clone().unwrap_or(Err(Stale::NothingSignedIn))
    }
}

/// Everything gathered from the machine and the network, so assembling it touches neither.
struct Facts {
    live: std::collections::BTreeMap<ProviderId, LiveLogin>,
    asked: bool,
    /// One per enrolled account, in order.
    parked_usage: Vec<Result<Snapshot, Stale>>,
    claude_code_cache: Option<Snapshot>,
}

impl Facts {
    fn live_for(&self, which: ProviderId) -> Option<&LiveLogin> {
        self.live.get(&which)
    }
}

/// A parked login holds the account's whole slice of the credential document, so the token
/// is inside its OAuth block; a park from a version that kept less is that block.
/// The parked login to ask about an account with, or why there is none.
///
/// Whether the document holds what a usage call needs is the tool's own question, answered
/// when it is asked: a Codex login keeps its token somewhere a Claude Code login does not.
fn parked_document(
    ctx: &Context,
    key: &Key,
    parked: Option<&Park>,
    now: i64,
) -> Result<Value, Stale> {
    match parked {
        None => Err(Stale::NothingParked),
        Some(p) if !p.askable_at(now) => Err(Stale::ParkedAccessExpired),
        Some(p) => park::load(ctx, key, p).map_err(|_| Stale::ParkUnreadable),
    }
}

/// What is known without asking anyone: who Claude Code's config says is signed in, and
/// the last numbers pitboard measured. Touches no network and no login, so it answers at
/// once and works on a train.
/// Which slot this machine's `claude` would read right now.
fn slot_of(ctx: &Context) -> Slot {
    Slot {
        service: claude::live_service(ctx),
        default: claude::is_default_slot(ctx),
    }
}

pub fn gather_offline(ctx: &Context, state: &State) -> Report {
    let config = claude::load_config(ctx).ok();
    let identity = config.as_ref().and_then(claude::identity);
    let facts = Facts {
        live: std::iter::once((
            ProviderId::Claude,
            LiveLogin {
                signed_in: None,
                config_uuid: identity.as_ref().map(|id| id.account_uuid.clone()),
                usage: Some(Err(Stale::NotAsked)),
            },
        ))
        .collect(),
        asked: false,
        parked_usage: state
            .accounts
            .iter()
            .map(|_| Err(Stale::NotAsked))
            .collect(),
        claude_code_cache: config.as_ref().and_then(crate::usage::from_config_cache),
    };
    let remembered = readings::load(ctx);
    Report {
        now: ctx.now(),
        slot: slot_of(ctx),
        rows: assemble(
            state,
            &facts,
            |uuid| remembered.get(uuid).cloned(),
            |uuid| crate::history::runway_for(ctx, uuid, ctx.now()),
        ),
        // Claude Code's config, which can be a day behind the login it describes. Good
        // enough to say who is in use; never good enough to move a login.
        signed_in: identity.map_or_else(
            || Err("not asked".into()),
            |id| {
                Ok(Owner {
                    account_uuid: id.account_uuid,
                    email: id.email,
                    organization_uuid: id.organization_uuid,
                })
            },
        ),
    }
}

/// What one account's request came to: the reading, and what the budget should learn from
/// it. Recorded by the caller once every request has finished, never from the thread that
/// made it.
type Asked = (Result<Snapshot, Stale>, Option<(String, budget::Outcome)>);

/// A provider's failure as the usage path has always classified one.
fn to_api(error: crate::provider::ProviderError) -> ApiError {
    use crate::provider::ProviderError as P;
    match error {
        P::Unauthorized => ApiError::Unauthorized,
        P::RateLimited { retry_after } => ApiError::RateLimited { retry_after },
        P::Network { detail, .. } => ApiError::Network(detail),
        P::Unexpected { status, .. } => ApiError::Unexpected { status },
        P::Malformed(detail) | P::ShapeUnexpected { detail, .. } => ApiError::Malformed(detail),
        P::InvalidGrant => ApiError::InvalidGrant,
    }
}

/// Ask about one account, or say why not.
fn ask_usage(
    ctx: &Context,
    which: ProviderId,
    account_uuid: Option<&str>,
    document: &Value,
    signed_in: bool,
    remembered: Option<&Snapshot>,
    fresh: bool,
) -> Asked {
    // An account pitboard cannot name cannot be budgeted for; it is asked about, which is
    // what always happened.
    if let Some(uuid) = account_uuid
        && let Some(held) = budget::may_ask(ctx, uuid, remembered, fresh)
    {
        let stale = match held {
            budget::Held::Fresh => Stale::AskedRecently,
            budget::Held::RateLimited => Stale::RateLimited,
            budget::Held::Unreachable => Stale::Unreachable,
        };
        return (Err(stale), None);
    }
    // Through the provider, because what a usage call needs is not the same everywhere:
    // Codex sends an account id header it reads out of the credential, and Gemini needs a
    // project id from a file the credential never mentions.
    let answer = crate::provider::of(which).usage(
        ctx,
        &crate::provider::Credential::new(which, document.clone()),
    );
    // A park that is not the shape its own tool keeps was damaged at rest, which is a fact
    // about the vault and not about the service. Said as such rather than as an answer the
    // service gave.
    if !signed_in
        && matches!(
            answer,
            Err(crate::provider::ProviderError::ShapeUnexpected { .. })
        )
    {
        return (Err(Stale::ParkUnreadable), None);
    }
    let answer = answer.map_err(to_api);
    let learned = account_uuid.and_then(|uuid| {
        let outcome = match &answer {
            Ok(_) => budget::Outcome::Answered,
            Err(api::ApiError::RateLimited { retry_after }) => {
                budget::Outcome::RateLimited(*retry_after)
            }
            Err(api::ApiError::Network(_) | api::ApiError::Unexpected { .. }) => {
                budget::Outcome::Unreachable
            }
            // A token that is refused, or an answer that is not understood, is not a reason
            // to wait: waiting fixes neither.
            Err(_) => return None,
        };
        Some((uuid.to_string(), outcome))
    });
    (answer.map_err(|e| Stale::of(&e, signed_in)), learned)
}

pub fn gather(ctx: &Context, state: &State, fresh: bool) -> Report {
    let now = ctx.now();
    // Each tool's live login, whole. Not a token out of it: what a usage call needs is not
    // the same everywhere, and pulling one field out here would decide that for all of them.
    let live_documents: Vec<(ProviderId, Option<Value>)> = ProviderId::ALL
        .iter()
        .map(|&which| {
            let document = crate::provider::of(which)
                .read_live(ctx)
                .ok()
                .flatten()
                .map(|credential| credential.raw);
            (which, document)
        })
        .collect();
    let parked_documents: Vec<Result<Value, Stale>> = state
        .accounts
        .iter()
        .map(|a| parked_document(ctx, &a.key(), a.parked.as_ref(), now))
        .collect();
    let config = claude::load_config(ctx).ok();
    // Only Claude Code keeps a local record of who is signed in that can lag its own
    // credential. Codex's login names its own account, so there is nothing to fall back to.
    let claude_config_uuid = config
        .as_ref()
        .and_then(claude::identity)
        .map(|id| id.account_uuid);
    let remembered = readings::load(ctx);

    let (live, parked_asked): (Vec<(ProviderId, LiveLogin, Option<_>)>, Vec<Asked>) =
        std::thread::scope(|scope| {
            // Who owns each live login is asked whatever the budget says: it decides which
            // account a row belongs to, it is not a measurement, and a switch needs it.
            let per_tool: Vec<_> = live_documents
                .iter()
                .map(|(which, document)| {
                    let which = *which;
                    let remembered = &remembered;
                    let config_uuid = (which == ProviderId::Claude)
                        .then(|| claude_config_uuid.clone())
                        .flatten();
                    scope.spawn(move || {
                        let Some(document) = document else {
                            return (
                                which,
                                LiveLogin {
                                    signed_in: None,
                                    config_uuid,
                                    usage: None,
                                },
                                None,
                            );
                        };
                        let credential = crate::provider::Credential::new(which, document.clone());
                        let owner = crate::provider::of(which)
                            .identify(ctx, &credential)
                            .map(|found| Owner {
                                account_uuid: found.account_id,
                                email: found.email,
                                organization_uuid: found.group.unwrap_or_default(),
                            })
                            .map_err(to_api);
                        let uuid = owner.as_ref().ok().map(|o| o.account_uuid.clone());
                        let (usage, learned) = ask_usage(
                            ctx,
                            which,
                            uuid.as_deref(),
                            document,
                            true,
                            uuid.as_deref().and_then(|u| remembered.get(u)),
                            fresh,
                        );
                        (
                            which,
                            LiveLogin {
                                signed_in: Some(owner),
                                config_uuid,
                                usage: Some(usage),
                            },
                            learned,
                        )
                    })
                })
                .collect();
            let parked: Vec<_> = parked_documents
                .iter()
                .zip(state.accounts.iter())
                .map(|(token, account)| {
                    let remembered = &remembered;
                    scope.spawn(move || match token.as_ref() {
                        Err(stale) => (Err(*stale), None),
                        Ok(document) => ask_usage(
                            ctx,
                            account.provider(),
                            Some(&account.account_uuid),
                            document,
                            false,
                            remembered.get(&account.account_uuid),
                            fresh,
                        ),
                    })
                })
                .collect();
            (
                per_tool
                    .into_iter()
                    .map(|h| {
                        h.join()
                            .unwrap_or((ProviderId::Claude, LiveLogin::default(), None))
                    })
                    .collect(),
                parked
                    .into_iter()
                    .map(|h| h.join().unwrap_or((Err(Stale::Interrupted), None)))
                    .collect(),
            )
        });

    // Once, from one thread. Every account's record lives in one file, so a thread each
    // reading it, changing one entry and writing it back would erase what the others
    // learned.
    let learned: Vec<(String, budget::Outcome)> = live
        .iter()
        .filter_map(|(_, _, learned)| learned.clone())
        .chain(
            parked_asked
                .iter()
                .filter_map(|(_, learned)| learned.clone()),
        )
        .collect();
    budget::record(ctx, &learned);

    let mut by_tool: std::collections::BTreeMap<ProviderId, LiveLogin> = live
        .into_iter()
        .map(|(which, login, _)| (which, login))
        .collect();
    // Taken out rather than cloned: an API error is not `Clone`, and the report wants the
    // default tool's answer whole.
    let signed_in_default = by_tool
        .get_mut(&crate::label::DEFAULT)
        .and_then(|login| login.signed_in.take());
    if let Some(login) = by_tool.get_mut(&crate::label::DEFAULT) {
        login.signed_in = match &signed_in_default {
            Some(Ok(owner)) => Some(Ok(owner.clone())),
            // The rows only need to know whether it answered, and with what account.
            Some(Err(e)) => Some(Err(ApiError::Malformed(e.to_string()))),
            None => None,
        };
    }
    let facts = Facts {
        live: by_tool,
        asked: true,
        parked_usage: parked_asked.into_iter().map(|(usage, _)| usage).collect(),
        claude_code_cache: config.as_ref().and_then(crate::usage::from_config_cache),
    };
    let rows = assemble(
        state,
        &facts,
        |uuid| remembered.get(uuid).cloned(),
        |uuid| crate::history::runway_for(ctx, uuid, now),
    );
    for row in &rows {
        if let Some(live) = row.usage.as_ref().filter(|u| u.source == Source::Live) {
            crate::history::record(ctx, &row.account_uuid, live);
        }
    }
    readings::remember(
        ctx,
        &rows
            .iter()
            .filter_map(|r| {
                r.usage
                    .as_ref()
                    .filter(|u| u.source == Source::Live)
                    .map(|u| (r.account_uuid.clone(), u.clone()))
            })
            .collect::<Vec<_>>(),
    );

    Report {
        slot: slot_of(ctx),
        now,
        rows,
        // The default tool's answer. Every tool's is in the rows, which is where a
        // machine with more than one signed in has to be read from: there is no single
        // fact called "the account signed in on this machine" any more.
        signed_in: match signed_in_default {
            Some(Ok(owner)) => Ok(owner),
            Some(Err(e)) => Err(e.to_string()),
            None => Err("nothing is signed in".into()),
        },
    }
}

fn assemble(
    state: &State,
    facts: &Facts,
    recall: impl Fn(&str) -> Option<Snapshot>,
    lasting: impl Fn(&str) -> crate::history::Runway,
) -> Vec<Row> {
    // Per tool, because an account is signed in to its own tool or to nothing. Comparing
    // every account against one machine-wide answer would have marked a Codex account
    // signed in because a Claude Code account with the same uuid was.
    let live_uuid = |which: ProviderId| -> Option<&str> {
        let live = facts.live_for(which)?;
        match (&live.signed_in, facts.asked) {
            (Some(Ok(owner)), _) => Some(owner.account_uuid.as_str()),
            // Unreachable, or never asked. The service decides who is signed in; without
            // its answer, the tool's own local record is the only one there is.
            (Some(Err(_)), _) | (None, false) => live.config_uuid.as_deref(),
            (None, true) => None,
        }
    };
    // Claude Code's cache counts only when it was measured for the account in question.
    let cached_for = |uuid: &str| {
        facts
            .claude_code_cache
            .clone()
            .filter(|c| c.account_uuid.as_deref() == Some(uuid))
    };
    let reading = |asked: &Result<Snapshot, Stale>, fallback: Option<Snapshot>| match asked {
        Ok(live) => (Some(live.clone()), None),
        Err(stale) => (fallback, Some(*stale)),
    };

    let mut rows: Vec<Row> = state
        .accounts
        .iter()
        .zip(&facts.parked_usage)
        .map(|(account, parked)| {
            let uuid = account.account_uuid.as_str();
            let which = account.provider();
            let signed_in = live_uuid(which) == Some(uuid);
            let (usage, stale) = if signed_in {
                let live = facts
                    .live_for(which)
                    .map_or(Err(Stale::NothingSignedIn), LiveLogin::usage);
                reading(&live, cached_for(uuid).or_else(|| recall(uuid)))
            } else {
                reading(parked, recall(uuid))
            };
            Row {
                label: Some(account.label.clone()),
                email: account.email.clone(),
                account_uuid: account.account_uuid.clone(),
                signed_in,
                parked: account.parked.clone(),
                usage,
                stale,
                runway: lasting(uuid),
            }
        })
        .collect();

    // A tool signed in to an account nothing has enrolled still gets a row, so somebody can
    // see what is there and give it a name. One per tool, because each can have its own.
    for which in ProviderId::ALL {
        let Some(Ok(owner)) = facts
            .live_for(*which)
            .map(|l| &l.signed_in)
            .and_then(Option::as_ref)
        else {
            continue;
        };
        if rows.iter().any(|r| r.account_uuid == owner.account_uuid) {
            continue;
        }
        let live = facts
            .live_for(*which)
            .map_or(Err(Stale::NothingSignedIn), LiveLogin::usage);
        let (usage, stale) = reading(&live, cached_for(&owner.account_uuid));
        rows.push(Row {
            label: None,
            email: owner.email.clone(),
            account_uuid: owner.account_uuid.clone(),
            signed_in: true,
            parked: None,
            usage,
            stale,
            runway: lasting(&owner.account_uuid),
        });
    }

    rows.sort_by_key(|r| !r.signed_in);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Account;
    use crate::usage::Window;
    use serde_json::json;

    const NOW: i64 = 1_789_935_000;

    fn owner(uuid: &str) -> Owner {
        Owner {
            account_uuid: uuid.into(),
            email: format!("{uuid}@example.com"),
            organization_uuid: "org".into(),
        }
    }

    fn reading(percent: f64, source: Source, account: Option<&str>) -> Snapshot {
        Snapshot {
            windows: vec![Window {
                kind: "session".into(),
                scope: None,
                severity: None,
                percent,
                resets_at: Some(NOW + 3_600),
                is_active: true,
                length_seconds: None,
            }],
            observed_at: Some(NOW - 7_200),
            account_uuid: account.map(str::to_owned),
            source,
        }
    }

    fn parked(refresh_expires_at: i64) -> Park {
        Park {
            service: "pitboard-park-x-1".into(),
            parked_at: NOW - 86_400,
            refresh_fingerprint: "f".into(),
            access_expires_at: Some(NOW - 3_600),
            refresh_expires_at: Some(refresh_expires_at),
        }
    }

    fn account(label: &str) -> Account {
        Account {
            last_used_at: None,
            label: label.into(),
            account_uuid: format!("{label}-uuid"),
            email: format!("{label}@example.com"),
            detail: crate::state::Detail::Claude {
                organization_uuid: "org".into(),
                oauth_account: json!({}),
            },
            parked: Some(parked(NOW + 20 * 86_400)),
        }
    }

    fn state(labels: &[&str]) -> State {
        State {
            accounts: labels.iter().map(|l| account(l)).collect(),
            ..State::default()
        }
    }

    fn facts(
        signed_in: &str,
        live: Result<Snapshot, Stale>,
        parked: Vec<Result<Snapshot, Stale>>,
    ) -> Facts {
        only_claude(
            LiveLogin {
                signed_in: Some(Ok(owner(signed_in))),
                config_uuid: None,
                usage: Some(live),
            },
            parked,
        )
    }

    /// The one-tool case every test here was written for, in the shape facts now take.
    fn only_claude(live: LiveLogin, parked: Vec<Result<Snapshot, Stale>>) -> Facts {
        Facts {
            live: std::iter::once((ProviderId::Claude, live)).collect(),
            asked: true,
            parked_usage: parked,
            claude_code_cache: None,
        }
    }

    /// Anthropic decides who is signed in, but unreachable is not absent. Without this the
    /// account in use renders as one with nothing parked, advising a sign-in it does not
    /// need.
    #[test]
    fn an_unreachable_anthropic_leaves_the_signed_in_row_signed_in() {
        let state = state(&["alpha", "beta"]);
        let facts = Facts {
            live: std::iter::once((
                ProviderId::Claude,
                LiveLogin {
                    signed_in: Some(Err(ApiError::Network("offline".into()))),
                    config_uuid: Some("alpha-uuid".into()),
                    usage: Some(Err(Stale::Unreachable)),
                },
            ))
            .collect(),
            asked: true,
            parked_usage: vec![Err(Stale::Unreachable), Err(Stale::Unreachable)],
            claude_code_cache: None,
        };
        let rows = assemble(&state, &facts, nothing_remembered, nothing_known);
        let a = rows
            .iter()
            .find(|r| r.account_uuid == "alpha-uuid")
            .unwrap();
        assert!(
            a.signed_in,
            "the account in use is still the account in use"
        );
        let b = rows.iter().find(|r| r.account_uuid == "beta-uuid").unwrap();
        assert!(!b.signed_in);
    }

    fn nothing_remembered(_: &str) -> Option<Snapshot> {
        None
    }

    fn nothing_known(_: &str) -> crate::history::Runway {
        crate::history::Runway::Unknown
    }

    #[test]
    fn every_stale_code_is_its_json_form() {
        for stale in [
            Stale::NothingSignedIn,
            Stale::SessionExpired,
            Stale::ParkedAccessExpired,
            Stale::NothingParked,
            Stale::ParkUnreadable,
            Stale::RateLimited,
            Stale::Unreachable,
            Stale::ServerError,
            Stale::AnswerNotUnderstood,
            Stale::LoginRefused,
            Stale::Interrupted,
            Stale::NotAsked,
        ] {
            assert_eq!(serde_json::to_value(stale).unwrap(), stale.code());
        }
    }

    /// The three answers that used to arrive as one. What a front end does next differs for
    /// each: wait and ask again, stop asking because the shape moved, or tell the person
    /// their parked login is finished.
    #[test]
    fn anthropics_failures_are_told_apart() {
        use crate::api::ApiError;
        let parked = |e: &ApiError| Stale::of(e, false);
        assert_eq!(
            parked(&ApiError::Unexpected { status: 503 }),
            Stale::ServerError
        );
        assert_eq!(
            parked(&ApiError::Malformed("no windows".into())),
            Stale::AnswerNotUnderstood
        );
        assert_eq!(parked(&ApiError::InvalidGrant), Stale::LoginRefused);
        assert_eq!(
            parked(&ApiError::RateLimited { retry_after: None }),
            Stale::RateLimited
        );
        assert_eq!(
            parked(&ApiError::Network("down".into())),
            Stale::Unreachable
        );

        // The same token failure means different things for a live login and a parked one.
        assert_eq!(parked(&ApiError::Unauthorized), Stale::ParkedAccessExpired);
        assert_eq!(
            Stale::of(&ApiError::Unauthorized, true),
            Stale::SessionExpired
        );
    }

    #[test]
    fn a_live_reading_wins_over_claude_codes_cache() {
        let s = state(&["work"]);
        let mut f = facts(
            "work-uuid",
            Ok(reading(30.0, Source::Live, None)),
            vec![Err(Stale::NothingParked)],
        );
        f.claude_code_cache = Some(reading(2.0, Source::ClaudeCodeCache, Some("work-uuid")));
        let rows = assemble(&s, &f, nothing_remembered, nothing_known);
        let usage = rows[0].usage.as_ref().unwrap();
        assert_eq!(usage.source, Source::Live);
        assert_eq!(usage.windows[0].percent, 30.0);
        assert_eq!(rows[0].stale, None);
    }

    #[test]
    fn an_expired_session_falls_back_to_claude_codes_cache_and_says_why() {
        let s = state(&["work"]);
        let mut f = facts(
            "work-uuid",
            Err(Stale::SessionExpired),
            vec![Err(Stale::NothingParked)],
        );
        f.claude_code_cache = Some(reading(2.0, Source::ClaudeCodeCache, Some("work-uuid")));
        let rows = assemble(&s, &f, nothing_remembered, nothing_known);
        assert_eq!(
            rows[0].usage.as_ref().unwrap().source,
            Source::ClaudeCodeCache
        );
        assert_eq!(rows[0].stale, Some(Stale::SessionExpired));
    }

    #[test]
    fn claude_codes_cache_for_another_account_is_never_shown_as_this_one() {
        let s = state(&["work"]);
        let mut f = facts(
            "work-uuid",
            Err(Stale::Unreachable),
            vec![Err(Stale::NothingParked)],
        );
        f.claude_code_cache = Some(reading(99.0, Source::ClaudeCodeCache, Some("someone-else")));
        assert!(
            assemble(&s, &f, nothing_remembered, nothing_known)[0]
                .usage
                .is_none()
        );
    }

    #[test]
    fn a_parked_account_is_asked_with_its_own_login() {
        let s = state(&["work", "personal"]);
        let f = facts(
            "work-uuid",
            Ok(reading(30.0, Source::Live, None)),
            vec![
                Err(Stale::NothingParked),
                Ok(reading(12.0, Source::Live, None)),
            ],
        );
        let rows = assemble(&s, &f, nothing_remembered, nothing_known);
        let personal = rows
            .iter()
            .find(|r| r.label.as_deref() == Some("personal"))
            .unwrap();
        assert_eq!(personal.usage.as_ref().unwrap().windows[0].percent, 12.0);
        assert!(personal.switchable(NOW));
    }

    /// The login signed in has a session of its own tool's to renew, and a parked one does
    /// not. The same refusal means different things on each side, and saying "parked" about
    /// the login somebody is using sends them looking for a park that is not there.
    #[test]
    fn a_refused_token_is_an_expired_session_when_it_is_the_one_signed_in() {
        let api = crate::api::scripted::ScriptedApi::new();
        api.token_trouble("t", crate::api::scripted::Trouble::Unauthorized);
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"))
            .with_pitboard_home(std::env::temp_dir().join("pitboard-status-refused"))
            .with_scripted_api(api);
        let login = serde_json::json!({"claudeAiOauth": {"accessToken": "t"}});
        let ask = |signed_in| {
            ask_usage(
                &ctx,
                ProviderId::Claude,
                None,
                &login,
                signed_in,
                None,
                true,
            )
            .0
        };
        assert_eq!(ask(true).unwrap_err(), Stale::SessionExpired);
        assert_eq!(ask(false).unwrap_err(), Stale::ParkedAccessExpired);
    }

    /// A parked Codex login keeps its token where a Claude Code login does not. Reading it
    /// with Claude Code's layout called every Codex park unreadable; what is unreadable now
    /// is decided by the tool the park belongs to.
    #[test]
    fn a_park_not_in_its_own_tools_shape_is_unreadable_rather_than_an_answer() {
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"));
        let not_codex = serde_json::json!({"claudeAiOauth": {"accessToken": "t"}});
        let (answer, learned) =
            ask_usage(&ctx, ProviderId::Codex, None, &not_codex, false, None, true);
        assert_eq!(answer.unwrap_err(), Stale::ParkUnreadable);
        assert!(
            learned.is_none(),
            "nothing was asked, so there is nothing to learn"
        );
    }

    #[test]
    fn a_parked_login_past_its_access_expiry_is_not_asked() {
        let ctx = Context::from_env();
        let personal = Key::new(ProviderId::Claude, "personal");
        let token = parked_document(&ctx, &personal, Some(&parked(NOW + 86_400)), NOW);
        assert_eq!(token, Err(Stale::ParkedAccessExpired));
        assert_eq!(
            parked_document(&ctx, &personal, None, NOW),
            Err(Stale::NothingParked)
        );
    }

    #[test]
    fn what_cannot_be_asked_falls_back_to_what_was_remembered() {
        let s = state(&["work", "personal"]);
        let f = facts(
            "work-uuid",
            Ok(reading(30.0, Source::Live, None)),
            vec![Err(Stale::NothingParked), Err(Stale::ParkedAccessExpired)],
        );
        let remembered = |uuid: &str| {
            (uuid == "personal-uuid").then(|| reading(44.0, Source::Remembered, Some(uuid)))
        };
        let rows = assemble(&s, &f, remembered, nothing_known);
        let personal = rows
            .iter()
            .find(|r| r.label.as_deref() == Some("personal"))
            .unwrap();
        assert_eq!(personal.usage.as_ref().unwrap().source, Source::Remembered);
        assert_eq!(personal.stale, Some(Stale::ParkedAccessExpired));
    }

    #[test]
    fn the_signed_in_account_comes_first_and_is_taken_from_the_server_not_the_config() {
        let s = state(&["alpha", "beta"]);
        let f = facts(
            "beta-uuid",
            Ok(reading(5.0, Source::Live, None)),
            vec![Err(Stale::NothingParked), Err(Stale::NothingParked)],
        );
        let rows = assemble(&s, &f, nothing_remembered, nothing_known);
        assert_eq!(rows[0].label.as_deref(), Some("beta"));
        assert!(rows[0].signed_in && !rows[0].switchable(NOW));
        assert!(!rows[1].signed_in);
    }
}
