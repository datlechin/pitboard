//! A web address as an account window's rules read one: the page a navigation asks for, a
//! response's address, a download's. WebKit hands the macOS app each as a `URL`, whose
//! scheme, host, port and user the rules compared before they were Rust, so they are read
//! here as Foundation's `URL` reads them, as docs/facts/foundation-urls.md has it.

use crate::address;

/// The scheme of a web address, and the host, port and user it names: what decides whether a
/// page is one of a site's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebAddress {
    scheme: String,
    host: Option<String>,
    port: Option<String>,
    user: bool,
    path: String,
}

impl WebAddress {
    /// `text` read as Foundation's `URL` reads it. The scheme is what comes before the first
    /// `:`, where RFC 3986 allows it as one; the host, port and user follow `//`. Text with
    /// no scheme reads as an address on none, and one whose host cannot be read names none.
    ///
    /// Two readings differ from `URL`'s. Where `URL` drops what it cannot hold, a port no
    /// `Int` holds, this keeps it: a page that names any port, or any user, is never a site's
    /// own. And text with no scheme names no host here, where `URL` reads `claude.ai` in
    /// `//claude.ai/x`: an address on no scheme is no site's either way.
    pub fn parse(text: &str) -> WebAddress {
        let scheme = address::scheme(text)
            .unwrap_or_default()
            .to_ascii_lowercase();
        let parts = address::parse(text);
        let path = parts
            .as_ref()
            .map(address::Address::url_path)
            .unwrap_or_default();
        let authority = parts.and_then(|parts| parts.authority);
        WebAddress {
            scheme,
            host: authority
                .as_ref()
                .map(|authority| authority.url_host.clone())
                .filter(|host| !host.is_empty()),
            port: authority.as_ref().and_then(|a| a.port.clone()),
            user: authority.is_some_and(|authority| authority.user_info),
            path,
        }
    }

    /// The scheme in lower case, or empty where there is none.
    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    /// The host as `URL.host` gives it: percent-decoded and in the case it was written in, a
    /// host that is not ASCII in IDNA's ASCII form and one in ASCII as written, even where
    /// IDNA refuses an `xn--` label in it, so it names the host a request goes to, and an IP
    /// literal without its brackets. `None` where the address names no host, or one that
    /// cannot be read.
    pub fn host(&self) -> Option<&str> {
        self.host.as_deref()
    }

    /// The port it names, as digits without leading zeros, even `0`.
    pub fn port(&self) -> Option<&str> {
        self.port.as_deref()
    }

    /// Whether it names a user, even an empty one, as `https://@claude.ai/` does.
    pub fn names_user(&self) -> bool {
        self.user
    }

