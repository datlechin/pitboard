import CoreServices
import Foundation
import PitboardKit
import Testing

@testable import PitboardApp

private let claude = URL(string: "https://claude.ai/")!
private let fixtureHome = URL(string: "pitboard-fixture://claude.ai/")!

private func url(_ text: String) -> URL { URL(string: text)! }

private func routed(
    _ text: String, _ target: WebRequest.Target = .window, clicked: Bool = false,
    download: Bool = false, fromHome: Bool = false, home: URL = claude
) -> WebRoute {
    route(
        WebRequest(
            url: url(text), target: target, clicked: clicked, download: download,
            fromHome: fromHome),
        home: home)
}

// MARK: - Where a navigation goes

@Test func aClaudePageLoadsInTheWindowAndItsFramesLoadFromAnywhere() {
    #expect(routed("https://claude.ai/new") == .load)
    #expect(routed("https://claude.ai/chat/abc?x=1#y", clicked: true) == .load)
    #expect(routed("https://claude.ai/") == .load)
    #expect(routed("https://www.claudeusercontent.com/artifact", .frame) == .load)
    #expect(routed("https://challenges.cloudflare.com/x", .frame) == .load)
    #expect(routed("about:srcdoc", .frame) == .load)
    #expect(routed("blob:https://claude.ai/1234", .frame) == .load)
    #expect(routed("file:///etc/hosts", .frame) == .drop, "a web page never opens a local file")
    #expect(routed("about:blank") == .load)
    #expect(routed("about:blank", .newWindow) == .drop, "pitboard makes no popups")
}

@Test func aLinkOutsideClaudeOpensInTheBrowserWhetherClickedRedirectedOrANewWindow() {
    let outside = "https://example.com/page"
    #expect(routed(outside, clicked: true) == .elsewhere(url(outside)))
    #expect(routed(outside) == .elsewhere(url(outside)), "a redirect")
    #expect(routed(outside, .newWindow) == .elsewhere(url(outside)))
    #expect(
        routed("http://example.com/", clicked: true) == .elsewhere(url("http://example.com/")))
    #expect(
        routed("https://console.anthropic.com/", clicked: true)
            == .elsewhere(url("https://console.anthropic.com/")))
}

/// Google blocks its pages in a web view inside an app: its sign-in, the single sign-on it
/// provides, and connecting Gmail, Drive or Calendar. Opening one in the browser would sign in
/// the browser, not this window, so it goes nowhere.
@Test func googleSignInIsRefusedInTheWindowAndInANewWindowAndNeverSentToTheBrowser() {
    let google = "https://accounts.google.com/o/oauth2/v2/auth?client_id=x"
    #expect(routed(google, clicked: true) == .refuse(.google))
    #expect(routed(google, .newWindow) == .refuse(.google))
    #expect(routed("https://ACCOUNTS.GOOGLE.COM/signin") == .refuse(.google))
    #expect(routed("http://accounts.google.com/") == .refuse(.google))
    #expect(routed(google, .frame) == .load, "a frame is the page's business")
}

@Test func aClaudeLinkAskedForInANewWindowLoadsInThisOne() {
    let link = "https://claude.ai/public/artifacts/0e5a"
    #expect(routed(link, .newWindow) == .loadHere(url(link)))
    #expect(routed(link, .newWindow, clicked: true) == .loadHere(url(link)))
}

@Test func onlyClaudeItselfStays() {
    for text in [
        "https://claude.ai.evil.example/", "https://evilclaude.ai/", "https://sub.claude.ai/",
        "https://claude.ai:8443/", "https://claude.ai@evil.example/", "https://x@claude.ai/",
    ] {
        #expect(routed(text) == .elsewhere(url(text)), "\(text)")
    }
    #expect(routed("https://CLAUDE.AI/new") == .load)
    #expect(
        routed("http://claude.ai/") == .elsewhere(url("http://claude.ai/")),
        "plain http is not home, and claude.ai upgrades it in the browser")
}

