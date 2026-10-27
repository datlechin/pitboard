import AppKit
import Foundation
import PitboardGlance
import PitboardWidgetViews
import SwiftUI
import Testing
import WidgetKit

private let golden = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent()
    .appending(path: "crates/pitboard-ffi/src/present/glance.json")

private let readAt = Date(timeIntervalSince1970: 1_800_000_000)

private func goldenGlance() throws -> Glance {
    try #require(Glance(try Data(contentsOf: golden)))
}

@Test func aTimelineHasAnEntryNowAndAtEachResetToComeAndNeverAsksForMore() throws {
    let timeline = GlanceEntry.timeline(
        of: try goldenGlance(), chosen: "c17c8cd1-64cc-5e37-bb6c-a31d984738ec",
        at: readAt.addingTimeInterval(60))
    #expect(
        timeline.entries.map(\.date.timeIntervalSince1970) == [
            1_800_000_060, 1_800_001_800, 1_800_003_600, 1_800_259_200,
        ])
    #expect(timeline.entries.allSatisfy { $0.chosen == "c17c8cd1-64cc-5e37-bb6c-a31d984738ec" })
    #expect(timeline.policy == .never)
    #expect(GlanceEntry.timeline(of: nil, at: readAt).entries.map(\.date) == [readAt])
}

@Test func aSmallWidgetShowsTheAccountChosenOrElseTheOneInUse() throws {
    let glance = try goldenGlance()
    #expect(glance.account("c17c8cd1-64cc-5e37-bb6c-a31d984738ec")?.name == "personal")
    #expect(glance.account(nil)?.name == "work")
    #expect(glance.account("gone")?.name == "work")
    #expect(glance.inUse.map(\.name) == ["work", "main"])
}

// A Claude Desktop account has no usage to show beside the others in use.
@Test func aMediumWidgetShowsTwoAccountsInUseThoseWithUsageFirst() throws {
    let three = try glance(at: readAt, withDesktop)
    #expect(three.inUse.map(\.name) == ["work", "main", "desktop"])
    #expect(three.medium.map(\.name) == ["work", "main"])
    let noCodex = try glance(at: readAt) {
        withDesktop($0).filter { $0["tool"] as? String != "codex" }
    }
    #expect(noCodex.medium.map(\.name) == ["work"])
    let desktopAlone = try glance(at: readAt) {
        withDesktop($0).filter { $0["tool"] as? String == "desktop" }
    }
    #expect(desktopAlone.medium.map(\.name) == ["desktop"])
}

@Test func aControlNamesItsAccountWithItsTightestLimitOrInUse() throws {
    let glance = try goldenGlance()
    #expect(
        ControlShown(glance, chosen: "c17c8cd1-64cc-5e37-bb6c-a31d984738ec", app: "Pitboard")
            == ControlShown(name: "personal", value: "75%"))
    #expect(
        ControlShown(glance, chosen: nil, app: "Pitboard")
            == ControlShown(name: "work", value: "In Use"))
    #expect(
        ControlShown(nil, chosen: nil, app: "Pitboard")
            == ControlShown(name: "Pitboard", value: ""))
}

// Each family's content is checked to fit these sizes inside the HIG's tight 11-point margins.
private enum Family: CaseIterable {
    case small, medium, large

    var size: CGSize {
        switch self {
        case .small: CGSize(width: 158, height: 158)
        case .medium: CGSize(width: 338, height: 158)
        case .large: CGSize(width: 338, height: 354)
        }
    }

    @MainActor
    func view(_ entry: GlanceEntry) -> AnyView {
        switch self {
        case .small: AnyView(AccountView(entry: entry))
        case .medium: AnyView(AccountsView(entry: entry, family: .systemMedium))
        case .large: AnyView(AccountsView(entry: entry, family: .systemLarge))
        }
    }
}

private let margin: CGFloat = 11

// Drawn as AppKit draws a window, which draws Gauge as it is, where ImageRenderer does not.
// PITBOARD_WIDGET_RENDERS names a folder to keep each picture in.
@MainActor
private func draw(
    _ family: Family, _ entry: GlanceEntry, mode: WidgetRenderingMode, dark: Bool, named: String
) throws -> NSBitmapImageRep {
    let size = family.size
    let content = family.view(entry)
        .environment(\.widgetRenderingMode, mode)
        .padding(margin)
        .frame(width: size.width, height: size.height)
        .background(Color(nsColor: .windowBackgroundColor))
    let host = NSHostingView(rootView: content)
    host.frame = NSRect(origin: .zero, size: size)
    host.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
    host.layout()
    let picture = try #require(host.bitmapImageRepForCachingDisplay(in: host.bounds))
    host.cacheDisplay(in: host.bounds, to: picture)
    if let folder = ProcessInfo.processInfo.environment["PITBOARD_WIDGET_RENDERS"] {
        let file = URL(fileURLWithPath: folder).appending(path: "\(named).png")
        try picture.representation(using: .png, properties: [:])?.write(to: file)
    }
    return picture
}

