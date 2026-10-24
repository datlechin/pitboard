//! Where an account window's pages go: the site and its sign-in stay in the window, Google's
//! sign-in is refused, other web pages go to the browser, and nothing else leaves.
//!
//! Pure functions of what the web view says, so every rule is tested without starting one.
//! Each app turns its engine's callbacks into a `NavigationRequest`, and does what the
//! decision says.

use super::notes::WindowNoteKind;
use crate::{Site, SiteLink};
use pitboard_sites::WebAddress;

/// The rules of `site`'s windows, whose pages are on `scheme`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct NavigationPolicy {
    pub site: Site,
    /// The scheme every page the window keeps is on: `https`, and a fixture's own in a
    /// fixture, where each stand-in site keeps its host on that scheme.
    pub scheme: String,
}

/// Which page asks: an account window's own, or a sign-in window one of its pages opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum PageRole {
    Window,
    Popup,
}

/// Where a navigation would load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum NavigationTarget {
    /// The page itself.
    Page,
    /// A frame inside it.
    Frame,
    /// A new window.
    NewWindow,
}

/// Which page asked for a navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum Asker {
    /// The site's own page, as the window's main page.
    Site,
    /// A page of the site's sign-in, as the window's main page, or a sign-in window's own
    /// page.
    SignIn,
    /// Anything else: a frame inside the page, such as an artifact's, another window's page,
    /// or a load the app started itself.
    Other,
}

/// A navigation as the policy sees it, from what the web view says about it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct NavigationRequest {
    /// The address asked for, empty for a window asked for with none yet.
    pub url: String,
    pub target: NavigationTarget,
    /// A person clicked a link or submitted a form.
    pub clicked: bool,
    /// The web view says to download what this asks for.
    pub download: bool,
    pub asker: Asker,
}

/// What becomes of a navigation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum NavigationDecision {
    /// Loaded where it was asked for.
    Load,
    /// A new window's page, loaded in the page that asked instead: a window per account, so a
    /// link of the site's that asks for a tab stays signed in as this account.
    LoadInPage,
    /// A new window for the site's sign-in, sharing the account's store, so what it signs in
    /// is this account's. A page that opens a blank window and gives it an address afterwards
    /// gets one too, and the sign-in window's own policy decides where that address goes.
    Popup,
    /// Saved to the downloads folder; `ask` asks the person first, as a browser asks before a
    /// website it does not know downloads.
    Download { ask: bool },
    /// Handed to the system: the default browser for a web page, the default email app for an
    /// address. `url` is the request's own. The window then says `note`, when there is one.
    OpenElsewhere {
        url: String,
        note: Option<WindowNoteKind>,
    },
    /// Stopped, and the window says `note`.
    Refuse { note: WindowNoteKind },
    /// Stopped, and nothing is said.
    Ignore,
}

/// What the web view says about a response a page received.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct ResponseFacts {
    /// The response's address, where it has one.
    pub url: Option<String>,
    /// Whether it answers the page itself rather than a frame inside it.
    pub main_frame: bool,
    /// Whether the web view can show what it holds.
    pub can_show: bool,
    /// Its `Content-Disposition` header, where it has one.
    pub disposition: Option<String>,
}

/// What becomes of a response a page received.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum ResponseDecision {
    /// Shown in the page.
    Show,
    /// Saved to the downloads folder; `ask` asks the person first.
    Download { ask: bool },
    /// Stopped: a sign-in window saves nothing.
    Ignore,
}

/// The frame that asked for a navigation, as the web view describes its origin.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct FrameOrigin {
    /// Whether it is the page's main frame.
    pub main_frame: bool,
    pub scheme: String,
    /// Empty for an origin with none, such as a sandboxed frame's.
    pub host: String,
    /// `0` for the scheme's own.
    pub port: i64,
}