@Test func downloadsAreSavedFromHttpsBlobAndDataOnly() {
    #expect(
        routed("https://claude.ai/files/report.pdf", download: true, fromHome: true)
            == .download)
    #expect(
        routed("https://example.com/report.pdf", download: true, fromHome: true) == .download)
    #expect(routed("blob:https://claude.ai/1234", download: true, fromHome: true) == .download)
    #expect(
        routed("data:text/plain,hi", .newWindow, download: true, fromHome: true) == .download)
    #expect(routed("http://example.com/report.pdf", download: true, fromHome: true) == .drop)
    #expect(routed("file:///etc/hosts", download: true, fromHome: true) == .drop)
    #expect(routed("ftp://example.com/x", download: true, fromHome: true) == .drop)
}

/// An artifact runs somebody else's code in a frame of its own, so what it downloads, or any
/// frame that is not claude.ai's own page, is asked about first, as Safari asks.
@Test func aDownloadNotFromClaudesOwnPageIsAskedAboutFirst() {
    #expect(
        routed("blob:https://www.claudeusercontent.com/1", download: true) == .askToDownload)
    #expect(routed("https://example.com/report.pdf", download: true) == .askToDownload)
    #expect(routed("data:text/plain,hi", .newWindow, download: true) == .askToDownload)
    #expect(routed("http://example.com/report.pdf", download: true) == .drop)

    #expect(isHome(scheme: "https", host: "claude.ai", port: 0, claude))
    #expect(isHome(scheme: "HTTPS", host: "Claude.AI", port: 0, claude))
    #expect(isHome(scheme: "pitboard-fixture", host: "claude.ai", port: 0, fixtureHome))
    #expect(!isHome(scheme: "https", host: "www.claudeusercontent.com", port: 0, claude))
    #expect(!isHome(scheme: "https", host: "", port: 0, claude), "a sandboxed frame")
    #expect(!isHome(scheme: "http", host: "claude.ai", port: 0, claude))
    #expect(!isHome(scheme: "https", host: "claude.ai", port: 8443, claude))
}

@Test func aResponseWebKitCannotShowOrAnAttachmentIsDownloaded() {
    func routed(_ canShow: Bool, _ disposition: String?) -> ResponseRoute {
        responseRoute(canShow: canShow, disposition: disposition, mainFrame: true)
    }
    #expect(routed(true, nil) == .show)
    #expect(routed(true, "inline") == .show)
    #expect(routed(false, nil) == .download)
    #expect(routed(true, "attachment; filename=a.csv") == .download)
    #expect(routed(true, " Attachment") == .download)

    #expect(
        responseRoute(canShow: true, disposition: "attachment", mainFrame: false)
            == .askToDownload, "a frame's is asked about")
    #expect(responseRoute(canShow: false, disposition: nil, mainFrame: false) == .askToDownload)
    #expect(responseRoute(canShow: true, disposition: nil, mainFrame: false) == .show)
}

/// WebKit's target frame, as the policy sees it. A main frame taken for a frame would load
/// any page, Google's sign-in included, in the window.
@Test func theTargetFrameDecidesTheTarget() {
    #expect(WebRequest.Target(frameIsMain: nil) == .newWindow)
    #expect(WebRequest.Target(frameIsMain: true) == .window)
    #expect(WebRequest.Target(frameIsMain: false) == .frame)
}

