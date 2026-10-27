import AppIntents
import PitboardKit
import WidgetKit

// What the system is told as the glance a widget shows moves, and never more often: WidgetKit
// gives a widget "from 40 to 70 refreshes" a day, and the accounts are read every five minutes.
@MainActor
struct GlanceWatch {
    // the app target's App Shortcuts, whose accounts Shortcuts reads again
    let shortcuts: (any AppShortcutsProvider.Type)?
    let reload: @MainActor () -> Void

    static func live(shortcuts: (any AppShortcutsProvider.Type)?) -> GlanceWatch {
        GlanceWatch(shortcuts: shortcuts) {
            WidgetCenter.shared.reloadAllTimelines()
            if #available(macOS 26.0, *) {
                ControlCenter.shared.reloadAllControls()
            }
        }
    }

    func watch(_ model: AppModel) {
        model.glanceChanged = { [shortcuts, reload] in
            shortcuts?.updateAppShortcutParameters()
            reload()
        }
    }
}
