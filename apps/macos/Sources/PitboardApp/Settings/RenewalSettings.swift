import PitboardKit
import SwiftUI

struct RenewalSettings: View {
    let model: AppModel

    var body: some View {
        let schedule = model.machine.schedule
        Form {
            Section {
                Toggle(
                    "Renew parked logins daily",
                    isOn: Binding(
                        get: { schedule.on }, set: { model.send(.setSchedule(on: $0)) })
                )
                .accessibilityIdentifier("settings.renewDaily")
                .disabled(!schedule.enabled)
                if let note = schedule.note {
                    Text(note).explanatory()
                }
                if let failed = schedule.failed {
                    Text(failed).explanatory()
                }
            } footer: {
                Text(schedule.about).footnote()
            }
        }
        .formStyle(.grouped)
        // A terminal can change the schedule while the app runs.
        .task { model.send(.readSchedule) }
    }
}
