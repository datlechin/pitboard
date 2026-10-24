import AppKit
import Foundation
import PitboardKit
import PitboardTestSupport
import Testing
import WebKit

@testable import PitboardApp

// MARK: - The app's presence

@MainActor
private final class PresenceLog {
    var said: [String] = []
    var active = true

    func presence() -> AppPresence {
        AppPresence(
            apply: { self.said.append($0 == .regular ? "regular" : "accessory") },
            bringForward: { self.said.append("activate") },
            reopen: { self.said.append("reopen") },
            isActive: { self.active },
            settle: { await Task.yield() })
    }
}

@MainActor
@Test func pitboardIsRegularWhileAnyWindowIsOpen() async {
    let log = PresenceLog()
    let presence = log.presence()
    let main = UUID()
    let account = UUID()

    presence.opened(main)
    presence.opened(account)
    #expect(presence.hasWindows)
    presence.closed(main)
    await settled()
    #expect(log.said == ["regular", "regular"], "one window still open")
    presence.closed(account)
    await settled()
    #expect(log.said.last == "accessory")
    #expect(!presence.hasWindows)
}

/// The account picker closes before the window it opened appears. Going back to the menu bar
/// in between would give the app's activation away, so the window would open behind.
@MainActor
@Test func aWindowReplacingTheLastOneKeepsPitboardRegular() async {
    let log = PresenceLog()
    let presence = log.presence()
    let picker = UUID()
    presence.opened(picker)
    presence.activate()
    presence.closed(picker)
    presence.opened(UUID())
    await settled()
    #expect(!log.said.contains("accessory"))
}

/// A shared link can find Pitboard in the background, where its own activation is refused:
/// it asks Launch Services to open it instead.
@MainActor
@Test func aLinkFromOutsideBringsPitboardForwardFromTheBackground() {
    let log = PresenceLog()
    log.active = false
    let presence = log.presence()
    presence.comeForward()
    #expect(log.said == ["regular", "reopen"])
    log.said = []
    log.active = true
    presence.comeForward()
    #expect(log.said == ["regular", "activate"])
}

/// A window asked for after a notification's button, a switch that ends in a question, finds
/// Pitboard in the background, where its own activation is refused: Launch Services then
/// brings it forward.
@MainActor
@Test func aWindowRefusedTheFrontComesForwardThroughLaunchServices() async {
    let log = PresenceLog()
    log.active = false
    let presence = log.presence()
    presence.activate()
    await settled()
    #expect(log.said == ["regular", "activate", "reopen"])
    log.said = []
    log.active = true
    presence.activate()
    await settled()
    #expect(log.said == ["regular", "activate"])
}

@MainActor
private func settled() async {
    for _ in 0..<10 { await Task.yield() }
}

/// Measured on macOS 27: an app made regular while it is already active does not get the
/// menu bar until it is next activated, so it is made regular first.
@MainActor
@Test func bringingPitboardForwardMakesItRegularFirst() {
    let log = PresenceLog()
    log.presence().activate()
    #expect(log.said == ["regular", "activate"])
}

// MARK: - Windows and their sessions

// Which windows each account has, which open and close, what each shows first and which
// stores go are the model's, tested in pitboard-ffi's `model/windowing.rs`.

private let twoTools = [
    window(
        "work", email: "dana@work.example", inUse: true,
        store: "7e15c34f-69ec-55b4-9542-f1c1fe3d7085"),
    window(
        "personal", email: "dana@home.example", store: "0c1d6a52-3f0e-5d5c-9a51-2f3e8b7c6d10"),
    window("old", email: "dana@old.example", store: "4b2f7e91-6a3c-5e8d-b1f2-7c9d0e1a2b30"),
    window(
        "main", on: .chatGPT, email: "dana@work.example", inUse: true,
        store: "8ff9e0e6-a7e2-53e2-a594-6e53ddadd38a"),
    window(
        "spare", on: .chatGPT, email: "dana@home.example",
        store: "5d3a8c72-9e1b-5f4a-8c2d-3e6f7a8b9c40"),
]

private let twoToolsMenus = [
    SiteMenu.several(
        title: "Open claude.ai", site: .claude, windows: Array(twoTools.prefix(3))),
    SiteMenu.several(
        title: "Open chatgpt.com", site: .chatGPT, windows: Array(twoTools.suffix(2))),
]

