//! Every way Pitboard can fail. Each variant has a message naming the cause and an action,
//! a stable code for programs to branch on, and an exit code.

use crate::host::Os;
use crate::provider::ProviderId;
use crate::state::Key;
use crate::words::Spelling;
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

/// How far putting away the file behind a tool's store had gone when something stopped it.
/// Never as far as deleting the file, and never further than keeping the login it held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Partway {
    /// Nothing was changed.
    Nothing,
    /// The login Claude Code has stored was renewed to tell whose it is, which spent its
    /// refresh token, and the renewed one waits in Pitboard's vault for the next change to
    /// save it there.
    StoredRenewedKept,
    /// The login the file held was renewed, which spent its refresh token, and the renewed
    /// one could not be written back there, nor kept.
    RenewedUnwritten,
    /// The login the file held was renewed, which spent its refresh token, and the renewed
    /// one waits in Pitboard's vault for the next change to write it back there.
    RenewedKept,
    /// The login in the file was renewed, and the renewed one written back there.
    Renewed,
    /// The login in the file was parked for `label`.
    Parked { label: String },
}

impl std::fmt::Display for Partway {
    /// What Pitboard did, after "Pitboard".
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Partway::Nothing => write!(f, "changed nothing"),
            Partway::StoredRenewedKept => write!(
                f,
                "deleted nothing. It renewed the login Claude Code has stored and keeps the \
                 renewed login until the next change saves it there"
            ),
            Partway::RenewedUnwritten => write!(
                f,
                "deleted nothing. It renewed the login the file held and could not write the \
                 renewed login back there, so that login no longer works"
            ),
            Partway::RenewedKept => write!(
                f,
                "deleted nothing. It renewed the login the file held and keeps the renewed login \
                 until the next change writes it back there"
            ),
            Partway::Renewed => write!(
                f,
                "deleted nothing, having renewed the login in the file and written it back there"
            ),
            Partway::Parked { label } => write!(
                f,
                "deleted nothing, having parked the login in the file for `{label}`"
            ),
        }
    }
}

/// Why a request to Anthropic did not produce an answer Pitboard could use.
///
/// The code on an error says what Pitboard was doing; this says what went wrong underneath
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
    /// Anthropic refused the request, and asked again would refuse it the same way.
    Refused,
    /// Anthropic answered something Pitboard does not understand, which means its shape
    /// moved. Trying again will produce the same thing.
    AnswerNotUnderstood,
    /// This login is finished: revoked, or already used somewhere else.
    LoginRefused,
    /// The token has expired. For the signed-in login that is ordinary and Claude Code
    /// renews it; for a parked one it means the park needs renewing first.
    TokenExpired,
}

impl Cause {
    pub(crate) fn of_provider(error: &crate::provider::ProviderError) -> Cause {
        use crate::provider::ProviderError as P;
        match error {
            P::Unauthorized => Cause::TokenExpired,
            P::RateLimited { .. } => Cause::RateLimited,
            P::Network { .. } => Cause::Unreachable,
            P::Unavailable { .. } => Cause::ServerError,
            // The request is not one the service takes any more, which no sign-in mends.
            P::Refused { refusal, .. } if matches!(refusal.status, 404 | 405) => {
                Cause::AnswerNotUnderstood
            }
            P::Refused { .. } | P::OnHold { .. } | P::OtherClient { .. } => Cause::Refused,
            P::Unnamed { error } => Cause::of_provider(error),
            P::Malformed { .. }
            | P::ShapeUnexpected { .. }
            | P::Unsupported { .. }
            | P::NoLogin { .. }
            | P::CodexHomeMissing { .. } => Cause::AnswerNotUnderstood,
            P::InvalidGrant { .. } => Cause::LoginRefused,
        }
    }

    /// Stable, for a program to branch on; the same as its JSON form.
    pub fn code(self) -> &'static str {
        match self {
            Cause::Unreachable => "unreachable",
            Cause::RateLimited => "rate_limited",
            Cause::ServerError => "server_error",
            Cause::Refused => "refused",
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
            Cause::Refused
            | Cause::AnswerNotUnderstood
            | Cause::LoginRefused
            | Cause::TokenExpired => false,
        }
    }
}