/// What claude.ai sends to the browser without a click, or opens as a window pitboard does
/// not make, is said: something connected in the browser goes to the browser's account.
@Test func whatLeavesTheWindowWithoutAClickIsSaid() {
    func noted(_ request: WebRequest) -> WebNote? {
        routeNote(
            route(request, home: claude), for: request, email: "a@b.example", label: "work")
    }
    let outside = url("https://github.com/login/oauth/authorize")
    #expect(
        noted(WebRequest(url: outside, target: .newWindow)) == .openedInBrowser(label: "work"))
    #expect(noted(WebRequest(url: outside, target: .window)) == .openedInBrowser(label: "work"))
    #expect(noted(WebRequest(url: outside, target: .window, clicked: true)) == nil)
    #expect(noted(WebRequest(url: outside, target: .newWindow, clicked: true)) == nil)
    #expect(noted(WebRequest(url: url("mailto:a@b.example"), target: .newWindow)) == nil)
    #expect(
        noted(WebRequest(url: url("https://accounts.google.com/"), target: .newWindow))
            == .googleRefused(email: "a@b.example"))
    #expect(noted(WebRequest(url: url("about:blank"), target: .newWindow)) == .windowNotShown)
    #expect(
        noted(WebRequest(url: url("blob:https://claude.ai/1"), target: .newWindow))
            == .windowNotShown)
    #expect(noted(WebRequest(url: url("about:blank"), target: .window)) == nil)
    #expect(noted(WebRequest(url: url("file:///etc/hosts"), target: .frame)) == nil)
    #expect(
        noted(
            WebRequest(url: url("http://example.com/a.pdf"), target: .newWindow, download: true)
        )
            == nil)
    #expect(noted(WebRequest(url: url("https://claude.ai/new"), target: .window)) == nil)
}

/// WebKit shows no error page, so a failed load is said, except a load stopped for another and
/// one a policy decision ended, which every download gives.
@Test func aFailedLoadIsSaidUnlessItWasStoppedOnPurpose() {
    #expect(!loadFailureShown(domain: NSURLErrorDomain, code: NSURLErrorCancelled))
    #expect(!loadFailureShown(domain: "WebKitErrorDomain", code: 102))
    #expect(loadFailureShown(domain: NSURLErrorDomain, code: NSURLErrorNotConnectedToInternet))
    #expect(loadFailureShown(domain: NSURLErrorDomain, code: NSURLErrorCannotFindHost))
    #expect(loadFailureShown(domain: NSURLErrorDomain, code: NSURLErrorSecureConnectionFailed))
    #expect(loadFailureShown(domain: "WebKitErrorDomain", code: 101))
    #expect(loadFailureShown(domain: "OtherDomain", code: -999))

    let failed = WebNote.loadFailed(reason: "The Internet connection appears to be offline.")
    #expect(
        failed.text
            == "claude.ai didn’t load: The Internet connection appears to be offline. Click "
            + "Reload to try again.")
    #expect(failed.isLoadFailure)
    #expect(!WebNote.firstSignIn(email: "a").isLoadFailure)
}

/// WebKit's own Download Linked File, Download Image and Download Video reach the app only
/// through private calls, so they are taken out of the shortcut menu.
@Test func theShortcutMenuLosesOnlyWebKitsOwnDownloads() {
    #expect(!keepsMenuItem("WKMenuItemIdentifierDownloadLinkedFile"))
    #expect(!keepsMenuItem("WKMenuItemIdentifierDownloadImage"))
    #expect(!keepsMenuItem("WKMenuItemIdentifierDownloadMedia"))
    #expect(keepsMenuItem("WKMenuItemIdentifierCopyLink"))
    #expect(keepsMenuItem("WKMenuItemIdentifierOpenLinkInNewWindow"))
    #expect(keepsMenuItem("WKMenuItemIdentifierCopyImage"))
    #expect(keepsMenuItem(""))
}

@Test func mailtoLeavesOnlyFromAClickAndOtherSchemesAreDropped() {
    let mail = "mailto:support@example.com"
    #expect(routed(mail, clicked: true) == .elsewhere(url(mail)))
    #expect(routed(mail, .newWindow) == .elsewhere(url(mail)))
    #expect(routed(mail) == .drop, "a page never sends mail by itself")
    for text in [
        "file:///etc/hosts", "claude://login", "ssh://example.com", "data:text/html,x",
    ] {
        #expect(routed(text, clicked: true) == .drop, "\(text)")
    }
}

