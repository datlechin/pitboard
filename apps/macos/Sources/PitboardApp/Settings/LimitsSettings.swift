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
                // Read as the sentence it is: when a limit reaches the share, do what is chosen.
                LabeledContent(atLimit.heading) {
                    HStack {
                        // The HIG pairs a stepper with a field where large changes are likely.
                        TextField(atLimit.shareLabel, value: share, format: .percent)
                            .labelsHidden()
                            .textFieldStyle(.roundedBorder)
                            .multilineTextAlignment(.trailing)
                            .frame(width: shareWidth)
                            .disabled(!atLimit.shareEnabled)
                            .accessibilityIdentifier("settings.atLimitShareField")
                        Stepper(
                            atLimit.shareLabel, value: share,
                            in: Int(atLimit.lowest)...Int(atLimit.highest)
                        )
                        .labelsHidden()
                        .disabled(!atLimit.shareEnabled)
                        .accessibilityIdentifier("settings.atLimitShare")
                        Picker(
                            atLimit.atLabel,
                            selection: Binding(
                                get: { atLimit.choice },
                                set: { model.send(.setAtLimit(choice: $0, at: atLimit.at)) })
                        ) {
                            ForEach(atLimit.choices, id: \.choice) { choice in
                                Text(choice.title).tag(choice.choice)
                            }
                        }
                        .labelsHidden()
                        .fixedSize()
                        .accessibilityIdentifier("settings.atLimit")
                    }
                }
                .disabled(!atLimit.enabled)
                if let standing = atLimit.standing {
                    Text(standing)
                        .explanatory()
                        .accessibilityIdentifier("settings.autoSwitchStanding")
                }
            } header: {
                Text(atLimit.title)
            } footer: {
                VStack(alignment: .leading, spacing: 4) {
                    if let note = atLimit.note { Text(note).footnote() }
                    if let terms = atLimit.terms { TermsText(terms: terms) }
                }
            }

            Section {
                DatePicker(
                    "From",
                    selection: timeOfDay(warm.from) {
                        model.send(.setWarmRange(from: $0, to: warm.to))
                    },
                    displayedComponents: .hourAndMinute
                )
                .accessibilityIdentifier("settings.warmFrom")
                DatePicker(
                    "To",
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
                        Text(account.plan.map { "\(account.title) · \($0)" } ?? account.title)
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
                VStack(alignment: .leading, spacing: 4) {
                    Text(warm.note).footnote()
                    TermsText(terms: warm.terms)
                }
            }
            .disabled(!warm.enabled)
        }
        .formStyle(.grouped)
    }
}

// The sentence, then the page that says whether the terms allow it, as one text that wraps.
private struct TermsText: View {
    let terms: TermsShown

    var body: some View {
        var link = AttributedString(terms.linkTitle)
        link.link = URL(string: terms.link)
        return Text(AttributedString("\(terms.sentence) ") + link).footnote()
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
