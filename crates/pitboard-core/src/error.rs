//! Every way pitboard can fail. Each variant has a message naming the cause and an action,
//! a stable code for programs to branch on, and an exit code.

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
            ApiError::RateLimited => Cause::RateLimited,
            ApiError::Network(_) => Cause::Unreachable,
            ApiError::Unexpected { .. } => Cause::ServerError,
            ApiError::Malformed(_) => Cause::AnswerNotUnderstood,
            ApiError::InvalidGrant => Cause::LoginRefused,
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
         forbids. Unset it, or sign out of MCP servers you no longer use to make the login \
         smaller."
    )]
    CredentialTooLarge {
        label: String,
        bytes: usize,
        limit: usize,
    },

    #[error(
        "`{program}` is not on this machine, and pitboard signs in with Claude Code's own \
         sign-in. Install Claude Code, or point pitboard at it."
    )]
    ClaudeProgramMissing { program: String },

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
         {expected}). The command line and the app update separately, so upgrade whichever \
         is behind: `brew upgrade pitboard`, or the app's own Check for Updates."
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
        "{path} was written on another computer. Parked logins do not move between \
         machines, because two machines taking turns presenting one refresh token ends the \
         login for both. Run `pitboard adopt` to keep your accounts here and drop the \
         logins they came with; each then needs one `pitboard enroll <label> --sign-in`."
    )]
    StateWrongMachine { path: PathBuf },

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

    #[error("nothing is signed in right now. Run `claude`, sign in, then try again.")]
    LiveCredentialAbsent,

    #[error(
        "the signed-in credential is not shaped like a Claude Code login ({detail}). \
         Run `pitboard doctor` before switching again."
    )]
    LiveCredentialShapeUnexpected { detail: String },

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
         Claude Code has moved on from it. Run `pitboard enroll {label} --sign-in` to sign \
         in to it again."
    )]
    NothingParked { label: String },

    #[error(
        "the parked login for `{label}` has expired. Run `pitboard enroll {label} --sign-in` \
         to sign in to it again."
    )]
    ParkedLoginExpired { label: String },

    #[error(
        "{email} is signed in but not enrolled, so it cannot be parked. \
         Run `pitboard enroll <label>` for it first."
    )]
    LiveAccountNotEnrolled { email: String },

    #[error(
        "{email} is already enrolled as `{label}`. To add a different account, run \
         `pitboard enroll <label> --sign-in`."
    )]
    AlreadyEnrolled { email: String, label: String },

    #[error("`{label}` already refers to {email}. Choose a different label.")]
    LabelTaken { label: String, email: String },

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
        "Claude Code's session has expired, so pitboard cannot confirm which account is \
         signed in. Run `claude` once so it refreshes, then try again."
    )]
    SessionExpired,

    #[error(
        "pitboard could not confirm with Anthropic which account is signed in ({detail}), \
         and will not move a login it cannot identify. Check the connection and try again."
    )]
    IdentityUnverifiable { cause: Cause, detail: String },

    #[error("the signed-in account changed while switching. Nothing was moved; try again.")]
    SignedInAccountChanged,

    #[error(
        "an earlier switch from `{from}` to `{to}` was interrupted, and pitboard cannot yet \
         tell whether it finished ({detail}). Nothing was changed. Run `claude` once so its \
         session is current, then try again."
    )]
    RecoveryUndetermined {
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
        "could not sign in as `{to}` ({detail}), and could not read the credential store \
         back to find out whether anything changed. Nothing has been deleted and both \
         logins are still here. Unlock the keychain and run `pitboard` again; it finishes \
         or undoes this before doing anything else."
    )]
    SwitchUnverified {
        from: String,
        to: String,
        detail: String,
    },

    #[error(
        "could not sign in as `{to}`, and could not put `{from}` back either ({detail}). \
         `{from}`'s login is still parked: run `claude` and sign in to any enrolled account, \
         then `pitboard use {from}`."
    )]
    SwitchCorrupted {
        from: String,
        to: String,
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

    #[error("could not read or write pitboard's recovery record at {path}: {source}")]
    RecoveryFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("`claude` was not found on PATH. Install Claude Code, run it once, then try again.")]
    ClaudeNotFound,

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
            ClaudeProgramMissing { .. } => "claude_program_missing",
            CredentialTooLarge { .. } => "credential_too_large",
            CustomOauthEndpoint => "custom_oauth_endpoint",
            StateUnreadable { .. } => "state_unreadable",
            StateCorrupt { .. } => "state_corrupt",
            StateFromNewerVersion { .. } => "state_from_newer_version",
            StateVersionUnknown { .. } => "state_version_unknown",
            StateWrongMachine { .. } => "state_wrong_machine",
            StateWriteFailed { .. } => "state_write_failed",
            HomeUnwritable { .. } => "home_unwritable",
            ClaudeConfigMissing { .. } => "claude_config_missing",
            ClaudeConfigUnreadable { .. } => "claude_config_unreadable",
            ClaudeConfigNotJson { .. } => "claude_config_not_json",
            LiveCredentialAbsent => "live_credential_absent",
            LiveCredentialShapeUnexpected { .. } => "live_credential_shape_unexpected",
            AccountUnknown { .. } => "account_unknown",
            NothingParked { .. } => "nothing_parked",
            ParkedLoginExpired { .. } => "parked_login_expired",
            LiveAccountNotEnrolled { .. } => "live_account_not_enrolled",
            AlreadyEnrolled { .. } => "already_enrolled",
            LabelTaken { .. } => "label_taken",
            CannotForgetActiveAccount { .. } => "cannot_forget_active_account",
            ParkSlotExhausted => "park_slot_exhausted",
            ParkedCredentialMissing { .. } => "parked_credential_missing",
            ParkedCredentialCorrupt { .. } => "parked_credential_corrupt",
            ConfigBackupFailed { .. } => "config_backup_failed",
            ConfigWriteFailed { .. } => "config_write_failed",
            SwitchRolledBack { .. } => "switch_rolled_back",
            SwitchUnverified { .. } => "switch_unverified",
            SwitchCorrupted { .. } => "switch_corrupted",
            RecoveryFailed { .. } => "recovery_failed",
            RecoveryRecordCorrupt { .. } => "recovery_record_corrupt",
            SessionExpired => "session_expired",
            IdentityUnverifiable { .. } => "identity_unverifiable",
            SignedInAccountChanged => "signed_in_account_changed",
            RecoveryUndetermined { .. } => "recovery_undetermined",
            ClaudeNotFound => "claude_not_found",
            SignInIncomplete => "sign_in_incomplete",
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
            SessionExpired => Some(Cause::TokenExpired),
            _ => None,
        }
    }

    pub fn exit_code(&self) -> u8 {
        use Error::*;
        match self {
            // A login or Claude Code's files in a state pitboard will not act on, which is
            // what exit 3 means: not a failure of the attempt, a refusal to attempt.
            LiveCredentialShapeUnexpected { .. }
            | ClaudeConfigNotJson { .. }
            | SwitchCorrupted { .. }
            | SwitchUnverified { .. }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_so_a_caller_can_branch_on_them() {
        let samples = [
            Error::LiveCredentialAbsent,
            Error::ParkSlotExhausted,
            Error::AccountUnknown {
                label: "x".into(),
                enrolled: Enrolled::default(),
            },
            Error::NothingParked { label: "x".into() },
            Error::ParkedLoginExpired { label: "x".into() },
            Error::LabelTaken {
                label: "x".into(),
                email: "e".into(),
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
            Error::LiveCredentialShapeUnexpected { detail: "x".into() }.exit_code(),
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
            Error::LiveCredentialAbsent.to_string(),
            Error::AccountUnknown {
                label: "work".into(),
                enrolled: Enrolled(vec!["personal".into()]),
            }
            .to_string(),
            Error::NothingParked {
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
                email: "a@b.c".into(),
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
}
