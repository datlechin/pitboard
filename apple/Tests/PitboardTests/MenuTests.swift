import PitboardKit
import Testing

@testable import Pitboard

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
    let advice = Advice.about(read, unless: [:]).first
    #expect(advice?.ran == "work")
    #expect(advice?.use == "fresh")
    #expect(advice?.left == 95)
    #expect(advice?.switchTo == "claude/fresh", "the switch names the account with its tool")
    #expect(advice?.tool == nil, "one tool, so nothing says which")
    #expect(advice?.notification.userInfo["label"] as? String == "claude/fresh")
}

@Test func anAccountThatCannotBeSwitchedToIsNotOffered() {
    let read = status([
        account("work", signedIn: true, [window("session", 100)]),
        account("parked", switchable: false, [window("session", 0)]),
    ])
    #expect(Advice.about(read, unless: [:]).isEmpty)
}

@Test func nothingIsSaidWhenEveryAccountIsSpent() {
    let read = status([
        account("work", signedIn: true, [window("session", 100)]),
        account("spare", [window("session", 100)]),
    ])
    #expect(Advice.about(read, unless: [:]).isEmpty)
}

@Test func oneExhaustedWindowIsMentionedOnce() {
    let exhausted = window("session", 100, resets: 42)
    let read = status([
        account("work", signedIn: true, [exhausted]),
        account("spare", [window("session", 0)]),
    ])
    let told = Advice.key("claude", "work", exhausted)
    #expect(Advice.about(read, unless: [told: 42]).isEmpty)
    // The next window is its own; what was said about the last one does not carry over.
    #expect(Advice.about(read, unless: [told: 42 - 18_000]).first?.use == "spare")
}

/// A session is given a reset in whole seconds, and Anthropic's answer gives a fraction that
/// is dropped, so one window can come back a second apart from what was told about it. The
/// core counts resets a minute apart as one, and so does what is told.
@Test func aWindowToldAboutIsNotToldAgainWithItsResetRoundedOtherwise() {
    let exhausted = window("session", 100, resets: 43)
    let read = status([
        account("work", signedIn: true, [exhausted]),
        account("spare", [window("session", 0)]),
    ])
    #expect(Advice.about(read, unless: [Advice.key("claude", "work", exhausted): 42]).isEmpty)
}

/// Advice is about one window. The window after it, spent as well, is advice of its own to
/// tell, and not this advice still holding.
@Test func adviceHoldsForTheWindowItIsAboutAndNotTheOneAfter() {
    let spent = { (resets: Int64) in
        status([
            account("work", signedIn: true, [window("session", 100, resets: resets)]),
            account("spare", [window("session", 0)]),
        ])
    }
    let advice = Advice.about(spent(7_200), unless: [:]).first
    #expect(advice?.holds(in: spent(7_200)) == true)
    #expect(advice?.holds(in: spent(7_201)) == true, "one reset, as another source rounds it")
    #expect(advice?.holds(in: spent(25_200)) == false)
}

@Test func aWeeklyLimitIsComparedWithWeeklyLimits() {
    let read = status([
        account("work", signedIn: true, [window("session", 10), window("weekly_all", 100)]),
        account("spare", [window("session", 100), window("weekly_all", 20)]),
    ])
    let advice = Advice.about(read, unless: [:]).first
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

// MARK: - More than one tool

/// A Codex account with room is no help to somebody whose Claude Code account has run out:
/// a switch between them is not a switch at all.
@Test func adviceNeverCrossesTools() {
    let read = status([
        account("work", signedIn: true, [window("session", 100)]),
        account("job", of: "codex", signedIn: true, [window("five_hour", 10, length: 18_000)]),
        account("spare", of: "codex", [window("five_hour", 0, length: 18_000)]),
    ])
    #expect(Advice.about(read, tools: bothTools, unless: [:]).isEmpty)
}

/// Each tool is advised about from its own accounts, and says which tool it is about.
@Test func eachToolIsAdvisedFromItsOwnAccounts() {
    let read = status([
        account("work", signedIn: true, [window("session", 100)]),
        account("personal", [window("session", 30)]),
        account(
            "work", of: "codex", signedIn: true, [window("seven_day", 100, length: 604_800)]),
        account("spare", of: "codex", [window("seven_day", 60, length: 604_800)]),
    ])
    let advice = Advice.about(read, tools: bothTools, unless: [:])
    #expect(advice.map(\.switchTo) == ["claude/personal", "codex/spare"])
    #expect(advice.map(\.tool) == ["Claude Code", "Codex"])
    #expect(advice.last?.limit == "weekly")
    #expect(
        advice.last?.said
            == "Codex: work has none of its weekly limit left. spare has 40% of its own left.")
}

