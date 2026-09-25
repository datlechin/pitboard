//! Every way pitboard can fail. Each variant has a message naming the cause and an action,
//! a stable code for programs to branch on, and an exit code.

use crate::provider::ProviderId;
use crate::state::Key;
use std::path::PathBuf;

/// The labels an account list holds, rendered for a message: " Enrolled: `a`, `b`." or
/// nothing at all when none are.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Enrolled(pub Vec<String>);

impl std::fmt::Display for Enrolled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.is_empty() {
            return write!(f, " Nothing is enrolled yet.");
        }
        let labels: Vec<String> = self.0.iter().map(|l| format!("`{l}`")).collect();
        write!(f, " Enrolled: {}.", labels.join(", "))
    }
}

/// Why a request to Anthropic did not produce an answer pitboard could use.
///
/// The code on an error says what pitboard was doing; this says what went wrong underneath
/// it, and whether trying again is worth anything. Without it every failure that was not a
/// 401 arrived at a front end as one code with a sentence of prose, so neither the command
/// line nor the app could tell being offline from being rate limited from a login Anthropic
/// has finished with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Cause {
    /// Anthropic could not be reached at all.
    Unreachable,
    /// Anthropic asked for less traffic.
    RateLimited,
    /// Anthropic answered, badly, and may answer well later.
    ServerError,
    /// Anthropic answered something pitboard does not understand, which means its shape
    /// moved. Trying again will produce the same thing.
    AnswerNotUnderstood,
    /// This login is finished: revoked, or already used somewhere else.
    LoginRefused,
    /// The token has expired. For the signed-in login that is ordinary and Claude Code
    /// renews it; for a parked one it means the park needs renewing first.
    TokenExpired,
}

impl Cause {
    pub fn of(error: &crate::api::ApiError) -> Cause {
        use crate::api::ApiError;
        match error {
            ApiError::Unauthorized => Cause::TokenExpired,
            ApiError::RateLimited { .. } => Cause::RateLimited,
            ApiError::Network(_) => Cause::Unreachable,
            ApiError::Unexpected { .. } => Cause::ServerError,
            ApiError::Malformed(_) => Cause::AnswerNotUnderstood,
            ApiError::InvalidGrant => Cause::LoginRefused,
        }
    }

    /// The same question asked of a provider rather than of Anthropic directly.
    pub fn of_provider(error: &crate::provider::ProviderError) -> Cause {
        use crate::provider::ProviderError as P;
        match error {
            P::Unauthorized => Cause::TokenExpired,
            P::RateLimited { .. } => Cause::RateLimited,
            P::Network { .. } => Cause::Unreachable,
            P::Unexpected { .. } => Cause::ServerError,
            P::Malformed { .. }
            | P::ShapeUnexpected { .. }
            | P::Unsupported { .. }
            | P::NoLogin { .. } => Cause::AnswerNotUnderstood,
            P::InvalidGrant { .. } => Cause::LoginRefused,
        }
    }

