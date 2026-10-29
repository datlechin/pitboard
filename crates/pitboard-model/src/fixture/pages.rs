//! The stand-in pages a fixture's account windows load on the scheme `pitboard-fixture`, in
//! place of the sites, so nothing reaches claude.ai or chatgpt.com: a page for each site, a
//! sign-in page for the hosts a site's sign-in goes to, and the frame an artifact is in,
//! each made from the site table both apps read. The macOS app's fixture served these from
//! FixtureWeb.swift, and they are the same pages, link for link, so its UI tests find what
//! they look for.

use crate::Site;
use pitboard_sites::WebAddress;

/// The scheme the stand-ins are served under. WebKit does not let an app serve `https`
/// itself, so each keeps its site's host on a scheme of its own, and the navigation rules
/// are the same in a fixture as on the sites.
pub(crate) const SCHEME: &str = "pitboard-fixture";

/// The host an artifact's frame is served from: not the site's, as a real artifact's is not.
const ARTIFACT_HOST: &str = "artifact.fixture";

/// The page for `url`: a site's own host gets its stand-in, with the path asked for in it, a
/// host a site's sign-in goes to gets a sign-in stand-in, the artifact's host its frame, and
/// anything else a page with nothing on it. The host is read as Foundation's `URL.host`
/// reads it, in any case, and the path as `URL.path` does.
pub(crate) fn page(url: &str) -> String {
    let address = WebAddress::parse(url);
    let host = address.host().unwrap_or_default().to_ascii_lowercase();
    let sites = pitboard_sites::ALL.map(Site::from);
    if let Some(site) = sites.iter().find(|site| site.host == host) {
        return site_page(site, address.path());
    }
    if sites.iter().any(|site| site.sign_in_hosts.contains(&host)) {
        return sign_in_page(&host);
    }
    if host == ARTIFACT_HOST {
        // A message from the page clicks the link, as a person would inside the frame,
        // since a page cannot reach into a frame of another origin.
        return document(
            "Artifact",
            "<a id=\"artifact-download\" download=\"artifact.txt\" \
             href=\"data:text/plain,artifact\">Download the artifact</a>\
             <script>addEventListener('message', () => \
             document.getElementById('artifact-download').click())</script>",
        );
    }
    document("Stand-in", "<p>Nothing is here.</p>")
}

fn site_page(site: &Site, path: &str) -> String {
    let mut hosts = site.sign_in_hosts.clone();
    hosts.sort();
    let sign_in = hosts.first().map_or_else(String::new, |host| {
        let address = format!("{SCHEME}://{host}/sign-in");
        format!(
            "<p><a id=\"sign-in-link\" href=\"{address}\" target=\"_blank\" \
             rel=\"opener\">Continue with {host}</a></p>\
             <p><button id=\"sign-in-button\" onclick=\"window.open('{address}', \
             'sign-in', 'width=480,height=600')\">Sign in in a window</button></p>\
             <p><button id=\"blank-popup\" onclick=\"const w = window.open(''); \
             w.location = '{address}'\">Sign in through a blank window</button></p>"
        )
    });
    let (name, host) = (&site.name, &site.host);
    document(
        &format!("{name} stand-in"),
        &format!(
            "<p>A Pitboard fixture page at {path}. Nothing here reaches the network. \
             Find the word needle here, and the needle there.</p>\
             <p><a id=\"outside\" href=\"https://example.com/\">A link outside \
             {name}</a></p>\
             <p><a id=\"google\" href=\"https://accounts.google.com/o/oauth2/v2/auth\">\
             Continue with Google</a></p>\
             <p><a id=\"other-app\" href=\"vscode://file/x\">Open in an editor</a></p>\
             <p><a id=\"chat\" href=\"{SCHEME}://{host}/chat/fixture\">A chat</a></p>\
             <p><a id=\"download\" download=\"notes.txt\" \
             href=\"data:text/plain,notes\">Download notes</a></p>\
             <p><button id=\"alert\" onclick=\"alert('Saved.')\">Alert</button>\
             <button id=\"confirm\" onclick=\"document.title = confirm('Delete?') ? \
             'confirmed' : 'declined'\">Confirm</button></p>\
             {sign_in}\
             <iframe title=\"artifact\" src=\"{SCHEME}://{ARTIFACT_HOST}/\" \
             width=\"320\" height=\"80\"></iframe>"
        ),
    )
}

fn sign_in_page(host: &str) -> String {
    document(
        &format!("{host} sign-in stand-in"),
        "<p>A Pitboard fixture page for signing in.</p>\
         <p><button id=\"done\" onclick=\"window.close()\">Done</button></p>",
    )
}

fn document(title: &str, body: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title>\
         </head><body><h1>{title}</h1>{body}</body></html>"
    )
}
