import AppKit
import Foundation
import PitboardKit
import PitboardLinks
import Testing

@testable import PitboardApp

private let artifact = "https://claude.ai/public/artifacts/0e5a"
/// Where the fixture opens that artifact: its path on the fixture's own origin.
private let onFixture = URL(string: "pitboard-fixture://claude.ai/public/artifacts/0e5a")!

private func link(to claude: String) -> URL {
    pitboardLink(opening: claude, scheme: Fixture.linkScheme)
}

@MainActor
private func model(
    _ fixture: Fixture = .twoTools, read: Bool = true, record: WebRecord = WebRecord()
) async -> (AppModel, StandInStores) {
    let stores = StandInStores()
    let model = AppModel(
        testing: FixtureCore(fixture), web: .standIn(stores: stores, record: record))
    if read { await model.refresh() }
    return (model, stores)
}

@MainActor
private func pick(_ model: AppModel) -> LinkPick {
    linkPick(pending: model.links.pending, status: model.status, problem: model.problem)
}

// MARK: - A link from outside

/// A web page that knows the scheme can send a link. It shows the picker and nothing else:
/// no window is asked for and no store is made until somebody chooses.
@MainActor
@Test func aLinkFromOutsideAsksAndOpensNothing() async {
    let (model, stores) = await model()
    model.links.receive(link(to: artifact))
    #expect(model.links.requests == 1)
    #expect(model.links.pending == .link(.success(onFixture)))
    #expect(model.web.requests == 0)
    #expect(model.web.pages.isEmpty)
    #expect(model.web.opening.isEmpty)
    #expect(stores.made.isEmpty)
}

@MainActor
@Test func choosingAnAccountOpensItsWindowAtTheLink() async {
    let (model, _) = await model()
    model.links.receive(link(to: artifact))
    let personal = webStoreID(accountUuid: "dana@home.example")
    model.links.choose(personal, link: onFixture)
    #expect(model.web.requested == personal)
    #expect(model.web.requests == 1)
    #expect(model.web.opening[personal] == onFixture)
    #expect(model.links.pending == nil, "the picker closes")
    #expect(model.links.lastChosen == personal)

    let page = await model.claudePage(for: personal)
    #expect(page?.first == onFixture, "the window's first page is the link")
    #expect(model.web.opening.isEmpty)
}

/// Opening a link in an account's open window navigates it, as a link does in a browser tab,
/// and the picker's row says so before the choice is made.
@MainActor
@Test func aLinkForAnOpenWindowLoadsInIt() async throws {
    let (model, _) = await model()
    let work = webStoreID(accountUuid: "dana@work.example")
    let page = try #require(await model.claudePage(for: work))
    #expect(model.web.isOpen(work))
    #expect(!model.web.isOpen(webStoreID(accountUuid: "dana@home.example")))

    model.links.receive(link(to: artifact))
    model.links.choose(work, link: onFixture)
    #expect(page.first == onFixture)
    #expect(model.web.opening.isEmpty, "loaded in the page there is, not kept for a new one")
    #expect(model.web.requested == work, "and its window comes forward")
    #expect(windowOpenNote == "Window open. The link replaces what it shows; Back returns.")
}

@MainActor
@Test func thePickerListsEnrolledClaudeAccountsOnly() async {
    let (model, _) = await model(.twoTools)
    model.links.receive(link(to: artifact))
    guard case .choose(let shown, let accounts) = pick(model) else {
        Issue.record("a link and accounts to choose from")
        return
    }
    #expect(shown == onFixture)
    #expect(accounts.map(\.label) == ["work", "personal", "old"], "not main or spare")
    #expect(
        accounts.map(\.email) == ["dana@work.example", "dana@home.example", "dana@old.example"])
}

@MainActor
@Test func withOneAccountThePickerStillAsks() async {
    let (model, _) = await model(.onlyOne)
    model.links.receive(link(to: artifact))
    #expect(pick(model).accounts.map(\.label) == ["work"])
    #expect(model.web.requests == 0)
}

