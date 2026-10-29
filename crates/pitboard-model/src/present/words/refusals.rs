//! The core's errors and warnings as the apps say them, where the core's own sentence names a
//! command or says to run it again, which an app can't.

use super::{awaiting_lines, quoted, sign_in_to_keep, typeset};
use crate::Front;
use crate::Tool;
use crate::model::state::in_its_app;
use pitboard_core::error::{Cause, Error, LoginDoubt, Then, Unidentified};
use pitboard_core::host::Private;
use pitboard_core::provider::ProviderId;
use pitboard_core::service::{Stored, Warning as CoreWarning};
use pitboard_core::state::{Key, SetAside};
use pitboard_core::words::{capitalised, listed};

// An account the core names as typed, `desktop/work`, as the app names it: `work`.
fn bare(typed: &str) -> String {
    quoted(&Key::parse(typed).label)
}

// Where an account is named: one added by its name, any other as the app can say it.
fn stored(stored: &Stored) -> String {
    match stored {
        Stored::Account(name) => bare(name).to_owned(),
        Stored::Unenrolled(email) if email.is_empty() => "an account you haven’t added".into(),
        Stored::Unenrolled(email) => email.clone(),
        Stored::Nothing | Stored::Unknown => "nobody".into(),
    }
}

fn add_what_it_shows(app: &str) -> String {
    format!("choose Add Account…, then Add the Account {app} Shows, under that account’s name.")
}

