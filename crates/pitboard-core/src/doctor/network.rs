use super::{Check, ok, warn};
use crate::words;
use std::fmt::Write as _;

/// How Pitboard's requests leave this machine: directly, or through the HTTP proxy its
/// environment names, read as ureq 3.4.2 reads one from a process's environment. A SOCKS
/// proxy, which Pitboard does not use yet, fails each request it would have carried.
pub(crate) struct NetworkFact {
    /// The proxy the environment names, where it names one Pitboard can read.
    pub(crate) proxy: Option<ProxyFact>,
    /// The variables before it set to something that is not a proxy's address, which
    /// Pitboard passes over, in the order it reads them.
    pub(crate) passed_over: Vec<&'static str>,
    // Each proxy variable set to anything, as the tools read the ones they read.
    pub(crate) set: Vec<&'static str>,
}

impl NetworkFact {
    pub(super) fn sets(&self, any_of: &[&str]) -> Option<&'static str> {
        self.set.iter().copied().find(|name| any_of.contains(name))
    }
}

/// The proxy the environment names.
pub(crate) struct ProxyFact {
    /// The variable that names it.
    pub(crate) variable: &'static str,
    /// Its scheme, host and port, with `***` for a user name or a password it holds.
    pub(crate) address: String,
    /// Its host and port, `proxy.example.com:3128`, as `address` shows them: the part of it
    /// that can name the company whose network it is.
    pub(crate) server: String,
    /// Whether its host is loopback, this machine itself, which names nothing beyond it.
    pub(crate) on_loopback: bool,
    /// Whether it is a SOCKS proxy, which Pitboard does not use yet: a request to a host it
    /// does not exempt fails before anything is sent, and one to a host it exempts goes out
    /// directly.
    pub(crate) socks: bool,
    // Whether a request to a host Pitboard calls fails, as `Proxies::refuses_some` says.
    pub(crate) fails: bool,
    /// `NO_PROXY` or `no_proxy`, whichever is read, where either is set.
    pub(crate) exempting: Option<&'static str>,
    /// The hosts Pitboard calls that it exempts, which Pitboard reaches directly.
    pub(crate) exempt: Vec<String>,
}

/// How the context's requests leave this machine, as its environment says.
pub(super) fn network_fact(proxies: &crate::proxy::Proxies) -> NetworkFact {
    NetworkFact {
        proxy: proxies
            .variable()
            .zip(proxies.address())
            .map(|(variable, address)| ProxyFact {
                variable,
                address,
                server: proxies.server().unwrap_or_default(),
                on_loopback: proxies.on_loopback(),
                socks: proxies.socks(),
                fails: proxies.refuses_some(),
                exempting: proxies.exempting(),
                exempt: proxies.exempt_hosts(),
            }),
        passed_over: proxies.passed_over().to_vec(),
        set: proxies
            .variables()
            .iter()
            .filter(|(name, value)| !value.is_empty() && crate::proxy::NAMING.contains(name))
            .map(|(name, _)| *name)
            .collect(),
    }
}

/// How Pitboard's requests leave this machine: directly, or through the HTTP proxy the
/// environment names, and from which variable, with the hosts Pitboard calls that the
/// variable exempting hosts sends around it. It warns where Pitboard passes over a variable
/// that holds no proxy's address, since its requests then go where the person may have meant
/// them not to.
///
/// A SOCKS proxy, which Pitboard does not use yet, is named in the words a request fails
/// with, then the proxy, and warns, since each request to a host Pitboard calls that the
/// variable exempting hosts does not name fails. With every one of them exempt, nothing
/// fails, and it does not warn for that.
pub(super) fn judge_network(network: &NetworkFact) -> Check {
    pub(super) const NAME: &str = "network";
    let (passed, set_right) = match network.passed_over.as_slice() {
        [] => (None, ""),
        [one] => (
            Some(format!(
                "{one} holds no proxy's address Pitboard can read, so it is passed over"
            )),
            "Set it to a proxy's address, such as http://proxy.example.com:8080, or unset it.",
        ),
        many => (
            Some(format!(
                "{} hold no proxy's address Pitboard can read, so they are passed over",
                words::listed(many.iter())
            )),
            "Set each to a proxy's address, such as http://proxy.example.com:8080, or unset them.",
        ),
    };
    let Some(proxy) = &network.proxy else {
        return match passed {
            None => ok("network", NAME, "direct: no variable names a proxy"),
            Some(passed) => warn("network", NAME, format!("direct: {passed}"), set_right),
        };
    };
    let fails = proxy.fails;
    let mut detail = if !proxy.socks {
        format!("through {}, from {}", proxy.address, proxy.variable)
    } else {
        format!(
            "{}: {}, {}",
            if fails { "requests fail" } else { "direct" },
            crate::proxy::refused(proxy.variable),
            proxy.address
        )
    };
    for clause in exempting(proxy).iter().chain(&passed) {
        let _ = write!(detail, "; {clause}");
    }
    if fails {
        let (each, them) = if network.passed_over.is_empty() {
            (proxy.variable.to_string(), "it")
        } else {
            let mut names: Vec<String> = network
                .passed_over
                .iter()
                .map(|name| (*name).to_string())
                .collect();
            names.push(proxy.variable.to_string());
            (format!("each of {}", words::listed(names)), "them")
        };
        return warn(
            "network",
            NAME,
            detail,
            format!(
                "Pitboard goes through HTTP and HTTPS proxies only. For its requests to go \
                 out, set {each} to an HTTP proxy's address, such as \
                 http://proxy.example.com:8080, or unset {them}."
            ),
        );
    }
    match passed {
        None => ok("network", NAME, detail),
        Some(_) => warn("network", NAME, detail, set_right),
    }
}

/// What the variable exempting hosts does with the hosts Pitboard calls, where one is set:
/// "NO_PROXY exempts api.anthropic.com, which Pitboard reaches directly".
pub(super) fn exempting(proxy: &ProxyFact) -> Option<String> {
    proxy.exempting.map(|exempting| {
        if proxy.exempt.is_empty() {
            format!("{exempting} exempts none of the hosts Pitboard calls")
        } else if proxy.exempt.len() == crate::proxy::called().len() {
            format!("{exempting} exempts every host Pitboard calls, which it reaches directly")
        } else {
            format!(
                "{exempting} exempts {}, which Pitboard reaches directly",
                words::listed(proxy.exempt.clone())
            )
        }
    })
}