/// The login whose account Pitboard could not confirm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unidentified {
    /// The one its tool has signed in.
    SignedIn,
    /// A login whose account its service named, and whose email it would not give.
    Unnamed,
    /// The one a sign-in for the account typed as `label` made.
    SignIn { label: String },
    /// The one parked for the account typed as `label`.
    Parked { label: String },
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error(
        "{path} looks like it is inside {marker}, which syncs to other machines. \
         Parked logins belong to one machine; set PITBOARD_HOME to a local folder."
    )]
    StateOnSyncedDrive { path: PathBuf, marker: String },

    /// A home the environment names is empty or relative, so what it leads to would depend
    /// on the folder each program runs in. `variable` is the one that named it.
    #[error("{}", not_absolute(variable, path))]
    HomeNotAbsolute {
        variable: &'static str,
        path: PathBuf,
    },

    #[error("{}.", codex_home_missing(path))]
    CodexHomeMissing { path: PathBuf },

    #[error("{}", too_large(crate::host::OS, *tool, label, *bytes, *limit))]
    CredentialTooLarge {
        tool: ProviderId,
        label: String,
        bytes: usize,
        limit: usize,
    },

    #[error(
        "`{program}` is not on this machine, and Pitboard signs in with {}'s own sign-in. \
         Install {}, or point Pitboard at it.",
        tool.name(),
        tool.name()
    )]
    ProgramMissing { tool: ProviderId, program: String },

    #[error(
        "CLAUDE_CODE_CUSTOM_OAUTH_URL is set, so Claude Code keeps its login under a \
         different name than the one Pitboard reads. Unset it to use Pitboard."
    )]
    CustomOauthEndpoint,

    #[error("could not read Pitboard's account list at {path}: {source}")]
    StateUnreadable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "Pitboard's account list at {path} is corrupt ({source}). \
         Delete it and enroll your accounts again; parked logins will be lost."
    )]
    StateCorrupt {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error(
        "{path} was written by a newer Pitboard (its format is {found}, this one reads \
         {expected}). Update this Pitboard the way you installed it. The app, and the command \
         line inside it, update with the app's Check for Updates."
    )]
    StateFromNewerVersion {
        path: PathBuf,
        found: u32,
        expected: u32,
    },

    #[error(
        "{path} is in a format ({found}) no version of Pitboard has ever written. Delete it \
         and enroll your accounts again."
    )]
    StateVersionUnknown { path: PathBuf, found: u32 },

    #[error("could not read which accounts Pitboard warms up, at {path}: {source}")]
    WarmSettingUnreadable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "which accounts Pitboard warms up, at {path}, cannot be read ({detail}). Delete it, \
         then turn warm-up on again for each account you want it for."
    )]
    WarmSettingCorrupt { path: PathBuf, detail: String },

    #[error("{}", warm_up_unsupported(*tool, label))]
    WarmUpUnsupported { tool: ProviderId, label: String },

    #[error("`{label}` was not warmed up: {}.", crate::words::warm_failed(.why))]
    WarmUpFailed {
        label: String,
        why: crate::duty::warm::Failure,
    },

    #[error("`{label}` {why}, so it is not warmed up now.")]
    WarmUpNotDue {
        label: String,
        why: String,
        code: &'static str,
    },

    #[error("Pitboard warms up no account now: {}.", crate::words::not_warming(.halt))]
    WarmUpHalted { halt: crate::duty::warm::Halt },

    #[error("another Pitboard is warming up an account. Try again in a minute.")]
    WarmUpInProgress,

    #[error(
        "{path} has an account for `{tool}`, a tool this Pitboard does not know, so it was \
         written by a newer one. Update this Pitboard the way you installed it. The app, and \
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

    #[error("{}", crate::words::no_scheduler(crate::host::OS, *job))]
    ScheduleUnsupported { job: crate::schedule::Job },

    #[error("the scheduler refused: {detail}")]
    ScheduleRefused { detail: String },

    /// Each job serves the default home alone, so it is installed only where `PITBOARD_HOME`
    /// names that home or nothing. `home` is the directory it names, and `default` the
    /// default home, `~/.pitboard`.
    #[error("{}", not_default_home(home, default, *job))]
    ScheduleNotDefaultHome {
        home: PathBuf,
        default: PathBuf,
        job: crate::schedule::Job,
    },

    #[error("{} would run {}, which is not there. Nothing was {}.", job_named(*job), path.display(), job_done(*job))]
    ScheduleProgramMissing {
        path: PathBuf,
        job: crate::schedule::Job,
    },

    /// macOS runs an app opened where it was downloaded from a copy it makes somewhere
    /// temporary, which is there while the app runs and gone once it quits. Windows' Explorer
    /// runs a program opened inside a zip from a copy of it.
    #[error("{}", temporary_copy(.path, *job))]
    ScheduleProgramTemporary {
        path: PathBuf,
        job: crate::schedule::Job,
    },

    /// An app that names no command line for the schedule, where the only other thing to
    /// schedule is the app itself, which renews nothing.
    #[error("this copy of Pitboard has no command line inside it for the renewal schedule to run.")]
    ScheduleProgramUnnamed,

    #[error("could not write to Pitboard's directory at {path}: {source}")]
    HomeUnwritable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not save Pitboard's account list at {path}: {source}")]
    StateWriteFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "Claude Code has not run on this machine yet ({path} does not exist). \
         Run `{}` once, sign in, then try again.",
        spelling.of(ProviderId::Claude)
    )]
    ClaudeConfigMissing { path: PathBuf, spelling: Spelling },

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

    #[error("{}", crate::words::nothing_signed_in(*tool, spelling.of(*tool), label.as_deref()))]
    LiveCredentialAbsent {
        tool: ProviderId,
        // the label typed, where a sentence names the command to run for it
        label: Option<String>,
        spelling: Spelling,
    },

    #[error("{}", login_elsewhere(crate::host::OS, email))]
    LiveCredentialElsewhere { email: String },

    #[error(
        "the signed-in credential is not shaped like a {} login ({detail}). \
         Run `pitboard doctor` before switching again.",
        tool.name()
    )]
    LiveCredentialShapeUnexpected { tool: ProviderId, detail: String },

    /// The tool is configured to keep its login somewhere Pitboard does not handle.
    #[error("{reason}.")]
    LiveStoreUnsupported { tool: ProviderId, reason: String },

    #[error(
        "{} would not keep `{label}`'s login: {reason}. Pitboard changed nothing. {}",
        tool.name(),
        crate::words::restricted_way_out(*tool)
    )]
    LoginRestricted {
        tool: ProviderId,
        label: String,
        reason: String,
    },

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

    #[error("{}", not_enrolled(*tool, who))]
    LiveAccountNotEnrolled { tool: ProviderId, who: String },

    #[error("{}", already_enrolled(*tool, email, label, asked, *waited))]
    AlreadyEnrolled {
        tool: ProviderId,
        email: String,
        label: String,
        // the label typed
        asked: String,
        // a sign-in inside the app was waited for under `asked`
        waited: bool,
    },

    #[error("`{label}` already refers to {who}. Choose a different label.")]
    LabelTaken { label: String, who: String },

    #[error(
        "`{typed}` is not a tool Pitboard knows. It knows: {}.",
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

    /// Asked to write an account into its tool's config as the one in use, once another
    /// account's login is stored, as after a switch made since somebody asked.
    #[error(
        "`{label}` is not signed in now, so Pitboard left {}'s config as it is. A switch \
         writes the account it puts in use there.",
        tool.name()
    )]
    AccountNotInUse { tool: ProviderId, label: String },

    #[error("could not find a free place to park this login. Run `pitboard doctor`.")]
    ParkSlotExhausted,

    #[error(
        "the parked login for `{label}` is missing. Run `pitboard enroll {label} --sign-in` \
         to sign in to it again."
    )]
    ParkedCredentialMissing { label: String },

    #[error(
        "the parked login for `{label}` is not the one Pitboard recorded ({detail}). Run \
         `pitboard enroll {label} --sign-in` to replace it."
    )]
    ParkedCredentialCorrupt { label: String, detail: String },

    #[error("could not back up Claude Code's config ({path}): {source}. Nothing was changed.")]
    ConfigBackupFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Claude Code fetches its profile again only once the config's is a day old (the
    /// register's `config_identity_is_the_last_writers`), so this does not correct itself
    /// sooner.
    #[error(
        "Claude Code's config at {path} could not be updated ({detail}), so Claude Code may \
         show another account's name than the one in use. `pitboard use` with the account in \
         use writes it there."
    )]
    ConfigWriteFailed { path: PathBuf, detail: String },

    /// The service refused the login's access token as expired. `refused` where Pitboard
    /// tried to renew the login and its service refuses its refresh token for good, or it
    /// has none, so only signing in again mends it.
    #[error("{}", session_expired(*tool, spelling.of(*tool), *refused))]
    SessionExpired {
        tool: ProviderId,
        refused: bool,
        spelling: Spelling,
    },

    #[error("{}", identity_unverifiable(*tool, spelling.of(*tool), login, *cause, detail))]
    IdentityUnverifiable {
        tool: ProviderId,
        login: Unidentified,
        cause: Cause,
        detail: String,
        spelling: Spelling,
    },

    #[error("the signed-in account changed while switching. Nothing was moved; try again.")]
    SignedInAccountChanged,

    /// The service refuses the refresh token of the login in use for good, so that login no
    /// longer works and cannot be parked, and a switch to `label` asks first to put that
    /// account's login in its place.
    #[error("{}", live_login_refused(*tool, label, detail))]
    LiveLoginRefused {
        tool: ProviderId,
        label: String,
        detail: String,
    },

    #[error(
        "could not put `{label}`'s login in place of {}'s refused login ({detail}). `{label}`'s \
         login is still parked, and nothing was lost.",
        tool.name()
    )]
    ReplaceFailed {
        tool: ProviderId,
        label: String,
        detail: String,
    },

    #[error(
        "could not put `{label}`'s login where {} reads it ({detail}). `{label}`'s login is \
         still parked, and nothing was lost.",
        tool.name()
    )]
    InstallFailed {
        tool: ProviderId,
        label: String,
        detail: String,
    },

    #[error("{}", account_on_hold(*tool, login, link.as_deref()))]
    AccountOnHold {
        tool: ProviderId,
        login: Unidentified,
        link: Option<String>,
    },

    #[error("{}", recovery_undetermined(*tool, spelling.of(*tool), from, to, detail))]
    RecoveryUndetermined {
        tool: ProviderId,
        from: String,
        to: String,
        detail: String,
        spelling: Spelling,
    },

    #[error(
        "an earlier switch from `{from}` to `{to}` stopped partway through writing {}'s login, \
         which nothing reads until a whole login is written over it, and Pitboard could not \
         write `{to}`'s ({detail}). Nothing was deleted. Fix that, then try again: Pitboard \
         finishes the switch first.",
        tool.name()
    )]
    RecoveryUnfinished {
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
        "{} no longer accepts `{label}`'s parked login, so Pitboard did not move anything. \
         The copy has been dropped; sign in to that account again with \
         `pitboard enroll {label} --sign-in`.",
        tool.service()
    )]
    ParkedLoginRefused { tool: ProviderId, label: String },

    #[error(
        "`{label}`'s parked login belongs to {who}, not to the account Pitboard has \
         under that label. Nothing was moved. Run `pitboard doctor`, then \
         `pitboard enroll {label} --sign-in` to replace it."
    )]
    ParkedLoginBelongsElsewhere { label: String, who: String },

    #[error(
        "signed in as `{to}`, and the login was gone again before Pitboard finished. {}",
        after_it_did_not_hold(*tool, spelling.of(*tool), from, to)
    )]
    SwitchDidNotHold {
        tool: ProviderId,
        from: String,
        to: String,
        spelling: Spelling,
    },

    #[error(
        "could not sign in as `{to}` ({detail}), and could not read the credential store \
         back to find out whether anything changed. Nothing has been deleted and both \
         logins are still here. {} and run `pitboard use {to}` again; it finishes or \
         undoes this before doing anything else.",
        make_readable(crate::host::OS, *tool)
    )]
    SwitchUnverified {
        tool: ProviderId,
        from: String,
        to: String,
        detail: String,
    },

    #[error(
        "could not sign in as `{to}`, and could not put `{from}` back either ({detail}). \
         `{from}`'s login is still parked: {} and sign in to any enrolled account, then \
         `pitboard use {from}`.",
        crate::words::sign_in_yourself(*tool, spelling.of(*tool), false)
    )]
    SwitchCorrupted {
        tool: ProviderId,
        from: String,
        to: String,
        detail: String,
        spelling: Spelling,
    },

    #[error(
        "signed in to `{label}` again, and Pitboard could not confirm that its new login \
         took the place of the one in use ({detail}), so {} may have no login for it now. {}",
        tool.name(),
        not_in_use(*tool, spelling.of(*tool), label, *parked)
    )]
    SignInNotInstalled {
        tool: ProviderId,
        label: String,
        detail: String,
        /// Whether the new login was parked instead, which keeps the one copy of it.
        parked: bool,
        spelling: Spelling,
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
        "the record of an interrupted switch at {path} is damaged ({source}), so Pitboard \
         cannot tell what that switch did. Nothing was changed. Check that `pitboard status` \
         shows the account you expect, then delete the file to continue."
    )]
    RecoveryRecordCorrupt {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("{}", recovery_elsewhere(*tool, from, to, slot))]
    RecoveryElsewhere {
        tool: ProviderId,
        from: String,
        to: String,
        slot: String,
    },

    #[error("could not read or write Pitboard's recovery record at {path}: {source}")]
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

    /// The file behind the store in use holds something else than what putting it away was
    /// confirmed for, or is gone.
    #[error(
        "{} has changed since putting it away was confirmed. Putting it away again says first \
         what it holds now.",
        path.display()
    )]
    LeftLoginChanged { path: PathBuf },

    /// The login stored changed before the file behind it could go, where it was going
    /// because of that login: the file held that very login, or another of the same account.
    #[error(
        "the login {} has stored changed while {} was being put away, and whether the file's \
         login is kept somewhere depended on it. Putting it away again says first what it does \
         now.",
        ProviderId::Claude.name(),
        path.display()
    )]
    StoredLoginChanged { path: PathBuf },

    /// The login left in the file is an account's nobody enrolled, so there is no account to
    /// keep it for, and deleting the file would lose it.
    #[error(
        "{} holds {email}'s login{}, an account not enrolled here, so Pitboard has no account to \
         keep it for. Enrol that account first, with `pitboard enroll <label> --sign-in`, or \
         with `pitboard enroll <label>` while Claude Code is signed in to it, then run \
         `pitboard stow` again.",
        path.display(),
        crate::words::in_organisation(organization)
    )]
    LeftLoginNotEnrolled {
        path: PathBuf,
        email: String,
        organization: String,
    },

    #[error(
        "Pitboard could not ask {} whose the login left in {} is ({detail}). {}",
        ProviderId::Claude.service(),
        path.display(),
        try_again(*cause).unwrap_or("Run `pitboard doctor`.")
    )]
    LeftLoginUnidentified {
        path: PathBuf,
        cause: Cause,
        detail: String,
    },

    /// What stopped putting away the file behind a tool's store, said with how far that had
    /// gone, which is never as far as deleting the file. Its code, cause and exit status are
    /// `error`'s, so a program branches on what stopped it.
    #[error("{} Pitboard {partway}.", sentence(error))]
    StowStopped {
        partway: Partway,
        /// What parking the login warned about, where it was parked first. Taken out by the
        /// service and reported beside the error.
        warnings: Vec<crate::service::Warning>,
        error: Box<Error>,
    },

    /// What stopped Pitboard saving a login of a tool's own it had just renewed, where no copy
    /// of the renewed login could be kept either, so the refresh token the tool's store holds
    /// is spent. Its code, cause and exit status are `error`'s.
    #[error(
        "{} Pitboard renewed {}'s login and could not save the renewed login, so the refresh \
         token stored is spent. {}, sign in, then try again.",
        sentence(error),
        tool.name(),
        crate::words::sign_in_yourself(*tool, spelling.of(*tool), true)
    )]
    RenewalLost {
        tool: ProviderId,
        error: Box<Error>,
        spelling: Spelling,
    },

    // A refusal after the change had done what `said` says. The service takes it apart.
    #[error("{error}")]
    SaidFirst {
        said: Vec<crate::service::Warning>,
        error: Box<Error>,
    },

    /// Signing in to a second account works by pointing the tool's own login at a scratch
    /// directory. Where that does not isolate it from the live login, running one would
    /// write over the account somebody is using, so Pitboard will not.
    #[error(
        "Pitboard will not sign in to a second account on this machine: {reason} Signing \
         in would write over the login you are using."
    )]
    SignInNotIsolated { reason: String },

    #[error(
        "another `pitboard enroll --sign-in` is already waiting for its sign-in. Finish or \
         cancel that one first."
    )]
    SignInInProgress,

    #[error("{}", pitboard_open(home))]
    PitboardOpen { home: PathBuf },

    #[error("{}", app_open(app, still))]
    AppOpen {
        app: &'static str,
        // what of the app still runs once its own program has quit, as `ShipIt (pid 812)`
        still: Vec<String>,
    },

    #[error(
        "Pitboard could not tell whether {app} is open ({why}), so nothing has changed. Run \
         this again."
    )]
    AppStateUnknown { app: &'static str, why: String },

    #[error(
        "{}'s data folder {what}, so Pitboard moves no {} login. {}Nothing has changed.",
        tool.program(),
        tool.name(),
        then.said()
    )]
    LoginFolderUnknown {
        tool: ProviderId,
        what: String,
        then: Then,
    },

    #[error(
        "Pitboard could not read {program}'s cookie store ({detail}). Open {program}, quit it \
         with Command-Q, then run this again. Nothing has changed.",
        program = tool.program()
    )]
    LoginFolderUnreadable { tool: ProviderId, detail: String },

    #[error(
        "{program} has not finished signing in. Open {program}, wait until it shows your \
         chats, quit it with Command-Q, then run this again. Nothing has changed.",
        program = tool.program()
    )]
    SignInUnfinished { tool: ProviderId },

    #[error("{}", in_doubt(*tool, why))]
    LoginInDoubt { tool: ProviderId, why: LoginDoubt },

    #[error(
        "Another Pitboard is changing {app}'s login now, so nothing has changed. Run this \
         again once it has finished."
    )]
    LoginFolderBusy { app: &'static str },

    #[error(
        "Pitboard parks a Claude Desktop login by renaming it into {}, which is on another \
         volume than {}, and it never copies one. Keep Pitboard's folder on the volume your \
         home is on. Nothing has changed.",
        parks.display(),
        root.display()
    )]
    DifferentVolume { parks: PathBuf, root: PathBuf },

    #[error("{}", parks_not_private(path, *why))]
    ParksNotPrivate {
        path: PathBuf,
        why: crate::host::Private,
    },

    #[error(
        "Pitboard could not read this Mac's identity, which it writes beside every parked \
         Claude Desktop login so it never puts back one parked on another Mac. Nothing has \
         changed."
    )]
    MachineUnknown,

    #[error("{}", opened_midway(app, label.as_deref()))]
    AppOpenedMidway {
        app: &'static str,
        // the account whose login was moving; None from the start of a sign-in
        label: Option<String>,
    },

    #[error(
        "Pitboard could not park `{label}`'s login ({detail}) and undid what it had done. \
         `{label}` is in use, and nothing was lost. Run this again."
    )]
    MoveUndone { label: String, detail: String },

    #[error(
        "Pitboard stopped while moving `{label}`'s login ({detail}). Keep Claude closed and \
         run this again: Pitboard settles the move first."
    )]
    MoveStopped { label: String, detail: String },

    // A settle or a give-up that failed partway: coded `recovery_undetermined`, given up alike.
    #[error(
        "Pitboard stopped while settling an interrupted move of `{label}`'s login ({detail}). \
         Nothing is lost. Keep Claude closed and run this again: Pitboard settles the move \
         first. If it stops again, `pitboard abandon` keeps every folder and moves on."
    )]
    RecoveryStopped { label: String, detail: String },

    #[error("{}", left_signed_out(from.as_deref(), to, detail))]
    SwitchLeftSignedOut {
        from: Option<String>,
        to: String,
        detail: String,
    },

    #[error("{}", move_mixed(label, places))]
    MoveMixed { label: String, places: Vec<String> },

    // the account parked last, if any, and each folder set aside that holds a login
    #[error("{}", would_strand(label.as_deref(), set_aside))]
    UninstallWouldStrand {
        label: Option<String>,
        set_aside: Vec<PathBuf>,
    },

    #[error(
        "Another Pitboard command finished, put back or removed the sign-in this one waited \
         for, so it enrolled nothing. Run `pitboard` to see who is signed in."
    )]
    SignInSettledElsewhere { label: String },

    /// The host says this process is not the person themselves (`Host::elevation`), or could
    /// not say whether it is, so Pitboard changes nothing: the one gate every change passes
    /// refused it. `why` is how it runs, as the host said it, and `None` where the host could
    /// not say.
    #[error("{}", crate::words::elevated(crate::host::OS, *why).refusal)]
    Elevated { why: Option<&'static str> },

    #[error("{}", crate::words::too_old(*build).refusal)]
    SystemTooOld { build: Option<u32> },

    /// This is a Windows build of a release made before Pitboard for Windows is released, so
    /// it does nothing ([`crate::release`]). Kept once Windows is released, where it never
    /// occurs.
    #[error("Pitboard for Windows is not released yet. This build changes nothing.")]
    WindowsNotReleased,

    /// The command line itself was wrong; the message is clap's.
    #[error("{0}")]
    Usage(String),

    /// A command that deletes something was run with nobody to ask, without `--yes`.
    #[error(
        "`pitboard {command}` asks before it deletes anything, and can ask only in a terminal \
         without `--json`. Nothing was changed. Run it in a terminal, or add `--yes`."
    )]
    ConfirmationNeeded { command: &'static str },

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
            HomeNotAbsolute { .. } => "home_not_absolute",
            CodexHomeMissing { .. } => "codex_home_missing",
            ProgramMissing { tool, .. } => match tool {
                ProviderId::Claude => "claude_program_missing",
                ProviderId::Codex => "codex_program_missing",
                ProviderId::Desktop => "desktop_program_missing",
            },
            CredentialTooLarge { .. } => "credential_too_large",
            CustomOauthEndpoint => "custom_oauth_endpoint",
            StateUnreadable { .. } => "state_unreadable",
            StateCorrupt { .. } => "state_corrupt",
            StateFromNewerVersion { .. } => "state_from_newer_version",
            StateVersionUnknown { .. } => "state_version_unknown",
            WarmSettingUnreadable { .. } => "warm_setting_unreadable",
            WarmSettingCorrupt { .. } => "warm_setting_corrupt",
            WarmUpUnsupported { .. } => "warm_up_unsupported",
            WarmUpFailed { why, .. } => why.code(),
            WarmUpNotDue { code, .. } => code,
            WarmUpHalted { halt } => halt.code(),
            WarmUpInProgress => "warm_up_in_progress",
            StateNamesUnknownTool { .. } => "state_names_unknown_tool",
            StateWrongMachine { .. } => "state_wrong_machine",
            StateWriteFailed { .. } => "state_write_failed",
            ScheduleUnsupported { .. } => "schedule_unsupported",
            ScheduleRefused { .. } => "schedule_refused",
            ScheduleNotDefaultHome { .. } => "schedule_not_default_home",
            ScheduleProgramMissing { .. } => "schedule_program_missing",
            ScheduleProgramTemporary { .. } => "schedule_program_temporary",
            ScheduleProgramUnnamed => "schedule_program_unnamed",
            HomeUnwritable { .. } => "home_unwritable",
            ClaudeConfigMissing { .. } => "claude_config_missing",
            ClaudeConfigUnreadable { .. } => "claude_config_unreadable",
            ClaudeConfigNotJson { .. } => "claude_config_not_json",
            LiveCredentialAbsent { .. } => "live_credential_absent",
            LiveStoreUnsupported { .. } => "live_store_unsupported",
            LoginRestricted { .. } => "login_restricted",
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
            AccountNotInUse { .. } => "account_not_in_use",
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
            LiveLoginRefused { .. } => "live_login_refused",
            ReplaceFailed { .. } => "replace_failed",
            InstallFailed { .. } => "install_failed",
            AccountOnHold { .. } => "account_on_hold",
            RecoveryUndetermined { .. } | RecoveryUnfinished { .. } | RecoveryStopped { .. } => {
                "recovery_undetermined"
            }
            RecoveryElsewhere { .. } => "recovery_elsewhere",
            ProgramNotFound { tool } => match tool {
                ProviderId::Claude => "claude_not_found",
                ProviderId::Codex => "codex_not_found",
                ProviderId::Desktop => "desktop_not_found",
            },
            SignInIncomplete => "sign_in_incomplete",
            LeftLoginChanged { .. } => "left_login_changed",
            StoredLoginChanged { .. } => "stored_login_changed",
            LeftLoginNotEnrolled { .. } => "left_login_not_enrolled",
            LeftLoginUnidentified { .. } => "left_login_unidentified",
            StowStopped { error, .. } | RenewalLost { error, .. } | SaidFirst { error, .. } => {
                error.code()
            }
            SignInNotIsolated { .. } => "sign_in_not_isolated",
            RenewalFailed { .. } => "renewal_failed",
            SignInInProgress => "sign_in_in_progress",
            AppOpen { .. } => "app_open",
            PitboardOpen { .. } => "pitboard_open",
            AppStateUnknown { .. } => "app_state_unknown",
            LoginFolderUnknown { .. } => "login_folder_unknown",
            LoginFolderUnreadable { .. } => "login_folder_unreadable",
            SignInUnfinished { .. } => "sign_in_unfinished",
            LoginInDoubt { .. } => "login_in_doubt",
            LoginFolderBusy { .. } => "login_folder_busy",
            DifferentVolume { .. } => "different_volume",
            ParksNotPrivate { .. } => "parks_not_private",
            MachineUnknown => "machine_unknown",
            AppOpenedMidway { .. } => "app_opened_midway",
            MoveUndone { .. } => "move_undone",
            MoveStopped { .. } => "move_stopped",
            SwitchLeftSignedOut { .. } => "switch_left_signed_out",
            MoveMixed { .. } => "move_mixed",
            UninstallWouldStrand { .. } => "uninstall_would_strand",
            SignInSettledElsewhere { .. } => "sign_in_settled_elsewhere",
            Elevated { .. } => "elevated",
            SystemTooOld { .. } => "system_too_old",
            WindowsNotReleased => "windows_not_released",
            Usage(_) => "usage",
            ConfirmationNeeded { .. } => "confirmation_needed",
            Store(e) => e.code(),
            Lock(e) => e.code(),
        }
    }

    /// What went wrong underneath, where the failure came from a request to Anthropic.
    /// `None` where nothing was asked.
    pub fn cause(&self) -> Option<Cause> {
        use Error::*;
        match self {
            IdentityUnverifiable { cause, .. } | LeftLoginUnidentified { cause, .. } => {
                Some(*cause)
            }
            RenewalFailed { cause, .. } => *cause,
            SessionExpired { refused: true, .. } | LiveLoginRefused { .. } => {
                Some(Cause::LoginRefused)
            }
            AccountOnHold { .. } => Some(Cause::Refused),
            SessionExpired { .. } => Some(Cause::TokenExpired),
            StowStopped { error, .. } | RenewalLost { error, .. } | SaidFirst { error, .. } => {
                error.cause()
            }
            _ => None,
        }
    }

    /// Warnings a failed change carries in itself, for the caller to report beside it. Only
    /// a failure that happened after something worth warning about was done carries any.
    pub(crate) fn take_warnings(&mut self) -> Vec<crate::service::Warning> {
        let mut said = self.take_said_first();
        if let Error::SignInNotInstalled { warnings, .. } | Error::StowStopped { warnings, .. } =
            self
        {
            said.append(warnings);
        }
        said
    }

    // What the change had done before it refused, outermost first. What is left is the refusal
    // itself.
    pub(crate) fn take_said_first(&mut self) -> Vec<crate::service::Warning> {
        let mut said = Vec::new();
        while let Error::SaidFirst { said: first, error } = self {
            said.append(first);
            *self = std::mem::replace(error.as_mut(), Error::SignInIncomplete);
        }
        said
    }

    // This refusal, after a change that had done what `said` says.
    pub(crate) fn after(self, said: Vec<crate::service::Warning>) -> Error {
        if said.is_empty() {
            return self;
        }
        Error::SaidFirst {
            said,
            error: Box::new(self),
        }
    }

    /// 1 when a request could not be met; 2 when the command line was wrong, or lacks the
    /// `--yes` a command that deletes needs with nobody to ask; 3 when a login or Claude
    /// Code's files are in a state Pitboard cannot safely act on: an unexpected format, or a
    /// login that could not be put back.
    pub fn exit_code(&self) -> u8 {
        use Error::*;
        match self {
            // A login or Claude Code's files in a state Pitboard will not act on, which is
            // what exit 3 means: not a failure of the attempt, a refusal to attempt.
            LiveCredentialShapeUnexpected { .. }
            | LiveStoreUnsupported { .. }
            | LoginRestricted { .. }
            | ClaudeConfigNotJson { .. }
            | SwitchCorrupted { .. }
            | SwitchUnverified { .. }
            | SwitchDidNotHold { .. }
            | SignInNotInstalled { .. }
            | LiveCredentialElsewhere { .. }
            | CredentialTooLarge { .. }
            | CustomOauthEndpoint
            | RecoveryRecordCorrupt { .. }
            | LoginFolderUnknown { .. }
            | LoginInDoubt { .. }
            | DifferentVolume { .. }
            | ParksNotPrivate { .. }
            | MachineUnknown
            | MoveMixed { .. } => 3,
            Usage(_) | ConfirmationNeeded { .. } => 2,
            Store(e) => e.exit_code(),
            StowStopped { error, .. } | RenewalLost { error, .. } | SaidFirst { error, .. } => {
                error.exit_code()
            }
            _ => 1,
        }
    }
}

