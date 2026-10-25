import AppKit
import PitboardKit
import SwiftUI
import Testing

@testable import PitboardApp

// Every sentence a view shows is the model's. These test what the views decide themselves.

@Test func aRequestNamesThePaneTheWindowShows() {
    for left in WindowPane.allCases {
        for wanted in WindowPane.allCases {
            #expect(left.after(WindowRequest(serial: 1, pane: wanted.pane)) == wanted)
        }
        #expect(left.after(WindowRequest(serial: 2, pane: nil)) == left)
    }
    #expect(WindowPane.allCases.map(\.pane) == [.accounts, .activity, .machine])
    #expect(WindowPane.allCases.allSatisfy { WindowPane($0.pane) == $0 })
}

/// The window opens once for each request, whether the menu bar item first sees it as it
/// appears, which is how the first launch's request can reach it, or as the serial moves,
/// and the first snapshot's serial asks for nothing. MenuBarLabel asks this both ways, and
/// the UI tests' testTheFirstLaunchOpensTheWindow sees the window it opens.
@MainActor
@Test func theWindowOpensOnceForEachRequest() {
    let requests = WindowRequests()
    #expect(!requests.opens(WindowRequest(serial: 0, pane: nil)), "nothing asked yet")
    #expect(requests.opens(WindowRequest(serial: 1, pane: nil)), "as the label appears")
    #expect(!requests.opens(WindowRequest(serial: 1, pane: nil)), "and not as it moves")
    #expect(requests.opens(WindowRequest(serial: 3, pane: .machine)))
    #expect(!requests.opens(WindowRequest(serial: 2, pane: nil)), "an older one")
}

/// Opened again some other way, the window stays on the pane it was left on rather than
/// going back to the one an old request wanted. MainWindow asks this both as the window
/// appears and as the serial moves.
@MainActor
@Test func theWindowShowsThePaneItIsAskedForOnce() {
    let requests = WindowRequests()
    let notice = WindowRequest(serial: 1, pane: .accounts)
    #expect(requests.pane(for: notice, from: .machine) == .accounts)
    #expect(requests.pane(for: notice, from: .activity) == .activity, "answered already")
    #expect(
        requests.pane(for: WindowRequest(serial: 2, pane: nil), from: .activity) == .activity)
    #expect(
        requests.pane(for: WindowRequest(serial: 3, pane: .machine), from: .activity)
            == .machine)
}

/// The raw values are what the preference is stored as, so changing one would quietly reset
/// what everybody chose.
@Test func theMenuBarChoicesKeepTheirNamesAndTheirStoredValues() {
    #expect(MenuBarShows.allCases == [.nameAndUsage, .usage, .icon])
    #expect(
        MenuBarShows.allCases.map(\.title) == ["Account and usage", "Usage only", "Icon only"])
    #expect(MenuBarShows.allCases.map(\.rawValue) == ["nameAndUsage", "usage", "icon"])
    #expect(MenuBarShows.allCases.allSatisfy { $0.id == $0.rawValue })
    let bar = MenuBarText(
        nameAndUsage: "work 42%", usage: "42%", spoken: "Pitboard, work 42%", level: .plenty,
        attention: false, tip: "Pitboard\nwork: 5-hour 42%")
    #expect(MenuBarShows.allCases.map { $0.text(of: bar) } == ["work 42%", "42%", ""])
}

/// If two ever came to look the same, a broken check would read as a passing one and an
/// error would pass for a note.
@Test func everyLevelAndSeverityLooksLikeItself() {
    let levels: [Level] = [.ok, .warn, .fail]
    #expect(Set(levels.map(\.symbol)).count == levels.count)
    let severities: [Severity] = [.info, .warning, .error]
    #expect(Set(severities.map(\.symbol)).count == severities.count)
}

@Test func aShortcutMenuHidesWhatCannotBeChosenAndTheMenuBarDimsIt() {
    let now = ItemOffer(
        title: "Warm Up Now", intent: .warmNow(qualified: "claude/work"), enabled: false,
        confirm: nil)
    let rename = ItemOffer(
        title: "Rename…",
        intent: .presentSheet(sheet: .rename(provider: "claude", label: "work")),
        enabled: true, confirm: nil)
    #expect(AccountMenuItems.shown([rename, now], in: .shortcutMenu) == [rename])
    #expect(AccountMenuItems.shown([rename, now], in: .menuBar) == [rename, now])
}

// Each row has a grid of its own, so an empty pace column widened its bars past another row's.
@MainActor
@Test func theBarsOfTwoRowsLineUpWhetherOrNotAPaceIsSaid() {
    func width(_ pace: LimitPace?) -> CGFloat {
        let limit = LimitRow(
            short: "5h", percent: 42, figure: "42%", level: .plenty,
            resets: "resets in 2h 10m", pace: pace, spoken: "5-hour limit 42% used")
        return NSHostingView(rootView: UsageBars(limits: [limit])).fittingSize.width
    }
    let over = LimitPace(expected: 30, standing: .over, said: "27% over pace", help: "")
    #expect(width(nil) == width(over))
}

/// The UI tests find a control by the model's case it shows or sends, whatever it carries.
@Test func aCaseIsNamedWithoutWhatItCarries() {
    #expect(caseName(Intent.switchTo(qualified: "codex/spare")) == "switchTo")
    #expect(caseName(Intent.abandonStuckSwitch) == "abandonStuckSwitch")
    #expect(
        caseName(Intent.presentSheet(sheet: .rename(provider: "claude", label: "work")))
            == "rename")
    #expect(caseName(Footing.onlyOne(provider: "claude", label: "work")) == "onlyOne")
    #expect(caseName(Sheet.signInAgain(provider: "claude", label: "old")) == "signInAgain")
    #expect(caseName(WindowNoteKind.otherApp(scheme: "vscode")) == "otherApp")
}

/// A sheet put up in place of another is told apart from it, so the window puts it up
/// afresh.
@Test func eachSheetIsToldApart() {
    let sheets: [Sheet] = [
        .add(provider: nil), .add(provider: "codex"),
        .signInAgain(provider: "claude", label: "old"),
        .name(provider: "claude", label: "work", email: "dana@work.example"),
        .rename(provider: "claude", label: "personal"),
        .rename(provider: "codex", label: "personal"),
        .stow,
    ]
    #expect(Set(sheets.map(\.id)).count == sheets.count)
    #expect(
        Sheet.rename(provider: "claude", label: "personal").id
            == Sheet.rename(provider: "claude", label: "personal").id)
}

/// The sign-in page's link is copied in place of whatever was there, as an address for an
/// app that reads one and as text for a field that reads only text, such as a browser's
/// address bar. Written to a pasteboard of the test's own, never the one people copy to.
@Test func theSignInLinkIsCopiedAsAnAddressAndAsText() throws {
    let board = NSPasteboard(name: NSPasteboard.Name("pitboard-test-\(UUID().uuidString)"))
    defer { board.releaseGlobally() }
    board.clearContents()
    board.setString("copied before", forType: .string)
    let link = try #require(
        URL(string: "https://claude.com/cai/oauth/authorize?code=true&state=pitboard-fixture"))

    board.copy(link: link)

    #expect(board.pasteboardItems?.count == 1)
    #expect(board.string(forType: .URL) == link.absoluteString)
    #expect(board.string(forType: .string) == link.absoluteString)
}
