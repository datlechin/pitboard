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
        guard let advice = Advice.about(status, unless: told) else { return }
        told[advice.window.kind] = advice.window.resetsAt ?? 0
        centre.add(
            UNNotificationRequest(
                identifier: "\(advice.window.kind)-\(advice.window.resetsAt ?? 0)",
                content: advice.notification, trigger: nil))
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

/// An account has run out, and another has room.
struct Advice {
    /// The account that ran out, and the window it ran out of.
    let ran: String
    let window: Limits
    /// The account offered instead, and what it has left in the same kind of window.
    let use: String
    let left: Int

    /// Nothing to say unless the account in use has exhausted a window that has not been
    /// mentioned yet and an account that can be switched to now has room in the same kind
    /// of window. Weekly limits are per account, so the comparison is like for like.
    static func about(_ status: Status, unless told: [String: Int64]) -> Advice? {
        guard let current = status.accounts.first(where: \.signedIn), let ran = current.label
        else { return nil }
        for window in current.usage?.windows ?? [] where window.percent >= 100 {
            guard told[window.kind] != (window.resetsAt ?? 0) else { continue }
            let spare = status.accounts
                .filter { $0.switchable && $0.label != nil }
                .min { used($0, like: window) < used($1, like: window) }
            guard let spare, let use = spare.label, used(spare, like: window) < 100 else {
                continue
            }
            return Advice(
                ran: ran, window: window, use: use,
                left: 100 - Int(used(spare, like: window).rounded()))
        }
        return nil
    }

    /// What an account has used of the same window, counting a window it does not report
    /// as spent: an account whose limits are unknown is not one to recommend.
    private static func used(_ account: Account, like window: Limits) -> Double {
        account.usage?.windows.first { $0.kind == window.kind && $0.scope == window.scope }?
            .percent ?? 100
    }

    var notification: UNNotificationContent {
        let content = UNMutableNotificationContent()
        let limit = window.kind == "session" ? "5-hour" : "weekly"
        content.title = "\(ran) has no \(limit) limit left"
        content.body = "\(use) has \(left)% of its own left."
        content.categoryIdentifier = "limit"
        content.userInfo = ["label": use]
        return content
    }
}
