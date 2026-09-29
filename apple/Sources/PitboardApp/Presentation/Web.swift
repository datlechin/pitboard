import CoreServices
import CryptoKit
import Foundation
import PitboardKit

// What a claude.ai window decides, as functions of what it is asked, so each rule is tested
// without starting WebKit: which account's data a window keeps, where a navigation goes,
// what is downloaded and under which name, and what the window says.

// MARK: - An account's store

/// pitboard's namespace for claude.ai windows. It never changes, and neither does the name
/// hashed in it: a change would leave every window's store an orphan, and the next sweep
/// would sign every window out. A golden test pins both.
let webStoreNamespace = UUID(uuidString: "674b09f3-8d37-4e48-a361-5af2a6856773")!

/// The data store of `account`'s claude.ai window, or nil for an account that has none: a
/// Codex account, a Claude Code login pitboard has no name for, or one it cannot place.
///
/// Derived rather than stored, so there is nothing to keep in step with the accounts: a
/// rename keeps the window's sign-in, and a store no enrolled account derives is an orphan
/// the app can find.
func webStoreID(for account: Account) -> UUID? {
    guard account.provider == defaultProvider, account.label != nil, !account.unplaced,
        !account.accountUuid.isEmpty
    else { return nil }
    return webStoreID(accountUuid: account.accountUuid)
}

/// A version 5 UUID (RFC 9562, SHA-1) of `claude:<account id>` in pitboard's namespace. A
/// version 5 UUID is never the nil UUID, which WebKit refuses as a store's identifier. The
/// account id is not secret, and the hash keeps it out of folder names and saved windows and
/// nothing more.
func webStoreID(accountUuid: String) -> UUID {
    let namespace = withUnsafeBytes(of: webStoreNamespace.uuid) { Array($0) }
    let name = Array("claude:\(accountUuid.lowercased())".utf8)
    var bytes = Array(Insecure.SHA1.hash(data: namespace + name).prefix(16))
    bytes[6] = (bytes[6] & 0x0F) | 0x50
    bytes[8] = (bytes[8] & 0x3F) | 0x80
    return UUID(
        uuid: (
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
        ))
}

/// An account that has a claude.ai window, as the menus and the account picker list it.
struct ClaudeAccount: Equatable, Hashable, Identifiable {
    let label: String
    let email: String
    let store: UUID
    let inUse: Bool

    var id: UUID { store }
}

/// Every enrolled Claude Code account, in the order the core lists them: what the menu, the
/// File menu, the shortcut menu and the account picker offer, so they cannot disagree.
func claudeWindows(in status: Status?) -> [ClaudeAccount] {
    (status?.accounts ?? []).compactMap { account in
        guard let store = webStoreID(for: account), let label = account.label else {
            return nil
        }
        return ClaudeAccount(
            label: label, email: account.email, store: store, inUse: account.signedIn)
    }
}

/// How a menu offers the claude.ai windows: nothing with no account that has one, an item
/// naming the only one, and a submenu of labels for several. A submenu of one item is what
/// the platform asks not to make.
enum ClaudeMenu: Equatable {
    case none
    case one(ClaudeAccount)
    case several([ClaudeAccount])

    init(_ accounts: [ClaudeAccount]) {
        switch accounts.count {
        case 0: self = .none
        case 1: self = .one(accounts[0])
        default: self = .several(accounts)
        }
    }

    /// The item's title, or the submenu's.
    var title: String? {
        switch self {
        case .none: nil
        case .one(let account): "Open claude.ai as \(account.label)"
        case .several: "Open claude.ai"
        }
    }
}

/// What the forget alert says. Forgetting a Claude Code account that has a claude.ai window
/// deletes what that window keeps too, and a person deciding should know.
func forgetMessage(for account: Account) -> String {
    guard webStoreID(for: account) != nil else {
        return "pitboard deletes the login it parked for this account. Using it again needs a "
            + "sign-in in your browser."
    }
    return "pitboard deletes the login it parked for this account, and everything its "
        + "claude.ai window keeps on this Mac, its sign-in included. Using it again needs a "
        + "sign-in in your browser."
}

/// What the sign-out alert says for the account `label` names, quoted as the forget alert
/// quotes one.
func signOutMessage(label: String?) -> String {
    let account = label.map { "“\($0)”" } ?? "this account"
    return "pitboard deletes this window’s sign-in and everything else claude.ai keeps for "
        + "\(account) on this Mac. The account stays signed in on your other devices and "
        + "browsers. claude.ai’s own Log Out would sign it out on every device."
}

// MARK: - Where a navigation goes

