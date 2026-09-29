import Foundation
import PitboardKit
import Testing

@testable import PitboardApp

/// The store of a fixture account's claude.ai window, whose account id is its email.
private func store(of email: String) -> UUID {
    webStoreID(accountUuid: email)
}

private let work = store(of: "dana@work.example")
private let personal = store(of: "dana@home.example")
private let old = store(of: "dana@old.example")

/// A model over `fixture`, read once, whose stand-in stores refuse to delete a store while
/// the model still has a page for it, as WebKit refuses one a web view still uses. The stores
/// `existing` names were made by this pitboard; those `unrecorded` names were not, as the
/// stores of the copy installed are not for a copy run with another home.
@MainActor
private func model(
    _ fixture: Fixture = .twoTools, existing: Set<UUID> = [], unrecorded: Set<UUID> = [],
    record: WebRecord = WebRecord()
) async -> (AppModel, StandInStores, FixtureCore) {
    let stores = StandInStores()
    stores.made(existing)
    stores.existing.formUnion(unrecorded)
    let core = FixtureCore(fixture)
    let model = AppModel(testing: core, web: .standIn(stores: stores, record: record))
    stores.inUse = { [weak model] store in
        guard let web = model?.web else { return false }
        return web.isOpen(store) || web.downloads.isSaving(for: store)
    }
    await model.refresh()
    await model.web.settle()
    return (model, stores, core)
}

/// A download as the model keeps it, standing in for WebKit's.
@MainActor
private final class Transfer {
    var stopped = false
}

/// A core that can read nothing, not even what is known offline.
private final class Unreadable: Core, @unchecked Sendable {
    private let fixture = FixtureCore(.empty)
    func status(fresh: Bool) async throws -> Status { throw failure }
    func statusOffline() async throws -> Status { throw failure }
    func doctor() async -> Diagnosis { await fixture.doctor() }
    func switchTo(_ label: String) async throws -> Switched { throw failure }
    func enrollCurrent(_ label: String) async throws -> Enrolled { throw failure }
    func forget(_ label: String) async throws -> Changed { throw failure }
    func rename(_ from: String, to: String) async throws -> Changed { throw failure }
    func signIn(_ label: String) async throws -> SignIn { throw failure }
    func abandonRecovery() async throws -> Abandoned? { nil }
    func log(limit: UInt32) async -> [Change] { [] }
    func renew() async -> [Renewed] { [] }
    func schedule() async -> Schedule { .absent }
    func scheduleInstall() async throws -> String { throw failure }
    func scheduleUninstall() async throws -> Bool { false }
    func scheduleRepair() async throws -> Bool { false }
    func changedAt() async -> Int64 { 1 }
    func readingsChangedAt() async -> Int64 { 1 }
    func tools() -> [Tool] { FixtureCore.tools }
    func installed() async -> [Tool] { FixtureCore.tools }
    func searchPath() async -> String? { nil }

    private var failure: PitboardError {
        .Failed(
            code: "state_newer", cause: nil,
            message: "state.json was written by a newer pitboard.",
            warnings: [])
    }
}

// MARK: - Forgetting

@MainActor
@Test func forgettingAClaudeAccountRemovesItsStoreAfterTheCoreForgetsIt() async {
    let (model, stores, _) = await model(existing: [work, personal])
    #expect(await model.forget("claude/personal") == nil)
    #expect(stores.removed == [personal])
    #expect(stores.existing == [work])
    #expect(model.claudeWindows.map(\.label) == ["work", "old"])
}

/// The account in use cannot be forgotten, and its window keeps its sign-in.
@MainActor
@Test func aFailedForgetLeavesTheStore() async {
    let (model, stores, _) = await model(existing: [work, personal])
    let failure = await model.forget("claude/work")
    #expect(failure?.code == "cannot_forget_active_account")
    #expect(stores.removed.isEmpty)
    #expect(stores.existing == [work, personal])
}

/// The stand-in refuses to delete a store while a page uses it, as WebKit does. Forgetting
/// deletes it all the same, which it can only do by closing the page first.
@MainActor
@Test func forgettingClosesTheWindowBeforeRemovingTheStore() async throws {
    let (model, stores, _) = await model(existing: [personal])
    let page = try #require(await model.claudePage(for: personal))
    #expect(model.web.isOpen(personal))
    #expect(await model.forget("claude/personal") == nil)
    #expect(page.closed)
    #expect(!model.web.isOpen(personal))
    #expect(stores.removed == [personal])
}

