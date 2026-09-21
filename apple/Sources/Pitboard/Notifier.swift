import PitboardKit
import UserNotifications

/// Tells you once when the account in use runs out, and offers the account with the most
/// left. Switching is the button's job, never the notification's: pitboard does not switch
/// accounts on its own.
@MainActor
final class Notifier: NSObject, UNUserNotificationCenterDelegate {
    /// Called when the notification's button is pressed, with the label to switch to.
    var onSwitch: ((String) -> Void)?

    private let centre = UNUserNotificationCenter.current()
    private nonisolated static let category = "limit"
    private nonisolated static let action = "switch"
    /// The reset time of the window each kind was last reported for, so one exhausted
    /// window is mentioned once rather than every few minutes until it resets.
    private var told: [String: Int64] = [:]

    func start() {
        centre.delegate = self
        centre.setNotificationCategories([
            UNNotificationCategory(
                identifier: Self.category,
                actions: [
                    UNNotificationAction(
                        identifier: Self.action, title: "Switch", options: [.foreground])
                ],
                intentIdentifiers: [])
        ])
        centre.requestAuthorization(options: [.alert]) { _, _ in }
    }

    func consider(_ status: Status) {
        guard let current = status.accounts.first(where: \.signedIn), let label = current.label
        else { return }
        for window in current.usage?.windows ?? [] where window.percent >= 100 {
            guard told[window.kind] != (window.resetsAt ?? 0) else { continue }
            guard let spare = mostLeft(in: status, like: window) else { continue }
            told[window.kind] = window.resetsAt ?? 0
            tell(label, ran: window, switchTo: spare)
        }
    }

    /// The account with the most room left in the same kind of window, among those that
    /// can be switched to now.
    private func mostLeft(in status: Status, like window: Limits) -> Account? {
        status.accounts
            .filter { $0.switchable && $0.label != nil }
            .min { left($0, like: window) < left($1, like: window) }
            .flatMap { left($0, like: window) < 100 ? $0 : nil }
    }

    private func left(_ account: Account, like window: Limits) -> Double {
        account.usage?.windows.first { $0.kind == window.kind && $0.scope == window.scope }?
            .percent ?? 100
    }

    private func tell(_ label: String, ran window: Limits, switchTo spare: Account) {
        guard let to = spare.label else { return }
        let content = UNMutableNotificationContent()
        content.title =
            "\(label) has no \(window.kind == "session" ? "5-hour" : "weekly") limit left"
        content.body =
            "\(to) has \(100 - Int(left(spare, like: window).rounded()))% of its own left."
        content.categoryIdentifier = Self.category
        content.userInfo = ["label": to]
        centre.add(
            UNNotificationRequest(
                identifier: "\(window.kind)-\(window.resetsAt ?? 0)", content: content,
                trigger: nil))
    }

    nonisolated func userNotificationCenter(
        _ centre: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse
    ) async {
        guard response.actionIdentifier == Self.action,
            let label = response.notification.request.content.userInfo["label"] as? String
        else { return }
        await MainActor.run { onSwitch?(label) }
    }
}
