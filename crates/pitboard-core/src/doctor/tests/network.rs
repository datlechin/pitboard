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
        (Level::Ok, "network", words::DIRECT)
    );
    let both = [
        "api.anthropic.com".to_string(),
        "platform.claude.com".into(),
    ];
    for (exempting, exempt, said) in [
        (None, &[][..], String::new()),
        (
            Some("NO_PROXY"),
            &[],
            format!("; {}", words::exempts_none("NO_PROXY")),
        ),
        (
            Some("no_proxy"),
            &["api.anthropic.com"],
            format!(
                "; {}",
                words::exempts("no_proxy", &["api.anthropic.com".into()])
            ),
        ),
        (
            Some("NO_PROXY"),
            &["api.anthropic.com", "platform.claude.com"],
            format!("; {}", words::exempts("NO_PROXY", &both)),
        ),
        (
            Some("NO_PROXY"),
            &[
                "api.anthropic.com",
                "platform.claude.com",
                "auth.openai.com",
                "chatgpt.com",
            ],
            format!("; {}", words::exempts_every_host("NO_PROXY")),
        ),
    ] {
        let proxy = through(exempting, exempt);
        let detail = words::through(&proxy.address, proxy.variable) + &said;
        let check = network(Some(proxy), &[]);
        assert_eq!(check.level, Level::Ok, "{said}");
        assert_eq!(check.detail, detail);
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
        (socks.level, socks.detail, socks.advice),
        (
            Level::Warn,
            words::socks(true, "all_proxy", "socks5://127.0.0.1:7890"),
            words::http_proxies_only("all_proxy", "it")
        )
    );

    let some = network_read_from(&[
        ("ALL_PROXY", "socks5h://alice:s3cret@p.example"),
        ("NO_PROXY", "chatgpt.com"),
    ]);
    assert_eq!(
        (some.level, some.detail),
        (
            Level::Warn,
            format!(
                "{}; {}",
                words::socks(true, "ALL_PROXY", "socks5h://***:***@p.example:1080"),
                words::exempts("NO_PROXY", &["chatgpt.com".into()])
            )
        )
    );

    let passed = network_read_from(&[
        ("ALL_PROXY", "not a proxy"),
        ("HTTPS_PROXY", "socks://p.example"),
    ]);
    assert_eq!(
        (passed.level, passed.detail, passed.advice),
        (
            Level::Warn,
            format!(
                "{}; {}",
                words::socks(true, "HTTPS_PROXY", "socks5://p.example:1080"),
                words::passed_over(&["ALL_PROXY"])
            ),
            words::http_proxies_only(
                &words::each_of(&["ALL_PROXY".into(), "HTTPS_PROXY".into()]),
                "them"
            )
        )
    );

    let every = network_read_from(&[("HTTPS_PROXY", "socks4://10.0.0.1"), ("no_proxy", "*")]);
    assert_eq!(
        (every.level, every.detail, every.advice),
        (
            Level::Ok,
            format!(
                "{}; {}",
                words::socks(false, "HTTPS_PROXY", "socks4://10.0.0.1:1080"),
                words::exempts_every_host("no_proxy")
            ),
            String::new()
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
        words::direct_passing_over(&words::passed_over(&["HTTPS_PROXY"]))
    );
    assert_eq!(passed.advice, words::SET_IT_RIGHT);
    let both = ProxyFact {
        variable: "HTTP_PROXY",
        address: "http://proxy.example.com:8080".into(),
        ..through(None, &[])
    };
    let passed = network(Some(both), &["ALL_PROXY", "HTTPS_PROXY"]);
    assert_eq!(passed.level, Level::Warn);
    assert_eq!(
        passed.detail,
        format!(
            "{}; {}",
            words::through("http://proxy.example.com:8080", "HTTP_PROXY"),
            words::passed_over(&["ALL_PROXY", "HTTPS_PROXY"])
        )
    );
    assert_eq!(passed.advice, words::SET_EACH_RIGHT);
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