/// What was said about one tool's `work` says nothing about another's.
@Test func whatWasToldIsKeptApartByTool() {
    let exhausted = window("session", 100, resets: 7)
    let read = status([
        account("work", of: "codex", signedIn: true, [exhausted]),
        account("spare", of: "codex", [window("session", 0)]),
    ])
    let toldClaude = Advice.key("claude", "work", exhausted)
    #expect(Advice.about(read, unless: [toldClaude: 7]).first?.switchTo == "codex/spare")
    #expect(Advice.about(read, unless: [Advice.key("codex", "work", exhausted): 7]).isEmpty)
}

/// With an account in use in each tool and one menu bar, the bar shows the one closest to
/// running out.
@Test func theMenuBarFollowsTheMostUsedAccountAcrossTools() {
    let read = status([
        account("personal", signedIn: true, [window("session", 40)]),
        account("job", of: "codex", signedIn: true, [window("five_hour", 71, length: 18_000)]),
        account("spare", of: "codex", [window("five_hour", 99, length: 18_000)]),
    ])
    #expect(menuTitle(for: read, order: bothTools) == "job 71%")
}

/// A tie goes to the tool listed first, and a login nobody has named does not take the bar
/// from one that has a name.
@Test func aTieInTheMenuBarGoesToTheFirstTool() {
    let tied = status([
        account("job", of: "codex", signedIn: true, [window("five_hour", 50)]),
        account("personal", signedIn: true, [window("session", 50)]),
    ])
    #expect(menuTitle(for: tied, order: bothTools) == "personal 50%")

    let unnamed = status([
        account("personal", signedIn: true, [window("session", 10)]),
        account(nil, of: "codex", signedIn: true, uuid: "c", [window("five_hour", 99)]),
    ])
    #expect(menuTitle(for: unnamed, order: bothTools) == "personal 10%")
}

/// One tool, whichever it is: no headings, the rows in the order the core gave them, and the
/// bar naming the first account signed in, as it always has.
@Test func oneToolLooksAsItAlwaysDid() {
    for tool in ["claude", "codex"] {
        let accounts = [
            account(nil, of: tool, signedIn: true, uuid: "u", [window("session", 80)]),
            account("work", of: tool, [window("session", 10)]),
        ]
        let groups = grouped(accounts, by: bothTools)
        #expect(groups.count == 1)
        #expect(groups.first?.name == nil)
        #expect(groups.first?.accounts == accounts)
        #expect(menuTitle(for: status(accounts), order: bothTools) == "unenrolled 80%")
    }
    #expect(grouped([], by: bothTools).isEmpty)
}

/// More than one tool: a section per tool, headed by its name, in the order the tools are
/// listed whatever order the rows came in.
@Test func accountsOfTwoToolsAreGroupedByTool() {
    let groups = grouped(
        [
            account("job", of: "codex"),
            account("work", signedIn: true),
            account("side", of: "codex", signedIn: true),
        ],
        by: bothTools)
    #expect(groups.map(\.name) == ["Claude Code", "Codex"])
    #expect(groups.map { $0.accounts.compactMap(\.label) } == [["work"], ["job", "side"]])
}
