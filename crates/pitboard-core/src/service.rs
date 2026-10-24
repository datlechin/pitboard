//! Pitboard's operations, each run as every front end must: settled first, then audited.

use crate::context::Context;
use crate::doctor::{self, Diagnosis};
use crate::duty::autoswitch::{Auto, Look, Threshold};
use crate::error::{Error, Result};
use crate::holder;
use crate::host::Os;
pub use crate::in_use::Stored;
pub use crate::permit::Permit;
use crate::permit::gate;
use crate::provider::{LoginKind, ProviderId};
use crate::state::{self, Account, Key};
use crate::switch::{self, Enrolled, Outcome, Renewal, Settled, SignIn};
use crate::usage::budget::Ask;
use crate::usage::readings;
pub use crate::warning::Warning;
use crate::{audit, schedule, status, statusline};

#[derive(Debug)]
pub struct Done<T> {
    pub value: T,
    pub warnings: Vec<Warning>,
}

/// A change that failed, with what was found on the way.
#[derive(Debug)]
pub struct Failed {
    pub error: Error,
    pub warnings: Vec<Warning>,
    // how many of `warnings`, from the first, were done before the refusal
    pub said_first: usize,
}

impl Failed {
    // What was done first leads: an interrupted switch settled, then what the change did.
    fn of(mut error: Error, found: Vec<Warning>) -> Failed {
        let (mut warnings, found): (Vec<Warning>, Vec<Warning>) = found
            .into_iter()
            .partition(|warning| matches!(warning, Warning::Recovered(_)));
        warnings.extend(error.take_said_first());
        let said_first = warnings.len();
        warnings.extend(found);
        warnings.extend(error.take_warnings());
        Failed {
            error,
            warnings,
            said_first,
        }
    }
}

// A refusal with nothing found on the way.
impl From<Error> for Failed {
    fn from(error: Error) -> Failed {
        Failed {
            error,
            warnings: Vec::new(),
            said_first: 0,
        }
    }
}

pub type Changing<T> = std::result::Result<Done<T>, Failed>;

#[derive(Debug)]
pub struct Pitboard {
    ctx: Context,
}

impl Pitboard {
    pub fn new(ctx: Context) -> Pitboard {
        Pitboard { ctx }
    }

    /// Whether this process may change anything here, with the proof the gate hands out.
    pub fn permit(&self) -> Result<Permit> {
        gate(&self.ctx)
    }

    // None where this process may change nothing here, or another app holds it.
    pub fn hold_open(&self) -> Option<crate::app::Open> {
        crate::app::Open::hold(&self.ctx, self.permit().ok()?)
    }

    /// Whether every home the environment names is a full path, as the gate asks.
    pub fn check_homes(&self) -> Result<()> {
        crate::home::check_absolute(&self.ctx)
    }

    fn permitted(&self) -> std::result::Result<Permit, Failed> {
        self.permit().map_err(Failed::from)
    }

    /// Who is signed in and what every account has left; `fresh` asks each anew.
    pub fn status(&self, fresh: bool) -> Result<Done<status::Report>> {
        let permit = match self.permit() {
            Ok(permit) => permit,
            Err(refused) => {
                let Some(warning) = Warning::read_only(&refused) else {
                    return Err(refused);
                };
                let mut read = self.status_offline()?;
                read.warnings.push(warning);
                return Ok(read);
            }
        };
        let renewed = switch::renew_parked(&self.ctx, permit);
        // Unreadable is not empty, which would say the enrolled logins are gone.
        let state = state::load(&self.ctx)?;
        // Beside the answers: asked one after the other, a silent service held the read twice.
        let (stuck, answers) = std::thread::scope(|scope| {
            let stuck = scope.spawn(|| switch::stuck(&self.ctx, &state, switch::Asking::Service));
            let when = if fresh { Ask::Now } else { Ask::WhenDue };
            let answers = status::ask(&self.ctx, permit, &state, when);
            (stuck.join().ok().flatten(), answers)
        });
        switch::identify::record_read(&self.ctx, permit, &answers.found, &answers.plans);
        let found = answers.found.clone();
        // Again, so the rows are of whose login each tool has stored as just recorded.
        let state = state::load(&self.ctx)?;
        let report = status::report(&self.ctx, permit, &state, answers);
        let mut warnings: Vec<Warning> = stuck.map(Warning::SwitchStuck).into_iter().collect();
        for (key, outcome) in renewed {
            audit::record(&self.ctx, permit, "renew", &key.typed(), outcome.code());
            match outcome {
                Renewal::Refused => warnings.push(Warning::ParkedLoginRefused {
                    tool: key.provider,
                    label: state.typed(&key),
                }),
                Renewal::Failed(e) => warnings.push(Warning::RenewalFailed(e)),
                Renewal::Renewed | Renewal::Deferred => {}
            }
        }
        warnings.extend(
            ProviderId::ALL
                .iter()
                .filter_map(|&tool| self.left_behind(tool)),
        );
        warnings.extend(status::standing(&self.ctx, &state, &found));
        Ok(Done {
            value: report,
            warnings,
        })
    }

    pub fn doctor(&self) -> Diagnosis {
        self.doctor_with(&crate::time::moment)
    }

    /// The same checks, each moment one names written by `moment`, given it and now.
    pub fn doctor_with(&self, moment: &dyn Fn(i64, i64) -> String) -> Diagnosis {
        doctor::run(&self.ctx, moment)
    }

    /// The same report without asking anyone or renewing anything.
    pub fn status_offline(&self) -> Result<Done<status::Report>> {
        let state = state::load(&self.ctx)?;
        let warnings = switch::stuck(&self.ctx, &state, switch::Asking::Nobody)
            .map(Warning::SwitchStuck)
            .into_iter()
            .chain(status::standing(&self.ctx, &state, &[]))
            .collect();
        Ok(Done {
            value: status::gather_offline(&self.ctx, &state),
            warnings,
        })
    }

    /// The status line for Claude Code's session JSON, from files alone.
    pub fn statusline(&self, session: &str) -> statusline::StatusLine {
        statusline::read(&self.ctx, self.permit().ok(), session)
    }

    /// The enrolled account under `typed`, if any, read without taking the lock.
    pub fn account(&self, typed: &str) -> Option<Account> {
        let state = state::load(&self.ctx).ok()?;
        crate::label::resolve(&state, typed).ok().cloned()
    }

    pub fn switch_to(&self, typed: &str) -> Changing<Outcome> {
        let permit = self.permitted()?;
        let key = self.named(permit, "use", typed)?;
        self.changing(permit, "use", &key.typed(), Some(key.provider), |settled| {
            switch::switch(settled, &key)
        })
    }

    /// [`Pitboard::switch_to`] in place of a login its service refuses for good, parking nothing.
    pub fn replace_with(&self, typed: &str) -> Changing<Outcome> {
        let permit = self.permitted()?;
        let key = self.named(permit, "use", typed)?;
        self.changing(permit, "use", &key.typed(), Some(key.provider), |settled| {
            switch::replace(settled, &key)
        })
    }

    /// [`Pitboard::switch_to`] for the account in use, refused where another's is stored by then.
    pub fn update_config(&self, typed: &str) -> Changing<Outcome> {
        let permit = self.permitted()?;
        let key = self.named(permit, "use", typed)?;
        self.changing(permit, "use", &key.typed(), Some(key.provider), |settled| {
            switch::update_config(settled, &key)
        })
    }

    /// Switches Claude Code where a limit of the account in use has reached `threshold`.
    pub fn auto_switch(&self, threshold: Threshold) -> Changing<Auto> {
        let permit = self.permitted()?;
        let looked = self.auto_look(threshold).map_err(Failed::from)?;
        if let Look::Stands(value) = looked {
            return Ok(Done {
                value: *value,
                warnings: Vec::new(),
            });
        }
        let tool = ProviderId::Claude;
        let (settled, mut warnings) = self.settled_for(permit, "auto-switch", "", Some(tool))?;
        match switch::automatically(settled, threshold) {
            Ok((value, more)) => {
                warnings.extend(more);
                Ok(Done { value, warnings })
            }
            Err(error) => Err(Failed::of(error, warnings)),
        }
    }

    /// What switching by itself would come to now, from files alone and on no lock.
    pub fn auto_look(&self, threshold: Threshold) -> Result<Look> {
        let state = state::load(&self.ctx)?;
        Ok(crate::duty::autoswitch::look(&self.ctx, &state, threshold))
    }

    // Here, so every command takes `codex/work` and a bare `work` on the same terms.
    fn named(&self, permit: Permit, verb: &str, typed: &str) -> std::result::Result<Key, Failed> {
        let state = state::load(&self.ctx).map_err(Failed::from)?;
        crate::label::resolve(&state, typed)
            .map(Account::key)
            .map_err(|error| self.refused(permit, verb, typed, error.code(), error))
    }

    /// `typed` may name a tool, as in `claude/work`; a bare name means the default tool.
    pub fn enroll_current(&self, typed: &str) -> Changing<Enrolled> {
        let permit = self.permitted()?;
        let key = self.chosen(permit, "enroll", typed)?;
        self.changing(
            permit,
            "enroll",
            &key.typed(),
            Some(key.provider),
            |settled| switch::enroll(settled, &key, None),
        )
    }

    // Here, so nothing below sees a name with a tool still stuck to its front.
    fn chosen(&self, permit: Permit, verb: &str, typed: &str) -> std::result::Result<Key, Failed> {
        crate::label::enrolling(state::load(&self.ctx).ok().as_ref(), typed)
            .map_err(|error| self.refused(permit, verb, typed, "label_unusable", error))
    }

    // A mistyped name still settles an interrupted switch, and says what it found.
    fn refused(
        &self,
        permit: Permit,
        verb: &str,
        subject: &str,
        code: &str,
        error: Error,
    ) -> Failed {
        let recovered = if switch::interrupted(&self.ctx) {
            switch::settle(&self.ctx, permit, switch::interrupted_tool(&self.ctx))
                .ok()
                .and_then(|(_, recovered)| recovered)
        } else {
            None
        };
        let mut warnings = Vec::new();
        if let Some(r) = recovered {
            audit::record(&self.ctx, permit, "recover", &r.to, r.code());
            warnings.push(Warning::Recovered(r));
        }
        audit::record(&self.ctx, permit, verb, subject, code);
        Failed::of(error, warnings)
    }

    /// The enrolled account `pitboard enroll <typed>` would sign in to again.
    pub fn account_to_enroll(&self, typed: &str) -> Option<Account> {
        let state = state::load(&self.ctx).ok()?;
        let key = crate::label::enrolling(Some(&state), typed).ok()?;
        state.get(&key).cloned()
    }

    /// What to type for the account `pitboard enroll <typed>` is about, on this machine.
    pub fn name_to_type(&self, typed: &str) -> String {
        let state = state::load(&self.ctx).ok();
        match crate::label::enrolling(state.as_ref(), typed) {
            Ok(key) => state.map_or_else(|| key.typed(), |state| state.typed(&key)),
            Err(_) => typed.to_string(),
        }
    }

    /// What the tool already runs, for a front end to print before a sign-in; off Windows, `None`.
    pub fn running_before_sign_in(&self, typed: &str) -> Option<String> {
        let which = crate::label::choose(typed)
            .map(|chosen| chosen.provider)
            .unwrap_or(crate::label::DEFAULT);
        switch::running_already(&self.ctx, which)
    }

    /// A program's sign-in, in a private directory, taking no lock but its own.
    pub fn sign_in(&self, typed: &str) -> std::result::Result<SignIn, Failed> {
        let permit = self.permitted()?;
        let key = self.chosen(permit, "enroll", typed)?;
        let driver = match crate::provider::of(key.provider) {
            LoginKind::Document(driver) => driver,
            // The wait is the core's, and it prints nothing: a front end reads `said`.
            LoginKind::Folder(_) => {
                return self
                    .sign_in_watched(typed)?
                    .finish()
                    .map_err(|error| self.not_started(permit, typed, error));
            }
        };
        switch::ready_to_sign_in(&self.ctx, driver)
            .and_then(|()| switch::sign_in(&self.ctx, permit, driver))
            .map_err(|error| self.not_started(permit, typed, error))
    }

    /// A sign-in whose caller shows what is said, and types a code back where one is read.
    pub fn sign_in_watched(
        &self,
        typed: &str,
    ) -> std::result::Result<switch::WatchedSignIn, Failed> {
        let permit = self.permitted()?;
        let key = self.chosen(permit, "enroll", typed)?;
        let driver = match crate::provider::of(key.provider) {
            LoginKind::Document(driver) => driver,
            // `signin.lock` is taken before `state.lock`, as every sign-in's.
            LoginKind::Folder(app) => {
                let one_at_a_time = switch::one_at_a_time(&self.ctx, permit)
                    .map_err(|error| self.not_started(permit, typed, error))?;
                let subject = key.typed();
                let settling = self.settled_for(permit, "park", &subject, Some(key.provider))?;
                // A sign-in returns no warnings, so what settling found is said first.
                let first = settling
                    .1
                    .iter()
                    .map(|warning| format!("{warning}\n"))
                    .collect();
                return self
                    .change(permit, "park", &subject, settling, |settled| {
                        switch::folder::awaited::start(settled, &key, app, one_at_a_time, first)
                    })
                    .map(|started| started.value);
            }
        };
        switch::ready_to_sign_in(&self.ctx, driver)
            .and_then(|()| switch::sign_in_watched_as(&self.ctx, permit, driver, &key))
            .map_err(|error| self.not_started(permit, typed, error))
    }

    /// The email a stored login names by itself, asking nobody: `None` for Claude Code's.
    pub fn signed_in_email(&self, which: ProviderId) -> Option<String> {
        let LoginKind::Document(tool) = crate::provider::of(which) else {
            return None;
        };
        if !tool.identifies_by_itself() {
            return None;
        }
        let credential = tool.read_live(&self.ctx).ok()??;
        let owner = tool.identify(&self.ctx, &credential).ok()?;
        Some(owner.email).filter(|email| !email.is_empty())
    }

    /// The tools a sign-in could start here now.
    pub fn installed(&self) -> Vec<ProviderId> {
        crate::provider::installed_among(&self.ctx, crate::host::proc::can_run)
    }

    fn not_started(&self, permit: Permit, typed: &str, error: Error) -> Failed {
        audit::record(&self.ctx, permit, "enroll", typed, error.code());
        Failed::from(error)
    }

