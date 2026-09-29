import Foundation
import PitboardKit

/// Everything the app reaches outside itself through: the core, the defaults it keeps its
/// own preferences in, the login item, the command line link, Notification Center, the
/// claude.ai windows and the links that open them.
///
/// Gathered in one value so a launch decides once which world the app runs in. A UI test
/// launches the debug build into a fixture, where every one of these is a stand-in and
/// nothing reaches the keychain, the network, launchd, the login items or an administrator
/// prompt of whoever runs the tests.
@MainActor
public struct Dependencies {
    let core: any Core
    let defaults: UserDefaults
    let loginItem: any LoginItem
    let commandLineTool: CommandLineTool
    /// Whether notifications are posted. Off in a fixture, where asking for permission
    /// would put a system prompt in front of the test.
    let notifies: Bool
    /// Whether the app reads on its own: the periodic read, the wake notice, the poll that
    /// notices a change made elsewhere, and the one repair of an older app's schedule.
    let watching: Bool
    /// What the claude.ai windows load, keep their data in, and hand to the browser.
    let web: ClaudeWeb
    /// The scheme of the pitboard links this build answers: the one its bundle claims.
    let linkScheme: String
    /// Whether the app tells macOS about its Service when its version changes. Never in a
    /// fixture, which must not change what the Services menu of whoever runs it offers.
    let registersServices: Bool

    /// This machine, as the person running the app has it.
    public static func live() -> Dependencies {
        Dependencies(
            core: PitboardService(asking: { Settings.forCurrentUserAsked() }),
            defaults: .standard,
            loginItem: MainAppLoginItem(),
            commandLineTool: CommandLineTool(),
            notifies: true,
            watching: true,
            web: .live(defaults: .standard),
            linkScheme: claimedScheme(in: .main) ?? "pitboard",
            registersServices: true)
    }

    /// The pitboard link scheme `bundle` claims in its Info.plist: `pitboard` for a release
    /// and `pitboard-debug` for a debug build, which the project sets per configuration.
    static func claimedScheme(in bundle: Bundle) -> String? {
        let types = bundle.object(forInfoDictionaryKey: "CFBundleURLTypes") as? [[String: Any]]
        let schemes = types?.first?["CFBundleURLSchemes"] as? [String]
        return schemes?.first
    }

    /// The world this launch runs in: `live()`, unless this is a debug build started with
    /// `PITBOARD_FIXTURE` naming one of the fixtures UI tests use.
    public static func forLaunch(
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> Dependencies {
        #if DEBUG
            if let name = environment[Fixture.variable] {
                guard let fixture = Fixture(rawValue: name) else {
                    Launch.fail("no fixture is called \(name)")
                }
                return fixture.dependencies()
            }
        #endif
        return live()
    }
}
