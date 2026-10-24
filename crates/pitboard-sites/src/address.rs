//! A link split into its parts the way Foundation's `URLComponents` splits one, which is how
//! the macOS app read every link before these rules moved here, as
//! docs/facts/foundation-urls.md has it.
//!
//! RFC 3986's grammar, with what it does not allow percent-encoded rather than refused, as
//! Foundation encodes it: a space in a path is `%20`, a second `#` is `%23`, a `%` that starts
//! no escape is `%25`. A host is not encoded: one that RFC 3986 does not allow makes no
//! address at all. A host that is not plain ASCII, or holds a punycode label, goes through
//! IDNA's mapping, so `ｃｌａｕｄｅ.ai` is `claude.ai` as it is to Foundation.
//!
//! The WHATWG URL standard, which the `url` crate follows, reads the same text otherwise: it
//! resolves `/x/../y` to `/y`, takes `\` for `/`, drops a default port such as `:443`, finds a
//! host in `https:claude.ai`, strips a tab or a newline, and leaves a second `#` as it is. A
//! link from outside means what the macOS app took it to mean, so `url` is used for IDNA alone.

/// A link's parts, each percent-encoded as Foundation keeps it.
#[derive(Debug)]
pub(crate) struct Address<'a> {
    /// As written, in whatever case.
    pub(crate) scheme: &'a str,
    /// What follows `//`, when the link has one.
    pub(crate) authority: Option<Authority>,
    /// Empty, or starting with `/` when there is an authority.
    pub(crate) path: String,
    pub(crate) query: Option<String>,
    pub(crate) fragment: Option<String>,
}

/// Who and where a link names.
#[derive(Debug)]
pub(crate) struct Authority {
    /// Whether it names a user, even an empty one, as `https://@claude.ai/` does.
    pub(crate) user_info: bool,
    /// Percent-decoded, and through IDNA where that applies, in the case it was written in.
    /// An IP literal keeps its brackets. `None` where IDNA refuses an `xn--` label of a host
    /// in ASCII, for which `URLComponents` names no host.
    pub(crate) host: Option<String>,
    /// The host as Foundation's `URL.host` gives it, which a page's address is compared by:
    /// percent-decoded, in the case it was written in, a host that is not ASCII as IDNA's
    /// ASCII form and a host in ASCII as written, even where IDNA refuses one of its `xn--`
    /// labels, and an IP literal without its brackets.
    pub(crate) url_host: String,
    /// The port, as digits without leading zeros, when one is written: `claude.ai:` names
    /// none, and `claude.ai:0443` names 443.
    pub(crate) port: Option<String>,
}

impl Address<'_> {
    /// The path percent-decoded, with any byte that is not UTF-8 in it as U+FFFD.
    ///
    /// Foundation gives an empty path for one that does not decode, and the rules once read
    /// `/magic-link/%FF` as no path at all.
    pub(crate) fn decoded_path(&self) -> String {
        String::from_utf8_lossy(&percent_decoded(&self.path)).into_owned()
    }

    /// The path as Foundation's `URL.path` gives it: percent-decoded, empty where that is not
    /// UTF-8, and without the slashes it ends in, but for the one a path of nothing else
    /// keeps.
    pub(crate) fn url_path(&self) -> String {
        let Ok(decoded) = String::from_utf8(percent_decoded(&self.path)) else {
            return String::new();
        };
        match decoded.trim_end_matches('/') {
            "" if decoded.starts_with('/') => "/".into(),
            kept => kept.into(),
        }
    }
}

/// `text` split into its parts, or `None` where Foundation finds no link with a scheme: no
/// scheme, a scheme RFC 3986 does not allow, or an authority it cannot read.
pub(crate) fn parse(text: &str) -> Option<Address<'_>> {
    let scheme = scheme(text)?;
    let rest = &text[scheme.len() + 1..];
    let (rest, fragment) = match rest.split_once('#') {
        Some((rest, fragment)) => (rest, Some(encoded(fragment, in_query))),
        None => (rest, None),
    };
    let (rest, query) = match rest.split_once('?') {
        Some((rest, query)) => (rest, Some(encoded(query, in_query))),
        None => (rest, None),
    };
    let (authority, path) = match rest.strip_prefix("//") {
        Some(after) => {
            let end = after.find('/').unwrap_or(after.len());
            (Some(authority(&after[..end])?), &after[end..])
        }
        None => (None, rest),
    };
    Some(Address {
        scheme,
        authority,
        path: encoded(path, in_path),
        query,
        fragment,
    })
}

