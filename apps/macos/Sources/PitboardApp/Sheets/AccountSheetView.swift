import Accessibility
import PitboardKit
import SwiftUI

/// The sheet `sheet` asks for, in the words the model says it in.
struct AccountSheetView: View {
    let model: AppModel
    let sheet: Sheet

    var body: some View {
        switch sheet {
        case .add, .signInAgain:
            SignInSheet(model: model, sheet: sheet)
        case .name(let provider, _):
            NameSheet(model: model, sheet: sheet) { .enrol(provider: provider, name: $0) }
        case .rename(let provider, let label):
            NameSheet(model: model, sheet: sheet) {
                .rename(provider: provider, label: label, to: $0)
            }
        case .stow:
            StowSheet(model: model)
        }
    }
}

/// How every sheet is laid out: a title and what the sheet is for at the top, a grouped
/// form, and its buttons at the bottom right, the default one last.
struct SheetLayout<Content: View, Buttons: View>: View {
    let title: String
    let message: String
    @ViewBuilder let content: Content
    @ViewBuilder let buttons: Buttons

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: Design.rowSpacing) {
                Text(title)
                    .font(.headline)
                    .accessibilityAddTraits(.isHeader)
                Text(message).explanatory()
            }
            .padding([.horizontal, .top], 20)
            Form { content }
                .formStyle(.grouped)
                .scrollDisabled(true)
                .fixedSize(horizontal: false, vertical: true)
            HStack {
                Spacer()
                buttons
            }
            .padding([.horizontal, .bottom], 20)
        }
        .frame(width: 440)
    }
}

/// A failure said inside the sheet that met it, where the name typed is still there to
/// correct.
struct SheetFailure: View {
    let failure: Failure

    var body: some View {
        Section {
            Label {
                VStack(alignment: .leading, spacing: Design.lineSpacing) {
                    Text(failure.title).fontWeight(.medium)
                    Text(failure.message).explanatory().textSelection(.enabled)
                    ForEach(failure.warnings, id: \.self) { warning in
                        Text(warning.message).explanatory()
                    }
                }
            } icon: {
                Image(systemName: Severity.error.symbol)
                    .foregroundStyle(Severity.error.tint)
            }
        }
        // It appears where nobody's focus is, after the button that was pressed, and
        // VoiceOver does not read what appears by itself.
        .onAppear(perform: announce)
        .onChange(of: failure.id, announce)
    }

    private func announce() {
        AccessibilityNotification.Announcement("\(failure.title). \(failure.message)").post()
    }
}