@MainActor
@Test(arguments: [Fixture.empty, .noClaudeCode])
func withNoClaudeAccountThePickerOffersToAddOne(_ fixture: Fixture) async {
    let (model, _) = await model(fixture)
    model.links.receive(link(to: artifact))
    #expect(pick(model) == .noAccount(link: onFixture))
    #expect(pick(model).title == "No Account Has a claude.ai Window")
    #expect(pick(model).message.contains("Add one"))
}

@MainActor
@Test func anUnnamedLoginIsOfferedANameInstead() async {
    let (model, _) = await model(.unnamed)
    model.links.receive(link(to: artifact))
    #expect(pick(model) == .unnamed(email: "dana@work.example", link: onFixture))
    #expect(pick(model).message.contains("dana@work.example, has no name in pitboard"))
}

/// A link that launches the app arrives before the first read. It waits for the accounts
/// rather than guess, and nothing is dropped.
@MainActor
@Test func beforeTheFirstReadThePickerWaits() async {
    let (model, _) = await model(read: false)
    model.links.receive(link(to: artifact))
    #expect(pick(model) == .reading(link: onFixture))
    await model.refresh()
    #expect(pick(model).accounts.count == 3)
}

@MainActor
@Test func aFailedFirstReadIsSaid() {
    let state = linkPick(
        pending: .link(.success(onFixture)), status: nil,
        problem: "Anthropic could not be reached.")
    #expect(state == .readFailed(problem: "Anthropic could not be reached."))
    #expect(state.title == "Couldn’t Read Accounts")
    #expect(state.message == "Anthropic could not be reached.")
}

@MainActor
@Test func aSecondLinkReplacesTheFirstAndKeepsTheChoice() async {
    let (model, _) = await model()
    let personal = webStoreID(accountUuid: "dana@home.example")
    model.links.receive(link(to: artifact))
    model.links.choose(personal, link: onFixture)
    model.links.receive(link(to: "https://claude.ai/chat/one"))
    model.links.receive(link(to: "https://claude.ai/chat/two"))
    #expect(model.links.requests == 3)
    #expect(
        model.links.pending
            == .link(.success(URL(string: "pitboard-fixture://claude.ai/chat/two")!)))
    #expect(
        defaultChoice(in: pick(model).accounts, lastChosen: model.links.lastChosen) == personal)
}

@MainActor
@Test func cancellingForgetsTheLink() async {
    let (model, _) = await model()
    model.links.receive(link(to: artifact))
    model.links.cancel()
    #expect(model.links.pending == nil)
    #expect(pick(model) == .idle)
    #expect(model.web.requests == 0)
}

@Test func theDefaultChoiceIsTheLastChosenThenTheOneInUse() {
    let accounts = claudeWindows(
        in: status([account("personal"), account("work", signedIn: true), account("old")]))
    let personal = accounts[0].store
    let work = accounts[1].store
    #expect(defaultChoice(in: accounts, lastChosen: nil) == work, "the one in use")
    #expect(defaultChoice(in: accounts, lastChosen: personal) == personal, "the last chosen")
    #expect(
        defaultChoice(in: accounts, lastChosen: UUID()) == work,
        "one chosen that is gone since")
    let nobodyInUse = claudeWindows(in: status([account("personal"), account("old")]))
    #expect(defaultChoice(in: nobodyInUse, lastChosen: nil) == nobodyInUse[0].store)
    #expect(defaultChoice(in: [], lastChosen: nil) == nil)
}

@MainActor
@Test func eachRefusalIsWordedFromItsReason() async {
    let (model, _) = await model()
    let cases: [(URL, LinkRefusal)] = [
        (link(to: "https://example.com/"), .notClaude(host: "example.com")),
        (link(to: "https://claude.ai/magic-link#a:b"), .signInLink),
        (URL(string: "pitboard-debug://close?url=x")!, .unknownRequest),
        (URL(string: "pitboard-debug://open?url=https://claude.ai/x?a=1&b=2")!, .ambiguous),
        (link(to: "javascript:alert(1)"), .noLink),
    ]
    for (sent, refusal) in cases {
        model.links.receive(sent)
        #expect(pick(model) == .refused(refusal), "\(sent)")
        #expect(pick(model).title == "Can’t Open This Link")
        #expect(pick(model).message == refusal.message)
    }
    #expect(
        LinkPick.refused(.notClaude(host: "example.com")).message
            == "pitboard opens claude.ai links only. This link is on example.com.")
}

