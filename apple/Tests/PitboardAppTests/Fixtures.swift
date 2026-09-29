import Foundation
import PitboardKit
import WebKit

@testable import PitboardApp

/// The tools as the core lists them, written out so no test asks the core for them.
let claudeCode = Tool(
    code: "claude", name: "Claude Code", program: "claude", service: "Anthropic")
let codex = Tool(code: "codex", name: "Codex", program: "codex", service: "OpenAI")
let bothTools = [claudeCode, codex]

func window(
    _ kind: String, _ percent: Double, resets: Int64? = 100, scope: String? = nil,
    active: Bool = true, length: Int64? = nil
) -> Limits {
    Limits(
        kind: kind, lengthSeconds: length, scope: scope, percent: percent, resetsAt: resets,
        severity: nil, isActive: active)
}

/// An account as the core reports one. `label` nil is a login signed in and not enrolled.
/// Switchable unless it is the one signed in, as a real one is.
func account(
    _ label: String?, of provider: String = "claude", signedIn: Bool = false,
    switchable: Bool? = nil, uuid: String? = nil, _ windows: [Limits] = []
) -> Account {
    let uuid = uuid ?? label ?? "someone"
    return Account(
        id: "\(provider):\(uuid)", provider: provider, label: label,
        qualified: label.map { "\(provider)/\($0)" }, unplaced: false,
        email: "\(label ?? uuid)@example.com", accountUuid: uuid, signedIn: signedIn,
        switchable: switchable ?? (!signedIn && label != nil), parked: nil,
        usage: Usage(source: .live, observedAt: 0, windows: windows), stale: nil,
        staleExplanation: nil, lastsSeconds: nil, lastsBurning: false)
}

/// A tool's login that belongs to no account pitboard can name, as the core reports one:
/// no label, no email, no account id, and what is wrong with it.
func unplaced(of provider: String, signedIn: Bool = false) -> Account {
    Account(
        id: "\(provider):login", provider: provider, label: nil, qualified: nil,
        unplaced: true, email: "", accountUuid: "", signedIn: signedIn, switchable: false,
        parked: nil, usage: nil, stale: "login_unreadable",
        staleExplanation: "Codex's login could not be read; run `pitboard doctor`",
        lastsSeconds: nil, lastsBurning: false)
}

func status(_ accounts: [Account], warnings: [Warning] = []) -> Status {
    Status(now: 0, accounts: accounts, warnings: warnings)
}

extension AppModel {
    /// The app's model with nothing of the Mac running the tests behind it: `service` for
    /// the core, preferences of the test's own, a login item that registers nothing, a
    /// command line that links nothing, and claude.ai windows whose stores are stand-ins.
    /// Nothing runs by itself unless `watching` says so, so a test drives every read and
    /// knows what set what.
    convenience init(
        testing service: any Core, watching: Bool = false,
        defaults: UserDefaults = TestDefaults(), commandLineTool: CommandLineTool = .nowhere,
        loginItem: any LoginItem = StandInLoginItem(), web: ClaudeWeb = .standIn()
    ) {
        self.init(
            watching: watching, service: service, defaults: defaults,
            commandLineTool: commandLineTool, loginItem: loginItem, notifies: false, web: web,
            linkScheme: Fixture.linkScheme)
    }
}

/// Stands in for WebKit's persistent stores: remembers which exist and which this pitboard
/// recorded making, in preferences of its own, and deletes one the way WebKit does, refusing
/// while a page still uses it. No test writes under ~/Library/WebKit.
@MainActor
final class StandInStores: WebStores {
    /// The stores there are, whoever made them.
    var existing: Set<UUID> = []
    /// Which of them this pitboard made, as the app records it.
    let record = WebStoreRecord(defaults: TestDefaults(), directory: "/Users/dana/.pitboard")
    /// Every store asked for, in order.
    private(set) var made: [UUID] = []
    /// Every store deleted, in order.
    private(set) var removed: [UUID] = []
    /// How many deletions from now on are refused as though a web view still used the store.
    var refusals = 0
    /// Whether a page still uses a store, which WebKit refuses to delete. A test points this
    /// at the model's pages.
    var inUse: (UUID) -> Bool = { _ in false }
    private var objects: [UUID: WKWebsiteDataStore] = [:]

