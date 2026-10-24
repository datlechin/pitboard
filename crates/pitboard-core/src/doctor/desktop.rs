use super::parks::ParkFact;
use super::{Check, fail, ok, warn, words};
use crate::context::Context;
use crate::host::{Kind, Os};
use crate::in_use::Stored;
use crate::provider::{LoginKind, ProviderId};
use crate::state::{Key, State};
use crate::words as said;
use crate::{store, switch};
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

// An index from a Mac; only a build for tests gives Linux a Claude data folder.
pub(super) fn judge_desktop_off_macos(state: &State) -> Check {
    let held: Vec<String> = state
        .accounts
        .iter()
        .filter(|account| account.provider() == ProviderId::Desktop)
        .map(|account| format!("`{}`", state.typed(&account.key())))
        .collect();
    let holds = match held.is_empty() {
        true => words::FOLDERS_SET_ASIDE_FOR_IT.to_string(),
        false => said::listed(held),
    };
    Check {
        advice: words::nothing_here_changes(&holds),
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
            let advice = facts
                .own_home
                .as_ref()
                .map_or_else(String::new, |own| words::run_with_home_unset(own.display()));
            checks.push(fail("desktop_data_folder", folder, reason, advice));
        }
        Ok(root) => {
            let shown = root.display();
            checks.push(match facts.refused {
                Some(unfit) => fail(
                    "desktop_data_folder",
                    folder,
                    words::the_folder(shown, unfit),
                    match unfit {
                        Unfit::Link => words::NO_MOVE_THROUGH_A_LINK,
                        Unfit::ThirdParty(_) => words::NO_MOVE_FOR_A_THIRD_PARTY,
                        Unfit::ResetWaits => words::LET_THE_RESET_FINISH,
                    },
                ),
                None if facts.present => ok("desktop_data_folder", folder, shown.to_string()),
                None => warn(
                    "desktop_data_folder",
                    folder,
                    words::does_not_exist(shown),
                    words::ENROLL_THEN_SIGN_IN_INSIDE,
                ),
            });
        }
    }
    if let (Ok(root), Some(settings)) = (&facts.root, &facts.settings) {
        let name = "Claude settings";
        checks.push(match settings {
            Ok((Sidecar::Valid, _)) => ok("desktop_settings", name, words::SETTINGS_WITH_A_WRITE),
            Ok((Sidecar::Stale, _)) => {
                ok("desktop_settings", name, words::SETTINGS_WITH_A_STALE_WRITE)
            }
            Ok((Sidecar::None, true)) => ok("desktop_settings", name, words::SETTINGS_READ),
            Ok((Sidecar::None, false)) => ok("desktop_settings", name, words::NO_SETTINGS_YET),
            Err(Unread::Unknown { what, then }) => fail(
                "desktop_settings",
                name,
                words::the_folder(root.display(), what),
                then.said().trim_end(),
            ),
            Err(Unread::Unreadable(detail)) => fail("desktop_settings", name, detail.clone(), ""),
        });
        if facts.present {
            let name = "Claude data folder layout";
            checks.push(match facts.unread.as_slice() {
                [] => ok("desktop_layout", name, words::EVERY_PART_MOVED),
                [part] => warn(
                    "desktop_layout",
                    name,
                    words::holds_a_part_unread(part),
                    words::NO_MOVE_WHILE_IT_IS_THERE,
                ),
                parts => warn(
                    "desktop_layout",
                    name,
                    words::holds_parts_unread(parts),
                    words::NO_MOVE_WHILE_THEY_ARE_THERE,
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
            words::move_interrupted(&typed(&moving.label), &moment(moving.started_at, now)),
            words::KEEP_CLAUDE_CLOSED,
        ));
    }
    if let Some(awaited) = state.awaiting(ProviderId::Desktop)
        && let Some(put_back) = state.by_id(ProviderId::Desktop, &awaited.put_back)
    {
        let put_back = state.typed(&put_back.key());
        checks.push(warn(
            "desktop_awaiting",
            "awaited sign-in",
            words::signed_out_for_a_sign_in(&moment(awaited.since, now), &put_back),
            words::finish_the_sign_in(&typed(&awaited.label), &put_back),
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
                    words::on_another_volume(
                        &parks_dir,
                        facts.root.as_ref().ok().map(|root| root.display()),
                    ),
                    words::KEEP_ONE_VOLUME,
                ),
                crate::host::Private::Yes => ok(
                    "desktop_parks_folder",
                    name,
                    words::private_folder(&parks_dir),
                ),
                crate::host::Private::Shared => fail(
                    "desktop_parks_folder",
                    name,
                    words::reached_by_others(&parks_dir),
                    // Claude Desktop is macOS only, so no SID is ever named.
                    words::run_them(&os.make_private_commands(
                        Kind::Directory,
                        &[&parks_dir.to_string()],
                        None,
                    )),
                ),
                crate::host::Private::Link => fail(
                    "desktop_parks_folder",
                    name,
                    words::is_a_link(&parks_dir),
                    words::NO_PARK_THROUGH_A_LINK,
                ),
                crate::host::Private::NotOwn => fail(
                    "desktop_parks_folder",
                    name,
                    words::not_yours(&parks_dir),
                    words::NO_PARK_IN_ANOTHERS,
                ),
                crate::host::Private::NotADirectory | crate::host::Private::Missing => fail(
                    "desktop_parks_folder",
                    name,
                    words::not_a_folder(&parks_dir),
                    words::NO_PARK_THERE,
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
        let account = |aside: &crate::state::Aside| {
            aside
                .account
                .as_deref()
                .and_then(|id| state.owner_of_park(id))
        };
        let kept_beside = |aside: &crate::state::Aside| {
            account(aside).is_some_and(|a| a.parked.is_some() || in_use.as_ref() == Some(&a.key()))
        };
        let held: Vec<String> = holding
            .iter()
            .map(|aside| {
                let when = moment(aside.at, now);
                let typed = account(aside).map(|a| state.typed(&a.key()));
                match (aside.why, typed) {
                    (crate::state::SetAside::AnotherMachine, Some(typed)) => {
                        words::from_another_mac(&typed)
                    }
                    (crate::state::SetAside::Superseded, Some(typed)) if kept_beside(aside) => {
                        words::from_before_signing_in_again(&typed, &when)
                    }
                    (
                        crate::state::SetAside::Superseded | crate::state::SetAside::GivenUp,
                        Some(typed),
                    ) => words::earlier_login(&typed, &when),
                    _ => words::nobodys_login(&when),
                }
            })
            .collect();
        // No session: earlier keys, what a give-up left, or what Claude made while signed out.
        let mut rest: Vec<String> = parts
            .iter()
            .filter_map(|aside| {
                let when = moment(aside.at, now);
                match (aside.why, account(aside).map(|a| state.typed(&a.key()))) {
                    (_, Some(typed)) => Some(words::parts_of(&typed, &when)),
                    (crate::state::SetAside::InTheWay, None) => None,
                    (_, None) => Some(words::parts_of_nobodys(&when)),
                }
            })
            .collect();
        let only_signed_out = rest.is_empty();
        if rest.len() < parts.len() {
            rest.push(words::MADE_WHILE_SIGNED_OUT.into());
        }
        let mut detail = words::set_aside(asides.len(), &parks_dir, &held);
        if !rest.is_empty() {
            let none_held = holding.is_empty();
            detail.push_str(&words::the_rest_hold(
                none_held,
                none_held && only_signed_out,
                &rest,
            ));
        }
        checks.push(if holding.iter().all(|aside| kept_beside(aside)) {
            ok("desktop_set_aside", name, detail)
        } else {
            warn(
                "desktop_set_aside",
                name,
                detail,
                words::REPAIR_GIVES_ONE_BACK,
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
    let Some(last) = &known.last else {
        return Some(warn(
            "desktop_login",
            name,
            words::NOT_READ_WHO_IS_IN,
            words::THE_NEXT_CHANGE_READS_IT,
        ));
    };
    let when = moment(last.known_at, now);
    let who = Stored::of(state, tool, last.owner.as_ref());
    Some(match known.doubt {
        Some(crate::in_use::Doubt::LoginUnreadable) => warn(
            "desktop_login",
            name,
            words::settings_unreadable_since(&when),
            words::THE_SETTINGS_CHECK_SAYS_WHY,
        ),
        Some(crate::in_use::Doubt::NamedMoved) => warn(
            "desktop_login",
            name,
            said::desktop_named(
                &Stored::of(state, tool, known.own_record.as_ref()),
                &who,
                |stored| stored.said("nobody"),
            ),
            words::ENROLL_UNDER_ITS_NAME,
        ),
        _ if facts.signed_out => warn(
            "desktop_login",
            name,
            words::looks_signed_out_since(&who.said("nobody"), &when),
            words::THE_NEXT_CHANGE_READS_WHO,
        ),
        _ => match state.account_in_use(tool) {
            Some(account) if account.parked.is_some() => {
                let typed = state.typed(&account.key());
                warn(
                    "desktop_login",
                    name,
                    words::in_use_and_parked(&typed),
                    words::check_which_account_claude_shows(&typed),
                )
            }
            _ => ok(
                "desktop_login",
                name,
                words::as_last_read(&who.said("nobody"), &when),
            ),
        },
    })
}

pub(super) fn judge_desktop_park(facts: &DesktopFacts, fact: &ParkFact, now: i64) -> Check {
    let name = format!("account {}", fact.typed());
    let renew = words::sign_in_again(&fact.typed());
    let code = "desktop_parked_login";
    let Some(park) = &fact.park else {
        return if fact.active {
            ok(code, name, words::SIGNED_IN_PARKED_ON_A_SWITCH)
        } else {
            warn(code, name, words::NOTHING_PARKED, renew)
        };
    };
    let here = facts
        .parked_here
        .iter()
        .find(|(service, _)| *service == park.service)
        .map(|(_, folder)| folder);
    let parks_dir = facts.parks_dir.display();
    match here {
        Some(Ok(true)) => {}
        // What Pitboard could not look at is still there: no sign-in, as `use` asks none.
        Some(Err(e)) => {
            return warn(
                code,
                name,
                words::not_looked_at(&parks_dir, e),
                words::make_it_readable(&parks_dir),
            );
        }
        Some(Ok(false)) | None => {
            return fail(
                code,
                name,
                words::missing_from(&parks_dir),
                words::repair_then_sign_in(&fact.typed()),
            );
        }
    }
    let Some(at) = park.session_expires_at else {
        return ok(code, name, words::PARKED);
    };
    // The column `pitboard status` shows, after this check's own words.
    let life = said::session_life_column(at, now);
    if at > now {
        ok(code, name, words::parked_with(&life))
    } else {
        warn(
            code,
            name,
            words::parked_with(&life),
            words::if_claude_asks(&fact.typed()),
        )
    }
}

// Said, never a gate: Pitboard checks the folder before each move, not the build.
pub(super) fn judge_desktop_build(facts: &DesktopFacts) -> Check {
    match &facts.build {
        Some(build) => ok("desktop_build", "Claude", words::claude_build(build)),
        None => ok("desktop_build", "Claude", words::NOT_IN_APPLICATIONS),
    }
}

// The report's `desktop` block, on every machine: whether the folder is one, never what it holds.
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
