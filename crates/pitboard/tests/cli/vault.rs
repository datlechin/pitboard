//! `pitboard vault`, run as a person or a tool runs it: where this Pitboard directory keeps its
//! parked logins, on each system.

use crate::common::{Env, envelope, two_accounts};
use pitboard_core::host::{OS, Os};
use serde_json::Value;

fn json(env: &Env, args: &[&str]) -> (Value, i32) {
    let mut with_json = args.to_vec();
    with_json.push("--json");
    let (out, _, code) = env.run(&with_json);
    (envelope(&out), code)
}

// The test's own folder where there is one: a null is part of the contract too.
fn path() -> insta::internals::Redaction {
    insta::dynamic_redaction(|value, _| {
        if value.is_nil() {
            value
        } else {
            insta::internals::Content::from("[path]")
        }
    })
}

// Words for a person are not the contract: the code beside each is.
macro_rules! contract {
    ($name:literal, $value:expr, $code:expr) => {
        insta::assert_json_snapshot!($name, serde_json::json!({ "exit": $code, "envelope": $value }), {
            ".envelope.data.path" => path(),
            ".envelope.error.message" => "[message]",
            ".envelope.warnings[].message" => "[message]",
        });
    };
}

#[test]
fn vault_says_where_this_directory_keeps_its_parked_logins() {
    let env = two_accounts("vault-status");
    let (value, code) = json(&env, &["vault"]);
    match OS {
        Os::MacOs => {
            contract!("vault_status", value, code);
        }
        Os::Linux => {
            contract!("vault_status_on_linux", value, code);
        }
        Os::Windows => {
            contract!("vault_status_on_windows", value, code);
        }
    }

    let (out, err, code) = env.run(&["vault"]);
    assert_eq!(code, 0, "{err}");
    let folder = env.root.join("pitboard").join("vault");
    let said = match OS {
        Os::MacOs => "Parked logins are kept in the login keychain, which every Pitboard \
                      directory on this Mac shares.\n"
            .to_string(),
        Os::Linux => format!(
            "Parked logins are kept in 0600 files in {}, which only you can read.\n\
             No Secret Service answers on this session's bus, as over SSH or in a container.\n",
            folder.display()
        ),
        Os::Windows => format!(
            "Parked logins are kept in files in {}, sealed to your Windows account.\n",
            folder.display()
        ),
    };
    assert_eq!(out, said);
    assert_eq!(env.run(&["vault", "status"]), (out, err, code));
}