// The core's sentence names a command or says to run this again, which the app cannot.
pub(crate) fn in_app_refusal(front: Front, error: &Error) -> Option<String> {
    let desktop = ProviderId::Desktop;
    let app = desktop.program();
    let inside = |tool: &ProviderId| in_its_app(*tool);
    let inside_label = |label: &str| in_its_app(Key::parse(label).provider);
    let said = match error {
        Error::AppOpen { app, still } if still.is_empty() => format!(
            "{app} is open, and Pitboard changes its sign-in only while it is closed. Quit {app}, \
             then try again. Nothing has changed."
        ),
        Error::AppOpen { app, still } => format!(
            "{app} has quit, and {} from it is still running, and Pitboard changes {app}’s sign-in \
             only while all of it is closed. {} Nothing has changed.",
            listed(still),
            pitboard_core::words::wait_for_still(front.os(), "try again")
        ),
        Error::AppStateUnknown { app, why } => format!(
            "Pitboard couldn’t tell whether {app} is open ({why}), so nothing has changed. Try \
             again."
        ),
        Error::LoginFolderBusy { app } => format!(
            "Another Pitboard is changing {app}’s sign-in now, so nothing has changed. Try again \
             once it has finished."
        ),
        Error::MachineUnknown => "Pitboard couldn’t read this Mac’s identity, which it saves \
                                  with every Claude Desktop sign-in so it never uses one from \
                                  another Mac. Nothing has changed."
            .into(),
        Error::LiveCredentialAbsent { tool, .. } if inside(tool) => format!(
            "{} isn’t signed in. Choose Start Sign-In, then sign in to the account in {}.",
            tool.program(),
            tool.program()
        ),
        Error::SignInUnfinished { tool } if inside(tool) => {
            let app = tool.program();
            format!(
                "{app} hasn’t finished signing in. Open {app}, wait until it shows your chats, \
                 quit it, then try again. Nothing has changed."
            )
        }
        Error::LiveAccountNotEnrolled { tool, .. } if inside(tool) => format!(
            "{} is signed in to an account you haven’t added, so Pitboard can’t switch away from \
             it. Choose Add Account…, then Add the Account {} Shows, to add it first.",
            tool.program(),
            tool.program()
        ),
        Error::AlreadyEnrolled {
            tool,
            label,
            waited: true,
            ..
        } if inside(tool) => format!(
            "{} is signed in to {label}, which you’ve added. Choose Add {label} on its row’s Sign \
             In Again… to save that sign-in, then add another.",
            tool.program(),
            label = bare(label)
        ),
        Error::AlreadyEnrolled { tool, label, .. } if inside(tool) => format!(
            "{} is signed in to {label}, which you’ve added already. Switch to {label}, or choose \
             Add Account… to add another.",
            tool.program(),
            label = bare(label)
        ),
        Error::LoginInDoubt { tool, why } if inside(tool) => match why {
            LoginDoubt::NamedAnother { recorded, named } => format!(
                "Pitboard saved this {app} sign-in as {}’s, and {app} names {} for it. Open \
                 {app}, check which account it shows, quit it, then {} Nothing has changed.",
                bare(recorded),
                named
                    .as_deref()
                    .map_or_else(|| "an account you haven’t added".into(), bare),
                add_what_it_shows(tool.program()),
                app = tool.program()
            ),
            LoginDoubt::SignedInOutside { label } => format!(
                "{app} was signed in to {} outside Pitboard, so Pitboard can’t tell this sign-in \
                 from the one it saved. Open {app}, check which account it shows, quit it, then \
                 {} Nothing has changed.",
                bare(label),
                add_what_it_shows(tool.program()),
                app = tool.program()
            ),
        },
        Error::LoginFolderUnknown { tool, what, then } if inside(tool) => {
            let app = tool.program();
            let then = match then {
                Then::Nothing => String::new(),
                Then::OpenAndQuit => format!("Open {app}, quit it, then try again. "),
                Then::UpdatePitboard => "Update Pitboard, or report it at \
                                         https://github.com/datlechin/pitboard/issues if no \
                                         release reads it yet. "
                    .into(),
                Then::CopyAside => format!(
                    "{app} resets a settings file it can’t read when it opens, so copy it aside, \
                     open {app}, quit it, then try again. "
                ),
                Then::SignInAndEnrol => format!(
                    "Open {app}. If it asks you to sign in, sign in. Quit it, then choose Add \
                     Account…, then Add the Account {app} Shows. "
                ),
            };
            format!(
                "{app}’s data folder {what}, so Pitboard switches no {} sign-in. {then}Nothing has \
                 changed.",
                tool.name()
            )
        }
        Error::LoginFolderUnreadable { tool, detail } if inside(tool) => {
            let app = tool.program();
            format!(
                "Pitboard couldn’t read {app}’s cookie store ({detail}). Open {app}, quit it, then \
                 try again. Nothing has changed."
            )
        }
        Error::ParksNotPrivate { path, why } => {
            let path = path.display();
            let said = match why {
                Private::Shared => format!(
                    "{path} can be reached by others. Run chmod 700 on it in Terminal, then try \
                     again."
                ),
                Private::Link => format!(
                    "{path} is a link, so Pitboard saves no Claude Desktop sign-in there. Point \
                     PITBOARD_HOME at a folder of your own on this volume, then try again."
                ),
                Private::NotOwn => format!(
                    "{path} belongs to another user, so Pitboard saves no Claude Desktop sign-in \
                     there. Point PITBOARD_HOME at a folder of your own on this volume, then try \
                     again."
                ),
                Private::NotADirectory | Private::Missing | Private::Yes => format!(
                    "{path} isn’t a folder, so Pitboard saves no Claude Desktop sign-in there. \
                     Point PITBOARD_HOME at a folder of your own on this volume, then try again."
                ),
            };
            format!("{said} Nothing has changed.")
        }
        Error::DifferentVolume { parks, root } => format!(
            "Pitboard saves a Claude Desktop sign-in by moving it into {}, which is on another \
             volume than {}, and it never copies one. Keep Pitboard’s folder on the volume your \
             home is on. Nothing has changed.",
            parks.display(),
            root.display()
        ),
        Error::MoveUndone { label, detail } => format!(
            "Pitboard couldn’t save {label}’s sign-in ({detail}) and undid what it had done. \
             {label} is in use, and nothing was lost. Try again.",
            label = bare(label)
        ),
        Error::RecoveryUndetermined {
            tool, to, detail, ..
        } if inside(tool) => format!(
            "An earlier switch to {} didn’t finish, and Pitboard can’t tell where it stopped \
             ({detail}). Nothing has changed. Open {app} to see which account it shows, and quit \
             it. If it’s signed in, choose Add Account…, then Add the Account {app} Shows. If not, \
             choose Cancel Switch… on its notice.",
            bare(to),
            app = tool.program()
        ),
        Error::RecoveryElsewhere { tool, to, slot, .. } if inside(tool) => format!(
            "An earlier switch to {} didn’t finish while {} kept its data in {slot}. Nothing has \
             changed. Choose Cancel Switch… on its notice to keep every folder and go on.",
            bare(to),
            tool.program()
        ),
        Error::AppOpenedMidway {
            app,
            label: Some(label),
        } => format!(
            "{app} was opened while Pitboard switched it to {}, so Pitboard stopped. Nothing is \
             lost. Quit {app}, then try again: Pitboard finishes that switch first.",
            bare(label)
        ),
        Error::AppOpenedMidway { app, label: None } => format!(
            "{app} was opened while Pitboard moved the account name {app} kept out of the way, so \
             Pitboard stopped. Nothing is lost. Quit {app}, then try again."
        ),
        Error::MoveStopped { label, detail } => format!(
            "Pitboard stopped while switching {app} to {} ({detail}). Keep {app} closed and try \
             again: Pitboard finishes that switch first.",
            bare(label)
        ),
        Error::RecoveryStopped { label, detail } => format!(
            "Pitboard stopped while finishing an earlier switch to {} ({detail}). Nothing is \
             lost. Keep {app} closed and try again. If it stops again, choose Cancel Switch… on \
             its notice.",
            bare(label)
        ),
        Error::SwitchLeftSignedOut {
            from: Some(from),
            to,
            detail,
        } => format!(
            "{to} couldn’t be put in use ({detail}), so {app} is signed out. Both sign-ins are \
             saved: switch to {to} to try again, or to {from} to go back.",
            from = bare(from),
            to = bare(to)
        ),
        Error::SwitchLeftSignedOut {
            from: None,
            to,
            detail,
        } => format!(
            "{to} couldn’t be put in use ({detail}), so {app} is signed out. Its sign-in is still \
             saved: switch to {to} to try again.",
            to = bare(to)
        ),
        Error::MoveMixed { label, places } => format!(
            "Cancelling would leave parts of {} beside another sign-in in {app}’s folder: {}. \
             Nothing has changed. Open {app}, check which account it shows, quit it, then {}",
            bare(label),
            places.join(", "),
            add_what_it_shows(app)
        ),
        Error::SignInSettledElsewhere { .. } => {
            "This sign-in was finished, undone or removed elsewhere, so nothing was added.".into()
        }
        Error::NothingParked { tool, label } if inside(tool) => format!(
            "{} has no saved sign-in to switch to. Choose Sign In Again… on its row.",
            bare(label)
        ),
        Error::AccountUnknown { label, .. } if inside_label(label) => format!(
            "No {} account is added as {}. Choose Add Account… to add it.",
            Key::parse(label).provider.name(),
            bare(label)
        ),
        Error::ParkedCredentialMissing { label } if inside_label(label) => format!(
            "The saved sign-in for {0} is missing. Choose Sign In Again… on {0}’s row.",
            bare(label)
        ),
        Error::LiveLoginRefused {
            tool,
            label,
            detail,
        } => format!(
            "{} no longer accepts {}’s sign-in in use ({detail}), so putting {}’s sign-in in its \
             place loses nothing.",
            tool.service(),
            tool.name(),
            bare(label)
        ),
        Error::IdentityUnverifiable {
            tool,
            login: Unidentified::Parked { label },
            cause,
            detail,
            ..
        } if !cause.worth_retrying() && *cause != Cause::AnswerNotUnderstood => format!(
            "Pitboard couldn’t confirm with {} whose sign-in is saved for {} ({detail}), so \
             nothing has changed.",
            tool.service(),
            bare(label)
        ),
        Error::IdentityUnverifiable {
            tool,
            login: Unidentified::NewSignIn,
            cause,
            detail,
            ..
        } => format!(
            "Pitboard couldn’t confirm with {} whose account you signed in to ({detail}), so \
             nothing was added. {}",
            tool.service(),
            match cause {
                Cause::Unreachable => "Check the connection and try again.",
                Cause::RateLimited => "Try again in a few minutes.",
                Cause::ServerError => "Try again later.",
                _ => "Sign in again.",
            }
        ),
        Error::AccountOnHold {
            tool,
            login: Unidentified::NewSignIn,
            ..
        } => format!(
            "{0} has put the account you signed in to on hold. Follow {0}’s instructions for it, \
             then try again.",
            tool.service()
        ),
        Error::IdentityUnverifiable {
            tool,
            login: Unidentified::Parked { label },
            cause,
            detail,
            ..
        } => format!(
            "Pitboard couldn’t confirm with {} whose sign-in is saved for {} ({detail}), so \
             nothing has changed. {}",
            tool.service(),
            bare(label),
            match cause {
                Cause::Unreachable => "Check the connection and try again.",
                Cause::RateLimited => "Try again in a few minutes.",
                Cause::ServerError => "Try again later.",
                _ => "Check for a Pitboard update.",
            }
        ),
        Error::ParkedCredentialCorrupt { label, detail } if inside_label(label) => format!(
            "The saved sign-in for {0} isn’t the one Pitboard saved ({detail}). Choose Sign In \
             Again… on {0}’s row.",
            bare(label)
        ),
        // Claude Code's and Codex's, whose way on the failure's next step offers.
        Error::AccountUnknown { label, .. } => format!(
            "No {} account is added as {}.",
            Key::parse(label).provider.name(),
            bare(label)
        ),
        Error::LabelTaken { label, who } => format!(
            "{} is already the name of {who}. Type another name.",
            bare(label)
        ),
        Error::NothingParked { tool, label } => format!(
            "{} has no saved sign-in to switch to: the last one went back into use and {} has \
             moved on from it.",
            bare(label),
            tool.name()
        ),
        Error::ParkedLoginExpired { label } => {
            format!("The saved sign-in for {} has expired.", bare(label))
        }
        Error::ParkedCredentialMissing { label } => {
            format!("The saved sign-in for {} is missing.", bare(label))
        }
        Error::ParkedCredentialCorrupt { label, detail } => format!(
            "The saved sign-in for {} isn’t the one Pitboard saved ({detail}).",
            bare(label)
        ),
        Error::ParkedLoginRefused { tool, label } => format!(
            "{} no longer accepts {}’s saved sign-in, so Pitboard switched nothing and deleted \
             it.",
            tool.service(),
            bare(label)
        ),
        Error::ParkedLoginBelongsElsewhere { label, who } => format!(
            "{label}’s saved sign-in belongs to {who}, not to the account Pitboard has under that \
             name. Nothing was switched.",
            label = bare(label)
        ),
        Error::CannotForgetActiveAccount { label } => format!(
            "{} is in use. Switch to another account before removing it.",
            bare(label)
        ),
        Error::ParkSlotExhausted => "Pitboard found no free place to save this sign-in.".into(),
        Error::SignInNotKept {
            tool,
            label,
            detail,
        } => format!(
            "The new sign-in for {} couldn’t be put in use ({detail}), so it wasn’t kept, and {} \
             keeps the one it has.",
            bare(label),
            tool.name()
        ),
        Error::SignInIncomplete => "The sign-in didn’t finish, so nothing was added.".into(),
        Error::SignInNotInstalled {
            label,
            detail,
            parked,
            ..
        } => {
            let then = if *parked {
                "The new sign-in is saved, so it isn’t lost."
            } else {
                "It couldn’t be saved either, so sign in to it again."
            };
            format!(
                "Pitboard signed in to {} again and couldn’t confirm its new sign-in took the \
                 place of the one in use ({detail}). {then}",
                bare(label)
            )
        }
        Error::SwitchDidNotHold { tool, to, .. } => match Tool::from(*tool) {
            Tool::Claude => format!(
                "A /logout in Claude Code removed {to}’s sign-in before the switch finished. \
                 Both sign-ins are saved: sign in to any account you’ve added in Claude Code, \
                 then switch to {to}.",
                to = bare(to)
            ),
            Tool::Codex => format!(
                "A codex session still running rewrote Codex’s sign-in during the switch, and \
                 {}’s is still saved. Quit every running codex before you sign in or out of \
                 Codex: either ends the sign-in it finds.",
                bare(to)
            ),
            Tool::Desktop => "Claude was opened before the switch finished and wrote its own \
                              sign-in back. Quit Claude, then check which account it shows."
                .into(),
        },
        Error::SignInInProgress => "Another sign-in is already waiting in a terminal. Finish or \
                                    cancel it there first."
            .into(),
        Error::LiveAccountNotEnrolled { tool, who } => format!(
            "{who} is signed in to {} and isn’t added, so Pitboard can’t switch away from it.",
            tool.name()
        ),
        Error::AlreadyEnrolled { email, label, .. } => format!(
            "{email} is added already, as {}. Sign in to a different account to add another.",
            bare(label)
        ),
        Error::LiveCredentialShapeUnexpected { tool, detail } => format!(
            "The sign-in in use isn’t shaped like a {} sign-in ({detail}).",
            tool.name()
        ),
        Error::RenewalFailed { label, detail, .. } => format!(
            "Pitboard couldn’t refresh {}’s saved sign-in ({detail}), so its last reading is \
             shown.",
            bare(label)
        ),
        Error::ReplaceFailed {
            tool,
            label,
            detail,
        }
        | Error::InstallFailed {
            tool,
            label,
            detail,
        } => format!(
            "Pitboard couldn’t put {label}’s sign-in where {} reads it ({detail}). {label}’s \
             sign-in is still saved, and nothing was lost.",
            tool.name(),
            label = bare(label)
        ),
        Error::SwitchCorrupted {
            tool,
            from,
            to,
            detail,
            ..
        } => format!(
            "Pitboard couldn’t switch to {to}, or back to {from} ({detail}). {from}’s sign-in is \
             still saved: sign in to any account you’ve added in {}, then switch to {from}.",
            tool.name(),
            from = bare(from),
            to = bare(to)
        ),
        Error::StateCorrupt { path, source } => format!(
            "Pitboard’s account list at {} is damaged ({source}). Deleting it starts over, and \
             each account then needs adding again.",
            path.display()
        ),
        Error::StateVersionUnknown { path, found } => format!(
            "{} is in a format ({found}) no Pitboard has written. Deleting it starts over, and \
             each account then needs adding again.",
            path.display()
        ),
        Error::StateWrongMachine { path } => format!(
            "{} was written on another computer, and saved sign-ins don’t move between computers.",
            path.display()
        ),
        Error::StateOnSyncedDrive { path, marker } => format!(
            "{} looks like it’s inside {marker}, which syncs to other computers. Saved sign-ins \
             belong to one computer, so set PITBOARD_HOME to a local folder.",
            path.display()
        ),
        Error::LeftLoginNotEnrolled { path, email, .. } => format!(
            "{} holds {email}’s sign-in, an account you haven’t added, so Pitboard can’t clean \
             it up. Add that account first.",
            path.display()
        ),
        Error::UninstallWouldStrand {
            label: Some(label), ..
        } => format!(
            "No Claude Desktop account is in use, and {label}’s sign-in is saved, so uninstalling \
             would delete every Claude Desktop sign-in Pitboard holds. Switch to {label} first, \
             or remove each Claude Desktop account.",
            label = bare(label)
        ),
        Error::UninstallWouldStrand {
            label: None,
            set_aside,
        } => {
            let held = match set_aside.as_slice() {
                [_] => "holds a Claude Desktop sign-in",
                _ => "hold Claude Desktop sign-ins",
            };
            format!(
                "No Claude Desktop account is in use, and {} {held} Pitboard moved out of the way, \
                 so uninstalling would delete them. Add an account you keep first.",
                listed(set_aside.iter().map(|path| path.display().to_string()))
            )
        }
        _ => return None,
    };
    Some(typeset(&said))
}

