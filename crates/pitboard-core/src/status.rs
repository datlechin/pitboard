//! `pitboard status`: who is signed in, what each account has left, and which accounts can
//! be switched to.
//!
//! Numbers are asked of each tool's own service rather than read from a tool's cache, which
//! only moves when the tool itself asks. Every account is asked at once, so the command
//! costs one round trip, not one per account.
//!
//! Every row says which tool it belongs to. There is no such thing as "the account signed
//! in on this machine": each tool has a live login of its own or none, and a row is about
//! exactly one of them.

use crate::api::{self, ApiError, Owner};
use crate::context::Context;
use crate::error::Cause;
use crate::provider::claude::paths as claude;
use crate::provider::{ProviderError, ProviderId};
use crate::state::{Key, Park, State};
use crate::usage::{Snapshot, Source};
use crate::{budget, park, readings};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

/// Why a reading is not live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Stale {
    NothingSignedIn,
    /// The tool's live login is there to be read and could not be: a keychain that is
    /// locked for the moment, a store the tool's configuration puts where pitboard does not
    /// reach. Never the same as nothing signed in, which would tell somebody their login is
    /// gone when it is only out of reach.
    LoginUnreadable,
    /// The tool's live login was read and is not one account's login pitboard can park or
    /// switch: signed in with an API key rather than an account, or a Codex login whose
    /// tokens belong to one account and whose account id names another. Something is
    /// signed in, so saying nobody is would send somebody to sign in again for nothing.
    LoginUnusable,
    /// The tool's own session has expired; its next call renews it.
    SessionExpired,
    /// A parked login's access token has expired and could not be renewed this time.
    ParkedAccessExpired,
    NothingParked,
    ParkUnreadable,
    RateLimited,
    Unreachable,
    /// The service answered badly and may answer well later.
    ServerError,
    /// The service's answer was not in a shape pitboard understands, which means the shape
    /// moved. Asking again produces the same thing.
    AnswerNotUnderstood,
    /// The service will not accept this parked login again.
    LoginRefused,
    /// Asked recently enough that the answer cannot have moved by a percentage point, so
    /// the number shown is the one already measured rather than a new request.
    AskedRecently,
    /// The thread that was asking stopped before it answered. Nothing to do with the
    /// service, which is why it used to be filed under the same code as a bad answer.
    Interrupted,
    /// Nobody was asked: this reading was taken without touching the network.
    NotAsked,
}

impl Stale {
    /// Three answers used to arrive here as one. A front end deciding whether to ask
    /// again needs to tell a bad morning at the service from an answer whose shape moved
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
            Stale::LoginUnreadable => "login_unreadable",
            Stale::LoginUnusable => "login_unusable",
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

    /// What is worth a word, in the default tool's words.
    ///
    /// For a caller holding a code and nothing else. A row knows its own tool, and
    /// [`Row::explanation`] says it in that tool's words.
    pub fn explanation(self) -> Option<&'static str> {
        self.explanation_for(crate::label::DEFAULT)
    }

    /// Only what is worth a word: a parked login going quiet is how parking works.
    ///
    /// Named for the tool the row belongs to. "Anthropic could not be reached" said about a
    /// Codex account sends somebody to check the wrong service, and "`claude` renews it"
    /// said about a Codex session names a program that has nothing to do with it.
    pub fn explanation_for(self, provider: ProviderId) -> Option<&'static str> {
        // A sentence per tool rather than one assembled at run time, so every word a person
        // reads is here to be read, and Claude Code's are exactly what they always were.
        let per_tool = |claude: &'static str, codex: &'static str| match provider {
            ProviderId::Claude => claude,
            ProviderId::Codex => codex,
        };
        match self {
            Stale::NothingSignedIn => Some("nothing is signed in"),
            Stale::LoginUnreadable => Some(per_tool(
                "Claude Code's login could not be read; run `pitboard doctor`",
                "Codex's login could not be read; run `pitboard doctor`",
            )),
            Stale::LoginUnusable => Some(per_tool(
                "Claude Code's login is not one pitboard can park or switch; run `pitboard doctor`",
                "Codex's login is not one pitboard can park or switch; run `pitboard doctor`",
            )),
            Stale::SessionExpired => Some(per_tool(
                "Claude Code's session has expired; `claude` renews it",
                "Codex's session has expired; `codex` renews it",
            )),
            Stale::ParkedAccessExpired | Stale::NothingParked => None,
            Stale::ParkUnreadable => Some("its parked login cannot be read; run `pitboard doctor`"),
            Stale::RateLimited => Some(per_tool(
                "Anthropic is rate limiting usage checks",
                "OpenAI is rate limiting usage checks",
            )),
            Stale::Unreachable => Some(per_tool(
                "Anthropic could not be reached",
                "OpenAI could not be reached",
            )),
            Stale::ServerError => Some(per_tool(
                "Anthropic answered with an error; try again later",
                "OpenAI answered with an error; try again later",
            )),
            Stale::AnswerNotUnderstood => Some(per_tool(
                "Anthropic's answer was not understood",
                "OpenAI's answer was not understood",
            )),
            Stale::LoginRefused => Some("its parked login is no longer accepted; sign in again"),
            // Not worth a word: it is the ordinary state of a number that is
            // already as true as asking again would make it.
            Stale::AskedRecently => None,
            Stale::Interrupted => Some("the check did not finish"),
            Stale::NotAsked => Some(per_tool(
                "read without asking Anthropic",
                "read without asking OpenAI",
            )),
        }
    }
}

pub struct Row {
    /// Which tool this account belongs to, or whose live login this row is.
    ///
    /// Every row has one, including a login nobody has enrolled and a login that could not
    /// be read, so a front end can always say which tool a row is about and what to type to
    /// act on it. A hint that said `pitboard enroll work --sign-in` about a Codex account
    /// would have enrolled a Claude Code one.
    pub provider: ProviderId,
    /// `None` for a login nothing has enrolled: one that is signed in, or one pitboard
    /// could not pin on any account (see [`Row::unplaced`]).
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

    /// The account, the way pitboard tells accounts apart. `None` for a row nothing has
    /// enrolled.
    pub fn key(&self) -> Option<Key> {
        self.label
            .as_ref()
            .map(|label| Key::new(self.provider, label.clone()))
    }

