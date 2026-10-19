// Read by a change only, with the app seen closed and its folder held. Nothing is recorded.

use super::super::{Error, Result, shape};
use super::there;
use crate::context::Context;
use crate::error::Then;
use crate::provider::ProviderId;
use crate::provider::folder::{Folder, SignedIn, Unread};
use crate::state::{Key, ParkKind, State};
use std::path::Path;

pub(crate) enum Holds {
    Empty,
    SignedOut,
    // emptied: a token cache holds the value the app leaves after a Log out (E1b)
    SignedIn {
        session: SignedIn,
        named: Option<String>,
        emptied: bool,
    },
}

pub(crate) enum Verdict {
    // rows 3, 5, 10. `leftovers`: the app left a signed-out jar, which an arrive sets aside
    Nobody {
        leftovers: bool,
    },
    // rows 13, 16. `as_named`: a session Pitboard had not seen, taken as the app names it
    Account {
        key: Key,
        session: SignedIn,
        as_named: bool,
    },
    Unsure(Unsure),
}

pub(crate) enum Unsure {
    // 12
    Unfinished,
    // 14, 15
    RecordedUnderAnother {
        recorded: Key,
        named: Named,
    },
    // 17
    AlsoParked {
        key: Key,
        session: SignedIn,
    },
    // 18, 19
    Unseen {
        named: Named,
        session: SignedIn,
    },
    // 20: `also_in` is the folder under `parks` that holds the same session
    HeldTwice {
        named: Named,
        session: SignedIn,
        also_in: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Named {
    Enrolled(Key),
    NotEnrolled { uuid: String },
}

pub(crate) fn look(ctx: &Context, state: &State, app: &'static dyn Folder) -> Result<Verdict> {
    let root = app.root(ctx).map_err(|e| shape(app.id(), e))?;
    // Row 3, nobody, as row 5: the caller that refuses it looks first.
    if !there(app.id(), &root)? {
        return Ok(Verdict::Nobody { leftovers: false });
    }
    if let Some(unfit) = app.refuses(&root) {
        return Err(Error::LoginFolderUnknown {
            tool: app.id(),
            what: unfit.to_string(),
            then: Then::Nothing,
        });
    }
    let holds = holds(ctx, app, &root)?;
    Ok(verdict(state, app.id(), holds))
}

pub(crate) fn holds(ctx: &Context, app: &dyn Folder, root: &Path) -> Result<Holds> {
    let tool = app.id();
    let unread = |unread: Unread| match unread {
        Unread::Unknown { what, then } => Error::LoginFolderUnknown { tool, what, then },
        Unread::Unreadable(detail) => Error::LoginFolderUnreadable { tool, detail },
    };
    let settings = app.settings(root).map_err(|refused| match refused {
        Unread::Unreadable(detail) => Error::LoginFolderUnknown {
            tool,
            what: format!("has a settings file Pitboard could not read ({detail})"),
            then: Then::Nothing,
        },
        known => unread(known),
    })?;
    let keys = settings.account_keys(app);
    let caches = app.caches();
    let jar = root.join(app.items()[0]);
    if !there(tool, &jar)? {
        let mut item = None;
        for name in &app.items()[1..] {
            if there(tool, &root.join(name))? {
                item = Some(name);
                break;
            }
        }
        let key = app.keys().iter().find(|key| keys.get(key).is_some());
        return match item.or(key) {
            None => Ok(Holds::Empty),
            Some(name) => Err(Error::LoginFolderUnknown {
                tool,
                what: format!("has no cookie store and still holds `{name}`"),
                then: Then::OpenAndQuit,
            }),
        };
    }
    let session = app.session(ctx, &jar).map_err(unread)?;
    let emptied = |key: &&str| keys.get(key).is_some_and(|value| app.emptied(key, value));
    let Some(session) = session else {
        return if caches
            .iter()
            .all(|key| keys.get(key).is_none() || emptied(key))
        {
            Ok(Holds::SignedOut)
        } else {
            Err(Error::LoginFolderUnknown {
                tool,
                what: "holds no session and still holds a login's tokens".into(),
                then: Then::SignInAndEnrol,
            })
        };
    };
    Ok(Holds::SignedIn {
        session,
        named: settings.named(app),
        emptied: caches.iter().any(emptied),
    })
}

// Pure: no clock, no disk.
pub(crate) fn verdict(state: &State, tool: ProviderId, holds: Holds) -> Verdict {
    let (session, named, emptied) = match holds {
        Holds::Empty => return Verdict::Nobody { leftovers: false },
        Holds::SignedOut => return Verdict::Nobody { leftovers: true },
        Holds::SignedIn {
            session,
            named,
            emptied,
        } => (session, named, emptied),
    };
    let named = named.map(|uuid| {
        state
            .accounts
            .iter()
            .find(|a| a.provider() == tool && a.account_uuid == uuid)
            .map_or(Named::NotEnrolled { uuid }, |a| Named::Enrolled(a.key()))
    });
    let recorded = state
        .in_use(tool)
        .filter(|record| !record.login.is_empty() && record.login == session.login)
        .and_then(|record| record.owner.as_ref())
        .and_then(|owner| state.account_of(tool, owner))
        .map(crate::state::Account::key);
    // The fingerprint is Pitboard's own, whatever the caches hold.
    if let (Some(recorded), Some(named)) = (&recorded, &named) {
        return match named {
            Named::Enrolled(key) if key == recorded => Verdict::Account {
                key: key.clone(),
                session,
                as_named: false,
            },
            _ => Verdict::Unsure(Unsure::RecordedUnderAnother {
                recorded: recorded.clone(),
                named: named.clone(),
            }),
        };
    }
    let Some(named) = named else {
        return Verdict::Unsure(Unsure::Unfinished);
    };
    if let Some(also_in) = held_here(state, tool, &session.login) {
        return Verdict::Unsure(Unsure::HeldTwice {
            named,
            session,
            also_in,
        });
    }
    if emptied {
        return Verdict::Unsure(Unsure::Unfinished);
    }
    let in_use = state.account_in_use(tool).map(crate::state::Account::key);
    match named {
        Named::Enrolled(key) if Some(&key) == in_use.as_ref() => {
            let parked = state.get(&key).is_some_and(|a| a.parked.is_some());
            if parked {
                Verdict::Unsure(Unsure::AlsoParked { key, session })
            } else {
                Verdict::Account {
                    key,
                    session,
                    as_named: true,
                }
            }
        }
        named => Verdict::Unsure(Unsure::Unseen { named, session }),
    }
}

// The folder under `parks` of this home that holds `login`: an account's park or an aside.
fn held_here(state: &State, tool: ProviderId, login: &str) -> Option<String> {
    state
        .accounts
        .iter()
        .filter(|a| a.provider() == tool)
        .filter_map(|a| a.parked.as_ref())
        .find(|park| park.kind == ParkKind::Folder && park.login == login)
        .map(|park| park.service.clone())
        .or_else(|| {
            state
                .aside
                .iter()
                .find(|aside| aside.tool == tool && aside.login.as_deref() == Some(login))
                .map(|aside| aside.name.clone())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::in_use::InUse;
    use crate::state::{Account, Aside, Detail, Park, SetAside};

    const TOOL: ProviderId = ProviderId::Desktop;

    fn account(label: &str, parked: Option<&str>) -> Account {
        Account {
            label: label.into(),
            id: format!("desktop_{label}-uuid"),
            account_uuid: format!("{label}-uuid"),
            email: String::new(),
            parked: parked.map(|login| Park {
                service: format!("park-of-{label}"),
                kind: ParkKind::Folder,
                parked_at: 1,
                login: login.into(),
                access_expires_at: None,
                refresh_expires_at: None,
                session_expires_at: None,
            }),
            last_used_at: None,
            replaced_at: None,
            plan: None,
            detail: Detail::Desktop {},
        }
    }

    // `work` and `home` enrolled, `work` recorded in use with the session `seen`.
    fn state(works_park: Option<&str>, homes_park: Option<&str>) -> State {
        let mut state = State::default();
        state.accounts.push(account("work", works_park));
        state.accounts.push(account("home", homes_park));
        state.in_use.insert(
            TOOL.code().into(),
            InUse {
                owner: Some(state.accounts[0].owner()),
                login: "seen".into(),
                known_at: 1,
                named: Some("desktop_work-uuid".into()),
            },
        );
        state
    }

    fn signed_in(login: &str, named: Option<&str>, emptied: bool) -> Holds {
        Holds::SignedIn {
            session: SignedIn {
                login: login.into(),
                expires_at: None,
            },
            named: named.map(|label| format!("{label}-uuid")),
            emptied,
        }
    }

    fn said(verdict: Verdict) -> String {
        let name = |named: &Named| match named {
            Named::Enrolled(key) => key.label.clone(),
            Named::NotEnrolled { uuid } => format!("nobody enrolled ({uuid})"),
        };
        match verdict {
            Verdict::Nobody { .. } => "nobody".into(),
            Verdict::Account { key, session, .. } => {
                format!("{} with {}", key.label, session.login)
            }
            Verdict::Unsure(Unsure::Unfinished) => "unfinished".into(),
            Verdict::Unsure(Unsure::RecordedUnderAnother { recorded, named }) => {
                format!("recorded {}, named {}", recorded.label, name(&named))
            }
            Verdict::Unsure(Unsure::AlsoParked { key, session }) => {
                format!("{} with {}, and parked", key.label, session.login)
            }
            Verdict::Unsure(Unsure::Unseen { named, session }) => {
                format!("unseen {}, named {}", session.login, name(&named))
            }
            Verdict::Unsure(Unsure::HeldTwice {
                named,
                session,
                also_in,
            }) => format!(
                "{} also in {also_in}, named {}",
                session.login,
                name(&named)
            ),
        }
    }

    // Rows 6 to 9 and 11 are `holds`' refusals and never reach `verdict`.
    #[test]
    fn each_row_of_the_table_is_read_as_it_says() {
        let plain = || state(None, None);
        let mut nobody_recorded = plain();
        nobody_recorded.in_use.clear();
        let mut set_aside = plain();
        set_aside.aside.push(Aside {
            name: "pitboard-aside-1".into(),
            tool: TOOL,
            at: 1,
            why: SetAside::GivenUp,
            account: None,
            login: Some("kept".into()),
        });
        let mut another_tools = plain();
        another_tools.accounts[1].detail = Detail::Codex { workspace_id: None };
        let rows = [
            ("5", plain(), Holds::Empty, "nobody"),
            ("10", plain(), Holds::SignedOut, "nobody"),
            (
                "12, no name",
                plain(),
                signed_in("seen", None, false),
                "unfinished",
            ),
            (
                "12, emptied",
                plain(),
                signed_in("new", Some("work"), true),
                "unfinished",
            ),
            (
                "12, emptied, another",
                plain(),
                signed_in("new", Some("home"), true),
                "unfinished",
            ),
            (
                "12, emptied, nobody's",
                plain(),
                signed_in("new", Some("other"), true),
                "unfinished",
            ),
            (
                "13",
                plain(),
                signed_in("seen", Some("work"), false),
                "work with seen",
            ),
            (
                "13, emptied",
                plain(),
                signed_in("seen", Some("work"), true),
                "work with seen",
            ),
            (
                "13, parked too",
                state(Some("older"), None),
                signed_in("seen", Some("work"), false),
                "work with seen",
            ),
            (
                "14",
                plain(),
                signed_in("seen", Some("home"), false),
                "recorded work, named home",
            ),
            (
                "14, emptied",
                plain(),
                signed_in("seen", Some("home"), true),
                "recorded work, named home",
            ),
            (
                "15",
                plain(),
                signed_in("seen", Some("other"), false),
                "recorded work, named nobody enrolled (other-uuid)",
            ),
            (
                "16",
                plain(),
                signed_in("new", Some("work"), false),
                "work with new",
            ),
            (
                "17",
                state(Some("older"), None),
                signed_in("new", Some("work"), false),
                "work with new, and parked",
            ),
            (
                "18",
                plain(),
                signed_in("new", Some("home"), false),
                "unseen new, named home",
            ),
            (
                "18, parked",
                state(None, Some("older")),
                signed_in("new", Some("home"), false),
                "unseen new, named home",
            ),
            (
                "18, nobody recorded",
                nobody_recorded,
                signed_in("new", Some("work"), false),
                "unseen new, named work",
            ),
            (
                "19",
                plain(),
                signed_in("new", Some("other"), false),
                "unseen new, named nobody enrolled (other-uuid)",
            ),
            (
                "19, another tool's account",
                another_tools,
                signed_in("new", Some("home"), false),
                "unseen new, named nobody enrolled (home-uuid)",
            ),
            (
                "20, a park",
                state(None, Some("kept")),
                signed_in("kept", Some("home"), false),
                "kept also in park-of-home, named home",
            ),
            (
                "20, the account in use's park",
                state(Some("kept"), None),
                signed_in("kept", Some("work"), false),
                "kept also in park-of-work, named work",
            ),
            (
                "20, an aside",
                set_aside,
                signed_in("kept", Some("other"), false),
                "kept also in pitboard-aside-1, named nobody enrolled (other-uuid)",
            ),
            (
                "20, emptied",
                state(None, Some("kept")),
                signed_in("kept", Some("home"), true),
                "kept also in park-of-home, named home",
            ),
        ];
        for (row, state, holds, expected) in rows {
            assert_eq!(said(verdict(&state, TOOL, holds)), expected, "row {row}");
        }
    }
}