// Its notice offers to sign in again, so the words name no command.
fn replaced(tool: ProviderId, label: &str, now: &Stored) -> String {
    let label = bare(label);
    let name = tool.name();
    let has = match now {
        Stored::Account(now) => format!(": {name} is now signed in to {}", bare(now)),
        Stored::Unenrolled(email) if email.is_empty() => {
            format!(": {name} is now signed in to an account you haven’t added")
        }
        Stored::Unenrolled(email) => format!(": {name} is now signed in to {email}"),
        Stored::Nothing => format!(": {name} is now signed out"),
        Stored::Unknown => String::new(),
    };
    format!(
        "{label}’s sign-in was replaced outside Pitboard{has}, and Pitboard has no sign-in saved \
         for {label}."
    )
}

pub(crate) fn in_app_warning(front: Front, warning: &CoreWarning) -> Option<String> {
    if let Some(said) = in_app_any_warning(front, warning) {
        return Some(typeset(&said));
    }
    let tool = warning.tool().filter(|&tool| in_its_app(tool))?;
    let app = tool.program();
    let add_what_it_shows = format!(
        "Open {app}, check which account it shows, quit it, then choose Add the Account {app} \
         Shows."
    );
    let named = |named: &Stored, last: &Stored| {
        let since = match last {
            Stored::Account(_) | Stored::Unenrolled(_) => format!(
                "since Pitboard last read {} in use, so {app} may hold another sign-in",
                stored(last)
            ),
            Stored::Nothing | Stored::Unknown => {
                format!("since Pitboard last read {app} signed out, so somebody may have signed in")
            }
        };
        format!(
            "{app}’s settings have named {} {since}. {add_what_it_shows}",
            stored(named)
        )
    };
    let said = match warning {
        CoreWarning::InUseUnconfirmed {
            named: by, last, ..
        } => named(by, last),
        CoreWarning::ConfigNamesAnother { config, in_use, .. } => named(config, in_use),
        CoreWarning::LoginReplaced {
            label,
            now: Stored::Nothing,
            ..
        } => format!(
            "{} was signed out in {app}, which ends its sign-in at claude.ai, and Pitboard has no \
             sign-in saved for it.",
            bare(label)
        ),
        CoreWarning::LoginReplaced { label, now, .. } => replaced(tool, label, now),
        CoreWarning::TakenAsNamed {
            label,
            recorded: None,
            ..
        } => format!(
            "Pitboard hadn’t seen {app}’s sign-in, and took it as {label}’s because {app} names \
             that account. If {app} showed another account, switch to {label}, check in {app}, \
             quit it, then choose Add Account… and Add the Account {app} Shows.",
            label = bare(label)
        ),
        CoreWarning::TakenAsNamed {
            label,
            recorded: Some(recorded),
            ..
        } => format!(
            "Pitboard had saved {app}’s sign-in as {}’s, and now takes it as {}’s because {app} \
             names that account. Open {app} and check which account it shows. If it’s another, \
             quit {app}, then choose Add Account… and Add the Account {app} Shows.",
            bare(recorded),
            bare(label)
        ),
        CoreWarning::LooksSignedOut { label, .. } => format!(
            "{app} looks signed out since Pitboard last read {} in use. Pitboard checks at the \
             next change to a {} account.",
            bare(label),
            tool.name()
        ),
        CoreWarning::SetAside {
            path, label, why, ..
        } => {
            let path = path.display();
            let label = label.as_deref().map(bare);
            match (why, label) {
                (SetAside::Superseded, Some(label)) => format!(
                    "Pitboard kept {label}’s earlier saved sign-in in {path} and deleted nothing."
                ),
                (SetAside::Superseded, None) => {
                    format!("Pitboard kept an earlier saved sign-in in {path} and deleted nothing.")
                }
                (SetAside::GivenUp, Some(label)) => format!(
                    "Pitboard moved the parts of {label}’s sign-in left in {app}’s folder into \
                     {path} and deleted nothing."
                ),
                (SetAside::GivenUp, None) => format!(
                    "Pitboard moved the parts of a sign-in left in {app}’s folder into {path} and \
                     deleted nothing."
                ),
                (SetAside::InTheWay | SetAside::AnotherMachine | SetAside::Orphan, _) => {
                    format!(
                        "Pitboard moved what it found in the way into {path} and deleted nothing."
                    )
                }
            }
        }
        CoreWarning::KeysSetAside { path, label, .. } => format!(
            "{app} rewrote {}’s keys. Pitboard kept the earlier ones in {} and deleted nothing.",
            label.as_deref().map_or_else(|| "a sign-in".into(), bare),
            path.display()
        ),
        CoreWarning::MoveSettled(settled) => {
            use pitboard_core::state::Half;
            use pitboard_core::switch::Ended;
            let label = bare(&settled.label);
            match (settled.half, settled.ended) {
                (Half::Leave, Ended::Undone) => {
                    format!("Pitboard undid it, and {label} is in use.")
                }
                (Half::Leave, Ended::Finished) => {
                    format!("Pitboard saved {label}’s sign-in, and {app} is signed out.")
                }
                (Half::Arrive, Ended::Undone) => {
                    format!("Pitboard saved {label}’s sign-in again, and {app} is signed out.")
                }
                (Half::Arrive, Ended::Finished) => {
                    format!("The switch to {label} had finished after all.")
                }
            }
        }
        CoreWarning::MoveWaiting { label, .. } => format!(
            "A switch to {} didn’t finish. Quit {app}, then switch to any {} account: Pitboard \
             finishes the earlier switch first.",
            bare(label),
            tool.name()
        ),
        CoreWarning::SignInAwaited {
            label,
            put_back,
            again,
            ..
        } => awaiting_lines(app, &bare(label), &bare(put_back), *again).join(" "),
        CoreWarning::MoveNotGivenUp { label, why, .. } => {
            not_given_up(front, tool, &bare(label), why)
        }
        _ => return None,
    };
    Some(typeset(&said))
}

