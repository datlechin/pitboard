import AppKit
import PitboardKit
import SwiftUI
import Testing

@testable import PitboardApp

// Every sentence a view shows is the model's. These test what the views decide themselves.

@Test func eachPlaceHasAWindowOfItsOwn() {
    #expect(PlaceWindow(nil) == .accounts, "the accounts, as a failure asks")
    #expect(PlaceWindow(.accounts) == .accounts)
    #expect(PlaceWindow(.welcome) == .accounts)
    #expect(PlaceWindow(.activity) == .activity)
    #expect(PlaceWindow(.checks) == .diagnostics)
    #expect(PlaceWindow(.settings(section: .limits)) == nil, "the settings are a scene")
    #expect(PlaceWindow.allCases.map(\.id) == ["accounts", "activity", "diagnostics"])
}

/// The window opens once for each request, whether the menu bar item first sees it as it
/// appears, which is how the first launch's request can reach it, or as the serial moves,
/// and the first snapshot's serial asks for nothing. MenuBarLabel asks this both ways, and
/// the UI tests' testTheFirstLaunchOpensTheWindow sees the window it opens.
@MainActor
@Test func theWindowOpensOnceForEachRequest() {
    let requests = WindowRequests()
    #expect(!requests.opens(WindowRequest(serial: 0, place: nil)), "nothing asked yet")
    #expect(requests.opens(WindowRequest(serial: 1, place: nil)), "as the label appears")
    #expect(!requests.opens(WindowRequest(serial: 1, place: nil)), "and not as it moves")
    #expect(requests.opens(WindowRequest(serial: 3, place: .checks)))
    #expect(!requests.opens(WindowRequest(serial: 2, place: nil)), "an older one")
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

// The inspector's buttons are what the shortcut menu shows.
@Test func aShortcutMenuHidesWhatCannotBeChosenAndTheMenuBarDimsIt() {
    let now = AccountAction(
        kind: .warmUpNow, title: "Warm Up Now", intent: .warmNow(qualified: "claude/work"),
        window: nil, enabled: false, confirm: nil)
    let rename = AccountAction(
        kind: .rename, title: "Rename…",
        intent: .presentSheet(sheet: .rename(provider: "claude", label: "work")), window: nil,
        enabled: true, confirm: nil)
    #expect(AccountMenuItems.shown([rename, now], in: .shortcutMenu) == [rename])
    #expect(AccountMenuItems.shown([rename, now], in: .menuBar) == [rename, now])
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

// A limit's bar is AppKit's level indicator, which took a click on it from the list.
@MainActor
@Test func aClickOnALimitsBarReachesItsRow() {
    let limit = LimitRow(
        kind: .fiveHour, name: "5-hour limit", short: "5-hour", percent: 60, figure: "60%",
        used: "60% used", level: .plenty, resetsAt: nil, resets: "Resets 3:22 AM",
        resetsIn: "in 2 hr", spoken: "5-hour limit, 60 percent used")
    let bars = NSHostingView(rootView: UsageBars(limits: [limit]).frame(width: 480))
    let window = NSWindow(
        contentRect: NSRect(origin: .zero, size: bars.fittingSize), styleMask: .borderless,
        backing: .buffered, defer: true)
    window.contentView = bars
    bars.layoutSubtreeIfNeeded()

    let hits = stride(from: 0.0, to: bars.bounds.height, by: 2).flatMap { y in
        stride(from: 0.0, to: bars.bounds.width, by: 2).compactMap { x in
            bars.hitTest(NSPoint(x: x, y: y))
        }
    }

    #expect(!hits.isEmpty)
    #expect(hits.allSatisfy { $0 === bars })
}