impl NavigationPolicy {
    /// What becomes of `request`, asked by a page in `role`. The first rule that matches
    /// decides.
    fn decide(&self, role: PageRole, request: &NavigationRequest) -> NavigationDecision {
        use NavigationDecision::{Download, Ignore, Load, LoadInPage};
        let address = WebAddress::parse(&request.url);
        let scheme = address.scheme();
        // Saving a file does not move the page. Artifact downloads are often `blob:` links,
        // which name no host. Nothing plain `http`, local or unknown is saved, and a sign-in
        // window saves nothing. Only the site's own page saves without asking: an artifact
        // runs somebody else's code in a frame of its own.
        if request.download {
            if role != PageRole::Window || !["https", "blob", "data"].contains(&scheme) {
                return Ignore;
            }
            return Download {
                ask: request.asker != Asker::Site,
            };
        }
        // The page decides what it embeds: artifacts render in frames of their own origin.
        // What decides is where the page goes. A web page never opens a local file.
        if request.target == NavigationTarget::Frame {
            return if scheme == "file" { Ignore } else { Load };
        }
        if role == PageRole::Popup {
            return self.decide_in_popup(request, &address);
        }
        // A blank page, or a window asked for with no address yet, which a page fills in
        // after opening it. In a sign-in window the address it is given is decided again.
        if request.url.is_empty() || scheme == "about" {
            return Self::sign_in_window(request);
        }
        if self.is_site(&address) {
            return if request.target == NavigationTarget::Page {
                Load
            } else {
                LoadInPage
            };
        }
        if self.on(&address, &self.site.sign_in_hosts) {
            return Self::sign_in_window(request);
        }
        self.leaving(request, &address)
    }

    /// A page of the site's sign-in, or a blank one: loaded as the window's page, and asked
    /// for in a new window, opened in a sign-in window. Only the site's own page and its
    /// sign-in get one: a frame given a window could fill it with a page of its own, which
    /// nothing on the window's title would tell apart.
    fn sign_in_window(request: &NavigationRequest) -> NavigationDecision {
        match (request.target, request.asker) {
            (NavigationTarget::Page, _) => NavigationDecision::Load,
            (_, Asker::Other) => NavigationDecision::Ignore,
            _ => NavigationDecision::Popup,
        }
    }

    /// A sign-in window loads the site and its sign-in pages, saves nothing and opens no
    /// window of its own; anything else goes where it would from the window. Its page may go
    /// blank itself, and nothing else may make it: a frame that opened it could then fill it
    /// with a page of its own.
    fn decide_in_popup(
        &self,
        request: &NavigationRequest,
        address: &WebAddress,
    ) -> NavigationDecision {
        let own = request.target == NavigationTarget::Page;
        if request.url.is_empty() || address.scheme() == "about" {
            return if own && request.asker != Asker::Other {
                NavigationDecision::Load
            } else {
                NavigationDecision::Ignore
            };
        }
        if self.is_site(address) || self.on(address, &self.site.sign_in_hosts) {
            return if own {
                NavigationDecision::Load
            } else {
                NavigationDecision::Ignore
            };
        }
        self.leaving(request, address)
    }

    /// Where a request goes that leaves the window's own hosts. Google's sign-in is refused,
    /// since Google blocks it inside apps and in the browser it would sign in the browser.
    /// Any other web page goes to the browser, and an address a person clicked, or one asked
    /// for in a new window, to the default email app, as a browser does. A web page never
    /// launches another app through Pitboard; one a person clicked is said to be refused.
    ///
    /// A web page the site sent to the browser without a click is usually a sign-in to
    /// connect something, which the browser connects to whichever account of the site it is
    /// signed in to, so the window says so.
    fn leaving(&self, request: &NavigationRequest, address: &WebAddress) -> NavigationDecision {
        let scheme = address.scheme();
        if scheme == "http" || scheme == "https" {
            let blocked = address
                .host()
                .is_some_and(|host| self.site.blocked_hosts.contains(&host.to_lowercase()));
            if blocked {
                return NavigationDecision::Refuse {
                    note: WindowNoteKind::GoogleRefused,
                };
            }
            return NavigationDecision::OpenElsewhere {
                url: request.url.clone(),
                note: (!request.clicked).then_some(WindowNoteKind::OpenedInBrowser),
            };
        }
        let asked = request.clicked || request.target == NavigationTarget::NewWindow;
        if scheme == "mailto" {
            return if asked {
                NavigationDecision::OpenElsewhere {
                    url: request.url.clone(),
                    note: None,
                }
            } else {
                NavigationDecision::Ignore
            };
        }
        if asked && !scheme.is_empty() {
            NavigationDecision::Refuse {
                note: WindowNoteKind::OtherApp {
                    scheme: scheme.to_owned(),
                },
            }
        } else {
            NavigationDecision::Ignore
        }
    }