/// `error`'s message as a sentence, ending with a full stop as some do not, for more to be
/// said after it.
fn sentence(error: &Error) -> String {
    let said = error.to_string();
    if said.ends_with('.') {
        said
    } else {
        format!("{said}.")
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn codex_home_missing(path: &std::path::Path) -> String {
    format!("{}. {CODEX_HOME_REMEDY}", codex_home_no_folder(path))
}

pub(crate) fn codex_home_no_folder(path: &std::path::Path) -> String {
    format!(
        "CODEX_HOME is {}, which is no folder, so Codex does not start",
        path.display()
    )
}

pub(crate) const CODEX_HOME_REMEDY: &str = "Make that folder, or unset CODEX_HOME";

fn job_named(job: crate::schedule::Job) -> &'static str {
    match job {
        crate::schedule::Job::Renew => "the renewal schedule",
        crate::schedule::Job::Watch => "the watch service",
    }
}

fn job_done(job: crate::schedule::Job) -> &'static str {
    match job {
        crate::schedule::Job::Renew => "scheduled",
        crate::schedule::Job::Watch => "installed",
    }
}

fn not_default_home(
    home: &std::path::Path,
    default: &std::path::Path,
    job: crate::schedule::Job,
) -> String {
    let (serves, which) = match job {
        crate::schedule::Job::Renew => ("renews only the parked logins", "To schedule it"),
        crate::schedule::Job::Watch => ("watches only the accounts", "To install it"),
    };
    format!(
        "{} {serves} in {}, not those in {}, which PITBOARD_HOME names. Nothing was {}. \
         {which} for {}, unset PITBOARD_HOME and install it again.",
        job_named(job),
        default.display(),
        home.display(),
        job_done(job),
        default.display()
    )
}

