import AppKit
import PitboardKit
import SwiftUI

// What `pitboard doctor` checks, for when a failed switch's one line is not enough to act on.
struct DiagnosticsWindow: View {
    let model: AppModel
    let presence: AppPresence
    @State private var asking: Asking?

    var body: some View {
        let checks = model.machine.checks
        Form {
            if let summary = checks.summary {
                Section {
                    Summary(checks: checks, summary: summary)
                }
            }
            ForEach(checks.groups, id: \.id) { group in
                Section(group.heading) {
                    // By its place: doctor gives every account's saved sign-in one code.
                    ForEach(group.lines, id: \.id) { line in
                        CheckRow(line: line) { offer in
                            $asking.ask(offer.intent, first: offer.confirm, of: model)
                        }
                        .accessibilityIdentifier("check.\(line.id)")
                    }
                }
            }
        }
        .formStyle(.grouped)
        .overlay {
            if let waiting = checks.waiting {
                ProgressView(waiting)
            }
        }
        .navigationTitle(words.window.diagnostics)
        .navigationSubtitle(checks.checked ?? "")
        .toolbar {
            ToolbarItemGroup {
                Button {
                    checks.report.map(copy(report:))
                } label: {
                    Label(words.window.copyReport, systemImage: "doc.on.doc")
                }
                .help(words.window.copyReportHelp)
                .disabled(checks.report == nil)
                .accessibilityIdentifier("toolbar.copyReport")
                Button {
                    model.send(.paneShown(pane: .checks))
                } label: {
                    Label(words.window.checkAgain, systemImage: Symbol.refresh)
                }
                .help(words.window.checkAgainHelp)
                .disabled(checks.checking)
                .accessibilityIdentifier("toolbar.checkAgain")
            }
        }
        .focusedSceneValue(
            \.refresh,
            RefreshCommand(title: words.window.checkAgain, disabled: checks.checking) {
                model.send(.paneShown(pane: .checks))
            }
        )
        .focusedSceneValue(\.report, checks.report)
        // On each open or request: a check fixed in a terminal since would still read failed.
        .task { model.send(.paneShown(pane: .checks)) }
        .onChange(of: model.windowRequest) { _, request in
            if PlaceWindow(request.place) == .diagnostics {
                model.send(.paneShown(pane: .checks))
            }
        }
        .asking($asking, of: model)
        .frame(minWidth: 420, minHeight: 320)
        .appWindow(presence)
    }
}

extension FocusedValues {
    // Diagnostics' report, while its window is key and has one.
    @Entry var report: String?
}

// The toolbar's Copy Report, in the menu bar as well.
struct CopyReportCommands: Commands {
    @FocusedValue(\.report) private var report

    var body: some Commands {
        CommandGroup(after: .pasteboard) {
            Button(words.window.copyReport) { report.map(copy(report:)) }
                .disabled(report == nil)
        }
    }
}

@MainActor
private func copy(report: String) {
    NSPasteboard.general.clearContents()
    NSPasteboard.general.setString(report, forType: .string)
}

private struct Summary: View {
    let checks: ChecksShown
    let summary: String

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: Design.iconSpacing) {
            Image(systemName: checks.level.symbol)
                .foregroundStyle(checks.level.tint)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: Design.lineSpacing) {
                Text(summary).fontWeight(.medium)
                if let verdict = checks.verdict {
                    Text(verdict).explanatory()
                }
            }
            Spacer(minLength: 0)
            if checks.checking {
                ProgressView().controlSize(.small)
                    .accessibilityLabel(words.window.checking ?? "")
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("checks.summary")
    }
}

private struct CheckRow: View {
    let line: CheckLine
    let choose: (ItemOffer) -> Void

    var body: some View {
        let acts = line.action != nil || line.learn != nil
        LabeledContent {
            if let action = line.action {
                Button(action.title) { choose(action) }
                    .disabled(!action.enabled)
                    .accessibilityIdentifier("check.action.\(caseName(action.intent))")
            } else if let learn = line.learn, let link = URL(string: learn.link) {
                Link(learn.title, destination: link)
            } else {
                Text(line.detail)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.trailing)
                    .textSelection(.enabled)
            }
        } label: {
            HStack(alignment: .firstTextBaseline, spacing: Design.iconSpacing) {
                Image(systemName: line.level.symbol)
                    .foregroundStyle(line.level.tint)
                    .accessibilityLabel(line.spokenLevel)
                VStack(alignment: .leading, spacing: Design.lineSpacing) {
                    Text(line.name)
                    // Beside a button, what it found goes under its name.
                    if acts {
                        Text(line.detail).explanatory().textSelection(.enabled)
                    }
                    if let advice = line.advice {
                        Text(advice).explanatory()
                    }
                }
            }
        }
    }
}