    /// Whether `address` is one of the site's own pages: on the window's scheme, on the
    /// site's host or an alias of it, with no port and no user to hide the host behind.
    fn is_site(&self, address: &WebAddress) -> bool {
        self.on(address, &self.site.hosts)
    }

    fn on(&self, address: &WebAddress, hosts: &[String]) -> bool {
        address.scheme().eq_ignore_ascii_case(&self.scheme)
            && address
                .host()
                .is_some_and(|host| hosts.contains(&host.to_lowercase()))
            && address.port().is_none()
            && !address.names_user()
    }
}

/// What becomes of `request`, asked by a page in `role` of a window `policy` rules. The first
/// rule that matches decides.
///
/// A new window is decided once, where the web view asks for the page to put in it. WebKit
/// first asks about a link that asks for one as a navigation, and refusing it there stops
/// WebKit asking for the window at all, so the app lets that through.
#[uniffi::export]
pub fn decide_navigation(
    policy: NavigationPolicy,
    role: PageRole,
    request: NavigationRequest,
) -> NavigationDecision {
    policy.decide(role, &request)
}

/// What becomes of a response a page in `role` received: downloaded when the web view cannot
/// show it or the server says it is an attachment, and shown otherwise. Only a response to
/// the window's own page on the site saves without asking, and a sign-in window saves nothing.
#[uniffi::export]
pub fn decide_response(
    policy: NavigationPolicy,
    role: PageRole,
    response: ResponseFacts,
) -> ResponseDecision {
    // Trimmed as Foundation's `CharacterSet.whitespaces` trims: a tab, Unicode's spaces and
    // U+200B, measured on macOS 27 over every scalar. Not a newline.
    let blank = |c: char| {
        matches!(
            c,
            '\t' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
                ..='\u{200b}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
        )
    };
    let kind = response
        .disposition
        .as_deref()
        .map(|disposition| disposition.trim_matches(blank).to_lowercase())
        .unwrap_or_default();
    if response.can_show && !kind.starts_with("attachment") {
        return ResponseDecision::Show;
    }
    match role {
        PageRole::Popup => ResponseDecision::Ignore,
        PageRole::Window => {
            let from_site = response.main_frame
                && response
                    .url
                    .as_deref()
                    .is_some_and(|url| policy.is_site(&WebAddress::parse(url)));
            ResponseDecision::Download { ask: !from_site }
        }
    }
}

pub(crate) fn is_site_page(policy: &NavigationPolicy, url: &str) -> bool {
    policy.is_site(&WebAddress::parse(url))
}

/// Which page `frame` is, as an account window's policy tells pages apart: its main page on
/// the site, its main page on the site's sign-in, or anything else.
#[uniffi::export]
pub fn frame_asker(policy: NavigationPolicy, frame: FrameOrigin) -> Asker {
    if !frame.main_frame || !frame.scheme.eq_ignore_ascii_case(&policy.scheme) || frame.port != 0 {
        return Asker::Other;
    }
    let host = frame.host.to_lowercase();
    if policy.site.hosts.contains(&host) {
        Asker::Site
    } else if policy.site.sign_in_hosts.contains(&host) {
        Asker::SignIn
    } else {
        Asker::Other
    }
}

/// Where a window ruled by `policy` starts: the site's home, on the window's scheme.
#[uniffi::export]
pub fn window_home(policy: NavigationPolicy) -> String {
    format!("{}://{}/", policy.scheme, policy.site.host)
}

