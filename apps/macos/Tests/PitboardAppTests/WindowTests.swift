import PitboardKit
import Testing

@testable import PitboardApp

// What the views decide for themselves: which pane the window opens on, what the menu bar
// item shows of what the model says, and how each level and severity looks. Every sentence
// they show is the model's.

/// A request names the pane it wants, from whichever pane the window was left on, and
/// leaves the window where it is when it wants none; the model's panes and the window's are
/// the same three.
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

/// The window shows the pane a request wants once: as it appears for the request, or as the
/// serial moves while it is open. Opened again some other way, it stays on the pane it was
/// left on rather than going back to the one an old request wanted. MainWindow asks this
/// both ways.
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

/// The settings list what the menu bar can show in this order and by these names. The raw
/// values are what the preference is stored as, so changing one would quietly reset what
/// everybody chose. Each shows the form of the model's words it is named for.
@Test func theMenuBarChoicesKeepTheirNamesAndTheirStoredValues() {
    #expect(MenuBarShows.allCases == [.nameAndUsage, .usage, .icon])
    #expect(
        MenuBarShows.allCases.map(\.title) == ["Account and usage", "Usage only", "Icon only"])
    #expect(MenuBarShows.allCases.map(\.rawValue) == ["nameAndUsage", "usage", "icon"])
    #expect(MenuBarShows.allCases.allSatisfy { $0.id == $0.rawValue })
    let bar = MenuBarText(nameAndUsage: "work 42%", usage: "42%", spoken: "Pitboard, work 42%")
    #expect(MenuBarShows.allCases.map { $0.text(of: bar) } == ["work 42%", "42%", ""])
}

/// A check's standing and a notice's severity are shown as a shape and a colour, and said in
/// the model's words. If two ever came to look the same, a broken check would read as a
/// passing one and an error would pass for a note.
@Test func everyLevelAndSeverityLooksLikeItself() {
    let levels: [Level] = [.ok, .warn, .fail]
    #expect(Set(levels.map(\.symbol)).count == levels.count)
    let severities: [Severity] = [.info, .warning, .error]
    #expect(Set(severities.map(\.symbol)).count == severities.count)
}

/// A sheet the model puts up in place of another is told apart from it, so the window puts
/// it up afresh, and the same sheet asked for again is the one already up.
@Test func eachSheetIsToldApart() {
    let sheets: [Sheet] = [
        .add(provider: nil), .add(provider: "codex"),
        .signInAgain(provider: "claude", label: "old"),
        .name(provider: "claude", email: "dana@work.example"),
        .rename(provider: "claude", label: "personal"),
        .rename(provider: "codex", label: "personal"),
        .stow,
    ]
    #expect(Set(sheets.map(\.id)).count == sheets.count)
    #expect(
        Sheet.rename(provider: "claude", label: "personal").id
            == Sheet.rename(provider: "claude", label: "personal").id)
}
