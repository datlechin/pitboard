import AppKit
import PitboardKit
// Its types are not marked Sendable, though Notification Center is meant to be asked from
// any thread, as the model asks it from a thread of its own.
@preconcurrency import UserNotifications

// What the Rust model asks of macOS, through the traits it exports: other apps, Notification
// Center and the person's clock. Each is called on a thread of the model's own, never the
// main thread, and answers at once.

// MARK: - Other apps

/// A protocol so a test can stand in for an app: `forceTerminate` is here only so a test can
/// see that nothing calls it.
protocol RunningApp: AnyObject {
    var bundleURL: URL? { get }
    var isTerminated: Bool { get }
    @discardableResult func terminate() -> Bool
    @discardableResult func forceTerminate() -> Bool
}

extension NSRunningApplication: RunningApp {}

/// An app is asked to quit the way Command-Q asks it, which lets it ask about work in
/// progress, and never forced: a forced quit loses whatever it had not saved. How long it is
/// given, and what happens when it does not quit, is the model's.
final class MacAppControl: AppControl, @unchecked Sendable {
    // Unchecked because both closures are only called, never changed, and what they reach,
    // NSRunningApplication's list and NSWorkspace, are safe from any thread.
    private let copies: (String) -> [any RunningApp]
    private let open: (URL, Bool) -> Void

    init(copies: @escaping (String) -> [any RunningApp], open: @escaping (URL, Bool) -> Void) {
        self.copies = copies
        self.open = open
    }

    /// NSRunningApplication says it may be used from any thread; an app is opened from the
    /// main one.
    static func live() -> MacAppControl {
        MacAppControl(
            copies: { NSRunningApplication.runningApplications(withBundleIdentifier: $0) },
            open: { url, front in
                DispatchQueue.main.async {
                    NSWorkspace.shared.openApplication(
                        at: url, configuration: opening(front: front))
                }
            })
    }

    // In front for a sign-in inside the app; otherwise behind what Pitboard has to say.
    static func opening(front: Bool) -> NSWorkspace.OpenConfiguration {
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.activates = front
        return configuration
    }

    private func running(_ app: String) -> [any RunningApp] {
        copies(app).filter { !$0.isTerminated }
    }

    /// An app is a bundle, so a running copy always says where it is.
    func running(app: String) throws -> String? {
        running(app).lazy.compactMap(\.bundleURL).first?.path
    }

    func requestQuit(app: String) throws {
        for copy in running(app) { copy.terminate() }
    }

    func reopen(location: String, front: Bool) throws {
        open(URL(fileURLWithPath: location), front)
    }
}

// MARK: - Notifications

// A click on a notice, or on its button, hands the model back the text it gave.
final class MacNotifications: NSObject, Notifications, UNUserNotificationCenterDelegate,
    @unchecked Sendable
{
    // Unchecked because `centre` is set once, and `onClick` is the main actor's.
    nonisolated static let action = "button"
    nonisolated static let clicked = "clicked"

    // Nil outside an app bundle, where Notification Center stops the process.
    private let centre: UNUserNotificationCenter?
    @MainActor var onClick: ((String) -> Void)?

    // At launch: a notification clicked later, after an update relaunches the app too, is
    // delivered as soon as there is a delegate, and shows its button by a category registered
    // already.
    init(delivering: Bool) {
        centre = delivering ? .current() : nil
        super.init()
        guard let centre else { return }
        centre.delegate = self
        centre.setNotificationCategories(Self.categories(notificationButtons()))
    }

    // The model opens a window where a click needs one, so no button brings Pitboard forward.
    static func categories(_ titles: [String]) -> Set<UNNotificationCategory> {
        Set(
            titles.map { title in
                UNNotificationCategory(
                    identifier: title,
                    actions: [
                        UNNotificationAction(identifier: action, title: title, options: [])
                    ],
                    intentIdentifiers: [])
            })
    }

    static func content(of notice: RunOutNotice) -> UNNotificationContent {
        let content = UNMutableNotificationContent()
        content.title = notice.title
        if let subtitle = notice.subtitle { content.subtitle = subtitle }
        content.body = notice.body
        content.userInfo = [clicked: notice.clicked]
        if let button = notice.button {
            content.categoryIdentifier = button.title
            content.userInfo[action] = button.clicked
        }
        return content
    }

    static func argument(of response: String, in userInfo: [AnyHashable: Any]) -> String? {
        switch response {
        case action: userInfo[action] as? String
        case UNNotificationDefaultActionIdentifier: userInfo[clicked] as? String
        default: nil
        }
    }

    // Permission is asked at the first notice, not at launch; a refusal is no error.
    func post(notice: RunOutNotice) throws {
        guard let centre else { return }
        let request = UNNotificationRequest(
            identifier: notice.id, content: Self.content(of: notice), trigger: nil)
        centre.requestAuthorization(options: [.alert]) { granted, _ in
            if granted { centre.add(request) }
        }
    }

    func userNotificationCenter(
        _ centre: UNUserNotificationCenter, didReceive response: UNNotificationResponse
    ) async {
        guard
            let argument = Self.argument(
                of: response.actionIdentifier,
                in: response.notification.request.content.userInfo)
        else { return }
        await MainActor.run { onClick?(argument) }
    }
}

// MARK: - The person's clock

/// Clock times and dates as the person reads them: their locale, their calendar and time
/// zone, and whether they read 12 or 24 hours, which macOS keeps in the locale.
final class MacLocalTime: LocalTime {
    private let locale: Locale
    private let calendar: Calendar

    init(
        locale: Locale = .autoupdatingCurrent, calendar: Calendar = .autoupdatingCurrent,
        timeZone: TimeZone = .autoupdatingCurrent
    ) {
        self.locale = locale
        var calendar = calendar
        calendar.timeZone = timeZone
        self.calendar = calendar
    }

    private func style(date: Date.FormatStyle.DateStyle, time: Date.FormatStyle.TimeStyle)
        -> Date.FormatStyle
    {
        Date.FormatStyle(
            date: date, time: time, locale: locale, calendar: calendar,
            timeZone: calendar.timeZone)
    }

    /// "14:05" or "2:05 PM", and "Wed 14:05" with its weekday.
    func clock(epoch: Int64, withWeekday: Bool) throws -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(epoch))
        let time = date.formatted(style(date: .omitted, time: .shortened))
        guard withWeekday else { return time }
        let day = date.formatted(
            Date.FormatStyle(locale: locale, calendar: calendar, timeZone: calendar.timeZone)
                .weekday(.abbreviated))
        return "\(day) \(time)"
    }

    func sameDay(first: Int64, second: Int64) throws -> Bool {
        calendar.isDate(
            Date(timeIntervalSince1970: TimeInterval(first)),
            inSameDayAs: Date(timeIntervalSince1970: TimeInterval(second)))
    }

    func dateAndTime(epoch: Int64) throws -> String {
        Date(timeIntervalSince1970: TimeInterval(epoch))
            .formatted(style(date: .abbreviated, time: .shortened))
    }

    // On a day with no clock change, so every time of day is one.
    func timeOfDay(minutes: UInt16) throws -> String {
        let day = DateComponents(
            year: 2001, month: 1, day: 1, hour: Int(minutes / 60), minute: Int(minutes % 60))
        guard let date = calendar.date(from: day) else {
            throw PlatformError.Failed(reason: "no time \(minutes) minutes into a day")
        }
        return date.formatted(style(date: .omitted, time: .shortened))
    }
}
