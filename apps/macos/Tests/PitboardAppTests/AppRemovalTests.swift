import Foundation
import PitboardKit
import Testing

@testable import PitboardApp

// A scratch copy of the app, its own Application Support and Library folders, defaults and
// login item: nothing here reaches the Mac's own.
@MainActor
private final class Removing {
    let app: StandInApp
    let defaults = TestDefaults()
    let support: URL
    let library: URL
    let homebrew: URL
    let loginItem = StandInLoginItem()
    let scripts = Scripts()
    var answer: AppRemoval.Answer = .confirmed
    private(set) var asked: [AppRemoval.Asked] = []
    private(set) var trashed: [URL] = []
    private(set) var trashedWhenAsked: [Int] = []
    private(set) var quit = 0

    init() throws {
        app = try StandInApp()
        support = URL(fileURLWithPath: try app.directory("Application Support/app"))
        library = URL(fileURLWithPath: try app.directory("Library"))
        homebrew = app.root.appendingPathComponent("homebrew", isDirectory: true)
        try Data("{}".utf8).write(to: support.appendingPathComponent("windows.json"))
        defaults.set("limits", forKey: DefaultsKey.settingsTab)
    }

    var removal: AppRemoval {
        AppRemoval(
            loginItem: loginItem, defaults: defaults, domain: "com.usepitboard.Pitboard.test",
            support: support, library: library, commandLine: app.tool(scripts.run),
            app: app.bundle,
            homebrew: [homebrew], trash: { self.trashed.append($0) },
            ask: {
                self.asked.append($0)
                self.trashedWhenAsked.append(self.trashed.count)
                return self.answer
            }, quit: { self.quit += 1 })
    }

    // The pitboard-app cask zap's Library folders, under the test's bundle id.
    var leftInLibrary: [URL] {
        [
            "Caches/com.usepitboard.Pitboard.test",
            "HTTPStorages/com.usepitboard.Pitboard.test",
            "HTTPStorages/com.usepitboard.Pitboard.test.binarycookies",
            "Saved Application State/com.usepitboard.Pitboard.test.savedState",
            "WebKit/com.usepitboard.Pitboard.test",
        ].map { library.appendingPathComponent($0) }
    }

    // As Homebrew 7.0.9 left pitboard-app 0.4.0 (docs/facts/macos.md).
    func installWithHomebrew(linkingTo bundle: URL? = nil) throws {
        let version = homebrew.appendingPathComponent("Caskroom/pitboard-app/0.4.0")
        try FileManager.default.createDirectory(
            at: version, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(
            at: homebrew.appendingPathComponent("Caskroom/pitboard-app/.metadata"),
            withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(
            at: version.appendingPathComponent("Pitboard.app"),
            withDestinationURL: bundle ?? app.bundle)
    }
}

@MainActor
final class StandInLoginItem: LoginItem {
    private(set) var state: LoginItemState = .enabled
    func register() throws { state = .enabled }
    func unregister() throws { state = .disabled }
    func openSystemSettings() {}
}

@MainActor
@Test func theAppRemovesWhatIsItsOwnThenMovesItselfToTheTrash() async throws {
    let removing = try Removing()
    defer { removing.app.remove() }
    await removing.removal.finish()
    #expect(removing.loginItem.state == .disabled)
    #expect(!FileManager.default.fileExists(atPath: removing.support.path))
    #expect(removing.asked == [.trash(link: nil)])
    #expect(removing.scripts.ran.isEmpty, "no link of its own to remove")
    #expect(removing.trashed == [removing.app.bundle] + removing.leftInLibrary)
    #expect(removing.defaults.removedDomains == ["com.usepitboard.Pitboard.test"])
    #expect(removing.defaults.string(forKey: DefaultsKey.settingsTab) == nil)
    #expect(removing.quit == 1)
}

@MainActor
@Test func itsCommandLineLinkGoesWithItUnlessThePasswordIsRefused() async throws {
    let removing = try Removing()
    defer { removing.app.remove() }
    removing.app.makeLink()
    await removing.removal.finish()
    #expect(removing.asked == [.trash(link: removing.app.link)])
    #expect(
        removing.scripts.ran == [
            CommandLineTool.script(removing: removing.app.link, to: removing.app.helper.path)
        ])
    #expect(removing.trashed == [removing.app.bundle] + removing.leftInLibrary)

    let refused = try Removing()
    defer { refused.app.remove() }
    refused.app.makeLink()
    refused.scripts.raise(-128, "User canceled.")
    await refused.removal.finish()
    #expect(
        refused.trashed == refused.leftInLibrary,
        "a link left reaching nothing is worse than the app")
    #expect(refused.defaults.removedDomains.count == 1)
    #expect(refused.quit == 1)
}