    pub fn enroll_signed_in(&self, typed: &str, login: SignIn) -> Changing<Enrolled> {
        let permit = self.permitted()?;
        let key = self.chosen(permit, "enroll", typed)?;
        self.changing(
            permit,
            "enroll",
            &key.typed(),
            Some(key.provider),
            |settled| switch::enroll(settled, &key, Some(login)),
        )
    }

    /// Returns the account's email.
    pub fn forget(&self, typed: &str) -> Changing<String> {
        let permit = self.permitted()?;
        let key = self.named(permit, "forget", typed)?;
        self.changing(
            permit,
            "forget",
            &key.typed(),
            Some(key.provider),
            |settled| switch::forget(settled, &key),
        )
    }

    /// The way out of a switch or a move that cannot be finished.
    pub fn abandon_recovery(&self) -> Result<switch::GaveUp> {
        let permit = self.permit()?;
        let outcome = switch::abandon(&self.ctx, permit);
        audit::record(
            &self.ctx,
            permit,
            "abandon",
            "",
            match &outcome {
                Ok(_) => "ok",
                Err(e) => e.code(),
            },
        );
        outcome
    }

    /// Renews every parked login that is due, as the schedule runs it, in the default home.
    pub fn renew(&self) -> Result<Vec<(Key, Renewal)>> {
        let ctx = schedule::for_its_run(&self.ctx, schedule::Job::Renew);
        let permit = gate(&ctx)?;
        let outcomes = switch::renew_due(&ctx, permit, switch::Due::ToStayAlive);
        for (key, outcome) in &outcomes {
            audit::record(&ctx, permit, "renew", &key.typed(), outcome.code());
        }
        Ok(outcomes)
    }

    /// Whether the daily renewal schedule is installed.
    pub fn schedule(&self) -> schedule::Installed {
        schedule::status(&self.ctx)
    }

    /// Whether this home is the default one, the only one the schedule renews.
    pub fn schedule_renews_this_home(&self) -> bool {
        schedule::serves(&self.ctx)
    }

    /// Has the system's scheduler run `renew` daily; refused away from the default home.
    pub fn schedule_install(&self) -> Result<schedule::Location> {
        schedule::install(&self.ctx, self.permit()?)
    }

    /// `false` where nothing was installed.
    pub fn schedule_uninstall(&self) -> Result<bool> {
        schedule::uninstall(&self.ctx, self.permit()?)
    }

    /// Each job that runs this context's command line, the app's own; `false` where none.
    pub fn schedule_uninstall_for_app(&self) -> Result<bool> {
        schedule::uninstall_for_app(&self.ctx, self.permit()?)
    }

    /// Points a schedule whose Pitboard is gone, or an old app's, at this command line.
    pub fn schedule_repair(&self) -> Result<bool> {
        schedule::repair(&self.ctx, self.permit()?)
    }

    /// Whether `pitboard watch` runs from sign-in on, from the system's service manager.
    pub fn watch_service(&self) -> crate::watch_service::Installed {
        crate::watch_service::status(&self.ctx)
    }

    /// Has the system's service manager run `pitboard watch` at `at` from each sign-in.
    pub fn watch_service_install(&self, at: Threshold) -> Result<schedule::Location> {
        crate::watch_service::install(&self.ctx, self.permit()?, at)
    }

    /// `false` where nothing was installed.
    pub fn watch_service_uninstall(&self) -> Result<bool> {
        crate::watch_service::uninstall(&self.ctx, self.permit()?)
    }

    /// Keeps the accounts of a directory another machine wrote, without their logins.
    pub fn adopt(&self) -> Result<Option<switch::Adopted>> {
        switch::adopt(&self.ctx, self.permit()?)
    }

    /// Gives back or deletes every parked login in the store that no record names.
    pub fn repair(&self) -> Changing<switch::Reclaimed> {
        let permit = self.permitted()?;
        self.changing(permit, "repair", "", None, switch::repair)
    }

    /// What runs `which`'s tool with a login a switch would leave it on, by kind.
    pub fn holding(&self, which: ProviderId) -> Vec<holder::Holding> {
        match switch::still_holding(&self.ctx, which) {
            switch::StillHolding::These(holding) => holding,
            switch::StillHolding::Nothing | switch::StillHolding::Unknown => Vec::new(),
        }
    }

    /// Whether `which`'s app may be opened again, from the index alone.
    pub fn whole(&self, which: ProviderId) -> bool {
        match crate::provider::of(which) {
            LoginKind::Document(_) => true,
            LoginKind::Folder(_) => {
                let Ok(state) = state::load(&self.ctx) else {
                    return false;
                };
                let parked = state.accounts.iter().any(|a| {
                    a.provider() == which
                        && a.parked
                            .as_ref()
                            .is_some_and(|park| park.kind == state::ParkKind::Folder)
                });
                let nobody = state
                    .in_use(which)
                    .is_some_and(|record| record.owner.is_none());
                let left_signed_out = nobody && parked && state.awaiting(which).is_none();
                state.moving(which).is_none() && !left_signed_out
            }
        }
    }

    /// Whether a move of `which`'s login waits, from the index alone.
    pub fn moving(&self, which: ProviderId) -> bool {
        match crate::provider::of(which) {
            LoginKind::Document(_) => false,
            LoginKind::Folder(_) => {
                state::load(&self.ctx).map_or(true, |state| state.moving(which).is_some())
            }
        }
    }

    /// Where an app's parked logins and the folders set aside are kept.
    pub fn parks_folder(&self) -> std::path::PathBuf {
        crate::store::folder::parks_dir(&self.ctx)
    }

    /// `PITBOARD_HOME`, or the default in the home.
    pub fn directory(&self) -> std::path::PathBuf {
        crate::home::dir(&self.ctx)
    }

    /// The one the renewal schedule renews, whatever `PITBOARD_HOME` says.
    pub fn default_directory(&self) -> std::path::PathBuf {
        self.ctx.default_pitboard_home()
    }

    /// As a person reads a folder: under the home it starts `~` on macOS and Linux.
    pub fn shown(&self, folder: &std::path::Path) -> String {
        let home = self.ctx.home();
        match folder.strip_prefix(home) {
            Ok(rest) if self.ctx.os != Os::Windows && !home.as_os_str().is_empty() => {
                std::path::Path::new("~").join(rest).display().to_string()
            }
            _ => folder.display().to_string(),
        }
    }

    /// When Pitboard's account index last changed, in epoch milliseconds.
    pub fn changed_at(&self) -> i64 {
        state::changed_at(&self.ctx)
    }

    /// When Pitboard's usage readings last changed, in epoch milliseconds.
    pub fn readings_changed_at(&self) -> i64 {
        readings::changed_at(&self.ctx)
    }

    /// What the app keeps in `file` in Pitboard's directory, or `None`.
    pub fn app_file(&self, file: crate::app::AppFile) -> std::io::Result<Option<String>> {
        crate::app::read_app_file(&self.ctx, file)
    }

    /// Keeps `body` in the app's `file` in Pitboard's directory, private and whole.
    pub fn keep_app_file(&self, file: crate::app::AppFile, body: &str) -> Result<()> {
        crate::app::write_app_file(&self.ctx, self.permit()?, file, body)
    }

    /// The changes Pitboard has made, newest last.
    pub fn log(&self, limit: usize) -> Vec<audit::Entry> {
        audit::read(&self.ctx, limit)
    }

    /// Removes the schedule, every parked login and Pitboard's directory; logins in use stay.
    pub fn uninstall(&self) -> Changing<switch::Removed> {
        let permit = self.permitted()?;
        self.changing(permit, "uninstall", "", None, |settled| {
            switch::uninstall(settled).map(|mut removed| {
                let said = std::mem::take(&mut removed.warnings);
                (removed, said)
            })
        })
    }

    /// Returns the account's email; `to` stays in the account's own tool.
    pub fn rename(&self, from: &str, to: &str) -> Changing<String> {
        let permit = self.permitted()?;
        let from = self.named(permit, "rename", from)?;
        let chosen = crate::label::choose(to)
            .map(|chosen| Key::new(chosen.provider, chosen.label))
            .map_err(|refusal| {
                self.refused(
                    permit,
                    "rename",
                    to,
                    "label_unusable",
                    Error::Usage(refusal),
                )
            })?;
        if let Some(error) = crate::label::moved_by_a_rename(&from, to, chosen.provider) {
            return Err(self.refused(permit, "rename", &from.typed(), error.code(), error));
        }
        let to = chosen.label;
        self.changing(
            permit,
            "rename",
            &format!("{from} -> {to}"),
            Some(from.provider),
            |settled| switch::rename(settled, &from, &to).map(|email| (email, Vec::new())),
        )
    }

    /// Each Claude Code account, whether it is warmed up, and when, from files alone.
    pub fn warm_overview(&self) -> Result<crate::duty::warm::Overview> {
        let state = state::load(&self.ctx)?;
        let setting = crate::duty::warm::load(&self.ctx)?;
        Ok(crate::duty::warm::overview(&self.ctx, &state, &setting))
    }

    /// Warms up the account `typed` names now, whatever the span of the day.
    pub fn warm_now(&self, typed: &str) -> Changing<switch::warm::WarmUp> {
        let permit = self.permitted()?;
        let key = self.named(permit, "warm-up", typed)?;
        let settled = self.settled_to_warm(permit, "warm-up", &key.typed())?;
        Self::warming(settled, switch::warm::Target::Account(key))
    }

    /// Warms up the first account turned on that is due, as [`crate::duty::warm`] says.
    pub fn warm_up(&self) -> Changing<switch::warm::WarmUp> {
        let permit = self.permitted()?;
        let looked = self.warm_look().map_err(Failed::from)?;
        if let crate::duty::Look::Stands(overview) = looked {
            return Ok(Done {
                value: switch::warm::WarmUp::Stands(overview),
                warnings: Vec::new(),
            });
        }
        let settled = self.settled_to_warm(permit, "warm-up", "")?;
        Self::warming(settled, switch::warm::Target::Due)
    }

    // From files alone and on no lock, so a front end may look as often as it likes.
    pub(crate) fn warm_look(&self) -> Result<crate::duty::Look<crate::duty::warm::Overview>> {
        let state = state::load(&self.ctx)?;
        crate::duty::warm::look(&self.ctx, &state)
    }

    // A warm-up's run is given nothing of this process's environment, so no override is said.
    fn settled_to_warm(
        &self,
        permit: Permit,
        verb: &str,
        subject: &str,
    ) -> std::result::Result<(Settled, Vec<Warning>), Failed> {
        let (settled, recovered) = self.settled(permit, verb, subject, Some(ProviderId::Claude))?;
        Ok((settled, recovered.into_iter().collect()))
    }

    fn warming(
        (settled, mut warnings): (Settled, Vec<Warning>),
        target: switch::warm::Target,
    ) -> Changing<switch::warm::WarmUp> {
        match switch::warm::warm_up(settled, target) {
            Ok((value, more)) => {
                warnings.extend(more);
                Ok(Done { value, warnings })
            }
            Err(error) => Err(Failed::of(error, warnings)),
        }
    }

    /// Turns warming up of the account `typed` names on or off; whether that changed anything.
    pub fn warm_turn(&self, typed: &str, on: bool) -> Changing<bool> {
        let verb = if on { "warm-on" } else { "warm-off" };
        let permit = self.permitted()?;
        let key = self.named(permit, verb, typed)?;
        let subject = key.typed();
        let settled = self.settled_to_warm(permit, verb, &subject)?;
        self.change(permit, verb, &subject, settled, |settled| {
            switch::warm::turn(settled, &key, on).map(|changed| (changed, Vec::new()))
        })
    }

    /// Warms up within `range` of each day.
    pub fn warm_range(
        &self,
        range: crate::duty::warm::Range,
    ) -> Changing<crate::duty::warm::Range> {
        let permit = self.permitted()?;
        let subject = format!(
            "{}-{}",
            crate::duty::warm::clock(range.from()),
            crate::duty::warm::clock(range.to())
        );
        let settled = self.settled_to_warm(permit, "warm-range", &subject)?;
        self.change(permit, "warm-range", &subject, settled, |settled| {
            switch::warm::range(settled, range).map(|range| (range, Vec::new()))
        })
    }

    /// The login left in the file behind Claude Code's store, and what putting it away would do.
    pub fn left_login(&self) -> Result<Option<switch::Left>> {
        let state = state::load(&self.ctx)?;
        switch::left_login(&self.ctx, &state)
    }

    /// Puts away the login left behind Claude Code's store, while the file holds what `seen` was.
    pub fn stow(&self, seen: &str) -> Changing<switch::Stowed> {
        let permit = self.permitted()?;
        let tool = ProviderId::Claude;
        let (settled, recovered) = self.settled(permit, "stow", "", Some(tool))?;
        let warnings = self.overridden(tool).into_iter().chain(recovered).collect();
        self.change(permit, "stow", "", (settled, warnings), |settled| {
            switch::stow(settled, seen)
        })
    }

    // From files as well, so the app, with no shell environment, says what the command line says.
    fn overridden(&self, tool: ProviderId) -> Option<Warning> {
        let driver = match crate::provider::of(tool) {
            LoginKind::Document(driver) => driver,
            LoginKind::Folder(_) => return None,
        };
        let names = driver.overridden_by(&self.ctx);
        (!names.is_empty()).then_some(Warning::AuthOverridden { tool, names })
    }

    // It reads the store, so a read that asks nobody does not ask it.
    fn left_behind(&self, tool: ProviderId) -> Option<Warning> {
        let driver = match crate::provider::of(tool) {
            LoginKind::Document(driver) => driver,
            LoginKind::Folder(_) => return None,
        };
        let behind = driver.behind(&self.ctx)?;
        Some(Warning::FallbackLogin {
            tool,
            path: behind.path,
            held: behind.held,
        })
    }

    // Only the change's own tool's overrides are worth a warning.
    fn changing<T: Audited>(
        &self,
        permit: Permit,
        verb: &str,
        subject: &str,
        tool: Option<ProviderId>,
        run: impl FnOnce(Settled) -> Result<(T, Vec<Warning>)>,
    ) -> Changing<T> {
        let settled = self.settled_for(permit, verb, subject, tool)?;
        self.change(permit, verb, subject, settled, run)
    }

