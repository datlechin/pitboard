import Foundation
import PitboardKit
import WebKit

@testable import PitboardApp

// Kept in memory: a defaults suite of its own leaves a file in ~/Library/Preferences for
// every test, even once it is emptied.
final class TestDefaults: UserDefaults, @unchecked Sendable {
    // Unchecked because a test may hand it to code off the main thread; every read and write
    // holds `lock`.
    private let lock = NSLock()
    private var values: [String: Any] = [:]

    init() {
        super.init(suiteName: nil)!
    }

    override func object(forKey key: String) -> Any? { lock.withLock { values[key] } }
    override func set(_ value: Any?, forKey key: String) {
        lock.withLock { values[key] = value }
    }
    override func set(_ value: Bool, forKey key: String) {
        lock.withLock { values[key] = value }
    }
    override func removeObject(forKey key: String) { lock.withLock { values[key] = nil } }
    override func string(forKey key: String) -> String? { object(forKey: key) as? String }
    override func stringArray(forKey key: String) -> [String]? {
        object(forKey: key) as? [String]
    }
    override func bool(forKey key: String) -> Bool { object(forKey: key) as? Bool ?? false }
}

// Keeps each script in place of running it, so no test runs one that asks for an
// administrator's password.
final class Scripts: @unchecked Sendable {
    // Unchecked because the tool runs scripts off the main thread; every read and write
    // holds `lock`.
    private let lock = NSLock()
    private var kept: [String] = []
    private var raising: NSDictionary?

    var ran: [String] { lock.withLock { kept } }

    func raise(_ number: Int?, _ message: String? = nil) {
        var error: [String: Any] = [:]
        error[NSAppleScript.errorNumber] = number
        error[NSAppleScript.errorMessage] = message
        lock.withLock { raising = number == nil ? nil : error as NSDictionary }
    }

    func run(_ source: String) -> NSDictionary? {
        lock.withLock {
            kept.append(source)
            return raising
        }
    }
}

private struct InUse: Error {}

// Refuses a deletion as often as a test says, as a store a window or WebKit's network
// process still holds does.
@MainActor
final class StandInStores: WebsiteDataStores {
    var refusals: [UUID: Int] = [:]
    private(set) var removed: [UUID] = []
    private(set) var attempts: [UUID: Int] = [:]
    private var stores: [UUID: WKWebsiteDataStore] = [:]

    func store(for id: UUID) -> WKWebsiteDataStore {
        if let store = stores[id] { return store }
        let store = WKWebsiteDataStore.nonPersistent()
        stores[id] = store
        return store
    }

    func remove(_ id: UUID) async throws {
        attempts[id, default: 0] += 1
        if let left = refusals[id], left > 0 {
            refusals[id] = left - 1
            throw InUse()
        }
        stores[id] = nil
        removed.append(id)
    }
}

// The account windows of `model`, with nothing reaching WebKit's own stores.
@MainActor
func standInWindows(_ model: AppModel, stores: StandInStores = StandInStores())
    -> AccountWindows
{
    let environment = WebEnvironment(
        scheme: WebEnvironment.fixtureScheme, stores: stores,
        downloads: FileManager.default.temporaryDirectory, openElsewhere: { _ in },
        configure: { _ in }, pause: { _ in })
    return AccountWindows(
        model: model, environment: environment,
        presence: AppPresence(apply: { _ in }, bringForward: {}, settle: {}))
}

// A copy of the app with its command line inside, in a folder whose name has a space and a
// quote in it, as a folder somebody made might.
struct StandInApp {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("pitboard's app \(UUID().uuidString)")
    var bundle: URL { root.appendingPathComponent("Pitboard.app") }
    var helper: URL { bundle.appendingPathComponent("Contents/Helpers/pitboard") }
    var link: String { root.appendingPathComponent("bin/pitboard").path }

    init() throws {
        try FileManager.default.createDirectory(
            at: helper.deletingLastPathComponent(), withIntermediateDirectories: true)
        try program(at: helper)
    }

    func directory(_ name: String) throws -> String {
        let made = root.appendingPathComponent(name)
        try FileManager.default.createDirectory(at: made, withIntermediateDirectories: true)
        return made.path
    }

    func program(at url: URL, runnable: Bool = true) throws {
        try Data("#!/bin/sh\n".utf8).write(to: url)
        try FileManager.default.setAttributes(
            [.posixPermissions: runnable ? 0o755 : 0o644], ofItemAtPath: url.path)
    }

    func symlink(_ path: String, to destination: String) throws {
        try FileManager.default.createSymbolicLink(
            atPath: path, withDestinationPath: destination)
    }

    func tool(_ execute: @escaping CommandLineTool.Runner) -> CommandLineTool {
        CommandLineTool(helper: helper.path, link: link, execute: execute)
    }

    // What the script run as an administrator does.
    func makeLink() {
        _ = try? directory("bin")
        try? symlink(link, to: helper.path)
    }

    func remove() { try? FileManager.default.removeItem(at: root) }
}
