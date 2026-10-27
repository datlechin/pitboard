import Foundation
import Observation
import PitboardKit
import PitboardTestSupport
import Testing

// The app's model shows what the Rust model says. What the Rust model says, and when, is
// tested in `pitboard-ffi`; these test the glue: which snapshot is shown, what is drawn
// again, on which thread a snapshot arrives, and that the bindings carry one end to end.

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
    #expect(model.updatedMenu == "")

    model.apply(snapshot(3, updated: "Updated 14:05"))
    #expect(model.revision == 3)
    #expect(model.updatedMenu == "Updated 14:05")

    model.apply(snapshot(2, updated: "Updated 14:10"))
    model.apply(snapshot(3, updated: "Updated 14:10"))
    #expect(model.revision == 3)
    #expect(
        model.updatedMenu == "Updated 14:05", "neither older nor the same revision is shown")

    model.apply(snapshot(5, updated: "Updated 14:10"))
    #expect(model.revision == 5)
    #expect(model.updatedMenu == "Updated 14:10")
}

/// Each part is assigned only where it changed, so a view reading a part that stayed the
/// same is not drawn again when another part moves. What `apply` says it assigned is what a
/// test can see of the rule itself: Observation tells an observer nothing of an equal value
/// assigned again, so what is drawn again is the same with the comparison or without it.
@MainActor
@Test func onlyThePartsThatChangedAreAssigned() {
    let model = AppModel(model: StandInModel(snapshot(1, updated: "Updated 14:05")))
    let reading: [PartialKeyPath<AppModel>] = [\.revision, \.reading]
    #expect(model.apply(snapshot(2, updated: "Updated 14:05", reading: true)) == reading)
    let read = changes(model, \.reading) {
        #expect(model.apply(snapshot(3, updated: "Updated 14:05")) == reading)
    }
    #expect(read, "a read is no longer under way")
    let more = changes(model, \.updatedMenu) {
        #expect(
            model.apply(snapshot(4, updated: "Updated 14:10"))
                == [\.revision, \.updatedMenu, \.updatedWindow])
    }
    #expect(more)
    let rows = changes(model, \.sections) {
        model.apply(snapshot(5, updated: "Updated 14:05"))
    }
    #expect(!rows, "nothing about the rows moved")
    #expect(model.apply(snapshot(5, updated: "Updated 14:15")).isEmpty, "dropped")
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
    let deleting = windowsShown(
        deleting: [StoreDeletion(store: "7e15c34f-69ec-55b4-9542-f1c1fe3d7085", ask: 1)])

    model.apply(snapshot(1, updated: "Updated 14:05"))
    #expect(told.isEmpty)
    model.apply(snapshot(2, updated: "Updated 14:05", windows: deleting))
    #expect(told == [deleting])
    model.apply(snapshot(3, updated: "Updated 14:10", windows: deleting))
    #expect(told == [deleting], "only something else moved")
    model.apply(snapshot(3, updated: "Updated 14:05"))
    #expect(told == [deleting], "dropped")
    model.apply(snapshot(4, updated: "Updated 14:05"))
    #expect(told == [deleting, windowsShown()])
    #expect(model.accountWindows == windowsShown())
}

// The app's own removal starts from the model's removal, wherever Settings is.
@MainActor
@Test func theWelcomeAndTheRemovalAreShown() {
    let model = AppModel(model: StandInModel(snapshot(0)))
    let codex = WelcomeTool(
        code: "codex", name: "Codex", account: nil,
        step: .add(
            choice: Choice(title: "Add", intent: .enrol(provider: "codex"), enabled: true)))
    #expect(
        model.apply(snapshot(1, removal: .removing(title: "Uninstalling Pitboard…")))
            == [\.revision, \.removal])
    #expect(model.apply(snapshot(2, removal: .done, welcomeTools: [codex])).count == 3)
    #expect(model.removal == .done)
    #expect(model.welcome.tools == [codex])
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

// Each until the stand-in has been sent what it waits on.
@MainActor
private func sent(_ model: StandInModel, _ count: Int) async {
    while model.sent.count < count { await Task.yield() }
}

private func answer(_ request: UInt64, _ said: String) -> OutsideAnswer {
    OutsideAnswer(request: request, done: true, said: said, question: nil)
}

// A shortcut waits on its own request, whatever else the ten newest answers hold.
@MainActor
@Test func anAskIsAnsweredByItsOwnRequestsAnswer() async {
    let model = StandInModel(snapshot(0))
    let app = AppModel(model: model)
    let first = Task { await app.ask(.refresh, within: .seconds(60)) }
    await sent(model, 1)
    let second = Task { await app.ask(.switch(account: "p"), within: .seconds(60)) }
    await sent(model, 2)
    #expect(
        model.sent == [
            .outside(request: 1, ask: .refresh),
            .outside(request: 2, ask: .switch(account: "p")),
        ])
    app.apply(snapshot(1, answers: [answer(2, "Switched to personal.")]))
    #expect(await second.value == answer(2, "Switched to personal."))
    let both = [answer(2, "Switched to personal."), answer(1, "Updated")]
    app.apply(snapshot(2, answers: both))
    #expect(await first.value == answer(1, "Updated"))
}

// Shortcuts gives an action 30 seconds, so one asked of a model that never answers says so.
@MainActor
@Test func anAskNobodyAnswersInTimeIsNilAndALateAnswerIsDropped() async {
    let app = AppModel(model: StandInModel(snapshot(0)))
    #expect(await app.ask(.refresh, within: .milliseconds(10)) == nil)
    app.apply(snapshot(1, answers: [answer(1, "Updated")]))
}

@MainActor
@Test func theGlanceWaitsForTheLaunchsFirstReadAndNoLonger() async {
    let model = StandInModel(snapshot(0))
    let app = AppModel(model: model)
    #expect(await app.glance(within: .milliseconds(10)) == nil)

    let glanced = Task { await app.glance(within: .seconds(60)) }
    await Task.yield()
    model.glanceText = #"{"schema":1}"#
    app.apply(snapshot(1, glanceRevision: 1))
    #expect(await glanced.value == #"{"schema":1}"#)
    #expect(await app.glance(within: .zero) == #"{"schema":1}"#)
}

// Shortcuts reads the accounts again as they change, and a widget would reload.
@MainActor
@Test func aGlanceThatMovedIsToldOnceItsSnapshotIsShown() {
    let app = AppModel(model: StandInModel(snapshot(0)))
    var told = 0
    app.glanceChanged = { told += 1 }
    app.apply(snapshot(1))
    app.apply(snapshot(2, glanceRevision: 1))
    app.apply(snapshot(3, glanceRevision: 1))
    app.apply(snapshot(2, glanceRevision: 2))
    #expect(told == 1, "not for a revision that stayed, nor in a snapshot dropped")
    app.apply(snapshot(4, glanceRevision: 2))
    #expect(told == 2)
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
    let model = try fixtureModel("twoTools", in: own)
    #expect(model.sections.isEmpty, "nothing is read before it is started")
    model.send(.start)
    let deadline = Date().addingTimeInterval(30)
    while model.updatedWindow.isEmpty || model.reading, Date() < deadline {
        try await Task.sleep(for: .milliseconds(20))
    }
    model.shutdown()
    let labels = model.sections.flatMap(\.accounts).compactMap(\.qualified)
    #expect(
        Set(labels) == [
            "claude/work", "claude/personal", "claude/old", "codex/main", "codex/spare",
        ])
    #expect(model.sections.map(\.heading) == ["Claude Code", "Codex"])
    #expect(model.revision > 0)
}