    // Audited of the subject the change names, where it names one.
    fn change<T: Audited>(
        &self,
        permit: Permit,
        verb: &str,
        subject: &str,
        (settled, mut warnings): (Settled, Vec<Warning>),
        run: impl FnOnce(Settled) -> Result<(T, Vec<Warning>)>,
    ) -> Changing<T> {
        match run(settled) {
            Ok((value, more)) => {
                let subject = value.audit_subject().unwrap_or(subject);
                audit::record(&self.ctx, permit, verb, subject, value.audit_code());
                warnings.extend(more);
                Ok(Done { value, warnings })
            }
            Err(error) => {
                audit::record(&self.ctx, permit, verb, subject, error.code());
                Err(Failed::of(error, warnings))
            }
        }
    }

    fn settled_for(
        &self,
        permit: Permit,
        verb: &str,
        subject: &str,
        tool: Option<ProviderId>,
    ) -> std::result::Result<(Settled, Vec<Warning>), Failed> {
        let (settled, recovered) = self.settled(permit, verb, subject, tool)?;
        let mut warnings = Vec::new();
        if let Some(tool) = tool {
            warnings.extend(self.overridden(tool));
            warnings.extend(self.left_behind(tool));
        }
        warnings.extend(recovered);
        Ok((settled, warnings))
    }

    // A failed settle is audited as `verb` of `subject`; a finished recovery as `recover`.
    fn settled(
        &self,
        permit: Permit,
        verb: &str,
        subject: &str,
        tool: Option<ProviderId>,
    ) -> std::result::Result<(Settled, Option<Warning>), Failed> {
        let (settled, recovered) = switch::settle(&self.ctx, permit, tool).map_err(|error| {
            audit::record(&self.ctx, permit, verb, subject, error.code());
            Failed::from(error)
        })?;
        let recovered = recovered.map(|r| {
            audit::record(&self.ctx, permit, "recover", &r.to, r.code());
            Warning::Recovered(r)
        });
        Ok((settled, recovered))
    }
}

// How a successful change is written in the audit log.
trait Audited {
    fn audit_code(&self) -> &'static str {
        "ok"
    }

    // The account it was of, where only running it could tell.
    fn audit_subject(&self) -> Option<&str> {
        None
    }
}

impl Audited for Outcome {
    fn audit_code(&self) -> &'static str {
        match self {
            Outcome::Switched { .. } | Outcome::Installed { .. } => "ok",
            Outcome::AlreadyActive {
                config_updated: true,
                ..
            } => "config_updated",
            Outcome::AlreadyActive { .. } => "already_active",
        }
    }
}

impl Audited for switch::Reclaimed {}
impl Audited for switch::WatchedSignIn {}
impl Audited for Enrolled {}
impl Audited for String {}
impl Audited for bool {}
impl Audited for crate::duty::warm::Range {}

impl Audited for switch::Stowed {
    fn audit_code(&self) -> &'static str {
        self.kept.code()
    }

    fn audit_subject(&self) -> Option<&str> {
        self.kept.account()
    }
}

