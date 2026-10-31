mod claude;
mod codex;
mod desktop;
mod machine;
mod network;
mod parks;
mod redact;
mod run;
mod schedule;
mod windows;

pub(super) use super::example::{
    NOW, codex_parked, direct, facts, mode, no_codex, parked, with_codex,
};
use super::words;
use super::*;
use super::{
    claude::*, codex::*, desktop::*, machine::*, network::*, parks::*, redact::*, schedule::*,
    windows::*,
};
use crate::context::Context;
use crate::error::Error;
use crate::host::task::{Drift, Task};
use crate::host::{Access, Keyring, Kind, Os, SecretStore};
use crate::in_use::Stored;
use crate::permit::Permit;
use crate::provider::ProviderId;
use crate::provider::claude::daemon;
use crate::schedule::Location;
use crate::state::{Key, Park, RENEW_WITHIN, State};
use crate::time;
use crate::words as said;
use crate::{store, switch};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

pub(super) fn desktop() -> DesktopFacts {
    DesktopFacts {
        one_volume: Some(true),
        root: Ok(PathBuf::from(
            "/Users/dana/Library/Application Support/Claude",
        )),
        own_home: None,
        refused: None,
        present: true,
        settings: Some(Ok((crate::provider::folder::Sidecar::None, true))),
        unread: Vec::new(),
        known: None,
        signed_out: false,
        parks_dir: PathBuf::from("/Users/dana/.pitboard/parks"),
        private: Some(crate::host::Private::Yes),
        parked_here: Vec::new(),
        build: None,
    }
}

pub(super) fn desktop_account(label: &str, parked: Option<Park>) -> crate::state::Account {
    crate::state::Account {
        label: label.into(),
        id: format!("desktop_{label}"),
        account_uuid: label.into(),
        email: String::new(),
        parked,
        last_used_at: None,
        replaced_at: None,
        plan: None,
        detail: crate::state::Detail::Desktop {},
    }
}

pub(super) fn folder_park(service: &str, session_expires_at: Option<i64>) -> Park {
    Park {
        service: service.into(),
        kind: crate::state::ParkKind::Folder,
        parked_at: NOW - 86_400,
        login: "f".into(),
        access_expires_at: None,
        refresh_expires_at: None,
        session_expires_at,
    }
}

pub(super) fn said(checks: &[Check], code: &str) -> Vec<(Level, String)> {
    checks
        .iter()
        .filter(|c| c.code == code)
        .map(|c| (c.level, c.detail.clone()))
        .collect()
}

pub(super) const SID: &str = "S-1-5-21-1004336348-1177238915-682003330-1001";

// What doctor calls the Secret Service's collection where gnome-keyring answers for it.
pub(super) const LOGIN: Keyring = Keyring::Login;

pub(super) fn open_to_users() -> Access {
    Access {
        shared: true,
        described: "open to Users, you, SYSTEM and Administrators".into(),
    }
}

// Codex's section for a machine with no Codex accounts parked.
pub(super) fn codex_checks(codex: &CodexFacts) -> Vec<Check> {
    judge_codex(crate::host::OS, None, codex, &[], NOW, LOGIN, &time::moment)
}

pub(super) fn named<'a>(checks: &'a [Check], name: &str) -> &'a Check {
    checks.iter().find(|c| c.name == name).expect(name)
}

pub(super) fn check<'a>(checks: &'a [Check], code: &str) -> &'a Check {
    checks.iter().find(|c| c.code == code).expect(code)
}

// A schedule that runs a Pitboard still there, installed with `network`.
pub(super) fn scheduled(network: NetworkFact, network_differs: bool) -> ScheduleFact {
    ScheduleFact {
        location: Location::File(PathBuf::from(
            "/home/x/.config/systemd/user/pitboard-renew.timer",
        )),
        task: None,
        program: Some(PathBuf::from("/usr/local/bin/pitboard")),
        program_found: true,
        network,
        network_differs,
        outdated: false,
        refused: None,
    }
}

