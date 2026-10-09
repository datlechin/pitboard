import Foundation
import Observation
import PitboardKit
import Testing

// The app's model shows what the Rust model says. What the Rust model says, and when, is
// tested in `pitboard-ffi`; these test the glue: which snapshot is shown, what is drawn
// again, on which thread a snapshot arrives, and that the bindings carry one end to end.

/// Stands in for the Rust model: hands out the snapshot a test gives it, and keeps every
/// intent sent, so nothing a test does reaches a core.
private final class StandInModel: PitboardModelProtocol, @unchecked Sendable {
    // Unchecked because every read and write holds `lock`.
    private let lock = NSLock()
    private var first: Snapshot
    private var intents: [Intent] = []
    private var stops = 0

    init(_ first: Snapshot) {
        self.first = first
    }

    var sent: [Intent] { lock.withLock { intents } }
    var stopped: Int { lock.withLock { stops } }

    func send(intent: Intent) { lock.withLock { intents.append(intent) } }
    func shutdown() { lock.withLock { stops += 1 } }
    func snapshot() -> Snapshot { lock.withLock { first } }
}

/// The account windows of a model that has none open and nothing to say of them.
private let noWindows = AccountWindowsShown(
    accounts: [], menus: [], open: [],
    waiting: WindowWaiting(windowTitle: "Account", shown: .reading(title: "Reading accounts…")),
    closing: [], deleting: [], picker: nil, downloads: [])

/// A snapshot with nothing in it but what a test says.
private func snapshot(
    _ revision: UInt64, status: Status? = nil, reading: Bool = false,
    readFailure: ReadFailure? = nil,
    window: WindowRequest = WindowRequest(serial: 0, pane: nil),
    windows: AccountWindowsShown = noWindows
) -> Snapshot {
    Snapshot(
        revision: revision, now: Int64(revision), reading: reading, updatedAt: nil,
        status: status, warnings: [], readFailure: readFailure, stuck: false, installed: nil,
        switchUnderWay: nil, quitQuestion: nil, lastSwitches: [], abandoned: nil, failure: nil,
        windowRequest: window, signingIn: nil, sheet: nil, sheetFailure: nil,
        menuBar: MenuBarText(nameAndUsage: "", usage: "", spoken: "Pitboard"), sections: [],
        showsTools: false, notices: [],
        menuNotices: MenuNotices(install: nil, switches: [], others: nil), footing: .ready,
        setup: nil, accountsShown: .reading(title: "Reading accounts…"),
        menuAccountsNote: nil, updatedMenu: "", updatedWindow: "", sheetText: nil,
        signingInText: nil, quitConfirmation: nil, failureAlert: nil,
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
        accountWindows: windows)
}

/// Claude Code accounts read at `now`, the first signed in, each measured at `measured`
/// where it was.
private func status(_ labels: String..., now: Int64 = 0, measured: Int64? = nil) -> Status {
    Status(
        now: now,
        accounts: labels.map { label in
            Account(
                id: "claude:\(label)", provider: "claude", label: label,
                qualified: "claude/\(label)", unplaced: false, email: "\(label)@example.com",
                accountId: label, signedIn: label == labels.first, switchable: true,
                parked: nil,
                usage: measured.map {
                    Usage(source: .live, observedAt: $0, windows: [], listsEveryLimit: true)
                },
                stale: nil, staleExplanation: nil)
        },
        warnings: [])
}

/// Whether reading `part` of `model` is told of a change while `change` runs.
@MainActor
private func changes<Value>(
    _ model: AppModel, _ part: KeyPath<AppModel, Value>,
    when change: () -> Void
) -> Bool {
    let told = Told()
    withObservationTracking {
        _ = model[keyPath: part]
    } onChange: {
        told.mark()
    }
    change()
    return told.happened
}

private final class Told: @unchecked Sendable {
    // Unchecked because every read and write holds `lock`.
    private let lock = NSLock()
    private var marked = false
    var happened: Bool { lock.withLock { marked } }
    func mark() { lock.withLock { marked = true } }
}

/// The app's model shows the newest snapshot it has been given and drops any that is not
/// newer: the model can tell 3 and then 5, and `snapshot()` can give one never told, so one
/// that arrives late must never put back what a newer one replaced.
@MainActor
@Test func aSnapshotThatIsNotNewerIsDropped() {
    let model = AppModel(model: StandInModel(snapshot(0)))
    #expect(model.revision == 0)
    #expect(model.status == nil)

    model.apply(snapshot(3, status: status("work")))
    #expect(model.revision == 3)
    #expect(model.status == status("work"))

    model.apply(snapshot(2, status: status("personal")))
    model.apply(snapshot(3, status: status("personal")))
    #expect(model.revision == 3)
    #expect(model.status == status("work"), "neither older nor the same revision is shown")

    model.apply(snapshot(5, status: status("personal")))
    #expect(model.revision == 5)
    #expect(model.status == status("personal"))
}

