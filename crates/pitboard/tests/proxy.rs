//! Pitboard's requests go through the HTTP proxy its environment names, a SOCKS one is
//! refused, and `doctor` says which proxy that is and which variable named it, without the
//! user name and password in it.
//!
//! A stand-in HTTP proxy here tunnels each request to the stand-in for Anthropic and writes
//! down what it was asked, so a test sees where a request went rather than trusting a
//! setting. A SOCKS proxy's port, and Anthropic's where a request is refused, are listened on
//! and never answered, and a test asks whether anything reached them.
//! Nothing here leaves the machine: every stand-in listens on loopback, and `doctor` sends no
//! request, so the proxy it is told about is never reached.

mod common;

use common::Env;
use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

/// A proxy that answers `CONNECT` as an HTTP proxy does, tunnels to whatever it was asked
/// for, and keeps the head of every request it was sent.
struct Proxy {
    port: u16,
    asked: Arc<Mutex<Vec<String>>>,
}

impl Proxy {
    fn start() -> Proxy {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port on loopback");
        let port = listener.local_addr().expect("its address").port();
        let asked = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&asked);
        std::thread::spawn(move || {
            for client in listener.incoming().flatten() {
                let kept = Arc::clone(&kept);
                std::thread::spawn(move || tunnel(client, &kept));
            }
        });
        Proxy { port, asked }
    }

    /// Its address, with `userinfo`, such as `alice:s3cret@`, in front of the host.
    fn url(&self, userinfo: &str) -> String {
        format!("http://{userinfo}127.0.0.1:{}", self.port)
    }

    /// The head of every request it was sent, in the order they came.
    fn asked(&self) -> Vec<String> {
        self.asked.lock().expect("not poisoned").clone()
    }
}

/// Reads one request's head, keeps it, and for a `CONNECT` copies bytes both ways between
/// the client and the host it named until either side closes.
fn tunnel(client: TcpStream, kept: &Mutex<Vec<String>>) {
    let Ok(read_side) = client.try_clone() else {
        return;
    };
    let mut from_client = BufReader::new(read_side);
    let mut head = String::new();
    loop {
        let mut line = String::new();
        if from_client.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        if line == "\r\n" {
            break;
        }
        head.push_str(&line);
    }
    kept.lock().expect("not poisoned").push(head.clone());
    let Some(target) = head
        .strip_prefix("CONNECT ")
        .and_then(|rest| rest.split_whitespace().next())
    else {
        return;
    };
    let Ok(upstream) = TcpStream::connect(target) else {
        return;
    };
    let mut to_client = client;
    if to_client
        .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
        .is_err()
    {
        return;
    }
    let (Ok(mut to_upstream), Ok(mut from_upstream)) = (upstream.try_clone(), upstream.try_clone())
    else {
        return;
    };
    // What the client sent after its head, if anything, is in the reader's buffer, so the
    // copy reads through it rather than from the socket.
    let up = std::thread::spawn(move || {
        let _ = std::io::copy(&mut from_client, &mut to_upstream);
        let _ = to_upstream.shutdown(Shutdown::Write);
    });
    let _ = std::io::copy(&mut from_upstream, &mut to_client);
    let _ = to_client.shutdown(Shutdown::Write);
    let _ = up.join();
}

/// An account signed in, whose enrolment asks the stand-in for Anthropic who it is.
fn signed_in(name: &str) -> Env {
    let mut env = Env::new(name);
    let (a, o) = (env.uuid('a'), env.uuid('o'));
    env.sign_in(&a, "a@example.com", &o, "refresh-a");
    env
}

/// A request goes to the proxy `HTTPS_PROXY` names, as a `CONNECT` to the host it is for,
/// and the user name and password in the proxy's address go to the proxy, as ureq sends
/// them. Pitboard reads the variable itself and hands ureq the proxy: ureq read it before,
/// and a test could neither withhold it nor say which variable applied.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn requests_go_through_the_proxy_the_environment_names() {
    let proxy = Proxy::start();
    let env = signed_in("proxy-through");
    let (_, err, code) = env.run_with(
        &["enroll", "work"],
        &[("HTTPS_PROXY", &proxy.url("alice:s3cret@"))],
    );
    assert_eq!(code, 0, "enroll work: {err}");

    let asked = proxy.asked();
    assert!(!asked.is_empty(), "nothing went through the proxy");
    let connect = format!("CONNECT {} HTTP/1.1\r\n", env.api_address());
    for head in &asked {
        assert!(head.starts_with(&connect), "{head}");
        assert!(
            head.contains("Proxy-Authorization: Basic YWxpY2U6czNjcmV0\r\n"),
            "the proxy is given alice:s3cret: {head}"
        );
    }
}