    enum Refusal: Error, LocalizedError {
        case inUse
        var errorDescription: String? { "Data store is in use." }
    }

    func store(for id: UUID) -> WKWebsiteDataStore {
        made.append(id)
        record.add(id)
        existing.insert(id)
        if let object = objects[id] { return object }
        let object = WKWebsiteDataStore.nonPersistent()
        objects[id] = object
        return object
    }

    func identifiers() async -> Set<UUID> { existing }

    func recorded() -> Set<UUID> { record.ids }

    func unrecord(_ id: UUID) { record.remove(id) }

    /// Stores this pitboard made before, as the app would have recorded them.
    func made(_ ids: Set<UUID>) {
        existing.formUnion(ids)
        for id in ids { record.add(id) }
    }

    /// Runs as a deletion starts, for a test to do something while one is under way.
    var removing: (UUID) -> Void = { _ in }

    func remove(_ id: UUID) async throws {
        removing(id)
        if inUse(id) { throw Refusal.inUse }
        if refusals > 0 {
            refusals -= 1
            throw Refusal.inUse
        }
        existing.remove(id)
        objects[id] = nil
        removed.append(id)
    }
}

extension ClaudeWeb {
    /// claude.ai windows for a test: the fixture's home, `stores` for WebKit's, a downloads
    /// folder nobody writes to, and links and pauses kept in `record`, with no waiting.
    static func standIn(
        stores: StandInStores = StandInStores(), record: WebRecord = WebRecord()
    ) -> ClaudeWeb {
        ClaudeWeb(
            home: URL(string: "pitboard-fixture://claude.ai/")!,
            stores: stores,
            downloads: FileManager.default.temporaryDirectory
                .appendingPathComponent("pitboard-tests/Downloads"),
            openElsewhere: { record.open($0) },
            prepare: { _ in },
            pause: { record.pause($0) })
    }
}

/// Preferences kept in memory for one test alone, so what one test declines is not what the
/// next one reads, and nothing is written to the preferences of whoever runs the tests: a
/// suite of their own leaves a file behind in ~/Library/Preferences for every one, even once
/// it is emptied.
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

/// A login item that keeps what it is told and registers nothing, so no test puts itself
/// among the login items of whoever runs it.
@MainActor
final class StandInLoginItem: LoginItem {
    private(set) var state: LoginItemState = .disabled
    func register() throws { state = .enabled }
    func unregister() throws { state = .disabled }
    func openSystemSettings() {}
}

extension CommandLineTool {
    /// A command line that is not there: no app around it, nowhere to look for one, and a
    /// link that no script is run to make.
    static let nowhere = CommandLineTool(
        helper: nil, installPlaces: [],
        link: FileManager.default.temporaryDirectory
            .appendingPathComponent("pitboard-nowhere/bin/pitboard").path,
        execute: { _ in [NSAppleScript.errorMessage: "No script is run in a test."] })
}

/// Holds whatever comes to it until the test lets it through, so a test can make one thing
/// happen while another is still under way: a read still waiting on a service, a sign-in
/// still starting.
@MainActor
final class Gate {
    private var waiting: [CheckedContinuation<Void, Never>] = []
    private var opened = false
    /// How many have come to it so far, let through or not.
    private(set) var arrivals = 0

    /// Waits here until the test lets it through, or goes straight on once it is open.
    func pass() async {
        arrivals += 1
        guard !opened else { return }
        await withCheckedContinuation { waiting.append($0) }
    }

    /// Lets through the one that has waited longest, and nobody else.
    func letOneThrough() {
        guard !waiting.isEmpty else { return }
        waiting.removeFirst().resume()
    }

    /// Lets everyone through, those waiting and those still to come.
    func open() {
        opened = true
        let all = waiting
        waiting = []
        for waiter in all { waiter.resume() }
    }
}

/// Stands in for AppleScript: keeps each script it is handed and raises what it is told to,
/// so no test runs one that asks for an administrator's password.
final class Scripts: @unchecked Sendable {
    // Unchecked because the tool runs scripts off the main thread; every read and write
    // holds `lock`.
    private let lock = NSLock()
    private var kept: [String] = []
    private var raising: NSDictionary?

    /// Every script handed in, oldest first.
    var ran: [String] { lock.withLock { kept } }

    /// What each script from now on raises: AppleScript's error number, and its message.
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
