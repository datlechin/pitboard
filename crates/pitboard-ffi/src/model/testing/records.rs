use crate::model::Snapshot;
use crate::present::testing::standing_of;
use crate::{
    Account, Adoption, AutoSwitched, Change, Check, Enrolled, EnrolledAs, Holding, Level, Limit,
    PitboardError, Remedy, Status, Switch, Switched, Tool, Usage, Warning,
};
use pitboard_core::provider::ProviderId;

/// A control's words as this system writes them, title case on a Mac and sentence case on
/// Windows, for a test of what `shown` says.
pub(crate) fn titled(title_case: &str, sentence_case: &str) -> String {
    crate::present::words::titled(pitboard_core::host::OS, title_case, sentence_case)
}

/// The ChatGPT app's bundle id, which is how the core's holder detection names it.
pub(crate) const CHATGPT: &str = "com.openai.codex";

/// Where ChatGPT runs its own `codex`, as the process list of a Mac running ChatGPT
/// 26.928.31416 gave it, which is how the core tells the app is open.
pub(crate) const CHATGPT_CODEX: &str =
    "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex";

pub(crate) fn claude_code() -> Tool {
    crate::tool(ProviderId::Claude)
}

pub(crate) fn codex() -> Tool {
    crate::tool(ProviderId::Codex)
}

pub(crate) use crate::present::testing::{
    awaiting, claude_desktop, desktop_account, desktop_warning,
};

// Claude's bundle id, which is how the core's holder detection names it.
pub(crate) const CLAUDE: &str = "com.anthropic.claudefordesktop";

// Claude running with a login in memory, as the core's holder detection reports it.
pub(crate) fn claude_holding() -> Holding {
    Holding {
        kind: "claude_desktop".into(),
        phrase: "Claude".into(),
        pids: vec![812],
        remedy: Remedy::ReopenApp {
            app_id: CLAUDE.into(),
            name: "Claude".into(),
        },
    }
}

/// A limit as the core reports one.
pub(crate) fn window(kind: &str, percent: f64) -> Limit {
    Limit {
        kind: kind.into(),
        length_seconds: None,
        scope: None,
        percent,
        resets_at: Some(100),
        severity: None,
        is_active: true,
    }
}

/// An account as the core reports one. `label` `None` is a login signed in and not
/// enrolled. Switchable unless it is the one signed in, and its numbers every limit it has
/// where its tool is Claude Code, as a real one is.
pub(crate) fn account(
    label: Option<&str>,
    provider: &str,
    signed_in: bool,
    windows: Vec<Limit>,
) -> Account {
    let uuid = label.unwrap_or("someone");
    Account {
        id: format!("{provider}:{uuid}"),
        provider: provider.into(),
        label: label.map(str::to_owned),
        qualified: label.map(|label| format!("{provider}/{label}")),
        unplaced: false,
        email: format!("{uuid}@example.com"),
        account_id: uuid.into(),
        signed_in,
        standing: standing_of(signed_in, label.is_some()),
        parked: None,
        usage: Some(Usage {
            observed_at: Some(0),
            windows,
            lists_every_limit: provider == "claude",
        }),
        stale: None,
        stale_explanation: None,
        plan: None,
        read_at: None,
    }
}

/// A Claude Code account whose five-hour window is `percent` used.
pub(crate) fn claude(label: &str, signed_in: bool, percent: f64) -> Account {
    account(
        Some(label),
        "claude",
        signed_in,
        vec![window("session", percent)],
    )
}

/// A Codex account with no limits read.
pub(crate) fn codex_account(label: &str, signed_in: bool) -> Account {
    account(Some(label), "codex", signed_in, Vec::new())
}

pub(crate) fn status(accounts: Vec<Account>) -> Status {
    warned(accounts, Vec::new())
}

pub(crate) fn warned(accounts: Vec<Account>, warnings: Vec<Warning>) -> Status {
    Status {
        now: 0,
        accounts,
        warnings,
        awaiting: Vec::new(),
    }
}

pub(crate) fn warning(code: &str, message: &str) -> Warning {
    Warning {
        code: code.into(),
        message: message.into(),
        account: None,
        held: None,
        provider: None,
    }
}

