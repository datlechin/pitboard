import AppKit
import PitboardKit
import PitboardTestSupport
import SwiftUI
import Testing

@testable import PitboardApp

@MainActor
@Observable
private final class Waiting: Updates {
    var available: Bool { true }
    var waiting: String? { "1.1.0" }
    var checksAutomatically = true
    var installsAutomatically = false
    func check() {}
}

private let signInToOld = MenuEntry(
    title: "Sign In to old Again…", subtitle: "Sign-in expired", help: nil, severity: nil,
    intent: .presentSheet(sheet: .signInAgain(provider: "claude", label: "old")), link: nil,
    enabled: true)

private let issue = MenuEntry(
    title: "1 Issue", subtitle: "A switch didn’t finish",
    help: "Claude Code may be signed in to neither account.", severity: .warning,
    intent: .show(place: .accounts), link: nil, enabled: true)

private let advice = MenuEntry(
    title: "Switch to personal", subtitle: "work reached its 5-hour limit", help: nil,
    severity: nil, intent: .switchTo(qualified: "claude/personal"), link: nil, enabled: true)

private let accounts = AccountSection(
    id: "claude", heading: "Claude Code",
    accounts: [
        account(
            "work", plan: "Team 5x", standing: .inUse, limits: [], signIn: "In use",
            summary: "5-hour 42% used, Weekly 12%"),
        account(
            "personal", plan: "Max 5x", standing: .ready, limits: [], signIn: "",
            summary: "5-hour limit reached, resets 6:46 PM"),
    ])

// Its items as AppKit has them, SwiftUI's menu bar item menu being an NSMenu too.
@MainActor
@available(macOS 15, *)
private func menu(_ menuNotices: MenuNotices, updates: any Updates = Waiting())
    -> (NSMenu, StandInModel)
{
    let standIn = StandInModel(snapshot(0))
    let model = AppModel(model: standIn)
    let claude = window("work", inUse: true, store: "0b5f1a7e-3c4d-5e6f-8a9b-0c1d2e3f4a5b")
    model.apply(
        snapshot(
            1, updated: "Updated 1:36 PM",
            windows: windowsShown(
                [claude], menus: [.one(title: "Open claude.ai as work", window: claude)]),
            sections: [accounts], menuNotices: menuNotices))
    let menu = NSHostingMenu(
        rootView: MenuBarContent(model: model, windows: standInWindows(model), updates: updates)
    )
    menu.update()
    return (menu, standIn)
}

@MainActor
@available(macOS 15, *)
private func shown(_ menu: NSMenu) -> [String] {
    menu.items.filter { !$0.isSeparatorItem }.map { item in
        let subtitle = item.subtitle.map { " / \($0)" } ?? ""
        let key = item.keyEquivalent.isEmpty ? "" : " ⌘\(item.keyEquivalent)"
        let mark = item.isSectionHeader ? "# " : item.state == .on ? "✓ " : ""
        return mark + item.title + subtitle + key
    }
}

// Plan 4.2's order, with what needs attention only while something does.
@MainActor
@available(macOS 15, *)
@Test func theMenuListsWhatNeedsAttentionThenTheAccountsThenItsCommands() {
    let (both, _) = menu(
        MenuNotices(install: nil, switches: [], signIns: [signInToOld], others: nil))
    #expect(
        shown(both) == [
            "Sign In to old Again… / Sign-in expired",
            "Install Pitboard 1.1.0…",
            "# Claude Code",
            "✓ work / 5-hour 42% used, Weekly 12%",
            "personal / 5-hour limit reached, resets 6:46 PM",
            "Add Account… ⌘n",
            "Refresh / Updated 1:36 PM ⌘r",
            "Open claude.ai as work",
            "Open Pitboard",
            "Settings… ⌘,",
            "Check for Updates…",
            "About Pitboard",
            "Pitboard Help",
            "Quit Pitboard ⌘q",
        ])
    let (none, _) = menu(
        MenuNotices(install: nil, switches: [], signIns: [], others: nil), updates: NoUpdates())
    #expect(shown(none).first == "# Claude Code")
}

// The checkmark column is the menu's only mark, and an item says in its subtitle what a
// tooltip, which the keyboard never shows, said.
@MainActor
@available(macOS 15, *)
@Test func noItemCarriesASymbolOrATooltip() {
    let (menu, _) = menu(
        MenuNotices(
            install: nil, switches: [advice], signIns: [signInToOld], others: issue))
    #expect(
        shown(menu).prefix(4) == [
            "Switch to personal / work reached its 5-hour limit",
            "Sign In to old Again… / Sign-in expired",
            "1 Issue / A switch didn’t finish",
            "Install Pitboard 1.1.0…",
        ])
    for item in menu.items {
        #expect(item.image == nil, "\(item.title)")
        #expect((item.toolTip ?? "").isEmpty, "\(item.title)")
    }
}

@MainActor
@available(macOS 15, *)
@Test func choosingASignInToMakeAgainOpensSigningIn() throws {
    let (menu, model) = menu(
        MenuNotices(install: nil, switches: [], signIns: [signInToOld], others: nil))
    let item = try #require(menu.items.first { $0.title == signInToOld.title })
    // As AppKit chooses it: performActionForItem goes through NSApp, which a test has none of.
    let action = try #require(item.action)
    _ = (item.target as AnyObject?)?.perform(action, with: item)
    #expect(model.sent == [try #require(signInToOld.intent)])
}