@MainActor
@Test func keepingTheAppStillRemovesWhatIsItsOwnAndQuits() async throws {
    let removing = try Removing()
    defer { removing.app.remove() }
    removing.app.makeLink()
    removing.answer = .declined
    await removing.removal.finish()
    #expect(removing.trashedWhenAsked == [0], "the app writes to them until it stops")
    #expect(removing.trashed == removing.leftInLibrary)
    #expect(removing.scripts.ran.isEmpty)
    #expect(removing.loginItem.state == .disabled)
    #expect(removing.defaults.removedDomains.count == 1)
    #expect(removing.quit == 1)
}

@MainActor
@Test func aCopyHomebrewInstalledIsLeftToHomebrew() async throws {
    let removing = try Removing()
    defer { removing.app.remove() }
    try removing.installWithHomebrew()
    removing.app.makeLink()
    await removing.removal.finish()
    let help = try #require(words.settings.homebrewHelp.flatMap { URL(string: $0.link) })
    #expect(removing.asked == [.homebrew(help: help)], "its Help button says the cask command")
    #expect(removing.trashedWhenAsked == [0])
    #expect(removing.trashed == removing.leftInLibrary, "the app's own, whoever installed it")
    #expect(removing.scripts.ran.isEmpty, "Homebrew removes its own links")
    #expect(!FileManager.default.fileExists(atPath: removing.support.path))
    #expect(removing.quit == 1)
}

@MainActor
@Test func onlyTheCopyHomebrewLinksIsHomebrews() throws {
    let removing = try Removing()
    defer { removing.app.remove() }
    #expect(!AppRemoval.installedByHomebrew(removing.app.bundle, prefixes: [removing.homebrew]))
    try removing.installWithHomebrew(
        linkingTo: removing.app.root.appendingPathComponent("Other.app"))
    #expect(
        !AppRemoval.installedByHomebrew(removing.app.bundle, prefixes: [removing.homebrew]),
        "another copy")
    let other = try Removing()
    defer { other.app.remove() }
    try other.installWithHomebrew()
    #expect(AppRemoval.installedByHomebrew(other.app.bundle, prefixes: [other.homebrew]))
}

@MainActor
@Test func theAppRemovesItselfOnce() async throws {
    let removing = try Removing()
    defer { removing.app.remove() }
    let removal = removing.removal
    await removal.finish()
    await removal.finish()
    #expect(removing.asked.count == 1)
    #expect(removing.quit == 1)
}

@MainActor
@Test func theAlertsSayTheModelsWords() {
    let trash = AppRemoval.said(.trash(link: "/usr/local/bin/pitboard"))
    #expect(trash.title == words.settings.moveToTrashQuestion)
    #expect(trash.message == trashSays(link: "/usr/local/bin/pitboard"))
    #expect(trash.buttons == [words.settings.moveToTrash, words.window.cancel])
    let homebrew = AppRemoval.said(.homebrew(help: nil))
    #expect(homebrew.title == words.settings.removeWithHomebrew)
    #expect(homebrew.message == words.settings.homebrewRemovesIt)
    #expect(homebrew.buttons == [words.window.ok], "Help is the alert's own button")
}
