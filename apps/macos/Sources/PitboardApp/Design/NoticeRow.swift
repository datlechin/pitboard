import PitboardKit
import SwiftUI

struct NoticeRow: View {
    let notice: PanelNotice
    let perform: (NoticeAction) -> Void

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: Design.iconSpacing) {
            Image(systemName: notice.severity.symbol)
                .foregroundStyle(notice.severity.tint)
                .accessibilityLabel(notice.spokenSeverity)
            VStack(alignment: .leading, spacing: Design.rowSpacing) {
                Text(notice.title)
                    .fontWeight(.medium)
                    .fixedSize(horizontal: false, vertical: true)
                ForEach(Array(notice.lines.enumerated()), id: \.offset) { _, line in
                    Text(line).explanatory().textSelection(.enabled)
                }
                if let until = notice.until, let label = notice.untilLabel {
                    // Counts down by itself, and stops at zero once the moment has passed.
                    let follows = Date(timeIntervalSince1970: TimeInterval(until))
                    let timer = Text(
                        timerInterval: Date()...max(follows, Date()), countsDown: true)
                    Text("\(label) \(timer)")
                        .explanatory()
                        .monospacedDigit()
                }
                let buttons = notice.actions.filter { !$0.dismisses }
                if !buttons.isEmpty {
                    HStack {
                        ForEach(buttons, id: \.title) { action in
                            Button(action.title) { perform(action) }
                                .disabled(!action.enabled)
                        }
                    }
                }
            }
            Spacer(minLength: 0)
            if let dismiss = notice.actions.first(where: \.dismisses) {
                Button(dismiss.title, systemImage: "xmark") { perform(dismiss) }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .foregroundStyle(.secondary)
                    .help(dismiss.title)
            }
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("notice.\(notice.id)")
    }
}
