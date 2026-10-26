import PitboardKit
import SwiftUI

/// Pitboard's log, newest first, whichever front end made each change.
struct ActivityPane: View {
    let model: AppModel

    var body: some View {
        let activity = model.machine.activity
        Table(activity.lines) {
            TableColumn("Date") { line in
                Text(line.date).monospacedDigit()
            }
            .width(min: 120, ideal: 150)
            TableColumn("Change") { line in
                Text(line.change)
                    .accessibilityIdentifier("activity.change")
            }
            .width(min: 70, ideal: 90)
            TableColumn("Account") { line in
                Text(line.account).lineLimit(1).truncationMode(.middle)
            }
            TableColumn("Result") { line in
                Text(line.result)
                    .foregroundStyle(line.done ? .secondary : .primary)
            }
            .width(min: 60, ideal: 90)
            TableColumn("Asked By") { line in
                Text(line.askedBy).foregroundStyle(.secondary)
            }
            .width(min: 80, ideal: 110)
        }
        .overlay {
            if let empty = activity.empty {
                ContentUnavailableView(
                    empty.title, systemImage: Symbol.activity,
                    description: Text(empty.detail)
                )
                .accessibilityIdentifier("activity.empty")
            }
        }
        .navigationTitle("Activity")
        .toolbar {
            Button {
                model.send(.paneShown(pane: .activity))
            } label: {
                Label("Refresh", systemImage: Symbol.refresh)
            }
            .help("Read the log again")
        }
        .focusedSceneValue(
            \.refresh,
            RefreshCommand(title: "Refresh", disabled: false) {
                model.send(.paneShown(pane: .activity))
            }
        )
        // Every visit: the log is read each time the pane is shown.
        .task { model.send(.paneShown(pane: .activity)) }
    }
}

/// A line of the log, numbered by the model: the log has no id of its own, and rows that
/// claim the same identity make a Table drop all but one of them.
extension ActivityLine: Identifiable {}
