#if DEBUG
    import Foundation
    import PitboardKit

    /// A machine in a known state, for the UI tests and for looking at the app without
    /// touching the one it runs on. The worlds are Rust's, `pitboard-ffi`'s fixtures, by the
    /// names `fixtureNames()` gives: the real core and the real model over a machine of their
    /// own. Only a debug build reads `PITBOARD_FIXTURE`, so nothing outside can put a release
    /// in a world that is not real, and only a library built with `--fixture` has them.
    enum Fixture {
        static let variable = "PITBOARD_FIXTURE"
        // Set, macOS refuses the fixture's login item, as it can refuse an app's own.
        static let loginItemRefuses = "PITBOARD_FIXTURE_LOGIN_ITEM_REFUSES"

        /// The defaults a fixture keeps the app's view preferences in, emptied at every launch
        /// so each test starts from the same place and nothing reaches the real app's. The
        /// model's own preferences are in the fixture's Pitboard directory.
        static let suite = "com.usepitboard.Pitboard.fixture"

        /// The fixture's folder, `pitboard-fixture` in the temporary directory as Rust's
        /// `std::env::temp_dir` finds it: `TMPDIR` where it is set, which Foundation's
        /// temporary directory does not read, and the user's own temporary directory where
        /// it is not. The model makes the world there, and the native stand-ins keep what
        /// they make beside it.
        static func folder(environment: [String: String]) -> URL {
            let temporary =
                environment["TMPDIR"].map { URL(fileURLWithPath: $0, isDirectory: true) }
                ?? FileManager.default.temporaryDirectory
            return temporary.appendingPathComponent("pitboard-fixture", isDirectory: true)
        }

        /// The world reads, notices changes and reads when a menu opens, as the app does on a
        /// real machine, since that is what the UI tests test. It posts no notification.
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

        // The app's own shell commands, in the fixture's folder, run with no password.
        static func commandLineTool(in folder: URL) -> CommandLineTool {
            let helper = folder.appendingPathComponent("Pitboard.app/Contents/Helpers/pitboard")
            let link = folder.appendingPathComponent("bin/pitboard")
            return CommandLineTool(
                helper: helper.path, link: link.path,
                execute: { source in
                    CommandLineTool.execute(
                        script: source.replacingOccurrences(
                            of: " with administrator privileges", with: ""))
                })
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
