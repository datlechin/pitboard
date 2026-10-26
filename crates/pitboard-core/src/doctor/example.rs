//! A computer with nothing amiss, which the doctor's tests change one fact at a time.

use super::codex::{CodexFacts, CodexLogin};
use super::desktop::Installed;
use super::network::NetworkFact;
use super::parks::ParkFact;
use super::{Check, Facts, evaluate_with};
use crate::host::{Access, Os};
use crate::provider::ProviderId;
use crate::provider::claude::paths as claude_paths;
use crate::state::{Account, Detail, Key, Park, State};
use crate::store;
use serde_json::json;
use std::path::PathBuf;

// What a mode looks like as access, the way a Unix system describes one.
pub(crate) fn mode(mode: u32) -> Access {
    Access {
        shared: mode & 0o077 != 0,
        described: format!("mode {mode:o}"),
    }
}

// A network whose requests go out directly, as no variable names a proxy.
pub(crate) fn direct() -> NetworkFact {
    NetworkFact {
        proxy: None,
        passed_over: Vec::new(),
        set: Vec::new(),
    }
}

pub(crate) fn facts() -> Facts {
    Facts {
        os: crate::host::OS,
        sid: None,
        security_tool: Some("/usr/bin/security".into()),
        config_path: PathBuf::from("/home/x/.claude.json"),
        config: Ok(json!({"oauthAccount": {}})),
        config_account: Some(claude_paths::ConfigAccount {
            email: "a@b.c".into(),
            account_uuid: "acc".into(),
            organization_uuid: "org".into(),
            organization_name: None,
        }),
        credential_parts: Vec::new(),
        credential_cost: Some(store::Cost {
            needs: 900,
            limit: 4032,
            second_route: true,
        }),
        no_argv: false,
        service: "Claude Code-credentials".into(),
        account: "someone".into(),
        default_slot: true,
        storage_dir: "/home/x/.claude".into(),
        backend: Ok(store::Backend::Keychain),
        vault: store::Backend::Keychain,
        credential_file: PathBuf::from("/home/x/.claude/.credentials.json"),
        credential: Ok(Some(json!({"claudeAiOauth": {
            "refreshToken": "r",
            "refreshTokenExpiresAt": 2_000_000_000_000i64
        }}))),
        home: PathBuf::from("/home/x/.pitboard"),
        home_access: Some(mode(0o700)),
        readable_by_others: Vec::new(),
        machine_id_known: true,
        windows: None,
        hover_rest_env: false,
        daemon: None,
        pending_parks: Vec::new(),
        claude_version: Some(crate::provider::claude::assumptions::VERIFIED_AGAINST.into()),
        claude_program: Some(PathBuf::from("/opt/homebrew/bin/claude")),
        auth_overrides: Vec::new(),
        login_pins: Vec::new(),
        pinned_out: Vec::new(),
        asking_held: Vec::new(),
        state: Ok(State::default()),
        in_use: Some(in_use_as_named()),
        parks: Vec::new(),
        interrupted: false,
        stuck: None,
        codex: no_codex(),
        desktop: None,
        desktop_app: Installed {
            folder: None,
            present: false,
            build: None,
        },
        claude_present: true,
        schedule: None,
        watch_service: None,
        network: direct(),
        in_the_app: false,
        elevation: crate::host::Elevation::Normal,
        fallback_login: None,
        floor: crate::host::Floor::Met,
        status_line: None,
        windows_settings: None,
        pitboards: Vec::new(),
        wsl: None,
        now: NOW,
    }
}

// `acc`'s login in `org`, as Anthropic named it a minute ago, with the config naming it.
pub(crate) fn in_use_as_named() -> crate::in_use::Known {
    let owner = crate::provider::Owner {
        account_uuid: "acc".into(),
        email: "a@b.c".into(),
        organization_uuid: "org".into(),
    };
    let stored = json!({"claudeAiOauth": {"refreshToken": "r"}});
    crate::in_use::Known {
        last: Some(crate::in_use::InUse {
            owner: Some(owner.clone()),
            login: crate::provider::claude_code().fingerprint(&stored),
            known_at: NOW - 60,
            named: Some(crate::state::new_id(ProviderId::Claude, &owner)),
        }),
        doubt: None,
        own_record: Some(owner),
    }
}