/// Each part is assigned only where it changed, so a view reading a part that stayed the
/// same is not drawn again when another part moves. What `apply` says it assigned is what a
/// test can see of the rule itself: Observation tells an observer nothing of an equal value
/// assigned again, so what is drawn again is the same with the comparison or without it.
@MainActor
@Test func onlyThePartsThatChangedAreAssigned() {
    let model = AppModel(model: StandInModel(snapshot(1, status: status("work"))))
    let reading: [PartialKeyPath<AppModel>] = [\.revision, \.now, \.reading]
    #expect(model.apply(snapshot(2, status: status("work"), reading: true)) == reading)
    let read = changes(model, \.reading) {
        #expect(model.apply(snapshot(3, status: status("work"))) == reading)
    }
    #expect(read, "a read is no longer under way")
    let more = changes(model, \.status) {
        #expect(
            model.apply(snapshot(4, status: status("work", "spare")))
                == [\.revision, \.now, \.status])
    }
    #expect(more)
    let rows = changes(model, \.sections) {
        model.apply(snapshot(5, status: status("work")))
    }
    #expect(!rows, "nothing about the rows moved")
    #expect(model.apply(snapshot(5, status: status("personal"))).isEmpty, "dropped")
}

/// The account windows are told of their part each time a snapshot shown changes it, once:
/// not of a snapshot that moves something else, nor of one dropped. Which windows close and
/// which stores go, and after which reads, is the Rust model's, tested in pitboard-ffi's
/// `model/windowing.rs`.
@MainActor
@Test func theAccountWindowsAreToldOfEachChangeOnce() {
    let model = AppModel(model: StandInModel(snapshot(0)))
    var told: [AccountWindowsShown] = []
    model.accountWindowsChanged = { told.append($0) }
    let deleting = AccountWindowsShown(
        accounts: [], menus: [], open: [], waiting: noWindows.waiting, closing: [],
        deleting: [StoreDeletion(store: "7e15c34f-69ec-55b4-9542-f1c1fe3d7085", ask: 1)],
        picker: nil, downloads: [])

    model.apply(snapshot(1, status: status("work")))
    #expect(told.isEmpty)
    model.apply(snapshot(2, status: status("work"), windows: deleting))
    #expect(told == [deleting])
    model.apply(snapshot(3, status: status("work", "spare"), windows: deleting))
    #expect(told == [deleting], "only something else moved")
    model.apply(snapshot(3, status: status("work")))
    #expect(told == [deleting], "dropped")
    model.apply(snapshot(4, status: status("work")))
    #expect(told == [deleting, noWindows])
    #expect(model.accountWindows == noWindows)
}

/// What a view asks for reaches the model as it was asked, at once, and the app quitting
/// stops it.
@MainActor
@Test func whatIsAskedReachesTheModelAsItWasAsked() {
    let standIn = StandInModel(snapshot(0))
    let model = AppModel(model: standIn)
    model.send(.switchTo(qualified: "codex/spare"))
    model.send(.closeSheet)
    #expect(standIn.sent == [.switchTo(qualified: "codex/spare"), .closeSheet])
    model.shutdown()
    #expect(standIn.stopped == 1)
}

/// The model tells its listener on a thread of its own, one snapshot after another; the
/// listener hands each to the main actor, and they arrive there in the order they were told.
@MainActor
@Test func theListenerDeliversOnTheMainActorInOrder() async throws {
    let listener = ModelListening()
    var arrived: [UInt64] = []
    var onMain = true
    listener.deliver = { snapshot in
        onMain = onMain && Thread.isMainThread
        arrived.append(snapshot.revision)
    }
    let told = Array(UInt64(1)...200)
    let notifier = Thread {
        for revision in told { try? listener.changed(snapshot: snapshot(revision)) }
    }
    notifier.start()
    let deadline = Date().addingTimeInterval(10)
    while arrived.count < told.count, Date() < deadline {
        try await Task.sleep(for: .milliseconds(10))
    }
    #expect(arrived == told)
    #expect(onMain)
}

/// What is said of time in a fixture's snapshot, in UTC, as `pitboard-ffi`'s own tests say
/// it.
private final class Utc: LocalTime {
    func clock(epoch: Int64, withWeekday: Bool) throws -> String { "\(epoch)" }
    func sameDay(first: Int64, second: Int64) throws -> Bool { true }
    func dateAndTime(epoch: Int64) throws -> String { "\(epoch)" }
}

/// The bindings carry the model end to end: a fixture's model, the real core over a machine
/// of its own that reaches nothing of this one, started from Swift, reads its accounts and
/// tells a listener written in Swift, which hands them to the app's model on the main actor.
/// Only a library built with `build-xcframework.sh --fixture` has fixtures, as CI builds it
/// before these tests.
///
/// It launches into a temporary directory of its own, which it removes, rather than the one
/// a debug build launched into a fixture keeps its world in, so it empties neither that
/// world nor the one of another run of these tests.
@MainActor
@Test(.enabled(if: !fixtureNames().isEmpty, "the library was built without fixtures"))
func aFixturesModelPublishesItsAccounts() async throws {
    let own = FileManager.default.temporaryDirectory
        .appendingPathComponent("pitboard-bindings-\(UUID().uuidString)", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: own) }
    let model = try AppModel.listening { listener in
        try PitboardModel.fixtureIn(
            name: "twoTools", temporaryDirectory: own.path, listener: listener,
            localTime: Utc())
    }
    #expect(model.status == nil, "nothing is read before it is started")
    model.send(.start)
    let deadline = Date().addingTimeInterval(30)
    while model.status == nil || model.reading, Date() < deadline {
        try await Task.sleep(for: .milliseconds(20))
    }
    model.shutdown()
    let labels = model.status?.accounts.compactMap(\.qualified) ?? []
    #expect(
        Set(labels) == [
            "claude/work", "claude/personal", "claude/old", "codex/main", "codex/spare",
        ])
    #expect(model.sections.map(\.heading) == ["Claude Code", "Codex"])
    #expect(model.revision > 0)
}
