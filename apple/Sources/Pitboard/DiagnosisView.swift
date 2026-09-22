import PitboardKit
import SwiftUI

struct DiagnosisPanel: View {
    let model: AppModel
    /// Fixed at the body size, these run into each other the moment somebody has text set
    /// larger than the default, which is most of the people this matters to.
    @ScaledMetric(relativeTo: .caption) private var nameWidth: CGFloat = 110

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(model.checks, id: \.code) { check in
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Image(systemName: check.level.symbol)
                        .foregroundStyle(check.level.tint)
                        .accessibilityHidden(true)
                    Text(check.name).font(.caption).frame(width: nameWidth, alignment: .leading)
                    Text(check.detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                        .truncationMode(.middle)
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("\(check.name), \(check.level.spoken): \(check.detail)")
                if check.level != .ok, !check.advice.isEmpty {
                    Text(check.advice)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }
}