@MainActor
@Test func removalIsRetriedWhileWebKitSaysTheStoreIsInUse() async {
    let record = WebRecord()
    let (model, stores, _) = await model(existing: [personal], record: record)
    stores.refusals = 2
    #expect(await model.forget("claude/personal") == nil)
    #expect(stores.removed == [personal])
    #expect(record.pauses == [.milliseconds(250), .milliseconds(500)])
}

/// Five refusals in a row give up and say so, and the account is forgotten all the same.
@MainActor
@Test func aRemovalThatKeepsFailingIsSaid() async {
    let record = WebRecord()
    let (model, stores, _) = await model(existing: [personal], record: record)
    stores.refusals = 5
    let failure = await model.forget("claude/personal")
    #expect(failure?.title == "Couldn’t delete personal’s claude.ai data")
    #expect(failure?.message.contains("Data store is in use.") == true)
    #expect(failure?.message.contains("tries again") == true)
    #expect(stores.removed.isEmpty)
    #expect(record.pauses.count == 4)
    #expect(!model.claudeWindows.map(\.label).contains("personal"), "forgotten all the same")

    // The next read that assigns the accounts tries again, and this time WebKit lets go.
    await model.refresh()
    await model.web.settle()
    #expect(stores.removed == [personal])
}

// MARK: - The sweep

@MainActor
@Test func aStoreTheIndexNoLongerNamesIsSweptAfterARead() async throws {
    let (model, stores, core) = await model(existing: [work, personal, old])
    #expect(stores.removed.isEmpty)
    let page = try #require(await model.claudePage(for: personal))

    // Forgotten from the command line, behind the app's back.
    _ = try await core.forget("claude/personal")
    await model.noticeOtherChangesForTesting()
    await model.web.settle()
    #expect(page.closed, "its window's page is closed before its store goes")
    #expect(stores.removed == [personal])
    #expect(stores.existing == [work, old])
}

@MainActor
@Test func storesOfEnrolledAccountsAreKept() async {
    let orphan = store(of: "someone-forgotten")
    let (model, stores, _) = await model(existing: [work, personal, old, orphan])
    #expect(stores.removed == [orphan])
    #expect(stores.existing == [work, personal, old])
    #expect(
        stores.recorded() == [work, personal, old], "a store deleted is no longer recorded")
    #expect(model.status != nil)
}

/// WebKit keeps one app's stores under the person's own Library whatever `HOME` says, and the
/// accounts come from `HOME`. A copy run with a fresh home, which has no accounts, sees every
/// store of the copy installed, and deleting them would sign every one of its windows out.
@MainActor
@Test func aStoreThisPitboardDidNotMakeIsNeverSwept() async {
    let (_, stores, _) = await model(.empty, unrecorded: [work, personal])
    #expect(stores.removed.isEmpty)
    #expect(stores.existing == [work, personal])

    let (_, mixed, _) = await model(.empty, existing: [old], unrecorded: [work])
    #expect(mixed.removed == [old])
    #expect(mixed.existing == [work])
}

/// Each pitboard directory keeps its own record, so one home never learns of another's
/// stores.
@MainActor
@Test func eachPitboardDirectoryRecordsItsOwnStores() {
    let defaults = TestDefaults()
    let installed = WebStoreRecord(defaults: defaults, directory: "/Users/dana/.pitboard")
    let scratch = WebStoreRecord(defaults: defaults, directory: "/tmp/home/.pitboard")
    installed.add(work)
    installed.add(personal)
    scratch.add(old)
    #expect(installed.ids == [work, personal])
    #expect(scratch.ids == [old])
    installed.remove(work)
    #expect(installed.ids == [personal])
    scratch.remove(old)
    #expect(scratch.ids.isEmpty)
    #expect(installed.ids == [personal])

    #expect(
        WebStoreRecord.directory(environment: ["HOME": "/Users/dana"])
            == "/Users/dana/.pitboard")
    #expect(
        WebStoreRecord.directory(environment: [
            "HOME": "/Users/dana/", "PITBOARD_HOME": "/p/x/",
        ])
            == "/p/x")
    #expect(
        WebStoreRecord.directory(environment: ["HOME": "/tmp/a/../b"]) == "/tmp/b/.pitboard")
}

