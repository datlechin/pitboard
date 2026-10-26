import AppKit
import PitboardKit
import PitboardTestSupport
import SwiftUI
import Testing

@testable import PitboardApp

// Plan 4.3's sizes: the inspector widens the window rather than narrowing the list.
@MainActor
@Test func theInspectorWidensTheWindowRatherThanNarrowingTheList() async throws {
    let shown = try await accountsWindow()
    #expect(shown.contentMinSize == NSSize(width: 820, height: 400))
    shown.setContentSize(NSSize(width: 560, height: 400))
    shown.layoutIfNeeded()
    #expect(shown.contentLayoutRect.width == 820, "the window is no narrower than both")

    let inspector = try #require(splitView(in: shown.contentView))
    inspector.toggleInspector(nil)
    await settle()
    #expect(shown.contentMinSize == NSSize(width: 560, height: 400))
}

@MainActor
func accountsWindow(_ items: [AccountItem] = sample, notices: [PanelNotice] = []) async throws
    -> NSWindow
{
    let model = AppModel(model: StandInModel(snapshot(0)))
    model.apply(
        snapshot(
            1, updated: "Updated 1:36 PM",
            sections: [AccountSection(id: "claude", heading: nil, accounts: items)],
            notices: notices))
    let controller = NSHostingController(
        rootView: AccountsWindow(model: model, windows: standInWindows(model)))
    controller.sizingOptions = [.minSize]
    // Deferred: one that reaches the window server lets another test's AppleScript end the run
    // (docs/facts/macos-app.md).
    let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 820, height: 560),
        styleMask: [.titled, .closable, .resizable, .miniaturizable], backing: .buffered,
        defer: true)
    window.contentViewController = controller
    await settle()
    return window
}

// SwiftUI lays out a window it has just been handed over a few turns of the main run loop.
@MainActor
private func settle() async {
    for _ in 0..<10 {
        try? await Task.sleep(for: .milliseconds(30))
    }
}

// What the inspector's own command, Control-Command-I, sends.
@MainActor
private func splitView(in view: NSView?) -> NSSplitViewController? {
    guard let view else { return nil }
    if let split = (view as? NSSplitView)?.delegate as? NSSplitViewController { return split }
    return view.subviews.lazy.compactMap { splitView(in: $0) }.first
}

private func limit(_ kind: LimitKind, _ percent: Double, resets: String, in left: String)
    -> LimitRow
{
    let (name, short) =
        kind == .fiveHour ? ("5-hour limit", "5-hour") : ("Weekly limit", "Weekly")
    let figure = "\(Int(percent))%"
    return LimitRow(
        kind: kind, name: name, short: short, percent: percent, figure: figure,
        used: "\(figure) used", level: percent >= 100 ? .out : percent >= 80 ? .low : .plenty,
        resetsAt: nil, resets: resets, resetsIn: left,
        spoken: "\(name), \(Int(percent)) percent used, \(resets.lowercased())")
}

func account(
    _ label: String, plan: String, standing: AccountStanding, limits: [LimitRow],
    note: String? = nil, signIn: String, summary: String = ""
) -> AccountItem {
    let inUse = standing == .inUse
    let qualified = "claude/\(label)"
    let email = "dana@\(label).example"
    let switchTo = Intent.switchTo(qualified: qualified)
    let actions = [
        AccountAction(
            kind: .switch, title: "Switch to \(label)", intent: switchTo, window: nil,
            enabled: !inUse, confirm: nil),
        AccountAction(
            kind: .rename, title: "Rename…",
            intent: .presentSheet(sheet: .rename(provider: "claude", label: label)),
            window: nil, enabled: true, confirm: nil),
        AccountAction(
            kind: .remove, title: "Remove Account…", intent: .forget(qualified: qualified),
            window: nil, enabled: !inUse, confirm: nil),
    ]
    return AccountItem(
        id: qualified, qualified: qualified, title: label, email: email, emailLine: email,
        plan: plan, menuTitle: label, spoken: label, inUse: inUse,
        needsSignIn: standing == .needsSignIn, standing: standing, unplaced: false,
        end: inUse
            ? .inUse(title: "In Use")
            : .button(
                action: ItemAction(
                    title: "Switch", spoken: "Switch to \(label)", intent: switchTo)
            ),
        action: inUse ? nil : ItemAction(title: "Switch", spoken: label, intent: switchTo),
        summary: summary,
        notes: note.map { [AccountNote(kind: .problem, text: $0, action: nil, learn: nil)] }
            ?? [],
        lines: [],
        detail: AccountDetail(
            email: email, plan: plan, signedInUntil: nil, signIn: signIn,
            warm: WarmDetail(
                on: false, said: "Off", nextAt: nil, runningUntil: nil,
                settings: Choice(
                    title: "Warm-Up Settings…",
                    intent: .show(place: .settings(section: .limits)), enabled: true)),
            readAt: nil, read: nil),
        help: nil, limits: limits, offers: [], windows: [], forget: nil, actions: actions)
}

let sample = [
    account(
        "work", plan: "Team 5x", standing: .inUse,
        limits: [
            limit(.fiveHour, 42, resets: "Resets 3:45 PM", in: "in 2 hr 9 min"),
            limit(.weekly, 12, resets: "Resets Mon 9:00 AM", in: "in 19 hr 24 min"),
        ], signIn: "In use"),
    account(
        "personal", plan: "Max 5x", standing: .ready,
        limits: [
            limit(.fiveHour, 100, resets: "Resets 6:46 PM", in: "in 5 hr 10 min"),
            limit(.weekly, 61, resets: "Resets Thu 2:00 PM", in: "in 2 days"),
        ], signIn: "Saved sign-in valid until Nov 20"),
    account(
        "old", plan: "Pro", standing: .needsSignIn, limits: [], note: "Sign-in expired",
        signIn: "Sign-in expired"),
]