pub(crate) fn no_codex() -> CodexFacts {
    CodexFacts {
        home: PathBuf::from("/home/x/.codex"),
        present: false,
        enrolled: 0,
        backend: crate::provider::codex::paths::Backend::File,
        backend_setting: "Codex's default".into(),
        backend_remedy: String::new(),
        auth_file: PathBuf::from("/home/x/.codex/auth.json"),
        auth_access: None,
        login: Ok(None),
        program: None,
        version: None,
        running: Some(Vec::new()),
        stored_elsewhere: (0, 0),
        restrictions: Vec::new(),
        restrictions_unreadable: false,
        ruled_out: Vec::new(),
    }
}

// A machine whose Codex is signed in, with its login where it should be.
pub(crate) fn with_codex() -> CodexFacts {
    CodexFacts {
        present: true,
        auth_access: Some(mode(0o600)),
        login: Ok(Some(CodexLogin {
            email: "w@example.com".into(),
            account_id: "work-account".into(),
            fingerprint: "0123456789abcdef".into(),
        })),
        program: Some(PathBuf::from("/usr/local/bin/codex")),
        version: Some(crate::provider::codex::assumptions::VERIFIED_AGAINST.into()),
        ..no_codex()
    }
}

pub(crate) const NOW: i64 = 1_789_935_600;

pub(crate) fn parked(label: &str, refresh_expires_at: Option<i64>) -> ParkFact {
    ParkFact {
        provider: ProviderId::Claude,
        last_used_at: None,
        name: Key::new(ProviderId::Claude, label).typed(),
        active: false,
        park: Some(Park {
            service: format!("pitboard-park-{label}-1"),
            parked_at: NOW - 86_400,
            kind: crate::state::ParkKind::Vault,
            login: "f".into(),
            access_expires_at: None,
            refresh_expires_at,
            session_expires_at: None,
        }),
        unreadable: None,
    }
}

// A Codex account's park, named as a command here would take it.
#[cfg(test)]
pub(crate) fn codex_parked(label: &str, refresh_expires_at: Option<i64>) -> ParkFact {
    ParkFact {
        provider: ProviderId::Codex,
        name: Key::new(ProviderId::Codex, label).typed(),
        ..parked(label, refresh_expires_at)
    }
}

// The site's world (website/src/data/world.ts), read by doctor at 20:50 on 13 October 2026 in
// +07:00: personal and work for Claude Code, main and spare for Codex, and on a Mac home and
// office for Claude Desktop, the first of each in use and the other parked.
const SITE_NOW: i64 = 1_791_899_400;
// The SID of the site's Windows account, as cli.ts names its tasks after it.
const SITE_SID: &str = "S-1-5-21-1004-1005-1006-1001";
// When work's parked login lapses, 9 November 01:00, and when office's claude.ai session ends,
// 8 November 00:00.
const WORK_REFRESH_UNTIL: i64 = 1_794_160_800;
const PERSONAL_REFRESH_UNTIL: i64 = 1_794_160_800;
const OFFICE_SESSION_UNTIL: i64 = 1_794_070_800;
// When Claude Desktop was last read in use, 18:45.
const HOME_READ_AT: i64 = 1_791_891_900;

struct SiteAccount {
    label: &'static str,
    email: &'static str,
    account_uuid: &'static str,
    organization_uuid: &'static str,
}

const PERSONAL: SiteAccount = SiteAccount {
    label: "personal",
    email: "me@example.com",
    account_uuid: "5f0c9a52-7d1e-4b8a-9c3f-2e6a1d4b8c07",
    organization_uuid: "8b2d4e61-3c5a-4f7e-a1d9-6c0e2b7f5a34",
};
const WORK: SiteAccount = SiteAccount {
    label: "work",
    email: "me@company.com",
    account_uuid: "c3a7e1f9-5b2d-4e8c-b6a0-9d4f1e2c7b58",
    organization_uuid: "e4f8a2c6-1d7b-4a3e-9f5c-0b8d6e2a4c91",
};
const MAIN: SiteAccount = SiteAccount {
    label: "main",
    email: "me@example.com",
    account_uuid: "a1e5c9d3-6f2b-4c8a-8e7d-3b0f5a9c1e26",
    organization_uuid: "",
};
const SPARE: SiteAccount = SiteAccount {
    label: "spare",
    email: "me@company.com",
    account_uuid: "d7b3f1a5-9c4e-4e2a-a8f6-1c5e9b3d7f40",
    organization_uuid: "",
};

fn refresh_token(label: &str) -> String {
    format!("refresh-{label}")
}