/// `name=value` pairs of `query`, separated by `&`, each half percent-decoded, as
/// `URLComponents.queryItems` reads them. A `+` stays a `+`: it is a space only in a form.
/// A name that does not decode is empty, and a value that does not decode is none.
pub(crate) fn query_items(query: &str) -> Vec<(String, Option<String>)> {
    if query.is_empty() {
        return Vec::new();
    }
    query
        .split('&')
        .map(|item| match item.split_once('=') {
            Some((name, value)) => (decoded(name).unwrap_or_default(), decoded(value)),
            None => (decoded(item).unwrap_or_default(), None),
        })
        .collect()
}

/// The scheme of `text`, as written, or `None` where it has none RFC 3986 allows. A scheme
/// ends at the first `:`, where that comes before any `/`, `?` or `#`.
pub(crate) fn scheme(text: &str) -> Option<&str> {
    let end = text.find([':', '/', '?', '#'])?;
    let scheme = &text[..end];
    (text[end..].starts_with(':') && is_scheme(scheme)).then_some(scheme)
}

/// RFC 3986: a letter, then letters, digits, `+`, `-` and `.`.
fn is_scheme(scheme: &str) -> bool {
    let mut bytes = scheme.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
}

/// `[userinfo@]host[:port]`. The user information ends at the last `@`, and whatever it holds
/// is encoded rather than refused.
fn authority(text: &str) -> Option<Authority> {
    let (user_info, host_port) = match text.rfind('@') {
        Some(at) => (true, &text[at + 1..]),
        None => (false, text),
    };
    let ((host, url_host), port) = if host_port.starts_with('[') {
        let close = host_port.find(']')?;
        let after = &host_port[close + 1..];
        let port = if after.is_empty() {
            None
        } else {
            Some(after.strip_prefix(':')?)
        };
        (ip_literal(&host_port[..=close])?, port)
    } else {
        match host_port.split_once(':') {
            Some((host, port)) => (reg_name(host)?, Some(port)),
            None => (reg_name(host_port)?, None),
        }
    };
    let port = match port {
        None | Some("") => None,
        Some(digits) if digits.bytes().all(|b| b.is_ascii_digit()) => {
            let trimmed = digits.trim_start_matches('0');
            Some(if trimmed.is_empty() { "0" } else { trimmed }.to_owned())
        }
        Some(_) => return None,
    };
    Some(Authority {
        user_info,
        host,
        url_host,
        port,
    })
}

/// A host by name, as `URLComponents` reads it and then as `URL.host` does. Each ASCII
/// character must be one RFC 3986 allows in one. One that is not plain ASCII, or that has a
/// label starting `xn--`, is mapped through IDNA, as Foundation maps it, and read back in
/// Unicode: `bücher.de` stays `bücher.de`, and `ｃｌａｕｄｅ.ai` is `claude.ai`. Otherwise
/// it is percent-decoded, and must decode to UTF-8.
///
/// `URL.host` gives a host that is not ASCII in IDNA's ASCII form, `xn--bcher-kva.de`, which
/// is the name a request goes to, and keeps a host in ASCII as written. Where IDNA refuses an
/// `xn--` label of a host in ASCII, as it refuses `xn--claude-.ai`, `URLComponents` names no
/// host and `URL.host` that host as written. A host that is not ASCII and that IDNA refuses
/// makes no link to either.
fn reg_name(raw: &str) -> Option<(Option<String>, String)> {
    let allowed = raw
        .bytes()
        .all(|b| !b.is_ascii() || is_unreserved(b) || is_sub_delim(b) || b == b'%');
    if !allowed || !escapes_are_whole(raw) {
        return None;
    }
    let punycode = raw.split('.').any(|label| {
        label
            .get(..4)
            .is_some_and(|start| start.eq_ignore_ascii_case("xn--"))
    });
    if raw.is_ascii() && !punycode {
        let host = decoded(raw)?;
        return Some((Some(host.clone()), host));
    }
    // The WHATWG standard reads a host whose last label is a number as an IPv4 address, and
    // refuses `ü.1` as one; Foundation reads no host so. Each mapping is asked of the host
    // with a label after it, which leaves IDNA's answer for the host itself as it was.
    let mapped = |host: &str, map: fn(&str) -> String| {
        map(&format!("{host}.x"))
            .strip_suffix(".x")
            .map(str::to_owned)
    };
    let idna = mapped(raw, url::quirks::domain_to_ascii).and_then(|ascii| {
        let unicode = mapped(&ascii, url::quirks::domain_to_unicode)?;
        Some((unicode, ascii))
    });
    match idna {
        Some((unicode, _)) if raw.is_ascii() => Some((Some(unicode), decoded(raw)?)),
        Some((unicode, ascii)) => Some((Some(unicode), ascii)),
        None if raw.is_ascii() => Some((None, decoded(raw)?)),
        None => None,
    }
}