/// The same rules on the fixture's origin, which keeps its own and not claude.ai's: a
/// fixture's window sends a real claude.ai link to the recorder rather than the network.
@Test func theFixtureKeepsItsOwnOriginAndNotClaudes() {
    #expect(routed("pitboard-fixture://claude.ai/x", home: fixtureHome) == .load)
    #expect(
        routed("pitboard-fixture://claude.ai/x", .newWindow, home: fixtureHome)
            == .loadHere(url("pitboard-fixture://claude.ai/x")))
    #expect(
        routed("https://claude.ai/new", home: fixtureHome)
            == .elsewhere(url("https://claude.ai/new"))
    )
    #expect(
        routed("https://accounts.google.com/", clicked: true, home: fixtureHome)
            == .refuse(.google))
    #expect(routed("pitboard-fixture://claude.ai/x", home: claude) == .drop)
}

// MARK: - Which store

@Test func anEnrolledClaudeAccountHasAStoreDerivedFromItsAccountId() throws {
    #expect(
        webStoreID(accountUuid: "4f3c2a10-8b7e-4d2a-9c1e-5a6b7c8d9e0f").uuidString.lowercased()
            == "7e15c34f-69ec-55b4-9542-f1c1fe3d7085")
    #expect(
        webStoreID(accountUuid: "dana@work.example").uuidString.lowercased()
            == "323d12fb-2c52-55b5-baec-df74fb60bc24",
        "the fixture's account id")
    #expect(
        webStoreID(accountUuid: "4F3C2A10-8B7E-4D2A-9C1E-5A6B7C8D9E0F")
            == webStoreID(accountUuid: "4f3c2a10-8b7e-4d2a-9c1e-5a6b7c8d9e0f"))
    let id = webStoreID(accountUuid: "anything")
    let bytes = withUnsafeBytes(of: id.uuid) { Array($0) }
    #expect(bytes[6] >> 4 == 5, "version 5")
    #expect(bytes[8] >> 6 == 0b10, "the RFC's variant")
    #expect(id != UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)))
    #expect(webStoreNamespace.uuidString.lowercased() == "674b09f3-8d37-4e48-a361-5af2a6856773")

    let work = account("work", uuid: "4f3c2a10-8b7e-4d2a-9c1e-5a6b7c8d9e0f")
    #expect(
        webStoreID(for: work)?.uuidString.lowercased() == "7e15c34f-69ec-55b4-9542-f1c1fe3d7085"
    )
}

@Test func aRenameKeepsTheStore() {
    let before = account("work", uuid: "4f3c2a10-8b7e-4d2a-9c1e-5a6b7c8d9e0f")
    let after = account("office", uuid: "4f3c2a10-8b7e-4d2a-9c1e-5a6b7c8d9e0f")
    #expect(webStoreID(for: before) != nil)
    #expect(webStoreID(for: before) == webStoreID(for: after))
}

@Test func codexUnnamedAndUnplacedAccountsHaveNoStore() {
    #expect(webStoreID(for: account("main", of: "codex")) == nil)
    #expect(webStoreID(for: account(nil, signedIn: true, uuid: "dana")) == nil)
    #expect(webStoreID(for: unplaced(of: "claude", signedIn: true)) == nil)
    #expect(webStoreID(for: account("work", uuid: "")) == nil)
}

// MARK: - Downloads

@Test func aDownloadIsNamedAfterWhatTheServerSuggestsWithoutOverwriting() {
    let folder = URL(fileURLWithPath: "/Downloads")
    func named(_ suggested: String, taken: Set<String> = []) -> String {
        downloadDestination(suggested: suggested, in: folder) {
            taken.contains($0.lastPathComponent)
        }.lastPathComponent
    }
    #expect(named("report.pdf") == "report.pdf")
    #expect(named("report.pdf", taken: ["report.pdf"]) == "report 2.pdf")
    #expect(named("report.pdf", taken: ["report.pdf", "report 2.pdf"]) == "report 3.pdf")
    #expect(named("notes", taken: ["notes"]) == "notes 2")
    #expect(named("../../etc/passwd") == "passwd")
    #expect(named("a/b/report.pdf") == "report.pdf")
    #expect(named("re:port\u{7}.pdf") == "report.pdf")
    #expect(named(".bashrc") == "bashrc")
    #expect(named("..") == "Download")
    #expect(named("") == "Download")
    #expect(named("/") == "Download")
    #expect(
        downloadDestination(suggested: "a.pdf", in: folder) { _ in false }
            == folder.appendingPathComponent("a.pdf"))
}

