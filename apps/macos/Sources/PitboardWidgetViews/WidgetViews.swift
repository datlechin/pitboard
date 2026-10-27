import Foundation
import PitboardGlance
import SwiftUI
import WidgetKit

// The small widget: one account, and its first two limits.
public struct AccountView: View {
    let entry: GlanceEntry

    public init(entry: GlanceEntry) {
        self.entry = entry
    }

    public var body: some View {
        GlanceLayout(entry: entry, footer: false) { glance in
            if let account = glance.account(entry.chosen) {
                VStack(alignment: .leading, spacing: 6) {
                    AccountHeading(account: account, aside: nil, words: glance.words)
                    Limits(account: account, at: entry.date, words: glance.words)
                }
            } else {
                Said(glance.words.noAccounts)
            }
        }
    }
}

// The medium widget, each account in use beside the next; the large one, every account.
public struct AccountsView: View {
    let entry: GlanceEntry
    let family: WidgetFamily

    public init(entry: GlanceEntry, family: WidgetFamily) {
        self.entry = entry
        self.family = family
    }

    public var body: some View {
        GlanceLayout(entry: entry, footer: family == .systemLarge) { glance in
            if glance.accounts.isEmpty {
                Said(glance.words.noAccounts)
            } else if family == .systemLarge {
                AccountRows(glance: glance, at: entry.date)
            } else if glance.inUse.isEmpty {
                Said(glance.words.noneInUse)
            } else {
                HStack(alignment: .top, spacing: 16) {
                    ForEach(glance.medium) { account in
                        VStack(alignment: .leading, spacing: 6) {
                            AccountHeading(
                                account: account, aside: account.toolName, words: glance.words)
                            Limits(account: account, at: entry.date, words: glance.words)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
        }
    }
}

// Before Pitboard has written a glance there is nothing to say, and a click opens it. Only the
// large widget has room to say when the numbers were read.
struct GlanceLayout<Content: View>: View {
    let entry: GlanceEntry
    let footer: Bool
    @ViewBuilder let content: (Glance) -> Content

    var body: some View {
        Group {
            if let glance = entry.glance {
                VStack(alignment: .leading, spacing: 0) {
                    content(glance)
                    Spacer(minLength: 0)
                    if footer, let updated = glance.words.updated {
                        Text(updated)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .padding(.top, 8)
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            } else {
                Image("pitboard")
                    .font(.largeTitle)
                    .foregroundStyle(.secondary)
                    .accessibilityLabel(Bundle.main.displayName)
            }
        }
        .containerBackground(.fill.tertiary, for: .widget)
    }
}

struct Said: View {
    let text: String

    init(_ text: String) {
        self.text = text
    }

    var body: some View {
        Text(text)
            .font(.callout)
            .foregroundStyle(.secondary)
    }
}

// Its name, what is said beside it where there is room, and In Use or the Switch button.
struct AccountHeading: View {
    let account: GlanceAccount
    let aside: String?
    let words: GlanceWords

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 4) {
            Text(account.name)
                .font(.headline)
                .lineLimit(1)
                .layoutPriority(1)
            if let aside {
                Text(aside)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 4)
            if account.inUse {
                Image(systemName: "checkmark.circle.fill")
                    .foregroundStyle(.tint)
                    .widgetAccentable()
                    .accessibilityLabel(words.inUse)
            } else {
                SwitchButton(account: account, words: words)
            }
        }
    }
}

// Each limit over its gauge and its countdown, or what stands in for them.
struct Limits: View {
    let account: GlanceAccount
    let at: Date
    let words: GlanceWords

    var body: some View {
        if account.limits.isEmpty {
            Text(account.said)
                .font(.caption)
                .foregroundStyle(.secondary)
        } else {
            VStack(alignment: .leading, spacing: 6) {
                ForEach(account.limits.prefix(2), id: \.name) { limit in
                    VStack(alignment: .leading, spacing: 2) {
                        HStack(alignment: .firstTextBaseline) {
                            Text(limit.short)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                            Spacer(minLength: 4)
                            Text(limit.figure)
                                .monospacedDigit()
                        }
                        .font(.caption)
                        UsageGauge(limit: limit)
                        Resets(limit: limit, at: at, words: words)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(limit.spoken)
                }
            }
        }
    }
}

// The large widget: every account, or as many as it has room for, under its tool where there
// are two. One grid, so each limit's columns line up across the accounts.
struct AccountRows: View {
    let glance: Glance
    let at: Date

    var body: some View {
        // every account first, then one fewer, until they fit
        ViewThatFits(in: .vertical) {
            ForEach(glance.accounts.indices.reversed(), id: \.self) {
                rows(Array(glance.accounts[...$0]))
            }
        }
    }

    private func rows(_ shown: [GlanceAccount]) -> some View {
        let tools = shown.reduce(into: [String]()) { tools, account in
            if !tools.contains(account.toolName) { tools.append(account.toolName) }
        }
        // the heading at the very top, which needs no space above it
        let top = tools.count > 1 ? nil : shown.first?.id
        return Grid(alignment: .leading, horizontalSpacing: 6, verticalSpacing: 3) {
            ForEach(tools, id: \.self) { tool in
                if tools.count > 1 {
                    Text(tool)
                        .fontWeight(.semibold)
                        .foregroundStyle(.secondary)
                        .padding(.top, tool == tools.first ? 0 : 5)
                }
                ForEach(shown.filter { $0.toolName == tool }) { account in
                    AccountHeading(account: account, aside: account.plan, words: glance.words)
                        .padding(.top, account.id == top ? 0 : 5)
                    LimitRows(account: account, at: at, words: glance.words)
                }
            }
        }
        .font(.caption)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

// A grid row per limit, its columns and its words as the app's rows have them, or why there is
// none. A countdown here would widen its column as it ticks.
struct LimitRows: View {
    let account: GlanceAccount
    let at: Date
    let words: GlanceWords

    var body: some View {
        if account.limits.isEmpty {
            Text(account.said)
                .foregroundStyle(.secondary)
        } else {
            ForEach(account.limits, id: \.name) { limit in
                GridRow {
                    Text(limit.short)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                    UsageGauge(limit: limit)
                    Text(limit.figure)
                        .monospacedDigit()
                        .gridColumnAlignment(.trailing)
                    Text(limit.resetting(at: at) ? words.resettingNow : limit.resets)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(limit.spoken)
            }
        }
    }
}

// The fill takes the accent in the tinted and clear looks, where every colour turns white, and
// the figure beside it says the same in text.
struct UsageGauge: View {
    let limit: GlanceLimit

    var body: some View {
        Gauge(value: min(max(limit.used, 0), 100), in: 0...100) {
            EmptyView()
        }
        .gaugeStyle(.linearCapacity)
        .tint(limit.level.tint)
        .widgetAccentable()
    }
}

// A countdown the system draws, so it moves with no reload.
struct Resets: View {
    let limit: GlanceLimit
    let at: Date
    let words: GlanceWords

    var body: some View {
        if let reset = limit.resetsAt {
            Group {
                if limit.resetting(at: at) {
                    Text(words.resettingNow)
                } else {
                    Text("\(words.resetsIn) \(Text(reset, style: .relative))")
                        .monospacedDigit()
                }
            }
            .lineLimit(1)
        }
    }
}

// macOS 27 runs it in the app; before that a click on the widget opens Pitboard.
struct SwitchButton: View {
    let account: GlanceAccount
    let words: GlanceWords

    var body: some View {
        #if compiler(>=6.4)
            if #available(macOS 27.0, *),
                account.standing == .ready || account.standing == .expiring
            {
                Button(intent: SwitchButtonIntent(account: AccountEntity(account))) {
                    Text(words.switchButton)
                }
                .controlSize(.small)
            }
        #endif
    }
}

extension GlanceLimit {
    // From its reset until the app's next read brings the new numbers.
    func resetting(at date: Date) -> Bool {
        resetsAt.map { $0 <= date } ?? false
    }
}

extension GlanceLimit.Level {
    var tint: Color {
        switch self {
        case .plenty: .green
        case .low: .orange
        case .out: .red
        }
    }
}

extension Bundle {
    public var displayName: String {
        object(forInfoDictionaryKey: "CFBundleDisplayName") as? String ?? ""
    }
}
