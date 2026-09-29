import PitboardApp
import SwiftUI

/// The menu bar app. Everything it shows and does is PitboardApp's; what this target adds is
/// Sparkle, which the library leaves out so its tests need no framework only a bundle can
/// load. `Main` starts it.
///
/// The model comes from the app's delegate, which owns it so it exists before a pitboard
/// link that launched the app arrives.
struct Pitboard: App {
    @NSApplicationDelegateAdaptor(PitboardDelegate.self) private var delegate
    @State private var updater = SparkleUpdater()

    var body: some Scene {
        PitboardScenes(model: delegate.model, updates: updater)
    }
}
