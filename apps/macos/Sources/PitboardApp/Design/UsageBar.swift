import PitboardKit
import SwiftUI

/// A grid, so the columns line up across rows and the name column is as wide as its widest
/// name: a limit scoped to one model is named "Weekly (Sonnet 4.5)", which no fixed width
/// holds at every text size. What each column says is the model's.
struct UsageBars: View {
    let limits: [LimitRow]

    var body: some View {
        let showsPace = limits.contains { $0.pace != nil }
        Grid(
            alignment: .leading, horizontalSpacing: Design.iconSpacing,
            verticalSpacing: Design.rowSpacing
        ) {
            ForEach(Array(limits.enumerated()), id: \.offset) { _, limit in
                UsageBar(limit: limit, showsPace: showsPace)
            }
        }
        .font(.callout)
    }
}

/// VoiceOver hears the row as one sentence, read from its name, and not the bar and
/// each figure on their own.
struct UsageBar: View {
    let limit: LimitRow
    let showsPace: Bool
    @ScaledMetric(relativeTo: .callout) private var percentWidth: CGFloat = 38
    @ScaledMetric(relativeTo: .callout) private var resetsWidth: CGFloat = 110
    @ScaledMetric(relativeTo: .callout) private var paceWidth: CGFloat = 100

    var body: some View {
        GridRow {
            Text(limit.short)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .accessibilityLabel(limit.spoken)
            UsageGauge(limit: limit)
            Text(limit.figure)
                .monospacedDigit()
                .frame(minWidth: percentWidth, alignment: .trailing)
                .gridColumnAlignment(.trailing)
                .accessibilityHidden(true)
            if showsPace {
                Text(limit.pace?.said ?? "")
                    .foregroundStyle(
                        limit.pace?.standing.wordsStyle ?? AnyShapeStyle(.secondary)
                    )
                    .monospacedDigit()
                    .lineLimit(1)
                    .frame(minWidth: paceWidth, alignment: .trailing)
                    .gridColumnAlignment(.trailing)
                    .help(limit.pace?.help ?? "")
                    .accessibilityHidden(true)
            }
            Text(limit.resets)
                .foregroundStyle(.secondary)
                .monospacedDigit()
                .frame(minWidth: resetsWidth, alignment: .trailing)
                .gridColumnAlignment(.trailing)
                .accessibilityHidden(true)
        }
    }
}

/// The native gauge keeps its usage colour; the separate mark shows the model's expected
/// usage on the same scale. Its background outline stays visible against the fill.
struct UsageGauge: View {
    let limit: LimitRow

    var body: some View {
        Gauge(value: min(max(limit.percent, 0), 100), in: 0...100) {
            EmptyView()
        }
        .gaugeStyle(.linearCapacity)
        .tint(limit.level.tint)
        .overlay { PaceMark(pace: limit.pace) }
        .help(limit.pace?.help ?? "")
        // Neither the native indicator nor its mark may take a click from the account row.
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

private struct PaceMark: View {
    let pace: LimitPace?
    @ScaledMetric(relativeTo: .callout) private var overhang: CGFloat = 3

    var body: some View {
        if let pace, pace.standing != .even {
            GeometryReader { bar in
                let height = bar.size.height + 2 * overhang
                ZStack {
                    Capsule().fill(.background).frame(width: 4, height: height)
                    Capsule().fill(pace.standing.tint).frame(width: 2, height: height)
                }
                .position(
                    x: bar.size.width * min(max(pace.expected, 0), 100) / 100,
                    y: bar.size.height / 2)
            }
            .allowsHitTesting(false)
            .accessibilityHidden(true)
        }
    }
}