/// The model is a stand-in that reads nothing and keeps what it is sent: a test hands the
/// app's model each snapshot, as the Rust model would.
@MainActor
private func windows(
    _ accounts: [WindowAccount] = twoTools, stores: StandInStores = StandInStores()
) -> (AccountWindows, StandInModel) {
    let standIn = StandInModel(snapshot(0))
    let model = AppModel(model: standIn)
    let environment = WebEnvironment(
        scheme: WebEnvironment.fixtureScheme, stores: stores,
        downloads: FileManager.default.temporaryDirectory, openElsewhere: { _ in },
        configure: { _ in }, pause: { _ in })
    let windows = AccountWindows(
        model: model, environment: environment,
        presence: AppPresence(apply: { _ in }, bringForward: {}, settle: {}))
    model.apply(snapshot(1, windows: windowsShown(accounts, menus: twoToolsMenus)))
    return (windows, standIn)
}

@MainActor
private func account(_ label: String, in windows: AccountWindows) -> WindowAccount {
    windows.accounts.first { $0.label == label }!
}

private func opened(
    _ account: WindowAccount, _ url: String, serial: UInt64 = 1, note: WindowNoteKind? = nil
) -> OpenWindow {
    OpenWindow(
        store: account.store, account: account, load: PageLoad(serial: serial, url: url),
        note: note,
        clearDownloads: Choice(
            title: "Clear", intent: .clearDownloads(store: account.store), enabled: false))
}

@MainActor
private func settle(until done: () -> Bool) async {
    for _ in 0..<500 where !done() { await Task.yield() }
}

@MainActor
@Test func theWindowsAreTheModelsAccounts() {
    let (windows, _) = windows()
    #expect(windows.accounts.map(\.label) == ["work", "personal", "old", "main", "spare"])
    let titles = windows.menus.map { menu in
        switch menu {
        case .one(let title, _), .several(let title, _, _): title
        }
    }
    #expect(titles == ["Open claude.ai", "Open chatgpt.com"])
    #expect(windows.account(account("main", in: windows).id)?.site == .chatGPT)
    #expect(
        windows.account(UUID(uuidString: account("work", in: windows).store.uppercased())!)?
            .label == "work",
        "a window's store read back from its scene, as UUID writes one")
}

@MainActor
@Test func whatAWindowDoesIsToldToTheModel() {
    let (windows, standIn) = windows()
    let work = account("work", in: windows)
    windows.opening(work.id)
    windows.remember(URL(string: "pitboard-fixture://claude.ai/chat/1"), of: work)
    windows.remember(nil, of: work)
    windows.closed(work.id)
    windows.receive(URL(string: "pitboard-debug://open?url=https%3A%2F%2Fclaude.ai%2F")!)
    #expect(
        standIn.sent == [
            .windowOpened(store: work.id.uuidString),
            .pageShown(store: work.store, url: "pitboard-fixture://claude.ai/chat/1"),
            .windowClosed(store: work.id.uuidString),
            .linkArrived(text: "pitboard-debug://open?url=https%3A%2F%2Fclaude.ai%2F"),
        ])
}

@MainActor
@Test func aWindowLoadsWhatTheModelSaysOnce() {
    let (windows, _) = windows()
    let main = account("main", in: windows)
    let session = windows.session(
        for: opened(main, "pitboard-fixture://chatgpt.com/c/old", note: .signIn))
    #expect(session.page.webView.url?.absoluteString == "pitboard-fixture://chatgpt.com/c/old")
    #expect(session.note?.kind == .signIn)
    #expect(session.note?.text.contains("dana@work.example") == true)
    #expect(
        windows.session(for: opened(main, "pitboard-fixture://chatgpt.com/c/abc")) === session,
        "one window per account")
    #expect(
        session.page.webView.url?.absoluteString == "pitboard-fixture://chatgpt.com/c/old",
        "a page loads once")

    _ = windows.session(for: opened(main, "pitboard-fixture://chatgpt.com/c/abc", serial: 2))
    #expect(session.page.webView.url?.absoluteString == "pitboard-fixture://chatgpt.com/c/abc")
    windows.closed(main.id)
    #expect(windows.sessions.isEmpty)
    #expect(
        windows.session(for: opened(main, "pitboard-fixture://chatgpt.com/", serial: 3)).note
            == nil)
    windows.closed(main.id)
}