// The app's words for a warning about Claude Code or Codex, or about any tool alike.
fn in_app_any_warning(front: Front, warning: &CoreWarning) -> Option<String> {
    use pitboard_core::holder;
    use pitboard_core::provider::Held;
    let inside = |tool: &ProviderId| in_its_app(*tool);
    Some(match warning {
        CoreWarning::Recovered(recovered) => {
            let from = if recovered.from.is_empty() {
                String::new()
            } else {
                format!(" from {}", bare(&recovered.from))
            };
            let to = bare(&recovered.to);
            if recovered.finished {
                format!("The switch{from} to {to} had finished after all, and nothing was lost.")
            } else {
                format!("The switch{from} to {to} hadn’t finished, and nothing was lost.")
            }
        }
        CoreWarning::LoginReplaced {
            tool, label, now, ..
        } if !inside(tool) => replaced(*tool, label, now),
        CoreWarning::ParksPendingRemoval(1) => {
            "1 old saved sign-in couldn’t be deleted yet. Pitboard tries again at its next change."
                .into()
        }
        CoreWarning::ParksPendingRemoval(count) => format!(
            "{count} old saved sign-ins couldn’t be deleted yet. Pitboard tries again at its next \
             change."
        ),
        CoreWarning::ParkedLoginRefused { tool, label } => format!(
            "{} no longer accepts {}’s saved sign-in. {}",
            tool.service(),
            bare(label),
            sign_in_to_keep(&bare(label))
        ),
        CoreWarning::LockCompromised { tool } => format!(
            "{} took back its lock during this change and may have written a sign-in at the same \
             time. Check that the right account is in use.",
            tool.name()
        ),
        CoreWarning::FallbackLogin {
            tool,
            path,
            held: Held::Login,
            ..
        } => format!(
            "{} holds another {} sign-in, which sessions already running keep after a switch.",
            path.display(),
            tool.name()
        ),
        CoreWarning::FallbackLogin {
            tool,
            path,
            held: Held::NoLogin,
            ..
        } => format!(
            "{} holds no {} sign-in, and sessions already running don’t follow a switch while it’s \
             there.",
            path.display(),
            tool.name()
        ),
        CoreWarning::FallbackLogin {
            path,
            held: Held::Unreadable,
            ..
        } => format!(
            "Pitboard couldn’t read {}, and sessions already running don’t follow a switch while \
             it’s there.",
            path.display()
        ),
        CoreWarning::SessionsStillRunning { from, holding } => format!(
            "{} started before this switch {} still running and still using {from}. {} Don’t \
             sign out in {}: that ends {from}’s sign-in, which Pitboard just saved.",
            capitalised(&holder::described(holding)),
            if holder::plural(holding) { "are" } else { "is" },
            holder::remedies(holding, "to use the new account"),
            if holder::plural(holding) {
                "any of them"
            } else {
                "it"
            },
            from = bare(from)
        ),
        CoreWarning::SessionsUnknown { tool, from } => format!(
            "Pitboard couldn’t tell whether {} sessions started before this switch are still \
             running. Don’t sign out in one that is: that ends {from}’s sign-in, which Pitboard \
             just saved.",
            tool.name(),
            from = bare(from)
        ),
        CoreWarning::SignInParkedNotInUse {
            tool, label, why, ..
        } => format!(
            "{} goes on with the sign-in it has: Pitboard couldn’t tell whose it is ({why}), so \
             it saved the new sign-in for {} instead of writing over that one.",
            tool.name(),
            bare(label)
        ),
        CoreWarning::RenewalFailed(error)
        | CoreWarning::ConfigNotUpdated(error)
        | CoreWarning::SwitchStuck(error) => in_app_refusal(front, error)?,
        _ => return None,
    })
}

