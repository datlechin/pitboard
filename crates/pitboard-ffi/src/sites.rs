// The sites' records, declared apart from pitboard-sites' own: the C# generator cannot use a
// type from another crate.

use pitboard_core::host::{OS, Os};

/// A website Pitboard opens in an account's window: whose accounts it serves, which hosts are
/// its own, where its sign-in goes, and what its window says about signing in.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct Site {
    /// The site's own host, in lower case: where its windows start and what links to it are
    /// opened on.
    pub host: String,
    /// The site as a sentence names it: its host, so a window on chatgpt.com is never
    /// confused with OpenAI's ChatGPT app.
    pub name: String,
    /// The tool whose enrolled accounts get this site's windows, as a `Tool`'s `code`.
    pub provider: String,
    /// Other hosts that send a browser to `host` with the path kept, taken as `host`. Each is
    /// matched exactly: a subdomain of one is not one.
    pub aliases: Vec<String>,
    /// Every host whose links the site opens: `host`, then `aliases`.
    pub hosts: Vec<String>,
    /// Hosts the site's own sign-in leaves for and comes back from. A window's page loads
    /// them as it loads the site, and one asked for in a new window opens in a sign-in window
    /// that shares the account's store. A link from outside to one opens nothing.
    pub sign_in_hosts: Vec<String>,
    /// Hosts a window refuses as its page or a new window, and never hands to the browser.
    pub blocked_hosts: Vec<String>,
    /// Paths that sign in whoever a link belongs to, as the lower-case segments they start
    /// with.
    pub sign_in_paths: Vec<Vec<String>>,
    /// Hashed into the store id of each of the site's windows. It never changes once a site
    /// has shipped: a change would leave every window of the site without its data.
    pub store_name: String,
    /// How to sign in to the site in a window, as the steps its sign-in page shows, and what
    /// of them a window on this system cannot do.
    pub sign_in_steps: String,
    /// What the blocked hosts leave unusable in the site's window.
    pub blocked_services: String,
}

impl From<&pitboard_sites::Site> for Site {
    fn from(site: &pitboard_sites::Site) -> Site {
        let owned = |hosts: &[&str]| hosts.iter().map(|&host| host.to_owned()).collect();
        Site {
            host: site.host.into(),
            name: site.name().into(),
            provider: site.provider.into(),
            aliases: owned(site.aliases),
            hosts: site.hosts().map(str::to_owned).collect(),
            sign_in_hosts: owned(site.sign_in_hosts),
            blocked_hosts: owned(site.blocked_hosts),
            sign_in_paths: site.sign_in_paths.iter().map(|path| owned(path)).collect(),
            store_name: site.store_name.into(),
            sign_in_steps: sign_in_steps(OS, site),
            blocked_services: site.blocked_services.into(),
        }
    }
}

fn sign_in_steps(os: Os, site: &pitboard_sites::Site) -> String {
    let passkeys = match os {
        // WebKit gives a page a passkey only in an app macOS lets act as a browser, and
        // Pitboard is not one.
        Os::MacOs => false,
        // No app runs on Linux, and nothing was measured there, so none is promised.
        Os::Linux => false,
        // Whether WebView2 gives a page in an app's window a passkey is the Windows app's to
        // measure, and until it has, none is promised.
        Os::Windows => false,
    };
    if site.passkeys && !passkeys {
        format!("{} Passkeys don’t work in this window.", site.sign_in_steps)
    } else {
        site.sign_in_steps.to_owned()
    }
}

/// A link one of the sites opens, checked: the site, and the address its window loads.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct SiteLink {
    pub site: Site,
    /// The link on the site's own host over `https`, with its path, query and fragment.
    pub url: String,
}

impl From<pitboard_sites::SiteLink> for SiteLink {
    fn from(link: pitboard_sites::SiteLink) -> SiteLink {
        SiteLink {
            site: link.site().into(),
            url: link.url().into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pitboard_core::provider::ProviderId;

    /// Each site serves the accounts of a tool the core knows, by the code every `provider`
    /// field names it with: pitboard-sites holds nothing of the core to check that itself.
    #[test]
    fn every_site_serves_a_tool_the_core_knows() {
        for site in pitboard_sites::ALL.map(Site::from) {
            let tool = ProviderId::parse(&site.provider).expect("a tool the core knows");
            assert_eq!(tool.code(), site.provider);
        }
        assert_eq!(Site::from(&pitboard_sites::CLAUDE).name, "claude.ai");
        assert_eq!(
            Site::from(&pitboard_sites::CHATGPT).hosts,
            [
                "chatgpt.com",
                "chat.openai.com",
                "www.chatgpt.com",
                "chat.com"
            ]
        );
    }

    // Whether a passkey works is the system's to say, so pitboard-sites' steps leave it out.
    #[test]
    fn a_window_says_a_passkey_does_not_work_where_it_does_not() {
        for os in [Os::MacOs, Os::Linux, Os::Windows] {
            assert_eq!(
                sign_in_steps(os, &pitboard_sites::CHATGPT),
                "Enter your email address, then its password or the code chatgpt.com emails \
                 you, or click Continue with Microsoft or Continue with Apple. Passkeys don’t \
                 work in this window."
            );
            assert_eq!(
                sign_in_steps(os, &pitboard_sites::CLAUDE),
                "Click Continue with email, open the email on your phone, tap its link, then \
                 enter here the code claude.ai shows there.",
                "claude.ai's steps say nothing of a passkey"
            );
        }
        assert_eq!(
            Site::from(&pitboard_sites::CHATGPT).sign_in_steps,
            sign_in_steps(OS, &pitboard_sites::CHATGPT)
        );
    }

    #[test]
    fn a_link_crosses_as_pitboard_sites_reads_it() {
        let link = pitboard_sites::SiteLink::parse(" chat.openai.com/c/x?y=1#z ").expect("a link");
        let link = SiteLink::from(link);
        assert_eq!(link.url, "https://chatgpt.com/c/x?y=1#z");
        assert_eq!(link.site, Site::from(&pitboard_sites::CHATGPT));
    }
}