/// A read that fails with nothing known lists nothing, and nothing listed is not the same as
/// nothing enrolled: an older app meeting a newer state file must not sign every window out.
@MainActor
@Test func aFailedReadSweepsNothing() async {
    let stores = StandInStores()
    stores.made([work, personal])
    let model = AppModel(testing: Unreadable(), web: .standIn(stores: stores))
    await model.refresh()
    await model.web.settle()
    #expect(model.status == nil)
    #expect(model.problem != nil)
    #expect(stores.removed.isEmpty)
    #expect(stores.existing == [work, personal])
}

// MARK: - Signing out of one window

@MainActor
@Test func signingOutOfAWindowRemovesItsStoreAndOpensAFreshOne() async throws {
    let (model, stores, _) = await model(existing: [work])
    let before = try #require(await model.claudePage(for: work))
    #expect(before.note == nil, "a store WebKit listed has been signed in before")

    #expect(await model.web.signOut(work) == nil)
    #expect(before.closed)
    #expect(stores.removed == [work])
    let after = try #require(model.web.pages[work])
    #expect(after !== before)
    #expect(after.note == .firstSignIn(email: "dana@work.example"))
    #expect(after.first == model.web.web.home, "claude.ai's own page, where sign-in happens")
    #expect(model.web.signingOut.isEmpty)
}

/// When WebKit will not let the store go, the window opens again on it, still signed in,
/// and says why.
@MainActor
@Test func aSignOutWebKitRefusesKeepsTheWindowSignedIn() async throws {
    let (model, stores, _) = await model(existing: [work])
    _ = try #require(await model.claudePage(for: work))
    stores.refusals = 5
    let failure = await model.web.signOut(work)
    #expect(failure?.title == "Couldn’t sign out of this window")
    #expect(stores.existing == [work])
    let page = try #require(model.web.pages[work])
    #expect(page.note == nil)
}

/// A download holds its store, and WebKit refuses to delete a store in use, so a sign-out
/// stops the window's downloads first. Before, one longer than the retries signed nothing out.
@MainActor
@Test func signingOutStopsTheWindowsDownloadsBeforeRemovingTheStore() async throws {
    let (model, stores, _) = await model(existing: [work, personal])
    _ = try #require(await model.claudePage(for: work))
    let saving = Transfer()
    let other = Transfer()
    model.web.downloads.keep(saving, of: work) { saving.stopped = true }
    model.web.downloads.keep(other, of: personal) { other.stopped = true }
    #expect(model.web.downloads.isSaving(for: work))

    #expect(await model.web.signOut(work) == nil)
    #expect(saving.stopped)
    #expect(!other.stopped, "another account's download goes on")
    #expect(!model.web.downloads.isSaving(for: work))
    #expect(model.web.downloads.isSaving(for: personal))
    #expect(stores.removed == [work])
}

/// Forgetting an account stops its window's downloads the same way.
@MainActor
@Test func forgettingStopsTheAccountsDownloads() async {
    let (model, stores, _) = await model(existing: [personal])
    let saving = Transfer()
    model.web.downloads.keep(saving, of: personal) { saving.stopped = true }
    #expect(await model.forget("claude/personal") == nil)
    #expect(saving.stopped)
    #expect(stores.removed == [personal])
}

/// Closing a window is not a reason to stop its downloads: the model keeps them to the end.
@MainActor
@Test func closingAWindowLetsItsDownloadsGoOn() async throws {
    let (model, _, _) = await model(existing: [work])
    _ = try #require(await model.claudePage(for: work))
    let saving = Transfer()
    model.web.downloads.keep(saving, of: work) { saving.stopped = true }
    model.web.detach(work)
    #expect(!saving.stopped)
    #expect(model.web.downloads.isSaving(for: work))
}

/// A window closed while it signs out gets no page back: nothing would show it, and the
/// account picker would call it open.
@MainActor
@Test func aSignOutThatEndsAfterItsWindowClosedPutsNoPageBack() async throws {
    let (model, stores, _) = await model(existing: [work])
    _ = try #require(await model.claudePage(for: work))
    stores.refusals = 1
    stores.removing = { _ in model.web.detach(work) }
    #expect(await model.web.signOut(work) == nil)
    #expect(stores.removed == [work])
    #expect(model.web.pages[work] == nil)
    #expect(!model.web.isOpen(work))
}

