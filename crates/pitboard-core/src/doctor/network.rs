use super::{Check, ok, warn, words};
use std::fmt::Write as _;

// The environment's proxy, read as ureq 3.4.2 reads a process's.
pub(crate) struct NetworkFact {
    pub(crate) proxy: Option<ProxyFact>,
    // The variables before it that hold no proxy's address, in the order they are read.
    pub(crate) passed_over: Vec<&'static str>,
    // Each proxy variable set to anything, which the tools read by their own rules.
    pub(crate) set: Vec<&'static str>,
}

impl NetworkFact {
    pub(super) fn sets(&self, any_of: &[&str]) -> Option<&'static str> {
        self.set.iter().copied().find(|name| any_of.contains(name))
    }
}

pub(crate) struct ProxyFact {
    pub(crate) variable: &'static str,
    // With `***` for a user name or a password.
    pub(crate) address: String,
    // Host and port: the part that can name the company whose network it is.
    pub(crate) server: String,
    pub(crate) on_loopback: bool,
    pub(crate) socks: bool,
    // As `Proxies::refuses_some` says.
    pub(crate) fails: bool,
    pub(crate) exempting: Option<&'static str>,
    // The hosts Pitboard calls that go around it.
    pub(crate) exempt: Vec<String>,
}

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

// A variable passed over sends requests where the person may have meant them not to go.
pub(super) fn judge_network(network: &NetworkFact) -> Check {
    const NAME: &str = "network";
    let (passed, set_right) = match network.passed_over.as_slice() {
        [] => (None, ""),
        [_] => (
            Some(words::passed_over(&network.passed_over)),
            words::SET_IT_RIGHT,
        ),
        _ => (
            Some(words::passed_over(&network.passed_over)),
            words::SET_EACH_RIGHT,
        ),
    };
    let Some(proxy) = &network.proxy else {
        return match passed {
            None => ok("network", NAME, words::DIRECT),
            Some(passed) => warn(
                "network",
                NAME,
                words::direct_passing_over(&passed),
                set_right,
            ),
        };
    };
    let fails = proxy.fails;
    let mut detail = if !proxy.socks {
        words::through(&proxy.address, proxy.variable)
    } else {
        words::socks(fails, proxy.variable, &proxy.address)
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
            (words::each_of(&names), "them")
        };
        return warn(
            "network",
            NAME,
            detail,
            words::http_proxies_only(&each, them),
        );
    }
    match passed {
        None => ok("network", NAME, detail),
        Some(_) => warn("network", NAME, detail, set_right),
    }
}

pub(super) fn exempting(proxy: &ProxyFact) -> Option<String> {
    proxy.exempting.map(|exempting| {
        if proxy.exempt.is_empty() {
            words::exempts_none(exempting)
        } else if proxy.exempt.len() == crate::proxy::called().len() {
            words::exempts_every_host(exempting)
        } else {
            words::exempts(exempting, &proxy.exempt)
        }
    })
}
