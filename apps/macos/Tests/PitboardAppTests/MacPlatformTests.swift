import Foundation
import PitboardKit
import Testing
import UserNotifications

@testable import PitboardApp

// How long an app is given to quit, what a run-out notice says and when a clock time names
// its day are the model's, tested in `pitboard-model`. These test what macOS is asked.

/// Quits when asked only if it would: an app busy with work, or whose person said no, does
/// not.
private final class StandInRunningApp: RunningApp {
    let bundleURL: URL?
    var isTerminated = false
    let quits: Bool
    private(set) var asked: [String] = []

    init(at path: String, quits: Bool) {
        bundleURL = URL(fileURLWithPath: path)
        self.quits = quits
    }

    func terminate() -> Bool {
        asked.append("terminate")
        if quits { isTerminated = true }
        return quits
    }

    func forceTerminate() -> Bool {
        asked.append("forceTerminate")
        isTerminated = true
        return true
    }
}

/// Every running copy is asked the way Command-Q asks. One that declines is still running,
/// and is asked again only when the model asks again.
@Test func anAppIsAskedToQuitAndNeverForced() throws {
    let busy = StandInRunningApp(at: "/Applications/ChatGPT.app", quits: false)
    let other = StandInRunningApp(at: "/Users/x/Applications/ChatGPT.app", quits: true)
    let opened = Opened()
    let apps = MacAppControl(
        copies: { $0 == "com.openai.codex" ? [busy, other] : [] },
        open: { url, front in opened.keep(Opening(url: url, front: front)) })

    #expect(try apps.running(app: "com.openai.codex") == "/Applications/ChatGPT.app")
    #expect(try apps.running(app: "com.example.none") == nil)

    try apps.requestQuit(app: "com.openai.codex")
    #expect(busy.asked == ["terminate"])
    #expect(other.asked == ["terminate"])
    #expect(try apps.running(app: "com.openai.codex") == "/Applications/ChatGPT.app")

    try apps.requestQuit(app: "com.openai.codex")
    #expect(busy.asked == ["terminate", "terminate"], "a copy that quit is not asked again")
    #expect(other.asked == ["terminate"])

    try apps.reopen(location: "/Applications/ChatGPT.app", front: false)
    #expect(
        opened.asked == [
            Opening(url: URL(fileURLWithPath: "/Applications/ChatGPT.app"), front: false)
        ])
}

@Test func anAppComesForwardOnlyWhenAsked() throws {
    let opened = Opened()
    let apps = MacAppControl(
        copies: { _ in [] },
        open: { url, front in opened.keep(Opening(url: url, front: front)) })
    try apps.reopen(location: "/Applications/Claude.app", front: true)
    try apps.reopen(location: "/Applications/Claude.app", front: false)
    let claude = URL(fileURLWithPath: "/Applications/Claude.app")
    #expect(
        opened.asked == [Opening(url: claude, front: true), Opening(url: claude, front: false)])
    #expect(MacAppControl.opening(front: true).activates)
    #expect(!MacAppControl.opening(front: false).activates)
}

private struct Opening: Equatable {
    let url: URL
    let front: Bool
}

private final class Opened: @unchecked Sendable {
    // Unchecked because every read and write holds `lock`.
    private let lock = NSLock()
    private var kept: [Opening] = []
    var asked: [Opening] { lock.withLock { kept } }
    func keep(_ opening: Opening) { lock.withLock { kept.append(opening) } }
}

/// macOS keeps the person's 12 or 24 hours in their locale.
@Test func theClockFollowsThe12Or24HourSetting() throws {
    let utc = TimeZone(identifier: "UTC")!
    // 14:05 UTC on Wednesday 14 January 2026.
    let at: Int64 = 1_768_399_500
    let twelve = MacLocalTime(
        locale: Locale(identifier: "en_US"), calendar: Calendar(identifier: .gregorian),
        timeZone: utc)
    let twentyFour = MacLocalTime(
        locale: Locale(identifier: "en_US@hours=h23"),
        calendar: Calendar(identifier: .gregorian), timeZone: utc)

    #expect(try twelve.clock(epoch: at, withWeekday: false) == "2:05\u{202F}PM")
    #expect(try twentyFour.clock(epoch: at, withWeekday: false) == "14:05")
    #expect(try twentyFour.clock(epoch: at, withWeekday: true) == "Wed 14:05")
    #expect(try twentyFour.dateAndTime(epoch: at) == "Jan 14, 2026 at 14:05")

    #expect(try twentyFour.sameDay(first: at, second: at + 9 * 3600))
    #expect(try !twentyFour.sameDay(first: at, second: at + 10 * 3600), "past midnight in UTC")
}