/// `ALL_PROXY` is read before `HTTPS_PROXY`, as ureq reads them, whatever the request's
/// scheme: the proxy it names is the one asked, and the other is never reached.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn all_proxy_comes_before_https_proxy() {
    let (all, https) = (Proxy::start(), Proxy::start());
    let env = signed_in("proxy-all-first");
    let (_, err, code) = env.run_with(
        &["enroll", "work"],
        &[("ALL_PROXY", &all.url("")), ("HTTPS_PROXY", &https.url(""))],
    );
    assert_eq!(code, 0, "enroll work: {err}");
    assert!(!all.asked().is_empty(), "ALL_PROXY's proxy was not asked");
    assert_eq!(https.asked(), Vec::<String>::new(), "HTTPS_PROXY's was");
}

/// A host `NO_PROXY` names is reached directly: the proxy is asked nothing, and the request
/// still arrives.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn a_host_no_proxy_names_is_reached_directly() {
    let proxy = Proxy::start();
    let env = signed_in("proxy-exempt");
    let (_, err, code) = env.run_with(
        &["enroll", "work"],
        &[
            ("HTTPS_PROXY", &proxy.url("")),
            ("NO_PROXY", "localhost,127.0.0.1"),
        ],
    );
    assert_eq!(code, 0, "enroll work: {err}");
    assert_eq!(proxy.asked(), Vec::<String>::new());
}

/// The `network` check in what `doctor --json` printed, `out`, and to its standard error.
fn network_in(out: &str, err: &str) -> serde_json::Value {
    let value: serde_json::Value =
        serde_json::from_str(out).unwrap_or_else(|e| panic!("{e}: {out}{err}"));
    value["data"]["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .find(|check| check["code"] == "network")
        .cloned()
        .unwrap_or_else(|| panic!("no network check: {value}"))
}

/// A loopback port that takes no part: nothing accepts what connects to it, and a test asks
/// afterwards whether anything did. A connection made to it waits in the system's queue,
/// where [`reached`] finds it, after the Pitboard that made it has exited.
fn untouched() -> (TcpListener, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port on loopback");
    listener.set_nonblocking(true).expect("never waits");
    let port = listener.local_addr().expect("its address").port();
    (listener, port)
}

/// Whether anything connected to `listener` from [`untouched`].
fn reached(listener: &TcpListener) -> bool {
    listener.accept().is_ok()
}

/// Every scheme naming a SOCKS proxy, and how `doctor` writes it: `socks://` is SOCKS5, as
/// ureq reads it.
const SOCKS: [(&str, &str); 5] = [
    ("socks5", "socks5"),
    ("socks5h", "socks5h"),
    ("socks", "socks5"),
    ("socks4", "socks4"),
    ("socks4a", "socks4a"),
];

/// A SOCKS proxy, whichever scheme names it, is refused: `pitboard enroll` fails before
/// anything is sent, its cause `unreachable`, saying that Pitboard does not use SOCKS proxies
/// yet and which variable named one, and neither the proxy nor the stand-in for Anthropic is
/// reached. `doctor` warns in the same words, without the user name or password in the
/// proxy's address, which the error does not carry either.
///
/// Pitboard went around a `socks4://` or `socks5://` proxy, straight to the host, and failed
/// each request with a `socks4a://` or `socks5h://` one with "Connection refused", as ureq
/// did without its SOCKS feature. While it spoke SOCKS itself, each went through the proxy.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn every_socks_proxy_is_refused_and_nothing_is_sent() {
    for (scheme, shown) in SOCKS {
        let (proxy, proxy_port) = untouched();
        let (anthropic, anthropic_port) = untouched();
        let env = signed_in(&format!("proxy-{scheme}-refused"));
        let userinfo = if scheme.starts_with("socks5") {
            "alice:s3cret@"
        } else {
            ""
        };
        let address = format!("{scheme}://{userinfo}127.0.0.1:{proxy_port}");
        let api_base = format!("http://127.0.0.1:{anthropic_port}");
        let vars = [
            ("ALL_PROXY", address.as_str()),
            ("PITBOARD_API_BASE", api_base.as_str()),
        ];
        let (out, err, code) = env.run_with(&["enroll", "work", "--json"], &vars);
        assert_ne!(code, 0, "{scheme}: enroll work went out:\n{out}{err}");
        let value: serde_json::Value =
            serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}{err}"));
        assert_eq!(
            value["error"]["cause"]["code"], "unreachable",
            "{scheme}: {value}"
        );
        assert!(
            value["error"]["message"]
                .as_str()
                .is_some_and(|m| m
                    .contains("Pitboard does not use SOCKS proxies yet, and ALL_PROXY names one")),
            "{scheme}: {value}"
        );
        assert!(!reached(&proxy), "{scheme}: the proxy was reached");
        assert!(!reached(&anthropic), "{scheme}: Anthropic was reached");

        let (doctor, doctor_err, _) = env.run_with(&["doctor", "--json"], &vars);
        let network = network_in(&doctor, &doctor_err);
        assert_eq!(network["level"], "warn", "{scheme}: {network}");
        let hidden = if userinfo.is_empty() { "" } else { "***:***@" };
        assert_eq!(
            network["detail"],
            format!(
                "requests fail: Pitboard does not use SOCKS proxies yet, and ALL_PROXY names \
                 one, {shown}://{hidden}127.0.0.1:{proxy_port}"
            ),
            "{scheme}"
        );
        for said in [&out, &err, &doctor, &doctor_err] {
            for secret in ["alice", "s3cret"] {
                assert!(
                    !said.contains(secret),
                    "{scheme}: {secret} is shown:\n{said}"
                );
            }
        }
    }
}