// After Cancel Switch cancelled a switch, so never that nothing has changed.
fn not_given_up(front: Front, tool: ProviderId, label: &str, why: &Error) -> String {
    let app = tool.program();
    let settles = format!(
        "switch to any {} account: Pitboard finishes the earlier switch first",
        tool.name()
    );
    let because = match why {
        Error::MoveMixed { places, .. } => Some(format!(
            "cancelling it would leave parts of it beside another sign-in in {app}’s folder: {}",
            places.join(", ")
        )),
        Error::AppOpen { app, still } if still.is_empty() => Some(format!("{app} is open")),
        Error::AppOpen { app, still } => Some(format!(
            "{} from {app} is still running",
            listed(still.clone())
        )),
        Error::AppStateUnknown { app, why } => {
            Some(format!("it couldn’t tell whether {app} is open ({why})"))
        }
        Error::LoginFolderBusy { app } => {
            Some(format!("another Pitboard is changing {app}’s sign-in"))
        }
        Error::LoginFolderUnreadable { tool, detail } => Some(format!(
            "it couldn’t read {}’s cookie store ({detail})",
            tool.program()
        )),
        Error::AppOpenedMidway { app, .. } => Some(format!("{app} was opened meanwhile")),
        _ => why.unread_park().map(|(path, why)| {
            format!(
                "{label}’s saved sign-in at {} couldn’t be looked at ({why})",
                path.display()
            )
        }),
    };
    let then = match why {
        Error::AppOpen { still, .. } if still.is_empty() => format!("Quit {app}, then {settles}."),
        Error::AppOpen { .. } => pitboard_core::words::wait_for_still(front.os(), &settles),
        Error::LoginFolderBusy { .. } => format!("Once it has finished, {settles}."),
        Error::LoginFolderUnreadable { .. } => format!("Open {app}, quit it, then {settles}."),
        Error::AppOpenedMidway { .. } => format!("Nothing is lost. Quit {app}, then {settles}."),
        Error::MoveMixed { .. } => format!(
            "Open {app}, check which account it shows, quit it, then {}",
            add_what_it_shows(app)
        ),
        Error::Store(_) if because.is_some() => pitboard_core::words::readable_first(&settles),
        _ if because.is_some() => format!("To try again, {settles}."),
        _ => format!("Quit {app}, then {settles}."),
    };
    match because {
        Some(because) => format!(
            "Pitboard didn’t cancel the switch to {label} that didn’t finish, because {because}. \
             {then}"
        ),
        None => {
            format!("Pitboard couldn’t cancel the switch to {label} that didn’t finish. {then}")
        }
    }
}

