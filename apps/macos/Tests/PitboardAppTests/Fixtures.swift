import Foundation
import PitboardKit

@testable import PitboardApp

extension Site {
    /// The sites as the core declares them: what each test of a window's rules runs against.
    static var claude: Site { sitesFor(provider: "claude")[0] }
    static var chatGPT: Site { sitesFor(provider: "codex")[0] }
}

extension SiteLink {
    /// `text` checked as a link from outside, as the core checks one.
    init(_ text: String) throws {
        self = try siteLink(text: text)
    }
}

/// An account as the core reports one. `label` nil is a login signed in and not enrolled.
/// Switchable unless it is the one signed in, and its numbers every limit it has where its
/// tool is Claude Code, as a real one is.
func account(
    _ label: String?, of provider: String = "claude", email: String? = nil,
    signedIn: Bool = false, switchable: Bool? = nil, uuid: String? = nil
) -> Account {
    let uuid = uuid ?? label ?? "someone"
    return Account(
        id: "\(provider):\(uuid)", provider: provider, label: label,
        qualified: label.map { "\(provider)/\($0)" }, unplaced: false,
        email: email ?? "\(label ?? uuid)@example.com", accountId: uuid, signedIn: signedIn,
        switchable: switchable ?? (!signedIn && label != nil), parked: nil,
        usage: Usage(
            source: .live, observedAt: 0, windows: [], listsEveryLimit: provider == "claude"),
        stale: nil, staleExplanation: nil)
}

func status(_ accounts: [Account], warnings: [Warning] = []) -> Status {
    Status(now: 0, accounts: accounts, warnings: warnings)
}

/// Stands in for the Rust model: hands out the snapshot a test gives it, and keeps every
/// intent sent, so nothing a test does reaches a core or the machine running it.
final class StandInModel: PitboardModelProtocol, @unchecked Sendable {
    // Unchecked because every read and write holds `lock`.
    private let lock = NSLock()
    private let first: Snapshot
    private var intents: [Intent] = []

    init(_ first: Snapshot) {
        self.first = first
    }

    /// Every intent sent, oldest first.
    var sent: [Intent] { lock.withLock { intents } }

    func send(intent: Intent) { lock.withLock { intents.append(intent) } }
    func shutdown() {}
    func snapshot() -> Snapshot { first }
}

/// What a window waiting for its page shows while the accounts are read, as the model says.
let readingAccounts = WindowWaiting(
    windowTitle: "Account", shown: .reading(title: "Reading accounts…"))

/// The account windows' part of a snapshot: the windows and menus the model gives
/// `accounts`, and whatever else a test says.
func windowsShown(
    _ accounts: [Account] = [], open: [OpenWindow] = [], closing: [String] = [],
    deleting: [StoreDeletion] = [], picker: LinkPicker? = nil, downloads: [DownloadShown] = []
) -> AccountWindowsShown {
    AccountWindowsShown(
        accounts: windowAccounts(accounts: accounts), menus: siteMenus(accounts: accounts),
        open: open, waiting: readingAccounts, closing: closing, deleting: deleting,
        picker: picker, downloads: downloads)
}

/// A snapshot with nothing in it but what a test says, as a model that has read nothing yet
/// would make one, numbered `revision`. Its account windows are those of `status`'s
/// accounts unless a test says otherwise.
func snapshot(
    _ revision: UInt64, status: Status? = nil, readFailure: ReadFailure? = nil,
    window: WindowRequest = WindowRequest(serial: 0, pane: nil),
    windows: AccountWindowsShown? = nil
) -> Snapshot {
    Snapshot(
        revision: revision, now: 0, reading: false, updatedAt: nil, status: status,
        warnings: [], readFailure: readFailure, stuck: false, installed: nil,
        switchUnderWay: nil, quitQuestion: nil, lastSwitches: [], abandoned: nil, failure: nil,
        windowRequest: window, signingIn: nil, sheet: nil, sheetFailure: nil,
        menuBar: MenuBarText(nameAndUsage: "", usage: "", spoken: "Pitboard"), sections: [],
        showsTools: false, notices: [],
        menuNotices: MenuNotices(install: nil, switches: [], others: nil), footing: .ready,
        setup: nil, accountsShown: .reading(title: "Reading accounts…"),
        menuAccountsNote: nil, updatedMenu: "", updatedWindow: "", sheetText: nil,
        signingInText: nil, stowText: nil, quitConfirmation: nil, failureAlert: nil,
        machine: MachineShown(
            schedule: ScheduleShown(
                schedule: nil, on: false, changing: false, enabled: true, runs: nil,
                scheduledIn: nil, note: nil, failed: nil),
            renewal: RenewalShown(renewing: false, note: ""),
            checks: ChecksShown(
                lines: [], summary: nil, checking: false, checked: nil, waiting: nil),
            activity: ActivityShown(lines: [], empty: nil),
            commandLine: CommandLineShown(
                found: nil, inTerminal: nil, updateNote: nil, offersLink: false,
                cannotLink: nil),
            autoSwitch: AutoSwitchShown(
                on: false, at: 95, lowest: 50, highest: 99, enabled: true,
                atLabel: "Switch when a limit reaches 95%", note: "", standing: nil)),
        accountWindows: windows ?? windowsShown(status?.accounts ?? []))
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