fn temporary_copy(path: &std::path::Path, job: crate::schedule::Job) -> String {
    let turn_on = match job {
        crate::schedule::Job::Renew => "turn on daily renewal",
        crate::schedule::Job::Watch => "install the watch service",
    };
    if crate::schedule::opened_from_a_zip(path) {
        return format!(
            "{} would run {}, which is in a temporary copy Windows made of a zip it opened. \
             Install Pitboard, or extract the zip, and {turn_on} from there.",
            job_named(job),
            path.display()
        );
    }
    format!(
        "{} would run {}, which is in a temporary copy macOS made of the app and is gone once \
         the app quits. Move Pitboard to your Applications folder, open it from there, and \
         {turn_on} again.",
        job_named(job),
        path.display()
    )
}

fn not_absolute(variable: &str, path: &std::path::Path) -> String {
    let holds = if path.as_os_str().is_empty() {
        "empty".to_string()
    } else {
        format!("`{}`, which is not a full path", path.display())
    };
    format!(
        "{variable} is {holds}, so the folder it names would depend on where each program \
         runs. Set it to a full path, or unset it; Pitboard reads and changes nothing until \
         then."
    )
}

/// [`Error::IdentityUnverifiable`]'s words: what to do follows from why.
fn identity_unverifiable(
    tool: ProviderId,
    program: &str,
    login: &Unidentified,
    cause: Cause,
    detail: &str,
) -> String {
    let service = tool.service();
    let next = match (try_again(cause), cause, login) {
        (Some(later), ..) => later.to_owned(),
        (None, Cause::AnswerNotUnderstood, _) => "Check for a Pitboard update.".to_owned(),
        (None, _, Unidentified::SignedIn | Unidentified::Unnamed) => format!(
            "{}, sign in, then try again.",
            crate::words::sign_in_yourself(tool, program, true)
        ),
        (None, _, Unidentified::Parked { label } | Unidentified::SignIn { label }) => {
            format!("Sign in to it again with `pitboard enroll {label} --sign-in`.")
        }
    };
    match login {
        Unidentified::SignedIn => format!(
            "Pitboard could not confirm with {service} which account is signed in ({detail}), \
             and will not move a login it cannot identify. {next}"
        ),
        Unidentified::Parked { label } => format!(
            "Pitboard could not confirm with {service} whose login is parked for `{label}` \
             ({detail}), and will not put in use a login it cannot identify. {next}"
        ),
        Unidentified::SignIn { label } => format!(
            "Pitboard could not confirm with {service} whose login the sign-in for `{label}` \
             made ({detail}), and keeps no login it cannot identify. {next}"
        ),
        Unidentified::Unnamed => format!(
            "{service} named the account of {}'s login and would not give its email \
             ({detail}), and Pitboard keeps every account under its email. {next}",
            tool.name()
        ),
    }
}

/// [`Error::LiveLoginRefused`]'s words.
fn live_login_refused(tool: ProviderId, label: &str, detail: &str) -> String {
    format!(
        "{} Run `pitboard use {label} --yes` to put `{label}`'s login there.",
        crate::words::replacing(tool, detail)
    )
}