@Test func everyStateSaysWhatItIsFor() {
    let account = ClaudeAccount(label: "work", email: "a@b.c", store: UUID(), inUse: true)
    #expect(
        LinkPick.choose(link: onFixture, accounts: [account]).title == "Open claude.ai Link")
    #expect(
        LinkPick.choose(link: onFixture, accounts: [account]).message
            == "Choose the account whose claude.ai window opens this link.")
    #expect(LinkPick.enter(accounts: [account]).message.hasPrefix("Paste a claude.ai link"))
    #expect(LinkPick.enter(accounts: [account]).link == nil)
    #expect(LinkPick.reading(link: onFixture).link == onFixture)
    #expect(LinkPick.noAccount(link: nil).link == nil, "nothing for the browser to open")
    #expect(LinkPick.refused(.noLink).accounts.isEmpty)
}

@MainActor
@Test func openClaudeLinkFromTheMenuTakesAPastedLink() async {
    let (model, _) = await model()
    model.links.enter()
    #expect(model.links.requests == 1)
    #expect(pick(model).accounts.count == 3)
    guard case .enter = pick(model) else {
        Issue.record("a field to paste into")
        return
    }
}

@MainActor
@Test func theMenuOffersOpenLinkOnlyWithAClaudeAccount() async {
    for fixture in Fixture.allCases {
        let (model, _) = await model(fixture)
        let offered = ClaudeMenu(model.claudeWindows) != .none
        let expected = [.twoTools, .oneTool, .onlyOne, .readFailure, .stuck].contains(fixture)
        #expect(offered == expected, "\(fixture)")
    }
}

// MARK: - The ways in

@MainActor
@Test func theServiceHandsItsSelectionToThePicker() async {
    let (model, _) = await model()
    let service = LinkService(links: model.links)
    let board = NSPasteboard.withUniqueName()
    defer { board.releaseGlobally() }

    board.clearContents()
    board.setString("see https://example.com and claude.ai/share/x", forType: .string)
    var said: NSString = ""
    service.openClaudeLink(board, userData: nil, error: &said)
    #expect(said == "")
    #expect(
        model.links.pending
            == .link(.success(URL(string: "pitboard-fixture://claude.ai/share/x")!)))
    #expect(model.links.requests == 1)

    board.clearContents()
    board.setString("nothing to open here", forType: .string)
    service.openClaudeLink(board, userData: nil, error: &said)
    #expect(said == "There is no link in the selection.")
    #expect(model.links.requests == 1, "nothing more is asked")

    board.clearContents()
    board.setString("https://example.com/", forType: .URL)
    service.openClaudeLink(board, userData: nil, error: &said)
    #expect(model.links.pending == .link(.failure(.notClaude(host: "example.com"))))
}

@MainActor
@Test func theDelegateHandsPitboardLinksToThePickerAndIgnoresOthers() async {
    let (model, _) = await model()
    let delegate = PitboardDelegate(model: model)
    delegate.receive([URL(fileURLWithPath: "/etc/hosts"), URL(string: "https://claude.ai/")!])
    #expect(model.links.requests == 0)
    #expect(model.links.pending == nil)
    delegate.receive([link(to: artifact)])
    #expect(model.links.requests == 1)
    #expect(model.links.pending == .link(.success(onFixture)))
    delegate.receive([pitboardLink(opening: artifact, scheme: "pitboard")])
    #expect(model.links.requests == 1, "a release build's link is not this build's")
}

@MainActor
@Test func openInBrowserUsesTheRecordedOpener() async {
    let record = WebRecord()
    let (model, _) = await model(.empty, record: record)
    model.links.receive(link(to: artifact))
    let shown = pick(model)
    #expect(shown.link == onFixture)
    if let link = shown.link { model.web.web.openElsewhere(link) }
    #expect(record.opened == [onFixture])
}