pub(crate) fn in_app_given_up(given_up: &pitboard_core::switch::GivenUp) -> String {
    let app = given_up.tool.program();
    let way_on = format!(
        "choose Add Account… and Add the Account {app} Shows for the account {app} shows, or \
         Switch on an account where {app} is signed out."
    );
    let mut said = format!(
        "Pitboard cancelled the switch to {} and kept every folder.",
        bare(&given_up.label)
    );
    if let Some(slot) = &given_up.slot {
        said.push_str(&format!(
            " The switch was in {slot}, which Pitboard didn’t read."
        ));
    }
    if given_up.left.is_empty() {
        said.push_str(&format!(" {}", capitalised(&way_on)));
    } else {
        let stay = if given_up.left.len() == 1 {
            "stays"
        } else {
            "stay"
        };
        said.push_str(&format!(
            " {} {stay} in {app}’s folder. Open {app} once and quit it with Command-Q, then \
             {way_on}",
            listed(given_up.left.iter().map(|item| quoted(item)))
        ));
    }
    typeset(&said)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::present::words::used_then;
    use pitboard_core::host::SecretStore;

    // The sheet asks for a name, so its refusal says name where the command line says label.
    #[test]
    fn a_name_already_taken_is_refused_as_a_name() {
        let taken = |who: &str| {
            in_app_refusal(
                Front::WindowsNotificationArea,
                &Error::LabelTaken {
                    label: "claude/work".into(),
                    who: who.into(),
                },
            )
        };
        assert_eq!(
            taken("dana@work.example").as_deref(),
            Some("work is already the name of dana@work.example. Type another name.")
        );
        assert_eq!(
            taken("another Claude Desktop account").as_deref(),
            Some("work is already the name of another Claude Desktop account. Type another name.")
        );
    }

    // As its row, the menu and the rename field write it.
    #[test]
    fn a_label_keeps_every_letter_in_a_sentence() {
        let said = in_app_warning(
            Front::MacMenuBar,
            &CoreWarning::LooksSignedOut {
                tool: ProviderId::Desktop,
                label: "desktop/dana's".into(),
            },
        )
        .expect("said");
        assert!(said.contains(" read dana's in use."), "{said}");
        let said = used_then("cannot", "96% of its 5-hour limit", "");
        assert!(said.starts_with("cannot has used "), "{said}");
    }

    #[test]
    fn a_saved_sign_in_refused_is_named_as_the_app_names_its_account() {
        assert_eq!(
            in_app_refusal(
                Front::MacMenuBar,
                &Error::ParkedCredentialMissing {
                    label: "desktop/home".into(),
                }
            )
            .as_deref(),
            Some("The saved sign-in for home is missing. Choose Sign In Again… on home’s row.")
        );
        assert_eq!(
            in_app_refusal(
                Front::MacMenuBar,
                &Error::ParkedCredentialCorrupt {
                    label: "desktop/home".into(),
                    detail: "it was parked on another Mac".into(),
                }
            )
            .as_deref(),
            Some(
                "The saved sign-in for home isn’t the one Pitboard saved (it was parked on another \
                 Mac). Choose Sign In Again… on home’s row."
            )
        );
        assert_eq!(
            in_app_refusal(
                Front::MacMenuBar,
                &Error::ParkedCredentialMissing {
                    label: "work".into(),
                }
            )
            .as_deref(),
            Some("The saved sign-in for work is missing."),
            "Claude Code's says what is wrong, and its next step what to do"
        );
    }

    #[test]
    fn a_saved_sign_in_anthropic_will_not_name_is_signed_in_to_again_on_its_row() {
        let untold = |cause| Error::IdentityUnverifiable {
            tool: ProviderId::Claude,
            login: Unidentified::Parked {
                label: "dtg".into(),
            },
            cause,
            detail: "Anthropic answered 400".into(),
            spelling: pitboard_core::words::Spelling::Bare,
        };
        assert_eq!(
            in_app_refusal(Front::MacMenuBar, &untold(Cause::Refused)).as_deref(),
            Some(
                "Pitboard couldn’t confirm with Anthropic whose sign-in is saved for dtg \
                 (Anthropic answered 400), so nothing has changed."
            ),
            "the failure's Sign In Again is the way on"
        );
        assert_eq!(
            in_app_refusal(Front::MacMenuBar, &untold(Cause::Unreachable)).as_deref(),
            Some(
                "Pitboard couldn’t confirm with Anthropic whose sign-in is saved for dtg \
                 (Anthropic answered 400), so nothing has changed. Check the connection and try \
                 again."
            ),
            "an outage is tried again, not signed in to again"
        );
    }

    #[test]
    fn a_login_refused_for_good_says_replacing_it_loses_nothing() {
        let refused = Error::LiveLoginRefused {
            tool: ProviderId::Claude,
            label: "work".into(),
            detail: "its login has expired and cannot be renewed".into(),
        };
        assert_eq!(
            in_app_refusal(Front::MacMenuBar, &refused).as_deref(),
            Some(
                "Anthropic no longer accepts Claude Code’s sign-in in use (its login has expired \
                 and can’t be renewed), so putting work’s sign-in in its place loses nothing."
            )
        );
    }

    #[test]
    fn claudes_settings_naming_another_account_are_said_as_the_command_line_says_them() {
        let unconfirmed = CoreWarning::InUseUnconfirmed {
            tool: ProviderId::Desktop,
            last: Stored::Account("desktop/work".into()),
            named: Stored::Unenrolled(String::new()),
            asked: false,
        };
        assert_eq!(
            in_app_warning(Front::MacMenuBar, &unconfirmed).as_deref(),
            Some(
                "Claude’s settings have named an account you haven’t added since Pitboard last \
                 read work in use, so Claude may hold another sign-in. Open Claude, check which \
                 account it shows, quit it, then choose Add the Account Claude Shows."
            )
        );
    }

    #[test]
    fn a_list_in_the_apps_words_has_no_serial_comma() {
        let still = Error::AppOpen {
            app: "Claude",
            still: vec![
                "ShipIt (pid 812)".into(),
                "Claude Helper (pid 813)".into(),
                "chrome_crashpad_handler (pid 814)".into(),
            ],
        };
        let said = in_app_refusal(Front::MacMenuBar, &still).expect("in the app's words");
        assert!(
            said.contains(
                "ShipIt (pid 812), Claude Helper (pid 813) and chrome_crashpad_handler (pid 814) \
                 from it"
            ),
            "{said}"
        );
    }

    #[test]
    fn the_app_says_to_quit_what_is_left_of_an_app_in_task_manager_on_windows() {
        let still = |app| Error::AppOpen {
            app,
            still: vec!["Helper (pid 812)".into()],
        };
        for (front, monitor) in [
            (Front::WindowsNotificationArea, "Task Manager"),
            (Front::MacMenuBar, "Activity Monitor"),
        ] {
            assert_eq!(
                in_app_refusal(front, &still("ChatGPT")).expect("the app's words"),
                format!(
                    "ChatGPT has quit, and Helper (pid 812) from it is still running, and \
                     Pitboard changes ChatGPT’s sign-in only while all of it is closed. Wait until \
                     it ends, or quit it in {monitor}, then try again. Nothing has changed."
                )
            );
            let not_given_up = CoreWarning::MoveNotGivenUp {
                tool: ProviderId::Desktop,
                label: "desktop/work".into(),
                why: still("Claude"),
            };
            assert_eq!(
                in_app_warning(front, &not_given_up).expect("the app's words"),
                format!(
                    "Pitboard didn’t cancel the switch to work that didn’t finish, because Helper \
                     (pid 812) from Claude is still running. Wait until it ends, or quit it in \
                     {monitor}, then switch to any Claude Desktop account: Pitboard finishes the \
                     earlier switch first."
                )
            );
        }
    }

    #[test]
    fn a_switch_in_claude_not_cancelled_over_a_sign_in_it_could_not_look_at_says_why_and_how_out() {
        let why = pitboard_core::testing::park_unread(
            "desktop/work",
            std::path::Path::new("/Users/me/.pitboard/parks/a1"),
            "permission denied",
        );
        assert_eq!(
            in_app_warning(
                Front::MacMenuBar,
                &CoreWarning::MoveNotGivenUp {
                    tool: ProviderId::Desktop,
                    label: "desktop/work".into(),
                    why,
                },
            )
            .expect("the app's words"),
            "Pitboard didn’t cancel the switch to work that didn’t finish, because work’s saved \
             sign-in at /Users/me/.pitboard/parks/a1 couldn’t be looked at (permission denied). \
             Make that path, and each folder it is in, readable to you, then switch to any Claude \
             Desktop account: Pitboard finishes the earlier switch first."
        );
    }

    #[test]
    fn a_switch_in_claude_cancelled_is_summed_up_as_the_command_line_sums_it_up() {
        let given_up = pitboard_core::switch::GivenUp {
            tool: ProviderId::Desktop,
            label: "work".into(),
            parked: 1,
            set_aside: 0,
            left: vec!["Session Storage".into()],
            slot: Some("/Volumes/Old/Claude".into()),
        };
        assert_eq!(
            in_app_given_up(&given_up),
            "Pitboard cancelled the switch to work and kept every folder. The switch was in \
             /Volumes/Old/Claude, which Pitboard didn’t read. Session Storage stays in Claude’s \
             folder. Open Claude once and quit it with Command-Q, then choose Add Account… and \
             Add the Account Claude Shows for the account Claude shows, or Switch on an account \
             where Claude is signed out."
        );
        assert!(given_up.to_string().starts_with(
            "Pitboard gave up the move of `desktop/work`'s login and kept every folder: 1 parked, \
             0 set aside. The move was in /Volumes/Old/Claude, which Pitboard did not read. \
             `Session Storage` stays in Claude's folder. Open Claude once and quit it with \
             Command-Q, then run `pitboard enroll desktop/<label>`"
        ));
    }

    #[test]
    fn a_switch_in_claude_not_cancelled_never_says_nothing_has_changed() {
        let tool = ProviderId::Desktop;
        let app = "Claude";
        let said = |why| {
            in_app_warning(
                Front::MacMenuBar,
                &CoreWarning::MoveNotGivenUp {
                    tool,
                    label: "desktop/work".into(),
                    why,
                },
            )
            .expect("the app's words")
        };
        assert_eq!(
            said(Error::AppOpen {
                app,
                still: Vec::new()
            }),
            "Pitboard didn’t cancel the switch to work that didn’t finish, because Claude is open. \
             Quit Claude, then switch to any Claude Desktop account: Pitboard finishes the earlier \
             switch first."
        );
        for why in [
            Error::LoginFolderBusy { app },
            Error::MoveMixed {
                label: "desktop/work".into(),
                places: vec!["`Session Storage` in Claude's folder".into()],
            },
            Error::MachineUnknown,
        ] {
            let sentence = said(why);
            assert!(
                !sentence.to_lowercase().contains("nothing has changed"),
                "{sentence}"
            );
        }
    }

    // On the session's PC (2026-10-18) the banner told the app to run `pitboard enroll a
    // --sign-in` beside the row's own Sign in again… button. Its notice offers that sign-in now.
    #[test]
    fn a_login_replaced_outside_names_no_command_and_no_button() {
        use pitboard_core::state::Key;
        let replaced = |tool: ProviderId, now: Stored| CoreWarning::LoginReplaced {
            tool,
            id: "claude_5b1e".into(),
            label: Key::new(tool, "a").typed(),
            now,
        };
        assert_eq!(
            in_app_warning(
                Front::WindowsNotificationArea,
                &replaced(
                    ProviderId::Claude,
                    Stored::Account(Key::new(ProviderId::Claude, "b").typed())
                )
            )
            .as_deref(),
            Some(
                "a’s sign-in was replaced outside Pitboard: Claude Code is now signed in to b, and \
                 Pitboard has no sign-in saved for a."
            )
        );
        assert_eq!(
            in_app_warning(
                Front::MacMenuBar,
                &replaced(ProviderId::Codex, Stored::Nothing)
            )
            .as_deref(),
            Some(
                "a’s sign-in was replaced outside Pitboard: Codex is now signed out, and Pitboard \
                 has no sign-in saved for a."
            )
        );
        for tool in [ProviderId::Claude, ProviderId::Codex, ProviderId::Desktop] {
            for now in [
                Stored::Unenrolled(String::new()),
                Stored::Unenrolled("x@example.com".into()),
                Stored::Unknown,
                Stored::Nothing,
            ] {
                let said = in_app_warning(Front::WindowsNotificationArea, &replaced(tool, now))
                    .expect("the app's");
                assert!(!said.contains("pitboard "), "{said}");
                assert!(!said.contains("Sign in again"), "{said}");
            }
        }
    }

    // A label 0.1.x wrote can hold a slash, and is typed whole where what comes before it names
    // no tool.
    #[test]
    fn a_label_with_a_slash_from_0_1_is_said_whole() {
        let replaced = CoreWarning::LoginReplaced {
            tool: ProviderId::Claude,
            id: "claude_5b1e".into(),
            label: "team/a".into(),
            now: Stored::Nothing,
        };
        assert_eq!(
            in_app_warning(Front::MacMenuBar, &replaced).as_deref(),
            Some(
                "team/a’s sign-in was replaced outside Pitboard: Claude Code is now signed out, and \
                 Pitboard has no sign-in saved for team/a."
            )
        );
    }

    // Its notice's Show Diagnostics opens the checks that list it.
    #[test]
    fn what_claude_desktop_set_aside_is_said_without_where_it_is_listed() {
        let tool = ProviderId::Desktop;
        let path = std::path::PathBuf::from("/p/parks/x");
        let labels = [Some("desktop/home".to_owned()), None];
        let mut warnings: Vec<CoreWarning> = labels
            .iter()
            .flat_map(|label| {
                [SetAside::Superseded, SetAside::GivenUp].map(|why| CoreWarning::SetAside {
                    tool,
                    path: path.clone(),
                    label: label.clone(),
                    why,
                })
            })
            .collect();
        warnings.extend(labels.iter().map(|label| CoreWarning::KeysSetAside {
            tool,
            path: path.clone(),
            label: label.clone(),
        }));
        for front in [Front::MacMenuBar, Front::WindowsNotificationArea] {
            for warning in &warnings {
                let said = in_app_warning(front, warning).expect("Claude Desktop's words");
                assert!(said.ends_with("deleted nothing."), "{said}");
            }
        }
    }

    #[test]
    fn no_claude_desktop_sentence_in_the_app_names_a_command() {
        use pitboard_core::error::Enrolled;
        use pitboard_core::state::Half;
        use pitboard_core::switch::{Ended, GivenUp, MoveSettled};
        use std::path::PathBuf;
        let tool = ProviderId::Desktop;
        let app = "Claude";
        let typed = |label: &str| format!("desktop/{label}");
        let mut errors = vec![
            Error::AppOpen {
                app,
                still: Vec::new(),
            },
            Error::AppOpen {
                app,
                still: vec!["ShipIt (pid 812)".into()],
            },
            Error::AppStateUnknown {
                app,
                why: "`ps` did not answer within 5 seconds".into(),
            },
            Error::LoginFolderBusy { app },
            Error::LoginFolderUnreadable {
                tool,
                detail: "database is locked".into(),
            },
            Error::SignInUnfinished { tool },
            Error::LoginInDoubt {
                tool,
                why: LoginDoubt::NamedAnother {
                    recorded: typed("work"),
                    named: Some(typed("home")),
                },
            },
            Error::LoginInDoubt {
                tool,
                why: LoginDoubt::NamedAnother {
                    recorded: typed("work"),
                    named: None,
                },
            },
            Error::LoginInDoubt {
                tool,
                why: LoginDoubt::SignedInOutside {
                    label: typed("home"),
                },
            },
            Error::DifferentVolume {
                parks: PathBuf::from("/Volumes/Data/.pitboard/parks"),
                root: PathBuf::from("/Users/dana/Library/Application Support/Claude"),
            },
            Error::MachineUnknown,
            Error::AppOpenedMidway {
                app,
                label: Some(typed("work")),
            },
            Error::AppOpenedMidway { app, label: None },
            Error::MoveUndone {
                label: typed("work"),
                detail: "a rename failed".into(),
            },
            Error::MoveStopped {
                label: typed("work"),
                detail: "a rename failed".into(),
            },
            Error::RecoveryStopped {
                label: typed("work"),
                detail: "a rename failed".into(),
            },
            Error::SwitchLeftSignedOut {
                from: Some(typed("work")),
                to: typed("home"),
                detail: "a rename failed".into(),
            },
            Error::SwitchLeftSignedOut {
                from: None,
                to: typed("home"),
                detail: "a rename failed".into(),
            },
            Error::MoveMixed {
                label: typed("work"),
                places: vec!["`Session Storage` in Claude's folder".into()],
            },
            Error::SignInSettledElsewhere {
                label: typed("travel"),
            },
            Error::RecoveryUndetermined {
                tool,
                from: typed("work"),
                to: typed("work"),
                detail: "the jar is gone".into(),
                spelling: pitboard_core::words::Spelling::Bare,
            },
            Error::RecoveryElsewhere {
                tool,
                from: typed("work"),
                to: typed("work"),
                slot: "/Users/dana/Library/Application Support/Claude".into(),
            },
            Error::LiveStoreUnsupported {
                tool,
                reason: "Pitboard switches Claude Desktop accounts on macOS only".into(),
            },
            Error::LiveCredentialAbsent {
                tool,
                label: Some(typed("travel")),
                spelling: pitboard_core::words::Spelling::Bare,
            },
            Error::LiveAccountNotEnrolled {
                tool,
                who: String::new(),
            },
            Error::LabelTaken {
                label: typed("home"),
                who: "another Claude Desktop account".into(),
            },
            Error::NothingParked {
                tool,
                label: typed("home"),
            },
            Error::ParkedCredentialMissing {
                label: typed("home"),
            },
            Error::ParkedCredentialCorrupt {
                label: typed("home"),
                detail: "it was parked on another Mac, and Claude cannot read it here where its \
                         key differs"
                    .into(),
            },
            Error::CannotForgetActiveAccount {
                label: typed("work"),
            },
            Error::AccountUnknown {
                label: typed("home"),
                enrolled: Enrolled(vec![typed("work")]),
            },
            Error::AccountNotInUse {
                tool,
                label: typed("home"),
            },
        ];
        for waited in [false, true] {
            errors.push(Error::AlreadyEnrolled {
                tool,
                email: String::new(),
                label: typed("home"),
                asked: typed("travel"),
                waited,
            });
        }
        for then in [
            Then::Nothing,
            Then::OpenAndQuit,
            Then::UpdatePitboard,
            Then::CopyAside,
            Then::SignInAndEnrol,
        ] {
            errors.push(Error::LoginFolderUnknown {
                tool,
                what:
                    "holds `Cookies-wal`, a part of the account's data this Pitboard has not read"
                        .into(),
                then,
            });
        }
        for why in [
            Private::Shared,
            Private::Link,
            Private::NotOwn,
            Private::NotADirectory,
        ] {
            errors.push(Error::ParksNotPrivate {
                path: PathBuf::from("/Users/dana/.pitboard/parks"),
                why,
            });
        }
        let path = PathBuf::from("/Users/dana/.pitboard/parks/pitboard-aside-1791000400123");
        let mut warnings = vec![
            CoreWarning::TakenAsNamed {
                tool,
                label: typed("work"),
                recorded: None,
            },
            CoreWarning::TakenAsNamed {
                tool,
                label: typed("home"),
                recorded: Some(typed("work")),
            },
            CoreWarning::LooksSignedOut {
                tool,
                label: typed("work"),
            },
            CoreWarning::MoveWaiting {
                tool,
                label: typed("work"),
            },
            CoreWarning::ItemGone {
                tool,
                label: typed("work"),
                item: "WebStorage".into(),
            },
        ];
        for again in [false, true] {
            warnings.push(CoreWarning::SignInAwaited {
                tool,
                label: typed("home"),
                put_back: typed("work"),
                since: 0,
                again,
            });
        }
        for why in [
            Error::AppOpen {
                app,
                still: Vec::new(),
            },
            Error::AppOpen {
                app,
                still: vec!["ShipIt (pid 812)".into()],
            },
            Error::AppStateUnknown {
                app,
                why: "ps failed".into(),
            },
            Error::LoginFolderBusy { app },
            Error::LoginFolderUnreadable {
                tool,
                detail: "a test held the jar".into(),
            },
            Error::AppOpenedMidway {
                app,
                label: Some(typed("work")),
            },
            Error::MoveMixed {
                label: typed("work"),
                places: vec!["`Session Storage` in Claude's folder".into()],
            },
            Error::MachineUnknown,
        ] {
            warnings.push(CoreWarning::MoveNotGivenUp {
                tool,
                label: typed("work"),
                why,
            });
        }
        for (half, ended) in [
            (Half::Leave, Ended::Undone),
            (Half::Leave, Ended::Finished),
            (Half::Arrive, Ended::Undone),
            (Half::Arrive, Ended::Finished),
        ] {
            warnings.push(CoreWarning::MoveSettled(MoveSettled {
                tool,
                label: "work".into(),
                half,
                ended,
            }));
        }
        for why in [
            SetAside::InTheWay,
            SetAside::Superseded,
            SetAside::AnotherMachine,
            SetAside::GivenUp,
            SetAside::Orphan,
        ] {
            for label in [Some(typed("home")), None] {
                warnings.push(CoreWarning::SetAside {
                    tool,
                    path: path.clone(),
                    label,
                    why,
                });
            }
        }
        for label in [Some(typed("home")), None] {
            warnings.push(CoreWarning::KeysSetAside {
                tool,
                path: path.clone(),
                label,
            });
        }
        let stored = [
            Stored::Account(typed("home")),
            Stored::Unenrolled(String::new()),
            Stored::Nothing,
            Stored::Unknown,
        ];
        for now in stored.clone() {
            warnings.push(CoreWarning::LoginReplaced {
                tool,
                id: "desktop_5b1e".into(),
                label: typed("work"),
                now,
            });
        }
        for (named, last) in stored
            .iter()
            .flat_map(|named| stored.iter().map(move |last| (named, last)))
        {
            warnings.push(CoreWarning::InUseUnconfirmed {
                tool,
                last: last.clone(),
                named: named.clone(),
                asked: true,
            });
            warnings.push(CoreWarning::ConfigNamesAnother {
                tool,
                config: named.clone(),
                in_use: last.clone(),
            });
        }
        let given_up = |left: Vec<String>, slot: Option<String>| GivenUp {
            tool,
            label: "work".into(),
            parked: 1,
            set_aside: 1,
            left,
            slot,
        };
        let mut said: Vec<String> = errors
            .iter()
            .map(|error| {
                in_app_refusal(Front::MacMenuBar, error).unwrap_or_else(|| error.to_string())
            })
            .collect();
        said.extend(
            warnings
                .iter()
                .inspect(|warning| assert_eq!(warning.tool(), Some(tool)))
                .map(|warning| {
                    in_app_warning(Front::MacMenuBar, warning)
                        .unwrap_or_else(|| warning.to_string())
                }),
        );
        said.push(in_app_given_up(&given_up(Vec::new(), None)));
        said.push(in_app_given_up(&given_up(
            vec!["Session Storage".into(), "WebStorage".into()],
            Some("/Volumes/Old/Claude".into()),
        )));
        for sentence in said {
            assert!(
                !sentence.contains("pitboard ") && !sentence.contains("run this again"),
                "{sentence}"
            );
        }
    }

    #[test]
    fn a_new_sign_in_pitboard_could_not_name_or_that_is_on_hold_says_nothing_was_added() {
        let untold = |cause| Error::IdentityUnverifiable {
            tool: ProviderId::Claude,
            login: Unidentified::NewSignIn,
            cause,
            detail: "Anthropic answered 400".into(),
            spelling: pitboard_core::words::Spelling::Bare,
        };
        assert_eq!(
            in_app_refusal(Front::WindowsNotificationArea, &untold(Cause::Refused)).as_deref(),
            Some(
                "Pitboard couldn’t confirm with Anthropic whose account you signed in to \
                 (Anthropic answered 400), so nothing was added. Sign in again."
            )
        );
        assert_eq!(
            in_app_refusal(Front::MacMenuBar, &untold(Cause::RateLimited)).as_deref(),
            Some(
                "Pitboard couldn’t confirm with Anthropic whose account you signed in to \
                 (Anthropic answered 400), so nothing was added. Try again in a few minutes."
            )
        );
        let held = Error::AccountOnHold {
            tool: ProviderId::Codex,
            login: Unidentified::NewSignIn,
            link: None,
        };
        assert_eq!(
            in_app_refusal(Front::MacMenuBar, &held).as_deref(),
            Some(
                "OpenAI has put the account you signed in to on hold. Follow OpenAI’s \
                 instructions for it, then try again."
            )
        );
    }

    #[test]
    fn a_switch_or_a_move_that_did_not_finish_is_said_in_the_apps_words() {
        use pitboard_core::state::Half;
        use pitboard_core::switch::{Ended, MoveSettled, Recovered};
        let recovered = |from: &str, finished| {
            CoreWarning::Recovered(Recovered {
                from: from.into(),
                to: "home".into(),
                finished,
            })
        };
        let said =
            |warning: &CoreWarning| in_app_warning(Front::MacMenuBar, warning).expect("the app's");
        assert_eq!(
            said(&recovered("work", true)),
            "The switch from work to home had finished after all, and nothing was lost."
        );
        assert_eq!(
            said(&recovered("", false)),
            "The switch to home hadn’t finished, and nothing was lost."
        );
        let settled = |half, ended| {
            said(&CoreWarning::MoveSettled(MoveSettled {
                tool: ProviderId::Desktop,
                label: "desktop/work".into(),
                half,
                ended,
            }))
        };
        assert_eq!(
            [
                settled(Half::Leave, Ended::Undone),
                settled(Half::Leave, Ended::Finished),
                settled(Half::Arrive, Ended::Undone),
                settled(Half::Arrive, Ended::Finished),
            ],
            [
                "Pitboard undid it, and work is in use.",
                "Pitboard saved work’s sign-in, and Claude is signed out.",
                "Pitboard saved work’s sign-in again, and Claude is signed out.",
                "The switch to work had finished after all.",
            ]
        );
    }

    #[test]
    fn a_saved_sign_in_its_service_refuses_or_a_file_left_behind_says_what_to_do() {
        assert_eq!(
            in_app_warning(
                Front::WindowsNotificationArea,
                &CoreWarning::ParkedLoginRefused {
                    tool: ProviderId::Claude,
                    label: "work".into(),
                }
            )
            .as_deref(),
            Some(
                "Anthropic no longer accepts work’s saved sign-in. Sign in to work again to keep \
                 switching to it."
            )
        );
        let fallback = |held| CoreWarning::FallbackLogin {
            tool: ProviderId::Claude,
            path: std::path::PathBuf::from("/h/.claude/.credentials.json"),
            store: SecretStore::Keychain,
            held,
        };
        assert_eq!(
            in_app_warning(
                Front::WindowsNotificationArea,
                &fallback(pitboard_core::provider::Held::Login)
            )
            .as_deref(),
            Some(
                "/h/.claude/.credentials.json holds another Claude Code sign-in, which sessions \
                 already running keep after a switch."
            )
        );
        assert_eq!(
            in_app_warning(
                Front::WindowsNotificationArea,
                &fallback(pitboard_core::provider::Held::NoLogin)
            )
            .as_deref(),
            Some(
                "/h/.claude/.credentials.json holds no Claude Code sign-in, and sessions already \
                 running don’t follow a switch while it’s there."
            )
        );
        assert_eq!(
            in_app_warning(
                Front::WindowsNotificationArea,
                &fallback(pitboard_core::provider::Held::Unreadable)
            )
            .as_deref(),
            Some(
                "Pitboard couldn’t read /h/.claude/.credentials.json, and sessions already \
                 running don’t follow a switch while it’s there."
            )
        );
    }
}