/// [`Error::AccountOnHold`]'s words.
fn account_on_hold(tool: ProviderId, login: &Unidentified, link: Option<&str>) -> String {
    let service = tool.service();
    let see = link.map(|link| format!(" ({link})")).unwrap_or_default();
    let whose = match login {
        Unidentified::SignedIn | Unidentified::Unnamed => {
            format!("the account of {}'s login in use", tool.name())
        }
        Unidentified::Parked { label } | Unidentified::SignIn { label } => {
            format!("`{label}`'s account")
        }
    };
    format!(
        "{service} has put {whose} on hold{see}. Pitboard keeps its login. Follow {service}'s \
         instructions for the account, then try again."
    )
}

/// When asking again is the way on, what to say about it.
fn try_again(cause: Cause) -> Option<&'static str> {
    match cause {
        Cause::Unreachable => Some("Check the connection and try again."),
        Cause::RateLimited => Some("Try again in a few minutes."),
        Cause::ServerError => Some("Try again later."),
        Cause::Refused | Cause::AnswerNotUnderstood | Cause::LoginRefused | Cause::TokenExpired => {
            None
        }
    }
}

/// [`Error::SessionExpired`]'s words.
fn session_expired(tool: ProviderId, program: &str, refused: bool) -> String {
    if refused {
        return format!(
            "{}'s login in use has expired and cannot be renewed, so Pitboard cannot confirm \
             which account is signed in. {}, sign in, then try again.",
            tool.name(),
            crate::words::sign_in_yourself(tool, program, true)
        );
    }
    format!(
        "{}'s session has expired, so Pitboard cannot confirm which account is signed in. \
         {} once so it refreshes, then try again.",
        tool.name(),
        crate::words::run_it(tool, program, true)
    )
}

// What makes a tool's live login readable again, where it could not be read.
fn make_readable(os: Os, tool: ProviderId) -> &'static str {
    match tool {
        ProviderId::Claude => match os {
            Os::MacOs | Os::Linux => "Unlock the keychain",
            Os::Windows => {
                "Check that you can read and change Claude Code's folder and its .credentials.json"
            }
        },
        ProviderId::Codex => "Make Codex's auth.json readable to you again",
        ProviderId::Desktop => "Quit Claude",
    }
}

fn login_elsewhere(os: Os, email: &str) -> String {
    format!(
        "{}. It will not write a login where nobody reads it. This usually means Claude Code has \
         started keeping logins somewhere Pitboard does not know about yet: check for a Pitboard \
         update, and report it with `pitboard doctor --json` if there is none.",
        crate::words::named_but_not_found(os, email)
    )
}

// A keychain's ceiling is what `security` reads from stdin; Credential Manager's, 256 pieces.
fn too_large(os: Os, tool: ProviderId, label: &str, bytes: usize, limit: usize) -> String {
    match os {
        Os::MacOs | Os::Linux => format!(
            "the login for `{label}` needs {bytes} bytes and `security` reads {limit} from \
             stdin, so it can only be written on the argument line, which PITBOARD_NO_ARGV \
             forbids. {}",
            smaller(tool)
        ),
        Os::Windows => format!(
            "the login for `{label}` needs {}. Sign out of MCP servers you no longer use to make \
             the login smaller.",
            crate::words::past_credential_manager(bytes, limit)
        ),
    }
}

// What makes a login smaller, where anything does.
fn smaller(tool: ProviderId) -> &'static str {
    match tool {
        ProviderId::Claude => {
            "Unset it, or sign out of MCP servers you no longer use to make the login smaller."
        }
        ProviderId::Codex | ProviderId::Desktop => "Unset it to let Pitboard write it.",
    }
}

/// Which write took a switched-in login away again, as far as each tool is known to make
/// one, and what that leaves.
fn after_it_did_not_hold(tool: ProviderId, program: &str, from: &str, to: &str) -> String {
    match tool {
        ProviderId::Claude => format!(
            "Claude Code removes a login without taking the write lock when `/logout` has \
             given up waiting, which is the one write Pitboard cannot exclude. Nothing was \
             lost: both `{from}` and `{to}` are parked. Run `{program}` and sign in to any \
             enrolled account, then `pitboard use {to}`."
        ),
        // Not "nothing was lost": the likeliest writer is a codex session refreshing the
        // outgoing account, which spends the token in that account's park, and `codex
        // login` would revoke whatever login is left in auth.json.
        ProviderId::Codex => format!(
            "Codex takes no lock, so a codex session still running from before the switch, \
             refreshing or signing out, rewrote auth.json underneath it. `{to}` is still \
             parked. `{from}`'s park may hold a token that refresh spent. Quit every running \
             codex, then run `pitboard` to see what is signed in; do not run `{program} login` \
             or `{program} logout` until you have, because either revokes the login they find."
        ),
        ProviderId::Desktop => "Claude was opened before Pitboard finished and wrote its own \
                                login back. Quit Claude, then run `pitboard` to see which \
                                account it holds."
            .into(),
    }
}

/// Where a new login that could not be put in use went, and the way back from there.
fn not_in_use(tool: ProviderId, program: &str, label: &str, parked: bool) -> String {
    if parked {
        format!(
            "The new login is parked, so it is not lost. Run `pitboard` to see what is \
             signed in; if nothing is, {} and sign in to any enrolled account, then \
             `pitboard use {label}`.",
            crate::words::sign_in_yourself(tool, program, false)
        )
    } else {
        format!(
            "It could not be parked either: {} and sign in to `{label}` again.",
            crate::words::sign_in_yourself(tool, program, false)
        )
    }
}

// What a step of a Claude Desktop change says to do once it has refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    Nothing,
    OpenAndQuit,
    UpdatePitboard,
    CopyAside,
    SignInAndEnrol,
}

impl Then {
    // With a space after it, where it says anything.
    pub(crate) fn said(self) -> &'static str {
        match self {
            Then::Nothing => "",
            Then::OpenAndQuit => "Open Claude, quit it with Command-Q, then run this again. ",
            Then::UpdatePitboard => {
                "Update Pitboard, or report it at https://github.com/datlechin/pitboard/issues if \
                 no release reads it yet. "
            }
            Then::CopyAside => {
                "Claude resets a settings file it cannot read when it opens, so copy it aside, \
                 open Claude, quit it with Command-Q, then run this again. "
            }
            Then::SignInAndEnrol => {
                "Open Claude. If it asks you to sign in, sign in. Quit it with Command-Q, then \
                 run `pitboard enroll desktop/<label>` for the account it shows. "
            }
        }
    }
}

// Why the login Claude holds cannot be taken as any account's without the person's word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginDoubt {
    // `named`: the account Claude names, as typed; None for one Pitboard has not enrolled
    NamedAnother {
        recorded: String,
        named: Option<String>,
    },
    // `label` was signed in to outside Pitboard, beside the login it parked
    SignedInOutside {
        label: String,
    },
}

fn pitboard_open(home: &std::path::Path) -> String {
    let quit = match crate::host::OS {
        Os::MacOs | Os::Linux => "Quit it from its item in the menu bar",
        Os::Windows => "Exit it from its icon's menu in the notification area",
    };
    format!(
        "the Pitboard app is open, and it would make {} again once it was gone. {quit}, then \
         run this again. Nothing has changed.",
        home.display()
    )
}

fn app_open(app: &str, still: &[String]) -> String {
    if still.is_empty() {
        return format!(
            "{app} is open, and Pitboard changes its login only while it is closed. Quit {app} \
             with Command-Q, then run this again. Nothing has changed."
        );
    }
    format!(
        "{app} has quit, and {} from it is still running, and Pitboard changes {app}'s login \
         only while all of it is closed. {} Nothing has changed.",
        crate::words::listed(still.to_vec()),
        crate::words::wait_for_still(crate::host::OS, "run this again")
    )
}

fn in_doubt(tool: ProviderId, why: &LoginDoubt) -> String {
    let enroll = Key::new(tool, "<label>").typed();
    match why {
        LoginDoubt::NamedAnother { recorded, named } => {
            let named = named.as_ref().map_or_else(
                || "an account Pitboard has not enrolled".to_string(),
                |label| format!("`{label}`"),
            );
            format!(
                "Pitboard recorded this {program} login as `{recorded}`'s, and {program} names \
                 {named} for it. Open {program}, check which account it shows, quit it with \
                 Command-Q, then run `pitboard enroll {enroll}` under that account's name. \
                 Nothing has changed.",
                program = tool.program()
            )
        }
        LoginDoubt::SignedInOutside { label } => format!(
            "{program} was signed in to `{label}` outside Pitboard, so Pitboard cannot tell this \
             login from the one it parked. Open {program}, check which account it shows, and \
             quit it with Command-Q. If it shows `{label}`, run `pitboard enroll {label}`, then \
             this again. If it shows another account, run `pitboard enroll {enroll}` under that \
             account's name. Nothing has changed.",
            program = tool.program()
        ),
    }
}

fn not_enrolled(tool: ProviderId, who: &str) -> String {
    let enroll = Key::new(tool, "<label>").typed();
    match tool {
        ProviderId::Claude | ProviderId::Codex => format!(
            "{who} is signed in but not enrolled, so it cannot be parked. Run `pitboard enroll \
             {enroll}` for it first."
        ),
        ProviderId::Desktop => format!(
            "Claude is signed in to an account Pitboard has not enrolled, so it cannot be \
             parked. Run `pitboard enroll {enroll}` for it first."
        ),
    }
}

fn already_enrolled(
    tool: ProviderId,
    email: &str,
    label: &str,
    asked: &str,
    waited: bool,
) -> String {
    match tool {
        ProviderId::Claude | ProviderId::Codex => format!(
            "{email} is already enrolled as `{label}`. To add a different account, run \
             `pitboard enroll {} --sign-in`.",
            Key::new(tool, "<label>").typed()
        ),
        ProviderId::Desktop if waited => format!(
            "Claude is signed in to `{label}`. Run `pitboard enroll {label}` to record that \
             login, then add another with `pitboard enroll {asked} --sign-in`."
        ),
        ProviderId::Desktop => format!(
            "Claude is signed in to `{label}`, which is enrolled already. To add `{asked}`, run \
             `pitboard enroll {asked} --sign-in`."
        ),
    }
}

