//! Each site as values. The windows, their navigation policy, the menus, the account picker
//! and the Share extension read these values and never name a site, so a site is added by
//! declaring it in [`ALL`], with its fixture page and tests.

/// A website Pitboard opens in an account's window: whose accounts it serves, which hosts are
/// its own, where its sign-in goes, and what its window says about signing in.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Site {
    /// The site's own host, in lower case: where its windows start and what links to it are
    /// opened on. It is also the site's name, so a window on chatgpt.com is never confused
    /// with OpenAI's ChatGPT app, which Pitboard quits around a Codex switch.
    pub host: &'static str,
    /// The tool whose enrolled accounts get this site's windows, by the code the core gives
    /// it: `claude`, `codex`.
    pub provider: &'static str,
    /// Other hosts that send a browser to `host` with the path kept, taken as `host`. Each is
    /// matched exactly: a subdomain of one is not one.
    pub aliases: &'static [&'static str],
    /// Hosts the site's own sign-in leaves for and comes back from. A window's page loads them
    /// as it loads the site, and one asked for in a new window opens in a sign-in window that
    /// shares the account's store. A link from outside to one opens nothing.
    pub sign_in_hosts: &'static [&'static str],
    /// Hosts a window refuses as its page or a new window, and never hands to the browser.
    /// Google blocks its sign-in and the connection of its apps inside apps, and in the browser
    /// either would sign in the browser rather than this account. A frame the page embeds is
    /// the page's own decision.
    pub blocked_hosts: &'static [&'static str],
    /// Paths that sign in whoever a link belongs to, as the lower-case segments they start
    /// with. Opened from outside, somebody else's would sign the window in as them, under the
    /// person's own label.
    pub sign_in_paths: &'static [&'static [&'static str]],
    /// Hashed into the store id of each of the site's windows. It never changes once a site
    /// has shipped: a change would leave every window of the site without its data, and the
    /// next sweep would delete that data. Kept apart from `provider` on purpose, so a tool
    /// renamed in the core cannot move it.
    pub store_name: &'static str,
    /// How to sign in to the site in a window, as the steps its sign-in page shows.
    pub sign_in_steps: &'static str,
    /// Whether the site's sign-in offers a passkey. A page in a window can use one only where
    /// the system's web view gives it one, which the bindings' sign-in steps then say.
    pub passkeys: bool,
    /// What the blocked hosts leave unusable in the site's window.
    pub blocked_services: &'static str,
}

/// Google's sign-in, which Google refuses inside apps.
const GOOGLE: &[&str] = &["accounts.google.com"];

// Its email sign-in never leaves claude.ai.
pub static CLAUDE: Site = Site {
    host: "claude.ai",
    provider: "claude",
    aliases: &[],
    sign_in_hosts: &[],
    blocked_hosts: GOOGLE,
    sign_in_paths: &[&["magic-link"]],
    store_name: "claude",
    sign_in_steps: "Click Continue with email, open the email on your phone, tap its link, then \
                    enter here the code claude.ai shows there.",
    passkeys: false,
    blocked_services: "Gmail, Google Drive, Google Calendar and Google single sign-on can’t be \
                       connected in this window.",
};

// docs/facts/services.md's "claude.ai and chatgpt.com" says where its hosts and aliases were read.
pub static CHATGPT: Site = Site {
    host: "chatgpt.com",
    provider: "codex",
    aliases: &["chat.openai.com", "www.chatgpt.com", "chat.com"],
    sign_in_hosts: &[
        "auth.openai.com",
        "login.live.com",
        "login.microsoftonline.com",
        "appleid.apple.com",
    ],
    blocked_hosts: GOOGLE,
    sign_in_paths: &[&["api", "auth"]],
    store_name: "codex",
    sign_in_steps: "Enter your email address, then its password or the code chatgpt.com emails \
                    you, or click Continue with Microsoft or Continue with Apple.",
    passkeys: true,
    blocked_services: "Google Drive, Gmail and Google Calendar can’t be connected in this \
                       window.",
};

