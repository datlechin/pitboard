import Foundation
import PitboardApp
import PitboardKit

@main
enum Main {
    @MainActor
    static func main() {
        let helper = appCommandLine(app: Bundle.main.bundleURL.path)
        let action = Launch.action(
            for: CommandLine.arguments, helper: helper,
            environment: ProcessInfo.processInfo.environment)
        switch action {
        case .app:
            Pitboard.main()
        case .renew(let helper):
            Launch.renew(with: helper)
        case .fail:
            Launch.fail("this copy of Pitboard has no command line inside it to renew with.")
        case .outsideFixture:
            Launch.fail(
                "a debug build starts only in a fixture: set PITBOARD_FIXTURE and scratch homes, "
                    + "as CONTRIBUTING.md says.")
        }
    }
}