// A network whose requests go through `proxy`.
pub(super) fn by(proxy: ProxyFact) -> NetworkFact {
    NetworkFact {
        set: vec![proxy.variable],
        proxy: Some(proxy),
        passed_over: Vec::new(),
    }
}

// A proxy `HTTPS_PROXY` names, exempting the hosts Pitboard calls that `exempt` lists.
pub(super) fn through(exempting: Option<&'static str>, exempt: &[&str]) -> ProxyFact {
    ProxyFact {
        variable: "HTTPS_PROXY",
        address: "http://***:***@proxy.example.com:3128".into(),
        server: "proxy.example.com:3128".into(),
        on_loopback: false,
        socks: false,
        fails: false,
        exempting,
        exempt: exempt.iter().map(|host| (*host).to_string()).collect(),
    }
}

// The accounts `facts` has in use, by the name a command takes.
pub(super) fn in_use(facts: &Facts) -> Vec<String> {
    facts
        .parks
        .iter()
        .filter(|p| p.active)
        .map(ParkFact::typed)
        .collect()
}

// Every check doctor makes on `os`, each where it has something to say.
pub(super) fn everything_said(os: Os) -> Facts {
    let claude = crate::state::Account {
        label: "work".into(),
        id: "claude_work".into(),
        account_uuid: "work-uuid".into(),
        email: "work@example.com".into(),
        parked: None,
        last_used_at: None,
        replaced_at: None,
        plan: None,
        detail: crate::state::Detail::Claude {
            organization_uuid: "org".into(),
            oauth_account: json!({}),
        },
    };
    let codex = crate::state::Account {
        id: "codex_work".into(),
        detail: crate::state::Detail::Codex { workspace_id: None },
        ..claude.clone()
    };
    let mut state = State::default();
    state.accounts.push(claude);
    state.accounts.push(codex);
    state.accounts.push(desktop_account("work", None));
    state.accounts.push(desktop_account(
        "home",
        Some(folder_park("home-1", Some(NOW + 90_000))),
    ));
    state.discarded.push(crate::state::Dropped {
        service: "pitboard-park-x-1".into(),
        kind: crate::state::ParkKind::Vault,
        forgotten: false,
    });
    let awaited = crate::state::Awaited {
        since: NOW - 120,
        label: "new".into(),
        account: None,
        put_back: "desktop_home".into(),
    };
    state.awaiting.insert("desktop".into(), awaited);
    state.moving.insert(
        "desktop".into(),
        crate::state::Moving {
            half: crate::state::Half::Leave,
            started_at: NOW - 300,
            id: "desktop_work".into(),
            label: "work".into(),
            park: "pitboard-park-desktop_work-1".into(),
            login: "f0".into(),
            items: vec!["Cookies".into()],
            keys: std::collections::BTreeMap::new(),
            asides: std::collections::BTreeMap::new(),
            slot: String::new(),
            awaited: None,
        },
    );
    state.aside.push(crate::state::Aside {
        name: "pitboard-aside-1".into(),
        tool: ProviderId::Desktop,
        at: NOW - 3_600,
        why: crate::state::SetAside::Orphan,
        account: None,
        login: Some("d".into()),
    });
    let dormant = |mut park: ParkFact| {
        park.last_used_at = Some(NOW - 400 * 86_400);
        park
    };
    let desktop_park = |label: &str, active: bool, park: Option<Park>| ParkFact {
        provider: ProviderId::Desktop,
        name: format!("desktop/{label}"),
        active,
        last_used_at: None,
        park,
        unreadable: None,
    };
    let work = crate::in_use::InUse {
        owner: Some(state.accounts[2].owner()),
        login: "f0".into(),
        known_at: NOW - 60,
        named: Some("desktop_work".into()),
    };
    let mut desktop = desktop();
    desktop.known = Some(crate::in_use::Known {
        last: Some(work),
        doubt: None,
        own_record: Some(state.accounts[2].owner()),
    });
    desktop.parked_here = vec![("home-1".into(), Ok(true))];
    let mut f = facts();
    f.os = os;
    f.floor = crate::host::Floor::Below { build: 22631 };
    f.elevation = crate::host::Elevation::Elevated { why: "with sudo" };
    f.no_argv = true;
    let fallback = |store| FallbackLogin {
        path: PathBuf::from("/home/x/.claude/.credentials.json"),
        store,
        fingerprint: Ok(None),
    };
    f.machine_id_known = os == Os::Windows;
    f.interrupted = true;
    f.pending_parks = vec!["pitboard-park-y-1".into()];
    f.schedule = Some(scheduled(by(through(None, &[])), true));
    f.watch_service = Some(WatchServiceFact {
        location: Location::File(PathBuf::from("/home/x/watch.plist")),
        program: Some(PathBuf::from("/usr/local/bin/pitboard")),
        program_found: true,
        switching: None,
        task: None,
        outdated: false,
        refused: None,
    });
    f.login_pins = vec!["`forceLoginOrgUUID` set to `org` in managed settings".into()];
    f.parks = vec![
        dormant(parked("old", Some(NOW + 20 * 86_400))),
        dormant(codex_parked("old", Some(NOW + 20 * 86_400))),
        desktop_park("work", true, None),
        desktop_park("home", false, state.accounts[3].parked.clone()),
    ];
    f.state = Ok(state);
    f.codex = CodexFacts {
        enrolled: 1,
        restrictions: vec!["`forced_login_method` set to `chatgpt` in config.toml".into()],
        ..with_codex()
    };
    f.desktop = Some(desktop);
    match os {
        Os::Windows => {
            f.backend = Ok(store::Backend::CredentialManager);
            f.vault = store::Backend::SealedFile;
            f.fallback_login = Some(fallback(SecretStore::CredentialManager));
            f.codex.stored_elsewhere = (1, 1);
            f.windows = Some(WindowsHomes {
                home: PathBuf::from(r"C:\Users\dana"),
                default_pitboard_home: PathBuf::from(r"C:\Users\dana\AppData\Local\Pitboard"),
                local_app_data: Some(PathBuf::from(r"D:\AppData\Local")),
                profile: Some(PathBuf::from(r"D:\Users\dana")),
                codex_home_set: false,
                roams: true,
            });
            f.windows_settings = Some(crate::host::WindowsSettings {
                wsl: vec!["Ubuntu".into()],
                proxy: Some("proxy.example.com:3128".into()),
                admin_approval: Some(true),
            });
            f.pitboards = vec![
                PathBuf::from(r"C:\a\pitboard.exe"),
                PathBuf::from(r"C:\b\pitboard.exe"),
            ];
            f.status_line = Some(StatusLineFact {
                git_bash: claude_paths::GitBash {
                    found: None,
                    passed_over: None,
                },
                powershell: None,
                pitboard: None,
                runs_pitboard: true,
            });
        }
        // Claude Code's file is its only store, with no ceiling, and parks are in a locked
        // login keyring.
        Os::Linux => {
            // Only systemd's units are read as an earlier Pitboard wrote them.
            f.schedule.as_mut().expect("set above").outdated = true;
            f.watch_service.as_mut().expect("set above").outdated = true;
            f.backend = Ok(store::Backend::File);
            f.hover_rest_env = true;
            f.vault = store::Backend::SecretService;
            f.vault_form = Some(FormFact {
                kept: switch::Keeping {
                    place: Some(crate::host::Vault::SecretService),
                    shared: true,
                    folder: Some(PathBuf::from("/home/x/.pitboard/vault")),
                    secret_service: Some(true),
                    keyring: Some(LOGIN),
                    moving_to: None,
                    leaving: None,
                },
                locked: true,
            });
            f.parks[0].unreadable = Some(Unreadable::Locked(store::Backend::SecretService));
            f.credential_cost = None;
            f.readable_by_others = vec![(
                "/home/x/.claude/.credentials.json".into(),
                Kind::File,
                mode(0o644),
            )];
            f.wsl = Some(WslFact {
                distribution: Some("Ubuntu".into()),
                codex_home_on_windows: None,
            });
            f.desktop = None;
        }
        Os::MacOs => f.fallback_login = Some(fallback(SecretStore::Keychain)),
    }
    f
}
