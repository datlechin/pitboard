//! OpenAI's Codex CLI, as one provider among several.
//!
//! Everything here is a fact about `codex` rather than about parking a login: where it
//! keeps its auth file, which store it is configured to use, how its login document is
//! shaped, which endpoint renews a refresh chain and which one says how much of a plan is
//! left.
//!
//! Read from codex-cli 0.154.0, and dated in [`assumptions`].

pub(crate) mod api;
pub mod assumptions;
pub(crate) mod engine;
pub(crate) mod paths;
