import Foundation
import PitboardGlance
import PitboardKit
import PitboardTestSupport
import Testing

@testable import PitboardApp

private let twoTools = """
    {"schema": 1, "read_at": 1800000000, "accounts": [
      {"id": "w", "tool": "claude", "tool_name": "Claude Code", "name": "work", "in_use": true,
       "standing": "in_use", "plan": "Max 5x", "said": "5-hour 42% used, Weekly 12%",
       "limits": []},
      {"id": "p", "tool": "claude", "tool_name": "Claude Code", "name": "personal",
       "in_use": false, "standing": "ready", "plan": null, "said": "5-hour 75% used",
       "limits": []},
      {"id": "m", "tool": "codex", "tool_name": "Codex", "name": "main", "in_use": true,
       "standing": "in_use", "plan": "Plus", "said": "5-hour 10% used", "limits": []}
    ]}
    """

private func account(_ id: String) throws -> AccountEntity {
    try #require(AccountQuery.entities([id], in: Glance(Data(twoTools.utf8))).first)
}

@MainActor
private func sent(_ model: StandInModel, _ count: Int) async {
    while model.sent.count < count { await Task.yield() }
}

private func answer(
    _ request: UInt64, done: Bool, _ said: String, question: Question? = nil
) -> OutsideAnswer {
    OutsideAnswer(request: request, done: done, said: said, question: question)
}

private let quitChatGPT = Question(
    title: "Quit ChatGPT to switch?",
    message: "ChatGPT keeps using the account it started with until it quits.",
    confirm: "Quit and Switch", destructive: false, confirmIsDefault: true)

// What a confirmation was asked, from whichever thread the intent asks it on.
private final class Asked: @unchecked Sendable {
    // Unchecked because every read and write holds `lock`.
    private let lock = NSLock()
    private var asked: [Question] = []
    var questions: [Question] { lock.withLock { asked } }
    func ask(_ question: Question) { lock.withLock { asked.append(question) } }
}

@MainActor
@Test func aSwitchSaysWhatTheModelAnswered() async throws {
    let model = StandInModel(snapshot(0))
    let app = AppModel(model: model)
    let personal = try account("p")
    let switching = Task {
        try await Actions.switching(to: personal, with: app, asking: nil)
    }
    await sent(model, 1)
    #expect(model.sent == [.outside(request: 1, ask: .switch(account: "p"))])
    app.apply(
        snapshot(1, answers: [answer(1, done: true, "Switched Claude Code to personal.")]))
    #expect(try await switching.value == "Switched Claude Code to personal.")
}

// Shortcuts stops the shortcut and shows why, so what comes after it never runs on a switch
// that did not happen.
@MainActor
@Test func aSwitchTheModelRefusesStopsWithTheModelsWords() async throws {
    let model = StandInModel(snapshot(0))
    let app = AppModel(model: model)
    let old = try account("p")
    let switching = Task {
        try await Actions.switching(to: old, with: app, asking: nil)
    }
    await sent(model, 1)
    let refused = "Sign in to personal again in Pitboard to switch to it."
    app.apply(snapshot(1, answers: [answer(1, done: false, refused)]))
    await #expect(throws: Refused(said: refused)) { try await switching.value }
}

@MainActor
@Test func aSwitchAnAppHoldsAsksTheQuitQuestionThenQuitsItAndSwitches() async throws {
    let model = StandInModel(snapshot(0))
    let app = AppModel(model: model)
    let asked = Asked()
    let main = try account("m")
    let switching = Task {
        try await Actions.switching(to: main, with: app) { asked.ask($0) }
    }
    await sent(model, 1)
    let open = "Open Pitboard to switch while ChatGPT is open."
    app.apply(snapshot(1, answers: [answer(1, done: false, open, question: quitChatGPT)]))
    await sent(model, 2)
    #expect(asked.questions == [quitChatGPT])
    #expect(model.sent.last == .outside(request: 2, ask: .quitAndSwitch(account: "m")))
    app.apply(snapshot(2, answers: [answer(2, done: true, "Switched Codex to main.")]))
    #expect(try await switching.value == "Switched Codex to main.")
}

