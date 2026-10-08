//! The proxy Pitboard's requests go through: the one the environment it was started with
//! names, read from its [`Context`](crate::context::Context) the way ureq 3.4.2 reads one
//! from a process's environment, and handed to ureq outright.
//!
//! ureq looks for a proxy itself, in this process's environment, whenever an agent's
//! configuration is made. That read was outside the list of what Pitboard reads
//! (`context::READ`), so a test could not withhold a proxy exported where it runs, an app's
//! context could not say which one it was given, and `doctor` could not say which applied.
//! Pitboard reads the same variables by the same rules from the context, and gives ureq what
//! it read in place of what ureq found, before the agent is made, so ureq's own reading never
//! reaches a request.
//!
//! Environment variables only, as the owner chose: a proxy the system is set to use, and no
//! variable names, is not followed.
//!
//! The rules, read in ureq 3.4.2's `src/proxy.rs`, `src/config.rs`, `src/run.rs`,
//! `src/middleware.rs` and `src/unversioned/transport/` on 7 October 2026:
//!
//! - `Proxy::try_from_env` tries [`NAMING`] in that order and takes the first variable set
//!   to an address it reads as a proxy's, whatever the request's scheme. One holding
//!   anything else, or nothing, it passes over.
//! - `NoProxy::try_from_env` takes the first of [`EXEMPTING`] that is set, even empty, and
//!   splits it at commas, keeping any space: `*` exempts every host, `*name` and `.name` a
//!   name ending so, `name*` and `name.` one starting so, and anything else that host alone,
//!   compared without regard to ASCII case.
//! - An address with no scheme is `http://`. `http` and `https` name a proxy ureq asks with
//!   `CONNECT`, which is given each request to a host the variable exempting hosts does not
//!   name; the rest go out directly, before anything is asked of the proxy or of its name.
//!   `socks4`, `socks4a`, `socks5`, `socks5h`, and `socks`, which is `socks5`, name a SOCKS
//!   proxy.
//! - `Config::default()`, which every agent's configuration starts from, calls
//!   `Proxy::try_from_env`. `ConfigBuilder::proxy` replaces what it found.
//!
//! Pitboard does not use a SOCKS proxy yet, as the owner decided on 7 October 2026, until a
//! ureq release fixes the connector its `socks-proxy` feature brings, read in 3.4.2's
//! `src/unversioned/transport/socks.rs`: it waits for the proxy's answer inside
//! `thread::scope`, which joins a thread stuck on a proxy that never answers, so the
//! request's time limit never applies, and it looks the proxy's own name up before it asks
//! whether the variable exempting hosts names the request's host. Built without the feature,
//! ureq panics on a SOCKS proxy it is given, and goes around one it read itself, straight to
//! the host. So ureq is given none, and the agent fails each request such a proxy would have
//! carried, before anything is sent or looked up, saying which variable named it
//! ([`Refusal`]). A request to a host the variable exempting hosts names goes out directly,
//! as it would around any proxy.

use crate::context::Environment;
use ureq::http::{Request, Response, Uri};
use ureq::middleware::{Middleware, MiddlewareNext};
use ureq::{Body, ProxyProtocol, SendBody};

/// The variables a proxy is read from, in the order ureq 3.4.2 tries them: `ALL_PROXY` comes
/// before `HTTPS_PROXY`, and `HTTP_PROXY` is read for Pitboard's https requests too.
pub(crate) const NAMING: [&str; 6] = [
    "ALL_PROXY",
    "all_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "HTTP_PROXY",
    "http_proxy",
];

/// The variables that exempt hosts from the proxy, in the order ureq 3.4.2 tries them.
pub(crate) const EXEMPTING: [&str; 2] = ["NO_PROXY", "no_proxy"];