impl Audited for switch::Removed {
    fn audit_code(&self) -> &'static str {
        if self.pending > 0 {
            "parks_pending_removal"
        } else {
            "ok"
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;
    use crate::host::{Elevation, Floor};
    use crate::in_use::InUse;
    use crate::switch::Recovered;
    use crate::switch::harness::{
        Rig, Tool, audit_lines, codex_machine, hold, machine, signed_in_outside, state_file,
    };
    use std::collections::BTreeMap;
    use std::sync::Arc;

    type Make = fn(&str) -> Rig;

    // Codex's login says whose it is, so reading it asks nobody; Claude Code's does not say.
    #[test]
    fn whose_login_a_tool_stores_is_read_from_the_login_alone() {
        let m = codex_machine("signed-in-email");
        let pitboard = Pitboard::new(m.ctx.clone());
        let calls = m.api.calls();
        assert_eq!(
            pitboard.signed_in_email(ProviderId::Codex).as_deref(),
            Some("here@example.com")
        );
        assert_eq!(m.api.calls(), calls, "nobody was asked");
        assert_eq!(pitboard.signed_in_email(ProviderId::Claude), None);
        assert_eq!(pitboard.signed_in_email(ProviderId::Desktop), None);

        m.sign_in(&serde_json::json!({"auth_mode": "apikey", "OPENAI_API_KEY": "sk-test"}));
        assert_eq!(
            pitboard.signed_in_email(ProviderId::Codex),
            None,
            "no account's"
        );
        m.fault_live(crate::store::memory::Fault::Unreadable("locked".into()));
        assert_eq!(
            pitboard.signed_in_email(ProviderId::Codex),
            None,
            "unreadable"
        );
    }

    /// A name is refused the same way whichever tool it is for.
    const MACHINES: [(&str, Make); 2] = [("claude", machine), ("codex", codex_machine)];

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
    )]
    fn a_read_opens_no_jar_and_lists_no_process() {
        let m = switch::harness::desktop_machine("reads");
        let pitboard = Pitboard::new(m.ctx.clone());
        let (jars, listings) = (m.mem.jars_asked().len(), m.mem.listings());
        // Claude Code's own record, of nobody, is this first read's to make.
        pitboard.status(true).expect("read");
        let before = state_file(&m);
        let online = pitboard.status(true).expect("read");
        let offline = pitboard.status_offline().expect("read");
        let doctor = pitboard.doctor();
        pitboard.statusline("{}");
        assert_eq!(m.mem.jars_asked().len(), jars, "no jar was opened");
        assert_eq!(m.mem.listings(), listings, "no process was listed");
        assert_eq!(state_file(&m), before, "nothing was recorded");

        // And in the rows a change would open the jar for: 10, 15 and 6.
        let data = m.ctx_home().join("claude/data");
        let rows: [(&str, &dyn Fn()); 3] = [
            ("no session", &|| {
                crate::desktop_testing::write_stand_in_jar(&data.join("Cookies"), 24, &[]);
            }),
            ("another name", &|| {
                let settings = data.join("config.json");
                let written = std::fs::read_to_string(&settings).expect("settings");
                let renamed = written.replace(
                    switch::harness::WORK.uuid,
                    "0b7e2c44-2222-4d1f-8a35-91c0de77aa01",
                );
                std::fs::write(&settings, renamed).expect("renamed");
            }),
            ("no jar", &|| {
                std::fs::remove_file(data.join("Cookies")).expect("the jar taken away");
            }),
        ];
        for (row, change) in rows {
            change();
            pitboard.status(true).expect("read");
            pitboard.status_offline().expect("read");
            pitboard.doctor();
            pitboard.statusline("{}");
            assert_eq!(m.mem.jars_asked().len(), jars, "{row}: no jar was opened");
            assert_eq!(m.mem.listings(), listings, "{row}: no process was listed");
            assert_eq!(state_file(&m), before, "{row}: nothing was recorded");
        }

        for report in [&online.value, &offline.value] {
            let row = report
                .rows
                .iter()
                .find(|row| row.provider == ProviderId::Desktop)
                .expect("a row");
            assert!(row.signed_in);
            assert_eq!(row.read_at, Some(switch::harness::NOW));
        }
        for warnings in [&online.warnings, &offline.warnings] {
            assert!(
                warnings
                    .iter()
                    .all(|w| !w.to_string().contains("Claude Desktop")
                        && w.code() != "in_use_unconfirmed"),
                "{warnings:?}"
            );
        }
        let codes: Vec<&str> = doctor.checks.iter().map(|c| c.code).collect();
        for code in [
            "desktop_data_folder",
            "desktop_settings",
            "desktop_layout",
            "desktop_login",
            "desktop_parked_login",
            "desktop_build",
        ] {
            assert!(codes.contains(&code), "{code}: {codes:?}");
        }
        assert!(
            doctor
                .checks
                .iter()
                .filter(|c| c.code.starts_with("desktop_"))
                .all(|c| c.level == doctor::Level::Ok),
            "{:?}",
            doctor
                .checks
                .iter()
                .map(|c| (c.code, &c.detail))
                .collect::<Vec<_>>()
        );
    }

    // Claude's settings there and unreadable, which a read of them would trip over.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
    )]
    fn a_mac_with_no_claude_desktop_account_reads_nothing_of_claude() {
        let plain = machine("no-desktop-plain");
        let m = machine("no-desktop");
        let scratch = m.ctx_home().join("claude");
        std::fs::create_dir_all(scratch.join("data")).expect("a data folder");
        std::fs::write(scratch.join("data/config.json"), "{").expect("settings");
        crate::host::fs::testing::deny_reading(&scratch.join("data/config.json")).expect("000");
        let read = |m: &Rig, ctx: Context| {
            let pitboard = Pitboard::new(ctx);
            let codes = |done: Done<status::Report>| {
                let warnings: Vec<&str> = done.warnings.iter().map(Warning::code).collect();
                (done.value.rows.len(), format!("{warnings:?}"))
            };
            let doctor: Vec<&str> = pitboard.doctor().checks.iter().map(|c| c.code).collect();
            assert_eq!(m.mem.listings(), 0);
            assert!(m.mem.jars_asked().is_empty());
            (
                codes(pitboard.status(true).expect("read")),
                codes(pitboard.status_offline().expect("read")),
                format!("{doctor:?}"),
            )
        };
        let with_claude = crate::fault::killing("settings.read", || {
            read(&m, m.ctx.clone().with_scratch_claude(scratch))
        })
        .expect("no read opened Claude's settings");
        assert_eq!(
            with_claude,
            read(&plain, plain.ctx.clone()),
            "as with no Claude at all"
        );
        assert!(!with_claude.2.contains("desktop_"), "{}", with_claude.2);
    }

    // Forgetting the one account leaves a record of nobody in use, which opts nothing in.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
    )]
    fn forgetting_the_last_claude_desktop_account_stops_every_read_of_claude() {
        let m = switch::harness::desktop_machine("forgot-last");
        let claude = m.claude();
        claude.opens();
        claude.logs_out();
        claude.quits();
        let pitboard = Pitboard::new(m.ctx.clone());
        pitboard.forget("desktop/work").expect("forgotten");
        claude.signed_in_as(&switch::harness::STRANGER);
        let state = state::load(&m.ctx).expect("state");
        assert!(state.accounts.is_empty());
        assert!(state.in_use(ProviderId::Desktop).is_some(), "{state:?}");

        let (listings, jars) = (m.mem.listings(), m.mem.jars_asked().len());
        let said = crate::fault::killing("settings.read", || {
            let codes = |done: Done<status::Report>| -> Vec<&'static str> {
                done.warnings.iter().map(Warning::code).collect()
            };
            let doctor: Vec<&str> = pitboard
                .doctor()
                .checks
                .iter()
                .map(|c| c.code)
                .filter(|code| code.starts_with("desktop_"))
                .collect();
            (
                codes(pitboard.status(true).expect("read")),
                codes(pitboard.status_offline().expect("read")),
                doctor,
            )
        })
        .expect("no read opened Claude's settings");
        assert_eq!(said, (Vec::new(), Vec::new(), Vec::new()));
        assert_eq!(m.mem.listings(), listings);
        assert_eq!(m.mem.jars_asked().len(), jars);
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
    )]
    fn what_claudes_settings_say_since_is_a_warning_of_every_read() {
        let m = switch::harness::desktop_machine("standing");
        let pitboard = Pitboard::new(m.ctx.clone());
        let settings = m.ctx_home().join("claude/data/config.json");
        let said = |pitboard: &Pitboard| -> Vec<String> {
            pitboard
                .status_offline()
                .expect("read")
                .warnings
                .iter()
                .map(|w| format!("{}: {w}", w.code()))
                .collect()
        };
        assert!(said(&pitboard).is_empty());

        let written = std::fs::read_to_string(&settings).expect("settings");
        std::fs::write(
            &settings,
            written.replace(
                switch::harness::WORK.uuid,
                "0b7e2c44-2222-4d1f-8a35-91c0de77aa01",
            ),
        )
        .expect("renamed");
        let warned = said(&pitboard);
        assert_eq!(warned.len(), 1, "{warned:?}");
        assert!(
            warned[0].starts_with(
                "in_use_unconfirmed: Claude's settings have named an account Pitboard has not \
                 enrolled since Pitboard last read `desktop/work` in use, so Claude may hold \
                 another login. Open Claude,"
            ),
            "{warned:?}"
        );

        let emptied: String = written
            .lines()
            .map(|line| match line.split_once(": ") {
                Some((key, _)) if key.contains("oauth:tokenCache") => {
                    format!("{key}: {},\n", crate::desktop_testing::EMPTIED)
                }
                _ => format!("{line}\n"),
            })
            .collect();
        std::fs::write(&settings, emptied).expect("emptied");
        let warned = said(&pitboard);
        assert_eq!(
            warned,
            [
                "looks_signed_out: Claude looks signed out since Pitboard last read `desktop/work` \
              in use. The next change to a Claude Desktop account checks."
            ]
        );
        let mut kept: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&settings).expect("settings"))
                .expect("JSON");
        kept.insert(
            "custom3pMcpOAuth".into(),
            serde_json::json!({"grant": "kept"}),
        );
        std::fs::write(
            &settings,
            serde_json::to_string_pretty(&kept).expect("JSON"),
        )
        .expect("grants");
        assert_eq!(
            said(&pitboard),
            warned,
            "a Log out leaves the connector grants"
        );
        assert_eq!(m.mem.listings(), 1, "the enrolment's look, and no read's");
    }

    #[test]
    fn a_folder_set_aside_is_said_with_its_account_or_with_no_name() {
        use state::SetAside;
        let said = |why, label: Option<&str>| {
            Warning::SetAside {
                tool: ProviderId::Desktop,
                path: "/p/parks/x".into(),
                label: label.map(str::to_owned),
                why,
            }
            .to_string()
        };
        assert_eq!(
            said(SetAside::Superseded, Some("desktop/home")),
            "Pitboard kept `desktop/home`'s earlier parked login in /p/parks/x and deleted \
             nothing. `pitboard doctor` lists it."
        );
        assert_eq!(
            said(SetAside::Superseded, None),
            "Pitboard kept an earlier parked login in /p/parks/x and deleted nothing. `pitboard \
             doctor` lists it."
        );
        assert_eq!(
            said(SetAside::GivenUp, Some("desktop/work")),
            "Pitboard moved the parts of `desktop/work`'s login left in Claude's folder into \
             /p/parks/x and deleted nothing. `pitboard doctor` lists them."
        );
        assert_eq!(
            said(SetAside::GivenUp, None),
            "Pitboard moved the parts of a login left in Claude's folder into /p/parks/x and \
             deleted nothing. `pitboard doctor` lists them."
        );
        for why in [
            SetAside::InTheWay,
            SetAside::AnotherMachine,
            SetAside::Orphan,
        ] {
            for label in [None, Some("desktop/work")] {
                assert_eq!(
                    said(why, label),
                    "Pitboard moved what it found in the way into /p/parks/x and deleted nothing."
                );
            }
        }
        let keys = |label: Option<&str>| Warning::KeysSetAside {
            tool: ProviderId::Desktop,
            path: "/p/parks/x".into(),
            label: label.map(str::to_owned),
        };
        assert_eq!(keys(None).code(), "set_aside");
        assert_eq!(
            keys(Some("desktop/home")).to_string(),
            "Claude rewrote `desktop/home`'s keys. Pitboard kept the earlier ones in /p/parks/x \
             and deleted nothing. `pitboard doctor` lists them."
        );
        assert_eq!(
            keys(None).to_string(),
            "Claude rewrote a login's keys. Pitboard kept the earlier ones in /p/parks/x and \
             deleted nothing. `pitboard doctor` lists them."
        );
    }

    // What a change did before it refused leads, with a journal recovery, which ran before
    // it; what it only found follows.
    #[test]
    fn a_refused_change_counts_what_it_did_first() {
        let found = || Warning::LooksSignedOut {
            tool: ProviderId::Claude,
            label: "work".into(),
        };
        let refusal = || Error::AccountNotInUse {
            tool: ProviderId::Desktop,
            label: "desktop/work".into(),
        };
        let waiting = || Warning::MoveWaiting {
            tool: ProviderId::Desktop,
            label: "desktop/work".into(),
        };
        let recovered = || {
            Warning::Recovered(Recovered {
                from: "home".into(),
                to: "work".into(),
                finished: true,
            })
        };
        let codes = |failed: &Failed| -> Vec<&'static str> {
            failed.warnings.iter().map(Warning::code).collect()
        };
        let failed = Failed::of(refusal().after(vec![waiting()]), vec![found()]);
        assert_eq!(codes(&failed), ["move_waiting", "looks_signed_out"]);
        assert_eq!(failed.said_first, 1);
        assert_eq!(failed.error.code(), "account_not_in_use");
        let failed = Failed::of(refusal(), vec![found()]);
        assert_eq!(codes(&failed), ["looks_signed_out"]);
        assert_eq!(failed.said_first, 0);
        let failed = Failed::of(refusal(), vec![found(), recovered()]);
        assert_eq!(
            codes(&failed),
            ["interrupted_switch_finished", "looks_signed_out"]
        );
        assert_eq!(failed.said_first, 1);
        let failed = Failed::of(refusal().after(vec![waiting()]), vec![found(), recovered()]);
        assert_eq!(
            codes(&failed),
            [
                "interrupted_switch_finished",
                "move_waiting",
                "looks_signed_out"
            ]
        );
        assert_eq!(failed.said_first, 2);
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
    )]
    fn what_holds_claude_desktops_login_is_said() {
        let m = switch::harness::desktop_machine("holding");
        let pitboard = Pitboard::new(m.ctx.clone());
        let claude = m.claude();
        assert!(pitboard.holding(ProviderId::Desktop).is_empty());
        claude.opens();
        let holding = pitboard.holding(ProviderId::Desktop);
        assert_eq!(holding.len(), 1, "{holding:?}");
        assert_eq!(holding[0].holder.kind, "claude_desktop");
        assert_eq!(holding[0].pids.len(), 2, "its own program and a helper");
        claude.crashes();
        assert!(pitboard.holding(ProviderId::Desktop).is_empty());
        m.mem.without_a_process_list();
        assert!(matches!(
            switch::still_holding(&m.ctx, ProviderId::Desktop),
            switch::StillHolding::Unknown
        ));
        assert!(pitboard.holding(ProviderId::Desktop).is_empty());
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
    )]
    fn a_desktop_row_says_no_usage_and_asks_nobody() {
        use crate::status::Stale;
        let m = switch::harness::desktop_machine("rows");
        let now = switch::harness::NOW;
        let mut state = state::load(&m.ctx).expect("state");
        let another = |label: &str, parked, replaced_at| Account {
            label: label.into(),
            id: format!("desktop_{label}"),
            account_uuid: label.into(),
            email: String::new(),
            parked,
            last_used_at: None,
            replaced_at,
            plan: None,
            detail: state::Detail::Desktop {},
        };
        let park = state::Park {
            service: "pitboard-park-desktop_home-1".into(),
            kind: state::ParkKind::Folder,
            parked_at: now,
            login: "0a1b2c3d4e5f6071".into(),
            access_expires_at: None,
            refresh_expires_at: None,
            session_expires_at: Some(now + 20 * 86_400),
        };
        state.accounts.push(another("home", Some(park), None));
        state.accounts.push(another("gone", None, Some(now - 5)));
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");

        let pitboard = Pitboard::new(m.ctx.clone());
        for report in [
            pitboard.status(true).expect("read").value,
            pitboard.status_offline().expect("read").value,
        ] {
            let row = |label: &str| {
                report
                    .rows
                    .iter()
                    .find(|row| {
                        row.provider == ProviderId::Desktop && row.label.as_deref() == Some(label)
                    })
                    .unwrap_or_else(|| panic!("a row for {label}"))
            };
            let (work, home, gone) = (row("work"), row("home"), row("gone"));
            assert!(work.signed_in && !home.signed_in && !gone.signed_in);
            assert_eq!(work.read_at, Some(now));
            assert_eq!((home.read_at, gone.read_at), (None, None));
            for row in [work, home] {
                assert_eq!(row.stale, Some(Stale::NoUsage));
                assert!(row.usage.is_none() && row.plan.is_none() && row.lasts(now).is_none());
                assert_eq!(row.email, "");
                assert_eq!(row.organization_uuid, None);
            }
            assert!(home.switchable(now), "no deadline is known");
            assert_eq!(gone.stale, Some(Stale::LoginReplaced));
            assert_eq!(
                gone.explanation().as_deref(),
                Some("its login was signed out or replaced in Claude; sign in again")
            );
            assert_eq!(
                Stale::NoUsage
                    .explanation_for(ProviderId::Desktop, "Claude")
                    .as_deref(),
                Some("Pitboard reads no usage for Claude Desktop")
            );
            assert_eq!(
                Stale::Unreachable.explanation_for(ProviderId::Desktop, "Claude"),
                None
            );
        }
        assert!(m.api.asked().is_empty(), "{:?}", m.api.asked());
        assert!(
            m.mem
                .trail()
                .iter()
                .all(|touch| !touch.service.starts_with("pitboard-park-desktop_")),
            "{:?}",
            m.mem.trail()
        );
        assert!(
            m.mem.jars_asked().len() == 1 && m.mem.listings() == 1,
            "the enrolment's own"
        );
    }

    // The app took names with a space before a new name had to be one word.
    #[test]
    fn an_account_named_before_names_were_one_word_keeps_its_name() {
        let m = machine("one-word");
        let mut state = state::load(&m.ctx).expect("state");
        state
            .accounts
            .iter_mut()
            .find(|a| a.label == "there")
            .expect("there")
            .label = "over there".into();
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        let pitboard = Pitboard::new(m.ctx.clone());

        assert_eq!(
            pitboard.account_to_enroll("over there").map(|a| a.label),
            Some("over there".to_owned())
        );
        let refused = pitboard
            .enroll_current("new one")
            .expect_err("a new name of two words");
        assert_eq!(refused.error.code(), "usage", "{}", refused.error);
        assert_eq!(
            audit_lines(&m, "enroll"),
            [("new one".to_owned(), "label_unusable".to_owned())]
        );
    }

    // An existing account's name keeps it, and a rename gives a new one by the rules all the same.
    #[test]
    fn a_rename_to_a_name_another_tool_has_follows_the_rules_for_a_new_one() {
        let m = codex_machine("rename-one-word");
        let mut state = state::load(&m.ctx).expect("state");
        state
            .accounts
            .push(switch::harness::account("my work", "elsewhere", None));
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        let pitboard = Pitboard::new(m.ctx.clone());

        let refused = pitboard
            .rename("codex/there", "my work")
            .expect_err("a name of two words");
        assert_eq!(refused.error.code(), "usage", "{}", refused.error);
        assert!(
            state::load(&m.ctx)
                .expect("state")
                .get(&m.key("there"))
                .is_some(),
            "the Codex account keeps its name"
        );
    }

    type Refuse = fn(&Pitboard, ProviderId) -> Option<Failed>;

    /// Every way a change is refused over the name it was given, with the verb the audit
    /// log records it under, the code it is refused with and the one the log records: a
    /// name nobody enrolled, a name no account could have, whether the account signed in
    /// now is being enrolled or one is being signed in to, and a new name that would move
    /// an account to another tool.
    const REFUSALS: [(&str, &str, &str, &str, Refuse); 4] = [
        (
            "use",
            "use",
            "account_unknown",
            "account_unknown",
            |p, _| p.switch_to("nobody").err(),
        ),
        ("enroll", "enroll", "usage", "label_unusable", |p, _| {
            p.enroll_current("codx/work").err()
        }),
        ("sign-in", "enroll", "usage", "label_unusable", |p, _| {
            p.sign_in("codx/work").err()
        }),
        ("rename", "rename", "usage", "usage", |p, tool| {
            let other = if tool == ProviderId::Claude {
                ProviderId::Codex
            } else {
                ProviderId::Claude
            };
            p.rename(
                &Key::new(tool, "here").qualified(),
                &Key::new(other, "moved").qualified(),
            )
            .err()
        }),
    ];

    /// A switch from `here` to `there` killed after parking `here` and before installing
    /// `there`, so its record is all that says it happened.
    fn interrupted(make: Make, name: &str) -> Rig {
        let m = make(name);
        let settled = switch::settle(&m.ctx, Permit::for_a_test(), None)
            .expect("nothing to recover yet")
            .0;
        let died = crate::fault::killing("switch.park_recorded", || {
            switch::switch(settled, &m.key("there"))
        });
        assert_eq!(died.unwrap_err(), "switch.park_recorded");
        assert!(switch::interrupted(&m.ctx));
        m
    }

    /// The same switch, which the next change cannot finish: nothing is signed in to the
    /// tool any more, so nothing says which side of the switch won. Told apart without
    /// asking anyone, for either tool.
    fn stuck(make: Make, name: &str) -> Rig {
        let m = interrupted(make, name);
        m.fault_live(crate::store::memory::Fault::Vanish);
        m
    }

    /// The switch interrupted on a Claude Code machine, which the next change cannot finish
    /// either, though only Anthropic could tell: Claude Code has since renewed the login, so
    /// it matches neither side the record kept, and its session has expired, so Anthropic
    /// will not say whose it is. An access token the scripted service does not know is one
    /// it refuses.
    fn stuck_unless_asked(name: &str) -> Rig {
        let m = interrupted(machine, name);
        m.sign_in(&switch::harness::document("renewed-since"));
        m
    }

    /// The warnings a read gives, by code and words.
    fn said(read: &Done<status::Report>) -> Vec<(&'static str, String)> {
        read.warnings
            .iter()
            .map(|w| (w.code(), w.to_string()))
            .collect()
    }

    /// Everything a read must leave as it found it: Pitboard's files but the records a
    /// live read keeps by design (what it measured and when to ask again), every parked
    /// login and the live one.
    type Untouched = (
        BTreeMap<String, Vec<u8>>,
        Vec<(String, Option<String>)>,
        Option<serde_json::Value>,
    );

    fn untouched(m: &Rig) -> Untouched {
        let kept_by_a_read = ["usage.json", "usage.lock", "asking.json"];
        let mut files = files(m);
        files.retain(|path, _| !kept_by_a_read.iter().any(|kept| path.ends_with(kept)));
        let vault = m.mem.vault();
        let parked = vault
            .services()
            .into_iter()
            .map(|service| {
                let held = vault.peek(&service);
                (service, held)
            })
            .collect();
        (files, parked, m.live())
    }

    /// A switch says when Claude Code keeps another login in its fallback file, which a
    /// session that cannot read the keychain signs in with and no switch reaches. Codex has
    /// no store behind the one in use, so nothing is said of it.
    #[test]
    fn a_switch_says_a_login_is_left_where_it_does_not_reach() {
        let m = machine("switch-fallback-login");
        let left = plant_behind(
            &m,
            &switch::harness::document("left-by-a-sign-in").to_string(),
        );
        let switched = Pitboard::new(m.ctx.clone())
            .switch_to("there")
            .expect("a switch");
        let said: Vec<String> = switched
            .warnings
            .iter()
            .filter(|w| w.code() == "fallback_login")
            .map(ToString::to_string)
            .collect();
        assert_eq!(said.len(), 1, "{:?}", switched.warnings);
        assert!(
            said[0].starts_with(&left.display().to_string()),
            "{}",
            said[0]
        );

        for (tool, make) in MACHINES {
            let m = make(&format!("switch-nothing-left-{tool}"));
            let switched = Pitboard::new(m.ctx.clone())
                .switch_to(&m.key("there").typed())
                .expect("a switch");
            assert!(
                switched
                    .warnings
                    .iter()
                    .all(|w| w.code() != "fallback_login"),
                "{tool}: {:?}",
                switched.warnings
            );
        }
    }

    /// Plants `contents` in Claude Code's `.credentials.json` behind the keychain, as a
    /// sign-in where the keychain could not be read leaves it, and gives back its path.
    fn plant_behind(m: &Rig, contents: &str) -> std::path::PathBuf {
        let left = crate::provider::claude::live::credential_file(&m.ctx);
        m.mem.file_at(left.clone()).plant(
            &crate::provider::claude::paths::live_service(&m.ctx),
            contents,
        );
        left
    }

    /// While a file sits behind the keychain, running sessions keep their account after a
    /// switch until their login is next renewed, so a read says it is there, as a change
    /// does, and not only somebody about to switch hears of it. A read that asks nobody
    /// reads no keychain, and says nothing of what is behind one. Codex has no store behind
    /// its own.
    #[test]
    fn a_read_says_a_file_is_behind_the_keychain() {
        let m = machine("read-fallback-login");
        let left = plant_behind(
            &m,
            &switch::harness::document("left-by-a-sign-in").to_string(),
        );
        let pitboard = Pitboard::new(m.ctx.clone());

        let read = pitboard.status(false).expect("a read");
        let said: Vec<String> = read
            .warnings
            .iter()
            .filter(|w| w.code() == "fallback_login")
            .map(ToString::to_string)
            .collect();
        assert_eq!(said.len(), 1, "{:?}", read.warnings);
        assert!(
            said[0].starts_with(&format!(
                "{} holds another Claude Code login",
                left.display()
            )),
            "{}",
            said[0]
        );
        assert!(
            said[0].contains(
                "sessions already running at a switch keep the account they are on until \
                 their login is next renewed, or until they are started again"
            ),
            "{}",
            said[0]
        );
        assert!(
            said[0].ends_with(
                "`pitboard stow` keeps that login for its account, where Pitboard holds no \
                 other it can switch to, then deletes the file."
            ),
            "{}",
            said[0]
        );
        let offline = pitboard.status_offline().expect("a read of what is known");
        assert!(
            offline
                .warnings
                .iter()
                .all(|w| w.code() != "fallback_login"),
            "{:?}",
            offline.warnings
        );

        for (tool, make) in MACHINES {
            let m = make(&format!("read-nothing-left-{tool}"));
            let read = Pitboard::new(m.ctx.clone()).status(false).expect("a read");
            assert!(
                read.warnings.iter().all(|w| w.code() != "fallback_login"),
                "{tool}: {:?}",
                read.warnings
            );
        }
    }

    /// A running session watches the file by its being there, whatever it holds, so one
    /// with no login in it holds sessions to their account all the same, and is said too:
    /// in words that say so, and with what lets them follow.
    #[test]
    fn an_empty_file_behind_the_keychain_is_said_too() {
        let m = machine("read-empty-fallback");
        let left = plant_behind(&m, "{}");
        let pitboard = Pitboard::new(m.ctx.clone());

        for warnings in [
            pitboard.status(false).expect("a read").warnings,
            pitboard.switch_to("there").expect("a switch").warnings,
        ] {
            let said: Vec<String> = warnings
                .iter()
                .filter(|w| w.code() == "fallback_login")
                .map(ToString::to_string)
                .collect();
            assert_eq!(
                said,
                [format!(
                    "{path} is there with no Claude Code login in it. While it is, Claude \
                     Code sessions already running at a switch keep the account they are on \
                     until their login is next renewed, or until they are started again. \
                     `pitboard stow` deletes it, which lets them follow a switch.",
                    path = left.display()
                )],
                "{warnings:?}"
            );
        }
    }

    /// A running session looks at the file and never reads it to decide, so one Pitboard
    /// cannot read, as one that is not text or that this user may not read, holds sessions
    /// to their account all the same. It is said, in words that do not guess what it holds.
    #[test]
    fn a_file_behind_the_keychain_that_cannot_be_read_is_said_too() {
        let m = machine("read-unreadable-fallback");
        let left = plant_behind(&m, "{}");
        m.mem.file_at(left.clone()).fault(
            &crate::provider::claude::paths::live_service(&m.ctx),
            crate::store::memory::Fault::UnreadableContents(
                "stream did not contain valid UTF-8".into(),
            ),
        );
        let pitboard = Pitboard::new(m.ctx.clone());

        for warnings in [
            pitboard.status(false).expect("a read").warnings,
            pitboard.switch_to("there").expect("a switch").warnings,
        ] {
            let said: Vec<String> = warnings
                .iter()
                .filter(|w| w.code() == "fallback_login")
                .map(ToString::to_string)
                .collect();
            assert_eq!(
                said,
                [format!(
                    "{} is there, and Pitboard could not read it. While it is, Claude Code \
                     sessions already running at a switch keep the account they are on until \
                     their login is next renewed, or until they are started again. `pitboard \
                     doctor` says why, and what to do.",
                    left.display()
                )],
                "{warnings:?}"
            );
        }
    }

    /// A switch nothing can finish was said only by a change refused over it. The app looks
    /// for it in what a read says, so it never offered to give up on one, and `pitboard
    /// status` never said it at all. A read says it now, in the words of the refusal, and
    /// changes nothing: the record is kept for whatever finishes it or gives up on it.
    #[test]
    fn a_read_says_an_interrupted_switch_it_cannot_finish_is_waiting() {
        for (tool, make) in MACHINES {
            let m = stuck(make, &format!("read-stuck-{tool}"));
            let pitboard = Pitboard::new(m.ctx.clone());
            let before = untouched(&m);

            let read = pitboard
                .status(false)
                .expect("a read is not refused over it");

            assert_eq!(untouched(&m), before, "{tool}: a read changes nothing");
            assert!(switch::interrupted(&m.ctx), "{tool}: the record is kept");
            let refused = pitboard
                .switch_to(&m.key("there").typed())
                .expect_err("every change stops at it");
            assert_eq!(refused.error.code(), "recovery_undetermined", "{tool}");
            assert_eq!(
                said(&read),
                [("recovery_undetermined", refused.error.to_string())],
                "{tool}: said in the refusal's own words"
            );
        }
    }

    #[test]
    fn giving_up_on_a_switch_nothing_can_finish_happens_once() {
        for (tool, make) in MACHINES {
            let m = stuck(make, &format!("give-up-once-{tool}"));
            let pitboard = Pitboard::new(m.ctx.clone());

            let gave = pitboard.abandon_recovery().expect("given up");
            let abandoned = gave.switch.expect("the switch given up");
            assert_eq!(
                (abandoned.from, abandoned.to),
                (m.key("here").typed(), m.key("there").typed())
            );
            let again = pitboard.abandon_recovery().expect("nothing to give up");
            assert!(
                again.switch.is_none() && again.moves.is_empty(),
                "{tool}: {again:?}"
            );

            let read = pitboard.status(false).expect("a read");
            assert_eq!(said(&read), [], "{tool}: nothing waits");
            let changed = pitboard.switch_to(&m.key("there").typed());
            assert!(
                changed
                    .as_ref()
                    .err()
                    .is_none_or(|refused| refused.error.code() != "recovery_undetermined"),
                "{tool}: {:?}",
                changed.err().map(|refused| refused.error)
            );
        }
    }

    #[test]
    fn each_change_is_logged_newest_last_as_the_command_line_types_it() {
        for (tool, make) in MACHINES {
            let m = make(&format!("logged-{tool}"));
            let pitboard = Pitboard::new(m.ctx.clone().with_caller("app".into()));

            pitboard
                .switch_to(&m.key("there").typed())
                .expect("a switch");
            pitboard
                .rename(&m.key("here").typed(), "office")
                .expect("renamed");

            let log = pitboard.log(2);
            let (here, there) = (m.key("here"), m.key("there"));
            assert_eq!(
                log.iter()
                    .map(|change| (
                        change.caller.as_str(),
                        change.verb.as_str(),
                        change.subject.clone(),
                        change.outcome.as_str()
                    ))
                    .collect::<Vec<_>>(),
                [
                    ("app", "use", there.typed(), "ok"),
                    ("app", "rename", format!("{here} -> office"), "ok"),
                ],
                "{tool}"
            );
            assert_eq!(pitboard.log(1), log[1..], "{tool}");
        }
    }

    /// Where only the service can say whose the live login is, a read asks it, as the next
    /// change would.
    #[test]
    fn a_read_asks_whose_the_login_is_where_only_the_service_can_say() {
        let m = stuck_unless_asked("read-stuck-asked");
        let pitboard = Pitboard::new(m.ctx.clone());
        let before = untouched(&m);

        let read = pitboard.status(false).expect("a read");

        assert_eq!(untouched(&m), before, "a read changes nothing");
        let refused = pitboard.switch_to("there").expect_err("it stops at it");
        assert_eq!(
            said(&read),
            [("recovery_undetermined", refused.error.to_string())]
        );
        // Nothing tried to renew the login here, so nothing says it cannot be renewed.
        let said = refused.error.to_string();
        assert!(said.contains("session has expired"), "{said}");
        assert!(!said.contains("sign in"), "{said}");
    }

    /// A switch the next change finishes by itself is not one anybody has to do anything
    /// about. The rows already say who is signed in, from the tool's login and not from
    /// Pitboard's record, and the change that finishes it says what it found.
    #[test]
    fn a_read_says_nothing_of_a_switch_the_next_change_finishes() {
        for (tool, make) in MACHINES {
            let m = interrupted(make, &format!("read-settles-{tool}"));
            let before = untouched(&m);

            let read = Pitboard::new(m.ctx.clone()).status(false).expect("a read");

            assert_eq!(said(&read), [], "{tool}");
            assert_eq!(untouched(&m), before, "{tool}: a read finishes nothing");
            assert!(switch::interrupted(&m.ctx), "{tool}");
        }
    }

    /// The read never says a switch is waiting that the next change would not stop at for
    /// that reason, and never offers to give up on one that giving up cannot reach. Under a
    /// custom Claude Code endpoint every change is refused over the endpoint before it comes
    /// to a Claude Code switch, and giving up on one is refused too. A Codex switch is
    /// settled under one as anywhere else, and given up on as anywhere else: giving up
    /// refused it, so the read offered a way out that could not be taken.
    #[test]
    fn a_read_says_a_switch_is_waiting_only_where_a_change_would_stop_at_it() {
        for (tool, make) in MACHINES {
            let m = stuck(make, &format!("read-stuck-custom-{tool}"));
            let mut ctx = m.ctx.clone();
            ctx.custom_oauth = true;
            let pitboard = Pitboard::new(ctx);

            let read = pitboard.status(false).expect("a read");
            let known = pitboard.status_offline().expect("a read");
            let refused = pitboard
                .switch_to(&m.key("there").typed())
                .expect_err("refused");

            let codes: Vec<&str> = read.warnings.iter().map(Warning::code).collect();
            let expected: &[&str] = match Tool::of(m.which) {
                Tool::Claude => &[],
                Tool::Codex => &["recovery_undetermined"],
            };
            assert_eq!(codes, expected, "{tool}: {}", refused.error);
            let codes: Vec<&str> = known.warnings.iter().map(Warning::code).collect();
            assert_eq!(codes, expected, "{tool}: the read that asks nobody too");
            assert_eq!(
                refused.error.code() == "recovery_undetermined",
                !expected.is_empty(),
                "{tool}"
            );

            let gave_up = pitboard.abandon_recovery();
            match Tool::of(m.which) {
                Tool::Claude => assert_eq!(
                    gave_up.expect_err("refused").code(),
                    "custom_oauth_endpoint",
                    "{tool}"
                ),
                Tool::Codex => {
                    assert!(gave_up.expect("given up").switch.is_some(), "{tool}");
                    assert!(!switch::interrupted(&m.ctx), "{tool}: nothing waits");
                }
            }
        }
    }

    /// Every read says it, the one that asks nobody too: what an app reads each time the
    /// account index moves, and what `pitboard status --offline` prints. It sends no request,
    /// so it says it wherever the record and the logins tell it without one, in the words of
    /// the refusal, and changes nothing. Where only Anthropic could say whose the login is,
    /// it does not say what it cannot know.
    #[test]
    fn a_read_that_asks_nobody_says_a_switch_is_waiting_wherever_it_can_tell() {
        for (tool, make) in MACHINES {
            let m = stuck(make, &format!("known-stuck-{tool}"));
            let pitboard = Pitboard::new(m.ctx.clone());
            let (before, asked) = (untouched(&m), m.api.calls());

            let read = pitboard.status_offline().expect("a read");

            assert_eq!(untouched(&m), before, "{tool}: a read changes nothing");
            assert_eq!(m.api.calls(), asked, "{tool}: and asks nobody");
            assert!(switch::interrupted(&m.ctx), "{tool}: the record is kept");
            let refused = pitboard
                .switch_to(&m.key("there").typed())
                .expect_err("every change stops at it");
            assert_eq!(
                said(&read),
                [("recovery_undetermined", refused.error.to_string())],
                "{tool}"
            );
        }

        let m = stuck_unless_asked("known-stuck-unasked");
        let asked = m.api.calls();
        let read = Pitboard::new(m.ctx.clone())
            .status_offline()
            .expect("a read");
        assert_eq!(m.api.calls(), asked, "asks nobody");
        assert_eq!(said(&read), [], "only Anthropic could tell");
    }

    /// Claude Code 2.1.294 stamps its usage cache with the account its config names, and
    /// asks with the login its session holds, which can still be the account switched away
    /// from. Four of five caches captured on one machine were another login's numbers, so
    /// no read takes a reading from it, whether or not Anthropic answered this time.
    #[test]
    fn what_claude_code_cached_is_never_an_accounts_reading() {
        use crate::switch::harness::{NOW, cache_usage, usage_answer};
        let m = machine("cached-usage");
        readings::answered(
            &m.ctx,
            Permit::for_a_test(),
            &[(
                "here".into(),
                crate::usage::from_usage_object(&usage_answer(10.0), NOW),
            )],
        );
        cache_usage(&m, usage_answer(100.0));
        let pitboard = Pitboard::new(m.ctx.clone());
        for (how, read) in [
            ("offline", pitboard.status_offline()),
            ("online", pitboard.status(false)),
        ] {
            let read = read.expect("a read");
            let here = read
                .value
                .rows
                .iter()
                .find(|row| row.label.as_deref() == Some("here"))
                .expect("here's row");
            let shares: Vec<f64> = here
                .usage
                .iter()
                .flat_map(|usage| &usage.windows)
                .map(|window| window.percent)
                .collect();
            assert_eq!(shares, [10.0], "{how}");
        }
    }

    /// A login Anthropic has named is known by its refresh token's fingerprint from then on,
    /// so a read asks whose it is only once the login has changed. It asked on every read,
    /// one round trip more each time for an answer it already had.
    #[test]
    fn a_login_already_identified_is_not_asked_about_again() {
        use crate::api::scripted::Asked;
        let m = machine("read-known-login");

        Pitboard::new(m.ctx.clone()).status(false).expect("a read");

        let asked = m.api.asked();
        assert!(
            !asked.contains(&Asked::Owner("access-here-refresh".into())),
            "{asked:?}"
        );
        assert!(
            asked.contains(&Asked::Usage("access-here-refresh".into())),
            "what it has left is still asked: {asked:?}"
        );
    }

    /// A sign-in outside Pitboard replaced the only login of the account in use. The first
    /// read records it and the activity log says so once; every read after, the one that
    /// asks nobody too, says it until that account is signed in again or forgotten, and its
    /// row says why it has nothing parked. It read as `nothing parked`, and nothing said what
    /// had happened.
    #[test]
    fn a_sign_in_outside_pitboard_is_said_recorded_and_shown_on_its_row() {
        use crate::switch::harness::NOW;
        let m = signed_in_outside("read-signed-in-outside");
        let pitboard = Pitboard::new(m.ctx.clone());
        let replaced = "`here`'s login was replaced by a sign-in outside Pitboard: Claude Code \
                        now has `elsewhere`'s login stored, and Pitboard holds no login for \
                        `here`. Run `pitboard enroll here --sign-in` to sign in to it again.";
        let row = |read: &Done<status::Report>, label: &str| {
            read.value
                .rows
                .iter()
                .find(|row| row.label.as_deref() == Some(label))
                .map(|row| (row.signed_in, row.stale))
        };

        let read = pitboard.status(false).expect("a read");

        let state = state::load(&m.ctx).expect("state");
        assert_eq!(
            state
                .account_in_use(ProviderId::Claude)
                .map(|account| account.label.as_str()),
            Some("elsewhere")
        );
        let account = |label: &str| state.get(&m.key(label)).expect("enrolled");
        assert_eq!(account("here").replaced_at, Some(NOW));
        assert_eq!(account("elsewhere").last_used_at, Some(NOW));
        assert!(account("there").parked.is_some(), "`there` keeps its park");
        assert_eq!(
            row(&read, "elsewhere").map(|(signed_in, _)| signed_in),
            Some(true)
        );
        assert_eq!(
            row(&read, "here"),
            Some((false, Some(status::Stale::LoginReplaced)))
        );
        assert_eq!(said(&read), [("login_replaced", replaced.to_string())]);
        let noticed = [
            ("elsewhere".to_string(), "signed_in_outside".to_string()),
            ("here".to_string(), "login_replaced".to_string()),
        ];
        assert_eq!(audit_lines(&m, "in-use"), noticed);

        let recorded = state_file(&m);
        let again = pitboard.status(false).expect("a second read");
        assert_eq!(said(&again), [("login_replaced", replaced.to_string())]);
        assert_eq!(audit_lines(&m, "in-use"), noticed, "recorded once");
        assert_eq!(state_file(&m), recorded, "and written once");

        let offline = pitboard.status_offline().expect("a read of what is known");
        assert_eq!(said(&offline), [("login_replaced", replaced.to_string())]);
    }

    /// A read that finds what the record already says writes nothing, so several front ends
    /// reading one machine never take turns rewriting its account index.
    #[test]
    fn a_read_that_finds_nothing_new_writes_nothing() {
        let m = machine("read-nothing-new");
        let pitboard = Pitboard::new(m.ctx.clone());
        let before = state_file(&m);

        pitboard.status(false).expect("a read");
        pitboard.status(false).expect("another read");

        assert_eq!(state_file(&m), before);
        assert!(audit_lines(&m, "in-use").is_empty());
    }

    /// Each `CLAUDE_CONFIG_DIR` has a login of its own stored, and Pitboard keeps whose for
    /// each. A read under another one leaves this one's record, so a sign-in outside Pitboard
    /// here is still found by the next read here, and reads taking turns under the two write
    /// nothing once each has its record.
    #[test]
    fn a_read_under_another_claude_config_dir_keeps_this_ones_record() {
        use crate::switch::harness::{NOW, account, document, owner};
        let m = machine("read-two-slots");
        m.api
            .owned_by("access-elsewhere-refresh", owner("elsewhere"));
        m.api.owned_by("access-there-other", owner("there"));
        let mut state = state::load(&m.ctx).expect("state");
        state.accounts.push(account("elsewhere", "elsewhere", None));
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        let other = m
            .ctx
            .clone()
            .with_claude_config_dir(m.ctx_home().join("other").to_string_lossy().into_owned());
        m.mem.live().plant(
            &crate::provider::claude::paths::live_service(&other),
            &document("there-other").to_string(),
        );

        Pitboard::new(other.clone())
            .status(false)
            .expect("a read under the other");
        m.sign_in(&document("elsewhere-refresh"));
        Pitboard::new(m.ctx.clone())
            .status(false)
            .expect("a read here");

        let state = state::load(&m.ctx).expect("state");
        assert_eq!(
            state.get(&m.key("here")).expect("enrolled").replaced_at,
            Some(NOW)
        );
        assert_eq!(
            audit_lines(&m, "in-use"),
            [
                ("elsewhere".to_string(), "signed_in_outside".to_string()),
                ("here".to_string(), "login_replaced".to_string()),
            ]
        );

        let recorded = state_file(&m);
        for ctx in [&other, &m.ctx, &other] {
            Pitboard::new(ctx.clone()).status(false).expect("a read");
        }
        assert_eq!(state_file(&m), recorded, "reads taking turns write nothing");
    }

    /// `here` is signed in under two `CLAUDE_CONFIG_DIR`s, and a sign-in outside Pitboard
    /// replaces its login under one. Its login under the other is still stored, so no read
    /// says it was replaced, least of all one under the slot where it is in use. Once the
    /// other slot's is replaced too, it is.
    #[test]
    fn a_login_still_stored_under_another_claude_config_dir_is_not_said_replaced() {
        use crate::switch::harness::{NOW, account, document, owner};
        let m = machine("read-two-slots-one-account");
        m.api.owned_by("access-here-other", owner("here"));
        m.api
            .owned_by("access-elsewhere-refresh", owner("elsewhere"));
        let mut state = state::load(&m.ctx).expect("state");
        state.accounts.push(account("elsewhere", "elsewhere", None));
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        let other = m
            .ctx
            .clone()
            .with_claude_config_dir(m.ctx_home().join("other").to_string_lossy().into_owned());
        let plant_there = |login: &str| {
            m.mem.live().plant(
                &crate::provider::claude::paths::live_service(&other),
                &document(login).to_string(),
            );
        };
        let replaced = |read: &Done<status::Report>| {
            said(read)
                .into_iter()
                .filter(|(code, _)| *code == "login_replaced")
                .count()
        };

        plant_there("here-other");
        Pitboard::new(other.clone())
            .status(false)
            .expect("a read under the other");
        plant_there("elsewhere-refresh");
        let there = Pitboard::new(other.clone())
            .status(false)
            .expect("a read under the other after a sign-in there");
        let here = Pitboard::new(m.ctx.clone())
            .status(false)
            .expect("a read here");

        assert_eq!((replaced(&there), replaced(&here)), (0, 0));
        assert_eq!(
            state::load(&m.ctx)
                .expect("state")
                .get(&m.key("here"))
                .expect("enrolled")
                .replaced_at,
            None
        );
        assert_eq!(
            audit_lines(&m, "in-use"),
            [("elsewhere".to_string(), "signed_in_outside".to_string())]
        );

        m.sign_in(&document("elsewhere-refresh"));
        let gone = Pitboard::new(m.ctx.clone())
            .status(false)
            .expect("a read here after a sign-in here");

        assert_eq!(replaced(&gone), 1);
        assert_eq!(
            state::load(&m.ctx)
                .expect("state")
                .get(&m.key("here"))
                .expect("enrolled")
                .replaced_at,
            Some(NOW)
        );
        assert_eq!(
            audit_lines(&m, "in-use")[1..],
            [
                ("elsewhere".to_string(), "signed_in_outside".to_string()),
                ("here".to_string(), "login_replaced".to_string()),
            ]
        );
    }

    /// Claude Code's config names `here` and Pitboard finds no login where it reads: the
    /// login is somewhere Pitboard does not look, which is not a sign-out. A read records
    /// nothing, and says of no account that its login was replaced.
    #[test]
    fn a_read_that_cannot_find_the_login_claude_code_names_records_nothing() {
        let m = machine("read-login-elsewhere");
        m.mem.live().delete_everything();
        let before = state_file(&m);

        let read = Pitboard::new(m.ctx.clone()).status(false).expect("a read");

        assert_eq!(state_file(&m), before);
        assert!(audit_lines(&m, "in-use").is_empty());
        let said = said(&read);
        assert!(
            !said.iter().any(|(code, _)| *code == "login_replaced"),
            "{said:?}"
        );
    }

    /// The labels of the rows a read has in use.
    fn in_use(read: &Done<status::Report>) -> Vec<&str> {
        read.value
            .rows
            .iter()
            .filter(|row| row.signed_in)
            .filter_map(|row| row.label.as_deref())
            .collect()
    }

    /// Another Claude Code process that starts or signs in on another login can write that
    /// login's account into Claude Code's config, and leaves the keychain's as it was. A read
    /// that asks nobody takes the account in use from what Anthropic last said of the login
    /// stored, and says the config has moved since. It took the config's account.
    #[test]
    fn an_offline_read_names_the_account_anthropic_named_whatever_the_config_says() {
        let m = machine("offline-config-moved");
        switch::harness::config_names(&m, "there");

        let read = Pitboard::new(m.ctx.clone())
            .status_offline()
            .expect("a read");

        assert_eq!(in_use(&read), ["here"]);
        assert_eq!(
            said(&read),
            [(
                "in_use_unconfirmed",
                "Claude Code's config has named `there` since Pitboard last asked Anthropic \
                 whose login Claude Code has stored, so another login may be stored. The one \
                 stored then was `here`'s. `pitboard status` asks again."
                    .to_string()
            )]
        );
        assert_eq!(m.api.calls(), 0);
    }

    /// A record brought forward from 0.9.0 names the account it last switched to, which
    /// Anthropic never named. Where Claude Code's config names another account, a read that
    /// asks nobody says the account in use is not confirmed, as doctor and the status line do.
    #[test]
    fn an_offline_read_of_a_record_never_asked_about_says_the_config_names_another() {
        let m = machine("offline-never-asked");
        let mut state = state::load(&m.ctx).expect("state");
        let here = state.get(&m.key("here")).expect("here").owner();
        state.in_use.insert(
            ProviderId::Claude.code().into(),
            InUse {
                owner: Some(here),
                login: String::new(),
                known_at: 0,
                named: None,
            },
        );
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        switch::harness::config_names(&m, "there");

        let read = Pitboard::new(m.ctx.clone())
            .status_offline()
            .expect("a read");

        assert_eq!(in_use(&read), ["here"]);
        assert_eq!(
            said(&read),
            [(
                "in_use_unconfirmed",
                "Claude Code's config names `there`, and Pitboard has not asked Anthropic whose \
                 login Claude Code has stored since Pitboard was updated, so another login may \
                 be stored. The account it last switched to is `here`. `pitboard status` asks."
                    .to_string()
            )]
        );
        assert_eq!(m.api.calls(), 0);
    }

    // As on nearly every machine updated from 0.9.0 until its first read that asks.
    #[test]
    fn an_offline_read_of_a_record_never_asked_about_is_not_confirmed_where_the_config_agrees() {
        let m = machine("offline-never-asked-agrees");
        let mut state = state::load(&m.ctx).expect("state");
        let here = state.get(&m.key("here")).expect("here").owner();
        state.in_use.insert(
            ProviderId::Claude.code().into(),
            InUse {
                owner: Some(here),
                login: String::new(),
                known_at: 0,
                named: None,
            },
        );
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        switch::harness::config_names(&m, "here");

        let read = Pitboard::new(m.ctx.clone())
            .status_offline()
            .expect("a read");

        assert_eq!(in_use(&read), ["here"]);
        assert_eq!(
            said(&read),
            [(
                "in_use_unconfirmed",
                "Claude Code's config names `here`, and Pitboard has not asked Anthropic whose \
                 login Claude Code has stored since Pitboard was updated, so another login may \
                 be stored. The account it last switched to is `here`. `pitboard status` asks."
                    .to_string()
            )]
        );
        assert_eq!(m.api.calls(), 0);
    }

    /// A read that asks settles it: the login stored is the one Anthropic named, known by its
    /// fingerprint, so nobody is asked whose it is, and what stands is that the config names
    /// another account, which `/status` in Claude Code shows. Said by every read after it,
    /// the one that asks nobody too, until something writes the config again.
    #[test]
    fn a_read_settles_it_and_says_the_config_names_another() {
        use crate::api::scripted::Asked;
        let m = machine("read-config-names-another");
        switch::harness::config_names(&m, "there");
        let pitboard = Pitboard::new(m.ctx.clone());
        let names_another = "Claude Code's config names `there`, and the login Claude Code has \
                             stored is `here`'s, as Anthropic said. `/status` in Claude Code \
                             shows `there`. `pitboard use here` writes `here` into the config.";

        let read = pitboard.status(false).expect("a read");

        assert!(
            !m.api
                .asked()
                .contains(&Asked::Owner("access-here-refresh".into())),
            "{:?}",
            m.api.asked()
        );
        assert_eq!(in_use(&read), ["here"]);
        assert_eq!(
            said(&read),
            [("config_names_another", names_another.to_string())]
        );

        let offline = pitboard.status_offline().expect("a read of what is known");
        assert_eq!(in_use(&offline), ["here"]);
        assert_eq!(
            said(&offline),
            [("config_names_another", names_another.to_string())]
        );
    }

    /// A tool whose login names its own account is identified from the login, which asks
    /// nobody, so a read that sends no request can still tell whether the next change
    /// finishes its switch. Here Codex has renewed its login since the switch stopped, so the
    /// login matches neither side the record kept, and the copy the switch parked cannot be
    /// read. `doctor` and the read that asks nobody said nothing of it, as though only OpenAI
    /// could say whose the login is.
    #[test]
    fn whose_a_codex_login_is_is_told_without_asking_anyone() {
        let m = interrupted(codex_machine, "codex-renewed-since");
        m.sign_in(&switch::harness::codex_login("here", "renewed-since"));
        m.mem.vault().fault(
            &reserved(&m),
            crate::store::memory::Fault::Unreadable("locked".into()),
        );
        let pitboard = Pitboard::new(m.ctx.clone());
        let asked = m.api.calls();

        let read = pitboard.status_offline().expect("a read");
        let checks = pitboard.doctor().checks;

        assert_eq!(m.api.calls(), asked, "asks nobody");
        let refused = pitboard
            .switch_to("codex/there")
            .expect_err("it stops at it");
        assert_eq!(refused.error.code(), "recovery_undetermined");
        assert_eq!(
            said(&read),
            [("recovery_undetermined", refused.error.to_string())]
        );
        let check = checks
            .iter()
            .find(|c| c.code == "interrupted_switch")
            .expect("said");
        assert_eq!(check.detail, refused.error.to_string());
    }

    /// A Codex store that this machine's `requirements.toml` pins is refused before a sign-in
    /// starts, naming that file, as it is for a switch: no line of the person's own changes
    /// it, and the `-c` Pitboard's sign-ins give is under it, so the login would be kept where
    /// Pitboard cannot read it back. Pitboard used to read only `$CODEX_HOME/config.toml`, and
    /// started the sign-in.
    #[test]
    fn a_codex_store_a_requirement_pins_is_refused_before_a_sign_in() {
        let m = codex_machine("codex-pinned");
        // Named where nothing is, so a sign-in that got past the refusal would find no
        // program to run rather than this machine's own `codex`.
        let ctx = m.ctx.clone().with_codex_program("/nowhere/codex".into());
        let required = crate::provider::codex::layers::system_file(&ctx, "requirements.toml");
        m.mem.administers(
            required.clone(),
            "cli_auth_credentials_store = \"keyring\"\n",
        );
        let pitboard = Pitboard::new(ctx);
        for refused in [
            pitboard.sign_in("codex/new").expect_err("refused"),
            pitboard.sign_in_watched("codex/new").expect_err("refused"),
        ] {
            assert_eq!(refused.error.code(), "live_store_unsupported");
            let said = refused.error.to_string();
            assert!(
                said.contains(&format!("pinned to `keyring` by {}", required.display())),
                "{said}"
            );
            assert!(
                said.contains("No line in your own config.toml can change it"),
                "{said}"
            );
        }
        let switched = pitboard.switch_to("codex/there").expect_err("refused too");
        assert_eq!(switched.error.code(), "live_store_unsupported");
        let file = m
            .mem
            .file_at(crate::provider::codex::paths::auth_file(&m.ctx));
        let kept = crate::store::RawStore::read(&file, "auth.json")
            .expect("readable")
            .expect("there");
        let kept: serde_json::Value = serde_json::from_str(&kept).expect("JSON");
        assert_eq!(
            kept["tokens"]["refresh_token"], "here-refresh",
            "the auth.json Codex does not use is left as it was"
        );
    }

    /// The park the interrupted switch reserved, as its record names it.
    fn reserved(m: &Rig) -> String {
        let raw = std::fs::read_to_string(crate::home::dir(&m.ctx).join("journal.json"))
            .expect("a record");
        serde_json::from_str::<serde_json::Value>(&raw).expect("a record is JSON")["park_service"]
            .as_str()
            .expect("a park")
            .to_owned()
    }

    /// `doctor` sends no request, and says what a read says wherever it can tell without
    /// one: the refusal, and the way out. Where only the service could tell, it says what
    /// it always said, that a switch did not finish.
    #[test]
    fn doctor_says_a_switch_is_stuck_where_it_can_tell_without_asking() {
        for (tool, make) in MACHINES {
            let m = stuck(make, &format!("doctor-stuck-{tool}"));
            let pitboard = Pitboard::new(m.ctx.clone());
            let before = untouched(&m);

            let checks = pitboard.doctor().checks;

            assert_eq!(untouched(&m), before, "{tool}: doctor changes nothing");
            let refused = pitboard
                .switch_to(&m.key("there").typed())
                .expect_err("refused");
            let check = checks
                .iter()
                .find(|c| c.code == "interrupted_switch")
                .expect("said");
            assert_eq!(check.level, doctor::Level::Warn, "{tool}");
            assert_eq!(check.detail, refused.error.to_string(), "{tool}");
            assert!(check.advice.contains("pitboard abandon"), "{tool}");
        }

        let m = stuck_unless_asked("doctor-stuck-unasked");
        let asked = m.api.calls();
        let checks = Pitboard::new(m.ctx.clone()).doctor().checks;
        assert_eq!(m.api.calls(), asked, "doctor asks nobody");
        let check = checks
            .iter()
            .find(|c| c.code == "interrupted_switch")
            .expect("said");
        assert_eq!(check.detail, "a switch did not finish");
    }

    /// Every file in Pitboard's own directory but the audit log.
    fn files(m: &Rig) -> BTreeMap<String, Vec<u8>> {
        std::fs::read_dir(crate::home::dir(&m.ctx))
            .expect("a Pitboard home")
            .map(|entry| entry.expect("an entry").path())
            .filter(|path| path.file_name() != Some("audit.log".as_ref()))
            .map(|path| {
                let body = std::fs::read(&path).unwrap_or_default();
                (path.display().to_string(), body)
            })
            .collect()
    }

    /// The last two lines of the audit log, as verb and outcome.
    fn last_audited(m: &Rig) -> Vec<(String, String)> {
        audit::read(&m.ctx, 2)
            .into_iter()
            .map(|entry| (entry.verb, entry.outcome))
            .collect()
    }

    /// Every other change settles an interrupted switch before anything else, and one
    /// refused over its name did not, so the switch stayed unrecovered and the refusal was
    /// all anybody was told. It is settled now, recorded the way any recovery is, and
    /// reported beside the refusal.
    #[test]
    fn a_change_refused_over_its_name_still_recovers_an_interrupted_switch() {
        for (tool, make) in MACHINES {
            for (change, verb, code, audited, refuse) in REFUSALS {
                let at = format!("{tool}, {change}");
                let m = interrupted(make, &format!("refused-{tool}-{change}"));

                let failed = refuse(&Pitboard::new(m.ctx.clone()), m.which)
                    .unwrap_or_else(|| panic!("{at}: the name must still be refused"));

                assert_eq!(failed.error.code(), code, "{at}: {}", failed.error);
                let said: Vec<&str> = failed.warnings.iter().map(Warning::code).collect();
                assert_eq!(said, ["interrupted_switch_undone"], "{at}");
                assert!(!switch::interrupted(&m.ctx), "{at}: the record is resolved");
                hold(&m, &at);
                assert_eq!(
                    last_audited(&m),
                    [
                        (
                            "recover".to_string(),
                            "interrupted_switch_undone".to_string()
                        ),
                        (verb.to_string(), audited.to_string()),
                    ],
                    "{at}"
                );
            }
        }
    }

    /// A mistyped name with nothing to recover takes no lock and writes nothing but its
    /// line in the audit log.
    #[test]
    fn a_change_refused_over_its_name_with_nothing_interrupted_changes_nothing() {
        for (tool, make) in MACHINES {
            for (change, verb, code, audited, refuse) in REFUSALS {
                let at = format!("{tool}, {change}");
                let m = make(&format!("refused-quietly-{tool}-{change}"));
                let (before, parked, live) = (files(&m), m.mem.vault().services(), m.live());

                let failed = refuse(&Pitboard::new(m.ctx.clone()), m.which)
                    .unwrap_or_else(|| panic!("{at}: the name must still be refused"));

                assert_eq!(failed.error.code(), code, "{at}: {}", failed.error);
                assert!(failed.warnings.is_empty(), "{at}: {:?}", failed.warnings);
                assert_eq!(files(&m), before, "{at}: not even the lock file is made");
                assert_eq!(m.mem.vault().services(), parked, "{at}");
                assert_eq!(m.live(), live, "{at}");
                assert_eq!(
                    last_audited(&m).last(),
                    Some(&(verb.to_string(), audited.to_string())),
                    "{at}"
                );
            }
        }
    }

    /// A new login that did not hold after it was written is parked rather than lost, and
    /// parking a login too big for `security`'s standard input puts it on the argument line.
    /// The change then fails, and that is still said beside the failure.
    #[test]
    fn a_new_login_parked_after_it_did_not_hold_says_how_it_was_parked() {
        for (tool, make) in MACHINES {
            let m = make(&format!("not-installed-said-{tool}"));
            m.mem.vault().takes_on_stdin(64);
            let login = switch::harness::signed_in(&m, "here", "here-refresh-2");
            m.fault_live(crate::store::memory::Fault::DeletedAfterWrite);

            let failed = Pitboard::new(m.ctx.clone())
                .enroll_signed_in(&m.key("here").typed(), login)
                .expect_err("it did not hold");

            assert_eq!(failed.error.code(), "sign_in_not_installed", "{tool}");
            let said: Vec<&str> = failed.warnings.iter().map(Warning::code).collect();
            assert_eq!(said, ["written_on_the_command_line"], "{tool}");
        }
    }

    /// The recovery settles for the tool whose switch was interrupted, not for no tool in
    /// particular, so a custom Claude Code endpoint stops exactly what it stops for any
    /// change: the recovery of a Claude Code switch, and not of a Codex one. What it stops
    /// is left for a later run, and the refusal is reported as it always was.
    #[test]
    fn a_custom_claude_endpoint_stops_only_the_recovery_of_a_claude_code_switch() {
        let codex = interrupted(codex_machine, "refused-custom-codex");
        let mut ctx = codex.ctx.clone();
        ctx.custom_oauth = true;
        let failed = Pitboard::new(ctx).switch_to("nobody").expect_err("refused");
        assert_eq!(failed.error.code(), "account_unknown");
        let said: Vec<&str> = failed.warnings.iter().map(Warning::code).collect();
        assert_eq!(said, ["interrupted_switch_undone"]);
        assert!(!switch::interrupted(&codex.ctx));

        let claude = interrupted(machine, "refused-custom-claude");
        let mut ctx = claude.ctx.clone();
        ctx.custom_oauth = true;
        let failed = Pitboard::new(ctx).switch_to("nobody").expect_err("refused");
        assert_eq!(
            failed.error.code(),
            "account_unknown",
            "the refusal, not the recovery that could not run"
        );
        assert!(failed.warnings.is_empty(), "{:?}", failed.warnings);
        assert!(
            switch::interrupted(&claude.ctx),
            "the record is kept for a run that can finish it"
        );
    }

    #[derive(Debug, Clone, Copy)]
    enum Refused {
        As(Elevation),
        On(Floor),
    }

    const AS_ROOT: Refused = Refused::As(Elevation::Elevated { why: "as root" });
    const WITH_SUDO: Refused = Refused::As(Elevation::Elevated { why: "with sudo" });
    const SERVICE_ACCOUNT: Refused = Refused::As(Elevation::Elevated {
        why: crate::host::token::AS_A_SERVICE_ACCOUNT,
    });
    const ELEVATION_UNKNOWN: Refused = Refused::As(Elevation::Unknown);
    const BELOW_THE_FLOOR: Refused = Refused::On(Floor::Below { build: 22631 });
    const FLOOR_UNKNOWN: Refused = Refused::On(Floor::Unknown);

    const REFUSED: [Refused; 6] = [
        AS_ROOT,
        WITH_SUDO,
        SERVICE_ACCOUNT,
        ELEVATION_UNKNOWN,
        BELOW_THE_FLOOR,
        FLOOR_UNKNOWN,
    ];

    impl Refused {
        fn said_by(self, m: &Rig) {
            match self {
                Refused::As(elevation) => m.mem.runs_with(elevation),
                Refused::On(floor) => m.mem.runs_on(floor),
            }
        }

        fn lifted(m: &Rig) {
            m.mem.runs_with(Elevation::Normal);
            m.mem.runs_on(Floor::Met);
        }

        fn code(self) -> &'static str {
            match self {
                Refused::As(_) => "elevated",
                Refused::On(_) => "system_too_old",
            }
        }

        fn words(self) -> crate::words::ChangesNothing {
            match self {
                Refused::As(Elevation::Elevated { why }) => {
                    crate::words::elevated(crate::host::OS, Some(why))
                }
                Refused::As(_) => crate::words::elevated(crate::host::OS, None),
                Refused::On(Floor::Below { build }) => crate::words::too_old(Some(build)),
                Refused::On(_) => crate::words::too_old(None),
            }
        }
    }

    /// What one change came to, as a change refused at the gate is reported: its error,
    /// and the warnings found on the way, of which there must be none.
    fn failed<T>(outcome: Result<T>) -> Option<Failed> {
        outcome.err().map(Failed::from)
    }

    type Change = fn(&Pitboard, &Rig) -> Option<Failed>;

    /// Every change a front end can ask for, by its name. `enroll_signed_in` is asked apart,
    /// since it takes a sign-in made before.
    const CHANGES: [(&str, Change); 19] = [
        ("switch_to", |p, m| {
            p.switch_to(&m.key("there").typed()).err()
        }),
        ("enroll_current", |p, m| {
            p.enroll_current(&m.key("here").typed()).err()
        }),
        ("sign_in", |p, m| p.sign_in(&m.key("new").typed()).err()),
        ("sign_in_watched", |p, m| {
            p.sign_in_watched(&m.key("new").typed()).err()
        }),
        ("forget", |p, m| p.forget(&m.key("there").typed()).err()),
        ("rename", |p, m| {
            p.rename(&m.key("there").typed(), "elsewhere").err()
        }),
        ("abandon_recovery", |p, _| failed(p.abandon_recovery())),
        ("renew", |p, _| failed(p.renew())),
        ("schedule_install", |p, _| failed(p.schedule_install())),
        ("schedule_uninstall", |p, _| failed(p.schedule_uninstall())),
        ("schedule_uninstall_for_app", |p, _| {
            failed(p.schedule_uninstall_for_app())
        }),
        ("schedule_repair", |p, _| failed(p.schedule_repair())),
        ("watch_service_install", |p, _| {
            failed(p.watch_service_install(Threshold::DEFAULT))
        }),
        ("watch_service_uninstall", |p, _| {
            failed(p.watch_service_uninstall())
        }),
        ("adopt", |p, _| failed(p.adopt())),
        ("repair", |p, _| p.repair().err()),
        ("keep_app_file", |p, _| {
            failed(p.keep_app_file(crate::app::AppFile::Preferences, "{}"))
        }),
        ("uninstall", |p, _| p.uninstall().err()),
        ("auto_switch", |p, _| {
            p.auto_switch(Threshold::DEFAULT).err()
        }),
    ];

    /// Everything on a machine a change could leave different: every directory and file
    /// under its home, the audit log among them and the scheduler's files with them, every
    /// item in its keychain, every login parked and the one in use. The private directory
    /// a sign-in made before is not counted: dropping that sign-in takes it away again. The
    /// lock that sign-in holds counts by its length: Windows lets no other handle read it
    /// while it is held, and dropping the sign-in lets it go.
    type Everything = (
        BTreeMap<String, Option<Vec<u8>>>,
        Vec<(String, Option<String>)>,
        Vec<(String, Option<String>)>,
        Option<serde_json::Value>,
    );

    fn everything(m: &Rig) -> Everything {
        fn walk(dir: &std::path::Path, into: &mut BTreeMap<String, Option<Vec<u8>>>) {
            for entry in std::fs::read_dir(dir).expect("a directory to read") {
                let path = entry.expect("an entry").path();
                if path.ends_with(".pitboard/signin") {
                    continue;
                }
                if path.is_dir() {
                    into.insert(path.display().to_string(), None);
                    walk(&path, into);
                } else if path.ends_with(".pitboard/signin.lock") {
                    let len = std::fs::metadata(&path).expect("the lock's length").len();
                    into.insert(path.display().to_string(), Some(len.to_le_bytes().to_vec()));
                } else {
                    let body = std::fs::read(&path)
                        .unwrap_or_else(|e| panic!("{} to read: {e}", path.display()));
                    into.insert(path.display().to_string(), Some(body));
                }
            }
        }
        let mut files = BTreeMap::new();
        walk(&m.ctx_home(), &mut files);
        let items = |store: &crate::store::memory::MemoryStore| {
            store
                .services()
                .into_iter()
                .map(|service| {
                    let held = store.peek(&service);
                    (service, held)
                })
                .collect()
        };
        (files, items(m.mem.live()), items(m.mem.vault()), m.live())
    }

    /// Running as root or under sudo, Pitboard changed things as root: a state file, a
    /// schedule or a lock it made was root's, and a park might be, where the person's own
    /// runs might never read or replace it again. Every change is refused now, at one gate,
    /// before it reads, locks or records anything, and leaves the machine byte for byte as it
    /// was, the audit log included. The machine has an interrupted switch waiting, which every
    /// change that settles would otherwise finish, and asks nobody. One test for each tool
    /// and each way Pitboard may change nothing, named `<tool>_<way>`.
    mod every_change_is_refused_where_pitboard_may_change_nothing_and_changes_nothing {
        use super::*;

        macro_rules! cases {
            ($($case:ident: $make:ident, $refused:ident;)*) => {
                const _: () =
                    assert!([$(stringify!($case)),*].len() == MACHINES.len() * REFUSED.len());
                $(
                    #[test]
                    fn $case() {
                        every_change_is_refused(stringify!($case), $make, $refused);
                    }
                )*
            };
        }

        cases! {
            claude_as_root: machine, AS_ROOT;
            claude_with_sudo: machine, WITH_SUDO;
            claude_as_a_service_account: machine, SERVICE_ACCOUNT;
            claude_elevation_unknown: machine, ELEVATION_UNKNOWN;
            claude_below_the_floor: machine, BELOW_THE_FLOOR;
            claude_floor_unknown: machine, FLOOR_UNKNOWN;
            codex_as_root: codex_machine, AS_ROOT;
            codex_with_sudo: codex_machine, WITH_SUDO;
            codex_as_a_service_account: codex_machine, SERVICE_ACCOUNT;
            codex_elevation_unknown: codex_machine, ELEVATION_UNKNOWN;
            codex_below_the_floor: codex_machine, BELOW_THE_FLOOR;
            codex_floor_unknown: codex_machine, FLOOR_UNKNOWN;
        }
    }

    fn every_change_is_refused(case: &str, make: Make, refused: Refused) {
        for (change, run) in CHANGES {
            let at = format!("{case}, {change}");
            let m = interrupted(make, &format!("gate-{case}-{change}"));
            refused.said_by(&m);
            let (before, asked) = (everything(&m), m.api.calls());

            let failed =
                run(&Pitboard::new(m.ctx.clone()), &m).unwrap_or_else(|| panic!("{at}: refused"));

            assert_eq!(
                failed.error.code(),
                refused.code(),
                "{at}: {}",
                failed.error
            );
            assert!(failed.warnings.is_empty(), "{at}: {:?}", failed.warnings);
            assert!(everything(&m) == before, "{at}: nothing changes");
            assert_eq!(m.api.calls(), asked, "{at}: nobody is asked");
        }

        let at = format!("{case}, enroll_signed_in");
        let m = interrupted(make, &format!("gate-{case}-enroll-signed-in"));
        let login = switch::harness::signed_in(&m, "new", "new-refresh");
        refused.said_by(&m);
        let before = everything(&m);
        let failed = Pitboard::new(m.ctx.clone())
            .enroll_signed_in(&m.key("new").typed(), login)
            .expect_err("refused");
        assert_eq!(failed.error.code(), refused.code(), "{at}");
        assert!(everything(&m) == before, "{at}: nothing changes");
    }

    #[test]
    fn the_gate_says_why_it_refuses_and_lets_a_person_through() {
        let m = machine("gate-words");
        let pitboard = Pitboard::new(m.ctx.clone());
        assert!(pitboard.permit().is_ok(), "as the person");
        for refused in REFUSED {
            refused.said_by(&m);
            let refusal = pitboard.permit().expect_err("refused");
            assert_eq!(refusal.code(), refused.code(), "{refused:?}");
            assert_eq!(refusal.exit_code(), 1, "{refused:?}");
            assert_eq!(refusal.to_string(), refused.words().refusal, "{refused:?}");
            Refused::lifted(&m);
            assert!(pitboard.permit().is_ok(), "{refused:?}: lifted");
        }
    }

    #[test]
    fn the_gate_lets_windows_11_24h2_through_and_says_an_older_one_first() {
        let m = machine("gate-floor");
        let pitboard = Pitboard::new(m.ctx.clone());
        for build in [26100, 26200] {
            m.mem
                .runs_on(crate::host::windows_floor(Some((10, 0, build))));
            assert!(pitboard.permit().is_ok(), "{build}");
        }
        m.mem
            .runs_on(crate::host::windows_floor(Some((10, 0, 22631))));
        let refused = pitboard.permit().expect_err("refused");
        assert_eq!(refused.code(), "system_too_old");
        assert_eq!(
            refused.to_string(),
            crate::words::too_old(Some(22631)).refusal
        );
        m.mem.runs_with(Elevation::Elevated { why: "as root" });
        let first = pitboard.permit().expect_err("refused");
        assert_eq!(
            first.code(),
            "system_too_old",
            "the system before how it runs"
        );
    }

    /// An empty or relative `HOME` made every path under it lead into the folder Pitboard
    /// was run from: its own files where `PITBOARD_HOME` was not set, and the scheduler's
    /// and each tool's default folder whatever was. The gate refuses it now, and so does
    /// every read of the accounts, with `home_not_absolute`, and nothing changes. Asked of
    /// the gate and the reads first, which change nothing whatever they answer.
    #[test]
    fn every_change_and_read_is_refused_where_the_home_is_not_a_full_path() {
        for home in ["", "relative"] {
            for (tool, make) in MACHINES {
                let at = format!("{tool}, HOME={home:?}");
                let m = make(&format!("unplaced-{tool}-{}", home.len()));
                let mut ctx = m.ctx.clone();
                ctx.home = std::path::PathBuf::from(home);
                let pitboard = Pitboard::new(ctx);
                let refused = |error: Error| {
                    assert_eq!(error.code(), "home_not_absolute", "{at}: {error}");
                    assert!(
                        error
                            .to_string()
                            .starts_with(&format!("{} is ", crate::host::OS.home_variable())),
                        "{at}: {error}"
                    );
                };

                refused(pitboard.permit().expect_err("the gate refuses"));
                refused(pitboard.status_offline().expect_err("refused"));
                refused(pitboard.status(false).expect_err("refused"));
                let before = (everything(&m), m.api.calls());
                for (change, run) in CHANGES {
                    let failed = run(&pitboard, &m).unwrap_or_else(|| panic!("{at}: {change}"));
                    refused(failed.error);
                    assert!(failed.warnings.is_empty(), "{at}, {change}");
                }
                assert!(
                    (everything(&m), m.api.calls()) == before,
                    "{at}: nothing changes and nobody is asked"
                );
            }
        }
    }

    /// What a read says, account by account: enough to tell a live read from one of what
    /// was last measured.
    type Read = Vec<(
        ProviderId,
        Option<String>,
        bool,
        Option<crate::usage::Snapshot>,
        Option<status::Stale>,
    )>;

    fn rows(report: &status::Report) -> Read {
        report
            .rows
            .iter()
            .map(|row| {
                (
                    row.provider,
                    row.label.clone(),
                    row.signed_in,
                    row.usage.clone(),
                    row.stale,
                )
            })
            .collect()
    }

    /// A read renewed every parked login whose access had lapsed, asked each service about
    /// every account and wrote what it measured down, as root too. Where this process may
    /// change nothing it answers what the read that asks nobody answers, and says why: it
    /// renews nothing, asks nobody and writes nothing. A park whose access has lapsed is
    /// there to be renewed, so a read that renewed would show it.
    #[test]
    fn a_read_where_pitboard_may_change_nothing_asks_nobody_and_says_why() {
        for (tool, make) in MACHINES {
            for refused in REFUSED {
                let at = format!("{tool}, {refused:?}");
                let m = make(&format!("gate-read-{tool}"));
                let later = m
                    .ctx
                    .clone()
                    .with_clock(Arc::new(crate::time::FixedClock::at(
                        switch::harness::NOW + 11 * 86_400,
                    )));
                let pitboard = Pitboard::new(later);
                refused.said_by(&m);
                let (before, asked) = (everything(&m), m.api.calls());

                let read = pitboard.status(false).expect("a read");
                let known = pitboard.status_offline().expect("a read");

                assert!(everything(&m) == before, "{at}: nothing is written");
                assert_eq!(m.api.calls(), asked, "{at}: nobody is asked");
                assert_eq!(rows(&read.value), rows(&known.value), "{at}");
                let codes: Vec<&str> = read.warnings.iter().map(Warning::code).collect();
                assert_eq!(codes.last(), Some(&"read_only"), "{at}");
                assert_eq!(read.warnings.len(), known.warnings.len() + 1, "{at}");
                let said = read.warnings.last().expect("said").to_string();
                let words = refused.words();
                assert!(
                    said.starts_with(&format!("{}, so it changes nothing", words.because)),
                    "{at}: {said}"
                );
                assert!(said.ends_with(words.to_ask_again), "{at}: {said}");

                Refused::lifted(&m);
                let live = pitboard.status(false).expect("a read");
                assert!(m.api.calls() > asked, "{at}: as the person, it asks");
                assert!(
                    !live.warnings.iter().any(|w| w.code() == "read_only"),
                    "{at}"
                );
            }
        }
    }

    /// The status line kept what each session passed it and the readings it moved, as root
    /// too. Where this process may change nothing it draws the same line from the files as
    /// they are, and writes neither.
    #[test]
    fn the_status_line_where_pitboard_may_change_nothing_draws_the_same_line_and_writes_nothing() {
        let session = serde_json::json!({
            "session_id": "a-session",
            "rate_limits": {
                "five_hour": {"used_percentage": 40.0, "resets_at": switch::harness::NOW + 3_600},
                "seven_day": {"used_percentage": 20.0, "resets_at": switch::harness::NOW + 86_400},
            },
        })
        .to_string();
        for refused in REFUSED {
            let m = machine("gate-statusline");
            let pitboard = Pitboard::new(m.ctx.clone());
            refused.said_by(&m);
            let before = everything(&m);

            let drawn = pitboard.statusline(&session);

            assert!(everything(&m) == before, "{refused:?}: nothing is written");
            Refused::lifted(&m);
            assert_eq!(pitboard.statusline(&session), drawn, "{refused:?}");
            assert!(
                crate::home::dir(&m.ctx).join("sessions.json").exists(),
                "{refused:?}: as the person, the session is kept"
            );
        }
    }

    /// `renew` returned an empty list when it renewed nothing, whatever the reason, so a
    /// run that could not renew read as one where nothing was due. Refused, it says so, and
    /// asks nobody, with a park that is due to be renewed.
    #[test]
    fn a_renewal_where_pitboard_may_change_nothing_is_refused_and_asks_nobody() {
        for (tool, make) in MACHINES {
            for refused in REFUSED {
                let at = format!("{tool}, {refused:?}");
                let m = make(&format!("gate-renew-{tool}"));
                switch::harness::renews(&m, "there-refresh", "there-refresh-2");
                let later = m
                    .ctx
                    .clone()
                    .with_clock(Arc::new(crate::time::FixedClock::at(
                        switch::harness::NOW + 11 * 86_400,
                    )));
                let pitboard = Pitboard::new(later);
                refused.said_by(&m);
                let (before, asked) = (everything(&m), m.api.calls());

                let refusal = pitboard.renew().expect_err("refused");

                assert_eq!(refusal.code(), refused.code(), "{at}");
                assert!(everything(&m) == before, "{at}: nothing changes");
                assert_eq!(m.api.calls(), asked, "{at}: nobody is asked");

                Refused::lifted(&m);
                let renewed = pitboard.renew().expect("as the person, it renews");
                let codes: Vec<&str> = renewed.iter().map(|(_, r)| r.code()).collect();
                assert_eq!(codes, ["renewed"], "{at}");
            }
        }
    }

    /// `doctor` says why every change is refused, first, as a check that fails, and says
    /// nothing of it where Pitboard runs as the person.
    #[test]
    fn doctor_says_why_pitboard_changes_nothing() {
        let m = machine("gate-doctor");
        let pitboard = Pitboard::new(m.ctx.clone());
        let refusing = |checks: &[doctor::Check]| -> Vec<&'static str> {
            checks
                .iter()
                .map(|c| c.code)
                .filter(|code| ["elevated", "system_too_old"].contains(code))
                .collect()
        };
        assert!(
            refusing(&pitboard.doctor().checks).is_empty(),
            "as the person"
        );
        for refused in REFUSED {
            refused.said_by(&m);
            let checks = pitboard.doctor().checks;
            let check = checks.first().expect("checks");
            let words = refused.words();
            assert_eq!(check.code, refused.code(), "{refused:?}");
            assert_eq!(check.level, doctor::Level::Fail, "{refused:?}");
            assert_eq!(check.detail, words.because, "{refused:?}");
            assert!(
                check.advice.starts_with("Pitboard changes nothing ")
                    && check.advice.ends_with(words.way_out),
                "{refused:?}: {}",
                check.advice
            );
            Refused::lifted(&m);
        }
        m.mem.runs_on(Floor::Below { build: 22631 });
        m.mem.runs_with(Elevation::Unknown);
        assert_eq!(
            refusing(&pitboard.doctor().checks),
            ["system_too_old", "elevated"]
        );
    }
}