// requestConfirmation(actionName:dialog:) is macOS 15's.
@MainActor
@Test func aQuitQuestionMacOS14CannotAskSaysToOpenPitboard() async throws {
    let model = StandInModel(snapshot(0))
    let app = AppModel(model: model)
    let main = try account("m")
    let switching = Task {
        try await Actions.switching(to: main, with: app, asking: nil)
    }
    await sent(model, 1)
    let open = "Open Pitboard to switch while ChatGPT is open."
    app.apply(snapshot(1, answers: [answer(1, done: false, open, question: quitChatGPT)]))
    await #expect(throws: Refused(said: open)) { try await switching.value }
    #expect(model.sent.count == 1)
}

@MainActor
@Test func aQuitQuestionAnsweredNoQuitsNothing() async throws {
    let model = StandInModel(snapshot(0))
    let app = AppModel(model: model)
    let main = try account("m")
    let switching = Task {
        try await Actions.switching(to: main, with: app) { _ in
            throw CancellationError()
        }
    }
    await sent(model, 1)
    let open = "Open Pitboard to switch while ChatGPT is open."
    app.apply(snapshot(1, answers: [answer(1, done: false, open, question: quitChatGPT)]))
    await #expect(throws: CancellationError.self) { try await switching.value }
    #expect(model.sent.count == 1)
}

@MainActor
@Test func aSwitchPitboardDoesNotAnswerInTimeSaysItIsStillSwitching() async throws {
    let app = AppModel(model: StandInModel(snapshot(0)))
    let personal = try account("p")
    await #expect(throws: Refused(said: stillSwitching(label: "personal"))) {
        try await Actions.switching(
            to: personal, with: app, asking: nil, within: .milliseconds(10))
    }
}

@MainActor
@Test func usageIsOfTheAccountsInUseOrOfTheOneAsked() async throws {
    let model = StandInModel(snapshot(0))
    model.glanceText = twoTools
    let app = AppModel(model: model)

    let (inUse, saidInUse) = try await Actions.usage(of: nil, with: app)
    #expect(inUse.map(\.name) == ["work", "main"])
    #expect(saidInUse == "work: 5-hour 42% used, Weekly 12%\nmain: 5-hour 10% used")

    let (asked, saidAsked) = try await Actions.usage(of: try account("p"), with: app)
    #expect(asked.map(\.name) == ["personal"])
    #expect(saidAsked == "personal: 5-hour 75% used")
}

@MainActor
@Test func usageWithNoAccountInUseSaysSo() async throws {
    let model = StandInModel(snapshot(0))
    model.glanceText = #"{"schema": 1, "read_at": null, "accounts": []}"#
    let app = AppModel(model: model)
    let (accounts, said) = try await Actions.usage(of: nil, with: app)
    #expect(accounts.isEmpty)
    #expect(said == words.shortcuts.noneInUse)
}

@MainActor
@Test func usageBeforePitboardHasReadAnythingSaysSo() async throws {
    let app = AppModel(model: StandInModel(snapshot(0)))
    await #expect(throws: Refused(said: words.shortcuts.notReadYet ?? "")) {
        try await Actions.usage(of: nil, with: app, within: .milliseconds(10))
    }
}

@MainActor
@Test func openPitboardOpensTheAccountsWindow() {
    let model = StandInModel(snapshot(0))
    Actions.opening(AppModel(model: model))
    #expect(model.sent == [.show(place: .accounts)])
}

// The Rust end to end: the glance the model hands the app, and a switch asked from outside.
@MainActor
@Test(.enabled(if: !fixtureNames().isEmpty, "the library was built without fixtures"))
func aShortcutSwitchesAFixturesAccountFromItsGlance() async throws {
    let own = FileManager.default.temporaryDirectory
        .appendingPathComponent("pitboard-intents-\(UUID().uuidString)", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: own) }
    let app = try fixtureModel("twoTools", in: own)
    defer { app.shutdown() }
    app.send(.start)
    let text = try #require(await app.glance(within: .seconds(30)))
    let accounts = AccountQuery.entities(in: Glance(Data(text.utf8)))
    let personal = try #require(
        accounts.first { $0.name == "personal" && $0.tool == "Claude Code" })
    #expect(!personal.inUse)
    let said = try await Actions.switching(to: personal, with: app, asking: nil)
    #expect(said == "Switched Claude Code to personal.")
}