    /// What to tell a person about [`Row::stale`], in the words of this row's own tool.
    pub fn explanation(&self) -> Option<&'static str> {
        self.stale
            .and_then(|stale| stale.explanation_for(self.provider))
    }

    /// A tool's live login that pitboard could not pin on any account: one it could not
    /// read, or read and could not use. It has no email and no account id, and nothing to
    /// type about it but `pitboard doctor`, so a front end says what it is rather than
    /// showing it as an account nobody has enrolled.
    pub fn unplaced(&self) -> bool {
        self.label.is_none()
            && !self.signed_in
            && matches!(
                self.stale,
                Some(Stale::LoginUnreadable | Stale::LoginUnusable)
            )
    }
}

pub struct Report {
    pub now: i64,
    pub rows: Vec<Row>,
    /// Who the default tool's live login belongs to, as its service says: Claude Code's
    /// config can be a day behind. Every tool's answer is in the rows.
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
    /// Whose it is. `None` means nobody is signed in to this tool, but only when
    /// [`Facts::asked`] is true: unasked and absent are different things, and reading one as
    /// the other says the account in use is gone. `Some(Err)` is a login whose owner could
    /// not be learned this time, and says why.
    signed_in: Option<Result<Owner, String>>,
    /// The account the tool's own local record names, where it keeps one. It can be behind
    /// the login it describes, so it never decides a switch; it keeps the row that is
    /// signed in from reading as though nobody is, when the service cannot be asked.
    recorded_uuid: Option<String>,
    usage: Option<Result<Snapshot, Stale>>,
    /// There is a login and pitboard cannot use it: it could not be read, or it was read and
    /// is not one account's login. A tool in this state whose record names none of its
    /// accounts still gets a row, so that nobody reads its silence as nobody signed in.
    out_of_reach: bool,
}

impl LiveLogin {
    fn usage(&self) -> Result<Snapshot, Stale> {
        self.usage.clone().unwrap_or(Err(Stale::NothingSignedIn))
    }
}

