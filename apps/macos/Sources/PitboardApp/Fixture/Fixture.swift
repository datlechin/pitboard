#if DEBUG
    import Foundation
    import PitboardKit

    // pitboard-ffi's worlds, the real core and model over a machine of their own. Only a debug
    // build reads PITBOARD_FIXTURE, so nothing outside can put a release in one.
    enum Fixture {
        static let variable = "PITBOARD_FIXTURE"
        // Set, macOS refuses the fixture's login item, as it can refuse an app's own.
        static let loginItemRefuses = "PITBOARD_FIXTURE_LOGIN_ITEM_REFUSES"

        // Emptied at every launch, so each test starts from the same place.
        static let suite = "com.usepitboard.Pitboard.fixture"

        // Where Rust's std::env::temp_dir looks: TMPDIR, which Foundation's does not read.
        static func folder(environment: [String: String]) -> URL {
            let temporary =
                environment["TMPDIR"].map { URL(fileURLWithPath: $0, isDirectory: true) }
                ?? FileManager.default.temporaryDirectory
            return temporary.appendingPathComponent("pitboard-fixture", isDirectory: true)
        }

        // It posts no notification.
        @MainActor
        static func dependencies(named name: String, environment: [String: String])
            -> Dependencies
        {
            let model: AppModel
            do {
                model = try AppModel.listening { listener in
                    try PitboardModel.fixture(
                        name: name, listener: listener, localTime: MacLocalTime())
                }
            } catch let refused as FixtureError {
                switch refused {
                case .Unavailable(let reason), .Unknown(let reason), .Failed(let reason):
                    Launch.fail(reason)
                }
            } catch {
                Launch.fail("the fixture \(name) could not be made: \(error)")
            }
            // Only once the model has made its world: making it empties the folder first.
            let folder = folder(environment: environment)
            let defaults = UserDefaults(suiteName: suite) ?? .standard
            defaults.removePersistentDomain(forName: suite)
            let loginItem = FixtureLoginItem(refuses: environment[loginItemRefuses] != nil)
            let tool = commandLineTool(in: folder)
            return Dependencies(
                model: model,
                defaults: defaults,
                loginItem: loginItem,
                notificationStatus: { nil },
                commandLineTool: tool,
                removal: removal(
                    in: folder, loginItem: loginItem, defaults: defaults, commandLine: tool),
                web: .fixture(folder: folder),
                quitting: {})
        }

        // The app's own shell commands, in the fixture's folder, with no password.
        static func commandLineTool(
            in folder: URL,
            execute: @escaping CommandLineTool.Runner = CommandLineTool.execute(script:)
        ) -> CommandLineTool {
            let helper = folder.appendingPathComponent("Pitboard.app/Contents/Helpers/pitboard")
            let link = folder.appendingPathComponent("bin/pitboard")
            return CommandLineTool(
                helper: helper.path, link: link.path, administrator: false, execute: execute)
        }
    }

    extension Fixture {
        // Removes the fixture's own app and folders, and nothing of the app under test.
        @MainActor
        static func removal(
            in folder: URL, loginItem: any LoginItem, defaults: UserDefaults,
            commandLine: CommandLineTool
        ) -> AppRemoval {
            AppRemoval(
                loginItem: loginItem, defaults: defaults, domain: suite,
                support: folder.appendingPathComponent(
                    "Application Support", isDirectory: true),
                library: folder.appendingPathComponent("Library", isDirectory: true),
                commandLine: commandLine,
                app: folder.appendingPathComponent("Pitboard.app", isDirectory: true),
                homebrew: [folder.appendingPathComponent("homebrew", isDirectory: true)],
                trash: { try FileManager.default.removeItem(at: $0) })
        }
    }

    @MainActor
    final class FixtureLoginItem: LoginItem {
        private(set) var state: LoginItemState = .disabled
        private let refuses: Bool

        init(refuses: Bool) {
            self.refuses = refuses
        }

        func register() throws {
            if refuses { throw CocoaError(.featureUnsupported) }
            state = .enabled
        }

        func unregister() throws { state = .disabled }
        func openSystemSettings() {}
    }
#endif