/// A host `NO_PROXY` names goes out directly with a SOCKS proxy named, as around any other
/// proxy: the request arrives, and the proxy is not reached.
#[test]
#[cfg_attr(windows, ignore = "W22: switching Claude Code on Windows")]
fn a_host_no_proxy_names_goes_around_a_socks_proxy() {
    for (scheme, _) in SOCKS {
        let (proxy, port) = untouched();
        let env = signed_in(&format!("proxy-{scheme}-exempt"));
        let (_, err, code) = env.run_with(
            &["enroll", "work"],
            &[
                ("ALL_PROXY", &format!("{scheme}://127.0.0.1:{port}")),
                ("NO_PROXY", "127.0.0.1"),
            ],
        );
        assert_eq!(code, 0, "{scheme}: enroll work: {err}");
        assert!(!reached(&proxy), "{scheme}: the proxy was reached");
    }
}

/// `doctor` names the proxy and the variable it came from, in the report a person reads and
/// in the one they paste, and neither carries the user name or the password in its address.
/// The one they paste shows a digest in place of the proxy's host and port, which can name
/// the company whose network it is.
#[test]
fn doctor_names_the_proxy_and_its_variable_and_hides_its_password() {
    let env = Env::new("proxy-doctor");
    let vars = [
        ("HTTPS_PROXY", "http://alice:s3cret@proxy.example.com:3128"),
        ("NO_PROXY", ".anthropic.com"),
    ];

    let (text, err, _) = env.run_with(&["doctor"], &vars);
    let line = text
        .lines()
        .find(|line| line.contains("network"))
        .unwrap_or_else(|| panic!("no network line:\n{text}{err}"));
    assert!(
        line.contains("through http://***:***@proxy.example.com:3128, from HTTPS_PROXY"),
        "{line}"
    );
    assert!(
        line.contains("NO_PROXY exempts api.anthropic.com"),
        "{line}"
    );

    let (out, err, _) = env.run_with(&["doctor", "--json"], &vars);
    let network = network_in(&out, &err);
    assert_eq!(network["level"], "ok", "{network}");
    let detail = network["detail"].as_str().unwrap_or_default();
    assert!(
        detail.starts_with("through http://***:***@<proxy ")
            && detail.contains(">, from HTTPS_PROXY; NO_PROXY exempts api.anthropic.com"),
        "{network}"
    );
    assert!(
        !out.contains("proxy.example.com"),
        "the host is shown:\n{out}{err}"
    );

    for said in [&text, &out] {
        for secret in ["alice", "s3cret"] {
            assert!(!said.contains(secret), "{secret} is shown:\n{said}");
        }
    }
}
