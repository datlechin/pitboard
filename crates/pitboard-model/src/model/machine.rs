//! What the model knows of this machine rather than its accounts, as MachineModel.swift held
//! it: the daily renewal schedule and the one change to it under way, doctor's checks, what
//! Pitboard has changed, and the `pitboard` a terminal would run. `State` takes each intent
//! and answer about them, and `present` says them.
//!
//! Opening at login and linking the command line onto the `PATH` with an administrator's
//! password are the app's own, as its system's: once it has linked it, the app sends
//! `Intent::LookForCommandLine`, and the model looks for the command line again.

use crate::{Change, Check, Doctored, FoundCommandLine, OwnCommandLine, PitboardError, Schedule};

/// How many changes the activity pane reads, the newest of them, as ActivityPane.swift
/// asked MachineModel.swift's `readChanges` for them.
pub(crate) const LOG_LIMIT: u32 = 500;

/// What the model knows of this machine. The actor's alone, as the rest of `State` is.
#[derive(Debug, Default)]
pub(crate) struct MachineState {
    /// The schedule as the core last read it, or `None` before it has been read, which the
    /// settings show as off, as MachineModel.swift's `.absent` did.
    pub(crate) schedule: Option<Schedule>,
    /// The change to the schedule under way, what it asked for, from the moment it is asked
    /// for until the schedule has been read back after it: the switch shows it meanwhile
    /// rather than snapping back, and a second press does nothing.
    pub(crate) scheduling: Option<bool>,
    /// Why the last change to the schedule could not be made, until the next is asked for.
    pub(crate) schedule_failed: Option<ScheduleFailure>,
    /// The command line inside this copy of the app, as last read with the schedule, a
    /// change to it, the repair at launch or the command line found.
    pub(crate) own: Option<OwnCommandLine>,
    /// Doctor's checks, the last ones made.
    pub(crate) checks: Vec<Check>,
    // doctor's report of them, which Copy Report copies
    pub(crate) report: Option<String>,
    /// Checks under way. Counted, as reads are: one asked for while another runs answers
    /// after it, and the first to end does not say none is running.
    pub(crate) checking: u32,
    /// When the last checks came in, in epoch seconds.
    pub(crate) checked_at: Option<i64>,
    /// What Pitboard has changed, newest first, as last read.
    pub(crate) log: Vec<Change>,
    /// The `pitboard` a terminal would run, once looked for.
    pub(crate) command_line: Option<FoundCommandLine>,
    // the last notification the app's system refused, until one is posted
    pub(crate) post_refused: Option<PostRefused>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PostRefused {
    pub(crate) title: String,
    pub(crate) reason: String,
    pub(crate) at: i64,
}

/// Why a change to the schedule could not be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScheduleFailure {
    /// Turning it on was refused before the scheduler was asked: this copy of the app has no
    /// command line a schedule would keep reaching. `temporary` where that is because it runs
    /// from a temporary copy.
    CannotSchedule { temporary: bool },
    /// The core could not make it, in its own words.
    Refused { message: String },
}

/// What a change to the schedule came to, as the lane of changes answers it.
#[derive(Debug)]
pub(crate) enum Scheduled {
    /// The scheduler did as asked.
    Done,
    /// Turning it on was not asked of the scheduler: this copy of the app has no command line
    /// a schedule would keep reaching, as `OwnCommandLine::lasting` says.
    CannotSchedule,
    /// The core could not make it.
    Refused(PitboardError),
}

impl MachineState {
    /// A change to the schedule, unless one is under way already: it was pressed on a switch
    /// that did not yet show the first press. Whatever the last one said goes.
    pub(crate) fn schedule(&mut self, on: bool) -> bool {
        if self.scheduling.is_some() {
            return false;
        }
        self.scheduling = Some(on);
        self.schedule_failed = None;
        true
    }

    /// What a change to the schedule came to, given what the command line inside this copy
    /// is. Whether the schedule is to be read back: the switch then shows what the scheduler
    /// has, read again rather than assumed. A change refused before the scheduler was asked
    /// changed nothing, so nothing is read, and the change is over.
    pub(crate) fn scheduled(&mut self, own: OwnCommandLine, outcome: Scheduled) -> bool {
        self.own = Some(own);
        match outcome {
            Scheduled::Done => true,
            Scheduled::Refused(PitboardError::Failed { message, .. }) => {
                self.schedule_failed = Some(ScheduleFailure::Refused { message });
                true
            }
            Scheduled::CannotSchedule => {
                self.schedule_failed = Some(ScheduleFailure::CannotSchedule {
                    temporary: own.temporary,
                });
                self.scheduling = None;
                false
            }
        }
    }

    /// The schedule as the core read it. The read after a change ends that change.
    pub(crate) fn schedule_read(
        &mut self,
        schedule: Option<Schedule>,
        own: Option<OwnCommandLine>,
        after_change: bool,
    ) {
        if let Some(schedule) = schedule {
            self.schedule = Some(schedule);
        }
        if let Some(own) = own {
            self.own = Some(own);
        }
        if after_change {
            self.scheduling = None;
        }
    }

    /// Doctor's checks came in at `at`, in place of the last ones, or a check came to nothing.
    pub(crate) fn checked(&mut self, doctored: Option<Doctored>, at: i64) {
        self.checking = self.checking.saturating_sub(1);
        if let Some(Doctored { checks, report }) = doctored {
            self.checks = checks;
            self.report = Some(report);
            self.checked_at = Some(at);
        }
    }

    pub(crate) fn posted(&mut self, title: String, refused: Option<String>, at: i64) {
        self.post_refused = refused.map(|reason| PostRefused { title, reason, at });
    }

    /// The core's log came in, oldest first as it keeps it: shown newest first.
    pub(crate) fn logged(&mut self, changes: Vec<Change>) {
        self.log = changes.into_iter().rev().collect();
    }
}
