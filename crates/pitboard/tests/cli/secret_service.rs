//! The command line parking in a Secret Service on Linux: gnome-keyring on a bus the test
//! started, which each command is given as `PITBOARD_TEST_BUS`, as a desktop session's keyring
//! answers on the session's bus.

use crate::common::{Env, two_accounts_in};

fn parks(env: &Env) -> Vec<String> {
    env.secret_service()
        .items()
        .into_iter()
        .map(|item| item.attributes["service"].clone())
        .collect()
}

#[test]
fn a_switch_parks_and_puts_back_through_the_secret_service() {
    let env = two_accounts_in(Env::with_secret_service("secret-service-switch"));
    assert_eq!(
        env.state()["vault"],
        serde_json::json!({"form": "secret_service"})
    );
    let beta = env.parked_service("beta").expect("beta is parked");
    assert_eq!(parks(&env), vec![beta]);

    let (_, err, code) = env.run(&["use", "beta"]);
    assert_eq!(code, 0, "switch to beta: {err}");
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-b");
    let alpha = env.parked_service("alpha").expect("alpha is parked");
    assert_eq!(parks(&env), vec![alpha]);

    let (_, err, code) = env.run(&["use", "alpha"]);
    assert_eq!(code, 0, "switch back: {err}");
    assert_eq!(env.live()["claudeAiOauth"]["refreshToken"], "refresh-a");
    let beta = env.parked_service("beta").expect("beta is parked again");
    assert_eq!(parks(&env), vec![beta]);
    assert!(
        !env.root.join("pitboard/vault").exists(),
        "no park is a file"
    );
}