fn site_owner(a: &SiteAccount) -> crate::provider::Owner {
    crate::provider::Owner {
        account_uuid: a.account_uuid.into(),
        email: a.email.into(),
        organization_uuid: a.organization_uuid.into(),
    }
}

fn site_park(provider: ProviderId, label: &str, refresh_expires_at: Option<i64>) -> ParkFact {
    let mut fact = parked(label, refresh_expires_at);
    fact.provider = provider;
    fact.name = Key::new(provider, label).typed();
    if let Some(park) = &mut fact.park {
        park.parked_at = SITE_NOW - 86_400;
    }
    fact
}

fn site_account(a: &SiteAccount, provider: ProviderId, park: &ParkFact) -> Account {
    let owner = site_owner(a);
    Account {
        label: a.label.into(),
        id: crate::state::new_id(provider, &owner),
        account_uuid: a.account_uuid.into(),
        email: a.email.into(),
        parked: park.park.clone(),
        last_used_at: None,
        replaced_at: None,
        plan: None,
        detail: match provider {
            ProviderId::Claude => Detail::Claude {
                organization_uuid: a.organization_uuid.into(),
                oauth_account: json!({}),
            },
            ProviderId::Codex => Detail::Codex { workspace_id: None },
            ProviderId::Desktop => Detail::Desktop {},
        },
    }
}

fn desktop_account(label: &str, park: Option<Park>) -> Account {
    Account {
        label: label.into(),
        id: format!("desktop_{label}"),
        account_uuid: label.into(),
        email: String::new(),
        parked: park,
        last_used_at: None,
        replaced_at: None,
        plan: None,
        detail: Detail::Desktop {},
    }
}

/// What `pitboard doctor` finds on a healthy Mac in the site's world, each moment written in
/// the world's +07:00.
pub fn healthy_mac() -> Vec<Check> {
    site_machine(Os::MacOs)
}

/// The same on a healthy PC, which has no Claude Desktop for Pitboard.
pub fn healthy_pc() -> Vec<Check> {
    site_machine(Os::Windows)
}

/// The same on a healthy Linux machine.
pub fn healthy_linux() -> Vec<Check> {
    site_machine(Os::Linux)
}

// Each system's own paths, written as it writes them on every system.
fn site_path(os: Os, path: &str) -> PathBuf {
    let (home, separator) = match os {
        Os::MacOs => ("/Users/me", "/"),
        Os::Linux => ("/home/me", "/"),
        Os::Windows => ("C:\\Users\\me", "\\"),
    };
    if path.is_empty() {
        return PathBuf::from(home);
    }
    PathBuf::from(format!("{home}{separator}{}", path.replace('/', separator)))
}