/// Every site, in the order a listing shows them.
pub static ALL: [&Site; 2] = [&CLAUDE, &CHATGPT];

/// How a list of names is joined in a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conjunction {
    /// "claude.ai and chatgpt.com".
    And,
    /// "claude.ai or chatgpt.com".
    Or,
}

impl Site {
    /// The site as a sentence names it: its host.
    pub fn name(&self) -> &'static str {
        self.host
    }

    /// Every host whose links this site opens: its own, then its aliases.
    pub fn hosts(&self) -> impl Iterator<Item = &'static str> + use<> {
        std::iter::once(self.host).chain(self.aliases.iter().copied())
    }

    /// The sites whose windows the accounts of `provider` get, in the order a listing shows
    /// them.
    pub fn of_provider(provider: &str) -> impl Iterator<Item = &'static Site> + use<'_> {
        ALL.iter()
            .copied()
            .filter(move |site| site.provider == provider)
    }

    /// The site whose own host or alias `host` is, ignoring case. A subdomain of one is not
    /// one, and nor is a sign-in host.
    pub fn serving(host: &str) -> Option<&'static Site> {
        let host = host.to_lowercase();
        ALL.iter()
            .copied()
            .find(|site| site.hosts().any(|own| own == host))
    }

    /// The sites' names as a sentence lists them: "claude.ai and chatgpt.com".
    pub fn names(conjunction: Conjunction) -> String {
        listed(&ALL.map(Site::name), conjunction)
    }

    /// Whether `path`, percent-decoded, leads to one of the site's sign-in paths, which sign
    /// in whoever a link belongs to: a link from outside to one is refused, and a window is
    /// never opened on one again.
    ///
    /// Read by segment, as WebKit loads it: WebKit drops `.` and `..` segments, `%2e`
    /// included, before the request is sent, so they are resolved here. Empty segments are
    /// skipped, so `//magic-link`, which a server merging slashes would route to sign-in, is
    /// one too. Compared in lower case.
    pub fn signs_in(&self, path: &str) -> bool {
        let path = path.to_lowercase();
        let mut resolved: Vec<&str> = Vec::new();
        for segment in path.split('/').filter(|segment| !segment.is_empty()) {
            match segment {
                "." => {}
                ".." => {
                    resolved.pop();
                }
                _ => resolved.push(segment),
            }
        }
        self.sign_in_paths.iter().any(|prefix| {
            resolved.len() >= prefix.len()
                && resolved.iter().zip(prefix.iter()).all(|(a, b)| a == b)
        })
    }
}

