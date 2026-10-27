import Foundation
import PitboardKit

// A 0.3.0 schedule starts the app with `renew`; the app would repair that job from inside it.
public enum Launch {
    public enum Action: Equatable, Sendable {
        case app
        // the command line at this path, with `renew`, in this process's place
        case renew(String)
        // a renewal asked of a copy with no command line inside it
        case fail
        // a debug build started with no fixture, as Launch Services starts it for an intent
        case outsideFixture
    }

    // `helper` is nil where this is not running from an app bundle.
    public static func action(
        for arguments: [String], helper: String?, environment: [String: String]
    ) -> Action {
        #if DEBUG
            guard environment[Fixture.variable] != nil else { return .outsideFixture }
        #endif
        guard arguments.dropFirst() == ["renew"] else { return .app }
        return helper.map(Action.renew) ?? .fail
    }

    public static func renew(with helper: String) -> Never {
        let arguments = [strdup("pitboard"), strdup("renew"), nil]
        execv(helper, arguments)
        fail("could not run \(helper): \(String(cString: strerror(errno)))")
    }

    public static func fail(_ message: String) -> Never {
        FileHandle.standardError.write(Data("pitboard: \(message)\n".utf8))
        exit(1)
    }
}
