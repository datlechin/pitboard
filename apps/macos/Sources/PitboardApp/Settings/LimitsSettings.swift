import PitboardKit
import SwiftUI

struct LimitsSettings: View {
    let model: AppModel
    @ScaledMetric(relativeTo: .body) private var shareWidth: CGFloat = 56

    nonisolated static func share(_ typed: Int, of atLimit: AtLimitShown) -> UInt8 {
        UInt8(min(max(typed, Int(atLimit.lowest)), Int(atLimit.highest)))
    }

    var body: some View {
        let atLimit = model.machine.atLimit
        let warm = model.machine.warm
        let share = Binding(
            get: { Int(atLimit.at) },
            set: {
                model.send(.setAtLimit(choice: atLimit.choice, at: Self.share($0, of: atLimit)))
            })
        Form {
            Section {
                Picker(
                    atLimit.heading,
                    selection: Binding(
                        get: { atLimit.choice },
                        set: { model.send(.setAtLimit(choice: $0, at: atLimit.at)) })
                ) {
                    ForEach(atLimit.choices, id: \.choice) { choice in
                        Text(choice.title).tag(choice.choice)
                    }
                }
                .accessibilityIdentifier("settings.atLimit")
                if atLimit.choice != .nothing {
                    LabeledContent(atLimit.shareLabel) {
                        HStack {
                            // The HIG pairs a stepper with a field where large changes are likely.
                            ShareField(atLimit: atLimit, width: shareWidth) {
                                model.send(.setAtLimit(choice: atLimit.choice, at: $0))
                            }
                            Stepper(
                                atLimit.shareLabel, value: share,
                                in: Int(atLimit.lowest)...Int(atLimit.highest)
                            )
                            .labelsHidden()
                            .accessibilityIdentifier("settings.atLimitShare")
                        }
                    }
                    .disabled(!atLimit.shareEnabled)
                }
                if let standing = atLimit.standing {
                    Text(standing)
                        .explanatory()
                        .accessibilityIdentifier("settings.autoSwitchStanding")
                }
                if let terms = atLimit.terms, let page = URL(string: terms.link) {
                    Link(terms.linkTitle, destination: page)
                        .accessibilityIdentifier("settings.terms")
                }
            } footer: {
                if let note = atLimit.note { Text(note).footnote() }
            }
            .disabled(!atLimit.enabled)

            Section {
                DatePicker(
                    words.settings.from,
                    selection: timeOfDay(warm.from) {
                        model.send(.setWarmRange(from: $0, to: warm.to))
                    },
                    displayedComponents: .hourAndMinute
                )
                .accessibilityIdentifier("settings.warmFrom")
                DatePicker(
                    words.settings.to,
                    selection: timeOfDay(warm.to) {
                        model.send(.setWarmRange(from: warm.from, to: $0))
                    },
                    displayedComponents: .hourAndMinute
                )
                .accessibilityIdentifier("settings.warmTo")
                if let none = warm.noneEnrolled {
                    Text(none).explanatory()
                }
                ForEach(warm.accounts, id: \.qualified) { account in
                    Toggle(
                        isOn: Binding(
                            get: { account.on },
                            set: { model.send(.setWarm(qualified: account.qualified, on: $0)) })
                    ) {
                        Text(account.title)
                        if let standing = account.standing {
                            Text(standing)
                        }
                    }
                    .accessibilityIdentifier("settings.warm.\(account.qualified)")
                }
                if let standing = warm.standing {
                    Text(standing)
                        .explanatory()
                        .accessibilityIdentifier("settings.warmStanding")
                }
            } header: {
                Text(warm.title)
            } footer: {
                Text(warm.note).footnote()
            }
            .disabled(!warm.enabled)
        }
        .formStyle(.grouped)
    }
}

// Sent on Return or as focus leaves: a share clamped at each key left "120" in the field over 99.
private struct ShareField: View {
    let atLimit: AtLimitShown
    let width: CGFloat
    let send: (UInt8) -> Void
    @State private var typed = 0
    @FocusState private var editing: Bool

    var body: some View {
        TextField(atLimit.shareLabel, value: $typed, format: .percent)
            .labelsHidden()
            .textFieldStyle(.roundedBorder)
            .multilineTextAlignment(.trailing)
            .frame(width: width)
            .focused($editing)
            .onSubmit(take)
            .onChange(of: editing) {
                if !editing { take() }
            }
            .onChange(of: atLimit.at, initial: true) { typed = Int(atLimit.at) }
            .accessibilityIdentifier("settings.atLimitShareField")
    }

    private func take() {
        let share = LimitsSettings.share(typed, of: atLimit)
        typed = Int(share)
        if share != atLimit.at { send(share) }
    }
}

@MainActor
private func timeOfDay(
    _ minutes: UInt16, set: @escaping @MainActor @Sendable (UInt16) -> Void
) -> Binding<Date> {
    Binding(
        get: {
            // 24:00, the end of a day, shows as midnight.
            let minutes = Int(minutes) % (24 * 60)
            return Calendar.current.date(
                bySettingHour: minutes / 60, minute: minutes % 60, second: 0, of: .now) ?? .now
        },
        set: { date in
            let parts = Calendar.current.dateComponents([.hour, .minute], from: date)
            let picked = (parts.hour ?? 0) * 60 + (parts.minute ?? 0)
            set(UInt16(picked))
        })
}
