//! What a link from outside the app may be: a page of one of the sites, never its sign-in.

use crate::address;
use crate::site::{ALL, Conjunction, Site};
use std::fmt;

/// `text` without the white space Foundation's `whitespacesAndNewlines` trims, U+200B
/// included, as the macOS app trimmed what was typed or shared (docs/facts/foundation-urls.md).
pub fn trimmed(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || c == '\u{200b}')
}

/// A link one of the sites opens: the site, and the address its window loads.
///
/// Made only by checking a link from outside the app, so holding one means the check passed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SiteLink {
    site: &'static Site,
    url: String,
}

/// Why a link from outside is not opened.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LinkRefusal {
    /// Nothing that reads as a link to a web page.
    NoLink,
    /// A link to a host none of the sites is, with the host when there is one to name.
    NotASite { host: Option<String> },
    /// A site's sign-in link, which would sign a window in as whoever it is for.
    SignInLink(&'static Site),
    /// Longer than any of the sites' links is.
    TooLong,
    /// A Pitboard link this version does not read: another scheme, another request, or a
    /// link inside it that was not encoded.
    Unreadable,
}

impl SiteLink {
    /// The longest link read, in characters. The sites' own links to a page are under 200
    /// characters, and anything longer would only fill the picker.
    pub const LONGEST: usize = 8192;

    /// `text` as a link one of the sites opens, or why it is not one.
    ///
    /// Spaces around it are dropped, a bare host of a site is taken as `https`, and `http` as
    /// `https`. Only a site's own host or one of its aliases is accepted: no other host, no
    /// subdomain, no port and no user information. A site's sign-in link is refused as well,
    /// since opened in an account's window somebody else's would sign that window in as them.
    ///
    /// The link is read as Foundation's `URLComponents` reads it, which is how the macOS app
    /// read it before this rule was Rust: see the `address` module.
    pub fn parse(text: &str) -> Result<SiteLink, LinkRefusal> {
        let trimmed = trimmed(text);
        if trimmed.chars().count() > Self::LONGEST {
            return Err(LinkRefusal::TooLong);
        }
        let bare = trimmed.to_lowercase();
        let bare_host = ALL.iter().flat_map(|site| site.hosts()).any(|host| {
            bare.strip_prefix(host)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?', '#']))
        });
        let typed = if bare_host {
            format!("https://{trimmed}")
        } else {
            trimmed.to_owned()
        };
        let parts = address::parse(&typed).ok_or(LinkRefusal::NoLink)?;
        if !["https", "http"].contains(&parts.scheme.to_ascii_lowercase().as_str()) {
            return Err(LinkRefusal::NoLink);
        }
        let authority = parts.authority.as_ref().ok_or(LinkRefusal::NoLink)?;
        let host = authority.host.as_deref().unwrap_or_default().to_lowercase();
        if host.is_empty() {
            return Err(LinkRefusal::NoLink);
        }
        let Some(site) = Site::serving(&host) else {
            return Err(LinkRefusal::NotASite { host: Some(host) });
        };
        if let Some(port) = &authority.port {
            return Err(LinkRefusal::NotASite {
                host: Some(format!("{host}:{port}")),
            });
        }
        if authority.user_info {
            return Err(LinkRefusal::NotASite { host: None });
        }
        if let Some(refusal) = refusal_of_path(&parts.decoded_path(), site) {
            return Err(refusal);
        }
        let path = if parts.path.is_empty() {
            "/"
        } else {
            &parts.path
        };
        let mut url = format!("https://{}{path}", site.host);
        if let Some(query) = &parts.query {
            url.push('?');
            url.push_str(query);
        }
        if let Some(fragment) = &parts.fragment {
            url.push('#');
            url.push_str(fragment);
        }
        Ok(SiteLink { site, url })
    }

    /// The site the link is on.
    pub fn site(&self) -> &'static Site {
        self.site
    }

    /// The link on the site's own host over `https`, with its path, query and fragment.
    pub fn url(&self) -> &str {
        &self.url
    }
}