// windows: the Secret Service is Linux's own
#[cfg(target_os = "linux")]
#[test]
fn vault_says_the_login_keyring_where_a_new_directory_parks_in_it() {
    let env = crate::common::two_accounts_in(Env::with_secret_service("vault-status-keyring"));
    let (value, code) = json(&env, &["vault"]);
    contract!("vault_status_in_the_secret_service", value, code);

    let (out, err, code) = env.run(&["vault"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        out,
        format!(
            "Parked logins are kept in the login keyring, which every Pitboard directory of your \
             session shares.\n\
             `pitboard vault move files` keeps them in 0600 files in {} instead.\n",
            env.root.join("pitboard").join("vault").display()
        )
    );
}

// macOS keeps parked logins in the keychain alone, Windows in sealed files alone, and on Linux a
// directory nothing answers for in the Secret Service stays in files.
#[test]
fn vault_move_is_refused_where_it_cannot_move_and_changes_nothing() {
    let env = two_accounts("vault-move-refused");
    let before = (env.state(), env.parked_service("beta"));
    let to = match OS {
        Os::Linux => "secret-service",
        Os::MacOs | Os::Windows => "files",
    };

    let (value, code) = json(&env, &["vault", "move", to]);

    match OS {
        Os::MacOs | Os::Windows => {
            contract!("vault_move_unsupported", value, code);
        }
        Os::Linux => {
            contract!("vault_move_with_no_keyring", value, code);
        }
    }
    assert_eq!((env.state(), env.parked_service("beta")), before);
    let beta = before.1.expect("beta is parked");
    assert!(env.is_parked(&beta), "{beta} is where it was");
}

// windows: the Secret Service is Linux's own
#[cfg(target_os = "linux")]
#[test]
fn vault_move_moves_every_parked_login_into_files_and_back() {
    let env = crate::common::two_accounts_in(Env::with_secret_service("vault-move"));
    let beta = env.parked_service("beta").expect("beta is parked");
    let file = env.root.join("pitboard/vault").join(format!("{beta}.json"));
    let folder = env.root.join("pitboard/vault");

    let (value, code) = json(&env, &["vault", "move", "files"]);
    contract!("vault_move_to_files", value, code);
    assert_eq!(env.state()["vault"], serde_json::json!({"form": "files"}));
    assert!(env.secret_service().names().is_empty());
    assert!(file.is_file(), "{} holds beta's login", file.display());
    let (out, err, code) = env.run(&["vault"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        out,
        format!(
            "Parked logins are kept in 0600 files in {}, which only you can read.\n\
             A Secret Service answers on this session's bus: `pitboard vault move secret-service` \
             keeps them in the login keyring instead.\n",
            folder.display()
        )
    );

    let (out, err, code) = env.run(&["vault", "move", "secret-service"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, "Moved 1 parked login(s) into the login keyring.\n");
    assert_eq!(
        env.state()["vault"],
        serde_json::json!({"form": "secret_service"})
    );
    assert_eq!(env.secret_service().names(), vec![beta.clone()]);
    assert!(!file.exists(), "{} is gone", file.display());
    let (value, code) = json(&env, &["vault", "move", "secret-service"]);
    contract!("vault_move_kept_already", value, code);

    let (_, err, code) = env.run(&["use", "beta"]);
    assert_eq!(code, 0, "switch to beta: {err}");
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-b");
    let alpha = env.parked_service("alpha").expect("alpha is parked");
    assert_eq!(env.secret_service().names(), vec![alpha]);
}

// windows: the Secret Service is Linux's own
#[cfg(target_os = "linux")]
#[test]
fn vault_move_out_of_a_locked_keyring_is_refused_and_changes_nothing() {
    let env = crate::common::two_accounts_in(Env::with_secret_service("vault-move-locked"));
    env.secret_service().lock();
    let before = (env.state(), env.secret_service().names());

    let (value, code) = json(&env, &["vault", "move", "files"]);

    assert_eq!(code, 1, "{value}");
    assert_eq!(value["error"]["code"], "credential_store_locked", "{value}");
    assert_eq!((env.state(), env.secret_service().names()), before);
    assert!(!env.root.join("pitboard/vault").exists());
}

// windows: the Secret Service is Linux's own
#[cfg(target_os = "linux")]
#[test]
fn doctor_says_the_login_keyring_and_warns_once_it_is_locked() {
    let env = crate::common::two_accounts_in(Env::with_secret_service("vault-doctor"));
    let vault = |env: &Env| {
        let (value, _) = json(env, &["doctor"]);
        let checks = value["data"]["checks"].as_array().expect("checks").clone();
        let vault = checks
            .into_iter()
            .find(|check| check["code"] == "vault")
            .unwrap_or_else(|| panic!("no vault check: {value}"));
        (vault["level"].clone(), vault["detail"].clone())
    };

    assert_eq!(
        vault(&env),
        (
            "ok".into(),
            "in the login keyring, which every Pitboard directory of your session shares".into()
        )
    );
    env.secret_service().lock();
    assert_eq!(
        vault(&env),
        (
            "warn".into(),
            "in the login keyring, which is locked or which nothing answers for on this \
             session's bus"
                .into()
        )
    );
}

// windows: the Secret Service is Linux's own
#[cfg(target_os = "linux")]
#[test]
fn vault_and_doctor_say_kwallet_where_ksecretd_answers_for_the_secret_service() {
    let env = Env::with_ksecretd("vault-kwallet");

    let (out, err, code) = env.run(&["vault"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        out,
        format!(
            "Parked logins are kept in KWallet, which every Pitboard directory of your session \
             shares.\n\
             `pitboard vault move files` keeps them in 0600 files in {} instead.\n",
            env.root.join("pitboard").join("vault").display()
        )
    );
    let (value, _) = json(&env, &["doctor"]);
    let checks = value["data"]["checks"].as_array().expect("checks");
    let vault = (checks.iter())
        .find(|check| check["code"] == "vault")
        .unwrap_or_else(|| panic!("no vault check: {value}"));
    assert_eq!(
        (vault["level"].clone(), vault["detail"].clone()),
        (
            "warn".into(),
            "in KWallet, which is locked or which nothing answers for on this session's bus".into()
        )
    );
    let said = value.to_string();
    assert!(!said.contains("login keyring"), "{said}");
}