fn site_machine(os: Os) -> Vec<Check> {
    let home = |path: &str| site_path(os, path);
    let signed_in = |fact: ParkFact| ParkFact {
        active: true,
        park: None,
        ..fact
    };
    let parks = vec![
        signed_in(site_park(ProviderId::Claude, PERSONAL.label, None)),
        site_park(ProviderId::Claude, WORK.label, Some(WORK_REFRESH_UNTIL)),
        signed_in(site_park(ProviderId::Codex, MAIN.label, None)),
        site_park(ProviderId::Codex, SPARE.label, None),
    ];
    let office_park = Park {
        service: "pitboard-park-desktop_office-1".into(),
        kind: crate::state::ParkKind::Folder,
        parked_at: SITE_NOW - 86_400,
        login: "f".into(),
        access_expires_at: None,
        refresh_expires_at: None,
        session_expires_at: Some(OFFICE_SESSION_UNTIL),
    };
    let mut accounts = vec![
        site_account(&PERSONAL, ProviderId::Claude, &parks[0]),
        site_account(&WORK, ProviderId::Claude, &parks[1]),
        site_account(&MAIN, ProviderId::Codex, &parks[2]),
        site_account(&SPARE, ProviderId::Codex, &parks[3]),
    ];
    let mut parks = parks;
    let desktop = (os == Os::MacOs).then(|| {
        accounts.push(desktop_account("home", None));
        accounts.push(desktop_account("office", Some(office_park.clone())));
        parks.push(ParkFact {
            provider: ProviderId::Desktop,
            last_used_at: None,
            name: Key::new(ProviderId::Desktop, "office").typed(),
            active: false,
            park: Some(office_park.clone()),
            unreadable: None,
        });
        let home_owner = crate::provider::Owner {
            account_uuid: "home".into(),
            email: String::new(),
            organization_uuid: String::new(),
        };
        super::desktop::DesktopFacts {
            root: Ok(home("Library/Application Support/Claude")),
            own_home: None,
            refused: None,
            present: true,
            settings: Some(Ok((crate::provider::folder::Sidecar::None, true))),
            unread: Vec::new(),
            known: Some(crate::in_use::Known {
                last: Some(crate::in_use::InUse {
                    owner: Some(home_owner.clone()),
                    login: "f".into(),
                    known_at: HOME_READ_AT,
                    named: Some("desktop_home".into()),
                }),
                doubt: None,
                own_record: Some(home_owner),
            }),
            signed_out: false,
            parks_dir: home(".pitboard/parks"),
            private: Some(crate::host::Private::Yes),
            one_volume: Some(true),
            parked_here: vec![("pitboard-park-desktop_office-1".into(), Ok(true))],
            build: Some(crate::provider::desktop::assumptions::VERIFIED_AGAINST.into()),
        }
    });
    let personal = site_owner(&PERSONAL);
    let claude_code = crate::provider::claude_code();
    let stored = json!({"claudeAiOauth": {"refreshToken": refresh_token(PERSONAL.label)}});
    let in_use = crate::in_use::Known {
        last: Some(crate::in_use::InUse {
            owner: Some(personal.clone()),
            login: claude_code.fingerprint(&stored),
            known_at: SITE_NOW,
            named: Some(crate::state::new_id(ProviderId::Claude, &personal)),
        }),
        doubt: None,
        own_record: Some(personal),
    };
    let codex = CodexFacts {
        home: home(".codex"),
        auth_file: home(".codex/auth.json"),
        enrolled: 2,
        login: Ok(Some(CodexLogin {
            email: MAIN.email.into(),
            account_id: MAIN.account_uuid.into(),
            fingerprint: store::fingerprint(&refresh_token(MAIN.label)),
        })),
        ..with_codex()
    };
    let facts = Facts {
        os,
        config_path: home(".claude.json"),
        config_account: Some(claude_paths::ConfigAccount {
            email: PERSONAL.email.into(),
            account_uuid: PERSONAL.account_uuid.into(),
            organization_uuid: PERSONAL.organization_uuid.into(),
            organization_name: None,
        }),
        storage_dir: home(".claude").display().to_string(),
        credential_file: home(".claude/.credentials.json"),
        credential: Ok(Some(json!({"claudeAiOauth": {
            "refreshToken": refresh_token(PERSONAL.label),
            "refreshTokenExpiresAt": PERSONAL_REFRESH_UNTIL * 1000
        }}))),
        home: match os {
            Os::Windows => home("AppData/Local/Pitboard"),
            Os::MacOs | Os::Linux => home(".pitboard"),
        },
        security_tool: (os == Os::MacOs).then(|| "/usr/bin/security".into()),
        account: "me".into(),
        claude_version: Some(super::read_from(ProviderId::Claude, os).into()),
        state: Ok(State {
            accounts,
            ..State::default()
        }),
        in_use: Some(in_use),
        parks,
        codex,
        desktop,
        now: SITE_NOW,
        ..facts()
    };
    let facts = match os {
        Os::MacOs => facts,
        Os::Linux => Facts {
            backend: Ok(store::Backend::File),
            vault: store::Backend::File,
            credential_cost: None,
            ..facts
        },
        Os::Windows => Facts {
            sid: Some(SITE_SID.into()),
            backend: Ok(store::Backend::CredentialManager),
            vault: store::Backend::SealedFile,
            home_access: Some(Access {
                shared: false,
                described: "open to you, SYSTEM and Administrators".into(),
            }),
            windows: Some(super::windows::WindowsHomes {
                home: home(""),
                default_pitboard_home: home("AppData/Local/Pitboard"),
                // Windows' answer, which a Mac cannot join a Windows path to
                local_app_data: None,
                profile: Some(home("")),
                codex_home_set: false,
                roams: false,
            }),
            codex: CodexFacts {
                auth_access: Some(Access {
                    shared: false,
                    described: "open to you, SYSTEM and Administrators".into(),
                }),
                ..facts.codex
            },
            windows_settings: Some(crate::host::WindowsSettings {
                wsl: Vec::new(),
                proxy: None,
                admin_approval: Some(true),
            }),
            ..facts
        },
    };
    let zone = jiff::tz::TimeZone::fixed(jiff::tz::offset(7));
    evaluate_with(&facts, &|at, now| crate::time::moment_in(&zone, at, now))
}
