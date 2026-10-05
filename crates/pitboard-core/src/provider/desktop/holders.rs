//! What holds Claude Desktop's data folder while it runs.

use super::paths::{APP_NAME, BUNDLE_ID};
use crate::holder::{Holder, Location, Noun, Remedy};

/// What holds Claude Desktop's data folder: the app, every process of which runs from its
/// bundle. Written after the ChatGPT app's holder for Codex.
///
/// Every process asked about runs from the app's bundle already, by name or by the path a
/// test or an app gave, so each one is the app wherever it is and whatever the bundle is
/// called.
pub(crate) const HOLDERS: &[Holder] = &[Holder {
    kind: "claude_desktop_app",
    noun: Noun::One("the Claude app"),
    location: Location::Anywhere,
    remedy: Remedy::ReopenApp {
        bundle_id: BUNDLE_ID,
        name: APP_NAME,
    },
}];
