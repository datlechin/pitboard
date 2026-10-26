import Foundation
import PitboardKit
import Testing

@testable import PitboardApp

/// A renewal schedule an app up to 0.3.0 wrote starts the app with `renew`, and the command
/// line inside this one does the renewal in its place. A copy with none inside it starts
/// nothing, rather than a menu bar app that would repair the schedule from inside its job.
@Test func aScheduleThatStartsTheAppRenewsWithTheCommandLineInsideIt() {
    let app = "/Applications/Pitboard.app/Contents/MacOS/Pitboard"
    let helper = "/Applications/Pitboard.app/Contents/Helpers/pitboard"
    #expect(Launch.action(for: [app, "renew"], helper: helper) == .renew(helper))
    #expect(Launch.action(for: [app, "renew"], helper: nil) == .fail)

    #expect(Launch.action(for: [app], helper: helper) == .app)
    #expect(Launch.action(for: [], helper: helper) == .app)
    #expect(Launch.action(for: [app, "renew", "--json"], helper: helper) == .app)
    #expect(Launch.action(for: [app, "-AppleLanguages", "(en)"], helper: helper) == .app)
}

/// Stands in for both the Pitboard directory and the app's own folder.
private struct ScratchDirectory {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("pitboard-earlier-\(UUID().uuidString)")
    var appFile: URL { root.appendingPathComponent("app.json") }
    var windowsFile: URL { root.appendingPathComponent("windows.json") }