/// Everything gathered from the machine and the network, so assembling it touches neither.
struct Facts {
    live: BTreeMap<ProviderId, LiveLogin>,
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

/// Which slot this machine's `claude` would read right now.
fn slot_of(ctx: &Context) -> Slot {
    Slot {
        service: claude::live_service(ctx),
        default: claude::is_default_slot(ctx),
    }
}

/// What is known without asking anyone: who each tool's own record says is signed in, and
/// the last numbers pitboard measured. Touches no network, so it answers at once and works
/// on a train.
///
/// Each tool is asked for what it keeps without anybody's agreement: Claude Code's config,
/// which can be a day behind the login it describes, and a Codex login's own claims. Good
/// enough to say who is in use; never good enough to move a login. It used to read Claude
/// Code's alone, so a signed-in Codex account read as parked whenever the app had no
/// network.
pub fn gather_offline(ctx: &Context, state: &State) -> Report {
    let recorded: BTreeMap<ProviderId, crate::provider::Identity> = ProviderId::ALL
        .iter()
        .filter_map(|&which| Some((which, crate::provider::of(which).recorded_identity(ctx)?)))
        .collect();
    let facts = Facts {
        live: ProviderId::ALL
            .iter()
            .map(|&which| {
                let login = LiveLogin {
                    recorded_uuid: recorded.get(&which).map(|id| id.account_id.clone()),
                    usage: Some(Err(Stale::NotAsked)),
                    ..LiveLogin::default()
                };
                (which, login)
            })
            .collect(),
        asked: false,
        parked_usage: state
            .accounts
            .iter()
            .map(|_| Err(Stale::NotAsked))
            .collect(),
        claude_code_cache: claude::load_config(ctx)
            .ok()
            .as_ref()
            .and_then(crate::usage::from_config_cache),
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
        signed_in: recorded.get(&crate::label::DEFAULT).map_or_else(
            || Err("not asked".into()),
            |id| {
                Ok(Owner {
                    account_uuid: id.account_id.clone(),
                    email: id.email.clone(),
                    organization_uuid: id.group.clone().unwrap_or_default(),
                })
            },
        ),
    }
}

/// What one account's request came to: the reading, and what the budget should learn from
/// it. Recorded by the caller once every request has finished, never from the thread that
/// made it.
type Asked = (Result<Snapshot, Stale>, Option<(String, budget::Outcome)>);

/// What one tool's live login came to, and what the budget should learn from asking about
/// it.
type Answered = (LiveLogin, Option<(String, budget::Outcome)>);

/// A provider's failure as the usage path has always classified one.
fn to_api(error: crate::provider::ProviderError) -> ApiError {
    use crate::provider::ProviderError as P;
    match error {
        P::Unauthorized => ApiError::Unauthorized,
        P::RateLimited { retry_after, .. } => ApiError::RateLimited { retry_after },
        P::Network { detail, .. } => ApiError::Network(detail),
        P::Unexpected { status, .. } => ApiError::Unexpected { status },
        P::Malformed { detail, .. }
        | P::ShapeUnexpected { detail, .. }
        | P::Unsupported { reason: detail, .. } => ApiError::Malformed(detail),
        P::NoLogin { .. } => ApiError::Malformed("nothing is signed in".into()),
        P::InvalidGrant { .. } => ApiError::InvalidGrant,
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

/// Ask about one tool's live login: whose it is, and what it has left.
///
/// `read` is what reading the tool's store came to, kept whole. An error there is not
/// nothing signed in: the login may be exactly where it always was, behind a keychain that
/// is locked for the moment, and saying it is gone sends somebody to sign in again for
/// nothing. It used to be read as exactly that.
///
/// `active` is the account pitboard last recorded as signed in to this tool, which stands
/// in for the tool's own record when the login cannot be read at all. Codex keeps no
/// record apart from the login itself, so without this a Codex login caught half written
/// named nobody, and the account in use was told to sign in again while `doctor`, reading
/// the same machine, called it signed in.
fn ask_live(
    ctx: &Context,
    which: ProviderId,
    read: &Result<Option<Value>, ProviderError>,
    active: Option<&str>,
    remembered: &HashMap<String, Snapshot>,
    fresh: bool,
) -> Answered {
    let tool = crate::provider::of(which);
    let own_record = || tool.recorded_identity(ctx).map(|id| id.account_id);
    let document = match read {
        Ok(Some(document)) => document,
        Ok(None) => return (LiveLogin::default(), None),
        Err(error) => {
            let unreadable = LiveLogin {
                signed_in: Some(Err(error.to_string())),
                recorded_uuid: own_record().or_else(|| active.map(str::to_owned)),
                usage: Some(Err(Stale::LoginUnreadable)),
                out_of_reach: true,
            };
            return (unreadable, None);
        }
    };
    match tool.slice(document) {
        Ok(_) => {}
        // A document holding no account's login at all. Claude Code's `/logout` deletes the
        // login and leaves the document behind with the machine's MCP tokens still in it;
        // asking Anthropic whose that was got an answer about a shape rather than a person,
        // and the row read as Anthropic answering badly when the truth is that nobody is
        // signed in.
        Err(ProviderError::NoLogin { .. }) => return (LiveLogin::default(), None),
        // A login that is there and is not one account's: signed in with an API key, or a
        // Codex login that a session still running from before a switch rewrote with its
        // own account's tokens under the other account's id. Something is signed in, so
        // it is not nobody; nobody is asked about it, because there is no one account's
        // token to ask with. The tool's own record says whose it most likely is, and
        // pitboard's own record does not stand in, because what is there was read and is
        // not that account's.
        Err(unusable) => {
            let login = LiveLogin {
                signed_in: Some(Err(unusable.to_string())),
                recorded_uuid: own_record(),
                usage: Some(Err(Stale::LoginUnusable)),
                out_of_reach: true,
            };
            return (login, None);
        }
    }
    let credential = crate::provider::Credential::new(which, document.clone());
    // Who owns it is asked whatever the budget says: it decides which account a row
    // belongs to, it is not a measurement, and a switch needs it.
    let owner = tool
        .identify(ctx, &credential)
        .map(|found| Owner {
            account_uuid: found.account_id,
            email: found.email,
            organization_uuid: found.group.unwrap_or_default(),
        })
        .map_err(|e| to_api(e).to_string());
    // The tool's own record, for when its service could not say whose the login is. It
    // stands in twice: to keep the signed-in row signed in, and to key the budget. Without
    // the second, a morning when identifying fails is a morning when every `status` asks
    // about usage with no floor at all, which is what the budget exists to stop. Claude
    // Code's account always came from its config here before there was a second tool.
    let recorded = match &owner {
        Ok(_) => None,
        Err(_) => own_record(),
    };
    let uuid = owner
        .as_ref()
        .ok()
        .map(|o| o.account_uuid.clone())
        .or_else(|| recorded.clone());
    let (usage, learned) = ask_usage(
        ctx,
        which,
        uuid.as_deref(),
        document,
        true,
        uuid.as_deref().and_then(|u| remembered.get(u)),
        fresh,
    );
    let login = LiveLogin {
        signed_in: Some(owner),
        recorded_uuid: recorded,
        usage: Some(usage),
        out_of_reach: false,
    };
    (login, learned)
}

/// Every tool's answer, filed under the tool it was asked about.
///
/// A thread that stopped before answering stands in for its own tool and nobody else's. It
/// used to be replaced by an empty answer filed under Claude Code whichever tool it had
/// been asking about, and because answers are collected in order, a Codex thread that
/// panicked quietly erased Claude Code's signed-in row.
fn settle(
    ctx: &Context,
    answers: Vec<(ProviderId, std::thread::Result<Answered>)>,
) -> (
    BTreeMap<ProviderId, LiveLogin>,
    Vec<(String, budget::Outcome)>,
) {
    let mut live = BTreeMap::new();
    let mut learned = Vec::new();
    for (which, answer) in answers {
        let (login, outcome) = answer.unwrap_or_else(|_| {
            // Whose the login is was never learned, so the tool's own record stands in for
            // it, the same as when its service cannot be reached.
            let interrupted = LiveLogin {
                signed_in: Some(Err("the check did not finish".into())),
                recorded_uuid: crate::provider::of(which)
                    .recorded_identity(ctx)
                    .map(|id| id.account_id),
                usage: Some(Err(Stale::Interrupted)),
                out_of_reach: false,
            };
            (interrupted, None)
        });
        live.insert(which, login);
        learned.extend(outcome);
    }
    (live, learned)
}

pub fn gather(ctx: &Context, state: &State, fresh: bool) -> Report {
    let now = ctx.now();
    // Each tool's live login, whole, or why it could not be read. Not a token out of it:
    // what a usage call needs is not the same everywhere, and pulling one field out here
    // would decide that for all of them.
    let live_documents: Vec<(ProviderId, Result<Option<Value>, ProviderError>)> = ProviderId::ALL
        .iter()
        .map(|&which| {
            let read = crate::provider::of(which)
                .read_live(ctx)
                .map(|found| found.map(|credential| credential.raw));
            (which, read)
        })
        .collect();
    let parked_documents: Vec<Result<Value, Stale>> = state
        .accounts
        .iter()
        .map(|a| parked_document(ctx, &a.key(), a.parked.as_ref(), now))
        .collect();
    let config = claude::load_config(ctx).ok();
    let remembered = readings::load(ctx);

    let (answers, parked_asked): (Vec<(ProviderId, std::thread::Result<Answered>)>, Vec<Asked>) =
        std::thread::scope(|scope| {
            // Each handle stays with the tool it asks about, so a thread that panics can
            // only ever stand in for its own tool.
            let per_tool: Vec<_> = live_documents
                .iter()
                .map(|(which, read)| {
                    let remembered = &remembered;
                    let which = *which;
                    let active = active_uuid(state, which);
                    let handle =
                        scope.spawn(move || ask_live(ctx, which, read, active, remembered, fresh));
                    (which, handle)
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
                    .map(|(which, handle)| (which, handle.join()))
                    .collect(),
                parked
                    .into_iter()
                    .map(|h| h.join().unwrap_or((Err(Stale::Interrupted), None)))
                    .collect(),
            )
        });
    let (live, live_learned) = settle(ctx, answers);

    // Once, from one thread. Every account's record lives in one file, so a thread each
    // reading it, changing one entry and writing it back would erase what the others
    // learned.
    let learned: Vec<(String, budget::Outcome)> = live_learned
        .into_iter()
        .chain(
            parked_asked
                .iter()
                .filter_map(|(_, learned)| learned.clone()),
        )
        .collect();
    budget::record(ctx, &learned);

    let signed_in_default = live
        .get(&crate::label::DEFAULT)
        .and_then(|login| login.signed_in.clone());
    let facts = Facts {
        live,
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
            Some(Err(why)) => Err(why),
            None => Err("nothing is signed in".into()),
        },
    }
}

/// The account pitboard last recorded as signed in to this tool, by its id.
fn active_uuid(state: &State, which: ProviderId) -> Option<&str> {
    let label = state.active_for(which)?;
    state
        .get(&Key::new(which, label))
        .map(|account| account.account_uuid.as_str())
}

/// Where a tool comes in a listing: the order [`ProviderId::ALL`] gives them.
fn rank(which: ProviderId) -> usize {
    ProviderId::ALL
        .iter()
        .position(|&known| known == which)
        .unwrap_or(usize::MAX)
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
            // Unreachable, unreadable, or never asked. The service decides who is signed
            // in; without its answer, the tool's own local record is the only one there is.
            (Some(Err(_)), _) | (None, false) => live.recorded_uuid.as_deref(),
            (None, true) => None,
        }
    };
    // Claude Code's cache counts only for Claude Code's accounts, and only when it was
    // measured for the account in question.
    let cached_for = |which: ProviderId, uuid: &str| {
        facts
            .claude_code_cache
            .clone()
            .filter(|c| which == ProviderId::Claude && c.account_uuid.as_deref() == Some(uuid))
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
                reading(&live, cached_for(which, uuid).or_else(|| recall(uuid)))
            } else {
                reading(parked, recall(uuid))
            };
            Row {
                provider: which,
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
    // see what is there and give it a name. One per tool, because each can have its own,
    // and told apart by tool as well as by account: two tools' identities are two
    // namespaces, and one tool's row must never stand in for another's.
    for &which in ProviderId::ALL {
        let Some(live) = facts.live_for(which) else {
            continue;
        };
        if let Some(Ok(owner)) = &live.signed_in {
            if rows
                .iter()
                .any(|r| r.provider == which && r.account_uuid == owner.account_uuid)
            {
                continue;
            }
            let (usage, stale) = reading(&live.usage(), cached_for(which, &owner.account_uuid));
            rows.push(Row {
                provider: which,
                label: None,
                email: owner.email.clone(),
                account_uuid: owner.account_uuid.clone(),
                signed_in: true,
                parked: None,
                usage,
                stale,
                runway: lasting(&owner.account_uuid),
            });
            continue;
        }
        // A login that could not be read, or was read and is not one account's, and that no
        // record pins on any of the tool's accounts. Said, rather than left out, because
        // leaving it out reads as nobody being signed in; but only for a tool somebody uses
        // through pitboard, so a machine that has never enrolled a Codex account sees
        // nothing about Codex at all.
        let enrolled = state.accounts.iter().any(|a| a.provider() == which);
        let placed = rows.iter().any(|r| r.provider == which && r.signed_in);
        if live.out_of_reach && enrolled && !placed {
            rows.push(Row {
                provider: which,
                label: None,
                email: String::new(),
                account_uuid: String::new(),
                signed_in: false,
                parked: None,
                usage: None,
                stale: live.usage().err(),
                runway: crate::history::Runway::Unknown,
            });
        }
    }

    // Grouped by tool, in the order every listing uses, and the signed-in account first
    // within each. Stable, so a machine with one tool reads in exactly the order it always
    // did.
    rows.sort_by_key(|r| (rank(r.provider), !r.signed_in));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::scripted::{Asked as Question, ScriptedApi, Trouble};
    use crate::state::Account;
    use crate::store::memory::{Fault, MemoryHost};
    use crate::time::{Clock, FixedClock};
    use crate::usage::Window;
    use serde_json::json;
    use std::sync::Arc;

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
                usage: Some(live),
                ..LiveLogin::default()
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
                    signed_in: Some(Err("could not reach Anthropic: offline".into())),
                    recorded_uuid: Some("alpha-uuid".into()),
                    usage: Some(Err(Stale::Unreachable)),
                    out_of_reach: false,
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
            Stale::LoginUnreadable,
            Stale::LoginUnusable,
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

    /// Every stale reason, for the tests that hold each one to something.
    const EVERY_STALE: [Stale; 15] = [
        Stale::NothingSignedIn,
        Stale::LoginUnreadable,
        Stale::LoginUnusable,
        Stale::SessionExpired,
        Stale::ParkedAccessExpired,
        Stale::NothingParked,
        Stale::ParkUnreadable,
        Stale::RateLimited,
        Stale::Unreachable,
        Stale::ServerError,
        Stale::AnswerNotUnderstood,
        Stale::LoginRefused,
        Stale::AskedRecently,
        Stale::Interrupted,
        Stale::NotAsked,
    ];

    /// What a Claude Code row said before a row knew its tool, word for word. A person who
    /// has only ever used Claude Code must not read a single word differently.
    #[test]
    fn claude_codes_explanations_are_word_for_word_what_they_were() {
        let before = |stale: Stale| match stale {
            Stale::NothingSignedIn => Some("nothing is signed in"),
            Stale::SessionExpired => Some("Claude Code's session has expired; `claude` renews it"),
            Stale::ParkedAccessExpired | Stale::NothingParked | Stale::AskedRecently => None,
            Stale::ParkUnreadable => Some("its parked login cannot be read; run `pitboard doctor`"),
            Stale::RateLimited => Some("Anthropic is rate limiting usage checks"),
            Stale::Unreachable => Some("Anthropic could not be reached"),
            Stale::ServerError => Some("Anthropic answered with an error; try again later"),
            Stale::AnswerNotUnderstood => Some("Anthropic's answer was not understood"),
            Stale::LoginRefused => Some("its parked login is no longer accepted; sign in again"),
            Stale::Interrupted => Some("the check did not finish"),
            Stale::NotAsked => Some("read without asking Anthropic"),
            // New, so there is no before to hold them to.
            Stale::LoginUnreadable => {
                Some("Claude Code's login could not be read; run `pitboard doctor`")
            }
            Stale::LoginUnusable => Some(
                "Claude Code's login is not one pitboard can park or switch; run `pitboard doctor`",
            ),
        };
        for stale in EVERY_STALE {
            assert_eq!(
                stale.explanation_for(ProviderId::Claude),
                before(stale),
                "{stale:?}"
            );
            assert_eq!(
                stale.explanation(),
                stale.explanation_for(ProviderId::Claude),
                "a caller with only a code gets the default tool's words"
            );
        }
    }

    /// "Anthropic could not be reached" said about a Codex account sends somebody to check
    /// the wrong service, and "`claude` renews it" names a program that has nothing to do
    /// with a Codex session.
    #[test]
    fn a_codex_row_names_openai_and_codex_never_anthropic_or_claude() {
        for stale in EVERY_STALE {
            let Some(said) = stale.explanation_for(ProviderId::Codex) else {
                assert_eq!(stale.explanation_for(ProviderId::Claude), None, "{stale:?}");
                continue;
            };
            for other in ["Anthropic", "Claude", "claude"] {
                assert!(!said.contains(other), "{stale:?}: {said}");
            }
        }
        assert_eq!(
            Stale::Unreachable.explanation_for(ProviderId::Codex),
            Some("OpenAI could not be reached")
        );
        assert_eq!(
            Stale::SessionExpired.explanation_for(ProviderId::Codex),
            Some("Codex's session has expired; `codex` renews it")
        );

        // And a row asks in its own tool's words without the caller having to know which.
        let mut codex = codex_account("work", "work-acc");
        codex.parked = None;
        let s = State {
            accounts: vec![codex],
            ..State::default()
        };
        let f = Facts {
            live: BTreeMap::new(),
            asked: true,
            parked_usage: vec![Err(Stale::Unreachable)],
            claude_code_cache: None,
        };
        let rows = assemble(&s, &f, nothing_remembered, nothing_known);
        assert_eq!(rows[0].explanation(), Some("OpenAI could not be reached"));
    }

    fn codex_account(label: &str, uuid: &str) -> Account {
        Account {
            last_used_at: None,
            label: label.into(),
            account_uuid: uuid.into(),
            email: format!("{label}@example.com"),
            detail: crate::state::Detail::Codex {
                workspace_id: None,
                plan: None,
            },
            parked: Some(parked(NOW + 20 * 86_400)),
        }
    }

    fn live(owner_uuid: &str, usage: Result<Snapshot, Stale>) -> LiveLogin {
        LiveLogin {
            signed_in: Some(Ok(owner(owner_uuid))),
            usage: Some(usage),
            ..LiveLogin::default()
        }
    }

    /// Every row says which tool it is for, the unenrolled one included, and the same
    /// identity on two tools is two rows. Deduplicated by identity alone, a Codex login
    /// whose id happened to match an enrolled Claude Code account vanished from the list.
    #[test]
    fn the_same_identity_on_two_tools_is_two_rows() {
        let s = state(&["same"]);
        let f = Facts {
            live: [
                (
                    ProviderId::Claude,
                    live("same-uuid", Ok(reading(10.0, Source::Live, None))),
                ),
                (
                    ProviderId::Codex,
                    live("same-uuid", Ok(reading(20.0, Source::Live, None))),
                ),
            ]
            .into_iter()
            .collect(),
            asked: true,
            parked_usage: vec![Err(Stale::NothingParked)],
            claude_code_cache: None,
        };
        let rows = assemble(&s, &f, nothing_remembered, nothing_known);
        assert_eq!(rows.len(), 2, "one row per tool");
        assert_eq!(rows[0].provider, ProviderId::Claude);
        assert_eq!(rows[0].label.as_deref(), Some("same"));
        assert_eq!(rows[1].provider, ProviderId::Codex);
        assert_eq!(rows[1].label, None, "nothing has enrolled the Codex login");
        assert!(rows[1].signed_in);
        assert_eq!(rows[1].usage.as_ref().unwrap().windows[0].percent, 20.0);
    }

    /// Grouped by tool in the order every listing uses, signed in first within each, and
    /// otherwise in the order they were enrolled.
    #[test]
    fn rows_are_grouped_by_tool_with_the_signed_in_account_first_in_each() {
        let mut s = state(&["alpha"]);
        s.accounts.push(codex_account("work", "work-acc"));
        s.accounts.push(account("beta"));
        s.accounts.push(codex_account("home", "home-acc"));
        let f = Facts {
            live: [
                (
                    ProviderId::Claude,
                    live("beta-uuid", Ok(reading(5.0, Source::Live, None))),
                ),
                (
                    ProviderId::Codex,
                    live("home-acc", Ok(reading(6.0, Source::Live, None))),
                ),
            ]
            .into_iter()
            .collect(),
            asked: true,
            parked_usage: vec![Err(Stale::NothingParked); 4],
            claude_code_cache: None,
        };
        let order: Vec<(ProviderId, String, bool)> =
            assemble(&s, &f, nothing_remembered, nothing_known)
                .into_iter()
                .map(|r| (r.provider, r.label.unwrap_or_default(), r.signed_in))
                .collect();
        assert_eq!(
            order,
            [
                (ProviderId::Claude, "beta".into(), true),
                (ProviderId::Claude, "alpha".into(), false),
                (ProviderId::Codex, "home".into(), true),
                (ProviderId::Codex, "work".into(), false),
            ]
        );
    }

    /// Claude Code's cache is a Claude Code account's numbers and nobody else's, however
    /// the identities happen to line up.
    #[test]
    fn claude_codes_cache_is_never_shown_for_another_tools_account() {
        let mut s = State::default();
        s.accounts.push(codex_account("work", "work-acc"));
        let mut f = Facts {
            live: std::iter::once((ProviderId::Codex, live("work-acc", Err(Stale::Unreachable))))
                .collect(),
            asked: true,
            parked_usage: vec![Err(Stale::NothingParked)],
            claude_code_cache: None,
        };
        f.claude_code_cache = Some(reading(77.0, Source::ClaudeCodeCache, Some("work-acc")));
        let rows = assemble(&s, &f, nothing_remembered, nothing_known);
        assert!(rows[0].signed_in);
        assert!(rows[0].usage.is_none(), "{:?}", rows[0].usage);
    }

    /// A thread that panicked stands in for its own tool and nobody else's. It used to be
    /// filed under Claude Code whichever tool it had been asking about, and since answers
    /// are collected in order, a Codex thread that panicked erased Claude Code's signed-in
    /// row.
    #[test]
    fn a_thread_that_stopped_stands_in_for_its_own_tool_only() {
        let home = scratch("panicked");
        let (ctx, _mem, _api) = machine(&home.0, None);
        let claude_answer: Answered = (
            live("alpha-uuid", Ok(reading(30.0, Source::Live, None))),
            Some(("alpha-uuid".into(), budget::Outcome::Answered)),
        );
        let panicked: Box<dyn std::any::Any + Send> = Box::new("boom");
        let (settled, learned) = settle(
            &ctx,
            vec![
                (ProviderId::Claude, Ok(claude_answer)),
                (ProviderId::Codex, Err(panicked)),
            ],
        );
        assert_eq!(
            learned,
            [("alpha-uuid".to_string(), budget::Outcome::Answered)]
        );
        let codex = &settled[&ProviderId::Codex];
        assert_eq!(codex.usage().err(), Some(Stale::Interrupted));
        assert!(
            matches!(codex.signed_in, Some(Err(_))),
            "unknown, not nobody"
        );

        let mut s = state(&["alpha"]);
        s.accounts.push(codex_account("work", "work-acc"));
        let f = Facts {
            live: settled,
            asked: true,
            parked_usage: vec![Err(Stale::NothingParked), Err(Stale::NothingParked)],
            claude_code_cache: None,
        };
        let rows = assemble(&s, &f, nothing_remembered, nothing_known);
        let alpha = rows
            .iter()
            .find(|r| r.provider == ProviderId::Claude)
            .unwrap();
        assert!(alpha.signed_in, "Claude Code's row survives Codex's thread");
        assert_eq!(alpha.usage.as_ref().unwrap().windows[0].percent, 30.0);
    }

    /// Claude Code's `/logout` deletes the login and leaves the document behind, still
    /// holding the machine's MCP tokens. That is nobody signed in. It used to be sent to
    /// Anthropic to identify, and the row read as Anthropic answering badly.
    #[test]
    fn a_document_holding_no_account_is_nothing_signed_in_and_nobody_is_asked() {
        let home = scratch("logged-out");
        let (ctx, _mem, api) = machine(&home.0, None);
        let logged_out = json!({"mcpOAuth": {"some-server": {"token": "unrelated"}}});
        let (login, learned) = ask_live(
            &ctx,
            ProviderId::Claude,
            &Ok(Some(logged_out)),
            None,
            &HashMap::new(),
            true,
        );
        assert!(login.signed_in.is_none(), "nobody, rather than unknown");
        assert_eq!(login.usage().err(), Some(Stale::NothingSignedIn));
        assert!(learned.is_none());
        assert_eq!(api.calls(), 0, "Anthropic was asked: {:?}", api.asked());
    }

    /// A keychain that is locked for the moment is not a login that is gone. Read as one,
    /// the account in use showed as parked with nothing parked, and the advice was to sign
    /// in again.
    #[test]
    fn a_login_that_cannot_be_read_is_said_rather_than_read_as_nobody_signed_in() {
        let home = scratch("unreadable");
        let (ctx, mem, api) = machine(&home.0, Some("alpha-uuid"));
        let service = claude::live_service(&ctx);
        mem.live().plant(
            &service,
            &json!({"claudeAiOauth": {"accessToken": "t"}}).to_string(),
        );
        mem.live()
            .fault(&service, Fault::Unreadable("the keychain is locked".into()));
        let mut s = state(&["alpha", "beta"]);
        for a in &mut s.accounts {
            a.parked = None;
        }

        let report = gather(&ctx, &s, true);
        let alpha = report
            .rows
            .iter()
            .find(|r| r.label.as_deref() == Some("alpha"))
            .unwrap();
        assert!(
            alpha.signed_in,
            "Claude Code's own record still names the account in use"
        );
        assert_eq!(alpha.stale, Some(Stale::LoginUnreadable));
        assert_eq!(
            alpha.explanation(),
            Some("Claude Code's login could not be read; run `pitboard doctor`")
        );
        let why = report.signed_in.expect_err("nobody could say whose it is");
        assert_ne!(why, "nothing is signed in");
        assert_eq!(api.calls(), 0, "a login nobody could read was sent nowhere");
    }

    /// A tool whose login could not be read and whose own record names none of its
    /// accounts gets a row of its own, so its silence is not read as nobody signed in. Only
    /// for a tool somebody uses through pitboard: a machine that has never enrolled a Codex
    /// account sees nothing about Codex.
    #[test]
    fn an_unreadable_login_gets_a_row_only_for_a_tool_with_accounts() {
        let unreadable = || LiveLogin {
            signed_in: Some(Err("the keychain is locked".into())),
            usage: Some(Err(Stale::LoginUnreadable)),
            out_of_reach: true,
            ..LiveLogin::default()
        };
        let facts_with = |parked: usize| Facts {
            live: [
                (
                    ProviderId::Claude,
                    live("alpha-uuid", Ok(reading(5.0, Source::Live, None))),
                ),
                (ProviderId::Codex, unreadable()),
            ]
            .into_iter()
            .collect(),
            asked: true,
            parked_usage: vec![Err(Stale::NothingParked); parked],
            claude_code_cache: None,
        };

        let claude_only = state(&["alpha"]);
        let rows = assemble(
            &claude_only,
            &facts_with(1),
            nothing_remembered,
            nothing_known,
        );
        assert_eq!(
            rows.len(),
            1,
            "nothing new for somebody who never used Codex"
        );

        let mut both = state(&["alpha"]);
        both.accounts.push(codex_account("work", "work-acc"));
        let rows = assemble(&both, &facts_with(2), nothing_remembered, nothing_known);
        let said = rows
            .iter()
            .find(|r| r.provider == ProviderId::Codex && r.label.is_none())
            .expect("a row saying Codex's login could not be read");
        assert_eq!(said.stale, Some(Stale::LoginUnreadable));
        assert!(!said.signed_in && !said.switchable(NOW));
        assert_eq!(
            said.explanation(),
            Some("Codex's login could not be read; run `pitboard doctor`")
        );
        let work = rows
            .iter()
            .find(|r| r.label.as_deref() == Some("work"))
            .unwrap();
        assert!(!work.signed_in);
    }

    /// When whose the login is cannot be learned, the tool's own record keys the budget.
    /// Keyed on nothing, a morning when Anthropic's profile endpoint is failing was a
    /// morning when every `status` asked about usage with no floor at all. Before there was
    /// a second tool the account always came from Claude Code's config here.
    #[test]
    fn a_failing_identify_still_keeps_the_ask_again_floor() {
        let home = scratch("floor");
        let (ctx, _mem, api) = machine(&home.0, Some("acc-x"));
        api.token_trouble("access-x", Trouble::Offline);
        budget::record(&ctx, &[("acc-x".into(), budget::Outcome::Answered)]);
        let login = json!({"claudeAiOauth": {"accessToken": "access-x"}});

        let (live, learned) = ask_live(
            &ctx,
            ProviderId::Claude,
            &Ok(Some(login.clone())),
            None,
            &HashMap::new(),
            false,
        );
        assert_eq!(live.usage().err(), Some(Stale::AskedRecently));
        assert_eq!(live.recorded_uuid.as_deref(), Some("acc-x"));
        assert!(learned.is_none());
        assert_eq!(
            api.asked(),
            [Question::Owner("access-x".into())],
            "whose it is is always asked; what it has left is not, inside the floor"
        );

        // Asked for anyway, what is learned is kept under the same account.
        let (live, learned) = ask_live(
            &ctx,
            ProviderId::Claude,
            &Ok(Some(login)),
            None,
            &HashMap::new(),
            true,
        );
        assert_eq!(live.usage().err(), Some(Stale::Unreachable));
        assert_eq!(
            learned,
            Some(("acc-x".to_string(), budget::Outcome::Unreachable))
        );
    }

    /// Offline, each tool is asked for its own record. It used to be Claude Code's config
    /// alone, so on a plane a signed-in Codex account read as parked.
    #[test]
    fn offline_every_tool_names_its_own_signed_in_account() {
        let home = scratch("offline");
        let (ctx, _mem, api) = machine(&home.0, Some("alpha-uuid"));
        let codex = crate::provider::of(ProviderId::Codex)
            .live(&ctx)
            .expect("Codex keeps its login in a file here");
        let login = json!({
            "auth_mode": "chatgpt",
            "tokens": {
                "id_token": crate::provider::jwt::unsigned(&json!({
                    "email": "work@example.com",
                    "https://api.openai.com/auth": {"chatgpt_account_id": "work-acc"},
                })),
                "access_token": "codex-access",
                "refresh_token": "codex-refresh",
                "account_id": "work-acc",
            },
            "last_refresh": "2026-09-15T05:05:11Z",
        });
        crate::store::write_raw(&codex.chain, &codex.service, &login.to_string())
            .expect("a Codex login");

        let mut s = state(&["alpha", "beta"]);
        s.accounts.push(codex_account("work", "work-acc"));
        s.accounts.push(codex_account("home", "home-acc"));
        let report = gather_offline(&ctx, &s);
        let signed_in: Vec<(ProviderId, &str)> = report
            .rows
            .iter()
            .filter(|r| r.signed_in)
            .map(|r| (r.provider, r.label.as_deref().unwrap_or_default()))
            .collect();
        assert_eq!(
            signed_in,
            [(ProviderId::Claude, "alpha"), (ProviderId::Codex, "work")]
        );
        assert_eq!(
            report.signed_in.expect("Claude Code's config").account_uuid,
            "alpha-uuid"
        );
        assert_eq!(api.calls(), 0, "offline asks nobody");
    }

    /// Put `raw` where this machine's Codex keeps its login, exactly as written.
    fn plant_codex(ctx: &Context, raw: &str) {
        let codex = crate::provider::of(ProviderId::Codex)
            .live(ctx)
            .expect("Codex keeps its login in a file here");
        crate::store::write_raw(&codex.chain, &codex.service, raw).expect("a Codex login");
    }

    /// A Codex login whose ID token names `tokens_of` and whose account id names
    /// `account_id`. The two are the same in every login Codex writes on its own.
    fn codex_login(tokens_of: &str, account_id: &str) -> String {
        json!({
            "auth_mode": "chatgpt",
            "tokens": {
                "id_token": crate::provider::jwt::unsigned(&json!({
                    "email": format!("{tokens_of}@example.com"),
                    "https://api.openai.com/auth": {"chatgpt_account_id": tokens_of},
                })),
                "access_token": format!("access-{tokens_of}"),
                "refresh_token": format!("refresh-{tokens_of}"),
                "account_id": account_id,
            },
            "last_refresh": "2026-09-15T05:05:11Z",
        })
        .to_string()
    }

    /// Two Codex accounts, `a` parked and `b` the one pitboard last switched to, which has
    /// no park because a Codex park is moved rather than copied.
    fn codex_a_parked_b_active() -> State {
        let mut b = codex_account("b", "acc-B");
        b.parked = None;
        let mut s = State {
            accounts: vec![codex_account("a", "acc-A"), b],
            ..State::default()
        };
        s.set_active(ProviderId::Codex, Some("b".into()));
        s
    }

    fn codex_row<'a>(report: &'a Report, label: &str) -> &'a Row {
        report
            .rows
            .iter()
            .find(|r| r.provider == ProviderId::Codex && r.label.as_deref() == Some(label))
            .unwrap_or_else(|| panic!("a row for codex/{label}"))
    }