/// A navigation as the policy sees it.
struct WebRequest: Equatable {
    enum Target: Equatable {
        /// The window's own page.
        case window
        /// A frame inside it.
        case frame
        /// A new window: WebKit's target frame is nil.
        case newWindow

        /// The target of a navigation whose target frame is main, is not, or is not there
        /// at all, as `WKNavigationAction.targetFrame?.isMainFrame` says.
        init(frameIsMain: Bool?) {
            switch frameIsMain {
            case nil: self = .newWindow
            case true?: self = .window
            case false?: self = .frame
            }
        }
    }

    let url: URL
    let target: Target
    /// A person clicked a link or submitted a form.
    var clicked = false
    /// WebKit says to download what this asks for.
    var download = false
    /// The window's own page asked, on claude.ai, rather than a frame inside it such as an
    /// artifact's.
    var fromHome = false
}

enum WebRoute: Equatable {
    /// In this window, as asked.
    case load
    /// A new window's page, loaded in this window instead.
    case loadHere(URL)
    /// Downloaded into the downloads folder.
    case download
    /// Downloaded into the downloads folder once the person says so, as Safari asks before a
    /// website it does not know downloads.
    case askToDownload
    /// Cancelled here and handed to macOS: the default browser for a web page, the default
    /// email app for an address.
    case elsewhere(URL)
    /// Cancelled, and the window says why.
    case refuse(WebRefusal)
    /// Cancelled, and nothing is said.
    case drop
}

enum WebRefusal: Equatable {
    /// Google blocks its pages in a web view inside an app: its sign-in, the single sign-on
    /// it provides and the connection of its apps. In the browser, any of them would sign in
    /// the browser, not this window.
    case google
}

/// Where `request` goes. `home` is the one origin a window keeps: `https://claude.ai` in a
/// live run and the fixture's own in a fixture, compared by scheme, host without case, and
/// port. The first rule that matches decides.
func route(_ request: WebRequest, home: URL) -> WebRoute {
    let url = request.url
    let scheme = url.scheme?.lowercased() ?? ""
    // Saving a file does not move the window. Artifact downloads are often `blob:` links,
    // whose host Foundation does not see. Nothing plain `http`, local or unknown is saved.
    // Only claude.ai's own page saves without asking: an artifact runs somebody else's code
    // in a frame of its own.
    if request.download {
        guard ["https", "blob", "data"].contains(scheme) else { return .drop }
        return request.fromHome ? .download : .askToDownload
    }
    // The page decides what it embeds: artifacts render in frames of their own origin. What
    // decides is where the window goes. A web page never opens a local file.
    if request.target == .frame {
        return scheme == "file" ? .drop : .load
    }
    // A new document starts there and carries nothing from anywhere. pitboard makes no
    // popups, so one asked for in a new window goes nowhere.
    if scheme == "about" {
        return request.target == .window ? .load : .drop
    }
    // claude.ai itself. A claude.ai link asked for in a new tab stays in this window, so it
    // stays signed in as this account, and Back returns to where the person was.
    if isHome(url, home) {
        return request.target == .window ? .load : .loadHere(url)
    }
    if scheme == "http" || scheme == "https" {
        return url.host?.lowercased() == "accounts.google.com"
            ? .refuse(.google) : .elsewhere(url)
    }
    // A person's own click hands an address to the default email app, as a browser does.
    if scheme == "mailto", request.clicked || request.target == .newWindow {
        return .elsewhere(url)
    }
    // A web page never launches another app through pitboard.
    return .drop
}

/// Whether `url` is on `home`'s origin: the same scheme, the same host without case, the
/// same port, and no user information to hide the host behind.
func isHome(_ url: URL, _ home: URL) -> Bool {
    guard let scheme = url.scheme?.lowercased(), scheme == home.scheme?.lowercased(),
        let host = url.host?.lowercased(), host == home.host?.lowercased(),
        url.port == home.port, url.user == nil, url.password == nil
    else { return false }
    return true
}

/// Whether a frame's security origin, as WebKit gives it, is `home`'s. WebKit says 0 for a
/// scheme's own port, and an empty host for an origin of its own, such as a sandboxed
/// frame's.
func isHome(scheme: String, host: String, port: Int, _ home: URL) -> Bool {
    !host.isEmpty && scheme.lowercased() == home.scheme?.lowercased()
        && host.lowercased() == home.host?.lowercased() && port == (home.port ?? 0)
}

enum ResponseRoute: Equatable {
    case show
    case download
    case askToDownload
}

