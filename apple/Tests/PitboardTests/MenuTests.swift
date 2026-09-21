import PitboardKit
import Testing

@testable import Pitboard

private func window(
    _ kind: String, _ percent: Double, resets: Int64 = 100, scope: String? = nil,
    active: Bool = true
) -> Limits {
    Limits(
        kind: kind, scope: scope, percent: percent, resetsAt: resets, severity: nil,
        isActive: active)
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

/// Before the first read there is nothing true to say, and the icon is already there, so
/// the item shows the mark alone rather than a name it has not checked.
@Test func theMenuBarSaysNothingBeforeTheFirstRead() {
    #expect(menuTitle(for: nil).isEmpty)
}

/// The bar belongs to everything else running too.
@Test func aLongLabelIsBounded() {
    let read = status([
        account("a-very-long-account-label", signedIn: true, [window("session", 5)])
    ])
    #expect(menuTitle(for: read).count <= 17)
    #expect(menuTitle(for: read).hasSuffix("5%"))
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

/// A limit scoped to one model is not the account's limit. Reading 98% in the menu bar
/// while the binding limit is at 30% says the day is over when it is not.
@Test func aScopedLimitDoesNotTakeTheMenuBar() {
    let read = status([
        account(
            "work", signedIn: true,
            [
                window("session", 30),
                window("weekly_scoped", 98, scope: "Fable"),
            ])
    ])
    #expect(menuTitle(for: read) == "work 30%")
}

/// Among the account's own limits, the one it is working against wins, and failing that
/// the fullest.
@Test func theLimitInUseIsThePreferredHeadline() {
    let windows = [
        window("weekly_all", 80, active: false),
        window("session", 40, active: true),
    ]
    #expect(headline(of: windows)?.kind == "session")
    #expect(headline(of: [])?.kind == nil)
}
