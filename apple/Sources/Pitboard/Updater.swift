import Foundation
import Sparkle

/// Updates, when the build was made to receive them. A build from a clone carries no update
/// key, and Sparkle will not run without one, so such a build has no updater and says
/// nothing about updates.
@MainActor
final class Updater {
    private let controller: SPUStandardUpdaterController?

    init() {
        controller =
            Bundle.main.object(forInfoDictionaryKey: "SUPublicEDKey") == nil
            ? nil
            : SPUStandardUpdaterController(
                startingUpdater: true, updaterDelegate: nil, userDriverDelegate: nil)
    }

    var available: Bool { controller != nil }

    func check() {
        controller?.updater.checkForUpdates()
    }
}