/// `[...]`, brackets kept and percent-decoded, which Foundation takes as a host however little
/// it reads as an address, and then the same without its brackets, as `URL.host` gives it.
/// Never a site's.
fn ip_literal(raw: &str) -> Option<(Option<String>, String)> {
    let inside = &raw[1..raw.len() - 1];
    let allowed = inside
        .bytes()
        .all(|b| is_unreserved(b) || is_sub_delim(b) || matches!(b, b':' | b'%'));
    if !allowed || !escapes_are_whole(inside) {
        return None;
    }
    Some((Some(decoded(raw)?), decoded(inside)?))
}

/// Whether every `%` in `text` starts an escape of two hexadecimal digits.
fn escapes_are_whole(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.iter().enumerate().all(|(at, &b)| {
        b != b'%'
            || bytes
                .get(at + 1..at + 3)
                .is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit))
    })
}

/// `text` as Foundation keeps a path, a query or a fragment: as written where RFC 3986 allows
/// it, and otherwise with every character `allowed` refuses percent-encoded as UTF-8, a `%`
/// among them. So once anything in it is encoded, an escape already there is encoded too:
/// `/%41 x` is kept as `/%2541%20x`, which decodes to what was written.
fn encoded(text: &str, allowed: fn(u8) -> bool) -> String {
    let valid = text
        .bytes()
        .all(|b| b == b'%' || (b.is_ascii() && allowed(b)));
    if valid && escapes_are_whole(text) {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() * 3);
    for b in text.bytes() {
        if b.is_ascii() && b != b'%' && allowed(b) {
            out.push(char::from(b));
        } else {
            push_escape(&mut out, b);
        }
    }
    out
}

/// `%` and `b` in two upper-case hexadecimal digits.
pub(crate) fn push_escape(out: &mut String, b: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push('%');
    out.push(char::from(HEX[usize::from(b >> 4)]));
    out.push(char::from(HEX[usize::from(b & 0x0F)]));
}

/// `text` percent-decoded as UTF-8, or `None` when its bytes are not UTF-8.
fn decoded(text: &str) -> Option<String> {
    String::from_utf8(percent_decoded(text)).ok()
}

/// The bytes `text` stands for. A `%` that starts no escape stands for itself.
fn percent_decoded(text: &str) -> Vec<u8> {
    let digit = |b: u8| {
        char::from(b)
            .to_digit(16)
            .and_then(|d| u8::try_from(d).ok())
    };
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escaped = match bytes.get(at..at + 3) {
            Some(&[b'%', high, low]) => digit(high).zip(digit(low)),
            _ => None,
        };
        match escaped {
            Some((high, low)) => {
                out.push(high << 4 | low);
                at += 3;
            }
            None => {
                out.push(bytes[at]);
                at += 1;
            }
        }
    }
    out
}

/// RFC 3986's unreserved characters: letters, digits, `-`, `.`, `_` and `~`.
pub(crate) fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
}

