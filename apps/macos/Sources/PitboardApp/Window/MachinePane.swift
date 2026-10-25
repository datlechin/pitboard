import PitboardKit
import SwiftUI

/// What Pitboard finds about this Mac, the checks `pitboard doctor` makes, for when
/// something is wrong at machine level and a failed switch's one line is not enough to act
/// on.
struct MachinePane: View {
    let model: AppModel

    var body: some View {
        let checks = model.machine.checks
        Form {
            Section {
                // By its place: doctor gives every account's parked login one code.
                ForEach(checks.lines, id: \.id) { line in
                    CheckRow(line: line)
                        .accessibilityIdentifier("check.\(line.id)")
                }
            } header: {
                summary(checks)
            }
        }
        .formStyle(.grouped)
        .overlay {
            if let waiting = checks.waiting {
                ProgressView(waiting)
            }
        }
        .navigationTitle("This Mac")
        .toolbar {
            Button {
                model.send(.paneShown(pane: .machine))
            } label: {
                Label("Check Again", systemImage: Symbol.refresh)
            }
            .help("Make every check again")
            .accessibilityIdentifier("toolbar.checkAgain")
            .disabled(checks.checking)
        }
        .focusedSceneValue(
            \.refresh,
            RefreshCommand(title: "Check Again", disabled: checks.checking) {
                model.send(.paneShown(pane: .machine))
            }
        )
        // Every visit: a menu bar app runs for days, and a check fixed in a terminal since
        // would otherwise still read as failing. What was found stays up meanwhile.
        .task { model.send(.paneShown(pane: .machine)) }
    }

    @ViewBuilder private func summary(_ checks: ChecksShown) -> some View {
        if let summary = checks.summary {
            HStack(alignment: .firstTextBaseline) {
                // The last line of `pitboard doctor`, so the two never disagree.
                Text(summary)
                    .accessibilityIdentifier("checks.summary")
                Spacer()
                if checks.checking {
                    ProgressView().controlSize(.small)
                } else if let checked = checks.checked {
                    Text(checked)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .fontWeight(.regular)
                }
            }
        }
    }
}

private struct CheckRow: View {
    let line: CheckLine

    var body: some View {
        LabeledContent {
            Text(line.detail)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.trailing)
                .textSelection(.enabled)
        } label: {
            HStack(alignment: .firstTextBaseline, spacing: Design.iconSpacing) {
                Image(systemName: line.level.symbol)
                    .foregroundStyle(line.level.tint)
                    .accessibilityLabel(line.spokenLevel)
                VStack(alignment: .leading, spacing: Design.lineSpacing) {
                    Text(line.name)
                    if let advice = line.advice {
                        Text(advice).explanatory()
                    }
                }
            }
        }
    }
}
