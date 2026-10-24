use super::parks::ParkFact;
use super::{Check, commands, fail, ok, warn};
use crate::context::Context;
use crate::host::{Kind, Os};
use crate::in_use::Stored;
use crate::provider::{LoginKind, ProviderId};
use crate::state::{Key, State};
use crate::{store, switch, words};
use std::path::PathBuf;

// Read from the index, the settings and the data folder's first-level names: no jar, no process.
pub struct DesktopFacts {
    pub(super) root: Result<PathBuf, String>,
    // this account's own home, where `HOME` names another: only a release asks for it
    pub(super) own_home: Option<PathBuf>,
    pub(super) refused: Option<crate::provider::folder::Unfit>,
    pub(super) present: bool,
    // the sidecar, and whether `config.json` is there
    pub(super) settings:
        Option<Result<(crate::provider::folder::Sidecar, bool), crate::provider::folder::Unread>>,
    pub(super) unread: Vec<String>,
    pub(super) known: Option<crate::in_use::Known>,
    pub(super) signed_out: bool,
    pub(super) parks_dir: PathBuf,
    pub(super) private: Option<crate::host::Private>,
    // whether `parks` is on the data folder's volume, where both could be asked
    pub(super) one_volume: Option<bool>,
    // each folder park's name, and whether it is a folder there, or why it was not looked at
    pub(super) parked_here: Vec<(String, Result<bool, String>)>,
    pub(super) build: Option<String>,
}

pub(super) fn claude_desktop() -> Option<&'static dyn crate::provider::folder::Folder> {
    match crate::provider::of(ProviderId::Desktop) {
        LoginKind::Folder(app) => Some(app),
        LoginKind::Document(_) => None,
    }
}

