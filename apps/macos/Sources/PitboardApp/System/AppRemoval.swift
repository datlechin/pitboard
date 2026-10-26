import AppKit
import Foundation
import PitboardKit

// What is the app's own, removed once the model has removed Pitboard's folder.
@MainActor
final class AppRemoval {
    enum Asked: Equatable {
        // `link` the command line link Move to Trash removes too
        case trash(link: String?)
        case homebrew
    }

    enum Answer: Equatable {
        case confirmed
        case learnMore
        case declined
    }

    private let loginItem: any LoginItem
    private let defaults: UserDefaults
    private let domain: String
    private let support: URL
    private let commandLine: CommandLineTool
    private let app: URL
    private let homebrew: [URL]
    private let trash: @MainActor (URL) throws -> Void
    private let ask: @MainActor (Asked) -> Answer
    private let open: @MainActor (URL) -> Void
    private let quit: @MainActor () -> Void
    private var finishing = false

    init(
        loginItem: any LoginItem, defaults: UserDefaults, domain: String, support: URL,
        commandLine: CommandLineTool, app: URL, homebrew: [URL],
        trash: @escaping @MainActor (URL) throws -> Void,
        ask: @escaping @MainActor (Asked) -> Answer = AppRemoval.alert,
        open: @escaping @MainActor (URL) -> Void = { NSWorkspace.shared.open($0) },
        quit: @escaping @MainActor () -> Void = { NSApp.terminate(nil) }
    ) {
        self.loginItem = loginItem
        self.defaults = defaults
        self.domain = domain
        self.support = support
        self.commandLine = commandLine
        self.app = app
        self.homebrew = homebrew
        self.trash = trash
        self.ask = ask
        self.open = open
        self.quit = quit
    }

    static func live(
        loginItem: any LoginItem, defaults: UserDefaults, commandLine: CommandLineTool,
        bundle: Bundle = .main
    ) -> AppRemoval {
        AppRemoval(
            loginItem: loginItem, defaults: defaults,
            domain: bundle.bundleIdentifier ?? "com.usepitboard.Pitboard",
            support: WebEnvironment.recordsDirectory(bundle: bundle), commandLine: commandLine,
            app: bundle.bundleURL,
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
            if ask(.homebrew) == .learnMore, let help = words.settings.homebrewHelp,
                let page = URL(string: help.link)
            {
                open(page)
            }
        } else {
            let link = commandLine.linksHere ? commandLine.link : nil
            if ask(.trash(link: link)) == .confirmed {
                let unlinked = link == nil ? .linked : await commandLine.unlink()
                if unlinked == .linked { try? trash(app) }
            }
        }
        // Last, as AppKit writes to them until the app stops.
        defaults.removePersistentDomain(forName: domain)
        quit()
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
        let settings = words.settings
        switch asked {
        case .trash(let link):
            return (
                settings.moveToTrashQuestion ?? "", trashSays(link: link),
                [settings.moveToTrash ?? "", words.window.cancel]
            )
        case .homebrew:
            return (
                settings.removeWithHomebrew ?? "", settings.homebrewRemovesIt ?? "",
                [words.window.ok] + [settings.homebrewHelp?.title].compactMap { $0 }
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
        NSApp.activate()
        switch alert.runModal() {
        case .alertFirstButtonReturn: return .confirmed
        case .alertSecondButtonReturn: return asked == .homebrew ? .learnMore : .declined
        default: return .declined
        }
    }
}