/// Why a path of `site`'s, percent-decoded, is not opened, or `None` when it may be.
///
/// Checked by segment, as WebKit loads it: WebKit drops `.` and `..` segments, `%2e` included,
/// before the request is sent, and Foundation keeps them. The sites' own links never have one,
/// so a path with a dot segment is refused, as a sign-in link when that is where it leads.
/// Empty segments are skipped, so `//magic-link`, which a server merging slashes would route to
/// sign-in, is refused as well.
fn refusal_of_path(path: &str, site: &'static Site) -> Option<LinkRefusal> {
    if site.signs_in(path) {
        return Some(LinkRefusal::SignInLink(site));
    }
    path.split('/')
        .any(|segment| segment == "." || segment == "..")
        .then_some(LinkRefusal::NoLink)
}

/// The sentence the Share extension and the account picker both show.
impl fmt::Display for LinkRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let or = Site::names(Conjunction::Or);
        let and = Site::names(Conjunction::And);
        match self {
            LinkRefusal::NoLink => write!(f, "There is no {or} link in what was shared."),
            LinkRefusal::NotASite { host: Some(host) } => {
                write!(
                    f,
                    "Pitboard opens {and} links only. This link is on {host}."
                )
            }
            LinkRefusal::NotASite { host: None } => write!(f, "Pitboard opens {and} links only."),
            LinkRefusal::SignInLink(site) => {
                let name = site.name();
                write!(
                    f,
                    "Pitboard doesn’t open {name} sign-in links from outside: one would sign \
                     the window in as whoever the link belongs to. Sign in inside the \
                     account’s {name} window."
                )
            }
            LinkRefusal::TooLong => {
                write!(f, "What was shared is too long to be a {or} link.")
            }
            LinkRefusal::Unreadable => write!(
                f,
                "This Pitboard link isn’t one this version of Pitboard can read. Update \
                 Pitboard and share the page again."
            ),
        }
    }
}

impl std::error::Error for LinkRefusal {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::site::{CHATGPT, CLAUDE};

    /// The link `text` opens, as the window loads it, or `None` when it is refused.
    fn accepted(text: &str) -> Option<String> {
        SiteLink::parse(text).ok().map(|link| link.url)
    }