fn parks_not_private(path: &std::path::Path, why: crate::host::Private) -> String {
    use crate::host::Private;
    let path = path.display();
    let own = "Point `PITBOARD_HOME` at a folder of your own on this volume, then run this again.";
    let said = match why {
        Private::Shared => {
            format!("{path} can be reached by others. Run `chmod 700 {path}`, then run this again.")
        }
        Private::Link => {
            format!("{path} is a link, so Pitboard parks no Claude Desktop login there. {own}")
        }
        Private::NotOwn => {
            format!("{path} is not yours, so Pitboard parks no Claude Desktop login there. {own}")
        }
        Private::NotADirectory | Private::Missing | Private::Yes => format!(
            "{path} is not a folder, so Pitboard parks no Claude Desktop login there. {own}"
        ),
    };
    format!("{said} Nothing has changed.")
}

fn opened_midway(app: &str, label: Option<&str>) -> String {
    match label {
        Some(label) => format!(
            "{app} was opened while Pitboard moved `{label}`'s login, so Pitboard stopped. \
             Nothing is lost. Quit {app} with Command-Q, then run this again: Pitboard settles \
             the move first."
        ),
        None => format!(
            "{app} was opened while Pitboard set aside the account name {app} kept, so \
             Pitboard stopped. Nothing is lost. Quit {app} with Command-Q, then run this again."
        ),
    }
}

fn would_strand(label: Option<&str>, set_aside: &[PathBuf]) -> String {
    let every = "so uninstalling would delete every Claude Desktop login Pitboard holds.";
    match label {
        Some(label) => format!(
            "No Claude Desktop account is in use, and `{label}` is parked, {every} Run `pitboard \
             use {label}`, then uninstall, or `pitboard forget` each Claude Desktop account first."
        ),
        None => {
            let held = match set_aside {
                [_] => "holds a Claude Desktop login",
                _ => "hold Claude Desktop logins",
            };
            let folders = set_aside
                .iter()
                .map(|path| path.display().to_string())
                .collect();
            format!(
                "No Claude Desktop account is in use, and {} {held} Pitboard set aside, {every} \
                 Run `pitboard enroll desktop/<label> --sign-in` to sign in to an account you \
                 keep, then uninstall.",
                crate::words::listed(folders)
            )
        }
    }
}

fn move_mixed(label: &str, places: &[String]) -> String {
    format!(
        "Giving up would leave parts of `{label}` beside another login in Claude's folder: {}. \
         Nothing has changed. {}",
        places.join(", "),
        crate::words::mixed_way_out(label)
    )
}

fn left_signed_out(from: Option<&str>, to: &str, detail: &str) -> String {
    match from {
        Some(from) => format!(
            "`{from}` is parked, and `{to}` could not be put in use ({detail}). Claude is \
             signed out and both logins are parked. Run `pitboard use {to}` again, or `pitboard \
             use {from}` to go back."
        ),
        None => format!(
            "`{to}` could not be put in use ({detail}). Claude is signed out and `{to}` is \
             still parked. Run `pitboard use {to}` again."
        ),
    }
}

fn warm_up_unsupported(tool: ProviderId, label: &str) -> String {
    match tool {
        ProviderId::Claude | ProviderId::Codex => format!(
            "`{label}` is a {name} account, and Pitboard warms up Claude Code accounts only: when \
             a {name} period starts has not been measured.",
            name = tool.name()
        ),
        ProviderId::Desktop => format!(
            "`{label}` is a Claude Desktop account, and Pitboard sends no request for one, so it \
             warms up none."
        ),
    }
}

/// "from `work` to `home`", or "to `home`" for a replacement, which has no account going out.
fn between(from: &str, to: &str) -> String {
    if from.is_empty() {
        format!("to `{to}`")
    } else {
        format!("from `{from}` to `{to}`")
    }
}

fn recovery_undetermined(
    tool: ProviderId,
    program: &str,
    from: &str,
    to: &str,
    detail: &str,
) -> String {
    match tool {
        ProviderId::Claude | ProviderId::Codex => format!(
            "an earlier switch {} was interrupted, and Pitboard cannot yet \
             tell whether it finished ({detail}). Nothing was changed. {} once so its session \
             is current, then try again.",
            between(from, to),
            crate::words::run_it(tool, program, true)
        ),
        ProviderId::Desktop => format!(
            "An earlier move of `{to}`'s login was interrupted, and Pitboard cannot tell where it \
             stopped ({detail}). Nothing has changed. {}",
            crate::words::undetermined_way_out()
        ),
    }
}

