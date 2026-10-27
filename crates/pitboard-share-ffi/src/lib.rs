//! What the macOS Share extension and the Windows share target ask of Rust, as UniFFI bindings:
//! whether a shared page is one Pitboard opens, the Pitboard link that hands it to the app, and
//! what each says.
//!
//! The extension is sandboxed and links this alone, never `pitboard-ffi`: the core's bindings
//! check every export's checksum when they load, so linking them would bring the whole core
//! in. It is the same rule the app reads a Pitboard link with, from `pitboard-sites`, so the
//! two never disagree about a link. The app checks the link again all the same, since
//! anything on the Mac can open a Pitboard link.

uniffi::setup_scaffolding!();

/// Why a shared page is not handed to the app.
#[derive(Debug, PartialEq, Eq, uniffi::Error)]
pub enum ShareRefusal {
    /// Not a page Pitboard opens, and why, in the sentence the app's **Open Link** window
    /// shows for the same link.
    Refused { reason: String },
}

impl std::fmt::Display for ShareRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShareRefusal::Refused { reason } => f.write_str(reason),
        }
    }
}

impl std::error::Error for ShareRefusal {}

/// Every sentence the Share extension and the share target show beside a refusal's own.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ShareWords {
    // the extension's, while the app opens the page
    pub opening: String,
    // the extension's title over a refusal
    pub refused: String,
    // the extension's title over why the app did not open
    pub failed: String,
    pub ok: String,
    // the extension's, outside a Pitboard app
    pub no_app: String,
    // the share target's, for a share whose link could not be read
    pub not_read: String,
    // the share target's, where Pitboard.exe did not start
    pub not_started: String,
}

#[uniffi::export]
pub fn share_words() -> ShareWords {
    ShareWords {
        opening: "Opening in Pitboard…".into(),
        refused: pitboard_sites::LinkRefusal::TITLE.into(),
        failed: "Couldn’t Open Pitboard".into(),
        ok: "OK".into(),
        no_app: "Pitboard couldn’t find the app this extension came with.".into(),
        not_read: "Pitboard couldn’t read the link that was shared. Share the page again.".into(),
        not_started: "Pitboard couldn’t start the app that opens the page.".into(),
    }
}

/// The Pitboard link of `scheme` that hands the page `text` to the app, once `text` is
/// checked as a link from outside: one of the sites' own pages, never its sign-in.
#[uniffi::export]
pub fn share_link(text: String, scheme: String) -> Result<String, ShareRefusal> {
    let link = pitboard_sites::SiteLink::parse(&text).map_err(|refusal| ShareRefusal::Refused {
        reason: refusal.to_string(),
    })?;
    Ok(pitboard_sites::pitboard_link(&link, &scheme))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page of one of the sites goes to the app as the Pitboard link the app reads back.
    #[test]
    fn a_sites_page_is_handed_over_as_a_pitboard_link() {
        let handed = share_link(
            "https://chat.openai.com/c/x?y=1#z".into(),
            "pitboard-debug".into(),
        )
        .expect("a chatgpt.com page");
        assert_eq!(
            handed,
            "pitboard-debug://open?url=https%3A%2F%2Fchatgpt.com%2Fc%2Fx%3Fy%3D1%23z"
        );
        let read = pitboard_sites::read_pitboard_link(&handed, "pitboard-debug").expect("readable");
        assert_eq!(read.url(), "https://chatgpt.com/c/x?y=1#z");
    }

    #[test]
    fn the_share_extension_and_the_share_target_say_these() {
        assert_eq!(
            share_words(),
            ShareWords {
                opening: "Opening in Pitboard…".into(),
                refused: pitboard_sites::LinkRefusal::TITLE.into(),
                failed: "Couldn’t Open Pitboard".into(),
                ok: "OK".into(),
                no_app: "Pitboard couldn’t find the app this extension came with.".into(),
                not_read: "Pitboard couldn’t read the link that was shared. Share the page again."
                    .into(),
                not_started: "Pitboard couldn’t start the app that opens the page.".into(),
            }
        );
    }

    /// A page that is not one says why in the words the app's picker uses.
    #[test]
    fn a_refusal_is_said_as_the_app_says_it() {
        for (text, refusal) in [
            (
                "https://example.com/",
                pitboard_sites::LinkRefusal::NotASite {
                    host: Some("example.com".into()),
                },
            ),
            (
                "https://claude.ai/magic-link#a:b",
                pitboard_sites::LinkRefusal::SignInLink(&pitboard_sites::CLAUDE),
            ),
            ("", pitboard_sites::LinkRefusal::NoLink),
        ] {
            assert_eq!(
                share_link(text.into(), "pitboard".into()),
                Err(ShareRefusal::Refused {
                    reason: refusal.to_string()
                }),
                "{text}"
            );
        }
        assert_eq!(
            share_link("https://claude.ai/magic-link#a:b".into(), "pitboard".into())
                .expect_err("a sign-in link")
                .to_string(),
            pitboard_sites::LinkRefusal::SignInLink(&pitboard_sites::CLAUDE).to_string()
        );
    }
}