/// A link asked for while the window signs out is where the fresh page starts.
@MainActor
@Test func aLinkAskedForDuringASignOutOpensOnTheFreshPage() async throws {
    let (model, stores, _) = await model(existing: [work])
    _ = try #require(await model.claudePage(for: work))
    let link = URL(string: "pitboard-fixture://claude.ai/chat/abc")!
    stores.removing = { _ in model.openClaude(work, at: link) }
    #expect(await model.web.signOut(work) == nil)
    let page = try #require(model.web.pages[work])
    #expect(page.first == link)
    #expect(model.web.opening[work] == nil)
}

// MARK: - Opening a window

@MainActor
@Test func choosingAnAccountAsksForItsStoresWindow() async {
    let (model, stores, _) = await model()
    #expect(model.web.requests == 0)
    model.openClaude(personal)
    #expect(model.web.requested == personal)
    #expect(model.web.requests == 1)
    #expect(stores.made.isEmpty, "asking for a window makes no store")
}

@MainActor
@Test func aWindowOpenedForTheFirstTimeSaysHowToSignIn() async throws {
    let (model, _, _) = await model(existing: [work])
    let fresh = try #require(await model.claudePage(for: personal))
    #expect(fresh.note == .firstSignIn(email: "dana@home.example"))
    #expect(fresh.first == model.web.web.home)
    let known = try #require(await model.claudePage(for: work))
    #expect(known.note == nil)
    #expect(await model.claudePage(for: personal) === fresh, "one page per window")
}

/// Asking WebKit for a store makes one, so no page, and no store, is made for a window no
/// enrolled account derives: before the first read, or for a store nobody has.
@MainActor
@Test func noStoreIsMadeForAWindowWhoseAccountIsNotKnown() async {
    let stores = StandInStores()
    let model = AppModel(testing: FixtureCore(.twoTools), web: .standIn(stores: stores))
    #expect(await model.claudePage(for: work) == nil, "before the first read")
    await model.refresh()
    #expect(await model.claudePage(for: store(of: "nobody@example.com")) == nil)
    #expect(
        await model.claudePage(for: store(of: "dana@work.example")) != nil,
        "the Codex account shares the address, and only the Claude Code one has a window")
    #expect(model.web.pages.count == 1)
    #expect(stores.made.isEmpty, "a page makes no store until its window shows it")
}

/// A window's web view is made on its own account's store, and nobody else's.
@MainActor
@Test func aWindowsWebViewUsesItsAccountsStore() async throws {
    let (model, stores, _) = await model(existing: [work, personal])
    let page = try #require(await model.claudePage(for: work))
    let configuration = page.configuration()
    #expect(configuration.websiteDataStore === stores.store(for: work))
    #expect(configuration.websiteDataStore !== stores.store(for: personal))
}

/// The app shows in the Dock while a claude.ai window is open, signing out included, and
/// leaves it when the last one closes.
@MainActor
@Test func theAppWantsTheDockWhileAClaudeWindowIsOpen() async throws {
    let (model, stores, _) = await model(existing: [work, personal])
    #expect(!model.web.wantsDockIcon)
    _ = try #require(await model.claudePage(for: work))
    #expect(model.web.wantsDockIcon)
    _ = try #require(await model.claudePage(for: personal))
    var during: Bool?
    stores.removing = { _ in during = model.web.wantsDockIcon }
    model.web.detach(personal)
    #expect(model.web.wantsDockIcon)
    #expect(await model.web.signOut(work) == nil)
    #expect(during == true, "a window signing out is still open")
    model.web.detach(work)
    #expect(!model.web.wantsDockIcon)
}

@MainActor
@Test func aClosedWindowLetsItsPageGo() async throws {
    let (model, _, _) = await model()
    let page = try #require(await model.claudePage(for: work))
    model.web.detach(work)
    #expect(page.closed)
    #expect(!model.web.isOpen(work))
    #expect(page.hostedWebView() == nil, "a closed page makes no web view again")
}

/// A rename keeps the window and its sign-in: the store is the same, and the page follows the
/// account's new details.
@MainActor
@Test func aRenamedAccountKeepsItsWindow() async throws {
    let (model, stores, _) = await model(existing: [work, personal])
    let page = try #require(await model.claudePage(for: personal))
    #expect(await model.rename("personal", of: "claude", to: "home") == nil)
    await model.web.settle()
    #expect(stores.removed.isEmpty)
    #expect(await model.claudePage(for: personal) === page)
    #expect(page.account.label == "home")
    #expect(!page.closed)
}