    /// Stable, for a program to branch on; the same as its JSON form.
    pub fn code(self) -> &'static str {
        match self {
            Cause::Unreachable => "unreachable",
            Cause::RateLimited => "rate_limited",
            Cause::ServerError => "server_error",
            Cause::AnswerNotUnderstood => "answer_not_understood",
            Cause::LoginRefused => "login_refused",
            Cause::TokenExpired => "token_expired",
        }
    }

    /// Whether the same request, later, could answer differently. A front end deciding
    /// whether to back off or to give up reads this and nothing else.
    pub fn worth_retrying(self) -> bool {
        match self {
            Cause::Unreachable | Cause::RateLimited | Cause::ServerError => true,
            Cause::AnswerNotUnderstood | Cause::LoginRefused | Cause::TokenExpired => false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error(
        "{path} looks like it is inside {marker}, which syncs to other machines. \
         Parked logins belong to one machine; set PITBOARD_HOME to a local folder."
    )]
    StateOnSyncedDrive { path: PathBuf, marker: String },

    #[error(
        "the login for `{label}` needs {bytes} bytes and `security` reads {limit} from \
         stdin, so it can only be written on the argument line, which PITBOARD_NO_ARGV \
         forbids. {}",
        smaller(*tool)
    )]
    CredentialTooLarge {
        tool: ProviderId,
        label: String,
        bytes: usize,
        limit: usize,
    },

    #[error(
        "`{program}` is not on this machine, and pitboard signs in with {}'s own sign-in. \
         Install {}, or point pitboard at it.",
        tool.name(),
        tool.name()
    )]
    ProgramMissing { tool: ProviderId, program: String },

    #[error(
        "CLAUDE_CODE_CUSTOM_OAUTH_URL is set, so Claude Code keeps its login under a \
         different name than the one pitboard reads. Unset it to use pitboard."
    )]
    CustomOauthEndpoint,

    #[error("could not read pitboard's account list at {path}: {source}")]
    StateUnreadable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "pitboard's account list at {path} is corrupt ({source}). \
         Delete it and enroll your accounts again; parked logins will be lost."
    )]
    StateCorrupt {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error(
        "{path} was written by a newer pitboard (its format is {found}, this one reads \
         {expected}). Update this pitboard the way you installed it. The app, and the command \
         line inside it, update with the app's Check for Updates."
    )]
    StateFromNewerVersion {
        path: PathBuf,
        found: u32,
        expected: u32,
    },

    #[error(
        "{path} is in a format ({found}) no version of pitboard has ever written. Delete it \
         and enroll your accounts again."
    )]
    StateVersionUnknown { path: PathBuf, found: u32 },

    #[error(
        "{path} has an account for `{tool}`, a tool this pitboard does not know, so it was \
         written by a newer one. Update this pitboard the way you installed it. The app, and \
         the command line inside it, update with the app's Check for Updates."
    )]
    StateNamesUnknownTool { path: PathBuf, tool: String },

    #[error(
        "{path} was written on another computer. Parked logins do not move between \
         machines, because two machines taking turns presenting one refresh token ends the \
         login for both. Run `pitboard adopt` to keep your accounts here and drop the \
         logins they came with; each then needs one `pitboard enroll <label> --sign-in`."
    )]
    StateWrongMachine { path: PathBuf },

    #[error(
        "this platform has no scheduler pitboard knows how to write. Keeping parked logins \
         alive here means running `pitboard` yourself from time to time."
    )]
    ScheduleUnsupported,

    #[error("the scheduler refused: {detail}")]
    ScheduleRefused { detail: String },

    #[error("the renewal schedule would run {path}, which is not there. Nothing was scheduled.")]
    ScheduleProgramMissing { path: PathBuf },

    /// macOS runs an app opened where it was downloaded from a copy it makes somewhere
    /// temporary, which is there while the app runs and gone once it quits.
    #[error(
        "the renewal schedule would run {path}, which is in a temporary copy macOS made of \
         the app and is gone once the app quits. Move pitboard to your Applications folder, \
         open it from there, and turn on daily renewal again."
    )]
    ScheduleProgramTemporary { path: PathBuf },

    /// An app that names no command line for the schedule, where the only other thing to
    /// schedule is the app itself, which renews nothing.
    #[error("this copy of pitboard has no command line inside it for the renewal schedule to run.")]
    ScheduleProgramUnnamed,

    #[error("could not write to pitboard's directory at {path}: {source}")]
    HomeUnwritable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not save pitboard's account list at {path}: {source}")]
    StateWriteFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "Claude Code has not run on this machine yet ({path} does not exist). \
         Run `claude` once, sign in, then try again."
    )]
    ClaudeConfigMissing { path: PathBuf },

    #[error("could not read Claude Code's config at {path}: {source}")]
    ClaudeConfigUnreadable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "Claude Code's config at {path} is not valid JSON right now ({source}). \
         It may be mid-write; wait a few seconds and try again."
    )]
    ClaudeConfigNotJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error(
        "nothing is signed in to {} right now. Run `{}`, sign in, then try again.",
        tool.name(),
        tool.login_command()
    )]
    LiveCredentialAbsent { tool: ProviderId },

    #[error(
        "Claude Code's config says {email} is signed in, but pitboard cannot find that \
         login in the keychain or in the file it also reads. It will not write a login \
         where nobody reads it. This usually means Claude Code has started keeping logins \
         somewhere pitboard does not know about yet: check for a pitboard update, and \
         report it with `pitboard doctor --json` if there is none."
    )]
    LiveCredentialElsewhere { email: String },

    #[error(
        "the signed-in credential is not shaped like a {} login ({detail}). \
         Run `pitboard doctor` before switching again.",
        tool.name()
    )]
    LiveCredentialShapeUnexpected { tool: ProviderId, detail: String },

    /// The tool is configured to keep its login somewhere pitboard does not handle.
    #[error("{reason}.")]
    LiveStoreUnsupported { tool: ProviderId, reason: String },

    #[error(
        "no account is enrolled as `{label}`.{enrolled} Run `pitboard enroll {label} \
         --sign-in` to add it."
    )]
    AccountUnknown {
        label: String,
        /// The labels that do exist, so a typo costs no second command.
        enrolled: Enrolled,
    },

    #[error(
        "`{label}` has no parked login to switch to: the last one went back into use and \
         {} has moved on from it. Run `pitboard enroll {label} --sign-in` to sign in to it \
         again.",
        tool.name()
    )]
    NothingParked { tool: ProviderId, label: String },

    #[error(
        "the parked login for `{label}` has expired. Run `pitboard enroll {label} --sign-in` \
         to sign in to it again."
    )]
    ParkedLoginExpired { label: String },

    #[error(
        "{email} is signed in but not enrolled, so it cannot be parked. \
         Run `pitboard enroll {}` for it first.",
        Key::new(*tool, "<label>").typed()
    )]
    LiveAccountNotEnrolled { tool: ProviderId, email: String },

    #[error(
        "{email} is already enrolled as `{label}`. To add a different account, run \
         `pitboard enroll {} --sign-in`.",
        Key::new(*tool, "<label>").typed()
    )]
    AlreadyEnrolled {
        tool: ProviderId,
        email: String,
        label: String,
    },

    #[error("`{label}` already refers to {email}. Choose a different label.")]
    LabelTaken { label: String, email: String },

    #[error(
        "`{typed}` is not a tool pitboard knows. It knows: {}.",
        known.join(", ")
    )]
    ProviderUnknown { typed: String, known: Vec<String> },

    #[error(
        "`{label}` is enrolled for more than one tool: {}. Say which one.",
        matches.iter().map(|m| format!("`{m}`")).collect::<Vec<_>>().join(", ")
    )]
    LabelAmbiguous { label: String, matches: Vec<String> },

    #[error("`{label}` is signed in; switch to another account before forgetting it.")]
    CannotForgetActiveAccount { label: String },

    #[error("could not find a free place to park this login. Run `pitboard doctor`.")]
    ParkSlotExhausted,

    #[error(
        "the parked login for `{label}` is missing. Run `pitboard enroll {label} --sign-in` \
         to sign in to it again."
    )]
    ParkedCredentialMissing { label: String },

    #[error(
        "the parked login for `{label}` is not the one pitboard recorded ({detail}). Run \
         `pitboard enroll {label} --sign-in` to replace it."
    )]
    ParkedCredentialCorrupt { label: String, detail: String },

    #[error("could not back up Claude Code's config ({path}): {source}. Nothing was changed.")]
    ConfigBackupFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Claude Code refetches its profile only once a day, so this does not correct itself.
    #[error(
        "the login moved, but Claude Code's config at {path} could not be updated ({detail}). \
         Claude Code may show the previous account's name until the next switch."
    )]
    ConfigWriteFailed { path: PathBuf, detail: String },

    #[error(
        "{}'s session has expired, so pitboard cannot confirm which account is signed in. \
         Run `{}` once so it refreshes, then try again.",
        tool.name(),
        tool.program()
    )]
    SessionExpired { tool: ProviderId },

    #[error(
        "pitboard could not confirm with {} which account is signed in ({detail}), and will \
         not move a login it cannot identify. Check the connection and try again.",
        tool.service()
    )]
    IdentityUnverifiable {
        tool: ProviderId,
        cause: Cause,
        detail: String,
    },

    #[error("the signed-in account changed while switching. Nothing was moved; try again.")]
    SignedInAccountChanged,

    #[error(
        "an earlier switch from `{from}` to `{to}` was interrupted, and pitboard cannot yet \
         tell whether it finished ({detail}). Nothing was changed. Run `{}` once so its \
         session is current, then try again.",
        tool.program()
    )]
    RecoveryUndetermined {
        tool: ProviderId,
        from: String,
        to: String,
        detail: String,
    },

    #[error(
        "could not sign in as `{to}` ({detail}); `{from}` is still signed in, nothing was lost."
    )]
    SwitchRolledBack {
        from: String,
        to: String,
        detail: String,
    },

    #[error(
        "{} no longer accepts `{label}`'s parked login, so pitboard did not move anything. \
         The copy has been dropped; sign in to that account again with \
         `pitboard enroll {label} --sign-in`.",
        tool.service()
    )]
    ParkedLoginRefused { tool: ProviderId, label: String },

    #[error(
        "`{label}`'s parked login belongs to {email}, not to the account pitboard has \
         under that label. Nothing was moved. Run `pitboard doctor`, then \
         `pitboard enroll {label} --sign-in` to replace it."
    )]
    ParkedLoginBelongsElsewhere { label: String, email: String },

    #[error(
        "signed in as `{to}`, and the login was gone again before pitboard finished. {}",
        after_it_did_not_hold(*tool, from, to)
    )]
    SwitchDidNotHold {
        tool: ProviderId,
        from: String,
        to: String,
    },

    #[error(
        "could not sign in as `{to}` ({detail}), and could not read the credential store \
         back to find out whether anything changed. Nothing has been deleted and both \
         logins are still here. {} and run `pitboard` again; it finishes or undoes this \
         before doing anything else.",
        make_readable(*tool)
    )]
    SwitchUnverified {
        tool: ProviderId,
        from: String,
        to: String,
        detail: String,
    },

    #[error(
        "could not sign in as `{to}`, and could not put `{from}` back either ({detail}). \
         `{from}`'s login is still parked: run `{}` and sign in to any enrolled account, \
         then `pitboard use {from}`.",
        tool.login_command()
    )]
    SwitchCorrupted {
        tool: ProviderId,
        from: String,
        to: String,
        detail: String,
    },

    #[error(
        "signed in to `{label}` again, and pitboard could not confirm that its new login \
         took the place of the one in use ({detail}), so {} may have no login for it now. {}",
        tool.name(),
        not_in_use(*tool, label, *parked)
    )]
    SignInNotInstalled {
        tool: ProviderId,
        label: String,
        detail: String,
        /// Whether the new login was parked instead, which keeps the one copy of it.
        parked: bool,
        /// What writing it and parking it warned about, which is still true of a change
        /// that failed afterwards. Taken out by the service and reported beside the error.
        warnings: Vec<crate::service::Warning>,
    },

    #[error(
        "the new login for `{label}` could not be put in use ({detail}), so it was not \
         kept, and {} keeps the login it has. Run `pitboard enroll {label} --sign-in` to \
         sign in again.",
        tool.name()
    )]
    SignInNotKept {
        tool: ProviderId,
        label: String,
        detail: String,
    },

    #[error(
        "the record of an interrupted switch at {path} is damaged ({source}), so pitboard \
         cannot tell what that switch did. Nothing was changed. Check that `pitboard status` \
         shows the account you expect, then delete the file to continue."
    )]
    RecoveryRecordCorrupt {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error(
        "an earlier switch of {} from `{from}` to `{to}` was interrupted while its login was \
         at {slot}, and this run reads it from somewhere else, so it cannot tell what that \
         switch did. Nothing was changed. Run pitboard with {} pointing where it did to \
         finish it, or `pitboard abandon` to keep every login it names and move on.",
        tool.name(),
        tool.home_variable()
    )]
    RecoveryElsewhere {
        tool: ProviderId,
        from: String,
        to: String,
        slot: String,
    },

    #[error("could not read or write pitboard's recovery record at {path}: {source}")]
    RecoveryFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "`{}` was not found on PATH. Install {}, run it once, then try again.",
        tool.program(),
        tool.name()
    )]
    ProgramNotFound { tool: ProviderId },

    #[error(
        "could not renew the parked login for `{label}` ({detail}); its last reading is shown \
         instead. Run `pitboard doctor` if this keeps happening."
    )]
    RenewalFailed {
        label: String,
        cause: Option<Cause>,
        detail: String,
    },

    #[error("the sign-in did not finish, so nothing was enrolled.")]
    SignInIncomplete,

    /// Signing in to a second account works by pointing the tool's own login at a scratch
    /// directory. Where that does not isolate it from the live login, running one would
    /// write over the account somebody is using, so pitboard will not.
    #[error(
        "pitboard will not sign in to a second account on this machine: {reason} Signing \
         in would write over the login you are using."
    )]
    SignInNotIsolated { reason: String },

    #[error(
        "another `pitboard enroll --sign-in` is already waiting for its sign-in. Finish or \
         cancel that one first."
    )]
    SignInInProgress,

    /// The command line itself was wrong; the message is clap's.
    #[error("{0}")]
    Usage(String),

    #[error(transparent)]
    Store(#[from] crate::store::Error),

    #[error(transparent)]
    Lock(#[from] crate::lock::LockError),
}

impl Error {
    /// A stable identifier a program can branch on. Adding one is safe; renaming one is not.
    pub fn code(&self) -> &'static str {
        use Error::*;
        match self {
            StateOnSyncedDrive { .. } => "state_on_synced_drive",
            ProgramMissing { tool, .. } => match tool {
                ProviderId::Claude => "claude_program_missing",
                ProviderId::Codex => "codex_program_missing",
            },
            CredentialTooLarge { .. } => "credential_too_large",
            CustomOauthEndpoint => "custom_oauth_endpoint",
            StateUnreadable { .. } => "state_unreadable",
            StateCorrupt { .. } => "state_corrupt",
            StateFromNewerVersion { .. } => "state_from_newer_version",
            StateVersionUnknown { .. } => "state_version_unknown",
            StateNamesUnknownTool { .. } => "state_names_unknown_tool",
            StateWrongMachine { .. } => "state_wrong_machine",
            StateWriteFailed { .. } => "state_write_failed",
            ScheduleUnsupported => "schedule_unsupported",
            ScheduleRefused { .. } => "schedule_refused",
            ScheduleProgramMissing { .. } => "schedule_program_missing",
            ScheduleProgramTemporary { .. } => "schedule_program_temporary",
            ScheduleProgramUnnamed => "schedule_program_unnamed",
            HomeUnwritable { .. } => "home_unwritable",
            ClaudeConfigMissing { .. } => "claude_config_missing",
            ClaudeConfigUnreadable { .. } => "claude_config_unreadable",
            ClaudeConfigNotJson { .. } => "claude_config_not_json",
            LiveCredentialAbsent { .. } => "live_credential_absent",
            LiveStoreUnsupported { .. } => "live_store_unsupported",
            LiveCredentialElsewhere { .. } => "live_credential_elsewhere",
            LiveCredentialShapeUnexpected { .. } => "live_credential_shape_unexpected",
            AccountUnknown { .. } => "account_unknown",
            NothingParked { .. } => "nothing_parked",
            ParkedLoginExpired { .. } => "parked_login_expired",
            ParkedLoginRefused { .. } => "parked_login_refused",
            ParkedLoginBelongsElsewhere { .. } => "parked_login_belongs_elsewhere",
            LiveAccountNotEnrolled { .. } => "live_account_not_enrolled",
            AlreadyEnrolled { .. } => "already_enrolled",
            LabelTaken { .. } => "label_taken",
            ProviderUnknown { .. } => "provider_unknown",
            LabelAmbiguous { .. } => "label_ambiguous",
            CannotForgetActiveAccount { .. } => "cannot_forget_active_account",
            ParkSlotExhausted => "park_slot_exhausted",
            ParkedCredentialMissing { .. } => "parked_credential_missing",
            ParkedCredentialCorrupt { .. } => "parked_credential_corrupt",
            ConfigBackupFailed { .. } => "config_backup_failed",
            ConfigWriteFailed { .. } => "config_write_failed",
            SwitchRolledBack { .. } => "switch_rolled_back",
            SwitchDidNotHold { .. } => "switch_did_not_hold",
            SwitchUnverified { .. } => "switch_unverified",
            SwitchCorrupted { .. } => "switch_corrupted",
            SignInNotInstalled { .. } => "sign_in_not_installed",
            SignInNotKept { .. } => "sign_in_not_kept",
            RecoveryFailed { .. } => "recovery_failed",
            RecoveryRecordCorrupt { .. } => "recovery_record_corrupt",
            SessionExpired { .. } => "session_expired",
            IdentityUnverifiable { .. } => "identity_unverifiable",
            SignedInAccountChanged => "signed_in_account_changed",
            RecoveryUndetermined { .. } => "recovery_undetermined",
            RecoveryElsewhere { .. } => "recovery_elsewhere",
            ProgramNotFound { tool } => match tool {
                ProviderId::Claude => "claude_not_found",
                ProviderId::Codex => "codex_not_found",
            },
            SignInIncomplete => "sign_in_incomplete",
            SignInNotIsolated { .. } => "sign_in_not_isolated",
            RenewalFailed { .. } => "renewal_failed",
            SignInInProgress => "sign_in_in_progress",
            Usage(_) => "usage",
            Store(e) => e.code(),
            Lock(e) => e.code(),
        }
    }

    /// 1 when a request could not be met; 2 when the command line was wrong; 3 when a login
    /// or Claude Code's files are in a state pitboard cannot safely act on: an unexpected
    /// format, or a login that could not be put back.
    /// What went wrong underneath, where the failure came from a request to Anthropic.
    /// `None` where nothing was asked.
    pub fn cause(&self) -> Option<Cause> {
        use Error::*;
        match self {
            IdentityUnverifiable { cause, .. } => Some(*cause),
            RenewalFailed { cause, .. } => *cause,
            SessionExpired { .. } => Some(Cause::TokenExpired),
            _ => None,
        }
    }

    /// Warnings a failed change carries in itself, for the caller to report beside it. Only
    /// a failure that happened after something worth warning about was done carries any.
    pub(crate) fn take_warnings(&mut self) -> Vec<crate::service::Warning> {
        match self {
            Error::SignInNotInstalled { warnings, .. } => std::mem::take(warnings),
            _ => Vec::new(),
        }
    }

    pub fn exit_code(&self) -> u8 {
        use Error::*;
        match self {
            // A login or Claude Code's files in a state pitboard will not act on, which is
            // what exit 3 means: not a failure of the attempt, a refusal to attempt.
            LiveCredentialShapeUnexpected { .. }
            | LiveStoreUnsupported { .. }
            | ClaudeConfigNotJson { .. }
            | SwitchCorrupted { .. }
            | SwitchUnverified { .. }
            | SwitchDidNotHold { .. }
            | SignInNotInstalled { .. }
            | LiveCredentialElsewhere { .. }
            | CredentialTooLarge { .. }
            | CustomOauthEndpoint
            | RecoveryRecordCorrupt { .. } => 3,
            Usage(_) => 2,
            Store(e) => e.exit_code(),
            _ => 1,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// What makes a tool's live login readable again, where it could not be read.
fn make_readable(tool: ProviderId) -> &'static str {
    match tool {
        ProviderId::Claude => "Unlock the keychain",
        ProviderId::Codex => "Make Codex's auth.json readable to you again",
    }
}

/// What makes a login smaller, where anything does.
fn smaller(tool: ProviderId) -> &'static str {
    match tool {
        ProviderId::Claude => {
            "Unset it, or sign out of MCP servers you no longer use to make the login smaller."
        }
        ProviderId::Codex => "Unset it to let pitboard write it.",
    }
}

/// Which write took a switched-in login away again, as far as each tool is known to make
/// one, and what that leaves.
fn after_it_did_not_hold(tool: ProviderId, from: &str, to: &str) -> String {
    match tool {
        ProviderId::Claude => format!(
            "Claude Code removes a login without taking the write lock when `/logout` has \
             given up waiting, which is the one write pitboard cannot exclude. Nothing was \
             lost: both `{from}` and `{to}` are parked. Run `claude` and sign in to any \
             enrolled account, then `pitboard use {to}`."
        ),
        // Not "nothing was lost": the likeliest writer is a codex session refreshing the
        // outgoing account, which spends the token in that account's park, and `codex
        // login` would revoke whatever login is left in auth.json.
        ProviderId::Codex => format!(
            "Codex takes no lock, so a codex session still running from before the switch, \
             refreshing or signing out, rewrote auth.json underneath it. `{to}` is still \
             parked. `{from}`'s park may hold a token that refresh spent. Quit every running \
             codex, then run `pitboard` to see what is signed in; do not run `codex login` \
             or `codex logout` until you have, because either revokes the login they find."
        ),
    }
}

/// Where a new login that could not be put in use went, and the way back from there.
fn not_in_use(tool: ProviderId, label: &str, parked: bool) -> String {
    if parked {
        format!(
            "The new login is parked, so it is not lost. Run `pitboard` to see what is \
             signed in; if nothing is, run `{}` and sign in to any enrolled account, then \
             `pitboard use {label}`.",
            tool.login_command()
        )
    } else {
        format!(
            "It could not be parked either: run `{}` and sign in to `{label}` again.",
            tool.login_command()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_so_a_caller_can_branch_on_them() {
        let samples = [
            Error::LiveCredentialAbsent {
                tool: ProviderId::Claude,
            },
            Error::ParkSlotExhausted,
            Error::AccountUnknown {
                label: "x".into(),
                enrolled: Enrolled::default(),
            },
            Error::NothingParked {
                tool: ProviderId::Claude,
                label: "x".into(),
            },
            Error::ParkedLoginExpired { label: "x".into() },
            Error::LabelTaken {
                label: "x".into(),
                email: "e".into(),
            },
            Error::SwitchRolledBack {
                from: "x".into(),
                to: "y".into(),
                detail: "d".into(),
            },
            Error::SignInNotKept {
                tool: ProviderId::Codex,
                label: "x".into(),
                detail: "d".into(),
            },
        ];
        let mut codes: Vec<&str> = samples.iter().map(Error::code).collect();
        codes.sort_unstable();
        let before = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), before);
    }

    #[test]
    fn a_broken_assumption_exits_differently_from_a_bad_request() {
        assert_eq!(
            Error::LiveCredentialShapeUnexpected {
                tool: ProviderId::Claude,
                detail: "x".into()
            }
            .exit_code(),
            3
        );
        assert_eq!(
            Error::AccountUnknown {
                label: "x".into(),
                enrolled: Enrolled::default()
            }
            .exit_code(),
            1
        );
    }

    #[test]
    fn every_message_tells_the_user_something_to_do() {
        // A message that only states a fact leaves the user stuck.
        let actionable = [
            Error::LiveCredentialAbsent {
                tool: ProviderId::Claude,
            }
            .to_string(),
            Error::LiveCredentialAbsent {
                tool: ProviderId::Codex,
            }
            .to_string(),
            Error::AccountUnknown {
                label: "work".into(),
                enrolled: Enrolled(vec!["personal".into()]),
            }
            .to_string(),
            Error::NothingParked {
                tool: ProviderId::Claude,
                label: "work".into(),
            }
            .to_string(),
            Error::ParkedLoginExpired {
                label: "work".into(),
            }
            .to_string(),
            Error::ParkedCredentialMissing {
                label: "work".into(),
            }
            .to_string(),
            Error::LiveAccountNotEnrolled {
                tool: ProviderId::Claude,
                email: "a@b.c".into(),
            }
            .to_string(),
            Error::SignInNotKept {
                tool: ProviderId::Codex,
                label: "codex/work".into(),
                detail: "the keychain is locked".into(),
            }
            .to_string(),
        ];
        for message in actionable {
            assert!(
                message.contains("Run ") || message.contains("Sign in"),
                "no action offered: {message}"
            );
        }
    }

    /// Codex's messages name Codex and the command that signs in to it; Claude Code's name
    /// Claude Code. A message about the wrong tool sends somebody to run the wrong program.
    #[test]
    fn a_message_names_the_tool_it_is_about() {
        let codex = Error::LiveCredentialAbsent {
            tool: ProviderId::Codex,
        }
        .to_string();
        assert!(
            codex.contains("Codex") && codex.contains("`codex login`"),
            "{codex}"
        );
        assert!(!codex.contains("Claude"), "{codex}");

        let unverifiable = Error::IdentityUnverifiable {
            tool: ProviderId::Codex,
            cause: Cause::Unreachable,
            detail: "offline".into(),
        }
        .to_string();
        assert!(unverifiable.contains("OpenAI"), "{unverifiable}");

        let claude = Error::IdentityUnverifiable {
            tool: ProviderId::Claude,
            cause: Cause::Unreachable,
            detail: "offline".into(),
        }
        .to_string();
        assert!(claude.contains("confirm with Anthropic"), "{claude}");
    }

    /// The codes of the two tools' missing programs stay distinct, and Claude Code's keep
    /// the names they were released under.
    #[test]
    fn a_missing_program_keeps_its_released_code() {
        let missing = |tool| Error::ProgramNotFound { tool }.code();
        assert_eq!(missing(ProviderId::Claude), "claude_not_found");
        assert_eq!(missing(ProviderId::Codex), "codex_not_found");
        let absent = |tool| {
            Error::ProgramMissing {
                tool,
                program: "x".into(),
            }
            .code()
        };
        assert_eq!(absent(ProviderId::Claude), "claude_program_missing");
        assert_eq!(absent(ProviderId::Codex), "codex_program_missing");
    }

    /// A pitboard that finds its files written by a newer one is updated by the route it
    /// came by. The app's Check for Updates moves only the app and the command line inside
    /// it, so it is not offered as another way to update this one.
    #[test]
    fn a_newer_pitboards_files_say_which_update_moves_which_pitboard() {
        let path = PathBuf::from("/home/x/.pitboard/state.json");
        for message in [
            Error::StateFromNewerVersion {
                path: path.clone(),
                found: 9,
                expected: 5,
            }
            .to_string(),
            Error::StateNamesUnknownTool {
                path: path.clone(),
                tool: "gemini".into(),
            }
            .to_string(),
        ] {
            assert!(
                message.contains("Update this pitboard the way you installed it."),
                "{message}"
            );
            assert!(
                message.contains("The app, and the command line inside it, update with"),
                "{message}"
            );
            assert!(!message.contains("or with the app"), "{message}");
        }
    }

    /// Advice to enrol names the tool the account is for: a bare name would enrol a Claude
    /// Code account for a Codex login.
    #[test]
    fn enrolment_advice_keeps_the_tool() {
        let codex = Error::LiveAccountNotEnrolled {
            tool: ProviderId::Codex,
            email: "a@b.c".into(),
        }
        .to_string();
        assert!(codex.contains("pitboard enroll codex/<label>"), "{codex}");
        let claude = Error::LiveAccountNotEnrolled {
            tool: ProviderId::Claude,
            email: "a@b.c".into(),
        }
        .to_string();
        assert!(claude.contains("pitboard enroll <label>"), "{claude}");
        let already = Error::AlreadyEnrolled {
            tool: ProviderId::Codex,
            email: "a@b.c".into(),
            label: "codex/work".into(),
        }
        .to_string();
        assert!(
            already.contains("pitboard enroll codex/<label> --sign-in"),
            "{already}"
        );
    }
}