fn is_sub_delim(b: u8) -> bool {
    matches!(
        b,
        b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

/// What a path keeps as it is: RFC 3986's `pchar` and `/`.
fn in_path(b: u8) -> bool {
    is_unreserved(b) || is_sub_delim(b) || matches!(b, b':' | b'@' | b'/')
}

/// What a query or a fragment keeps as it is: a path's characters and `?`.
fn in_query(b: u8) -> bool {
    in_path(b) || b == b'?'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(text: &str) -> Option<String> {
        parse(text)?.authority?.host
    }

    /// Foundation's reading of each, measured on macOS 27 with `URLComponents(string:)`.
    #[test]
    fn a_link_is_split_as_foundation_splits_it() {
        let x = parse("https://claude.ai/x?a?b#c?d/e#f").expect("a link");
        assert_eq!(x.scheme, "https");
        assert_eq!(x.path, "/x");
        assert_eq!(x.query.as_deref(), Some("a?b"));
        assert_eq!(x.fragment.as_deref(), Some("c?d/e%23f"));

        let bare = parse("https://claude.ai?x").expect("a link");
        assert_eq!(bare.path, "");
        assert_eq!(bare.query.as_deref(), Some("x"));

        let empty = parse("https://claude.ai/x?#").expect("a link");
        assert_eq!(empty.query.as_deref(), Some(""));
        assert_eq!(empty.fragment.as_deref(), Some(""));

        assert!(parse("claude.ai/x").is_none(), "no scheme");
        assert!(parse("a?b:c").is_none(), "a `:` in a query is no scheme's");
        assert!(parse("h_ttps://claude.ai/").is_none());
        assert!(parse("1https://claude.ai/").is_none());
        assert!(
            parse("https:claude.ai/x")
                .expect("a link")
                .authority
                .is_none()
        );
        assert!(
            parse("https:/claude.ai/x")
                .expect("a link")
                .authority
                .is_none()
        );
    }

    #[test]
    fn what_rfc_3986_does_not_allow_is_encoded() {
        let path = |text: &str| parse(text).expect("a link").path;
        assert_eq!(
            path("https://claude.ai/a b\t\n\r\\\"<>^`{|}[]"),
            "/a%20b%09%0A%0D%5C%22%3C%3E%5E%60%7B%7C%7D%5B%5D"
        );
        assert_eq!(path("https://claude.ai/'()!*;:@=&+$,~"), "/'()!*;:@=&+$,~");
        assert_eq!(path("https://claude.ai/ü"), "/%C3%BC");
        assert_eq!(path("https://claude.ai/%"), "/%25");
        assert_eq!(path("https://claude.ai/%2"), "/%252");
        assert_eq!(path("https://claude.ai/x\0y"), "/x%00y");
        let query = parse("https://claude.ai/x?a=\"q\" [1]&b='()!*'").expect("a link");
        assert_eq!(query.query.as_deref(), Some("a=%22q%22%20%5B1%5D&b='()!*'"));
    }

    /// A part RFC 3986 allows is kept as written, escapes and all. One it does not allow has
    /// every `%` encoded with the rest, so it decodes to what was written, and `%2e` in it is
    /// no dot segment.
    #[test]
    fn once_a_part_is_encoded_its_escapes_are_too() {
        let x = parse("https://claude.ai/%2e%41?%20 #%20").expect("a link");
        assert_eq!(x.path, "/%2e%41");
        assert_eq!(x.query.as_deref(), Some("%2520%20"));
        assert_eq!(x.fragment.as_deref(), Some("%20"));
        let path = |text: &str| parse(text).expect("a link").path;
        assert_eq!(path("https://claude.ai/%zz%2e%2E"), "/%25zz%252e%252E");
        assert_eq!(path("https://claude.ai/%FF%"), "/%25FF%25");
        assert_eq!(path("https://claude.ai/é%20"), "/%C3%A9%2520");
        assert_eq!(
            path("https://claude.ai/%2e%2e/magic-link x"),
            "/%252e%252e/magic-link%20x"
        );
    }

    #[test]
    fn a_host_is_read_as_foundation_reads_it() {
        assert_eq!(host("https://CLAUDE.AI/").as_deref(), Some("CLAUDE.AI"));
        assert_eq!(host("https://cl%61ude.ai/").as_deref(), Some("claude.ai"));
        assert_eq!(host("https://claude%2Eai/").as_deref(), Some("claude.ai"));
        assert_eq!(
            host("https://claude.ai%00/").as_deref(),
            Some("claude.ai\0")
        );
        assert_eq!(
            host("https://%EF%BD%83laude.ai/").as_deref(),
            Some("ｃlaude.ai")
        );
        assert_eq!(
            host("https://ｃｌａｕｄｅ.ai/").as_deref(),
            Some("claude.ai")
        );
        assert_eq!(
            host("https://ＣＬＡＵＤＥ.ai/").as_deref(),
            Some("claude.ai")
        );
        assert_eq!(
            host("https://clau\u{ad}de.ai/").as_deref(),
            Some("claude.ai")
        );
        assert_eq!(
            host("https://claude\u{3002}ai/").as_deref(),
            Some("claude.ai")
        );
        assert_eq!(
            host("https://chat\u{ff0e}com/").as_deref(),
            Some("chat.com")
        );
        assert_eq!(host("https://bücher.de/").as_deref(), Some("bücher.de"));
        assert_eq!(
            host("https://XN--BCHER-KVA.de/").as_deref(),
            Some("bücher.de")
        );
        assert_eq!(host("https://claude..ai/").as_deref(), Some("claude..ai"));
        assert_eq!(host("https://1.2.3/").as_deref(), Some("1.2.3"));
        assert_eq!(host("https://0x7f.1/").as_deref(), Some("0x7f.1"));
        assert_eq!(
            host("https://ü.1/").as_deref(),
            Some("ü.1"),
            "no IPv4 address"
        );
        assert_eq!(host("https://１.２.３.４/").as_deref(), Some("1.2.3.4"));
        assert_eq!(host("https:///x").as_deref(), Some(""));
        assert_eq!(host("https://[::1]/").as_deref(), Some("[::1]"));
        assert_eq!(host("https://[::1%25en0]/").as_deref(), Some("[::1%en0]"));
        assert_eq!(host("https://[claude.ai]/").as_deref(), Some("[claude.ai]"));

        for none in [
            "https://cla ude.ai/",
            "https://cl<aude.ai/",
            "https://claude.ai\\magic-link",
            "https://clau\u{200d}de.ai/",
            "https://claude.ai\u{a0}/",
            "https://claude.ai%/",
            "https://claude.ai%zz/",
            "https://%FF.ai/",
            "https://xn--/",
            "https://xn--claude-.ai/",
            "https://[/",
            "https://[::1]x/",
            "https://[ ]/",
        ] {
            assert_eq!(host(none), None, "{none:?}");
        }
    }

    #[test]
    fn a_port_and_a_user_are_noticed_whatever_they_hold() {
        let port = |text: &str| parse(text).expect("a link").authority.expect("a host").port;
        assert_eq!(port("https://claude.ai/").as_deref(), None);
        assert_eq!(port("https://claude.ai:/").as_deref(), None);
        assert_eq!(port("https://claude.ai:443/").as_deref(), Some("443"));
        assert_eq!(port("https://claude.ai:0443/").as_deref(), Some("443"));
        assert_eq!(port("https://claude.ai:00/").as_deref(), Some("0"));
        assert_eq!(
            port("https://claude.ai:99999999999999999999/").as_deref(),
            Some("99999999999999999999"),
            "Foundation reads no port where an Int cannot hold it"
        );
        assert_eq!(port("https://[::1]:443/").as_deref(), Some("443"));
        assert!(parse("https://claude.ai:+443/").is_none());
        assert!(parse("https://claude.ai:443:443/").is_none());

        let user = |text: &str| {
            parse(text)
                .expect("a link")
                .authority
                .expect("a host")
                .user_info
        };
        assert!(!user("https://claude.ai/"));
        for text in [
            "https://@claude.ai/",
            "https://%FF@claude.ai/",
            "https://a@b@claude.ai/",
        ] {
            assert!(user(text), "{text}");
        }
        assert_eq!(
            host("https://claude.ai%2F@evil.com/").as_deref(),
            Some("evil.com")
        );
    }

    #[test]
    fn query_items_are_read_as_foundation_reads_them() {
        let items = |query: &str| query_items(query);
        let item = |name: &str, value: Option<&str>| (name.to_owned(), value.map(str::to_owned));
        assert_eq!(items(""), []);
        assert_eq!(items("url=a+b%2Bc"), [item("url", Some("a+b+c"))]);
        assert_eq!(items("url=%+1%-1%c3%A9"), [item("url", Some("%+1%-1é"))]);
        assert_eq!(items("%75rl=a=b"), [item("url", Some("a=b"))]);
        assert_eq!(items("url"), [item("url", None)]);
        assert_eq!(items("url=%FF"), [item("url", None)]);
        assert_eq!(items("%FF=1"), [item("", Some("1"))]);
        assert_eq!(
            items("url=a&&b"),
            [item("url", Some("a")), item("", None), item("b", None)]
        );
    }
}