/// What becomes of a response: downloaded when WebKit cannot show it or the server says it
/// is an attachment, and shown otherwise. `disposition` is its `Content-Disposition`. Only
/// the window's own page, which is always on claude.ai, saves without asking: a frame's
/// response is asked about first.
func responseRoute(canShow: Bool, disposition: String?, mainFrame: Bool) -> ResponseRoute {
    let kind = disposition?.trimmingCharacters(in: .whitespaces).lowercased() ?? ""
    guard !canShow || kind.hasPrefix("attachment") else { return .show }
    return mainFrame ? .download : .askToDownload
}

/// What the window says after `route` decided `request`, for the account `email` and
/// `label` name, or nil when nothing needs saying.
///
/// A page claude.ai sent to the browser without a click, a redirect or a window it opened,
/// is usually a sign-in to connect something. The browser connects it to whichever
/// claude.ai account it is signed in to, which is worth saying where several are.
func routeNote(
    _ route: WebRoute, for request: WebRequest, email: String, label: String
) -> WebNote? {
    switch route {
    case .refuse(.google):
        return .googleRefused(email: email)
    case .elsewhere(let url):
        let scheme = url.scheme?.lowercased()
        guard !request.clicked, scheme == "http" || scheme == "https" else { return nil }
        return .openedInBrowser(label: label)
    case .drop:
        return request.target == .newWindow && !request.download ? .windowNotShown : nil
    case .load, .loadHere, .download, .askToDownload:
        return nil
    }
}

// MARK: - Loading

/// Whether a failed load is worth saying. WebKit shows no error page, so without a word the
/// window stays blank. A load stopped for another, `NSURLErrorCancelled`, and one a policy
/// decision ended, WebKit's 102, which every download and cancelled navigation gives, are not
/// failures anybody needs told about.
func loadFailureShown(domain: String, code: Int) -> Bool {
    switch (domain, code) {
    case (NSURLErrorDomain, NSURLErrorCancelled): false
    case ("WebKitErrorDomain", 102): false
    default: true
    }
}

/// The items of WebKit's own shortcut menu a claude.ai window keeps. WebKit hands the
/// downloads these three start to the app only through private calls, so they would be
/// cancelled without a word. claude.ai's own download buttons still work.
func keepsMenuItem(_ identifier: String) -> Bool {
    ![
        "WKMenuItemIdentifierDownloadLinkedFile", "WKMenuItemIdentifierDownloadImage",
        "WKMenuItemIdentifierDownloadMedia",
    ].contains(identifier)
}

// MARK: - Downloads

/// Where a download is saved in `folder`: the last component of the name the server
/// suggests, with slashes, colons and control characters taken out and leading dots dropped,
/// or `Download` when nothing is left; then `report 2.pdf`, `report 3.pdf` and so on while
/// `taken` says a name is. WebKit wants a file that does not exist yet, and two downloads can
/// ask at once, so `taken` answers for names already reserved as well as files that exist.
func downloadDestination(suggested: String, in folder: URL, taken: (URL) -> Bool) -> URL {
    let last = suggested.split(separator: "/", omittingEmptySubsequences: true).last ?? ""
    var name = String(
        String.UnicodeScalarView(
            last.unicodeScalars.filter {
                $0 != ":" && !CharacterSet.controlCharacters.contains($0)
            }))
    while name.hasPrefix(".") { name.removeFirst() }
    name = name.trimmingCharacters(in: .whitespaces)
    if name.isEmpty { name = "Download" }
    let base = (name as NSString).deletingPathExtension
    let suffix = (name as NSString).pathExtension
    var candidate = folder.appendingPathComponent(name)
    var number = 2
    while taken(candidate) {
        let numbered = suffix.isEmpty ? "\(base) \(number)" : "\(base) \(number).\(suffix)"
        candidate = folder.appendingPathComponent(numbered)
        number += 1
    }
    return candidate
}

/// The quarantine a download gets, so Gatekeeper checks an app or a script saved from a
/// claude.ai window as it checks one saved from a browser.
struct Quarantine: Equatable {
    let agent: String
    /// Where it came from, when that is a web address and not a `blob:` or `data:` link.
    let origin: URL?

    /// As Launch Services takes it.
    var properties: [String: Any] {
        var properties: [String: Any] = [
            kLSQuarantineTypeKey as String: kLSQuarantineTypeWebDownload as String,
            kLSQuarantineAgentNameKey as String: agent,
        ]
        if let origin { properties[kLSQuarantineDataURLKey as String] = origin }
        return properties
    }
}

/// Whether `url` is taken for a download: another download of this app is saving to it, or
/// a file is already there.
func downloadTaken(_ url: URL, reserved: Set<URL>) -> Bool {
    reserved.contains(url) || FileManager.default.fileExists(atPath: url.path)
}

/// A web download's quarantine, by pitboard, from `url`.
func quarantine(downloadedFrom url: URL?) -> Quarantine {
    let scheme = url?.scheme?.lowercased()
    return Quarantine(
        agent: "pitboard", origin: scheme == "http" || scheme == "https" ? url : nil)
}

