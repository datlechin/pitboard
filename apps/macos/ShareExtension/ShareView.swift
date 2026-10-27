import PitboardShareBindings
import SwiftUI

let words = shareWords()

@MainActor
@Observable
final class ShareState {
    enum Phase: Equatable {
        case opening
        case refused(String)
        case failed(String)
    }

    var phase = Phase.opening
}

struct ShareView: View {
    let state: ShareState
    let done: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            switch state.phase {
            case .opening:
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small)
                    Text(words.opening)
                }
            case .refused(let reason):
                said(words.refused, reason)
            case .failed(let reason):
                said(words.failed, reason)
            }
            if state.phase != .opening {
                HStack {
                    Spacer()
                    Button(words.ok, action: done)
                        .keyboardShortcut(.defaultAction)
                }
            }
        }
        .padding(20)
        .frame(width: 360, alignment: .leading)
    }

    private func said(_ title: String, _ reason: String) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
            Text(reason)
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}
