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

@MainActor
private func height(_ family: Family, _ entry: GlanceEntry) -> CGFloat {
    let measured = NSHostingView(
        rootView: family.view(entry).fixedSize(horizontal: false, vertical: true)
            .frame(width: family.size.width - 2 * margin))
    return measured.fittingSize.height
}

@MainActor
@Test(arguments: Family.allCases)
private func eachWidgetDrawsItsAccountsInEveryLookWithinItsSize(_ family: Family) throws {
    let entries = [
        "now": GlanceEntry(date: readAt, glance: try goldenGlance()),
        "reset": GlanceEntry(
            date: readAt.addingTimeInterval(2 * 3_600), glance: try goldenGlance()),
        "empty": GlanceEntry(date: readAt, glance: nil),
    ]
    for (state, entry) in entries {
        #expect(height(family, entry) <= family.size.height - 2 * margin, "\(family) \(state)")
        for (look, mode) in [("full", WidgetRenderingMode.fullColor), ("tinted", .accented)] {
            for dark in [false, true] {
                let picture = try draw(
                    family, entry, mode: mode, dark: dark,
                    named: "\(family)-\(state)-\(look)-\(dark ? "dark" : "light")")
                #expect(picture.pixelsWide > 0)
            }
        }
    }
}