/// A name is taken by a file already there or a download of this app still saving to it:
/// WebKit refuses a file that exists, and two downloads can ask at once.
@Test func aDownloadNameIsTakenByAFileOrAnotherDownload() throws {
    let folder = FileManager.default.temporaryDirectory
        .appendingPathComponent("pitboard-tests-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: folder) }
    let there = folder.appendingPathComponent("report.pdf")
    try Data("x".utf8).write(to: there)
    let saving = folder.appendingPathComponent("notes.txt")
    let free = folder.appendingPathComponent("other.pdf")
    #expect(downloadTaken(there, reserved: []))
    #expect(downloadTaken(saving, reserved: [saving]))
    #expect(!downloadTaken(free, reserved: [saving]))
    #expect(
        downloadDestination(suggested: "report.pdf", in: folder) {
            downloadTaken($0, reserved: [folder.appendingPathComponent("report 2.pdf")])
        } == folder.appendingPathComponent("report 3.pdf"))
}

@Test func aDownloadIsQuarantinedAsAWebDownload() throws {
    let web = quarantine(downloadedFrom: url("https://claude.ai/files/a.pdf"))
    #expect(web == Quarantine(agent: "pitboard", origin: url("https://claude.ai/files/a.pdf")))
    #expect(quarantine(downloadedFrom: url("blob:https://claude.ai/1")).origin == nil)
    #expect(quarantine(downloadedFrom: nil).origin == nil)

    let properties = web.properties
    #expect(
        properties[kLSQuarantineTypeKey as String] as? String
            == kLSQuarantineTypeWebDownload as String)
    #expect(properties[kLSQuarantineAgentNameKey as String] as? String == "pitboard")
    #expect(
        properties[kLSQuarantineDataURLKey as String] as? URL
            == url("https://claude.ai/files/a.pdf"))
}

// MARK: - Zoom

@Test func zoomStepsFollowSafari() {
    #expect(zoom(from: 1, toward: .zoomIn) == 1.15)
    #expect(zoom(from: 1.15, toward: .zoomIn) == 1.25)
    #expect(zoom(from: 1, toward: .zoomOut) == 0.85)
    #expect(zoom(from: 0.85, toward: .zoomOut) == 0.75)
    #expect(zoom(from: 3, toward: .zoomIn) == 3, "the largest stays")
    #expect(zoom(from: 0.5, toward: .zoomOut) == 0.5, "the smallest stays")
    #expect(zoom(from: 2.5, toward: .actualSize) == 1)
    #expect(zoom(from: 1.1, toward: .zoomIn) == 1.15, "between steps, the next one up")
    #expect(zoom(from: 1.1, toward: .zoomOut) == 1, "between steps, the next one down")
}

// MARK: - What the windows say

