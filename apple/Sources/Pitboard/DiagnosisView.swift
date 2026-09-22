import PitboardKit
import SwiftUI

struct DiagnosisPanel: View {
    let model: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(model.checks, id: \.code) { check in
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Image(systemName: mark(check.level))
                        .foregroundStyle(colour(check.level))
                        .accessibilityHidden(true)
                    Text(check.name).font(.caption).frame(width: 110, alignment: .leading)
                    Text(check.detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                        .truncationMode(.middle)
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("\(check.name): \(check.detail)")
                if check.level != .ok, !check.advice.isEmpty {
                    Text(check.advice)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }

    private func mark(_ level: Level) -> String {
        switch level {
        case .ok: "checkmark.circle"
        case .warn: "exclamationmark.triangle"
        case .fail: "xmark.octagon"
        }
    }

    private func colour(_ level: Level) -> Color {
        switch level {
        case .ok: .green
        case .warn: .orange
        case .fail: .red
        }
    }
}
