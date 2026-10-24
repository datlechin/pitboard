//! OpenAI's Codex CLI, as one provider among several.
//!
//! Everything here is a fact about `codex` rather than about parking a login: where it
//! keeps its auth file, which store it is configured to use, how its login document is
//! shaped, which endpoint renews a refresh chain and which one says how much of a plan is
//! left.
//!
//! Read from codex-cli 0.154.0, or the build an entry names, and dated in [`assumptions`].

pub(crate) mod api;
pub(crate) mod assumptions;
pub(crate) mod engine;
pub(crate) mod holders;
pub(crate) mod layers;
pub(crate) mod paths;

// What every test's Codex login is made from, in the shape `codex login` writes one.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn login_for_a_test(
    account: &str,
    email: &str,
    access: &str,
    refresh: &str,
) -> serde_json::Value {
    serde_json::json!({
        "auth_mode": "chatgpt",
        "OPENAI_API_KEY": null,
        "tokens": {
            "id_token": crate::provider::jwt::unsigned(&serde_json::json!({
                "email": email,
                "https://api.openai.com/auth": {
                    "chatgpt_account_id": account,
                    "chatgpt_user_id": format!("user-{account}"),
                    "chatgpt_plan_type": "pro",
                },
            })),
            "access_token": access,
            "refresh_token": refresh,
            "account_id": account,
        },
        "last_refresh": "2025-10-09T08:00:00Z",
    })
}