/// `names` as an English sentence lists them, as Foundation's list format does in English:
/// "a", "a and b", "a, b, and c".
pub fn listed(names: &[&str], conjunction: Conjunction) -> String {
    let word = match conjunction {
        Conjunction::And => "and",
        Conjunction::Or => "or",
    };
    match names {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [first, second] => format!("{first} {word} {second}"),
        [rest @ .., last] => format!("{}, {word} {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_site_serves_one_tool() {
        assert_eq!(
            ALL,
            [&CLAUDE, &CHATGPT],
            "in the order a listing shows them"
        );
        assert_eq!(CLAUDE.provider, "claude");
        assert_eq!(
            CHATGPT.provider, "codex",
            "a Codex login is a ChatGPT sign-in"
        );
        assert_eq!(Site::of_provider("claude").collect::<Vec<_>>(), [&CLAUDE]);
        assert_eq!(Site::of_provider("codex").collect::<Vec<_>>(), [&CHATGPT]);
        assert_eq!(Site::of_provider("gemini").count(), 0);
        assert_eq!(CLAUDE.name(), "claude.ai");
        assert_eq!(
            CHATGPT.name(),
            "chatgpt.com",
            "never ChatGPT, which is OpenAI's app"
        );
    }

    /// A sign-in path is read by segment, in any case, with dot segments resolved and empty
    /// ones skipped, and a path that only starts like one, or holds one further down, is not
    /// one.
    #[test]
    fn a_sign_in_path_is_read_by_segment() {
        for path in [
            "/magic-link",
            "/magic-link/x",
            "/MAGIC-LINK",
            "//magic-link",
            "/./magic-link",
            "/x/../magic-link",
        ] {
            assert!(CLAUDE.signs_in(path), "{path}");
        }
        for path in [
            "/",
            "",
            "/magic-links-guide",
            "/chat/magic-link",
            "/magic-link/..",
        ] {
            assert!(!CLAUDE.signs_in(path), "{path}");
        }
        assert!(CHATGPT.signs_in("/api/auth/callback/openai"));
        assert!(CHATGPT.signs_in("/api/auth"));
        assert!(!CHATGPT.signs_in("/api"));
        assert!(!CHATGPT.signs_in("/auth/login"));
        assert!(
            !CHATGPT.signs_in("/magic-link"),
            "claude.ai's, not chatgpt.com's"
        );
    }

    /// Hashed into every window's store id: a change would leave every window without its
    /// data.
    #[test]
    fn the_store_names_never_change() {
        assert_eq!(CLAUDE.store_name, "claude");
        assert_eq!(CHATGPT.store_name, "codex");
    }

    #[test]
    fn a_site_is_found_by_any_of_its_hosts() {
        assert_eq!(Site::serving("claude.ai"), Some(&CLAUDE));
        assert_eq!(Site::serving("CLAUDE.AI"), Some(&CLAUDE));
        assert_eq!(Site::serving("chat.openai.com"), Some(&CHATGPT));
        assert_eq!(
            Site::serving("auth.openai.com"),
            None,
            "a sign-in host is no site"
        );
        assert_eq!(Site::serving("sub.claude.ai"), None);
        assert_eq!(
            CHATGPT.hosts().collect::<Vec<_>>(),
            [
                "chatgpt.com",
                "chat.openai.com",
                "www.chatgpt.com",
                "chat.com"
            ]
        );
    }

    /// Google refuses its pages inside apps, whichever site leads to them.
    #[test]
    fn every_site_blocks_googles_sign_in() {
        for site in ALL {
            assert!(
                site.blocked_hosts.contains(&"accounts.google.com"),
                "{}",
                site.name()
            );
            assert!(
                site.sign_in_hosts
                    .iter()
                    .all(|host| !site.blocked_hosts.contains(host))
            );
        }
    }

    /// Every host is written as it is compared: in lower case.
    #[test]
    fn every_host_is_in_lower_case() {
        for site in ALL {
            let hosts = site
                .hosts()
                .chain(site.sign_in_hosts.iter().copied())
                .chain(site.blocked_hosts.iter().copied());
            for host in hosts {
                assert_eq!(host, host.to_lowercase());
            }
        }
    }

    #[test]
    fn the_sites_are_named_in_a_sentence() {
        assert_eq!(Site::names(Conjunction::And), "claude.ai and chatgpt.com");
        assert_eq!(Site::names(Conjunction::Or), "claude.ai or chatgpt.com");
    }

    /// As Foundation's list format joins names in English, measured on macOS 27 with
    /// `formatted(.list(type:))` in the en_US locale.
    #[test]
    fn a_list_is_joined_as_english_joins_one() {
        assert_eq!(listed(&[], Conjunction::And), "");
        assert_eq!(listed(&["a"], Conjunction::And), "a");
        assert_eq!(listed(&["a", "b"], Conjunction::Or), "a or b");
        assert_eq!(listed(&["a", "b", "c"], Conjunction::And), "a, b, and c");
        assert_eq!(listed(&["a", "b", "c"], Conjunction::Or), "a, b, or c");
    }
}
