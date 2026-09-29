import Foundation
import PitboardLinks

/// A claude.ai link asked to be opened from outside the app, until somebody chooses the
/// account that opens it.
///
/// Every way in ends here: a pitboard link from a browser or a bookmarklet, the Service on a
/// selection, the Share extension, and **Open claude.ai Link…** in pitboard's menu. Nothing
/// here opens a window by itself. It records what was asked and counts the request, and the
/// account picker asks the person; only their choice opens a window.
@MainActor
@Observable
final class LinkModel {
    /// What the picker asks about.
    enum Request: Equatable {
        /// A link from outside, or why it will not be opened.
        case link(Result<URL, LinkRefusal>)
        /// A link to be pasted into the picker.
        case enter
    }

    /// This build's pitboard link scheme: `pitboard`, or `pitboard-debug` in a debug build,
    /// so a build from a branch never answers a link meant for the installed app.
    let scheme: String
    /// The request the picker is showing, nil once it is answered or cancelled.
    private(set) var pending: Request?
    /// Counts the requests. The menu bar item opens the picker whenever this moves.
    private(set) var requests = 0
    /// The account chosen last in this run, which the picker offers first. Never written to
    /// disk.
    private(set) var lastChosen: UUID?
    @ObservationIgnored private let web: WebModel

    init(scheme: String, web: WebModel) {
        self.scheme = scheme
        self.web = web
    }

    /// Where a claude.ai link opens: claude.ai, or a fixture's stand-in.
    var home: URL { web.web.home }

    /// A pitboard link from outside. A link of any other scheme is ignored. A pitboard link
    /// is read and checked here, and the picker shows the claude.ai link it carries or why
    /// it will not open it. A second request replaces the first.
    func receive(_ url: URL) {
        guard url.scheme?.lowercased() == scheme.lowercased() else { return }
        pending = .link(
            linkText(in: url, scheme: scheme).flatMap { claudeLink(from: $0, home: home) })
        requests += 1
    }

    /// Asks for a link to be pasted, from pitboard's own menu.
    func enter() {
        pending = .enter
        requests += 1
    }

    /// Opens `link` in `store`'s claude.ai window: the person chose that account.
    func choose(_ store: UUID, link: URL) {
        lastChosen = store
        pending = nil
        web.open(store, at: link)
    }

    /// The picker closed without a choice.
    func cancel() {
        pending = nil
    }
}
