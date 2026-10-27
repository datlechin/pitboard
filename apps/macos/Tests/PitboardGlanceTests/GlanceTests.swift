import Foundation
import PitboardGlance
import Testing

// The file glancing.rs pins the format with, so a change there fails here too.
private let golden = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent()
    .appending(path: "crates/pitboard-ffi/src/present/glance.json")

private func goldenGlance() throws -> Glance {
    try #require(Glance(try Data(contentsOf: golden)))
}

@Test func theGoldenGlanceDecodesWhole() throws {
    let glance = try goldenGlance()
    #expect(glance.readAt == Date(timeIntervalSince1970: 1_800_000_000))
    #expect(glance.accounts.map(\.name) == ["work", "personal", "old", "main"])
    #expect(
        glance.words
            == GlanceWords(
                inUse: "In Use", resetsIn: "Resets in", resettingNow: "Resetting now",
                updated: "Updated 08:00", noAccounts: "No Accounts",
                noneInUse: "No account is in use.", switchButton: "Switch",
                openToSwitch: "Open Pitboard to switch accounts."))

    let work = glance.accounts[0]
    #expect(work.id == "35c0d1c9-6b25-5cdc-b543-64c0c0d8de46")
    #expect(work.tool == "claude")
    #expect(work.toolName == "Claude Code")
    #expect(work.inUse)
    #expect(work.standing == .inUse)
    #expect(work.plan == "Max 5x")
    #expect(work.said == "5-hour 42% used, Weekly 12%")
    #expect(work.value == "In Use")
    let fiveHour = try #require(work.limits.first)
    #expect(fiveHour.kind == .fiveHour)
    #expect(fiveHour.model == nil)
    #expect(fiveHour.name == "5-hour limit")
    #expect(fiveHour.short == "5-hour")
    #expect(fiveHour.used == 42)
    #expect(fiveHour.figure == "42%")
    #expect(fiveHour.level == .plenty)
    #expect(fiveHour.resetsAt == Date(timeIntervalSince1970: 1_800_003_600))
    #expect(fiveHour.resets == "Resets 09:00")
    #expect(fiveHour.spoken == "5-hour limit, 42 percent used, resets 09:00")

    let personal = glance.accounts[1]
    #expect(personal.plan == nil)
    #expect(personal.value == "75%")
    #expect(personal.limits.map(\.kind) == [.fiveHour, .weeklyModel])
    #expect(personal.limits.map(\.level) == [.low, .plenty])
    #expect(personal.limits.last?.model == "Fable")

    let old = glance.accounts[2]
    #expect(old.standing == .needsSignIn)
    #expect(old.limits.isEmpty)
    #expect(old.value == "Sign-in expired")

    let main = glance.accounts[3]
    #expect(main.toolName == "Codex")
    #expect(main.limits.map(\.level) == [.out, .plenty])
    #expect(main.limits.last?.resetsAt == nil)
    #expect(main.limits.last?.resets == "")
}

@Test func aGlanceOfAnotherSchemaOrNoneIsNoGlance() throws {
    let text = try String(contentsOf: golden, encoding: .utf8)
    let newer = text.replacingOccurrences(of: #""schema": 1"#, with: #""schema": 2"#)
    #expect(newer != text)
    #expect(Glance(Data(newer.utf8)) == nil)
    #expect(Glance(Data("{}".utf8)) == nil)
    #expect(Glance(Data()) == nil)
}

@Test func anAccountIsItsEntityWithTheTwoLimitsEveryToolNames() throws {
    let accounts = AccountQuery.entities(in: try goldenGlance())
    #expect(accounts.map(\.id) == (try goldenGlance()).accounts.map(\.id))

    let work = accounts[0]
    #expect(work.name == "work")
    #expect(work.tool == "Claude Code")
    #expect(work.plan == "Max 5x")
    #expect(work.inUse)
    #expect(work.usage == "5-hour 42% used, Weekly 12%")
    #expect(work.fiveHourUsed == 42)
    #expect(work.fiveHourResets == Date(timeIntervalSince1970: 1_800_003_600))
    #expect(work.weeklyUsed == 12)
    #expect(work.weeklyResets == Date(timeIntervalSince1970: 1_800_259_200))

    // a weekly limit of one model's is in its usage line, not in the weekly limit's place
    let personal = accounts[1]
    #expect(personal.weeklyUsed == nil)
    #expect(personal.usage == "5-hour 75% used, Weekly (Fable) 30%")

    let old = accounts[2]
    #expect(old.fiveHourUsed == nil)
    #expect(old.usage == "Sign-in expired")

    let main = accounts[3]
    #expect(main.weeklyUsed == 50)
    #expect(main.weeklyResets == nil)
}

@Test func theQueryFindsAccountsByIdAndLeavesOutOnesPitboardNoLongerHas() throws {
    let glance = try goldenGlance()
    let ids = ["0968925e-cecf-512d-8bcd-72d0ed816a3e", "gone"]
    #expect(AccountQuery.entities(ids, in: glance).map(\.name) == ["main"])
    #expect(AccountQuery.entities(ids, in: nil).isEmpty)
    #expect(AccountQuery.entities(in: nil).isEmpty)
}

// The system runs a widget's Switch in the app alone; asked in the extension, it says so.
@Test func aSwitchTheWidgetExtensionIsAskedForSwitchesNothingAndSaysWhere() async throws {
    let glance = try goldenGlance()
    let actions = GlanceActions.refusing(GlanceSource { glance })
    let personal = AccountQuery.entities(in: glance)[1]
    await #expect(throws: Refused(said: "Open Pitboard to switch accounts.")) {
        try await actions.switchTo(personal)
    }
}
