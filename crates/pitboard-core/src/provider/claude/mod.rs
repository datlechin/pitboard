//! Claude Code, as one provider among several.
//!
//! Everything here is a fact about Anthropic's tool rather than about parking a login:
//! where it keeps its config and its credential, how it derives the keychain item's name
//! from a directory, which keys of its credential document belong to the account, how its
//! config file caches an identity that has to be corrected after a switch, and the
//! supervisor daemon that writes the credential behind a session's back.
//!
//! All of it was read out of one build and is dated in [`assumptions`](crate::assumptions).
//! None of it transfers to another tool: the next provider's equivalents have to be read
//! out of its own build the same way.

pub mod assumptions;
pub(crate) mod configfile;
pub(crate) mod daemon;
pub(crate) mod document;
pub(crate) mod engine;
pub(crate) mod live;
pub(crate) mod paths;
pub(crate) mod slot;