pub(super) fn desktop_facts(
    ctx: &Context,
    state: &State,
    build: Option<String>,
) -> Option<DesktopFacts> {
    let tool = ProviderId::Desktop;
    let app = claude_desktop()?;
    let enrolled = crate::status::shown(state, tool);
    if !enrolled && !state.aside.iter().any(|aside| aside.tool == tool) {
        return None;
    }
    let root = app.root(ctx).map_err(|e| e.to_string());
    let mut facts = DesktopFacts {
        own_home: root
            .as_ref()
            .err()
            .and_then(|_| crate::provider::desktop::paths::home_elsewhere(ctx)),
        root: root.clone(),
        refused: None,
        present: false,
        settings: None,
        unread: Vec::new(),
        known: None,
        signed_out: false,
        parks_dir: store::folder::parks_dir(ctx),
        private: crate::host::fs::private_dir(&store::folder::parks_dir(ctx)).ok(),
        one_volume: root.as_ref().ok().and_then(|root| {
            crate::host::fs::same_volume(root, &store::folder::parks_dir(ctx)).ok()
        }),
        parked_here: state
            .accounts
            .iter()
            .filter_map(|a| a.parked.as_ref())
            .filter(|park| park.kind == crate::state::ParkKind::Folder)
            .map(|park| {
                let at = store::folder::at(ctx, &park.service);
                let folder = match std::fs::symlink_metadata(at) {
                    Ok(found) => Ok(found.is_dir()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
                    Err(e) => Err(e.to_string()),
                };
                (park.service.clone(), folder)
            })
            .collect(),
        build,
    };
    if let Ok(root) = &root {
        facts.refused = app.refuses(root);
        facts.present = root.is_dir();
    }
    // Nothing inside Claude's data folder is read until one of its accounts is enrolled.
    if let (true, Ok(root)) = (enrolled, &root) {
        let settings = app.settings(root);
        let (known, signed_out) = switch::folder::judged(state, app, settings.as_ref().ok());
        facts.known = Some(known);
        facts.signed_out = signed_out.is_some();
        facts.settings = Some(settings.map(|read| (read.sidecar, read.file.is_some())));
        facts.unread = switch::folder::unread_parts(app, root).unwrap_or_default();
    }
    Some(facts)
}

// An index from a Mac can hold Claude Desktop's accounts, which nothing on Windows or Linux
// changes. Only a build for tests gives Linux a Claude data folder.
pub(super) fn judge_desktop_off_macos(state: &State) -> Check {
    let held: Vec<String> = state
        .accounts
        .iter()
        .filter(|account| account.provider() == ProviderId::Desktop)
        .map(|account| format!("`{}`", state.typed(&account.key())))
        .collect();
    let holds = match held.is_empty() {
        true => "folders set aside for Claude Desktop".to_string(),
        false => words::listed(held),
    };
    Check {
        advice: format!("This index holds {holds}. Nothing here changes them."),
        ..ok(
            "desktop_data_folder",
            "Claude data folder",
            crate::provider::desktop::paths::MACOS_ONLY,
        )
    }
}

// Claude Desktop's section, after Codex's.
pub(super) fn judge_desktop(
    os: Os,
    facts: &DesktopFacts,
    state: &State,
    parks: &[ParkFact],
    now: i64,
    moment: &dyn Fn(i64, i64) -> String,
) -> Vec<Check> {
    use crate::provider::folder::{Sidecar, Unfit, Unread};
    let mut checks = Vec::new();
    let folder = "Claude data folder";
    match &facts.root {
        Err(reason) => {
            let advice = facts.own_home.as_ref().map_or_else(String::new, |own| {
                format!(
                    "Run Pitboard with `HOME` unset, or set to {}.",
                    own.display()
                )
            });
            checks.push(fail("desktop_data_folder", folder, reason, advice));
        }
        Ok(root) => {
            let shown = root.display();
            checks.push(match facts.refused {
                Some(unfit @ Unfit::Link) => fail(
                    "desktop_data_folder",
                    folder,
                    format!("{shown} {unfit}"),
                    "Pitboard moves no Claude Desktop login through a link.",
                ),
                Some(unfit @ Unfit::ThirdParty(_)) => fail(
                    "desktop_data_folder",
                    folder,
                    format!("{shown} {unfit}"),
                    "Pitboard moves no Claude Desktop login for a third-party deployment.",
                ),
                Some(unfit @ Unfit::ResetWaits) => fail(
                    "desktop_data_folder",
                    folder,
                    format!("{shown} {unfit}"),
                    "Open Claude to let the reset finish, then quit it with Command-Q.",
                ),
                None if facts.present => ok("desktop_data_folder", folder, shown.to_string()),
                None => warn(
                    "desktop_data_folder",
                    folder,
                    format!("{shown} does not exist"),
                    "Run `pitboard enroll desktop/<label> --sign-in`, then open Claude and sign \
                     in: Pitboard waits for it to quit.",
                ),
            });
        }
    }
    if let (Ok(root), Some(settings)) = (&facts.root, &facts.settings) {
        let name = "Claude settings";
        checks.push(match settings {
            Ok((Sidecar::Valid, _)) => ok(
                "desktop_settings",
                name,
                "config.json read, with a settings write Claude left, which Pitboard reads as \
                 Claude will",
            ),
            Ok((Sidecar::Stale, _)) => ok(
                "desktop_settings",
                name,
                "config.json read; a stale settings write beside it, which Claude drops when it \
                 opens",
            ),
            Ok((Sidecar::None, true)) => ok("desktop_settings", name, "config.json read"),
            Ok((Sidecar::None, false)) => ok("desktop_settings", name, "no config.json yet"),
            Err(Unread::Unknown { what, then }) => fail(
                "desktop_settings",
                name,
                format!("{} {what}", root.display()),
                then.said().trim_end(),
            ),
            Err(Unread::Unreadable(detail)) => fail("desktop_settings", name, detail.clone(), ""),
        });
        if facts.present {
            let name = "Claude data folder layout";
            checks.push(match facts.unread.as_slice() {
                [] => ok(
                    "desktop_layout",
                    name,
                    "each part of the account's data is one Pitboard moves",
                ),
                [part] => warn(
                    "desktop_layout",
                    name,
                    format!(
                        "holds `{part}`, a part of the account's data this Pitboard has not read"
                    ),
                    "Pitboard starts no move of a Claude Desktop login while it is there. Update \
                     Pitboard, or report it at https://github.com/datlechin/pitboard/issues if no \
                     release reads it yet.",
                ),
                parts => warn(
                    "desktop_layout",
                    name,
                    format!(
                        "holds {}, parts of the account's data this Pitboard has not read",
                        words::listed(parts.iter().map(|part| format!("`{part}`")))
                    ),
                    "Pitboard starts no move of a Claude Desktop login while they are there. \
                     Update Pitboard, or report them at \
                     https://github.com/datlechin/pitboard/issues if no release reads them yet.",
                ),
            });
        }
    }
    checks.extend(judge_desktop_login(facts, state, now, moment));
    let in_use = state.account_in_use(ProviderId::Desktop).map(|a| a.key());
    for fact in parks.iter().filter(|p| p.provider == ProviderId::Desktop) {
        checks.push(judge_desktop_park(facts, fact, now));
    }
    let typed = |label: &str| state.typed(&Key::new(ProviderId::Desktop, label));
    if let Some(moving) = state.moving(ProviderId::Desktop) {
        checks.push(warn(
            "desktop_move",
            "interrupted Claude Desktop move",
            format!(
                "a move of `{}`'s login was interrupted {}",
                typed(&moving.label),
                moment(moving.started_at, now)
            ),
            "Keep Claude closed: the next change to a Claude Desktop account settles it first. \
             If it cannot, `pitboard abandon` keeps every folder and moves on.",
        ));
    }
    if let Some(awaited) = state.awaiting(ProviderId::Desktop)
        && let Some(put_back) = state.by_id(ProviderId::Desktop, &awaited.put_back)
    {
        let put_back = state.typed(&put_back.key());
        checks.push(warn(
            "desktop_awaiting",
            "awaited sign-in",
            format!(
                "Claude is signed out for a sign-in since {}, and `{put_back}` is parked",
                moment(awaited.since, now)
            ),
            format!(
                "Sign in inside Claude, quit it with Command-Q, then run `pitboard enroll {}`. \
                 To go back, run `pitboard use {put_back}`.",
                typed(&awaited.label)
            ),
        ));
    }
    let parks_dir = facts.parks_dir.display();
    match facts.private {
        Some(crate::host::Private::Missing) | None => {}
        Some(found) => {
            let name = "parked Claude Desktop logins";
            checks.push(match found {
                crate::host::Private::Yes if facts.one_volume == Some(false) => fail(
                    "desktop_parks_folder",
                    name,
                    format!(
                        "{parks_dir} is on another volume than {}",
                        facts.root.as_ref().map_or_else(
                            |_| "Claude's data".into(),
                            |root| root.display().to_string()
                        )
                    ),
                    "Keep Pitboard's folder on the volume your home is on.",
                ),
                crate::host::Private::Yes => ok(
                    "desktop_parks_folder",
                    name,
                    format!("private  ·  {parks_dir}"),
                ),
                crate::host::Private::Shared => fail(
                    "desktop_parks_folder",
                    name,
                    format!("{parks_dir} can be reached by others"),
                    // Claude Desktop is macOS only, so no SID is ever named.
                    commands(&os.make_private_commands(
                        Kind::Directory,
                        &[&parks_dir.to_string()],
                        None,
                    ))
                    .map_or_else(String::new, |commands| format!("Run {commands}.")),
                ),
                crate::host::Private::Link => fail(
                    "desktop_parks_folder",
                    name,
                    format!("{parks_dir} is a link"),
                    "Pitboard parks no Claude Desktop login through a link.",
                ),
                crate::host::Private::NotOwn => fail(
                    "desktop_parks_folder",
                    name,
                    format!("{parks_dir} is not yours"),
                    "Pitboard parks no Claude Desktop login in a folder another user owns.",
                ),
                crate::host::Private::NotADirectory | crate::host::Private::Missing => fail(
                    "desktop_parks_folder",
                    name,
                    format!("{parks_dir} is not a folder"),
                    "Pitboard parks no Claude Desktop login there.",
                ),
            });
        }
    }
    let asides: Vec<&crate::state::Aside> = state
        .aside
        .iter()
        .filter(|aside| aside.tool == ProviderId::Desktop)
        .collect();
    if !asides.is_empty() {
        let (holding, parts): (Vec<&crate::state::Aside>, Vec<&crate::state::Aside>) =
            asides.iter().partition(|aside| aside.login.is_some());
        let name = "folders set aside";
        let n = asides.len();
        let account = |aside: &crate::state::Aside| {
            aside
                .account
                .as_deref()
                .and_then(|id| state.owner_of_park(id))
        };
        let kept_beside = |aside: &crate::state::Aside| {
            account(aside).is_some_and(|a| a.parked.is_some() || in_use.as_ref() == Some(&a.key()))
        };
        let said = |aside: &crate::state::Aside| {
            let when = moment(aside.at, now);
            let typed = account(aside).map(|a| state.typed(&a.key()));
            match (aside.why, typed) {
                (crate::state::SetAside::AnotherMachine, Some(typed)) => {
                    format!("`{typed}`'s login from another Mac")
                }
                (crate::state::SetAside::Superseded, Some(typed)) if kept_beside(aside) => {
                    format!("`{typed}`'s login from before a sign-in again, set aside {when}")
                }
                (
                    crate::state::SetAside::Superseded | crate::state::SetAside::GivenUp,
                    Some(typed),
                ) => format!("`{typed}`'s earlier login, set aside {when}"),
                _ => format!("a login no account here holds, set aside {when}"),
            }
        };
        // No session: an account's earlier keys, the parts of its login a give-up left, or
        // what Claude made while signed out.
        let mut rest: Vec<String> = parts
            .iter()
            .filter_map(|aside| {
                let when = moment(aside.at, now);
                match (aside.why, account(aside).map(|a| state.typed(&a.key()))) {
                    (_, Some(typed)) => {
                        Some(format!("parts of `{typed}`'s login, set aside {when}"))
                    }
                    (crate::state::SetAside::InTheWay, None) => None,
                    (_, None) => Some(format!(
                        "parts of a login no account here holds, set aside {when}"
                    )),
                }
            })
            .collect();
        let only_signed_out = rest.is_empty();
        if rest.len() < parts.len() {
            rest.push("what Claude made while signed out".into());
        }
        let mut detail = match holding.as_slice() {
            [] => format!("{n} in {parks_dir}, none holding a login"),
            held => format!(
                "{n} in {parks_dir}, {} holding a login: {}",
                held.len(),
                words::listed(held.iter().map(|aside| said(aside)))
            ),
        };
        if !rest.is_empty() {
            let (whose, only) = match holding.is_empty() {
                true if only_signed_out => ("they hold", "only "),
                true => ("they hold", ""),
                false => ("the rest hold", ""),
            };
            detail.push_str(&format!("; {whose} {only}{}", words::listed(rest)));
        }
        checks.push(if holding.iter().all(|aside| kept_beside(aside)) {
            ok("desktop_set_aside", name, detail)
        } else {
            warn(
                "desktop_set_aside",
                name,
                detail,
                "`pitboard repair` gives one back to its account where it can. `pitboard \
                 forget` of an account deletes those set aside for it, and `pitboard \
                 uninstall` deletes them all.",
            )
        });
    }
    checks.push(judge_desktop_build(facts));
    checks
}

pub(super) fn judge_desktop_login(
    facts: &DesktopFacts,
    state: &State,
    now: i64,
    moment: &dyn Fn(i64, i64) -> String,
) -> Option<Check> {
    let known = facts.known.as_ref()?;
    let name = "Claude Desktop in use";
    let tool = ProviderId::Desktop;
    let enroll = "Open Claude, check which account it shows, quit it with Command-Q, then run \
                  `pitboard enroll desktop/<label>` under that account's name.";
    let Some(last) = &known.last else {
        return Some(warn(
            "desktop_login",
            name,
            "Pitboard has not read who is signed in to Claude",
            "The next change to a Claude Desktop account reads it, with Claude closed.",
        ));
    };
    let when = moment(last.known_at, now);
    let who = Stored::of(state, tool, last.owner.as_ref());
    Some(match known.doubt {
        Some(crate::in_use::Doubt::LoginUnreadable) => warn(
            "desktop_login",
            name,
            format!(
                "Claude's settings could not be read, so this is as Pitboard last read it {when}"
            ),
            "`Claude settings` says why.",
        ),
        Some(crate::in_use::Doubt::NamedMoved) => warn(
            "desktop_login",
            name,
            words::desktop_named(
                &Stored::of(state, tool, known.own_record.as_ref()),
                &who,
                |stored| stored.said("nobody"),
            ),
            enroll,
        ),
        _ if facts.signed_out => warn(
            "desktop_login",
            name,
            format!(
                "{}, as Pitboard last read it {when}; both token caches are emptied since, so \
                 Claude looks signed out",
                who.said("nobody")
            ),
            "The next change to a Claude Desktop account reads who is signed in, with Claude \
             closed.",
        ),
        _ => match state.account_in_use(tool) {
            Some(account) if account.parked.is_some() => {
                let typed = state.typed(&account.key());
                warn(
                    "desktop_login",
                    name,
                    format!("`{typed}` is recorded in use and also holds a parked login"),
                    format!(
                        "Open Claude, check which account it shows, and quit it with \
                         Command-Q. If it shows `{typed}`, run `pitboard enroll {typed}`: its \
                         older parked login is set aside, and nothing is deleted. If it shows \
                         another account, run `pitboard enroll desktop/<label>` under that \
                         account's name."
                    ),
                )
            }
            _ => ok(
                "desktop_login",
                name,
                format!("{}, as Pitboard last read it {when}", who.said("nobody")),
            ),
        },
    })
}

pub(super) fn judge_desktop_park(facts: &DesktopFacts, fact: &ParkFact, now: i64) -> Check {
    let name = format!("account {}", fact.typed());
    let renew = format!("Run `pitboard enroll {} --sign-in`.", fact.typed());
    let code = "desktop_parked_login";
    let Some(park) = &fact.park else {
        return if fact.active {
            ok(code, name, "signed in; parked when you switch away")
        } else {
            warn(code, name, "nothing parked to switch to", renew)
        };
    };
    let here = facts
        .parked_here
        .iter()
        .find(|(service, _)| *service == park.service)
        .map(|(_, folder)| folder);
    match here {
        Some(Ok(true)) => {}
        // What Pitboard could not look at is still there: no sign-in, as `use` asks none.
        Some(Err(e)) => {
            return warn(
                code,
                name,
                format!(
                    "not read: Pitboard could not look at it in {} ({e})",
                    facts.parks_dir.display()
                ),
                format!(
                    "Make {}, and each folder it is in, readable to you, then run `pitboard \
                     doctor` again.",
                    facts.parks_dir.display()
                ),
            );
        }
        Some(Ok(false)) | None => {
            return fail(
                code,
                name,
                format!(
                    "its parked login is missing from {}",
                    facts.parks_dir.display()
                ),
                format!(
                    "Run `pitboard repair`. If it gives nothing back, run `pitboard enroll {} \
                     --sign-in`.",
                    fact.typed()
                ),
            );
        }
    }
    let Some(at) = park.session_expires_at else {
        return ok(code, name, "parked");
    };
    // The column `pitboard status` shows, after this check's own words.
    let life = words::session_life_column(at, now);
    if at > now {
        ok(code, name, format!("parked, {life}"))
    } else {
        warn(
            code,
            name,
            format!("parked, {life}"),
            format!(
                "If Claude asks you to sign in once you use it, run `pitboard enroll {} \
                 --sign-in`.",
                fact.typed()
            ),
        )
    }
}

// Said, never a gate: Pitboard checks the folder before each move, not the build.
pub(super) fn judge_desktop_build(facts: &DesktopFacts) -> Check {
    use crate::provider::desktop::assumptions::{
        READ_FROM_CLAUDE_3, READ_FROM_THE_ARCHIVE, READ_FROM_THE_NEWER_BUNDLE, VERIFIED_AGAINST,
    };
    match &facts.build {
        Some(build) => ok(
            "desktop_build",
            "Claude",
            format!(
                "Claude {build}; Pitboard's facts were read from {VERIFIED_AGAINST}, \
                 {READ_FROM_THE_ARCHIVE}, {READ_FROM_THE_NEWER_BUNDLE} and {READ_FROM_CLAUDE_3}, \
                 and it checks the folder before each move, not the build"
            ),
        ),
        None => ok("desktop_build", "Claude", "not found in /Applications"),
    }
}

// On every machine, so its shape is one: whether the folder is a directory, never what it holds.
// What the report's `desktop` block says on every machine: whether the folder is a directory,
// never what it holds.
pub(crate) struct Installed {
    pub(crate) folder: Option<PathBuf>,
    pub(crate) present: bool,
    pub(crate) build: Option<String>,
}

pub(super) fn installed(
    ctx: &Context,
    facts: Option<&DesktopFacts>,
    build: Option<String>,
) -> Installed {
    let folder = match facts {
        Some(desktop) => desktop.root.clone().ok(),
        None => claude_desktop().and_then(|app| app.root(ctx).ok()),
    };
    Installed {
        present: folder.as_ref().is_some_and(|root| root.is_dir()),
        folder,
        build,
    }
}

// Claude Desktop is macOS's alone.
pub(super) fn desktop_build(ctx: &Context) -> Option<String> {
    match ctx.os {
        Os::MacOs => ctx
            .host()
            .app_version(std::path::Path::new("/Applications/Claude.app")),
        Os::Linux | Os::Windows => None,
    }
}
