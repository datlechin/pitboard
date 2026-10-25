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
pub(crate) fn codex_parked(label: &str, refresh_expires_at: Option<i64>) -> ParkFact {
    ParkFact {
        provider: ProviderId::Codex,
        name: Key::new(ProviderId::Codex, label).typed(),
        ..parked(label, refresh_expires_at)
    }
}

/// What doctor finds on a Mac with Claude Code and Codex signed in, two accounts of each
/// enrolled and one of each parked, each moment written in UTC.
pub fn healthy_mac() -> Vec<Check> {
    // A Mac's paths, written with slashes on every system.
    let home = |path: &str| PathBuf::from(format!("/Users/x/{path}"));
    let in_use = in_use_as_named();
    let owner = in_use.own_record.clone().expect("named");
    let codex = CodexFacts {
        home: home(".codex"),
        auth_file: home(".codex/auth.json"),
        enrolled: 2,
        ..with_codex()
    };
    let signed_in = |fact: ParkFact| ParkFact {
        active: true,
        park: None,
        ..fact
    };
    let refresh_expires_at = Some(NOW + 26 * 86_400 + 4 * 3_600);
    let parks = vec![
        signed_in(parked("personal", None)),
        parked("work", refresh_expires_at),
        signed_in(codex_parked("main", None)),
        codex_parked("spare", refresh_expires_at),
    ];
    let account = |label: &str, park: &ParkFact, account_uuid: &str, detail| Account {
        label: label.into(),
        id: format!("{label}-id"),
        account_uuid: account_uuid.into(),
        email: String::new(),
        parked: park.park.clone(),
        last_used_at: None,
        replaced_at: None,
        plan: None,
        detail,
    };
    let claude = || Detail::Claude {
        organization_uuid: owner.organization_uuid.clone(),
        oauth_account: json!({}),
    };
    let codex_detail = || Detail::Codex { workspace_id: None };
    let mut personal = account("personal", &parks[0], &owner.account_uuid, claude());
    personal.id = crate::state::new_id(ProviderId::Claude, &owner);
    let state = State {
        accounts: vec![
            personal,
            account("work", &parks[1], "work", claude()),
            account("main", &parks[2], "main", codex_detail()),
            account("spare", &parks[3], "spare", codex_detail()),
        ],
        ..State::default()
    };
    let facts = Facts {
        os: Os::MacOs,
        config_path: home(".claude.json"),
        storage_dir: "/Users/x/.claude".into(),
        credential_file: home(".claude/.credentials.json"),
        home: home(".pitboard"),
        state: Ok(state),
        in_use: Some(in_use),
        parks,
        codex,
        ..facts()
    };
    let utc = jiff::tz::TimeZone::UTC;
    evaluate_with(&facts, &|at, now| crate::time::moment_in(&utc, at, now))
}
