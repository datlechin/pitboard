use super::Facts;
use crate::context::Context;
use crate::provider::ProviderId;
use crate::provider::claude::slot;
use crate::proxy::system_proxies;

// Salted with this machine and moment: names line up inside one report, never across two.
pub(super) fn redaction_for(ctx: &Context, facts: &Facts) -> crate::redact::Sheet {
    let mut sheet = crate::redact::Sheet::of(
        ctx,
        format!("{}:{}", crate::state::machine_id(), ctx.now_millis()),
    );
    if facts.account != slot::FALLBACK_ACCOUNT {
        sheet = sheet.hide(facts.account.clone(), "user");
    }

    if let Some(id) = &facts.config_account {
        sheet = sheet
            .hide(id.email.clone(), "email")
            .hide(id.account_uuid.clone(), "account")
            .hide(id.organization_uuid.clone(), "org");
        if let Some(name) = &id.organization_name {
            sheet = sheet.hide(name.clone(), "org");
        }
    }
    // Neither need be an account anybody enrolled.
    let named = facts
        .in_use
        .iter()
        .flat_map(|known| known.owner().into_iter().chain(&known.own_record));
    for owner in named {
        sheet = sheet
            .hide(owner.email.clone(), "email")
            .hide(owner.account_uuid.clone(), "account")
            .hide(owner.organization_uuid.clone(), "org");
    }
    if let Ok(state) = &facts.state {
        for account in &state.accounts {
            sheet = sheet
                .hide(account.email.clone(), "email")
                .hide(account.account_uuid.clone(), "account")
                .hide(
                    account
                        .claude()
                        .map_or_else(String::new, |c| c.organization_uuid.to_string()),
                    "org",
                );
            // A Codex account's workspace names the organisation it belongs to.
            if let crate::state::Detail::Codex {
                workspace_id: Some(workspace),
                ..
            } = &account.detail
            {
                sheet = sheet.hide(workspace.clone(), "org");
            }
        }
    }
    // Codex's live login, which need not be an account anybody enrolled.
    if let Ok(Some(login)) = &facts.codex.login {
        sheet = sheet
            .hide(login.email.clone(), "email")
            .hide(login.account_id.clone(), "account")
            .hide(login.fingerprint.clone(), "login");
    }
    // A fingerprint is not a token, and it still identifies one login across reports.
    if let Ok(Some(doc)) = &facts.credential
        && let Some(fingerprint) = doc
            .get("claudeAiOauth")
            .map(crate::provider::claude::document::fingerprint_of)
            .filter(|f| !f.is_empty())
    {
        sheet = sheet.hide(fingerprint, "login");
    }
    if let Some(fingerprint) = facts
        .fallback_login
        .as_ref()
        .and_then(|left| left.fingerprint.as_ref().ok().cloned().flatten())
        .filter(|fingerprint| !fingerprint.is_empty())
    {
        sheet = sheet.hide(fingerprint, "login");
    }
    for park in &facts.parks {
        if let Some(held) = &park.park {
            sheet = sheet
                .hide(held.login.clone(), "login")
                .hide(held.service.clone(), "park");
        }
    }
    // Claude Desktop's session as last read, and each folder set aside with its session.
    if let Ok(state) = &facts.state {
        if let Some(record) = state.in_use(ProviderId::Desktop) {
            sheet = sheet.hide(record.login.clone(), "login");
        }
        for aside in &state.aside {
            sheet = sheet
                .hide(aside.name.clone(), "park")
                .hide(aside.login.clone().unwrap_or_default(), "login");
        }
    }
    // Windows' proxy names a network as a variable's does: `host:port`, or one per protocol.
    let system_proxy = facts
        .windows_settings
        .as_ref()
        .and_then(|settings| settings.proxy.as_deref());
    for entry in system_proxy.map(system_proxies).unwrap_or_default() {
        sheet = sheet.hide(entry.server.to_owned(), "proxy");
    }
    // With its port, so a host that is also a word is hidden only where it is the host.
    let schedules = facts
        .schedule
        .as_ref()
        .and_then(|s| s.network.proxy.as_ref());
    for proxy in facts.network.proxy.iter().chain(schedules) {
        if !proxy.on_loopback {
            sheet = sheet.hide(proxy.server.clone(), "proxy");
        }
    }
    sheet
}