/// What a context's environment says of proxies, read as ureq reads a process's.
#[derive(Clone, Default)]
pub(crate) struct Proxies {
    /// The variable that names the proxy, and ureq's reading of its address with the
    /// entries of [`Proxies::exempting`] in it, which ureq goes by for each request.
    named: Option<(&'static str, ureq::Proxy)>,
    /// The variables before it that are set to something ureq does not read as a proxy's
    /// address, which it passes over, in its order. An empty one is said to be nothing.
    passed_over: Vec<&'static str>,
    /// `NO_PROXY` or `no_proxy`, whichever ureq reads, where either is set.
    exempting: Option<&'static str>,
    /// Each of [`NAMING`] and [`EXEMPTING`] the environment sets, even empty, with what it
    /// holds, in that order. A schedule installed from here is given them, so that its runs
    /// read what this one read.
    set: Vec<(&'static str, String)>,
}

impl Proxies {
    /// What `env` says, by ureq 3.4.2's rules.
    pub(crate) fn read(env: &Environment) -> Proxies {
        let exempting = EXEMPTING.into_iter().find(|name| env.text(name).is_some());
        let entries: Vec<&str> = exempting
            .and_then(|name| env.text(name))
            .map_or_else(Vec::new, |list| list.split(',').collect());
        let mut read = Proxies {
            exempting,
            set: NAMING
                .iter()
                .chain(&EXEMPTING)
                .filter_map(|name| env.text(name).map(|value| (*name, value.to_string())))
                .collect(),
            ..Proxies::default()
        };
        for name in NAMING {
            let Some(value) = env.text(name) else {
                continue;
            };
            match ureq::Proxy::new(value) {
                Ok(proxy) => {
                    read.named = Some((name, exempting_from(&proxy, &entries)));
                    break;
                }
                Err(_) if value.is_empty() => {}
                Err(_) => read.passed_over.push(name),
            }
        }
        read
    }

    /// The proxy ureq is given: the one named, where it is an HTTP or an HTTPS one. A SOCKS
    /// proxy never is: built without its `socks-proxy` feature, ureq panics on one it is
    /// given, and the agent refuses the requests it would have carried itself
    /// ([`Proxies::refusal`]).
    pub(crate) fn for_ureq(&self) -> Option<ureq::Proxy> {
        self.named
            .as_ref()
            .map(|(_, proxy)| proxy)
            .filter(|proxy| tunnels(proxy))
            .cloned()
    }

    /// Whether the proxy named is a SOCKS one, which Pitboard does not use yet.
    pub(crate) fn socks(&self) -> bool {
        self.named
            .as_ref()
            .is_some_and(|(_, proxy)| !tunnels(proxy))
    }

    /// What the agent does with each request while the proxy named is a SOCKS one, where it
    /// is: refuse it, unless the variable exempting hosts sends it around the proxy.
    pub(crate) fn refusal(&self) -> Option<Refusal> {
        self.named
            .as_ref()
            .filter(|(_, proxy)| !tunnels(proxy))
            .map(|(variable, proxy)| Refusal {
                variable,
                proxy: proxy.clone(),
            })
    }

    /// The variable that names the proxy, where one does.
    pub(crate) fn variable(&self) -> Option<&'static str> {
        self.named.as_ref().map(|(name, _)| *name)
    }

    /// The proxy's address as Pitboard shows it ([`shown`]), where one is named.
    pub(crate) fn address(&self) -> Option<String> {
        self.named.as_ref().map(|(_, proxy)| shown(proxy))
    }

    /// The proxy's host and port, `proxy.example.com:3128`, as [`shown`] writes them, where
    /// one is named: the part of its address that can name the network it belongs to.
    pub(crate) fn server(&self) -> Option<String> {
        self.named.as_ref().map(|(_, proxy)| server(proxy))
    }

    /// Whether the proxy named is on loopback, this machine itself, which names nothing
    /// beyond it: `localhost`, or a loopback address.
    pub(crate) fn on_loopback(&self) -> bool {
        self.named.as_ref().is_some_and(|(_, proxy)| {
            let host = proxy.host();
            host.eq_ignore_ascii_case("localhost")
                || host
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        })
    }

    pub(crate) fn passed_over(&self) -> &[&'static str] {
        &self.passed_over
    }

    pub(crate) fn exempting(&self) -> Option<&'static str> {
        self.exempting
    }

    /// Each proxy variable the environment sets, even empty, with what it holds, in the
    /// order ureq reads them. What it holds can carry a password, so it is written only
    /// where the person alone can read it.
    pub(crate) fn variables(&self) -> &[(&'static str, String)] {
        &self.set
    }

    /// Whether a request to `url` goes around the proxy because the variable that exempts
    /// hosts names its host, judged by ureq itself. False where no proxy is named.
    pub(crate) fn exempts(&self, url: &Uri) -> bool {
        self.named
            .as_ref()
            .is_some_and(|(_, proxy)| proxy.is_no_proxy(url))
    }

    /// The hosts Pitboard calls ([`called`]) that go around the proxy, in that order.
    pub(crate) fn exempt_hosts(&self) -> Vec<String> {
        called()
            .iter()
            .filter_map(|url| url.parse::<Uri>().ok())
            .filter(|url| self.exempts(url))
            .filter_map(|url| url.host().map(str::to_string))
            .collect()
    }

    /// Whether Pitboard's requests go the same way under `self` as under `other`: through no
    /// proxy under both, or through the same one, its user name and password included, with
    /// the same hosts Pitboard calls exempt from it. Which variable names it does not matter.
    pub(crate) fn same_way(&self, other: &Proxies) -> bool {
        match (&self.named, &other.named) {
            (None, None) => true,
            (Some((_, mine)), Some((_, theirs))) => {
                mine.protocol() == theirs.protocol()
                    && mine.host().eq_ignore_ascii_case(theirs.host())
                    && mine.port() == theirs.port()
                    && mine.username() == theirs.username()
                    && mine.password() == theirs.password()
                    && mine.resolve_target() == theirs.resolve_target()
                    && self.exempt_hosts() == other.exempt_hosts()
            }
            _ => false,
        }
    }
}

/// Never the user name or the password a proxy's address may hold, nor anything a variable
/// holds: only the names of those set.
impl std::fmt::Debug for Proxies {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Proxies")
            .field("variable", &self.variable())
            .field("address", &self.address())
            .field("passed_over", &self.passed_over)
            .field("exempting", &self.exempting)
            .field(
                "set",
                &self.set.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            )
            .finish()
    }
}

