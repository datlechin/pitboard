use super::*;

#[test]
fn a_claude_desktop_login_is_redacted_from_a_report() {
    use crate::state::{Aside, SetAside};
    let uuid = "6f1c0a52-1111-4a6e-9d0b-2f6c1a9e7b10";
    let kept = format!("pitboard-park-desktop_{uuid}-1");
    let mut state = State::default();
    state.in_use.insert(
        "desktop".into(),
        crate::in_use::InUse {
            owner: None,
            login: "0123456789abcdef".into(),
            known_at: NOW,
            named: None,
        },
    );
    state.aside.push(Aside {
        name: kept.clone(),
        tool: ProviderId::Desktop,
        at: NOW,
        why: SetAside::Orphan,
        account: None,
        login: Some("fedcba9876543210".into()),
    });
    let mut f = facts();
    f.state = Ok(state);
    let ctx = Context::new(PathBuf::from("/home/x"));
    let text = format!("read 0123456789abcdef in use; {kept} holds fedcba9876543210");
    let hidden = redaction_for(&ctx, &f).over(&text);
    for secret in ["0123456789abcdef", "fedcba9876543210", kept.as_str(), uuid] {
        assert!(text.contains(secret), "{text}");
        assert!(!hidden.contains(secret), "{hidden}");
    }
}

/// The report a person pastes shows a digest in place of the proxy's host and port,
/// which can name the company whose network it is, as it does an organisation's name.
/// A proxy on loopback names nothing beyond this machine, and is shown. The report a
/// person reads on their own machine hides neither.
/// Claude Code's Windows build names its Credential Manager item with a literal, as its
/// macOS build does where no login name passes its screen. A report hides a person's login
/// name there, and shows the literal, which names nobody.
#[test]
fn the_literal_claude_code_names_its_item_with_is_shown_in_a_report() {
    let ctx = Context::new(PathBuf::from("/home/x"));
    let mut f = facts();
    f.account = "claude-code-user".into();
    let detail = "Credential Manager  ·  Claude Code-credentials-0123abcd/claude-code-user";
    assert_eq!(redaction_for(&ctx, &f).over(detail), detail);
    f.account = "dana".into();
    let hidden = redaction_for(&ctx, &f).over("Claude Code-credentials, account dana");
    assert!(!hidden.contains("dana"), "{hidden}");
}

#[test]
fn a_proxys_host_is_redacted_from_a_report_unless_it_is_on_loopback() {
    let ctx = Context::new(PathBuf::from("/home/x"));
    let mut f = facts();
    f.network.proxy = Some(through(None, &[]));
    let detail = check(&evaluate(&f), "network").detail.clone();
    let hidden = redaction_for(&ctx, &f).over(&detail);
    assert!(detail.contains("proxy.example.com:3128"), "{detail}");
    assert!(!hidden.contains("proxy.example.com"), "{hidden}");
    assert!(!hidden.contains("3128"), "{hidden}");
    assert!(
        hidden.starts_with("through http://***:***@<proxy ") && hidden.contains("from HTTPS_PROXY"),
        "{hidden}"
    );

    f.network.proxy = Some(ProxyFact {
        address: "http://127.0.0.1:7890".into(),
        server: "127.0.0.1:7890".into(),
        on_loopback: true,
        ..through(None, &[])
    });
    let detail = check(&evaluate(&f), "network").detail.clone();
    assert_eq!(redaction_for(&ctx, &f).over(&detail), detail);
}

/// What a pasted report must not carry, now that it can carry a Codex login too.
#[test]
fn a_codex_login_is_redacted_from_a_report() {
    let mut f = facts();
    f.codex = with_codex();
    let ctx = Context::new(PathBuf::from("/home/x"));
    let sheet = redaction_for(&ctx, &f);
    let login = check(&evaluate(&f), "codex_login").detail.clone();
    let hidden = sheet.over(&login);
    for secret in ["w@example.com", "work-account", "0123456789abcdef"] {
        assert!(login.contains(secret), "{login}");
        assert!(!hidden.contains(secret), "{hidden}");
    }
}

#[test]
fn windows_proxy_setting_is_redacted_from_a_report() {
    let ctx = Context::new(PathBuf::from("/home/x"));
    let mut f = facts();
    f.os = Os::Windows;
    for (server, hidden) in [
        ("proxy.example.com:8080", &["proxy.example.com:8080"][..]),
        (
            "http=web.example.com:80;https=secure.example.com:443",
            &["web.example.com:80", "secure.example.com:443"][..],
        ),
        (
            "http=alice:p=ss@proxy.example.com:8080",
            &["proxy.example.com:8080"][..],
        ),
    ] {
        f.windows_settings = Some(crate::host::WindowsSettings {
            proxy: Some(server.into()),
            ..crate::host::WindowsSettings::default()
        });
        let detail = check(&evaluate(&f), "system_proxy").detail.clone();
        let shown = redaction_for(&ctx, &f).over(&detail);
        for part in hidden {
            assert!(detail.contains(part), "{detail}");
            assert!(!shown.contains(part), "{shown}");
        }
        assert!(shown.contains("<proxy "), "{shown}");
    }
}