    /// The path as `URL.path` gives it: percent-decoded, with neither query nor fragment,
    /// empty where it does not decode as UTF-8, and without the slashes it ends in, but for
    /// the one a path of nothing else keeps: `/c/shared/` is `/c/shared`, `//` is `/`, and
    /// no path at all is empty.
    pub fn path(&self) -> &str {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(text: &str) -> Option<String> {
        WebAddress::parse(text).host().map(str::to_owned)
    }

    /// Each as `URL(string:)` read it, measured on macOS 27.0 on 5 October 2026: its
    /// `scheme`, `host`, `port`, `user` and `password`.
    #[test]
    fn an_address_is_read_as_foundations_url_reads_it() {
        let page = WebAddress::parse("HTTPS://CHATGPT.com/c/abc?x#y");
        assert_eq!(page.scheme(), "https");
        assert_eq!(page.host(), Some("CHATGPT.com"), "in the case written");
        assert_eq!(page.port(), None);
        assert!(!page.names_user());

        assert_eq!(WebAddress::parse("Mailto:A@b.c").scheme(), "mailto");
        assert_eq!(host("Mailto:A@b.c"), None);
        assert_eq!(
            WebAddress::parse("x-apple.systempreferences:com.apple").scheme(),
            "x-apple.systempreferences"
        );
        assert_eq!(host("vscode://file/x").as_deref(), Some("file"));
        for none in [
            "about:blank",
            "data:text/plain,x",
            "blob:https://claude.ai/1-2",
            "file:///etc/hosts",
            "https:claude.ai/x",
            "https:///x",
            "javascript:alert(1)",
        ] {
            assert_eq!(host(none), None, "{none}");
        }
        assert_eq!(
            host("pitboard-fixture://claude.ai/").as_deref(),
            Some("claude.ai")
        );
    }

    /// Each path as `URL(string:)!.path` read it, measured with Swift 6.4 on macOS 27.0 on
    /// 6 October 2026: decoded before the slashes it ends in are taken off, so an encoded
    /// slash at the end goes too, and empty for one that does not decode.
    #[test]
    fn a_path_is_read_as_foundations_url_reads_it() {
        for (text, path) in [
            ("pitboard-fixture://chatgpt.com/", "/"),
            ("pitboard-fixture://chatgpt.com", ""),
            ("pitboard-fixture://chatgpt.com/c/shared", "/c/shared"),
            ("pitboard-fixture://chatgpt.com/c/shared/", "/c/shared"),
            (
                "pitboard-fixture://claude.ai/chat/fixture?x=1#y",
                "/chat/fixture",
            ),
            ("pitboard-fixture://claude.ai/a%20b/%2F", "/a b"),
            ("pitboard-fixture://claude.ai/%FF", ""),
            ("pitboard-fixture://CLAUDE.AI/x", "/x"),
            ("pitboard-fixture:", ""),
            ("pitboard-fixture://claude.ai//", "/"),
            ("pitboard-fixture://claude.ai/a//", "/a"),
        ] {
            assert_eq!(WebAddress::parse(text).path(), path, "{text}");
        }
    }

    /// `URL(string:)` found no scheme and no host in any of these, measured the same day; the
    /// empty address is how WebKit hands over a window `window.open('')` asks for.
    #[test]
    fn text_with_no_scheme_is_on_none() {
        let texts = [
            "",
            "claude.ai/x",
            "a?b:c",
            "1https://claude.ai/",
            "h_ttps://claude.ai/",
            "/x",
        ];
        for text in texts {
            let address = WebAddress::parse(text);
            assert_eq!(address.scheme(), "", "{text:?}");
            assert_eq!(address.host(), None, "{text:?}");
        }
        let relative = WebAddress::parse("//claude.ai/x");
        assert_eq!(relative.scheme(), "");
        assert_eq!(
            relative.host(),
            None,
            "where URL reads claude.ai: on no scheme, no site's either way"
        );
    }

    /// A host is percent-decoded and kept in its case. A punycode label stays as written and
    /// a host that is not ASCII is in IDNA's ASCII form, so `аpple.com`, with a Cyrillic а,
    /// is `xn--pple-43d.com`: the host a request goes to, never a look-alike of another.
    #[test]
    fn a_host_is_the_one_a_request_goes_to() {
        assert_eq!(host("https://cl%61ude.ai/").as_deref(), Some("claude.ai"));
        assert_eq!(
            host("https://claude.ai%2Eexample.com/").as_deref(),
            Some("claude.ai.example.com")
        );
        assert_eq!(host("https://%C3%BCber.de/").as_deref(), Some("über.de"));
        assert_eq!(
            host("https://xn--bcher-kva.de/").as_deref(),
            Some("xn--bcher-kva.de")
        );
        assert_eq!(
            host("https://XN--BCHER-KVA.de/").as_deref(),
            Some("XN--BCHER-KVA.de")
        );
        assert_eq!(
            host("https://Bücher.de/").as_deref(),
            Some("xn--bcher-kva.de")
        );
        assert_eq!(
            host("https://аpple.com/").as_deref(),
            Some("xn--pple-43d.com")
        );
        assert_eq!(
            host("https://ｃｌａｕｄｅ.ai/").as_deref(),
            Some("claude.ai")
        );
        assert_eq!(host("https://claude.ai./").as_deref(), Some("claude.ai."));
        assert_eq!(host("https://[::1]:8080/").as_deref(), Some("::1"));
        assert_eq!(host("https://[::1%25en0]/").as_deref(), Some("::1%en0"));
        assert_eq!(
            host("https://claude.ai%2F@evil.com/").as_deref(),
            Some("evil.com")
        );
        assert_eq!(host("https://%FF.ai/"), None, "no host URL can read");
    }

    /// `URL.host` keeps an ASCII host as written even where IDNA refuses an `xn--` label in
    /// it, as `URLComponents` does not, measured the same day: `xn--claude-.ai` is no
    /// punycode, and `URLComponents.host` is nil for it. A host that is not ASCII and that
    /// IDNA refuses makes no `URL` at all.
    #[test]
    fn a_punycode_label_idna_refuses_is_kept_as_written() {
        for (text, written) in [
            ("https://xn--claude-.ai/", "xn--claude-.ai"),
            ("https://XN--CLAUDE-.ai/", "XN--CLAUDE-.ai"),
            ("https://xn--.ai/", "xn--.ai"),
            ("https://claude.xn--/", "claude.xn--"),
            (
                "https://xn--abc-.xn--bcher-kva.de/",
                "xn--abc-.xn--bcher-kva.de",
            ),
            ("https://xn--claude-.ai./", "xn--claude-.ai."),
            ("https://xn--claude-.ai%2Ex/", "xn--claude-.ai.x"),
        ] {
            assert_eq!(host(text).as_deref(), Some(written), "{text}");
        }
        let page = WebAddress::parse("https://u@xn--claude-.ai:443/");
        assert_eq!(page.host(), Some("xn--claude-.ai"));
        assert_eq!(page.port(), Some("443"));
        assert!(page.names_user());
        assert_eq!(host("https://bücher.xn--claude-.de/"), None);
    }

    /// `URL.port` is nil only where none is written, `claude.ai:`, and `URL.user` only
    /// where no `@` is. A port no `Int` holds, which `URL` drops, is kept.
    #[test]
    fn a_port_and_a_user_are_noticed_whatever_they_hold() {
        let port = |text: &str| WebAddress::parse(text).port().map(str::to_owned);
        assert_eq!(port("https://claude.ai:/x"), None);
        assert_eq!(port("https://claude.ai:0/").as_deref(), Some("0"));
        assert_eq!(port("https://CLAUDE.AI:443/x").as_deref(), Some("443"));
        assert_eq!(port("https://claude.ai:0443/").as_deref(), Some("443"));
        assert_eq!(
            port("https://claude.ai:99999999999999999999/").as_deref(),
            Some("99999999999999999999")
        );
        for user in [
            "https://x@claude.ai/",
            "https://@claude.ai/",
            "https://:@claude.ai/",
            "https://x:y@claude.ai/",
            "https://a@b@claude.ai/",
        ] {
            assert!(WebAddress::parse(user).names_user(), "{user}");
        }
        assert!(!WebAddress::parse("https://claude.ai/").names_user());
    }
}
