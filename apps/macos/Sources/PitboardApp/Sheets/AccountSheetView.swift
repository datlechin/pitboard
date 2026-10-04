import Accessibility
import SwiftUI

/// The sheet `sheet` asks for.
struct AccountSheetView: View {
    let model: AppModel
    let sheet: AccountSheet

    var body: some View {
        switch sheet {
        // Claude Desktop signs in inside Claude, so it has a sheet of its own.
        case .add where model.provider(for: sheet) == desktopProvider:
            DesktopSignInSheet(model: model, again: nil)
        case .add(let provider):
            SignInSheet(model: model, provider: provider, again: nil)
        case .signInAgain(desktopProvider, let label):
            DesktopSignInSheet(model: model, again: label)
        case .signInAgain(let provider, let label):
            SignInSheet(model: model, provider: provider, again: label)
        case .name(let provider, let email):
            NameSheet(model: model, provider: provider, email: email)
        case .rename(let provider, let label):
            RenameSheet(model: model, provider: provider, label: label)
        case .liveUsage:
            LiveUsageSheet(model: model)
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
    let failure: ActionFailure

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
                Image(systemName: Notice.Severity.error.symbol)
                    .foregroundStyle(Notice.Severity.error.tint)
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

/// A name as it will be enrolled: what was typed, without the spaces around it.
func trimmed(_ typed: String) -> String {
    typed.trimmingCharacters(in: .whitespacesAndNewlines)
}