// MARK: - Zoom

enum ZoomChange: Equatable {
    case actualSize
    case zoomIn
    case zoomOut
}

/// Safari's zoom steps, as fractions of the page's size.
let zoomSteps: [Double] = [0.5, 0.75, 0.85, 1, 1.15, 1.25, 1.5, 1.75, 2, 2.5, 3]

/// The page zoom after `change` from `current`: the next of Safari's steps either way,
/// staying at the ends, or back to 100%.
func zoom(from current: Double, toward change: ZoomChange) -> Double {
    let near = 0.001
    switch change {
    case .actualSize: return 1
    case .zoomIn: return zoomSteps.first { $0 > current + near } ?? zoomSteps.last!
    case .zoomOut: return zoomSteps.last { $0 < current - near } ?? zoomSteps.first!
    }
}

// MARK: - What a window says

/// What pitboard has to say about one claude.ai window, in the bar above its page. The
/// latest thing said replaces the one before.
enum WebNote: Equatable {
    /// The window opened on a store WebKit had not listed: it has never signed in, or its
    /// sign-in was deleted. pitboard knows which stores exist and never looks inside one.
    case firstSignIn(email: String)
    /// The policy stopped a page of Google's: its sign-in, its single sign-on, or connecting
    /// one of its apps.
    case googleRefused(email: String)
    /// claude.ai sent a page to the browser without a click.
    case openedInBrowser(label: String)
    /// claude.ai asked for a window pitboard does not make.
    case windowNotShown
    /// The page did not load, and WebKit shows no error page of its own.
    case loadFailed(reason: String)
    case downloading(name: String)
    case downloaded(name: String, file: URL)
    case downloadFailed(name: String, reason: String)

    var text: String {
        switch self {
        case .firstSignIn(let email):
            "Sign in to claude.ai as \(email). Click Continue with email: Google’s sign-in "
                + "does not work inside apps. Open the email on your phone, tap its link, "
                + "then enter here the code claude.ai shows there."
        case .googleRefused(let email):
            "Google does not allow its pages inside apps, so pitboard stopped it. To sign "
                + "in, click Continue with email with \(email), open the email on your "
                + "phone, tap its link, then enter here the code claude.ai shows there. "
                + "Gmail, Google Drive, Google Calendar and Google single sign-on cannot be "
                + "connected in this window."
        case .openedInBrowser(let label):
            "claude.ai opened a page in your browser. Anything you connect there goes to the "
                + "claude.ai account your browser is signed in to, which may not be "
                + "“\(label)”."
        case .windowNotShown:
            "claude.ai tried to open a window pitboard doesn’t show."
        case .loadFailed(let reason):
            "claude.ai didn’t load: \(reason) Click Reload to try again."
        case .downloading(let name):
            "Downloading “\(name)”…"
        case .downloaded(let name, _):
            "Downloaded “\(name)” to Downloads."
        case .downloadFailed(let name, let reason):
            "Couldn’t download “\(name)”: \(reason)"
        }
    }

    var symbol: String {
        switch self {
        case .firstSignIn: Symbol.signIn
        case .googleRefused, .openedInBrowser, .windowNotShown, .loadFailed:
            Notice.Severity.warning.symbol
        case .downloading, .downloaded: Symbol.download
        case .downloadFailed: Notice.Severity.error.symbol
        }
    }

    /// A download under way, which shows a spinner.
    var inProgress: Bool {
        if case .downloading = self { return true }
        return false
    }

    /// The file a finished download is in, which the bar offers to show in Finder.
    var file: URL? {
        if case .downloaded(_, let file) = self { return file }
        return nil
    }

    /// A failed load, which the next page to start loading puts away.
    var isLoadFailure: Bool {
        if case .loadFailed = self { return true }
        return false
    }
}

/// The site a download comes from, as its question names it: the origin of the frame that
/// started it, else its address's host. Foundation reads no host from a `blob:` link, so
/// the one inside it is read instead.
func downloadHost(frame: String?, url: URL?) -> String? {
    if let frame, !frame.isEmpty { return frame }
    guard let url else { return nil }
    if url.scheme?.lowercased() == "blob" {
        return URL(string: String(url.absoluteString.dropFirst("blob:".count)))?.host
    }
    return url.host
}

/// What the download alert asks, for a download a frame or a page not on claude.ai started:
/// the file's name and the site it comes from.
func downloadQuestion(name: String, host: String?) -> String {
    guard let host, !host.isEmpty else { return "Download “\(name)”?" }
    return "Download “\(name)” from \(host)?"
}
