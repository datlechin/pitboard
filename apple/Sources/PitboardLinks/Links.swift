import Foundation

// What may be handed to an account's claude.ai window from outside it, and the pitboard link
// that carries it there. Every way in, the Share extension, the Service, a bookmarklet and a
// pasted link, goes through these functions, so they refuse the same links in the same
// words. Foundation only: the Share extension links this and nothing else of pitboard's.

/// claude.ai, as a link from outside is checked against. A window's own home can differ: a
/// fixture's is a stand-in scheme, so nothing it opens reaches the network.
public let claudeHome = URL(string: "https://claude.ai/")!

/// Why a link from outside is not opened.
public enum LinkRefusal: Error, Equatable, Sendable {
    /// Not a pitboard link of this build: another scheme, or no `open` host where one goes.
    case notPitboard
    /// A pitboard link asking for something this version does not know.
    case unknownRequest
    /// More than one link in it, or none: a link inside it was not encoded.
    case ambiguous
    /// Longer than any claude.ai link is.
    case tooLong
    /// A link to somewhere other than claude.ai, and where, when there is a host to name.
    case notClaude(host: String?)
    /// claude.ai's emailed sign-in link, which would sign a window in as whoever it is for.
    case signInLink
    /// Nothing that reads as a link.
    case noLink

    /// The refusal as the picker and the Share extension say it.
    public var message: String {
        switch self {
        case .notClaude(let host?):
            "pitboard opens claude.ai links only. This link is on \(host)."
        case .notClaude(nil):
            "pitboard opens claude.ai links only."
        case .signInLink:
            "pitboard doesn’t open claude.ai sign-in links from outside: one would sign the "
                + "window in as whoever the link belongs to. Sign in inside the account’s "
                + "claude.ai window."
        case .unknownRequest, .notPitboard:
            "This pitboard link isn’t one this version knows. Update pitboard, or copy the "
                + "claude.ai link and choose Open claude.ai Link… in pitboard’s menu."
        case .ambiguous:
            "This pitboard link has more in it than one claude.ai link. The link inside it "
                + "has to be encoded, as the bookmarklet does."
        case .noLink:
            "There is no claude.ai link in what was sent."
        case .tooLong:
            "What was sent is too long to be a claude.ai link."
        }
    }
}

/// The longest pitboard link read. claude.ai's own links are under 200 characters, and a page
/// that sends more would only fill the picker.
let longestLink = 8192

