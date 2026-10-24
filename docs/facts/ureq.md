# ureq's proxies

Read in ureq 3.4.2's `src/proxy.rs`, `src/config.rs`, `src/run.rs`, `src/middleware.rs` and
`src/unversioned/transport/`, and the socks crate 0.3.4's `src/lib.rs`, `src/v4.rs` and
`src/v5.rs`, on 7 October 2026, and measured on macOS 27.0 the same day.
`proxy.rs` reads the variables by these rules, and its tests check each row of their table
against ureq's own `Proxy::try_from_env` in a process given that row's variables.

- `Proxy::try_from_env` tries `ALL_PROXY`, `all_proxy`, `HTTPS_PROXY`, `https_proxy`,
  `HTTP_PROXY` and `http_proxy`, in that order, and takes the first whose value parses as a
  proxy's address. It does not look at the request's scheme, so `HTTP_PROXY` applies to an
  https request, and `ALL_PROXY` wins over `HTTPS_PROXY`. An empty value, or one that does
  not parse, is passed over. An address with no scheme is `http://`.
- `NoProxy::try_from_env` takes the first of `NO_PROXY` and `no_proxy` that is set, even
  empty, splits it at commas without trimming, and matches each entry against the request's
  host without regard to ASCII case: `*` matches every host, an entry starting with `*` or
  `.` matches the end of the host, one ending with `*` or `.` its start, and any other only
  the same host.
- `Config::default()`, where every agent's configuration starts, calls
  `Proxy::try_from_env`. `ConfigBuilder::proxy` replaces what it found; nothing else turns
  the read off. The `win-system-proxy` feature, which Pitboard does not turn on, adds the
  registry's proxy on Windows.
- The scheme is read without regard to case. `http` and `https` name a proxy ureq asks with
  `CONNECT`; `socks4`, `socks4a`, `socks5`, `socks5h`, and `socks`, which is `socks5`, a
  SOCKS one. Without the `socks-proxy` feature, a SOCKS proxy ureq read from the environment
  is gone around, straight to the host, with a logged warning, and one it is given panics
  on the first request, whether or not `NO_PROXY` names the host: both in
  `DefaultConnector`'s `WarnOnNoSocksConnector`.
- The feature, which brings the socks crate 0.3.4 and byteorder 1.5.0, puts its
  `SocksConnector` first in the default chain of connectors
  (`src/unversioned/transport/mod.rs`). That connector has two faults:
  - Its handshake has no time limit. `try_connect_single` runs the socks crate on a thread
    inside `thread::scope` and waits with `recv_timeout`, but the scope joins the thread
    before it returns, and the socks crate's `TcpStream::connect` and `read_exact` set no
    timeout. Measured with the feature on and a 5-second limit, against a loopback listener
    that takes each connection and never answers: a request through `socks4://`,
    `socks4a://`, `socks5://`, `socks5h://` or `socks://` was still waiting after 15
    seconds, where one through an `http://` proxy failed with `timeout: global` at 5.
  - It looks the proxy's own host up before it asks whether `NO_PROXY` exempts the
    request's host (`socks.rs`, lines 50 to 63), where the `CONNECT` connector asks first
    (`connect.rs`, lines 48 to 64). Measured with the feature on, a resolver that cannot
    find the proxy's name and `NO_PROXY` naming the host: every request to that host failed
    with `host not found`.
- The owner decided on 7 October 2026 to have both fixed in ureq rather than keep a SOCKS
  client of Pitboard's own, and that Pitboard refuses every SOCKS proxy until a ureq release
  has the fix: a handshake that gives up at the request's time limit, and `NO_PROXY` asked
  before the proxy's name is looked up. Then the feature can be turned on.
- Until then ureq is given no SOCKS proxy, and the agent's middleware, `proxy::Refusal`,
  takes each request first. A request to a host `NO_PROXY` names goes on, directly; any
  other fails with "Pitboard does not use SOCKS proxies yet, and ALL_PROXY names one", as
  `Error::Io`, before anything is sent or looked up. A middleware that returns without
  calling `next` sends nothing. ureq runs the middleware once for each request and follows
  redirects inside `run`, after it, so the agent is given `max_redirects(0)` while a SOCKS
  proxy is named: measured without it, a redirect from an exempt loopback host to a host
  `NO_PROXY` does not name went out directly and waited there for its answer.
- `run.rs` looks a request's host up here only where there is no proxy, `NO_PROXY` exempts
  the host, or the proxy's `resolve_target` is set, which `socks4`, `socks5` and `socks`
  set and `socks4a`, `socks5h`, `http` and `https` do not. With the feature, ureq's
  connector hands a proxy with `resolve_target` the first address the lookup gave, trying
  the next only where the proxy refused the TCP connection, and one without it the host's
  name and port. The socks crate fails an IPv6 address behind SOCKS4 with "SOCKS4 does not
  support IPv6" before it sends the proxy anything, so `localhost`, which this Mac looks up
  as `::1` before `127.0.0.1`, failed each request behind `socks4://`. It gives SOCKS4 an
  empty user id, whatever the address holds, and SOCKS5 the address's user name and
  password, offering no sign-in as well (`5, 2, 2, 0`).
- Measured against a loopback server with `ALL_PROXY` set and ureq, without the feature,
  reading it: `socks4://`, `socks5://` and `socks://` went around the proxy and the request
  was answered; `socks4a://` and `socks5h://` left the request with no address to connect
  to, and it failed with `Connection refused`, unless `NO_PROXY` named the host. That is
  what 0.7.0 did. With the refusal, each of the five fails before anything reaches the
  proxy or the host, in the core's tests (`proxy.rs`) and in `pitboard enroll`
  (`tests/cli/proxy.rs`), and an exempt host is reached with nothing but its own address
  looked up.
- `Proxy::username` and `Proxy::password` are the address's own text, split at the last
  `:` before the `@`, and ureq sends them to an HTTP proxy as `Proxy-Authorization: Basic`.