    /// What a codex still running from before a switch leaves when it refreshes in the
    /// middle of one: its own account's tokens under the other account's id. That is not
    /// nobody signed in, which is what status used to say while `status --offline` named
    /// the account the tokens belong to and `doctor` failed the login. Online, offline and
    /// doctor now agree on whose it is, and online says pitboard cannot use it.
    #[test]
    fn a_codex_login_that_mixes_two_accounts_is_said_rather_than_read_as_nobody() {
        let home = scratch("mixed");
        let (ctx, _mem, api) = machine(&home.0, None);
        plant_codex(&ctx, &codex_login("acc-A", "acc-B"));
        let s = codex_a_parked_b_active();

        let report = gather(&ctx, &s, true);
        let a = codex_row(&report, "a");
        assert!(a.signed_in, "the tokens are a's, as its own record says");
        assert_eq!(a.stale, Some(Stale::LoginUnusable));
        assert_eq!(
            a.explanation(),
            Some("Codex's login is not one pitboard can park or switch; run `pitboard doctor`")
        );
        assert!(!codex_row(&report, "b").signed_in);
        assert!(
            report
                .rows
                .iter()
                .all(|r| r.stale != Some(Stale::NothingSignedIn)),
            "something is signed in"
        );
        assert!(
            report.rows.iter().all(|r| !r.unplaced()),
            "the login is pinned on a, so it needs no row of its own"
        );
        assert_eq!(
            api.calls(),
            0,
            "nobody was asked about a login mixing two accounts"
        );

        let offline = gather_offline(&ctx, &s);
        assert!(codex_row(&offline, "a").signed_in, "offline says the same");
    }

