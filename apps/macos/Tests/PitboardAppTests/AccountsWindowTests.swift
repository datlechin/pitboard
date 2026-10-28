import AppKit
import PitboardKit
import PitboardTestSupport
import SwiftUI
import Testing

@testable import PitboardApp

// The owner's sizes: a 280 point inspector widens the window rather than narrowing the list.
@MainActor
@Test func theInspectorWidensTheWindowRatherThanNarrowingTheList() async throws {
    let shown = try await accountsWindow()
    #expect(shown.contentMinSize == NSSize(width: 840, height: 400))
    shown.setContentSize(NSSize(width: 560, height: 400))
    shown.layoutIfNeeded()
    #expect(shown.contentLayoutRect.width == 840, "the window is no narrower than both")

    let inspector = try #require(splitView(in: shown.contentView))
    inspector.toggleInspector(nil)
    #expect(await eventually { shown.contentMinSize == NSSize(width: 560, height: 400) })
}

// macOS 27's AppKit crashed updating the list's rows and sections at once (docs/facts/macos-app.md).
@MainActor
@Test func theListStartsAfreshWhenAToolComesOrGoesAndOnlyThen() async throws {
    let model = AppModel(model: StandInModel(snapshot(0)))
    let alone = [AccountSection(id: "claude", heading: nil, accounts: sample)]
    model.apply(snapshot(1, sections: alone))
    let shown = await accountsWindow(showing: model)
    let first = try #require(listTable(in: shown.contentView))

    let fewer = AccountSection(id: "claude", heading: nil, accounts: sample.dropLast())
    model.apply(snapshot(2, sections: [fewer]))
    await settle()
    #expect(listTable(in: shown.contentView) === first, "an account went, and the tools stayed")

    model.apply(snapshot(3, sections: twoTools(sample)))
    await settle()
    let second = try #require(listTable(in: shown.contentView))
    #expect(second !== first)

    model.apply(snapshot(4, sections: alone))
    await settle()
    #expect(listTable(in: shown.contentView) !== second)
}

// A switch puts the account in use first, and macOS 27 crashed moving its row there
// (docs/facts/macos-app.md).
@MainActor
@Test func aSwitchDrawsTheRowsAgainWhereTheyStand() async throws {
    let model = AppModel(model: StandInModel(snapshot(0)))
    model.apply(snapshot(1, sections: twoTools(sample)))
    let shown = await accountsWindow(showing: model)
    let table = try #require(listTable(in: shown.contentView))
    let rows = (0..<table.numberOfRows).map { table.rowView(atRow: $0, makeIfNecessary: false) }
    #expect(rows.count == 7 && rows.allSatisfy { $0 != nil }, "two headings and five accounts")
    #expect(table.selectedRowIndexes == [1], "work, the account in use, under its heading")

    let switched = [
        account("personal", plan: "Max 5x", standing: .inUse, limits: [], signIn: "In use"),
        account("work", plan: "Team 5x", standing: .ready, limits: [], signIn: "Saved"),
        sample[2],
    ]
    model.apply(snapshot(2, sections: twoTools(switched)))
    await settle()
    #expect(listTable(in: shown.contentView) === table)
    for (row, view) in rows.enumerated() {
        #expect(table.rowView(atRow: row, makeIfNecessary: false) === view, "row \(row)")
    }
    #expect(table.selectedRowIndexes == [2], "work, now second")
}

private func twoTools(_ claude: [AccountItem]) -> [AccountSection] {
    let codex = ["main", "spare"].map { label in
        let item = account(label, plan: "Plus", standing: .ready, limits: [], signIn: "Saved")
        return AccountItem(
            id: "codex/\(label)", qualified: "codex/\(label)", title: item.title,
            email: item.email, plan: item.plan, menuTitle: item.menuTitle, spoken: item.spoken,
            inUse: item.inUse, needsSignIn: item.needsSignIn, standing: item.standing,
            unplaced: item.unplaced, end: item.end, action: item.action, summary: item.summary,
            notes: item.notes, detail: item.detail, help: item.help, limits: item.limits,
            actions: item.actions)
    }
    return [
        AccountSection(id: "claude", heading: "Claude Code", accounts: claude),
        AccountSection(id: "codex", heading: "Codex", accounts: codex),
    ]
}

// The inspector's form is a table too, at the window's trailing edge.
@MainActor
private func listTable(in view: NSView?) -> NSTableView? {
    func tables(in view: NSView) -> [NSTableView] {
        (view as? NSTableView).map { [$0] } ?? view.subviews.flatMap(tables)
    }
    return view.flatMap { tables(in: $0) }?
        .min { $0.convert($0.bounds, to: nil).minX < $1.convert($1.bounds, to: nil).minX }
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
    return await accountsWindow(showing: model)
}

@MainActor
func accountsWindow(showing model: AppModel) async -> NSWindow {
    let controller = NSHostingController(
        rootView: AccountsWindow(model: model, windows: standInWindows(model)))
    controller.sizingOptions = [.minSize]
    // Deferred: one that reaches the window server lets another test's AppleScript end the run
    // (docs/facts/macos-app.md).
    let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 840, height: 560),
        styleMask: [.titled, .closable, .resizable, .miniaturizable], backing: .buffered,
        defer: true)
    window.contentViewController = controller
    await settle()
    return window
}

// SwiftUI lays out a window it has just been handed over a few turns of the main run loop. The
// minimum it reaches first says nothing of when it follows a toggle (docs/facts/macos-app.md).
@MainActor
private func settle() async {
    for _ in 0..<10 {
        try? await Task.sleep(for: .milliseconds(30))
    }
}

// The bound is only a bound: a minimum that follows reaches it in milliseconds.
@MainActor
private func eventually(_ met: () -> Bool) async -> Bool {
    let bound = ContinuousClock.now + .seconds(5)
    while !met() {
        guard ContinuousClock.now < bound else { return false }
        try? await Task.sleep(for: .milliseconds(10))
    }
    return true
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
        id: qualified, qualified: qualified, title: label, email: email,
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
        detail: AccountDetail(
            email: email, plan: plan, signedInUntil: nil, signIn: signIn,
            warm: WarmDetail(
                on: false, said: "Off", nextAt: nil, runningUntil: nil,
                settings: Choice(
                    title: "Warm-Up Settings…",
                    intent: .show(place: .settings(section: .limits)), enabled: true)),
            readAt: nil, read: nil, notes: []),
        help: nil, limits: limits, actions: actions)
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