@Test func theFirstSignInAndGoogleNotesNameTheAccountAndSayWhatToDo() {
    let first = WebNote.firstSignIn(email: "dana@work.example").text
    #expect(first.contains("Sign in to claude.ai as dana@work.example."))
    #expect(first.contains("Continue with email"))
    #expect(first.contains("code"))
    #expect(first.contains("tap its link"), "the email holds a link, and the code comes after")
    let google = WebNote.googleRefused(email: "dana@work.example").text
    #expect(google.contains("pitboard stopped it"))
    #expect(google.contains("Continue with email with dana@work.example"))
    #expect(google.contains("tap its link"))
    #expect(
        google.contains(
            "Gmail, Google Drive, Google Calendar and Google single sign-on cannot be "
                + "connected in this window."))
    #expect(
        WebNote.openedInBrowser(label: "work").text
            == "claude.ai opened a page in your browser. Anything you connect there goes to "
            + "the claude.ai account your browser is signed in to, which may not be “work”.")
    #expect(
        WebNote.windowNotShown.text == "claude.ai tried to open a window pitboard doesn’t show."
    )
    #expect(
        downloadQuestion(name: "a.csv", host: "www.claudeusercontent.com")
            == "Download “a.csv” from www.claudeusercontent.com?")
    #expect(downloadQuestion(name: "a.csv", host: nil) == "Download “a.csv”?")
    #expect(
        downloadHost(frame: "www.claudeusercontent.com", url: url("blob:https://x.example/1"))
            == "www.claudeusercontent.com")
    #expect(
        downloadHost(frame: "", url: url("blob:https://www.claudeusercontent.com/1"))
            == "www.claudeusercontent.com", "a sandboxed frame's origin names nothing")
    #expect(downloadHost(frame: nil, url: url("https://example.com/a.pdf")) == "example.com")
    #expect(downloadHost(frame: nil, url: url("data:text/plain,hi")) == nil)
    #expect(downloadHost(frame: nil, url: nil) == nil)
    #expect(downloadQuestion(name: "a.csv", host: "") == "Download “a.csv”?")

    let file = URL(fileURLWithPath: "/Downloads/report.pdf")
    #expect(WebNote.downloading(name: "report.pdf").inProgress)
    #expect(
        WebNote.downloaded(name: "report.pdf", file: file).text
            == "Downloaded “report.pdf” to Downloads.")
    #expect(WebNote.downloaded(name: "report.pdf", file: file).file == file)
    #expect(WebNote.firstSignIn(email: "a").file == nil)
    #expect(
        WebNote.downloadFailed(name: "report.pdf", reason: "The network went away.").text
            == "Couldn’t download “report.pdf”: The network went away.")
}

@Test func theMenuOffersOneItemForOneAccountAndASubmenuForSeveral() {
    let one = claudeWindows(in: status([account("work", signedIn: true)]))
    guard case .one(let only) = ClaudeMenu(one) else {
        Issue.record("one account is one item")
        return
    }
    #expect(only.label == "work")
    #expect(ClaudeMenu(one).title == "Open claude.ai as work")

    let several = claudeWindows(
        in: status([
            account("work", signedIn: true), account("main", of: "codex", signedIn: true),
            account("personal"), account(nil, signedIn: true, uuid: "new"),
            unplaced(of: "claude"), account("old"),
        ]))
    #expect(several.map(\.label) == ["work", "personal", "old"], "in the core's order")
    #expect(several.map(\.inUse) == [true, false, false])
    #expect(ClaudeMenu(several).title == "Open claude.ai")
    #expect(ClaudeMenu(several) == .several(several))

    #expect(ClaudeMenu(claudeWindows(in: status([account("main", of: "codex")]))) == .none)
    #expect(ClaudeMenu(claudeWindows(in: status([account(nil, signedIn: true)]))) == .none)
    #expect(ClaudeMenu(claudeWindows(in: status([]))) == .none)
    #expect(ClaudeMenu(claudeWindows(in: nil)) == .none)
    #expect(ClaudeMenu(claudeWindows(in: nil)).title == nil)
}

/// A label in running text is quoted, as the forget alert quotes it, and never starts a
/// sentence: "for work" would read as "for work purposes".
@Test func theSignOutAlertQuotesTheLabel() {
    #expect(
        signOutMessage(label: "work")
            == "pitboard deletes this window’s sign-in and everything else claude.ai keeps for "
            + "“work” on this Mac. The account stays signed in on your other devices and "
            + "browsers. claude.ai’s own Log Out would sign it out on every device.")
    #expect(signOutMessage(label: nil).contains("keeps for this account on this Mac."))
}

@Test func forgettingAClaudeAccountSaysItsClaudeSignInGoesToo() {
    #expect(forgetMessage(for: account("work")).contains("claude.ai window keeps"))
    #expect(!forgetMessage(for: account("main", of: "codex")).contains("claude.ai"))
    #expect(
        forgetMessage(for: account("main", of: "codex"))
            == "pitboard deletes the login it parked for this account. Using it again needs a "
            + "sign-in in your browser.")
}