    /// Signed in with an API key: something is signed in, and it is no account. Not nobody,
    /// and not the account pitboard last switched to either, whose login the key replaced.
    #[test]
    fn a_codex_login_with_an_api_key_is_said_and_pinned_on_no_account() {
        let home = scratch("api-key");
        let (ctx, _mem, api) = machine(&home.0, None);
        plant_codex(
            &ctx,
            &json!({"auth_mode": "apikey", "OPENAI_API_KEY": "sk-not-a-real-key"}).to_string(),
        );
        let s = codex_a_parked_b_active();

        let report = gather(&ctx, &s, true);
        assert!(
            report.rows.iter().all(|r| !r.signed_in),
            "no account is signed in"
        );
        let said = report
            .rows
            .iter()
            .find(|r| r.unplaced())
            .expect("a row saying what is signed in to Codex");
        assert_eq!(said.provider, ProviderId::Codex);
        assert_eq!(said.stale, Some(Stale::LoginUnusable));
        assert!(said.email.is_empty() && said.account_uuid.is_empty());
        assert_eq!(api.calls(), 0);
    }

    /// Codex writes its login with a plain truncating write, so a read can catch it half
    /// written. Codex keeps no record apart from the login, so its own record names nobody
    /// then, and pitboard's record of its last switch stands in for it the way `doctor`
    /// already lets it. Without that, the account in use was told to sign in again.
    #[test]
    fn a_codex_login_caught_half_written_still_names_the_account_in_use() {
        let home = scratch("half-written");
        let (ctx, _mem, api) = machine(&home.0, None);
        let whole = codex_login("acc-B", "acc-B");
        plant_codex(&ctx, &whole[..whole.len() / 2]);
        let s = codex_a_parked_b_active();
        crate::state::save(&ctx, &s).expect("an account list");

        let report = gather(&ctx, &s, true);
        let b = codex_row(&report, "b");
        assert!(b.signed_in, "the account pitboard last switched to");
        assert_eq!(b.stale, Some(Stale::LoginUnreadable));
        assert!(!codex_row(&report, "a").signed_in);
        assert!(
            report.rows.iter().all(|r| !r.unplaced()),
            "pinned, so no row of its own"
        );
        assert_eq!(api.calls(), 0);

        let doctor = crate::doctor::gather(&ctx);
        let active: Vec<&str> = doctor
            .parks
            .iter()
            .filter(|p| p.active)
            .map(|p| p.label.as_str())
            .collect();
        assert_eq!(active, ["b"], "doctor reads the same machine the same way");
    }

