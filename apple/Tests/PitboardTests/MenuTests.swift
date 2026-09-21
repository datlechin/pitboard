import PitboardKit
import Testing

@testable import Pitboard

private func window(_ kind: String, _ percent: Double, resets: Int64 = 100) -> Limits {
    Limits(kind: kind, scope: nil, percent: percent, resetsAt: resets)
}

private func account(
    _ label: String, signedIn: Bool = false, switchable: Bool = true, _ windows: [Limits]
) -> Account {
    Account(
        label: label, email: "\(label)@example.com", accountUuid: label, signedIn: signedIn,
        switchable: switchable, parked: nil,
        usage: Usage(source: .live, observedAt: 0, windows: windows), stale: nil,
        staleExplanation: nil)
}

private func status(_ accounts: [Account]) -> Status {
    Status(now: 0, accounts: accounts, warnings: [])
}

@Test func theMenuBarNamesTheAccountInUseAndItsTightestLimit() {
    let read = status([
        account("work", signedIn: true, [window("session", 12), window("weekly_all", 64.4)]),
        account("personal", [window("session", 99)]),
    ])
    #expect(menuTitle(for: read) == "work 64%")
}

@Test func theMenuBarSaysOnlyItsOwnNameBeforeTheFirstRead() {
    #expect(menuTitle(for: nil) == "pitboard")
}

@Test func theAccountWithTheMostLeftIsOffered() {
    let read = status([
        account("work", signedIn: true, [window("session", 100)]),
        account("spare", [window("session", 80)]),
        account("fresh", [window("session", 5)]),
    ])
    let advice = Advice.about(read, unless: [:])
    #expect(advice?.ran == "work")
    #expect(advice?.use == "fresh")
    #expect(advice?.left == 95)
}

@Test func anAccountThatCannotBeSwitchedToIsNotOffered() {
    let read = status([
        account("work", signedIn: true, [window("session", 100)]),
        account("parked", switchable: false, [window("session", 0)]),
    ])
    #expect(Advice.about(read, unless: [:]) == nil)
}

@Test func nothingIsSaidWhenEveryAccountIsSpent() {
    let read = status([
        account("work", signedIn: true, [window("session", 100)]),
        account("spare", [window("session", 100)]),
    ])
    #expect(Advice.about(read, unless: [:]) == nil)
}

@Test func oneExhaustedWindowIsMentionedOnce() {
    let read = status([
        account("work", signedIn: true, [window("session", 100, resets: 42)]),
        account("spare", [window("session", 0)]),
    ])
    #expect(Advice.about(read, unless: ["session": 42]) == nil)
    // The next window is its own; what was said about the last one does not carry over.
    #expect(Advice.about(read, unless: ["session": 41])?.use == "spare")
}

@Test func aWeeklyLimitIsComparedWithWeeklyLimits() {
    let read = status([
        account("work", signedIn: true, [window("session", 10), window("weekly_all", 100)]),
        account("spare", [window("session", 100), window("weekly_all", 20)]),
    ])
    let advice = Advice.about(read, unless: [:])
    #expect(advice?.window.kind == "weekly_all")
    #expect(advice?.use == "spare")
}
