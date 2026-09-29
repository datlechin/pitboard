import AppKit
import PitboardLinks

/// **Open in pitboard**, in the Services menu and the shortcut menu of a selection in any app
/// that offers its text to Services: a page's text or a browser's address bar.
///
/// It takes the link the app sent, or the first claude.ai link in the selected text, and
/// hands it to the account picker in this process, the same entry a pitboard link from a
/// browser reaches. Handing a pitboard link to Launch Services instead would reach whichever
/// copy of pitboard claims the scheme, not necessarily this one. It returns at once, so the
/// app that asked never waits, and it reads nothing but the pasteboard it is handed.
@MainActor
final class LinkService: NSObject {
    let links: LinkModel

    init(links: LinkModel) {
        self.links = links
    }

    /// The message `NSServices` names in the app's Info.plist, `openClaudeLink`.
    @objc func openClaudeLink(
        _ pasteboard: NSPasteboard, userData: String?,
        error: AutoreleasingUnsafeMutablePointer<NSString>
    ) {
        let url = pasteboard.string(forType: .URL).flatMap(URL.init(string:))
        guard let link = serviceLink(url: url, text: pasteboard.string(forType: .string)) else {
            error.pointee = "There is no link in the selection."
            return
        }
        links.receive(pitboardLink(opening: link, scheme: links.scheme))
    }
}
