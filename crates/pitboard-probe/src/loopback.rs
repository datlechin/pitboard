//! Block F3's page: a blank page the probe serves itself on `127.0.0.1`, the only address its
//! browser blocks open besides F2's local file. No site is loaded.

const PAGE: &str = "<!doctype html><title>pitboard-probe</title>";

/// The address of the page `name`, served on `port`.
pub fn page_url(port: u16, name: &str) -> String {
    format!("http://127.0.0.1:{port}/{name}")
}

/// Whether `url` is one [`page_url`] makes: `127.0.0.1`, a port, and one path segment of
/// lower-case letters, digits and dashes.
pub fn is_page_url(url: &str) -> bool {
    let Some((port, name)) = url
        .strip_prefix("http://127.0.0.1:")
        .and_then(|rest| rest.split_once('/'))
    else {
        return false;
    };
    port.parse::<u16>().is_ok_and(|p| p != 0)
        && !port.starts_with('0')
        && !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The path of a request's first line, `GET /<path> HTTP/1.1`, once the head has arrived.
pub fn request_path(head: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(head).ok()?;
    let line = text.split("\r\n").next()?;
    let mut parts = line.split(' ');
    let (_method, path, version) = (parts.next()?, parts.next()?, parts.next()?);
    (path.starts_with('/') && version.starts_with("HTTP/")).then(|| path.to_string())
}

/// The whole answer to a request for `path`: the page for any path but the icon a browser
/// asks for on its own.
pub fn response(path: &str) -> Vec<u8> {
    if path == "/favicon.ico" {
        return b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_vec();
    }
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{PAGE}",
        PAGE.len()
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_probes_own_page_is_one_to_open() {
        let url = page_url(49152, "cold");
        assert_eq!(url, "http://127.0.0.1:49152/cold");
        assert!(is_page_url(&url));
        assert!(is_page_url("http://127.0.0.1:1/f3-warm2"));
        for refused in [
            "https://127.0.0.1:49152/cold",
            "http://localhost:49152/cold",
            "http://127.0.0.1:49152/",
            "http://127.0.0.1:49152/Cold",
            "http://127.0.0.1:49152/a/b",
            "http://127.0.0.1:49152/cold?x=1",
            "http://127.0.0.1:0/cold",
            "http://127.0.0.1:080/cold",
            "http://127.0.0.1:65536/cold",
            "http://127.0.0.1:49152@claude.ai/cold",
            "http://127.0.0.1/cold",
            "https://claude.ai/",
            "file:///C:/s/pitboard-probe-standin.html",
        ] {
            assert!(!is_page_url(refused), "{refused}");
        }
    }

    #[test]
    fn a_requests_path_is_read_from_its_first_line() {
        assert_eq!(
            request_path(b"GET /cold HTTP/1.1\r\nHost: 127.0.0.1:1\r\n\r\n").as_deref(),
            Some("/cold")
        );
        assert_eq!(
            request_path(b"GET /cold HTTP/1.1").as_deref(),
            Some("/cold")
        );
        assert_eq!(request_path(b"GET cold HTTP/1.1\r\n"), None);
        assert_eq!(request_path(b"GET /cold\r\n"), None);
        assert_eq!(request_path(b""), None);
        assert_eq!(request_path(&[0xff, 0xfe]), None);
    }

    #[test]
    fn the_page_is_served_and_the_icon_is_not() {
        let page = String::from_utf8(response("/cold")).unwrap();
        assert!(page.starts_with("HTTP/1.1 200 OK\r\n"));
        let (head, body) = page.split_once("\r\n\r\n").unwrap();
        assert!(head.contains(&format!("Content-Length: {}", body.len())));
        assert!(
            String::from_utf8(response("/favicon.ico"))
                .unwrap()
                .starts_with("HTTP/1.1 404")
        );
    }
}