    /// Only nothing at all is nobody. A Claude Code document with no account in it is what
    /// `/logout` leaves; one that is not a document of Claude Code's shape at all is a login
    /// pitboard cannot use, and is said as one.
    #[test]
    fn only_a_document_holding_no_login_is_nobody_signed_in() {
        let home = scratch("shapes");
        let (ctx, _mem, api) = machine(&home.0, Some("alpha-uuid"));
        let ask = |document: Value| {
            ask_live(
                &ctx,
                ProviderId::Claude,
                &Ok(Some(document)),
                Some("beta-uuid"),
                &HashMap::new(),
                true,
            )
            .0
        };

        let nobody = ask(json!({"mcpOAuth": {}}));
        assert!(nobody.signed_in.is_none() && !nobody.out_of_reach);
        assert_eq!(nobody.usage().err(), Some(Stale::NothingSignedIn));

        let unusable = ask(json!(["a login", "that is not an object"]));
        assert!(matches!(unusable.signed_in, Some(Err(_))));
        assert!(unusable.out_of_reach);
        assert_eq!(unusable.usage().err(), Some(Stale::LoginUnusable));
        assert_eq!(
            unusable.recorded_uuid.as_deref(),
            Some("alpha-uuid"),
            "Claude Code's config, never pitboard's record of its last switch"
        );
        assert_eq!(api.calls(), 0);
    }