/// Whether ureq tunnels through `proxy` with `CONNECT`, as it does through an HTTP or an
/// HTTPS proxy. Every other scheme ureq reads names a SOCKS proxy.
fn tunnels(proxy: &ureq::Proxy) -> bool {
    matches!(proxy.protocol(), ProxyProtocol::Http | ProxyProtocol::Https)
}

/// Why a request a SOCKS proxy would have carried fails, and what `doctor` says of the
/// proxy: that Pitboard does not use one yet, and which variable, `variable`, named it.
pub(crate) fn refused(variable: &str) -> String {
    format!("Pitboard does not use SOCKS proxies yet, and {variable} names one")
}

/// The agent's first step for each request it is handed while the proxy named is a SOCKS one
/// ([`Proxies::refusal`]). A request to a host the variable exempting hosts names goes on,
/// directly, since ureq is given no proxy. Any other fails before anything is sent, with
/// [`refused`]'s words, as ureq's error for a failed connection, so its cause is
/// `unreachable`. Nothing is looked up for it: not the proxy's name, which ureq is never
/// given, and not the host's, which ureq looks up only once this step hands the request on.
pub(crate) struct Refusal {
    variable: &'static str,
    proxy: ureq::Proxy,
}

impl Middleware for Refusal {
    fn handle(
        &self,
        request: Request<SendBody>,
        next: MiddlewareNext,
    ) -> Result<Response<Body>, ureq::Error> {
        if self.proxy.is_no_proxy(request.uri()) {
            return next.handle(request);
        }
        Err(ureq::Error::Io(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            refused(self.variable),
        )))
    }
}

/// `proxy`, with `entries` exempting hosts from it, as ureq makes one from a process's
/// environment. ureq takes the entries only through its builder, so the address it read is
/// handed back to it part by part: its scheme, host, port, user name and password, and
/// whether the host is looked up here, as it read each. Where the builder refuses what ureq
/// read, which no address it reads gives, the proxy is kept without the entries.
fn exempting_from(proxy: &ureq::Proxy, entries: &[&str]) -> ureq::Proxy {
    let mut builder = ureq::Proxy::builder(proxy.protocol())
        .host(proxy.host())
        .port(proxy.port())
        .resolve_target(proxy.resolve_target());
    if let Some(user) = proxy.username() {
        builder = builder.username(user);
    }
    if let Some(password) = proxy.password() {
        builder = builder.password(password);
    }
    for entry in entries {
        builder = builder.no_proxy(entry);
    }
    builder.build().unwrap_or_else(|_| proxy.clone())
}

/// A proxy's address as Pitboard shows it: its scheme, host and port as ureq reads them,
/// with `***` in place of a user name, and of a password, where the address holds one. Never
/// anything else of what was written: a path or a query is not part of a proxy's address.
pub(crate) fn shown(proxy: &ureq::Proxy) -> String {
    let hidden = match (proxy.username(), proxy.password()) {
        (Some(_), Some(_)) => "***:***@",
        (Some(_), None) => "***@",
        (None, _) => "",
    };
    format!(
        "{}://{hidden}{}",
        proxy.protocol().to_string().to_ascii_lowercase(),
        server(proxy)
    )
}

/// A proxy's host and port, as ureq reads them.
fn server(proxy: &ureq::Proxy) -> String {
    format!("{}:{}", proxy.host(), proxy.port())
}