// "Saved sign-in valid until Nov 20": the person's calendar orders the day and its month.
@Test func aDateIsItsDayAndShortMonth() throws {
    let utc = TimeZone(identifier: "UTC")!
    // 14:05 UTC on Wednesday 14 January 2026.
    let at: Int64 = 1_768_399_500
    let american = MacLocalTime(
        locale: Locale(identifier: "en_US"), calendar: Calendar(identifier: .gregorian),
        timeZone: utc)
    let british = MacLocalTime(
        locale: Locale(identifier: "en_GB"), calendar: Calendar(identifier: .gregorian),
        timeZone: utc)

    #expect(try american.date(epoch: at) == "Jan 14")
    #expect(try british.date(epoch: at) == "14 Jan")
}

/// The warm-up hours are said on the clock the time pickers beside them show.
@Test func aTimeOfDayIsTheClocks() throws {
    let utc = TimeZone(identifier: "UTC")!
    let twelve = MacLocalTime(
        locale: Locale(identifier: "en_US"), calendar: Calendar(identifier: .gregorian),
        timeZone: utc)
    let twentyFour = MacLocalTime(
        locale: Locale(identifier: "en_US@hours=h23"),
        calendar: Calendar(identifier: .gregorian), timeZone: utc)

    #expect(try twelve.timeOfDay(minutes: 22 * 60 + 30) == "10:30\u{202F}PM")
    #expect(try twelve.timeOfDay(minutes: 0) == "12:00\u{202F}AM")
    #expect(try twentyFour.timeOfDay(minutes: 6 * 60) == "06:00")
}

@Test func aRunOutIsPostedWithItsButtonsCategoryAndWhatEachClickHandsBack() {
    let content = MacNotifications.content(
        of: RunOutNotice(
            id: "claude/work/session/1768399500", title: "work has no 5-hour limit left",
            subtitle: "Claude Code", body: "spare has 80% of its own left.",
            clicked: "accounts",
            button: NotificationButton(title: "Switch", clicked: "switch/claude/spare")))
    #expect(content.title == "work has no 5-hour limit left")
    #expect(content.subtitle == "Claude Code")
    #expect(content.body == "spare has 80% of its own left.")
    #expect(content.categoryIdentifier == "Switch")
    #expect(
        MacNotifications.argument(of: MacNotifications.action, in: content.userInfo)
            == "switch/claude/spare")
    #expect(
        MacNotifications.argument(
            of: UNNotificationDefaultActionIdentifier, in: content.userInfo)
            == "accounts")
    #expect(
        MacNotifications.argument(
            of: UNNotificationDismissActionIdentifier, in: content.userInfo)
            == nil)
}

/// No category, so no button, and a click on it still hands back what it asks for.
@Test func aNoticeWithNoButtonHasNoCategory() {
    let content = MacNotifications.content(
        of: RunOutNotice(
            id: "auto-refused/switch_failed", title: "Pitboard couldn’t switch Claude Code",
            subtitle: nil, body: "It was refused.", clicked: "limits", button: nil))
    #expect(content.categoryIdentifier.isEmpty)
    #expect(
        MacNotifications.argument(
            of: UNNotificationDefaultActionIdentifier, in: content.userInfo)
            == "limits")
    #expect(MacNotifications.argument(of: MacNotifications.action, in: content.userInfo) == nil)
}

/// Each button the model can post is registered at launch with its symbol, and only one that
/// opens a window brings Pitboard forward, which macOS refuses the app itself from the background.
@Test func everyButtonHasACategoryOfItsOwn() {
    let buttons = notificationButtons()
    let categories = MacNotifications.categories(buttons)
    #expect(Set(categories.map(\.identifier)) == Set(buttons.map(\.title)))
    for category in categories {
        #expect(category.actions.map(\.title) == [category.identifier])
        let opens = buttons.first { $0.title == category.identifier }?.opensWindow ?? false
        let options: UNNotificationActionOptions = opens ? .foreground : []
        #expect(category.actions.allSatisfy { $0.options == options && $0.icon != nil })
    }
    #expect(buttons.contains { $0.title == "Sign In Again" && $0.opensWindow })
    #expect(buttons.contains { $0.title == "Switch" && !$0.opensWindow })
    #expect(buttons.contains { $0.title == "Switch Back" && !$0.opensWindow })
}

/// A notification an older Pitboard posted has no click of its own, and opens the accounts.
@Test func aClickOnAnOlderNotificationOpensTheAccounts() {
    #expect(
        MacNotifications.argument(of: UNNotificationDefaultActionIdentifier, in: [:]) == "")
}
