import Foundation
import PitboardKit
import PitboardLinkTarget
import UserNotifications

/// Gathered in one value so a launch decides once which world the app runs in. A UI test
/// launches the debug build into a fixture, where the model is a fixture's, over the real
/// core on a machine of its own, and nothing reaches the keychain, the network, launchd, the
/// login items or an administrator prompt of whoever runs the tests.
@MainActor
struct Dependencies {
    let model: AppModel
    let defaults: UserDefaults
    let loginItem: any LoginItem
    let notificationStatus: @MainActor () async -> UNAuthorizationStatus?
    let commandLineTool: CommandLineTool
    let web: WebEnvironment
    /// What is done as the app quits, once the model has stopped.
    let quitting: @MainActor () -> Void

    static func live(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        bundle: URL = Bundle.main.bundleURL
    ) -> Dependencies {
        let defaults = UserDefaults.standard
        let records = WebEnvironment.recordsDirectory()
        let earlier = EarlierStore(
            defaults: defaults, environment: environment, windowsDirectory: records)
        let notifications = MacNotifications(delivering: bundle.pathExtension == "app")
        let launch = AppLaunch(
            environment: environment, appLocation: bundle.path,
            earlierPreferences: earlier.handOver(),
            windows: WindowsLaunch(
                directory: records.path,
                key: WebEnvironment.recordKey(environment: environment),
                linkScheme: LinkTarget.scheme(in: .main) ?? "pitboard",
                earlier: earlier.handOverWindows()))
        let model = AppModel.listening { listener in
            PitboardModel(
                launch: launch, listener: listener, apps: MacAppControl.live(),
                notifications: notifications, localTime: MacLocalTime())
        }
        notifications.onClick = { [weak model] argument in
            model?.send(.notificationClicked(argument: argument))
        }
        return Dependencies(
            model: model,
            defaults: defaults,
            loginItem: MainAppLoginItem(),
            notificationStatus: { await notifications.authorization() },
            commandLineTool: CommandLineTool(bundle: bundle),
            web: .live(),
            quitting: {
                earlier.forgetOnceKept()
                earlier.forgetWindowsOnceKept()
            })
    }

    static func forLaunch(
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> Dependencies {
        #if DEBUG
            if let name = environment[Fixture.variable] {
                return Fixture.dependencies(named: name, environment: environment)
            }
        #endif
        return live(environment: environment)
    }
}