/// Every address Pitboard sends a request to: Anthropic's two and OpenAI's two.
pub(crate) fn called() -> [&'static str; 4] {
    [
        crate::api::BASE,
        crate::api::AUTH_BASE,
        crate::provider::codex::api::TOKEN_URL,
        crate::provider::codex::api::USAGE_BASE,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// One environment, and what Pitboard reads from it: the variable that names the proxy
    /// and the address shown, the variables passed over, the one that exempts hosts, and the
    /// hosts Pitboard calls that it exempts.
    struct Row {
        env: &'static [(&'static str, &'static str)],
        named: Option<(&'static str, &'static str)>,
        passed_over: &'static [&'static str],
        exempting: Option<&'static str>,
        exempt: &'static [&'static str],
    }

    const NONE: Row = Row {
        env: &[],
        named: None,
        passed_over: &[],
        exempting: None,
        exempt: &[],
    };

    /// ureq 3.4.2's rules, row by row, as its `proxy.rs` reads.
    const TABLE: &[Row] = &[
        NONE,
        Row {
            env: &[("HTTPS_PROXY", "http://p.example:8080")],
            named: Some(("HTTPS_PROXY", "http://p.example:8080")),
            ..NONE
        },
        Row {
            env: &[("https_proxy", "http://p.example:8080")],
            named: Some(("https_proxy", "http://p.example:8080")),
            ..NONE
        },
        // Upper case before lower, for each name.
        Row {
            env: &[
                ("https_proxy", "http://lower.example:2"),
                ("HTTPS_PROXY", "http://upper.example:1"),
            ],
            named: Some(("HTTPS_PROXY", "http://upper.example:1")),
            ..NONE
        },
        // ALL_PROXY before HTTPS_PROXY, and all_proxy before HTTPS_PROXY too.
        Row {
            env: &[
                ("HTTPS_PROXY", "http://https.example:2"),
                ("ALL_PROXY", "http://all.example:1"),
            ],
            named: Some(("ALL_PROXY", "http://all.example:1")),
            ..NONE
        },
        Row {
            env: &[
                ("HTTPS_PROXY", "http://https.example:2"),
                ("all_proxy", "http://all.example:1"),
            ],
            named: Some(("all_proxy", "http://all.example:1")),
            ..NONE
        },
        // HTTP_PROXY is read for an https request, after both spellings of HTTPS_PROXY.
        Row {
            env: &[("HTTP_PROXY", "http://http.example:3")],
            named: Some(("HTTP_PROXY", "http://http.example:3")),
            ..NONE
        },
        Row {
            env: &[
                ("HTTP_PROXY", "http://http.example:3"),
                ("https_proxy", "http://https.example:2"),
            ],
            named: Some(("https_proxy", "http://https.example:2")),
            ..NONE
        },
        // Empty is nothing, and passed over without a word.
        Row {
            env: &[("HTTPS_PROXY", ""), ("HTTP_PROXY", "http://http.example:3")],
            named: Some(("HTTP_PROXY", "http://http.example:3")),
            ..NONE
        },
        // Something that is no address, or no scheme ureq knows, is passed over.
        Row {
            env: &[
                ("HTTPS_PROXY", "not a proxy"),
                ("http_proxy", "http://http.example:3"),
            ],
            named: Some(("http_proxy", "http://http.example:3")),
            passed_over: &["HTTPS_PROXY"],
            ..NONE
        },
        Row {
            env: &[("ALL_PROXY", "ftp://ftp.example"), ("https_proxy", "::")],
            passed_over: &["ALL_PROXY", "https_proxy"],
            ..NONE
        },
        // No scheme is http, and the port is the scheme's own where none is given.
        Row {
            env: &[("HTTPS_PROXY", "proxy.example:3128")],
            named: Some(("HTTPS_PROXY", "http://proxy.example:3128")),
            ..NONE
        },
        Row {
            env: &[("HTTPS_PROXY", "https://proxy.example")],
            named: Some(("HTTPS_PROXY", "https://proxy.example:443")),
            ..NONE
        },
        Row {
            env: &[("HTTPS_PROXY", "http://[::1]:8080/")],
            named: Some(("HTTPS_PROXY", "http://[::1]:8080")),
            ..NONE
        },
        // A user name and a password are read, and never shown.
        Row {
            env: &[("HTTPS_PROXY", "http://alice:s3cret@p.example:8080")],
            named: Some(("HTTPS_PROXY", "http://***:***@p.example:8080")),
            ..NONE
        },
        Row {
            env: &[("HTTPS_PROXY", "alice@p.example:8080")],
            named: Some(("HTTPS_PROXY", "http://***@p.example:8080")),
            ..NONE
        },
        // A SOCKS proxy is named, and read before what follows it.
        Row {
            env: &[
                ("all_proxy", "socks5://127.0.0.1:7890"),
                ("https_proxy", "http://127.0.0.1:7890"),
                ("http_proxy", "http://127.0.0.1:7890"),
            ],
            named: Some(("all_proxy", "socks5://127.0.0.1:7890")),
            ..NONE
        },
        Row {
            env: &[("HTTPS_PROXY", "socks5h://p.example")],
            named: Some(("HTTPS_PROXY", "socks5h://p.example:1080")),
            ..NONE
        },
        // NO_PROXY: a suffix, an exact host, and every host.
        Row {
            env: &[
                ("HTTPS_PROXY", "http://p.example:8080"),
                ("NO_PROXY", ".anthropic.com,chatgpt.com"),
            ],
            named: Some(("HTTPS_PROXY", "http://p.example:8080")),
            exempting: Some("NO_PROXY"),
            exempt: &["api.anthropic.com", "chatgpt.com"],
            ..NONE
        },
        Row {
            env: &[
                ("HTTPS_PROXY", "http://p.example:8080"),
                ("no_proxy", "*.OpenAI.com,Platform.Claude.com"),
            ],
            named: Some(("HTTPS_PROXY", "http://p.example:8080")),
            exempting: Some("no_proxy"),
            exempt: &["platform.claude.com", "auth.openai.com"],
            ..NONE
        },
        Row {
            env: &[("HTTPS_PROXY", "http://p.example:8080"), ("no_proxy", "*")],
            named: Some(("HTTPS_PROXY", "http://p.example:8080")),
            exempting: Some("no_proxy"),
            exempt: &[
                "api.anthropic.com",
                "platform.claude.com",
                "auth.openai.com",
                "chatgpt.com",
            ],
            ..NONE
        },
        // An entry ending with `*` or `.` exempts the names starting so, and one starting
        // with `*` but not `*.` the names ending so, whole names among them.
        Row {
            env: &[
                ("HTTPS_PROXY", "http://p.example:8080"),
                ("NO_PROXY", "api.,chat*"),
            ],
            named: Some(("HTTPS_PROXY", "http://p.example:8080")),
            exempting: Some("NO_PROXY"),
            exempt: &["api.anthropic.com", "chatgpt.com"],
            ..NONE
        },
        Row {
            env: &[
                ("HTTPS_PROXY", "http://p.example:8080"),
                ("no_proxy", "*claude.com,*gpt.com"),
            ],
            named: Some(("HTTPS_PROXY", "http://p.example:8080")),
            exempting: Some("no_proxy"),
            exempt: &["platform.claude.com", "chatgpt.com"],
            ..NONE
        },
        // NO_PROXY, set even empty, is the one read, so no_proxy is not.
        Row {
            env: &[
                ("HTTPS_PROXY", "http://p.example:8080"),
                ("NO_PROXY", ""),
                ("no_proxy", "*"),
            ],
            named: Some(("HTTPS_PROXY", "http://p.example:8080")),
            exempting: Some("NO_PROXY"),
            ..NONE
        },
        // A bare domain is that host alone, and a space is part of the name.
        Row {
            env: &[
                ("HTTPS_PROXY", "http://p.example:8080"),
                ("NO_PROXY", "anthropic.com, chatgpt.com"),
            ],
            named: Some(("HTTPS_PROXY", "http://p.example:8080")),
            exempting: Some("NO_PROXY"),
            ..NONE
        },
    ];

    fn environment(row: &Row) -> Environment {
        row.env.iter().copied().collect()
    }

    /// Everything ureq goes by in a proxy, apart from whether it read it itself.
    fn described(proxy: &ureq::Proxy) -> String {
        format!(
            "{:?} {} {} {:?} {:?} {}",
            proxy.protocol(),
            proxy.host(),
            proxy.port(),
            proxy.username(),
            proxy.password(),
            proxy.resolve_target()
        )
    }

    #[test]
    fn each_environment_reads_as_ureq_reads_it() {
        for (index, row) in TABLE.iter().enumerate() {
            let read = Proxies::read(&environment(row));
            let said = format!("row {index}: {:?}", row.env);
            assert_eq!(
                read.variable().zip(read.address()),
                row.named.map(|(name, address)| (name, address.to_string())),
                "{said}"
            );
            assert_eq!(read.passed_over(), row.passed_over, "{said}");
            assert_eq!(read.exempting(), row.exempting, "{said}");
            assert_eq!(read.exempt_hosts(), row.exempt, "{said}");
        }
    }

    /// Set in a child's environment to the row of [`TABLE`] it checks, which is how the child
    /// knows it is one.
    const ROW: &str = "PITBOARD_TEST_PROXY_ROW";

    /// This test's name, which a child is asked to run and nothing else.
    const AGREES: &str = "proxy::tests::every_row_agrees_with_what_ureq_reads_from_a_process";

    /// Each row of the table, put in a process's own environment, is read by ureq's own
    /// `Proxy::try_from_env` as Pitboard reads it from a context: the same proxy, exempting
    /// the same hosts. So the table is ureq's rules, not a reading of them, and a ureq that
    /// changes them fails here.
    ///
    /// The variables have to be in the process's environment from the start, and setting
    /// them from a test would change it under every other test's threads, so each row runs
    /// in a child: this test binary again, given that row's variables and nothing else of
    /// its kind.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W14: a Windows environment, whose names are one in any case"
    )]
    #[allow(
        clippy::disallowed_methods,
        reason = "a child is told which row it checks through its environment"
    )]
    fn every_row_agrees_with_what_ureq_reads_from_a_process() {
        if let Some(row) = std::env::var_os(ROW) {
            let index: usize = row.to_str().and_then(|r| r.parse().ok()).expect("a row");
            return agrees_in_this_process(&TABLE[index]);
        }
        for (index, row) in TABLE.iter().enumerate() {
            let mut child = std::process::Command::new(std::env::current_exe().expect("me"));
            child
                .args([AGREES, "--exact", "--test-threads=1"])
                .env(ROW, index.to_string());
            for name in NAMING.iter().chain(&EXEMPTING) {
                child.env_remove(name);
            }
            for (name, value) in row.env {
                child.env(name, value);
            }
            let out = child.output().expect("this test binary runs");
            let said = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(
                out.status.success() && said.contains("1 passed"),
                "row {index}, {:?}:\n{said}",
                row.env
            );
        }
    }

    /// What a child checks: ureq's reading of this process's environment against Pitboard's
    /// reading of a context made from the same variables, and the agent that context's
    /// requests go out on, which is given Pitboard's.
    #[allow(
        clippy::disallowed_methods,
        reason = "ureq's own reading of this process's environment, which Pitboard's is held to"
    )]
    fn agrees_in_this_process(row: &Row) {
        let ours = Proxies::read(&environment(row));
        let theirs = ureq::Proxy::try_from_env();
        assert_eq!(
            ours.named.as_ref().map(|(_, proxy)| described(proxy)),
            theirs.as_ref().map(described),
            "the proxy"
        );
        let urls = called()
            .iter()
            .chain(&["http://127.0.0.1:8080/", "https://API.Anthropic.com/"])
            .map(|url| url.parse::<Uri>().expect("a URL"))
            .collect::<Vec<_>>();
        for url in &urls {
            assert_eq!(
                ours.exempts(url),
                theirs.as_ref().is_some_and(|proxy| proxy.is_no_proxy(url)),
                "{url}"
            );
        }

        // The context's agent is given the proxy the context read, and never the one ureq
        // read from this process: a unit test's context withholds every variable, so its
        // agent goes through none, whatever this process says.
        let given: Environment = row
            .env
            .iter()
            .copied()
            .chain([("HOME", "/nowhere")])
            .collect();
        let ctx = Context::for_command_line(&given);
        assert_eq!(
            crate::api::agent(&ctx).config().proxy().map(described),
            ours.for_ureq().as_ref().map(described),
            "the agent"
        );
        assert!(
            crate::api::agent(&Context::for_unit_test())
                .config()
                .proxy()
                .is_none(),
            "a unit test's agent goes through no proxy"
        );
    }

    /// ureq is given an HTTP or an HTTPS proxy, with the hosts exempt from it. A SOCKS one,
    /// which Pitboard does not use yet, it is never given, since it panics on one built
    /// without its `socks-proxy` feature: the agent refuses its requests itself, with the
    /// hosts exempt from it all the same, and follows no redirect.
    #[test]
    fn ureq_is_given_an_http_proxy_and_never_a_socks_one() {
        let read = |pairs: &[(&'static str, &str)]| Proxies::read(&pairs.iter().copied().collect());
        let http = read(&[
            ("HTTPS_PROXY", "http://alice:s3cret@p.example:8080"),
            ("NO_PROXY", ".anthropic.com"),
        ]);
        let proxy = http.for_ureq().expect("an HTTP proxy is given");
        assert_eq!(
            (
                proxy.host(),
                proxy.port(),
                proxy.username(),
                proxy.password()
            ),
            ("p.example", 8080, Some("alice"), Some("s3cret"))
        );
        let api = "https://api.anthropic.com/".parse::<Uri>().expect("a URL");
        let auth = "https://auth.openai.com/".parse::<Uri>().expect("a URL");
        assert!(proxy.is_no_proxy(&api) && !proxy.is_no_proxy(&auth));
        assert!(!http.socks() && http.refusal().is_none());
        let https = read(&[("HTTPS_PROXY", "https://p.example")]);
        assert!(https.for_ureq().is_some() && !https.socks());
        let ctx = read_from(&[("HTTPS_PROXY", "http://p.example:8080")]);
        let agent = crate::api::agent(&ctx);
        assert!(agent.config().proxy().is_some());
        assert_eq!(agent.config().max_redirects(), 10, "ureq's own");

        for address in [
            "socks://alice:s3cret@p",
            "socks4://p",
            "socks4a://p",
            "socks5://p",
            "socks5h://p",
            "SOCKS5H://p",
        ] {
            let socks = read(&[("ALL_PROXY", address), ("NO_PROXY", ".anthropic.com")]);
            assert!(socks.for_ureq().is_none(), "{address} is given");
            assert!(socks.socks() && socks.refusal().is_some(), "{address}");
            assert_eq!(socks.variable(), Some("ALL_PROXY"), "{address}");
            assert!(socks.exempts(&api) && !socks.exempts(&auth), "{address}");
            let ctx = read_from(&[("ALL_PROXY", address)]);
            let agent = crate::api::agent(&ctx);
            assert!(agent.config().proxy().is_none(), "{address}");
            assert_eq!(agent.config().max_redirects(), 0, "{address}");
        }
        let none = read(&[]);
        assert!(none.for_ureq().is_none() && !none.socks() && none.refusal().is_none());
    }

    /// A context's debug form, which a panic or a log may print, says which variable names
    /// the proxy and shows its address as `doctor` does: never its user name or password.
    #[test]
    fn a_contexts_debug_form_never_shows_a_proxys_password() {
        let env: Environment = [
            ("HOME", "/nowhere"),
            ("HTTPS_PROXY", "http://alice:s3cret@p.example:8080"),
        ]
        .into_iter()
        .collect();
        let ctx = Context::for_command_line(&env);
        let _ = crate::api::agent(&ctx);
        let debug = format!("{ctx:?}");
        assert!(debug.contains("HTTPS_PROXY"), "{debug}");
        assert!(debug.contains("http://***:***@p.example:8080"), "{debug}");
        for secret in ["alice", "s3cret"] {
            assert!(!debug.contains(secret), "{secret}: {debug}");
        }
    }

    /// A context made without an environment names no proxy, so a test or an app that starts
    /// from one reaches the services directly unless it was read from an environment.
    #[test]
    fn a_context_made_without_an_environment_names_no_proxy() {
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"));
        assert_eq!(ctx.proxy.variable(), None);
        assert!(crate::api::agent(&ctx).config().proxy().is_none());
    }

    /// A context read from `pairs`, and nothing else of the environment but a home.
    fn read_from(pairs: &[(&'static str, &str)]) -> Context {
        let env: Environment = pairs
            .iter()
            .copied()
            .chain([("HOME", "/nowhere")])
            .collect();
        Context::for_command_line(&env)
    }

    /// Every scheme naming a SOCKS proxy, each with a variable that names it.
    const SOCKS: [(&str, &str); 5] = [
        ("socks4", "HTTPS_PROXY"),
        ("socks4a", "all_proxy"),
        ("socks5", "ALL_PROXY"),
        ("socks5h", "http_proxy"),
        ("socks", "HTTP_PROXY"),
    ];

    /// The words a request through a SOCKS proxy fails with, `variable` having named it.
    fn failing_with(variable: &str) -> String {
        format!("io: Pitboard does not use SOCKS proxies yet, and {variable} names one")
    }

    /// A loopback port that takes no part: nothing accepts what connects to it, and a test
    /// asks afterwards whether anything did.
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

    /// A host on loopback that answers each request with `answer`, an HTTP response's head,
    /// and counts the requests it was sent.
    fn host_answering(answer: String) -> (u16, Arc<Mutex<usize>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port on loopback");
        let port = listener.local_addr().expect("its address").port();
        let asked = Arc::new(Mutex::new(0));
        let counted = Arc::clone(&asked);
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader, Write};
            for mut client in listener.incoming().flatten() {
                let mut head = BufReader::new(client.try_clone().expect("a second handle"));
                let mut line = String::new();
                while head.read_line(&mut line).is_ok_and(|n| n > 0) && line != "\r\n" {
                    line.clear();
                }
                *counted.lock().expect("not poisoned") += 1;
                let _ = client.write_all(answer.as_bytes());
            }
        });
        (port, asked)
    }

    const NO_CONTENT: &str =
        "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

    /// What `agent` makes of a GET of `url`: the status, or the error in ureq's words.
    fn got(agent: &ureq::Agent, url: &str) -> Result<u16, String> {
        agent
            .get(url)
            .call()
            .map(|response| response.status().as_u16())
            .map_err(|e| e.to_string())
    }

    /// A request with a SOCKS proxy named, whichever scheme and whichever variable names it,
    /// fails before anything is sent, saying that Pitboard does not use SOCKS proxies yet and
    /// which variable named one. Neither the proxy nor the host is reached, so no request goes
    /// out past the proxy the person named, and none waits on a proxy that never answers.
    ///
    /// ureq 3.4.2 without its `socks-proxy` feature went around a `socks4://` or `socks5://`
    /// proxy it read itself, straight to the host, and failed a request with a `socks4a://`
    /// or `socks5h://` one with "Connection refused". Its feature's connector waited past the
    /// time limit on a proxy that never answers, measured on 7 October 2026.
    #[test]
    fn every_socks_proxy_is_refused_before_anything_is_sent() {
        let (listener, port) = untouched();
        for (scheme, variable) in SOCKS {
            let proxy = format!("{scheme}://127.0.0.1:{port}");
            let api_base = format!("http://127.0.0.1:{port}");
            let ctx = read_from(&[
                (variable, proxy.as_str()),
                ("PITBOARD_API_BASE", api_base.as_str()),
            ]);
            assert_eq!(
                got(crate::api::agent(&ctx), &format!("{api_base}/")),
                Err(failing_with(variable)),
                "{scheme}"
            );
            let asked = crate::api::owner(&ctx, "a-token").expect_err("refused");
            assert_eq!(
                (asked.to_string(), crate::error::Cause::of(&asked).code()),
                (
                    format!("could not reach Anthropic: {}", failing_with(variable)),
                    "unreachable"
                ),
                "{scheme}"
            );
            assert!(!reached(&listener), "{scheme}: something was sent");
        }
    }

    /// Looks a host up as ureq does, and keeps each host it was asked for. The SOCKS proxy's
    /// host, [`UNKNOWN_PROXY`], it says is not found, without asking anybody, as a company's
    /// proxy is not found off its network.
    #[derive(Debug, Default)]
    struct Recording(Arc<Mutex<Vec<String>>>);

    const UNKNOWN_PROXY: &str = "socks.pitboard-test.invalid";

    impl ureq::unversioned::resolver::Resolver for Recording {
        #[allow(
            clippy::disallowed_types,
            reason = "ureq hands a resolver the agent's configuration, made in api::made"
        )]
        fn resolve(
            &self,
            uri: &Uri,
            config: &ureq::config::Config,
            timeout: ureq::unversioned::transport::NextTimeout,
        ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
            let host = uri.host().unwrap_or_default().to_string();
            self.0.lock().expect("not poisoned").push(host.clone());
            if host == UNKNOWN_PROXY {
                return Err(ureq::Error::HostNotFound);
            }
            ureq::unversioned::resolver::DefaultResolver::default().resolve(uri, config, timeout)
        }
    }

    /// A host the variable exempting hosts names goes out directly with a SOCKS proxy named,
    /// and the variable is asked before anything else: the proxy's own name is never looked
    /// up, so the host is reached where that name is not found, as off a company's network.
    /// A host it does not name is refused with nothing looked up at all.
    ///
    /// ureq 3.4.2's own SOCKS connector looked the proxy's host up before it asked whether
    /// the request's host was exempt, so every request failed there, as an HTTP proxy's never
    /// did.
    #[test]
    fn an_exempt_host_is_reached_without_looking_the_socks_proxy_up() {
        let (port, asked) = host_answering(NO_CONTENT.to_string());
        let url = format!("http://127.0.0.1:{port}/");
        for (scheme, variable) in SOCKS {
            let proxy = format!("{scheme}://{UNKNOWN_PROXY}:1080");
            let looked_up = Recording::default();
            let lookups = Arc::clone(&looked_up.0);
            let exempt = read_from(&[(variable, proxy.as_str()), ("NO_PROXY", "127.0.0.1")]);
            let agent = crate::api::made(&exempt, looked_up);
            assert_eq!(got(&agent, &url), Ok(204), "{scheme}");
            assert_eq!(
                *lookups.lock().expect("not poisoned"),
                ["127.0.0.1"],
                "{scheme}: only the host was looked up"
            );

            let looked_up = Recording::default();
            let lookups = Arc::clone(&looked_up.0);
            let not_exempt = read_from(&[(variable, proxy.as_str())]);
            let agent = crate::api::made(&not_exempt, looked_up);
            assert_eq!(got(&agent, &url), Err(failing_with(variable)), "{scheme}");
            assert!(
                lookups.lock().expect("not poisoned").is_empty(),
                "{scheme}: something was looked up"
            );
        }
        assert_eq!(
            *asked.lock().expect("not poisoned"),
            SOCKS.len(),
            "the host was asked once for each exempt request, and for nothing else"
        );
    }

    /// With a SOCKS proxy named, a redirect from a host the variable exempting hosts names is
    /// not followed, so it cannot take a request past the proxy to a host it does not name:
    /// the redirect comes back as the answer, and that host is never reached.
    ///
    /// The refusal is the agent's first step for each request it is handed, and ureq follows a
    /// redirect within that request, after the step: one it followed would go out directly.
    #[test]
    fn a_redirect_is_not_followed_past_a_socks_proxy() {
        let (elsewhere, port) = untouched();
        let (proxy, proxy_port) = untouched();
        let (exempt_port, asked) = host_answering(format!(
            "HTTP/1.1 302 Found\r\nLocation: http://localhost:{port}/\r\n\
             Content-Length: 0\r\nConnection: close\r\n\r\n"
        ));
        for (scheme, variable) in SOCKS {
            let address = format!("{scheme}://127.0.0.1:{proxy_port}");
            let ctx = read_from(&[(variable, address.as_str()), ("NO_PROXY", "127.0.0.1")]);
            assert_eq!(
                got(
                    crate::api::agent(&ctx),
                    &format!("http://127.0.0.1:{exempt_port}/")
                ),
                Ok(302),
                "{scheme}"
            );
            assert!(!reached(&elsewhere), "{scheme}: the redirect was followed");
            assert!(!reached(&proxy), "{scheme}: the proxy was reached");
        }
        assert_eq!(*asked.lock().expect("not poisoned"), SOCKS.len());
    }
}
