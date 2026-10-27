import AppKit
import Foundation
import PitboardKit
import WebKit

/// Titled with the site that asks, or as an embedded page's when a frame inside the page
/// asks, so a frame cannot pass its words off as the site's own. A page with no window gets
/// WebKit's own answer: an alert is dismissed, a question and a file chooser are cancelled.
@MainActor
enum PageDialogs {
    static func alert(_ message: String, from frame: WKFrameInfo, in window: NSWindow?) async {
        guard let window else { return }
        let alert = dialog(message, from: frame)
        alert.addButton(withTitle: words.window.ok)
        _ = await alert.beginSheetModal(for: window)
    }

    static func confirm(_ message: String, from frame: WKFrameInfo, in window: NSWindow?) async
        -> Bool
    {
        guard let window else { return false }
        let alert = dialog(message, from: frame)
        alert.addButton(withTitle: words.window.ok)
        alert.addButton(withTitle: words.window.cancel)
        return await alert.beginSheetModal(for: window) == .alertFirstButtonReturn
    }

    static func prompt(
        _ message: String, answer: String?, from frame: WKFrameInfo, in window: NSWindow?
    ) async -> String? {
        guard let window else { return nil }
        let alert = dialog(message, from: frame)
        let field = NSTextField(string: answer ?? "")
        field.frame = NSRect(x: 0, y: 0, width: 260, height: 24)
        alert.accessoryView = field
        alert.addButton(withTitle: words.window.ok)
        alert.addButton(withTitle: words.window.cancel)
        alert.window.initialFirstResponder = field
        let answered = await alert.beginSheetModal(for: window) == .alertFirstButtonReturn
        return answered ? field.stringValue : nil
    }

    static func chooseFiles(_ parameters: WKOpenPanelParameters, in window: NSWindow?) async
        -> [URL]?
    {
        guard let window else { return nil }
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = parameters.allowsDirectories
        panel.allowsMultipleSelection = parameters.allowsMultipleSelection
        return await panel.beginSheetModal(for: window) == .OK ? panel.urls : nil
    }

    static func allowDownload(_ name: String, from host: String?, in window: NSWindow?) async
        -> Bool
    {
        guard let window else { return false }
        let question = downloadQuestion(name: name, host: host)
        let alert = NSAlert()
        alert.messageText = question.title
        alert.informativeText = question.message
        alert.addButton(withTitle: words.accountWindow.download)
        alert.addButton(withTitle: words.window.cancel)
        return await alert.beginSheetModal(for: window) == .alertFirstButtonReturn
    }

    private static func dialog(_ message: String, from frame: WKFrameInfo) -> NSAlert {
        let alert = NSAlert()
        alert.messageText = dialogTitle(
            host: frame.securityOrigin.host, mainFrame: frame.isMainFrame)
        alert.informativeText = message
        return alert
    }
}