// On the window's scheme, so a fixture's window never reaches the network.
pub(crate) fn window_address(policy: &NavigationPolicy, link: &SiteLink) -> String {
    match link.url.split_once(':') {
        Some((_, rest)) => format!("{}:{rest}", policy.scheme),
        None => window_home(policy.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pitboard_sites::{CHATGPT, CLAUDE};

    fn policy(site: &pitboard_sites::Site, scheme: &str) -> NavigationPolicy {
        NavigationPolicy {
            site: site.into(),
            scheme: scheme.into(),
        }
    }

    fn chatgpt() -> NavigationPolicy {
        policy(&CHATGPT, "https")
    }

    fn claude() -> NavigationPolicy {
        policy(&CLAUDE, "https")
    }

    /// A request for `url`, as the Swift tests made them: asked for the page, by a page that
    /// is not the site's, without a click.
    fn request(url: &str, target: NavigationTarget) -> NavigationRequest {
        NavigationRequest {
            url: url.into(),
            target,
            clicked: false,
            download: false,
            asker: Asker::Other,
        }
    }

    fn page(url: &str) -> NavigationRequest {
        request(url, NavigationTarget::Page)
    }

    fn new_window(url: &str) -> NavigationRequest {
        request(url, NavigationTarget::NewWindow)
    }

    fn frame(url: &str) -> NavigationRequest {
        request(url, NavigationTarget::Frame)
    }

    fn by(asker: Asker, request: NavigationRequest) -> NavigationRequest {
        NavigationRequest { asker, ..request }
    }

    fn clicked(request: NavigationRequest) -> NavigationRequest {
        NavigationRequest {
            clicked: true,
            ..request
        }
    }

    fn download(request: NavigationRequest) -> NavigationRequest {
        NavigationRequest {
            download: true,
            ..request
        }
    }

    /// A window asked for with no address yet, as WebKit hands `window.open('')` over: an
    /// empty address, measured on macOS 27.
    fn empty_window(asker: Asker) -> NavigationRequest {
        by(asker, new_window(""))
    }

    fn in_window(policy: &NavigationPolicy, request: NavigationRequest) -> NavigationDecision {
        decide_navigation(policy.clone(), PageRole::Window, request)
    }

    fn in_popup(policy: &NavigationPolicy, request: NavigationRequest) -> NavigationDecision {
        decide_navigation(policy.clone(), PageRole::Popup, request)
    }

    /// Handed to the browser, as a page the person clicked, or one the site sent without a
    /// click, which the window then says.
    fn elsewhere(url: &str, clicked: bool) -> NavigationDecision {
        NavigationDecision::OpenElsewhere {
            url: url.into(),
            note: (!clicked).then_some(WindowNoteKind::OpenedInBrowser),
        }
    }

    use NavigationDecision::{Download, Ignore, Load, LoadInPage, Popup, Refuse};

    // An account window's own page.

    #[test]
    fn the_site_loads_in_its_window() {
        assert_eq!(
            in_window(&chatgpt(), page("https://chatgpt.com/c/abc")),
            Load
        );
        assert_eq!(in_window(&chatgpt(), page("https://CHATGPT.com/")), Load);
        assert_eq!(
            in_window(&chatgpt(), page("https://chat.openai.com/c/abc")),
            Load,
            "an alias redirects to the site with the path kept"
        );
        assert_eq!(in_window(&claude(), page("https://claude.ai/new")), Load);
    }

    /// A window per account: a link of the site's that asks for a tab stays in this window,
    /// so it stays signed in as this account, and Back returns to where the person was.
    #[test]
    fn a_site_link_asking_for_a_new_window_loads_in_the_page() {
        assert_eq!(
            in_window(&chatgpt(), new_window("https://chatgpt.com/c/abc")),
            LoadInPage
        );
    }

    /// The site's sign-in leaves the site: it loads in the window, and asked for in a new
    /// window it opens a sign-in window sharing the account's store.
    #[test]
    fn the_sites_sign_in_loads_or_opens_a_sign_in_window() {
        let auth = "https://auth.openai.com/authorize?client_id=x";
        assert_eq!(in_window(&chatgpt(), page(auth)), Load);
        assert_eq!(
            in_window(&chatgpt(), by(Asker::Site, new_window(auth))),
            Popup
        );
        assert_eq!(
            in_window(
                &chatgpt(),
                by(Asker::Site, new_window("https://login.live.com/oauth"))
            ),
            Popup
        );
        assert_eq!(
            in_window(&claude(), new_window(auth)),
            elsewhere(auth, false),
            "another site's sign-in is not this one's"
        );
    }

    /// Measured on macOS 27: `window.open('')` asks for a window with an empty address and
    /// gives it one afterwards, which the sign-in window's own policy decides.
    #[test]
    fn a_blank_window_opens_as_a_sign_in_window() {
        assert_eq!(in_window(&chatgpt(), empty_window(Asker::Site)), Popup);
        assert_eq!(in_window(&chatgpt(), empty_window(Asker::SignIn)), Popup);
        assert_eq!(
            in_window(&chatgpt(), by(Asker::Site, new_window("about:blank"))),
            Popup
        );
        assert_eq!(in_window(&chatgpt(), page("about:blank")), Load);
        assert_eq!(in_window(&chatgpt(), frame("about:srcdoc")), Load);
    }

    /// The page decides what it embeds: artifacts render in frames of their own origin. A
    /// web page never opens a local file.
    #[test]
    fn frames_load_what_the_page_embeds() {
        assert_eq!(in_window(&chatgpt(), frame("https://cdn.example/x")), Load);
        assert_eq!(
            in_window(&claude(), frame("https://www.claudeusercontent.com/")),
            Load
        );
        assert_eq!(in_window(&chatgpt(), frame("file:///etc/hosts")), Ignore);
    }

    #[test]
    fn other_web_pages_go_to_the_browser() {
        let away = "https://example.com/docs";
        assert_eq!(in_window(&chatgpt(), page(away)), elsewhere(away, false));
        assert_eq!(
            in_window(&chatgpt(), new_window(away)),
            elsewhere(away, false)
        );
        assert_eq!(
            in_window(&chatgpt(), page("http://example.com/")),
            elsewhere("http://example.com/", false)
        );
        assert_eq!(
            in_window(&claude(), page("https://claude.ai:8443/")),
            elsewhere("https://claude.ai:8443/", false),
            "a port is not the site"
        );
        assert_eq!(
            in_window(&claude(), page("https://x@claude.ai/")),
            elsewhere("https://x@claude.ai/", false),
            "neither is user information hiding the host"
        );
    }

    /// A page the site sent to the browser without a click is usually a sign-in to connect
    /// something, which goes to whichever account of the site the browser is signed in to:
    /// the window says so. One a person clicked, and an email address, say nothing.
    #[test]
    fn a_page_the_site_sent_to_the_browser_by_itself_is_said() {
        let away = "https://example.com/connect";
        assert_eq!(
            in_window(&chatgpt(), clicked(page(away))),
            elsewhere(away, true)
        );
        assert_eq!(
            in_window(&chatgpt(), clicked(new_window(away))),
            elsewhere(away, true)
        );
        assert_eq!(
            in_popup(&chatgpt(), page(away)),
            elsewhere(away, false),
            "a sign-in window's page too"
        );
        assert_eq!(
            in_window(&chatgpt(), new_window("mailto:help@example.com")),
            NavigationDecision::OpenElsewhere {
                url: "mailto:help@example.com".into(),
                note: None
            }
        );
    }

    /// Google blocks its sign-in and the connection of its apps inside apps, and in the
    /// browser either would sign in the browser rather than this account.
    #[test]
    fn googles_sign_in_is_refused_not_handed_to_the_browser() {
        let google = "https://accounts.google.com/o/oauth2/v2/auth";
        let refused = Refuse {
            note: WindowNoteKind::GoogleRefused,
        };
        assert_eq!(in_window(&chatgpt(), clicked(page(google))), refused);
        assert_eq!(in_window(&chatgpt(), new_window(google)), refused);
        assert_eq!(in_popup(&chatgpt(), page(google)), refused);
        assert_eq!(
            in_window(&claude(), page("https://ACCOUNTS.google.com:8443/x")),
            refused,
            "whatever its case or port"
        );
    }

    /// An address goes to the email app only when somebody asked, as a browser does. A web
    /// page never launches another app through Pitboard, and one a person clicked says so.
    #[test]
    fn only_asked_for_addresses_leave_and_other_apps_are_refused() {
        let mail = "mailto:help@example.com";
        let handed = NavigationDecision::OpenElsewhere {
            url: mail.into(),
            note: None,
        };
        assert_eq!(in_window(&chatgpt(), clicked(page(mail))), handed);
        assert_eq!(in_window(&chatgpt(), new_window(mail)), handed);
        assert_eq!(in_window(&chatgpt(), page(mail)), Ignore);
        assert_eq!(
            in_window(&chatgpt(), clicked(page("vscode://file/x"))),
            Refuse {
                note: WindowNoteKind::OtherApp {
                    scheme: "vscode".into()
                }
            }
        );
        assert_eq!(
            in_window(&chatgpt(), clicked(page("VSCode://file/x"))),
            Refuse {
                note: WindowNoteKind::OtherApp {
                    scheme: "vscode".into()
                }
            },
            "named in lower case"
        );
        assert_eq!(in_window(&chatgpt(), page("vscode://file/x")), Ignore);
        assert_eq!(
            in_window(&chatgpt(), clicked(page("no scheme"))),
            Ignore,
            "nothing is said of no app"
        );
    }

    /// Saving a file does not move the page. Only the site's own page saves without asking:
    /// an artifact runs somebody else's code in a frame of its own.
    #[test]
    fn downloads_from_the_site_save_and_others_ask() {
        assert_eq!(
            in_window(
                &claude(),
                by(Asker::Site, download(page("https://claude.ai/files/x")))
            ),
            Download { ask: false }
        );
        assert_eq!(
            in_window(
                &claude(),
                by(Asker::Site, download(page("blob:https://claude.ai/1")))
            ),
            Download { ask: false }
        );
        assert_eq!(
            in_window(&claude(), download(frame("data:text/plain,x"))),
            Download { ask: true }
        );
        assert_eq!(
            in_window(&claude(), download(new_window("https://example.com/x.zip"))),
            Download { ask: true }
        );
        assert_eq!(
            in_window(
                &claude(),
                by(Asker::Site, download(page("http://claude.ai/x")))
            ),
            Ignore,
            "nothing plain http is saved"
        );
        assert_eq!(in_window(&claude(), download(page("file:///x"))), Ignore);
    }

    // A sign-in window's page.

    #[test]
    fn a_sign_in_window_loads_the_site_and_its_sign_in_only() {
        assert_eq!(
            in_popup(&chatgpt(), page("https://auth.openai.com/log-in")),
            Load
        );
        assert_eq!(
            in_popup(&chatgpt(), page("https://chatgpt.com/api/auth/callback")),
            Load
        );
        assert_eq!(
            in_popup(&chatgpt(), by(Asker::SignIn, page("about:blank"))),
            Load
        );
        assert_eq!(
            in_popup(&chatgpt(), empty_window(Asker::Site)),
            Ignore,
            "it opens no window"
        );
        assert_eq!(
            in_popup(&chatgpt(), new_window("https://auth.openai.com/x")),
            Ignore
        );
        assert_eq!(
            in_popup(&chatgpt(), clicked(page("https://example.com/terms"))),
            elsewhere("https://example.com/terms", true)
        );
        assert_eq!(
            in_popup(
                &chatgpt(),
                by(Asker::Site, download(page("https://chatgpt.com/x")))
            ),
            Ignore,
            "it saves nothing"
        );
    }

    // Responses.

    fn response(main_frame: bool, can_show: bool, disposition: Option<&str>) -> ResponseFacts {
        ResponseFacts {
            url: Some("https://chatgpt.com/files/x".into()),
            main_frame,
            can_show,
            disposition: disposition.map(str::to_owned),
        }
    }

    #[test]
    fn a_response_is_downloaded_when_it_cannot_be_shown_or_is_an_attachment() {
        let decide = |role, facts| decide_response(chatgpt(), role, facts);
        let window = PageRole::Window;
        assert_eq!(
            decide(window, response(true, true, None)),
            ResponseDecision::Show
        );
        assert_eq!(
            decide(window, response(true, true, Some("inline"))),
            ResponseDecision::Show
        );
        assert_eq!(
            decide(
                window,
                response(true, true, Some(" Attachment; filename=x"))
            ),
            ResponseDecision::Download { ask: false }
        );
        assert_eq!(
            decide(window, response(false, false, None)),
            ResponseDecision::Download { ask: true },
            "a frame's"
        );
        assert_eq!(
            decide(
                window,
                ResponseFacts {
                    url: Some("https://example.com/x".into()),
                    ..response(true, false, None)
                }
            ),
            ResponseDecision::Download { ask: true },
            "a page off the site"
        );
        assert_eq!(
            decide(
                window,
                ResponseFacts {
                    url: None,
                    ..response(true, false, None)
                }
            ),
            ResponseDecision::Download { ask: true }
        );
        assert_eq!(
            decide(PageRole::Popup, response(true, false, None)),
            ResponseDecision::Ignore,
            "a sign-in window saves nothing"
        );
        assert_eq!(
            decide(PageRole::Popup, response(true, true, None)),
            ResponseDecision::Show
        );
    }

    /// The header is trimmed as Foundation's `whitespaces` trims it, measured on macOS 27: a
    /// tab, Unicode's spaces and U+200B go, a newline stays.
    #[test]
    fn a_disposition_is_trimmed_as_foundation_trims_it() {
        let decide = |disposition: &str| {
            decide_response(
                chatgpt(),
                PageRole::Window,
                response(true, true, Some(disposition)),
            )
        };
        assert_eq!(
            decide(" \t\u{a0}\u{200b}\u{3000}ATTACHMENT"),
            ResponseDecision::Download { ask: false }
        );
        assert_eq!(decide("\nattachment"), ResponseDecision::Show);
        assert_eq!(
            decide("attachments"),
            ResponseDecision::Download { ask: false }
        );
    }

    // Origins.

    /// A fixture keeps each site's host on a scheme of its own, so the same policy runs
    /// there, and a link of the site's opens the stand-in rather than the network.
    #[test]
    fn a_fixtures_window_keeps_its_own_scheme() {
        let fixture = policy(&CHATGPT, "pitboard-fixture");
        assert_eq!(
            window_home(fixture.clone()),
            "pitboard-fixture://chatgpt.com/"
        );
        assert_eq!(
            in_window(&fixture, page("pitboard-fixture://chatgpt.com/c/x")),
            Load
        );
        assert_eq!(
            in_window(&fixture, page("https://chatgpt.com/c/x")),
            elsewhere("https://chatgpt.com/c/x", false)
        );
        let link = pitboard_sites::SiteLink::parse("https://chat.openai.com/c/x?y=1#z");
        let link = SiteLink::from(link.expect("a link"));
        assert_eq!(
            window_address(&fixture, &link),
            "pitboard-fixture://chatgpt.com/c/x?y=1#z"
        );
        assert_eq!(window_address(&chatgpt(), &link), link.url);
        assert_eq!(window_home(claude()), "https://claude.ai/");
    }

    /// A frame given a blank window could fill it with a page of its own, which nothing on
    /// the window would tell apart from the site's: only the site's page and its sign-in get
    /// one.
    #[test]
    fn a_frame_gets_no_blank_window() {
        assert_eq!(in_window(&chatgpt(), empty_window(Asker::Other)), Ignore);
        assert_eq!(in_window(&chatgpt(), new_window("about:blank")), Ignore);
    }

    /// What decides is the frame that asked: a frame's download aimed at the top page still
    /// asks, and the site's own download asking for a new window does not.
    #[test]
    fn the_frame_that_asked_decides_whether_a_download_asks() {
        assert_eq!(
            in_window(&claude(), download(page("data:text/plain,x"))),
            Download { ask: true }
        );
        assert_eq!(
            in_window(
                &claude(),
                by(Asker::Site, download(new_window("https://claude.ai/x")))
            ),
            Download { ask: false }
        );
        assert_eq!(
            in_window(
                &claude(),
                by(Asker::SignIn, download(page("https://auth.openai.com/x")))
            ),
            Download { ask: true }
        );
    }

    #[test]
    fn the_frame_that_asked_is_told_apart() {
        let asker = |main_frame: bool, scheme: &str, host: &str, port: i64| {
            frame_asker(
                chatgpt(),
                FrameOrigin {
                    main_frame,
                    scheme: scheme.into(),
                    host: host.into(),
                    port,
                },
            )
        };
        assert_eq!(asker(true, "https", "chatgpt.com", 0), Asker::Site);
        assert_eq!(asker(true, "https", "chat.com", 0), Asker::Site);
        assert_eq!(asker(true, "HTTPS", "CHATGPT.COM", 0), Asker::Site);
        assert_eq!(asker(true, "https", "auth.openai.com", 0), Asker::SignIn);
        assert_eq!(asker(false, "https", "chatgpt.com", 0), Asker::Other);
        assert_eq!(asker(true, "http", "chatgpt.com", 0), Asker::Other);
        assert_eq!(asker(true, "https", "chatgpt.com", 8443), Asker::Other);
        assert_eq!(asker(true, "https", "", 0), Asker::Other);
        assert_eq!(asker(true, "https", "evil.example", 0), Asker::Other);
    }

    /// Only the site's own page or its sign-in opens a sign-in window: a frame that could
    /// open one could fill it with a page of its own, titled as it likes.
    #[test]
    fn a_frame_opens_no_sign_in_window() {
        let auth = "https://login.microsoftonline.com/x";
        assert_eq!(
            in_window(&chatgpt(), by(Asker::Site, new_window(auth))),
            Popup
        );
        assert_eq!(
            in_window(&chatgpt(), by(Asker::SignIn, new_window(auth))),
            Popup
        );
        assert_eq!(in_window(&chatgpt(), new_window(auth)), Ignore);
    }

    /// A sign-in window's page may go blank itself; its opener may not make it.
    #[test]
    fn only_a_sign_in_windows_own_page_blanks_it() {
        assert_eq!(
            in_popup(&chatgpt(), by(Asker::SignIn, page("about:blank"))),
            Load
        );
        assert_eq!(in_popup(&chatgpt(), page("about:blank")), Ignore);
        assert_eq!(in_popup(&chatgpt(), empty_window(Asker::Other)), Ignore);
    }

    /// A page the window keeps as its last is one of the site's own on the window's scheme,
    /// never a page off the site, its sign-in or another scheme's.
    #[test]
    fn only_the_sites_own_pages_are_its_pages() {
        let fixture = policy(&CLAUDE, "pitboard-fixture");
        assert!(is_site_page(
            &fixture,
            "pitboard-fixture://claude.ai/chat/old"
        ));
        assert!(!is_site_page(&fixture, "https://example.com/somewhere"));
        assert!(!is_site_page(&fixture, "https://claude.ai/chat/old"));
        assert!(is_site_page(&chatgpt(), "https://chat.com/c/x"));
        assert!(!is_site_page(&chatgpt(), "https://auth.openai.com/log-in"));
        assert!(!is_site_page(&chatgpt(), "https://chatgpt.com:0/"));
        assert!(!is_site_page(&chatgpt(), "https://@chatgpt.com/"));
        assert!(!is_site_page(&chatgpt(), ""));
    }

    /// A page's host is the one its request goes to, as Foundation's `URL.host` gives it: a
    /// look-alike of a site's host in another script, or a punycode label, is not the site,
    /// and an escape in the host is read, as `URL.host` reads it.
    #[test]
    fn a_page_is_the_sites_only_on_the_sites_own_host() {
        assert!(is_site_page(&claude(), "https://cl%61ude.ai/"));
        assert!(!is_site_page(&claude(), "https://xn--claude-.ai/"));
        assert!(!is_site_page(&chatgpt(), "https://chаtgpt.com/"));
        assert!(!is_site_page(&claude(), "https://claude.ai./"));
        assert!(!is_site_page(
            &claude(),
            "https://claude.ai:99999999999999999999/"
        ));
    }
}