    init() throws {
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    func store(_ defaults: UserDefaults) -> EarlierStore {
        EarlierStore(defaults: defaults, appFile: appFile, windowsFile: windowsFile)
    }

    /// What the model writes once it has taken what UserDefaults held.
    func keep() throws {
        try Data(#"{"second_account_declined":["claude"],"has_been_seen":true}"#.utf8)
            .write(to: appFile)
    }

    /// What the model writes once it has taken the account windows' records.
    func keepWindows() throws {
        try Data(#"{"stores":{},"pages":{}}"#.utf8).write(to: windowsFile)
    }

    func remove() { try? FileManager.default.removeItem(at: root) }
}

/// What this app kept in UserDefaults before the model kept its preferences in Pitboard's
/// directory is handed over as it was, with the old nudge's key beside the others, for the
/// model to read as it reads them.
@Test func whatUserDefaultsHeldIsHandedOverAsItWas() throws {
    let directory = try ScratchDirectory()
    defer { directory.remove() }
    let defaults = TestDefaults()
    let store = directory.store(defaults)
    #expect(store.handOver() == nil, "nothing was ever set")

    defaults.set(["codex"], forKey: "secondAccountDeclined")
    defaults.set(true, forKey: "hasBeenSeen")
    defaults.set(true, forKey: "hideSecondAccountNudge")
    #expect(
        store.handOver()
            == EarlierPreferences(
                secondAccountDeclined: ["codex"], hasBeenSeen: true,
                secondAccountNudgeHidden: true))
    #expect(store.handOver() != nil, "kept until the model has kept them")
}

/// The old keys go once the model has kept what they held in `app.json`, at launch or as the
/// app quits once the model has stopped, and not before: a launch that stopped before the
/// file was written hands them over again. Where the file is there, nothing is handed over,
/// since the model reads the file and nothing else.
@Test func theOldKeysGoOnceTheModelHasKeptThem() throws {
    let directory = try ScratchDirectory()
    defer { directory.remove() }
    let defaults = TestDefaults()
    defaults.set(["codex"], forKey: "secondAccountDeclined")
    defaults.set(true, forKey: "hasBeenSeen")
    defaults.set(true, forKey: "menuBarShows")
    defaults.set(["/Users/dana/.pitboard": ["X"]], forKey: "webStores")
    let store = directory.store(defaults)

    #expect(!store.forgetOnceKept())
    #expect(defaults.object(forKey: "hasBeenSeen") != nil)

    try directory.keep()
    #expect(store.forgetOnceKept())
    for key in ["secondAccountDeclined", "hasBeenSeen", "hideSecondAccountNudge"] {
        #expect(defaults.object(forKey: key) == nil, "\(key)")
    }
    #expect(defaults.object(forKey: "menuBarShows") != nil, "the app's own stay")
    #expect(defaults.object(forKey: "webStores") != nil, "the windows' records' own file")

    defaults.set(true, forKey: "hasBeenSeen")
    #expect(store.handOver() == nil)
    #expect(defaults.object(forKey: "hasBeenSeen") == nil)
}

/// The account windows' records this app kept in UserDefaults, exactly as its `StoreRecord`
/// and `PageRecord` wrote them, are handed over as they were: every Pitboard directory's by
/// its standardised path, and each store as `UUID.uuidString` writes it, in upper case, which
/// the model takes in any case.
@Test func theWindowsRecordsAreHandedOverAsTheAppWroteThem() throws {
    let directory = try ScratchDirectory()
    defer { directory.remove() }
    let defaults = TestDefaults()
    let store = directory.store(defaults)
    #expect(store.handOverWindows() == nil, "nothing was ever set")

    let work = UUID(uuidString: "7e15c34f-69ec-55b4-9542-f1c1fe3d7085")!
    let personal = UUID(uuidString: "323d12fb-2c52-55b5-baec-df74fb60bc24")!
    let scratch = UUID(uuidString: "8ff9e0e6-a7e2-53e2-a594-6e53ddadd38a")!
    // As StoreRecord.save and PageRecord.save wrote them, at a3e5ce0.
    let stores: [String: [String]] = [
        "/Users/dana/.pitboard": [work, personal].map(\.uuidString).sorted(),
        "/tmp/test/.pitboard": [scratch.uuidString],
    ]
    let pages: [String: [String: String]] = [
        "/Users/dana/.pitboard": [work.uuidString: "https://claude.ai/chat/1"]
    ]
    defaults.set(stores, forKey: "webStores")
    defaults.set(pages, forKey: "windowPages")
    #expect(stores["/Users/dana/.pitboard"]?.first == "323D12FB-2C52-55B5-BAEC-DF74FB60BC24")
    #expect(store.handOverWindows() == EarlierWindowRecords(stores: stores, pages: pages))
    #expect(store.handOverWindows() != nil, "kept until the model has kept them")
}

/// The windows' old keys go once the model has kept what they held in `windows.json`, at
/// launch or as the app quits, and not before, apart from the preferences' keys, which go
/// with `app.json`.
@Test func theOldWindowKeysGoOnceTheModelHasKeptThem() throws {
    let directory = try ScratchDirectory()
    defer { directory.remove() }
    let defaults = TestDefaults()
    defaults.set(["/Users/dana/.pitboard": ["X"]], forKey: "webStores")
    defaults.set(["/Users/dana/.pitboard": ["X": "https://claude.ai/"]], forKey: "windowPages")
    defaults.set(true, forKey: "hasBeenSeen")
    let store = directory.store(defaults)

    #expect(!store.forgetWindowsOnceKept())
    #expect(defaults.object(forKey: "webStores") != nil)

    try directory.keepWindows()
    #expect(store.forgetWindowsOnceKept())
    #expect(defaults.object(forKey: "webStores") == nil)
    #expect(defaults.object(forKey: "windowPages") == nil)
    #expect(defaults.object(forKey: "hasBeenSeen") != nil, "the preferences' own file")
    #expect(store.handOverWindows() == nil)
}

@Test func theStoreIsInThePitboardDirectoryTheCoreReads() {
    let store = EarlierStore(
        defaults: TestDefaults(),
        environment: ["HOME": "/Users/dana", "PITBOARD_HOME": "/Users/dana/elsewhere"],
        windowsDirectory: URL(fileURLWithPath: "/Users/dana/Library/Application Support/app"))
    #expect(store.appFile.path == "/Users/dana/elsewhere/app.json")
    #expect(
        store.windowsFile.path == "/Users/dana/Library/Application Support/app/windows.json")
}

#if DEBUG
    /// The UI tests launch a fixture by setting this variable to one of these names, from a
    /// list of their own in `UITests/Launching.swift`, so a name changed in the core's
    /// fixtures has to change there. The names are checked against a library that has them.
    @Test func theUITestsNameEveryFixtureAsTheAppDoes() {
        #expect(Fixture.variable == "PITBOARD_FIXTURE")
        let launched = [
            "twoTools", "oneTool", "empty", "firstLaunch", "noClaudeCode", "unnamed",
            "onlyOne", "readFailure", "stuck", "chatGPTOpen", "claudeDesktop",
            "claudeDesktopOpen", "warmUp", "leftInFile", "loginReplaced", "switchedByItself",
        ]
        if !fixtureNames().isEmpty {
            #expect(fixtureNames() == launched)
        }
    }

    @Test func aFixtureLinksItsCommandLineWhereItsModelLooks() {
        #expect(
            Fixture.folder(environment: ["TMPDIR": "/private/tmp/x/"]).path
                == "/private/tmp/x/pitboard-fixture")
        #expect(
            Fixture.folder(environment: [:])
                == FileManager.default.temporaryDirectory.appendingPathComponent(
                    "pitboard-fixture", isDirectory: true))
        let tool = Fixture.commandLineTool(in: URL(fileURLWithPath: "/private/tmp/x/f"))
        #expect(tool.helper == "/private/tmp/x/f/Pitboard.app/Contents/Helpers/pitboard")
        #expect(tool.link == "/private/tmp/x/f/bin/pitboard")
    }
#endif
