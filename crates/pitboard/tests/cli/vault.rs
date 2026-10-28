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
        "Parked logins are kept in the login keyring, which every Pitboard directory of your \
         session shares.\n"
    );
}