// The size the content takes when the widget offers it its own, as WidgetKit does: more than
// that is cut off.
@MainActor
private func taken(_ family: Family, _ entry: GlanceEntry) -> CGSize {
    let room = CGSize(
        width: family.size.width - 2 * margin, height: family.size.height - 2 * margin)
    return NSHostingController(rootView: family.view(entry)).sizeThatFits(in: room)
}

// The golden's instants moved to now, since the system counts a countdown from the clock, and
// its accounts as `change` makes them.
private func glance(
    at now: Date, _ change: ([[String: Any]]) -> [[String: Any]] = { $0 }
) throws -> Glance {
    let golden = try JSONSerialization.jsonObject(with: Data(contentsOf: golden))
    var root = try #require(moved(golden, by: now.timeIntervalSince(readAt)) as? [String: Any])
    root["accounts"] = change(try #require(root["accounts"] as? [[String: Any]]))
    return try #require(Glance(try JSONSerialization.data(withJSONObject: root)))
}

private func moved(_ value: Any, by shift: TimeInterval) -> Any {
    switch value {
    case let object as [String: Any]:
        object.reduce(into: [String: Any]()) { object, field in
            if field.key.hasSuffix("_at"), let at = field.value as? Double {
                object[field.key] = at + shift
            } else {
                object[field.key] = moved(field.value, by: shift)
            }
        }
    case let array as [Any]: array.map { moved($0, by: shift) }
    default: value
    }
}

// The golden's accounts and a Claude Desktop account in use, which has no usage.
private func withDesktop(_ accounts: [[String: Any]]) -> [[String: Any]] {
    var desktop = accounts[0]
    desktop["id"] = "desktop"
    desktop["tool"] = "desktop"
    desktop["tool_name"] = "Claude Desktop"
    desktop["name"] = "desktop"
    desktop["plan"] = nil
    desktop["said"] = "No usage read"
    desktop["value"] = "In Use"
    desktop["limits"] = [Any]()
    return accounts + [desktop]
}

// Six Claude Code accounts with three limits each, more than a large widget has room for.
private func crowded(_ accounts: [[String: Any]]) -> [[String: Any]] {
    let limits =
        (accounts[0]["limits"] as? [Any] ?? [])
        + (accounts[1]["limits"] as? [Any] ?? []).suffix(1)
    return (1...6).map { copy in
        var account = accounts[0]
        account["id"] = "work-\(copy)"
        account["name"] = "work-\(copy)"
        account["in_use"] = copy == 1
        account["standing"] = copy == 1 ? "in_use" : "ready"
        account["limits"] = limits
        return account
    }
}

// Each picture is named as the UI tests' are, "macos-widget-small-data", then "-tinted" and
// "-dark" where it is in that look, for pull requests (macos-widget-pictures).
@MainActor
@Test(arguments: Family.allCases)
private func eachWidgetDrawsItsAccountsInEveryLookWithinItsSize(_ family: Family) throws {
    let now = Date.now
    let data = try glance(at: now)
    var entries = [
        "data": GlanceEntry(date: now, glance: data),
        "reset": GlanceEntry(date: now.addingTimeInterval(2 * 3_600), glance: data),
        "no-glance": GlanceEntry(date: now, glance: nil),
        "no-accounts": GlanceEntry(date: now, glance: try glance(at: now) { _ in [] }),
        "none-in-use": GlanceEntry(
            date: now,
            glance: try glance(at: now) {
                $0.map { account in
                    var account = account
                    account["in_use"] = false
                    account["standing"] = "ready"
                    return account
                }
            }),
        "one-tool": GlanceEntry(
            date: now,
            glance: try glance(at: now) { $0.filter { $0["tool"] as? String == "claude" } }),
        "three-tools": GlanceEntry(date: now, glance: try glance(at: now, withDesktop)),
        "crowded": GlanceEntry(date: now, glance: try glance(at: now, crowded)),
    ]
    // main, whose five-hour limit is used up
    if family == .small, let main = data.accounts.first(where: { $0.name == "main" }) {
        entries["at-limit"] = GlanceEntry(date: now, glance: data, chosen: main.id)
    }
    for (state, entry) in entries {
        #expect(
            taken(family, entry).height <= family.size.height - 2 * margin, "\(family) \(state)"
        )
        for (look, mode) in [("", WidgetRenderingMode.fullColor), ("-tinted", .accented)] {
            for dark in [false, true] {
                let picture = try draw(
                    family, entry, mode: mode, dark: dark,
                    named: "macos-widget-\(family)-\(state)\(look)\(dark ? "-dark" : "")")
                #expect(picture.pixelsWide > 0)
            }
        }
    }
}
