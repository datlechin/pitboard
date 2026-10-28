import AppKit
import Foundation
import PitboardKit

// What is the app's own, removed once the model has removed Pitboard's folder.
@MainActor
final class AppRemoval {
    enum Asked: Equatable {
        // `link` the command line link Move to Trash removes too
        case trash(link: String?)
        // `help` the page the alert's Help button opens, which says the cask command
        case homebrew(help: URL?)
    }

    enum Answer: Equatable {
        case confirmed
        case declined
    }

    private let loginItem: any LoginItem
    private let defaults: UserDefaults
    private let domain: String
    private let support: URL
    private let library: URL
    private let commandLine: CommandLineTool
    private let app: URL
    private let homebrew: [URL]
    private let trash: @MainActor (URL) throws -> Void
    private let ask: @MainActor (Asked) -> Answer
    private let quit: @MainActor () -> Void
    private var finishing = false

    init(
        loginItem: any LoginItem, defaults: UserDefaults, domain: String, support: URL,
        library: URL, commandLine: CommandLineTool, app: URL, homebrew: [URL],
        trash: @escaping @MainActor (URL) throws -> Void,
        ask: @escaping @MainActor (Asked) -> Answer = AppRemoval.alert,
        quit: @escaping @MainActor () -> Void = { NSApp.terminate(nil) }
    ) {
        self.loginItem = loginItem
        self.defaults = defaults
        self.domain = domain
        self.support = support
        self.library = library
        self.commandLine = commandLine
        self.app = app
        self.homebrew = homebrew
        self.trash = trash
        self.ask = ask
        self.quit = quit
    }

    // `app` the copy whose command line `commandLine` links, `support` its records' folder
    static func live(
        loginItem: any LoginItem, defaults: UserDefaults, support: URL,
        commandLine: CommandLineTool, app: URL
    ) -> AppRemoval {
        AppRemoval(
            loginItem: loginItem, defaults: defaults,
            domain: Bundle.main.bundleIdentifier ?? "com.usepitboard.Pitboard",
            support: support, library: .libraryDirectory, commandLine: commandLine, app: app,
            homebrew: ["/opt/homebrew", "/usr/local"].map { URL(fileURLWithPath: $0) },
            trash: { try FileManager.default.trashItem(at: $0, resultingItemURL: nil) })
    }

    func finish() async {
        guard !finishing else { return }
        finishing = true
        // Nowhere is left to say a failure in.
        try? loginItem.unregister()
        try? FileManager.default.removeItem(at: support)
        if Self.installedByHomebrew(app, prefixes: homebrew) {
            let help = URL(string: macWords.homebrewHelp.link)
            _ = ask(.homebrew(help: help))
        } else {
            let link = commandLine.linksHere ? commandLine.link : nil
            if ask(.trash(link: link)) == .confirmed {
                let unlinked = link == nil ? .linked : await commandLine.unlink()
                if unlinked == .linked { try? trash(app) }
            }
        }
        // Last, as AppKit writes to them until the app stops.
        for folder in Self.leftInLibrary(by: domain) {
            try? trash(library.appendingPathComponent(folder))
        }
        defaults.removePersistentDomain(forName: domain)
        quit()
    }

    // What macOS and WebKit keep for the app, as the pitboard-app cask's zap trashes them.
    nonisolated static func leftInLibrary(by domain: String) -> [String] {
        [
            "Caches/\(domain)", "HTTPStorages/\(domain)",
            "HTTPStorages/\(domain).binarycookies",
            "Saved Application State/\(domain).savedState", "WebKit/\(domain)",
        ]
    }

    // A link under any version in the cask's Caskroom, which Sparkle leaves (docs/facts/macos.md).
    nonisolated static func installedByHomebrew(_ app: URL, prefixes: [URL]) -> Bool {
        let files = FileManager.default
        let own = app.resolvingSymlinksInPath().path
        return prefixes.contains { prefix in
            let room = prefix.appendingPathComponent("Caskroom/pitboard-app", isDirectory: true)
            let versions =
                (try? files.contentsOfDirectory(
                    at: room, includingPropertiesForKeys: nil, options: .skipsHiddenFiles))
                ?? []
            return versions.contains { version in
                let installed =
                    (try? files.contentsOfDirectory(
                        at: version, includingPropertiesForKeys: nil,
                        options: .skipsHiddenFiles)) ?? []
                return installed.contains { $0.resolvingSymlinksInPath().path == own }
            }
        }
    }

    // The first button is the default.
    static func said(_ asked: Asked) -> (title: String, message: String, buttons: [String]) {
        switch asked {
        case .trash(let link):
            return (
                macWords.moveToTrashQuestion, trashSays(link: link),
                [macWords.moveToTrash, words.window.cancel]
            )
        case .homebrew:
            return (
                macWords.removeWithHomebrew, macWords.homebrewRemovesIt, [words.window.ok]
            )
        }
    }

    // Asked with no window it belongs to: the windows may have closed while the model removed.
    static func alert(_ asked: Asked) -> Answer {
        let text = said(asked)
        let alert = NSAlert()
        alert.messageText = text.title
        alert.informativeText = text.message
        for button in text.buttons { alert.addButton(withTitle: button) }
        var help: HelpPage?
        if case .homebrew(let page?) = asked {
            help = HelpPage(page)
            alert.showsHelp = true
            alert.delegate = help
        }
        NSApp.activate()
        // The delegate is weak: kept until the alert is answered.
        let response = withExtendedLifetime(help) { alert.runModal() }
        return response == .alertFirstButtonReturn ? .confirmed : .declined
    }
}

private final class HelpPage: NSObject, NSAlertDelegate {
    let page: URL

    init(_ page: URL) {
        self.page = page
    }

    func alertShowHelp(_ alert: NSAlert) -> Bool {
        NSWorkspace.shared.open(page)
    }
}