    fn site(text: &str) -> Option<&'static Site> {
        SiteLink::parse(text).ok().map(|link| link.site)
    }

    fn refusal(text: &str) -> Option<LinkRefusal> {
        SiteLink::parse(text).err()
    }

    fn not_a_site(host: &str) -> Option<LinkRefusal> {
        Some(LinkRefusal::NotASite {
            host: Some(host.to_owned()),
        })
    }

    // The vectors of the Swift SiteLinkTests, each with the answer it had there.

    #[test]
    fn only_the_sites_own_links_are_accepted() {
        let same = |text: &str| assert_eq!(accepted(text).as_deref(), Some(text), "{text}");
        same("https://claude.ai/new");
        assert_eq!(
            accepted("  https://claude.ai/new\n").as_deref(),
            Some("https://claude.ai/new")
        );
        assert_eq!(
            accepted("claude.ai/chat/abc").as_deref(),
            Some("https://claude.ai/chat/abc")
        );
        assert_eq!(
            accepted("Claude.AI/chat/abc").as_deref(),
            Some("https://claude.ai/chat/abc")
        );
        assert_eq!(accepted("claude.ai").as_deref(), Some("https://claude.ai/"));
        assert_eq!(
            accepted("http://claude.ai/chat/abc").as_deref(),
            Some("https://claude.ai/chat/abc")
        );
        assert_eq!(
            accepted("HTTPS://CLAUDE.AI/new").as_deref(),
            Some("https://claude.ai/new")
        );
        assert_eq!(
            accepted("https://claude.ai").as_deref(),
            Some("https://claude.ai/")
        );
        same("https://claude.ai/public/artifacts/0e5a?x=1&y=%20#frag");
        same("https://chatgpt.com/c/68d1?model=x");
        assert_eq!(
            accepted("chatgpt.com/share/abc").as_deref(),
            Some("https://chatgpt.com/share/abc")
        );
        assert_eq!(
            accepted("chatgpt.com").as_deref(),
            Some("https://chatgpt.com/")
        );
        same("https://chatgpt.com/codex/tasks/t1");

        assert_eq!(refusal("https://example.com/"), not_a_site("example.com"));
        assert_eq!(
            refusal("example.com/x"),
            Some(LinkRefusal::NoLink),
            "only a site's host is taken without a scheme"
        );
        assert_eq!(
            refusal("https://claude.ai.evil.example/"),
            not_a_site("claude.ai.evil.example")
        );
        assert_eq!(
            refusal("https://evilclaude.ai/"),
            not_a_site("evilclaude.ai")
        );
        assert_eq!(
            refusal("https://sub.claude.ai/"),
            not_a_site("sub.claude.ai")
        );
        assert_eq!(
            refusal("https://sub.chatgpt.com/"),
            not_a_site("sub.chatgpt.com")
        );
        assert_eq!(
            refusal("https://claude.ai:8443/"),
            not_a_site("claude.ai:8443")
        );
        let anonymous = Some(LinkRefusal::NotASite { host: None });
        assert_eq!(refusal("https://x@claude.ai/"), anonymous);
        assert_eq!(refusal("https://x:y@chatgpt.com/"), anonymous);
        assert_eq!(
            refusal("https://claude.ai@evil.example/"),
            not_a_site("evil.example"),
            "the host is what follows the @"
        );
        for nothing in [
            "javascript:alert(1)",
            "data:text/html,hi",
            "file:///etc/hosts",
            "pitboard-fixture://claude.ai/x",
            "",
            "   ",
            "nothing to open",
        ] {
            assert_eq!(refusal(nothing), Some(LinkRefusal::NoLink), "{nothing:?}");
        }
        assert_eq!(
            refusal(&format!("https://claude.ai/{}", "a".repeat(8200))),
            Some(LinkRefusal::TooLong)
        );
    }

    #[test]
    fn each_link_belongs_to_its_own_site() {
        assert_eq!(site("https://claude.ai/new"), Some(&CLAUDE));
        assert_eq!(site("https://chatgpt.com/"), Some(&CHATGPT));
    }

    /// Measured on 29 September 2026: each of these answers with a redirect to the same path
    /// on chatgpt.com, so a link to one is a chatgpt.com link, opened on chatgpt.com itself.
    #[test]
    fn chatgpts_other_hosts_are_taken_as_chatgpt() {
        for alias in ["chat.openai.com", "www.chatgpt.com", "chat.com"] {
            assert_eq!(site(&format!("https://{alias}/c/abc")), Some(&CHATGPT));
            assert_eq!(
                accepted(&format!("https://{alias}/c/abc?x=1#y")).as_deref(),
                Some("https://chatgpt.com/c/abc?x=1#y")
            );
            assert_eq!(
                accepted(&format!("{alias}/share/x")).as_deref(),
                Some("https://chatgpt.com/share/x")
            );
        }
        assert_eq!(refusal("https://sub.chat.com/"), not_a_site("sub.chat.com"));
    }

    /// OpenAI's sign-in pages are on a host of their own. A link to one, such as the page that
    /// approves a Codex sign-in on another device, is no chatgpt.com link and opens nothing.
    #[test]
    fn openais_sign_in_host_is_not_a_site() {
        assert_eq!(
            refusal("https://auth.openai.com/codex/device"),
            not_a_site("auth.openai.com")
        );
    }

    /// Somebody else's sign-in link would sign the window in as whoever it belongs to, while
    /// the window stays titled with the person's own label.
    #[test]
    fn a_sign_in_link_is_refused_from_outside() {
        for text in [
            "https://claude.ai/magic-link#a:b",
            "https://claude.ai/MAGIC-LINK",
            "https://claude.ai/magic-link/x",
            "https://claude.ai/magic-link/",
            "https://claude.ai/magic%2Dlink#a:b",
            "claude.ai/magic-link#a:b",
            // WebKit drops dot segments, `%2e` included, before it loads a link, and keeps an
            // empty segment that a server merging slashes would route to sign-in all the same.
            "https://claude.ai/./magic-link#a:b",
            "https://claude.ai/x/../magic-link#a:b",
            "https://claude.ai/a/../magic-link#a:b",
            "https://claude.ai/%2e/magic-link#a:b",
            "claude.ai/%2e%2e/magic-link#a:b",
            "claude.ai/%2E%2E/magic-link#a:b",
            "https://claude.ai/./magic-link",
            "https://claude.ai//magic-link#a:b",
            "https://claude.ai/%2Fmagic-link#a:b",
        ] {
            assert_eq!(
                refusal(text),
                Some(LinkRefusal::SignInLink(&CLAUDE)),
                "{text}"
            );
        }
        assert!(accepted("https://claude.ai/magic-links-guide").is_some());
        assert!(accepted("https://claude.ai/chat/magic-link").is_some());
    }

    /// chatgpt.com's sign-in comes back through `/api/auth`, on chatgpt.com and its other
    /// hosts.
    #[test]
    fn a_chatgpt_sign_in_link_is_refused_from_outside() {
        for text in [
            "https://chatgpt.com/api/auth/callback/openai?code=x",
            "https://chatgpt.com/API/Auth/session",
            "https://chatgpt.com/api/auth",
            "https://chatgpt.com//api//auth/x",
            "https://chatgpt.com/x/../api/auth/x",
            "https://chat.openai.com/api/auth/callback/openai",
        ] {
            assert_eq!(
                refusal(text),
                Some(LinkRefusal::SignInLink(&CHATGPT)),
                "{text}"
            );
        }
        assert!(accepted("https://chatgpt.com/api/other").is_some());
        assert!(accepted("https://chatgpt.com/c/api/auth").is_some());
        assert!(
            accepted("https://claude.ai/api/auth").is_some(),
            "a path only chatgpt.com signs in on"
        );
    }

    /// The sites' own links never have a dot segment. One that does not lead to sign-in is
    /// refused too, rather than opened somewhere other than it reads.
    #[test]
    fn a_link_with_a_dot_segment_is_refused() {
        for text in [
            "https://claude.ai/chat/../new",
            "https://claude.ai/./new",
            "https://claude.ai/magic-link/..",
            "https://chatgpt.com/c/../x",
        ] {
            assert_eq!(refusal(text), Some(LinkRefusal::NoLink), "{text}");
        }
        assert_eq!(
            accepted("https://claude.ai/chat/a.b").as_deref(),
            Some("https://claude.ai/chat/a.b")
        );
        assert_eq!(
            accepted("https://claude.ai/x/...").as_deref(),
            Some("https://claude.ai/x/...")
        );
    }

    /// The Share extension and the picker say a refusal the same way, from its reason.
    #[test]
    fn each_refusal_says_why() {
        let said = |refusal: LinkRefusal| refusal.to_string();
        assert_eq!(
            said(LinkRefusal::NotASite {
                host: Some("example.com".into())
            }),
            "Pitboard opens claude.ai and chatgpt.com links only. This link is on example.com."
        );
        assert_eq!(
            said(LinkRefusal::NotASite { host: None }),
            "Pitboard opens claude.ai and chatgpt.com links only."
        );
        assert_eq!(
            said(LinkRefusal::SignInLink(&CHATGPT)),
            "Pitboard doesn’t open chatgpt.com sign-in links from outside: one would sign the \
             window in as whoever the link belongs to. Sign in inside the account’s \
             chatgpt.com window."
        );
        assert_eq!(
            said(LinkRefusal::NoLink),
            "There is no claude.ai or chatgpt.com link in what was shared."
        );
        assert_eq!(
            said(LinkRefusal::TooLong),
            "What was shared is too long to be a claude.ai or chatgpt.com link."
        );
        assert_eq!(
            said(LinkRefusal::Unreadable),
            "This Pitboard link isn’t one this version of Pitboard can read. Update Pitboard \
             and share the page again."
        );
    }

    // Where Foundation and the WHATWG URL standard read a link differently, it is read as
    // Foundation read it. Each answer here is the Swift SiteLink's, measured on macOS 27.

    #[test]
    fn a_link_is_read_as_foundation_reads_it() {
        let read =
            |text: &str, url: &str| assert_eq!(accepted(text).as_deref(), Some(url), "{text:?}");
        read(
            "https://claude.ai/x#one#two",
            "https://claude.ai/x#one%23two",
        );
        read("https://claude.ai/a\nb", "https://claude.ai/a%0Ab");
        read("https://claude.ai/a\tb", "https://claude.ai/a%09b");
        read("https://claude.ai/a\\b", "https://claude.ai/a%5Cb");
        read(
            "https://claude.ai/x?a='()!*'",
            "https://claude.ai/x?a='()!*'",
        );
        read(
            "https://claude.ai/x?a=\"q\"",
            "https://claude.ai/x?a=%22q%22",
        );
        read("https://claude.ai/x?a=b c", "https://claude.ai/x?a=b%20c");
        read(
            "https://claude.ai/é?é#é",
            "https://claude.ai/%C3%A9?%C3%A9#%C3%A9",
        );
        read(
            "https://claude.ai/[1]?a=[1]#[1]",
            "https://claude.ai/%5B1%5D?a=%5B1%5D#%5B1%5D",
        );
        read("https://claude.ai/%zz", "https://claude.ai/%25zz");
        read("https://claude.ai/%", "https://claude.ai/%25");
        read("https://claude.ai/x?", "https://claude.ai/x?");
        read("https://claude.ai/x#", "https://claude.ai/x#");
        read("https://claude.ai?x", "https://claude.ai/?x");
        read("claude.ai#x", "https://claude.ai/#x");
        read("https://claude.ai:/x", "https://claude.ai/x");
        read("https://cl%61ude.ai/x", "https://claude.ai/x");
        read("https://ｃｌａｕｄｅ.ai/x", "https://claude.ai/x");
        read("https://chat\u{ff0e}com/x", "https://chatgpt.com/x");
        read("\u{a0}https://claude.ai/x\u{3000}", "https://claude.ai/x");
        read("\u{200b}claude.ai\u{200b}", "https://claude.ai/");
        read(
            "\u{85}\u{2028}claude.ai/x\u{2029}\u{202f}",
            "https://claude.ai/x",
        );
        read(
            "https://claude.ai/magic-link;x",
            "https://claude.ai/magic-link;x",
        );
        read(
            "https://claude.ai/MAGIC-LINK%3Fx",
            "https://claude.ai/MAGIC-LINK%3Fx",
        );

        assert_eq!(
            refusal("https://CLAUDE.AI:443/x"),
            not_a_site("claude.ai:443")
        );
        assert_eq!(
            refusal("https://claude.ai:0443/x"),
            not_a_site("claude.ai:443")
        );
        assert_eq!(refusal("https://claude.ai:0/x"), not_a_site("claude.ai:0"));
        assert_eq!(refusal("https://claude.ai./x"), not_a_site("claude.ai."));
        assert_eq!(refusal("https://bücher.de/x"), not_a_site("bücher.de"));
        assert_eq!(
            refusal("https://xn--bcher-kva.de/x"),
            not_a_site("bücher.de")
        );
        assert_eq!(refusal("https://[::1]/x"), not_a_site("[::1]"));
        let anonymous = Some(LinkRefusal::NotASite { host: None });
        assert_eq!(refusal("https://@claude.ai/"), anonymous);
        assert_eq!(refusal("https://:@claude.ai/"), anonymous);
        for nothing in [
            "https:claude.ai/x",
            "https:/claude.ai/x",
            "https://claude.ai\\magic-link",
            "https://cla ude.ai/x",
            "https://claude.ai:abc/x",
            "https://claude.ai:443:443/x",
            "https://clau\u{200d}de.ai/x",
            "https://xn--claude-.ai/x",
            "claude.ai:443/x",
            "https//claude.ai",
            "ftp://claude.ai/x",
            "https://claude.ai@/",
        ] {
            assert_eq!(refusal(nothing), Some(LinkRefusal::NoLink), "{nothing:?}");
        }
        for sign_in in [
            "https://claude.ai/%6Dagic-link",
            "https://claude.ai/MAGIC%2DLINK",
            "https://claude.ai/magic-lin\u{212a}",
            "https://claude.ai/x/..%2fmagic-link",
        ] {
            assert_eq!(
                refusal(sign_in),
                Some(LinkRefusal::SignInLink(&CLAUDE)),
                "{sign_in:?}"
            );
        }
        for sign_in in [
            "https://chatgpt.com/api%2Fauth",
            "https://chatgpt.com/api/./auth",
        ] {
            assert_eq!(
                refusal(sign_in),
                Some(LinkRefusal::SignInLink(&CHATGPT)),
                "{sign_in:?}"
            );
        }
        for dots in ["https://claude.ai/%2e%2e", "https://claude.ai/.%2e/x"] {
            assert_eq!(refusal(dots), Some(LinkRefusal::NoLink), "{dots:?}");
        }
    }

    /// A link with a port, a user or a path whose bytes are not UTF-8 is read for what it
    /// holds. Foundation reads no port where an Int cannot hold the number, no user where the
    /// name is not UTF-8, and an empty path where the path is not, so the Swift SiteLink
    /// opened each of these: the first two on the site without what was dropped, and the last
    /// two past the sign-in rule.
    #[test]
    fn a_port_a_user_or_a_path_counts_whatever_bytes_it_holds() {
        assert_eq!(
            refusal("https://claude.ai:99999999999999999999/x"),
            not_a_site("claude.ai:99999999999999999999")
        );
        assert_eq!(
            refusal("https://%FF@claude.ai/x"),
            Some(LinkRefusal::NotASite { host: None })
        );
        assert_eq!(
            refusal("https://claude.ai/magic-link/%FF#a:b"),
            Some(LinkRefusal::SignInLink(&CLAUDE))
        );
        assert_eq!(
            refusal("https://chatgpt.com/api/auth/%C3?code=x"),
            Some(LinkRefusal::SignInLink(&CHATGPT))
        );
        for dots in [
            "https://claude.ai/x/../%FF",
            "https://claude.ai/%FF/./x",
            "claude.ai/f%cauth;p/.",
        ] {
            assert_eq!(refusal(dots), Some(LinkRefusal::NoLink), "{dots:?}");
        }
        assert_eq!(
            accepted("https://claude.ai/%FF").as_deref(),
            Some("https://claude.ai/%FF"),
            "a path with such a byte is opened where no rule says otherwise"
        );
    }

    /// Swift compared strings by grapheme cluster, so a combining mark or a joiner right after
    /// a `/`, `?` or `#` made one character with it, and hid it. This compares the text. The
    /// Swift SiteLink of 0.7.0, run on the same text on macOS 27, opened each sign-in link
    /// here as a page of the site, and found no link in each bare host.
    #[test]
    fn a_mark_or_a_joiner_after_a_slash_hides_nothing() {
        for sign_in in [
            "https://claude.ai/magic-link/%CC%81",
            "https://claude.ai/magic-link/\u{301}",
            "claude.ai/magic-link/%CC%81",
            "https://claude.ai/magic-link/%E2%80%8D",
        ] {
            assert_eq!(
                refusal(sign_in),
                Some(LinkRefusal::SignInLink(&CLAUDE)),
                "{sign_in:?}"
            );
        }
        assert_eq!(
            refusal("https://chatgpt.com/api/auth/%CC%81"),
            Some(LinkRefusal::SignInLink(&CHATGPT))
        );
        let read =
            |text: &str, url: &str| assert_eq!(accepted(text).as_deref(), Some(url), "{text:?}");
        read("chat.com/\u{301}", "https://chatgpt.com/%CC%81");
        read("claude.ai?\u{301}", "https://claude.ai/?%CC%81");
        read("claude.ai#\u{301}", "https://claude.ai/#%CC%81");
        read("claude.ai/\u{200d}", "https://claude.ai/%E2%80%8D");
    }

    /// The longest link is counted in Unicode scalars. Swift counted grapheme clusters, and
    /// the Swift SiteLink opened the longer of these, 4106 clusters of 8194 scalars.
    #[test]
    fn a_link_is_measured_in_unicode_scalars() {
        let accents = |count: usize| format!("https://claude.ai/{}", "e\u{301}".repeat(count));
        assert_eq!(accents(4087).chars().count(), SiteLink::LONGEST);
        assert!(accepted(&accents(4087)).is_some());
        assert_eq!(refusal(&accents(4088)), Some(LinkRefusal::TooLong));
    }

    /// A host that is not ASCII is percent-decoded before IDNA maps it, as the WHATWG URL
    /// standard does. ICU's IDNA, under Foundation, mapped the letters and left the escapes,
    /// and the Swift SiteLink said this link was on `cｌaude.ai`.
    #[test]
    fn a_host_is_decoded_before_idna_maps_it() {
        assert_eq!(
            accepted("https://ｃ%EF%BD%8Caude.ai/x").as_deref(),
            Some("https://claude.ai/x")
        );
    }
}