fn recovery_elsewhere(tool: ProviderId, from: &str, to: &str, slot: &str) -> String {
    match tool {
        ProviderId::Claude | ProviderId::Codex => format!(
            "an earlier switch of {} {} was interrupted while its login was at {slot}, and \
             this run reads it from somewhere else, so it cannot tell what that switch did. \
             Nothing was changed. {} to finish it, or `pitboard abandon` to keep every login \
             it names and move on.",
            tool.name(),
            between(from, to),
            crate::words::pointing_where_it_did(tool)
        ),
        ProviderId::Desktop => format!(
            "An earlier move of `{to}`'s login was interrupted while Claude kept its data in \
             {slot}, and this run finds it somewhere else. Nothing has changed. {}",
            crate::words::elsewhere_way_out(tool)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Claude Code keeps no keychain on Windows: its file, or Credential Manager.
    #[test]
    fn claude_codes_lost_and_unreadable_logins_are_worded_for_each_system() {
        let keychain = "the login for `l` needs 9 bytes and `security` reads 4 from stdin, so it \
                        can only be written on the argument line, which PITBOARD_NO_ARGV \
                        forbids. Unset it, or sign out of MCP servers you no longer use to make \
                        the login smaller.";
        for (os, too_large_said) in [
            (Os::MacOs, keychain),
            (Os::Linux, keychain),
            (
                Os::Windows,
                "the login for `l` needs 9 bytes, past the 4 Claude Code keeps in Credential \
                 Manager. Sign out of MCP servers you no longer use to make the login smaller.",
            ),
        ] {
            assert_eq!(too_large(os, ProviderId::Claude, "l", 9, 4), too_large_said);
        }
        for (os, places, readable) in [
            (
                Os::MacOs,
                "in the keychain or in the file it also reads",
                "Unlock the keychain",
            ),
            (
                Os::Linux,
                "in the keychain or in the file it also reads",
                "Unlock the keychain",
            ),
            (
                Os::Windows,
                "in .credentials.json or in Credential Manager",
                "Check that you can read and change Claude Code's folder and its .credentials.json",
            ),
        ] {
            assert!(
                login_elsewhere(os, "a@example.com").starts_with(&format!(
                    "Claude Code's config says a@example.com is signed in, but Pitboard cannot \
                     find that login {places}. It will not write a login where nobody reads it."
                )),
                "{os:?}"
            );
            assert_eq!(make_readable(os, ProviderId::Claude), readable);
        }
    }

    // Word for word what each said before a tool could be an app, with npm's `.cmd` where found.
    #[test]
    fn every_claude_code_and_codex_sentence_reads_as_it_did() {
        let said = |tool: ProviderId, spelling: Spelling| {
            let moved = |make: fn(ProviderId, Spelling, String, String, String) -> Error| {
                make(tool, spelling, "a".into(), "b".into(), "d".into()).to_string()
            };
            let not_installed = |parked| Error::SignInNotInstalled {
                tool,
                label: "l".into(),
                detail: "d".into(),
                parked,
                spelling,
                warnings: Vec::new(),
            };
            vec![
                Error::LiveCredentialAbsent {
                    tool,
                    label: None,
                    spelling,
                }
                .to_string(),
                moved(
                    |tool, spelling, from, to, detail| Error::RecoveryUndetermined {
                        tool,
                        from,
                        to,
                        detail,
                        spelling,
                    },
                ),
                moved(|tool, spelling, from, to, detail| Error::SwitchCorrupted {
                    tool,
                    from,
                    to,
                    detail,
                    spelling,
                }),
                moved(|tool, _, from, to, slot| Error::RecoveryElsewhere {
                    tool,
                    from,
                    to,
                    slot,
                }),
                Error::RenewalLost {
                    tool,
                    error: Box::new(Error::SignedInAccountChanged),
                    spelling,
                }
                .to_string(),
                Error::SessionExpired {
                    tool,
                    refused: true,
                    spelling,
                }
                .to_string(),
                Error::SessionExpired {
                    tool,
                    refused: false,
                    spelling,
                }
                .to_string(),
                not_installed(true).to_string(),
                not_installed(false).to_string(),
                crate::service::Warning::SignInParkedNotInUse {
                    tool,
                    label: "l".into(),
                    why: "w".into(),
                    spelling,
                }
                .to_string(),
                Error::LiveAccountNotEnrolled {
                    tool,
                    who: "a@b.c".into(),
                }
                .to_string(),
                Error::AlreadyEnrolled {
                    tool,
                    email: "a@b.c".into(),
                    label: "l".into(),
                    asked: "other".into(),
                    waited: false,
                }
                .to_string(),
            ]
        };
        let before = |name: &str, sign_in: &str, run: &str, home: &str, enroll: &str| {
            vec![
                format!(
                    "nothing is signed in to {name} right now. Run `{sign_in}`, sign in, then try again."
                ),
                format!(
                    "an earlier switch from `a` to `b` was interrupted, and Pitboard cannot yet \
                     tell whether it finished (d). Nothing was changed. Run `{run}` once so its \
                     session is current, then try again."
                ),
                format!(
                    "could not sign in as `b`, and could not put `a` back either (d). `a`'s login \
                     is still parked: run `{sign_in}` and sign in to any enrolled account, then \
                     `pitboard use a`."
                ),
                format!(
                    "an earlier switch of {name} from `a` to `b` was interrupted while its login \
                     was at d, and this run reads it from somewhere else, so it cannot tell what \
                     that switch did. Nothing was changed. Run Pitboard with {home} pointing \
                     where it did to finish it, or `pitboard abandon` to keep every login it \
                     names and move on."
                ),
                format!(
                    "the signed-in account changed while switching. Nothing was moved; try again. \
                     Pitboard renewed {name}'s login and could not save the renewed login, so \
                     the refresh token stored is spent. Run `{sign_in}`, sign in, then try again."
                ),
                format!(
                    "{name}'s login in use has expired and cannot be renewed, so Pitboard cannot \
                     confirm which account is signed in. Run `{sign_in}`, sign in, then try again."
                ),
                format!(
                    "{name}'s session has expired, so Pitboard cannot confirm which account is \
                     signed in. Run `{run}` once so it refreshes, then try again."
                ),
                format!(
                    "signed in to `l` again, and Pitboard could not confirm that its new login \
                     took the place of the one in use (d), so {name} may have no login for it \
                     now. The new login is parked, so it is not lost. Run `pitboard` to see what \
                     is signed in; if nothing is, run `{sign_in}` and sign in to any enrolled \
                     account, then `pitboard use l`."
                ),
                format!(
                    "signed in to `l` again, and Pitboard could not confirm that its new login \
                     took the place of the one in use (d), so {name} may have no login for it \
                     now. It could not be parked either: run `{sign_in}` and sign in to `l` again."
                ),
                format!(
                    "{name} goes on with the login it has: Pitboard could not tell whose it is \
                     (w), so it parked the new login for `l` rather than write over that one. If \
                     that login no longer works, run `{sign_in}` and sign in to `l` there."
                ),
                format!(
                    "a@b.c is signed in but not enrolled, so it cannot be parked. Run `pitboard \
                     enroll {enroll}` for it first."
                ),
                format!(
                    "a@b.c is already enrolled as `l`. To add a different account, run `pitboard \
                     enroll {enroll} --sign-in`."
                ),
            ]
        };
        let pairs = |tool, spelling, expected: Vec<String>| {
            for (now, then) in said(tool, spelling).into_iter().zip(expected) {
                assert_eq!(now, then, "{tool}");
            }
        };
        for spelling in [Spelling::Bare, Spelling::Cmd] {
            let (claude, codex) = (
                spelling.of(ProviderId::Claude),
                spelling.of(ProviderId::Codex),
            );
            pairs(
                ProviderId::Claude,
                spelling,
                before(
                    "Claude Code",
                    claude,
                    claude,
                    "CLAUDE_CONFIG_DIR",
                    "<label>",
                ),
            );
            pairs(
                ProviderId::Codex,
                spelling,
                before(
                    "Codex",
                    &format!("{codex} login"),
                    codex,
                    "CODEX_HOME",
                    "codex/<label>",
                ),
            );
        }
        // Claude Desktop has words of its own for each, and none names a program to run.
        for sentence in said(ProviderId::Desktop, Spelling::Bare) {
            assert!(!sentence.contains("Run `Claude`"), "{sentence}");
            assert!(!sentence.contains("run `Claude`"), "{sentence}");
        }
        let absent = |label: Option<&str>| {
            Error::LiveCredentialAbsent {
                tool: ProviderId::Desktop,
                label: label.map(str::to_string),
                spelling: Spelling::Bare,
            }
            .to_string()
        };
        assert_eq!(
            absent(Some("desktop/travel")),
            "Claude is not signed in. Run `pitboard enroll desktop/travel --sign-in`, then sign \
             in inside Claude and quit it: Pitboard waits for that, and sets aside any account \
             name Claude kept first."
        );
        assert!(absent(None).contains("`pitboard enroll desktop/<label> --sign-in`"));
    }

    // Windows PowerShell 5.1 refuses the `claude.ps1` beside npm's `claude.cmd`.
    #[test]
    fn every_command_to_run_names_the_cmd_pitboard_found() {
        let missing = Error::ClaudeConfigMissing {
            path: PathBuf::from("C:/Users/x/.claude.json"),
            spelling: Spelling::Cmd,
        };
        assert!(
            missing
                .to_string()
                .ends_with("Run `claude.cmd` once, sign in, then try again."),
            "{missing}"
        );
        let gone = |tool, spelling| {
            Error::SwitchDidNotHold {
                tool,
                from: "a".into(),
                to: "b".into(),
                spelling,
            }
            .to_string()
        };
        let claude = gone(ProviderId::Claude, Spelling::Cmd);
        assert!(
            claude.contains("Run `claude.cmd` and sign in to any enrolled account"),
            "{claude}"
        );
        let codex = gone(ProviderId::Codex, Spelling::Cmd);
        assert!(
            codex.contains("do not run `codex.cmd login` or `codex.cmd logout`"),
            "{codex}"
        );
        let broken = crate::store::Error::Broken {
            why: "`t` lacks piece #2 of the 3 its manifest names".into(),
            spelling: Spelling::Cmd,
        };
        assert!(
            broken
                .to_string()
                .contains("`claude.cmd auth logout` deletes it"),
            "{broken}"
        );
    }

    #[test]
    fn a_moves_refusals_say_their_code_their_exit_and_what_to_do() {
        let said = |error: Error| (error.code(), error.exit_code(), error.to_string());
        let label = || "desktop/work".to_string();
        assert_eq!(
            said(Error::MachineUnknown),
            (
                "machine_unknown",
                3,
                "Pitboard could not read this Mac's identity, which it writes beside every \
                 parked Claude Desktop login so it never puts back one parked on another Mac. \
                 Nothing has changed."
                    .into()
            )
        );
        assert_eq!(
            said(Error::AppOpenedMidway {
                app: "Claude",
                label: Some(label())
            }),
            (
                "app_opened_midway",
                1,
                "Claude was opened while Pitboard moved `desktop/work`'s login, so Pitboard \
                 stopped. Nothing is lost. Quit Claude with Command-Q, then run this again: \
                 Pitboard settles the move first."
                    .into()
            )
        );
        assert!(
            said(Error::AppOpenedMidway {
                app: "Claude",
                label: None
            })
            .2
            .contains("set aside the account name Claude kept")
        );
        assert_eq!(
            said(Error::SwitchLeftSignedOut {
                from: Some(label()),
                to: "desktop/home".into(),
                detail: "d".into()
            }),
            (
                "switch_left_signed_out",
                1,
                "`desktop/work` is parked, and `desktop/home` could not be put in use (d). \
                 Claude is signed out and both logins are parked. Run `pitboard use \
                 desktop/home` again, or `pitboard use desktop/work` to go back."
                    .into()
            )
        );
        assert!(
            said(Error::SwitchLeftSignedOut {
                from: None,
                to: "desktop/home".into(),
                detail: "d".into()
            })
            .2
            .ends_with("`desktop/home` is still parked. Run `pitboard use desktop/home` again.")
        );
        assert_eq!(
            said(Error::RecoveryStopped {
                label: label(),
                detail: "d".into()
            }),
            (
                "recovery_undetermined",
                1,
                "Pitboard stopped while settling an interrupted move of `desktop/work`'s login \
                 (d). Nothing is lost. Keep Claude closed and run this again: Pitboard settles \
                 the move first. If it stops again, `pitboard abandon` keeps every folder and \
                 moves on."
                    .into()
            )
        );
        let parks = PathBuf::from("/p/parks");
        for (why, words) in [
            (crate::host::Private::Shared, "Run `chmod 700 /p/parks`"),
            (crate::host::Private::Link, "/p/parks is a link"),
            (crate::host::Private::NotOwn, "/p/parks is not yours"),
        ] {
            let (code, exit, sentence) = said(Error::ParksNotPrivate {
                path: parks.clone(),
                why,
            });
            assert_eq!((code, exit), ("parks_not_private", 3));
            assert!(sentence.contains(words), "{sentence}");
            assert!(sentence.ends_with("Nothing has changed."), "{sentence}");
        }
        for (error, code, exit) in [
            (
                Error::DifferentVolume {
                    parks: parks.clone(),
                    root: PathBuf::from("/data"),
                },
                "different_volume",
                3,
            ),
            (
                Error::MoveUndone {
                    label: label(),
                    detail: "d".into(),
                },
                "move_undone",
                1,
            ),
            (
                Error::MoveStopped {
                    label: label(),
                    detail: "d".into(),
                },
                "move_stopped",
                1,
            ),
            (
                Error::MoveMixed {
                    label: label(),
                    places: vec!["`a` in Claude's folder".into()],
                },
                "move_mixed",
                3,
            ),
            (
                Error::UninstallWouldStrand {
                    label: Some(label()),
                    set_aside: Vec::new(),
                },
                "uninstall_would_strand",
                1,
            ),
            (
                Error::UninstallWouldStrand {
                    label: None,
                    set_aside: vec![PathBuf::from("/p/parks/pitboard-aside-1")],
                },
                "uninstall_would_strand",
                1,
            ),
            (
                Error::SignInSettledElsewhere { label: label() },
                "sign_in_settled_elsewhere",
                1,
            ),
        ] {
            assert_eq!((error.code(), error.exit_code()), (code, exit), "{error}");
            assert!(!error.to_string().contains('\u{2014}'), "{error}");
        }
        let waited = Error::AlreadyEnrolled {
            tool: ProviderId::Desktop,
            email: String::new(),
            label: "desktop/home".into(),
            asked: "desktop/travel".into(),
            waited: true,
        };
        assert_eq!(
            waited.to_string(),
            "Claude is signed in to `desktop/home`. Run `pitboard enroll desktop/home` to \
             record that login, then add another with `pitboard enroll desktop/travel \
             --sign-in`."
        );
    }

    #[test]
    fn codes_are_unique_so_a_caller_can_branch_on_them() {
        let samples = [
            Error::LiveCredentialAbsent {
                tool: ProviderId::Claude,
                label: None,
                spelling: Spelling::Bare,
            },
            Error::ParkSlotExhausted,
            Error::SignedInAccountChanged,
            Error::AccountUnknown {
                label: "x".into(),
                enrolled: Enrolled::default(),
            },
            Error::NothingParked {
                tool: ProviderId::Claude,
                label: "x".into(),
            },
            Error::ParkedLoginExpired { label: "x".into() },
            Error::AccountNotInUse {
                tool: ProviderId::Claude,
                label: "x".into(),
            },
            Error::LabelTaken {
                label: "x".into(),
                who: "e".into(),
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
            Error::Elevated { why: None },
            Error::SystemTooOld { build: Some(22631) },
            Error::WindowsNotReleased,
            Error::LeftLoginChanged { path: "f".into() },
            Error::StoredLoginChanged { path: "f".into() },
            Error::Usage("u".into()),
            Error::ConfirmationNeeded { command: "stow" },
        ];
        let mut codes: Vec<&str> = samples.iter().map(Error::code).collect();
        codes.sort_unstable();
        let before = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), before);
    }

    /// Whatever stops putting the file away is branched on by its own code, cause and exit
    /// status, and says after its own words how far putting it away had gone.
    #[test]
    fn what_stopped_putting_a_file_away_keeps_its_code_and_says_how_far_it_went() {
        let stopped = Error::StowStopped {
            partway: Partway::Parked {
                label: "work".into(),
            },
            warnings: Vec::new(),
            error: Box::new(Error::Store(crate::store::Error::Locked)),
        };

        assert_eq!(stopped.code(), "credential_store_locked");
        assert_eq!(
            stopped.exit_code(),
            Error::Store(crate::store::Error::Locked).exit_code()
        );
        assert!(
            stopped.to_string().ends_with(
                "desktop session. Pitboard deleted nothing, having parked the login in the file \
                 for `work`."
            ),
            "{stopped}"
        );

        let unidentified = Error::StowStopped {
            partway: Partway::Nothing,
            warnings: Vec::new(),
            error: Box::new(Error::LeftLoginUnidentified {
                path: "f".into(),
                cause: Cause::Unreachable,
                detail: "offline".into(),
            }),
        };
        assert_eq!(unidentified.code(), "left_login_unidentified");
        assert_eq!(unidentified.cause(), Some(Cause::Unreachable));
        assert!(
            unidentified
                .to_string()
                .ends_with("try again. Pitboard changed nothing."),
            "{unidentified}"
        );
    }

    #[test]
    fn a_refusal_after_what_was_said_is_given_back_as_itself() {
        let refusal = || Error::AccountNotInUse {
            tool: ProviderId::Desktop,
            label: "desktop/work".into(),
        };
        let waiting = crate::service::Warning::MoveWaiting {
            tool: ProviderId::Desktop,
            label: "desktop/work".into(),
        };
        let mut carried = refusal().after(vec![waiting]);
        assert_eq!(carried.code(), "account_not_in_use");
        assert_eq!(carried.exit_code(), refusal().exit_code());
        assert_eq!(carried.to_string(), refusal().to_string());
        let said = carried.take_warnings();
        assert_eq!(
            said.iter()
                .map(crate::service::Warning::code)
                .collect::<Vec<_>>(),
            ["move_waiting"]
        );
        assert!(
            matches!(carried, Error::AccountNotInUse { .. }),
            "{carried:?}"
        );
        assert!(carried.take_warnings().is_empty());
        let alone = refusal().after(Vec::new());
        assert!(matches!(alone, Error::AccountNotInUse { .. }), "{alone:?}");
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
                label: None,
                spelling: Spelling::Bare,
            }
            .to_string(),
            Error::LiveCredentialAbsent {
                tool: ProviderId::Codex,
                label: None,
                spelling: Spelling::Bare,
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
                who: "a@b.c".into(),
            }
            .to_string(),
            Error::SignInNotKept {
                tool: ProviderId::Codex,
                label: "codex/work".into(),
                detail: "the keychain is locked".into(),
            }
            .to_string(),
            Error::ConfirmationNeeded { command: "forget" }.to_string(),
            Error::LoginRestricted {
                tool: ProviderId::Claude,
                label: "work".into(),
                reason: "a pin leaves it out".into(),
            }
            .to_string(),
            Error::LoginRestricted {
                tool: ProviderId::Codex,
                label: "codex/work".into(),
                reason: "a setting rules it out".into(),
            }
            .to_string(),
        ];
        for message in actionable {
            assert!(
                ["Run ", "Sign in", "Switch to"]
                    .iter()
                    .any(|way| message.contains(way)),
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
            label: None,
            spelling: Spelling::Bare,
        }
        .to_string();
        assert!(
            codex.contains("Codex") && codex.contains("`codex login`"),
            "{codex}"
        );
        assert!(!codex.contains("Claude"), "{codex}");

        let unverifiable = Error::IdentityUnverifiable {
            tool: ProviderId::Codex,
            login: Unidentified::SignedIn,
            cause: Cause::Unreachable,
            detail: "offline".into(),
            spelling: Spelling::Bare,
        }
        .to_string();
        assert!(unverifiable.contains("OpenAI"), "{unverifiable}");

        let claude = Error::IdentityUnverifiable {
            tool: ProviderId::Claude,
            login: Unidentified::SignedIn,
            cause: Cause::Unreachable,
            detail: "offline".into(),
            spelling: Spelling::Bare,
        }
        .to_string();
        assert!(claude.contains("confirm with Anthropic"), "{claude}");
    }

    // A 400 used to say "check the connection", which no connection mends.
    #[test]
    fn a_refusal_says_to_sign_in_again_and_an_outage_to_try_again() {
        let said = |cause| {
            Error::IdentityUnverifiable {
                tool: ProviderId::Claude,
                login: Unidentified::SignedIn,
                cause,
                detail: "Anthropic answered 400: No organization associated with this token".into(),
                spelling: Spelling::Bare,
            }
            .to_string()
        };
        assert_eq!(
            said(Cause::Refused),
            "Pitboard could not confirm with Anthropic which account is signed in (Anthropic \
             answered 400: No organization associated with this token), and will not move a \
             login it cannot identify. Run `claude`, sign in, then try again."
        );
        assert!(said(Cause::Unreachable).ends_with("Check the connection and try again."));
        assert!(said(Cause::ServerError).ends_with("Try again later."));
        assert!(Cause::ServerError.worth_retrying());
        assert!(!Cause::Refused.worth_retrying());
    }

    // The reporter's dialog said "which account is signed in" when it may have been the
    // parked login of the account being switched to that failed.
    #[test]
    fn the_parked_login_that_failed_is_named() {
        let said = |cause| {
            Error::IdentityUnverifiable {
                tool: ProviderId::Claude,
                login: Unidentified::Parked {
                    label: "dtg".into(),
                },
                cause,
                detail: "Anthropic answered 400".into(),
                spelling: Spelling::Bare,
            }
            .to_string()
        };
        assert_eq!(
            said(Cause::Refused),
            "Pitboard could not confirm with Anthropic whose login is parked for `dtg` \
             (Anthropic answered 400), and will not put in use a login it cannot identify. Sign \
             in to it again with `pitboard enroll dtg --sign-in`."
        );
        assert!(said(Cause::Unreachable).ends_with("Check the connection and try again."));
        assert!(!said(Cause::Refused).contains("signed in ("));
    }

    // A refused sign-in used to be told to run `claude` and sign in to the tool itself.
    #[test]
    fn a_sign_in_that_cannot_be_identified_is_signed_in_again_through_pitboard() {
        let said = Error::IdentityUnverifiable {
            tool: ProviderId::Claude,
            login: Unidentified::SignIn {
                label: "dtg".into(),
            },
            cause: Cause::Refused,
            detail: "Anthropic answered 400".into(),
            spelling: Spelling::Bare,
        }
        .to_string();
        assert_eq!(
            said,
            "Pitboard could not confirm with Anthropic whose login the sign-in for `dtg` made \
             (Anthropic answered 400), and keeps no login it cannot identify. Sign in to it \
             again with `pitboard enroll dtg --sign-in`."
        );
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

    /// A Pitboard that finds its files written by a newer one is updated by the route it
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
                path,
                tool: "gemini".into(),
            }
            .to_string(),
        ] {
            assert!(
                message.contains("Update this Pitboard the way you installed it."),
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
            who: "a@b.c".into(),
        }
        .to_string();
        assert!(codex.contains("pitboard enroll codex/<label>"), "{codex}");
        let claude = Error::LiveAccountNotEnrolled {
            tool: ProviderId::Claude,
            who: "a@b.c".into(),
        }
        .to_string();
        assert!(claude.contains("pitboard enroll <label>"), "{claude}");
        let already = Error::AlreadyEnrolled {
            tool: ProviderId::Codex,
            email: "a@b.c".into(),
            label: "codex/work".into(),
            asked: "codex/other".into(),
            waited: false,
        }
        .to_string();
        assert!(
            already.contains("pitboard enroll codex/<label> --sign-in"),
            "{already}"
        );
    }

    /// Only a command that changes something finishes an interrupted switch; a plain
    /// `pitboard` reads and leaves it. So the message names the switch to run again, as it
    /// would be typed here.
    #[test]
    fn an_unverified_switch_names_a_command_that_finishes_it() {
        let message = Error::SwitchUnverified {
            tool: ProviderId::Codex,
            from: "codex/personal".into(),
            to: "codex/work".into(),
            detail: "auth.json is not readable".into(),
        }
        .to_string();
        assert!(
            message.contains("run `pitboard use codex/work` again"),
            "{message}"
        );
        assert!(!message.contains("run `pitboard` again"), "{message}");
    }
}
