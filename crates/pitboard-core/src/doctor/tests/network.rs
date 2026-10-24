use super::*;

/// The network check, for a network that is `proxy`, passing over `passed_over`.
fn network(proxy: Option<ProxyFact>, passed_over: &[&'static str]) -> Check {
    let mut f = facts();
    f.network = NetworkFact {
        set: passed_over
            .iter()
            .copied()
            .chain(proxy.as_ref().map(|proxy| proxy.variable))
            .collect(),
        proxy,
        passed_over: passed_over.to_vec(),
    };
    let checks = evaluate(&f);
    let found = check(&checks, "network");
    Check {
        code: found.code,
        name: found.name.clone(),
        level: found.level,
        detail: found.detail.clone(),
        advice: found.advice.clone(),
    }
}

/// doctor says how Pitboard's requests leave this machine: directly, or through which
/// proxy and from which variable, and which of the hosts Pitboard calls go around it. It
/// had no such check, while ureq read the variables itself, so nobody could tell.
#[test]
fn the_network_check_names_the_proxy_and_the_variable_it_came_from() {
    let direct = network(None, &[]);
    assert_eq!(
        (direct.level, direct.name.as_str(), direct.detail.as_str()),
        (Level::Ok, "network", "direct: no variable names a proxy")
    );
    for (exempting, exempt, said) in [
        (None, &[][..], ""),
        (
            Some("NO_PROXY"),
            &[],
            "; NO_PROXY exempts none of the hosts Pitboard calls",
        ),
        (
            Some("no_proxy"),
            &["api.anthropic.com"],
            "; no_proxy exempts api.anthropic.com, which Pitboard reaches directly",
        ),
        (
            Some("NO_PROXY"),
            &["api.anthropic.com", "platform.claude.com"],
            "; NO_PROXY exempts api.anthropic.com and platform.claude.com, which Pitboard \
             reaches directly",
        ),
        (
            Some("NO_PROXY"),
            &[
                "api.anthropic.com",
                "platform.claude.com",
                "auth.openai.com",
                "chatgpt.com",
            ],
            "; NO_PROXY exempts every host Pitboard calls, which it reaches directly",
        ),
    ] {
        let check = network(Some(through(exempting, exempt)), &[]);
        assert_eq!(check.level, Level::Ok, "{said}");
        assert_eq!(
            check.detail,
            format!("through http://***:***@proxy.example.com:3128, from HTTPS_PROXY{said}")
        );
        assert!(check.advice.is_empty(), "{}", check.advice);
    }
}

/// The network check of a context read from `pairs`, and nothing else of the environment
/// but a home.
fn network_read_from(pairs: &[(&'static str, &str)]) -> Check {
    let env: crate::context::Environment = pairs
        .iter()
        .copied()
        .chain([("HOME", "/nowhere")])
        .collect();
    let fact = network_fact(&Context::for_command_line(&env).proxy);
    network(fact.proxy, &fact.passed_over)
}

/// A SOCKS proxy, which Pitboard does not use yet, is a warning. The check says so in the
/// words a request fails with, naming the variable that named the proxy, then the proxy,
/// and says what to set. A host the variable exempting hosts names still goes out
/// directly; with every host Pitboard calls exempt, nothing fails, and it is no warning.
///
/// It named a SOCKS proxy as one Pitboard went through, like any other, while Pitboard
/// spoke SOCKS itself.
#[test]
fn a_socks_proxy_is_a_warning_naming_its_variable() {
    let socks = network_read_from(&[("all_proxy", "socks5://127.0.0.1:7890")]);
    assert_eq!(
        (socks.level, socks.detail.as_str(), socks.advice.as_str()),
        (
            Level::Warn,
            "requests fail: Pitboard does not use SOCKS proxies yet, and all_proxy names \
             one, socks5://127.0.0.1:7890",
            "Pitboard goes through HTTP and HTTPS proxies only. For its requests to go \
             out, set all_proxy to an HTTP proxy's address, such as \
             http://proxy.example.com:8080, or unset it."
        )
    );

    let some = network_read_from(&[
        ("ALL_PROXY", "socks5h://alice:s3cret@p.example"),
        ("NO_PROXY", "chatgpt.com"),
    ]);
    assert_eq!(
        (some.level, some.detail.as_str()),
        (
            Level::Warn,
            "requests fail: Pitboard does not use SOCKS proxies yet, and ALL_PROXY names \
             one, socks5h://***:***@p.example:1080; NO_PROXY exempts chatgpt.com, which \
             Pitboard reaches directly"
        )
    );

    let passed = network_read_from(&[
        ("ALL_PROXY", "not a proxy"),
        ("HTTPS_PROXY", "socks://p.example"),
    ]);
    assert_eq!(
        (passed.level, passed.detail.as_str(), passed.advice.as_str()),
        (
            Level::Warn,
            "requests fail: Pitboard does not use SOCKS proxies yet, and HTTPS_PROXY \
             names one, socks5://p.example:1080; ALL_PROXY holds no proxy's address \
             Pitboard can read, so it is passed over",
            "Pitboard goes through HTTP and HTTPS proxies only. For its requests to go \
             out, set each of ALL_PROXY and HTTPS_PROXY to an HTTP proxy's address, such \
             as http://proxy.example.com:8080, or unset them."
        )
    );

    let every = network_read_from(&[("HTTPS_PROXY", "socks4://10.0.0.1"), ("no_proxy", "*")]);
    assert_eq!(
        (every.level, every.detail.as_str(), every.advice.as_str()),
        (
            Level::Ok,
            "direct: Pitboard does not use SOCKS proxies yet, and HTTPS_PROXY names one, \
             socks4://10.0.0.1:1080; no_proxy exempts every host Pitboard calls, which it \
             reaches directly",
            ""
        )
    );
}

/// Where Pitboard passes over a variable that holds no proxy's address, the check warns
/// and says what to set.
#[test]
fn a_variable_pitboard_cannot_read_is_a_warning() {
    let passed = network(None, &["HTTPS_PROXY"]);
    assert_eq!(passed.level, Level::Warn);
    assert_eq!(
        passed.detail,
        "direct: HTTPS_PROXY holds no proxy's address Pitboard can read, so it is passed \
         over"
    );
    assert_eq!(
        passed.advice,
        "Set it to a proxy's address, such as http://proxy.example.com:8080, or unset it."
    );
    let both = ProxyFact {
        variable: "HTTP_PROXY",
        address: "http://proxy.example.com:8080".into(),
        ..through(None, &[])
    };
    let passed = network(Some(both), &["ALL_PROXY", "HTTPS_PROXY"]);
    assert_eq!(passed.level, Level::Warn);
    assert_eq!(
        passed.detail,
        "through http://proxy.example.com:8080, from HTTP_PROXY; ALL_PROXY and HTTPS_PROXY \
         hold no proxy's address Pitboard can read, so they are passed over"
    );
    assert_eq!(
        passed.advice,
        "Set each to a proxy's address, such as http://proxy.example.com:8080, or unset \
         them."
    );
}

/// What doctor is given is read from the context, as the agent's proxy is, with the
/// user name and password in the address hidden, and the hosts exempt judged by ureq.
#[test]
fn the_network_is_read_from_the_context() {
    let env: crate::context::Environment = [
        ("HOME", "/nowhere"),
        ("HTTPS_PROXY", "http://alice:s3cret@proxy.example.com:3128"),
        ("NO_PROXY", ".anthropic.com,localhost"),
    ]
    .into_iter()
    .collect();
    let fact = network_fact(&Context::for_command_line(&env).proxy);
    let proxy = fact.proxy.expect("a proxy");
    assert_eq!(proxy.variable, "HTTPS_PROXY");
    assert_eq!(proxy.address, "http://***:***@proxy.example.com:3128");
    assert_eq!(proxy.server, "proxy.example.com:3128");
    assert!(!proxy.on_loopback && !proxy.socks);
    assert_eq!(proxy.exempting, Some("NO_PROXY"));
    assert_eq!(proxy.exempt, ["api.anthropic.com"]);
    assert!(fact.passed_over.is_empty());

    let unit = network_fact(&Context::for_unit_test().proxy);
    assert!(unit.proxy.is_none() && unit.passed_over.is_empty());

    for (address, shown, on_loopback) in [
        ("socks5://localhost:7890", "socks5://localhost:7890", true),
        ("socks4://127.0.0.1", "socks4://127.0.0.1:1080", true),
        ("socks5h://[::1]:1080", "socks5h://[::1]:1080", true),
        ("socks4a://10.0.0.1", "socks4a://10.0.0.1:1080", false),
        ("socks://p.example", "socks5://p.example:1080", false),
        (
            "https://Proxy.Example.com",
            "https://Proxy.Example.com:443",
            false,
        ),
    ] {
        let env: crate::context::Environment = [("HOME", "/nowhere"), ("ALL_PROXY", address)]
            .into_iter()
            .collect();
        let proxy = network_fact(&Context::for_command_line(&env).proxy)
            .proxy
            .expect("a proxy");
        assert_eq!(
            (proxy.address.as_str(), proxy.on_loopback, proxy.socks),
            (shown, on_loopback, shown.starts_with("socks")),
            "{address}"
        );
    }
}
