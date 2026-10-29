import AppKit
import PitboardKit
import SwiftUI

// Alive from launch to quit, so it opens a window asked for away from any view.
struct MenuBarLabel: View {
    let model: AppModel
    let windows: AccountWindows
    let requests: WindowRequests
    let removal: AppRemoval
    @Environment(\.openWindow) private var openWindow
    @Environment(\.openSettings) private var openSettings
    @AppStorage(DefaultsKey.menuBarShows) private var shows = MenuBarShows.nameAndUsage
    @AppStorage(DefaultsKey.settingsTab) private var settingsTab = SettingsView.Tab.general

    var body: some View {
        // The macOS front has its item from launch to quit, and with nothing to say it is the
        // mark alone.
        let item = model.statusItem
        let title = item.map(shows.text(of:)) ?? ""
        // A mark as well as words: on a crowded menu bar macOS drops the widest items
        // first, and an item that is only text is the widest thing up there. A Label would
        // render as the icon alone, so both are placed by hand.
        HStack(spacing: 4) {
            // Hierarchical draws the board's second row dimmed, as the app icon has it.
            Image(item?.symbol ?? StatusItemShown.mark)
                .symbolRenderingMode(.hierarchical)
            if !title.isEmpty {
                Text(title).monospacedDigit()
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(item?.spoken ?? words.appName)
        // As it first appears too: the first launch's request can be in before it is drawn.
        .onChange(of: model.windowRequest.serial, initial: true) {
            guard requests.opens(model.windowRequest) else { return }
            // An app with no Dock icon has nothing to bring forward but itself, and the
            // window opens behind whatever is in front otherwise.
            windows.presence.activate()
            if let window = PlaceWindow(model.windowRequest.place) {
                openWindow(id: window.id)
            } else if case .settings(let section) = model.windowRequest.place {
                settingsTab = SettingsView.Tab(section)
                openSettings()
            }
        }
        // Whichever window asked for it may have closed meanwhile.
        .onChange(of: model.removal) { _, removed in
            if removed == .done { Task { await removal.finish() } }
        }
        .onChange(of: windows.requested) {
            for store in windows.takeRequests() {
                windows.presence.activate()
                openWindow(id: AccountWindowScene.id, value: store)
            }
        }
    }
}

// Symbols in the app's asset catalog; the badge says the menu has something to look at.
extension StatusItemShown {
    static let mark = "pitboard"

    var symbol: String { attention ? "pitboard.badge.exclamationmark" : Self.mark }
}