    /// A front end is told which rows are a login pitboard could not pin on any account,
    /// so it can say so instead of showing an account nobody has enrolled.
    #[test]
    fn a_row_is_unplaced_only_when_it_is_a_login_on_no_account() {
        let mut row = Row {
            provider: ProviderId::Codex,
            label: None,
            email: String::new(),
            account_uuid: String::new(),
            signed_in: false,
            parked: None,
            usage: None,
            stale: Some(Stale::LoginUnreadable),
            runway: crate::history::Runway::Unknown,
        };
        assert!(row.unplaced());
        row.stale = Some(Stale::LoginUnusable);
        assert!(row.unplaced());
        row.signed_in = true;
        row.stale = Some(Stale::SessionExpired);
        assert!(!row.unplaced(), "an unenrolled login that is signed in");
        row.signed_in = false;
        row.label = Some("work".into());
        row.stale = Some(Stale::LoginUnreadable);
        assert!(!row.unplaced(), "an account");
    }

    /// A home of this test's own, removed when the test is done with it.
    struct Scratch(std::path::PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn scratch(name: &str) -> Scratch {
        let root = std::env::temp_dir().join(format!(
            "pitboard-status-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a scratch home");
        Scratch(root)
    }

    /// A machine of this test's own: stores in memory, services from a script, a clock
    /// that stands still, every tool's home inside `root`, and Claude Code's config naming
    /// `recorded` as signed in where it names anybody.
    fn machine(
        root: &std::path::Path,
        recorded: Option<&str>,
    ) -> (Context, Arc<MemoryHost>, Arc<ScriptedApi>) {
        let mem = MemoryHost::new();
        let api = ScriptedApi::new();
        let ctx = Context::new(root.to_path_buf())
            .with_pitboard_home(root.join(".pitboard"))
            .with_codex_home(root.join("codex").to_string_lossy().into())
            // Named, so nothing here looks up this machine's own `codex` on `PATH`.
            .with_codex_program(root.join("bin").join("codex"))
            .with_memory_stores(Arc::clone(&mem))
            .with_scripted_api(Arc::clone(&api))
            .with_clock(Arc::new(FixedClock::at(NOW)) as Arc<dyn Clock>);
        crate::home::ensure(&ctx).expect("a pitboard home");
        if let Some(uuid) = recorded {
            std::fs::write(
                root.join(".claude.json"),
                json!({"oauthAccount": {
                    "accountUuid": uuid,
                    "emailAddress": format!("{uuid}@example.com"),
                    "organizationUuid": "org",
                }})
                .to_string(),
            )
            .expect("a Claude Code config");
        }
        (ctx, mem, api)
    }
}
