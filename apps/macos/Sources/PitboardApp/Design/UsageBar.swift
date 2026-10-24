import PitboardKit
import SwiftUI

/// A grid, so the columns line up across rows and the name column is as wide as its widest
/// name: a limit scoped to one model is named "week · Sonnet 4.5", which no fixed width
/// holds at every text size. What each column says is the model's, made again every minute,
/// which is as often as "resets in 2h 10m" changes and as often as an even pace moves.
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
    // As wide as "27% over pace", so a row with no pace has bars as wide as one with.
    @ScaledMetric(relativeTo: .callout) private var paceWidth: CGFloat = 100

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
            .overlay { PaceMark(pace: limit.pace) }
            .help(limit.pace?.help ?? "")
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
            Text(limit.pace?.said ?? "")
                .foregroundStyle(limit.pace?.standing.wordsStyle ?? AnyShapeStyle(.secondary))
                .monospacedDigit()
                .lineLimit(1)
                .frame(minWidth: paceWidth, alignment: .leading)
                .accessibilityHidden(true)
        }
    }
}

/// Where an even use of a limit would be by now, as a mark across its bar in the colour of
/// its pace. None at an even pace, where nothing is worth marking, and none where the pace
/// means nothing. It sits in a gap cut in the background's colour and reaches past the
/// bar's edges, so it reads on whatever tint the bar has under it: a limit over pace has its
/// mark inside the fill, which is red itself from 90%. It takes no clicks, so the bar's help
/// still shows under the pointer.
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
