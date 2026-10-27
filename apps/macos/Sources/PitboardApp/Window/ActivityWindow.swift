import PitboardKit
import SwiftUI

// Pitboard's log, newest first, whichever front end made each change.
struct ActivityWindow: View {
    let model: AppModel
    let presence: AppPresence

    var body: some View {
        let activity = model.machine.activity
        let columns = words.activity
        Table(activity.lines) {
            TableColumn(columns.date) { line in
                Text(line.date).monospacedDigit()
            }
            .width(min: 120, ideal: 160)
            // Ideal widths that show "Sign-in in use changed" and "Signed in outside" whole at
            // the window's default size.
            TableColumn(columns.change) { line in
                Text(line.change)
                    .accessibilityIdentifier("activity.change")
            }
            .width(min: 70, ideal: 150)
            TableColumn(columns.account) { line in
                Text(line.account).lineLimit(1).truncationMode(.middle)
            }
            .width(min: 80, ideal: 120)
            TableColumn(columns.result) { line in
                Text(line.result)
                    .foregroundStyle(line.done ? .secondary : .primary)
            }
            .width(min: 60, ideal: 120)
            TableColumn(columns.by) { line in
                Text(line.askedBy).foregroundStyle(.secondary)
            }
            .width(min: 80, ideal: 120)
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
        .navigationTitle(words.window.activity)
        .toolbar {
            Button {
                model.send(.paneShown(pane: .activity))
            } label: {
                Label(words.window.refresh, systemImage: Symbol.refresh)
            }
            .help(columns.refreshHelp)
            .accessibilityIdentifier("toolbar.refreshActivity")
        }
        .focusedSceneValue(
            \.refresh,
            RefreshCommand(title: words.window.refresh, disabled: false) {
                model.send(.paneShown(pane: .activity))
            }
        )
        // The log is read each time it opens or is asked for.
        .task { model.send(.paneShown(pane: .activity)) }
        .onChange(of: model.windowRequest) { _, request in
            if PlaceWindow(request.place) == .activity {
                model.send(.paneShown(pane: .activity))
            }
        }
        .frame(minWidth: 520, minHeight: 240)
        .appWindow(presence)
    }
}

// By the model's number: a Table drops all but one of rows that share an identity.
extension ActivityLine: Identifiable {}
