import PitboardKit
import SwiftUI

/// A grid, so the columns line up across rows and the name column is as wide as its widest
/// name: a limit scoped to one model is named "Weekly (Sonnet 4.5)", which no fixed width
/// holds at every text size. What each column says is the model's.
struct UsageBars: View {
    let limits: [LimitRow]

    var body: some View {
        Grid(
            alignment: .leading, horizontalSpacing: Design.iconSpacing,
            verticalSpacing: Design.rowSpacing
        ) {
            ForEach(Array(limits.enumerated()), id: \.offset) { _, limit in
                UsageBar(limit: limit)
            }
        }
        .font(.callout)
    }
}

/// VoiceOver hears the row as one sentence, read from its name, and not the bar and
/// each figure on their own.
struct UsageBar: View {
    let limit: LimitRow
    @ScaledMetric(relativeTo: .callout) private var percentWidth: CGFloat = 38
    @ScaledMetric(relativeTo: .callout) private var resetsWidth: CGFloat = 110

    var body: some View {
        GridRow {
            Text(limit.short)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .accessibilityLabel(limit.spoken)
            Gauge(value: min(max(limit.percent, 0), 100), in: 0...100) {
                EmptyView()
            }
            .gaugeStyle(.linearCapacity)
            .tint(limit.level.tint)
            .accessibilityHidden(true)
            Text(limit.figure)
                .monospacedDigit()
                .frame(minWidth: percentWidth, alignment: .trailing)
                .gridColumnAlignment(.trailing)
                .accessibilityHidden(true)
            Text(limit.resets)
                .foregroundStyle(.secondary)
                .monospacedDigit()
                .frame(minWidth: resetsWidth, alignment: .trailing)
                .gridColumnAlignment(.trailing)
                .accessibilityHidden(true)
        }
    }
}