/// The pitboard link that asks the app to open `link`: `<scheme>://open?url=<link>`, with
/// everything but the unreserved characters percent-encoded. That is a subset of what
/// JavaScript's `encodeURIComponent` leaves bare, so a link either of them builds reads back
/// the same.
public func pitboardLink(opening link: String, scheme: String) -> URL {
    let unreserved = CharacterSet(
        charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~")
    let encoded = link.addingPercentEncoding(withAllowedCharacters: unreserved) ?? ""
    var parts = URLComponents()
    parts.scheme = scheme
    parts.host = "open"
    parts.percentEncodedQuery = "url=\(encoded)"
    // Only unreserved characters and escapes, after a scheme the app declares: it parses.
    return parts.url!
}

/// The link a pitboard link carries, or why it carries none this build opens.
///
/// Strict on purpose. A link written without encoding its claude.ai link,
/// `pitboard://open?url=https://claude.ai/x?a=1&b=2#c`, reads as a shorter link, another
/// item and a fragment, and opening the first part would open the wrong page without a word.
public func linkText(in url: URL, scheme: String) -> Result<String, LinkRefusal> {
    guard url.absoluteString.count <= longestLink else { return .failure(.tooLong) }
    guard url.scheme?.lowercased() == scheme.lowercased(),
        let parts = URLComponents(url: url, resolvingAgainstBaseURL: false),
        let host = parts.host, !host.isEmpty,
        parts.user == nil, parts.password == nil, parts.port == nil
    else { return .failure(.notPitboard) }
    guard host.lowercased() == "open", parts.path.isEmpty || parts.path == "/" else {
        return .failure(.unknownRequest)
    }
    guard parts.fragment == nil, let items = parts.queryItems, items.count == 1,
        let item = items.first, item.name == "url", let value = item.value, !value.isEmpty
    else { return .failure(.ambiguous) }
    return .success(value)
}

/// `text` as a claude.ai link to open on `home`, the window's own origin, or why not.
///
/// Spaces around it are dropped, a bare `claude.ai/…` is taken as `https`, and `http` is
/// taken as `https`. Only claude.ai itself: no other host, no subdomain, no port and no user
/// information. claude.ai's emailed sign-in link is refused as well: opened in an account's
/// window, somebody else's would sign that window in as them, under the person's label.
///
/// The link's path, query and fragment go onto `home`, so a fixture's window opens its
/// stand-in page for any claude.ai link and never the network.
public func claudeLink(from text: String, home: URL) -> Result<URL, LinkRefusal> {
    var typed = text.trimmingCharacters(in: .whitespacesAndNewlines)
    let bare = typed.lowercased()
    if bare == "claude.ai"
        || ["claude.ai/", "claude.ai?", "claude.ai#"].contains(where: bare.hasPrefix)
    {
        typed = "https://" + typed
    }
    guard !typed.isEmpty, let parts = URLComponents(string: typed),
        let scheme = parts.scheme?.lowercased(), scheme == "https" || scheme == "http",
        let host = parts.host?.lowercased(), !host.isEmpty
    else { return .failure(.noLink) }
    guard host == "claude.ai" else { return .failure(.notClaude(host: host)) }
    if let port = parts.port { return .failure(.notClaude(host: "\(host):\(port)")) }
    guard parts.user == nil, parts.password == nil else {
        return .failure(.notClaude(host: nil))
    }
    if let refusal = pathRefusal(parts.path) { return .failure(refusal) }
    guard var opened = URLComponents(url: home, resolvingAgainstBaseURL: false) else {
        return .failure(.noLink)
    }
    opened.percentEncodedPath =
        parts.percentEncodedPath.isEmpty ? "/" : parts.percentEncodedPath
    opened.percentEncodedQuery = parts.percentEncodedQuery
    opened.percentEncodedFragment = parts.percentEncodedFragment
    guard let url = opened.url else { return .failure(.noLink) }
    return .success(url)
}

/// Why a claude.ai link's path, percent-decoded as `URLComponents.path` gives it, is not
/// opened, or nil when it may be.
///
/// Checked by segment, as WebKit loads it: WebKit drops `.` and `..` segments, `%2e`
/// included, before the request is sent, and Foundation keeps them. claude.ai's own links
/// never have one, so a path with a dot segment is refused, as a sign-in link when that is
/// where it leads. The first segment that is not empty decides, so `//magic-link`, which a
/// server merging slashes would route to sign-in, is refused as well.
func pathRefusal(_ path: String) -> LinkRefusal? {
    let segments = path.lowercased().split(separator: "/", omittingEmptySubsequences: true)
    guard segments.contains(where: { $0 == "." || $0 == ".." }) else {
        return segments.first == "magic-link" ? .signInLink : nil
    }
    var resolved: [Substring] = []
    for segment in segments {
        switch segment {
        case ".": continue
        case "..": _ = resolved.popLast()
        default: resolved.append(segment)
        }
    }
    return resolved.first == "magic-link" ? .signInLink : .noLink
}

/// How much of a selection the Service looks through for a link. It runs on the main thread,
/// and looking through all of a 10 MB selection took 1.67 seconds.
let longestSelection = 64 * 1024

/// The link a Service request carries: the one the app sent as a link, when it sent one;
/// else the first claude.ai link in the first 64 KB of the selected text; else its first
/// link of any kind, so the refusal can say where it goes; else nil.
public func serviceLink(url: URL?, text: String?) -> String? {
    if let url { return url.absoluteString }
    guard let whole = text, !whole.isEmpty,
        let detector = try? NSDataDetector(
            types: NSTextCheckingResult.CheckingType.link.rawValue)
    else { return nil }
    let text = String(whole.prefix(longestSelection))
    let found = detector.matches(in: text, range: NSRange(text.startIndex..., in: text))
        .compactMap(\.url)
    let claude = found.first { $0.host?.lowercased() == "claude.ai" }
    return (claude ?? found.first)?.absoluteString
}

/// A link as the picker shows it: without its scheme, which is the same for every link it
/// shows and, in a fixture, not the one the link had.
public func shownLink(_ url: URL) -> String {
    let text = url.absoluteString
    guard let scheme = url.scheme, text.hasPrefix("\(scheme)://") else { return text }
    return String(text.dropFirst(scheme.count + 3))
}

/// The app an extension is inside: `Pitboard.app` for
/// `Pitboard.app/Contents/PlugIns/PitboardShare.appex`. Nil when it is inside none.
public func containingApp(of extensionURL: URL) -> URL? {
    var folder = extensionURL.standardizedFileURL.deletingLastPathComponent()
    while folder.path != "/" && !folder.path.isEmpty {
        if folder.pathExtension == "app" { return folder }
        folder = folder.deletingLastPathComponent()
    }
    return nil
}
