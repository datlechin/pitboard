import Foundation
import PitboardKit

/// Where this program starts, before any of the app does.
///
/// A renewal schedule an app up to 0.3.0 wrote starts the app itself with `renew`, and
/// nothing else starts it with arguments. When this app has replaced that one, the command
/// line inside it does the renewal in the same process, which launchd goes on tracking as
/// the job. Started as the menu bar app, it would repair the schedule from inside that job,
/// and launchd would stop it when the repair unloads the job, before it is loaded again.
@main
enum Launch {
    enum Action: Equatable {
        /// The menu bar app.
        case app
        /// The command line at this path, with `renew`, in this process's place.
        case renew(String)
        /// Nothing: a renewal was asked for, and there is no command line here to do it.
        case fail
    }

    /// What `arguments`, as this program was started with them, ask for. `helper` is the
    /// command line inside this app, nil where it is not running from an app bundle.
    static func action(for arguments: [String], helper: String?) -> Action {
        guard arguments.dropFirst() == ["renew"] else { return .app }
        return helper.map(Action.renew) ?? .fail
    }

    @MainActor
    static func main() {
        let helper = Settings.bundledCommandLine(in: Bundle.main.bundleURL)
        switch action(for: CommandLine.arguments, helper: helper) {
        case .app:
            PitboardApp.main()
        case .renew(let helper):
            let arguments = [strdup("pitboard"), strdup("renew"), nil]
            execv(helper, arguments)
            fail("could not run \(helper): \(String(cString: strerror(errno)))")
        case .fail:
            fail("this copy of pitboard has no command line inside it to renew with.")
        }
    }

    private static func fail(_ message: String) -> Never {
        FileHandle.standardError.write(Data("pitboard: \(message)\n".utf8))
        exit(1)
    }
}
