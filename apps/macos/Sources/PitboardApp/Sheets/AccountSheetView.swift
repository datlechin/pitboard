import Accessibility
import PitboardKit
import SwiftUI

struct AccountSheetView: View {
    let model: AppModel
    let sheet: Sheet

    var body: some View {
        Group {
            switch sheet {
            case .add, .signInAgain:
                SignInSheet(model: model, sheet: sheet)
            case .name, .rename:
                NameSheet(model: model, sheet: sheet)
            case .stow:
                StowSheet(model: model)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("sheet.\(caseName(sheet))")
        // Only while the model keeps this sheet: as it closes, the window's take over.
        .windowAlerts(model, shown: model.sheet?.id == sheet.id)
    }
}

struct SheetLayout<Content: View, Status: View, Buttons: View>: View {
    let title: String
    let message: String
    @ViewBuilder let content: Content
    /// At the leading end of the buttons, where a Mac sheet shows its spinner while it waits.
    @ViewBuilder let status: Status
    @ViewBuilder let buttons: Buttons

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: Design.rowSpacing) {
                Text(title)
                    .font(.headline)
                    .accessibilityAddTraits(.isHeader)
                    .accessibilityIdentifier("sheet.title")
                Text(message).explanatory()
                    .accessibilityIdentifier("sheet.message")
            }
            .padding([.horizontal, .top], 20)
            Form { content }
                .formStyle(.grouped)
                .scrollDisabled(true)
                .fixedSize(horizontal: false, vertical: true)
            HStack {
                status
                Spacer()
                buttons
            }
            .padding([.horizontal, .bottom], 20)
        }
        .frame(width: 440)
    }
}

extension SheetLayout where Status == EmptyView {
    init(
        title: String,
        message: String,
        @ViewBuilder content: () -> Content,
        @ViewBuilder buttons: () -> Buttons
    ) {
        self.init(
            title: title, message: message, content: content, status: { EmptyView() },
            buttons: buttons)
    }
}

/// A tool Pitboard did not find, where it is picked: what to install, and a link to how.
struct ToolMissing: View {
    let missing: Missing

    var body: some View {
        Section {
            Label {
                VStack(alignment: .leading, spacing: Design.lineSpacing) {
                    Text(missing.title).fontWeight(.medium)
                    Text(missing.detail).explanatory()
                    if let page = URL(string: missing.link) {
                        Link(missing.linkTitle, destination: page)
                            .accessibilityIdentifier("sheet.missing.link")
                    }
                }
            } icon: {
                Image(systemName: Severity.warning.symbol)
                    .foregroundStyle(Severity.warning.tint)
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("sheet.missing")
        }
        // Picking a tool Pitboard did not find changes the sheet below the field in focus.
        .onAppear(perform: announce)
        .onChange(of: missing.title, announce)
    }

    private func announce() {
        AccessibilityNotification.Announcement("\(missing.title) \(missing.detail)").post()
    }
}

/// Why what is typed in a name field cannot be a name, under the field.
struct NameProblem: View {
    let problem: String

    var body: some View {
        Text(problem)
            .footnote()
            .accessibilityIdentifier("sheet.problem")
            .onChange(of: problem, initial: true) {
                AccessibilityNotification.Announcement(problem).post()
            }
    }
}

/// A failure said inside the sheet that met it, where the name typed is still there to
/// correct.
struct SheetFailure: View {
    let failure: FailureText

    var body: some View {
        Section {
            Label {
                VStack(alignment: .leading, spacing: Design.lineSpacing) {
                    Text(failure.title).fontWeight(.medium)
                    Text(failure.message).explanatory().textSelection(.enabled)
                    ForEach(failure.warnings, id: \.self) { warning in
                        Text(warning).explanatory()
                    }
                }
            } icon: {
                Image(systemName: Severity.error.symbol)
                    .foregroundStyle(Severity.error.tint)
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("sheet.failure")
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