@MainActor
@Test func whatTheModelClosesAndDeletesIsClosedAndDeletedOnce() async {
    let stores = StandInStores()
    let (windows, standIn) = windows(stores: stores)
    let personal = account("personal", in: windows)
    let spare = account("spare", in: windows)
    _ = windows.session(for: opened(personal, "pitboard-fixture://claude.ai/"))
    _ = windows.session(for: opened(spare, "pitboard-fixture://chatgpt.com/"))

    let forgotten = twoTools.filter { $0.label != "personal" }
    let ask = StoreDeletion(store: personal.store, ask: 1)
    windows.model.apply(
        snapshot(
            2, windows: windowsShown(forgotten, closing: [personal.store], deleting: [ask])))
    #expect(windows.sessions.keys.sorted() == [spare.id])
    #expect(windows.isClosing(personal.id))
    await settle { !standIn.sent.isEmpty }
    #expect(stores.removed == [personal.id])
    #expect(standIn.sent == [.storeDeleted(store: personal.id.uuidString)])

    windows.model.apply(snapshot(3, windows: windowsShown(forgotten, deleting: [ask])))
    await settle { false }
    #expect(stores.attempts[personal.id] == 1, "an ask is deleted once")

    stores.refusals[spare.id] = 100
    windows.model.apply(
        snapshot(
            4,
            windows: windowsShown(
                forgotten, deleting: [StoreDeletion(store: spare.store, ask: 2)])))
    await settle { standIn.sent.count > 1 }
    #expect(standIn.sent.last == .storeHeld(store: spare.id.uuidString))
    windows.closed(spare.id)
}

/// The Dock icon's menu asks for a window from outside any view; the menu bar item opens it.
@MainActor
@Test func aWindowAskedForFromTheDockWaitsForTheMenuBarItem() {
    let (windows, _) = windows()
    let work = account("work", in: windows)
    windows.request(work)
    #expect(windows.requested == [work.id])
    #expect(windows.takeRequests() == [work.id])
    #expect(windows.requested.isEmpty)
}

// MARK: - Pages

// What a page may do, what a download is called and what a dialog is titled are the core's
// rules, tested in pitboard-ffi. These are WebKit's side.

@Test func zoomFollowsSafarisSteps() {
    #expect(Page.zoom(from: 1, toward: .zoomIn) == 1.15)
    #expect(Page.zoom(from: 1, toward: .zoomOut) == 0.85)
    #expect(Page.zoom(from: 3, toward: .zoomIn) == 3, "it stays at the ends")
    #expect(Page.zoom(from: 0.5, toward: .zoomOut) == 0.5)
    #expect(Page.zoom(from: 1.3, toward: .zoomIn) == 1.5, "from between two steps")
    #expect(Page.zoom(from: 2.5, toward: .actualSize) == 1)
}

/// A load stopped for another, or by a decision such as a download, is no failure worth a
/// word; anything else is said in place of the page.
@Test func onlyRealFailuresAreShown() {
    #expect(!Page.isShown(NSError(domain: NSURLErrorDomain, code: NSURLErrorCancelled)))
    #expect(!Page.isShown(NSError(domain: "WebKitErrorDomain", code: 102)))
    #expect(
        Page.isShown(NSError(domain: NSURLErrorDomain, code: NSURLErrorNotConnectedToInternet)))
    #expect(Page.isShown(NSError(domain: "WebKitErrorDomain", code: 101)))
}

/// A page that keeps ending its content process says so in place of loading forever, as the
/// core's rule decides from when it last ended: a heavy page ends it after it has loaded, so
/// loading again is no sign it works.
@MainActor
@Test func aPageThatKeepsCrashingSaysSo() {
    var now = Date(timeIntervalSince1970: 0)
    let page = Page(
        role: .window, configuration: WKWebViewConfiguration(), now: { now })
    page.contentEnded()
    #expect(page.failure == nil, "loaded again once")
    page.loadProgressed()
    now += 5
    page.contentEnded()
    #expect(page.failure == "The page stopped working.")

    page.reload()
    // Past the minute within which a second end counts.
    now += 61
    page.contentEnded()
    #expect(page.failure == nil, "a crash long after the last one is loaded again")
    page.close()
}

/// WebKit's own download items in a page's shortcut menu would do nothing, so they are left
/// out; everything else stays.
@Test func aPagesShortcutMenuLeavesOutWebKitsDownloadItems() {
    #expect(!PageWebView.keeps("WKMenuItemIdentifierDownloadLinkedFile"))
    #expect(!PageWebView.keeps("WKMenuItemIdentifierDownloadImage"))
    #expect(!PageWebView.keeps("WKMenuItemIdentifierDownloadMedia"))
    #expect(PageWebView.keeps("WKMenuItemIdentifierCopyLink"))
    #expect(PageWebView.keeps(""))
}