/// The warning a Codex switch carries when `codex` sessions it cannot reach still run, as
/// the core words it.
pub(crate) fn still_running() -> Warning {
    warning(
        "sessions_still_running",
        "2 `codex` sessions started before this switch are still running and still using \
         `codex/personal`. Quit them and start again to use the new account. Do not sign out \
         in any of them: signing out there revokes `codex/personal`'s login, which Pitboard \
         has just parked.",
    )
}

/// A switch of `provider`'s tool, as the core reports one: `from` and `to` typed the way
/// the core types them, bare for Claude Code and with the tool for any other. Codex's open
/// sessions never follow; Claude Code's follow within 33 seconds.
pub(crate) fn switched(
    provider: &str,
    from: &str,
    to: &str,
    warnings: Vec<Warning>,
) -> Result<Switched, Refusal> {
    let adoption = if provider == "codex" {
        Adoption::RestartRequired {
            program: "codex",
            holders: &[],
        }
    } else {
        Adoption::PollingWithin(33)
    };
    Ok(Switched {
        outcome: Switch::Switched {
            provider: provider.into(),
            from: from.into(),
            to: to.into(),
            adoption,
        },
        warnings,
    })
}

/// Nothing to switch by itself: no limit of `account`, the account in use, has reached the
/// share, with no reading to say how much of which it has used.
pub(crate) fn watching(account: &str) -> AutoSwitched {
    AutoSwitched::Watching {
        account: account.into(),
        used: None,
        as_of: None,
        held_until: None,
    }
}

/// A switch to the account already in use, as the core reports one.
pub(crate) fn already_active(label: &str, warnings: Vec<Warning>) -> Result<Switched, Refusal> {
    Ok(Switched {
        outcome: Switch::AlreadyActive {
            label: label.into(),
        },
        warnings,
    })
}

/// What a sign-in enrolled, as the core reports it.
pub(crate) fn enrolled_as(
    outcome: EnrolledAs,
    warnings: Vec<Warning>,
) -> Result<Enrolled, Refusal> {
    Ok(Enrolled {
        email: "w@example.com".into(),
        outcome,
        warnings,
    })
}

/// ChatGPT running Codex's login, as the core's holder detection reports it.
pub(crate) fn chatgpt_holding() -> Holding {
    Holding {
        kind: "chatgpt_app".into(),
        phrase: "the ChatGPT app".into(),
        pids: vec![4242, 4243],
        remedy: Remedy::ReopenApp {
            app_id: CHATGPT.into(),
            name: "ChatGPT".into(),
        },
    }
}

/// A failure as the core reports one.
#[derive(Debug, Clone)]
pub(crate) struct Refusal {
    pub code: String,
    pub message: String,
    pub warnings: Vec<Warning>,
}

impl Refusal {
    pub(crate) fn error(&self) -> PitboardError {
        PitboardError::Failed {
            code: self.code.clone(),
            message: self.message.clone(),
            warnings: self.warnings.clone(),
        }
    }
}

pub(crate) fn refusal(code: &str, message: &str, warnings: Vec<Warning>) -> Refusal {
    Refusal {
        code: code.into(),
        message: message.into(),
        warnings,
    }
}

// The percentage the first limit of the row at `index` shows.
pub(crate) fn percent(snapshot: &Snapshot, index: usize) -> Option<f64> {
    let row = snapshot
        .sections
        .iter()
        .flat_map(|section| &section.accounts)
        .nth(index)?;
    Some(row.limits.first()?.percent)
}

/// The Pitboard directory of a test's app, as the account windows' records key it.
pub(crate) const DANA: &str = "/Users/dana/.pitboard";

/// Where a scheduler keeps the renewal job, as the core names it on a Mac. Never written.
pub(crate) const PLIST: &str = "/Users/x/Library/LaunchAgents/com.usepitboard.renew.plist";

/// A check as doctor reports one.
pub(crate) fn check(code: &str, name: &str, level: Level, detail: &str, advice: &str) -> Check {
    Check {
        code: code.into(),
        name: name.into(),
        level,
        detail: detail.into(),
        advice: advice.into(),
    }
}

/// A change as the core's log keeps one.
pub(crate) fn change(at: &str, caller: &str, verb: &str, subject: &str, outcome: &str) -> Change {
    Change {
        at: at.into(),
        caller: caller.into(),
        verb: verb.into(),
        subject: subject.into(),
        outcome: outcome.into(),
    }
}

// A read has answered: the window says when.
pub(crate) fn read_in(snapshot: &Snapshot) -> bool {
    !snapshot.reading && !snapshot.updated_window.is_empty()
}
