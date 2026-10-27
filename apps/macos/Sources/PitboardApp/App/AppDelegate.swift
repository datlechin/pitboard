import AppIntents
import AppKit
import PitboardGlance
import PitboardKit
import SwiftUI

/// Owns the app's models, since AppKit asks it things at any time and it has to answer
/// from the same models the scenes show. Links from the Share extension are not its: the
/// account picker's scene receives them.
@MainActor
public final class AppDelegate: NSObject, NSApplicationDelegate {
    let model: AppModel
    let windows: AccountWindows
    let openAtLogin: OpenAtLogin
    let notificationsAllowed: NotificationsAllowed
    let commandLineLink: CommandLineLink
    let removal: AppRemoval
    let windowRequests = WindowRequests()
    let defaults: UserDefaults
    private let quitting: @MainActor () -> Void
    private var watching: [NSObjectProtocol] = []

    // The app target's App Shortcuts; Main sets it at launch.
    public static var shortcuts: (any AppShortcutsProvider.Type)?

    override public init() {
        let dependencies = Dependencies.forLaunch()
        let model = dependencies.model
        // Before anything else: the system can run an intent as soon as the app has launched.
        AppDependencyManager.shared.add(dependency: model)
        AppDependencyManager.shared.add(
            dependency: GlanceSource {
                await model.glance(within: Actions.patience).flatMap { Glance(Data($0.utf8)) }
            })
        GlanceWatch.live(shortcuts: AppDelegate.shortcuts).watch(model)
        self.model = model
        defaults = dependencies.defaults
        windows = AccountWindows(
            model: model, environment: dependencies.web, presence: .live())
        openAtLogin = OpenAtLogin(dependencies.loginItem) { [model] on in
            model.send(.opensAtLogin(on: on))
        }
        notificationsAllowed = NotificationsAllowed(status: dependencies.notificationStatus)
        commandLineLink = CommandLineLink(dependencies.commandLineTool, model: model)
        removal = dependencies.removal
        quitting = dependencies.quitting
        super.init()
    }

    public func applicationDidFinishLaunching(_ notification: Notification) {
        let model = model
        let openAtLogin = openAtLogin
        watching = [
            // Login Items is changed in System Settings, and coming back is the next look.
            NotificationCenter.default.addObserver(
                forName: NSApplication.didBecomeActiveNotification, object: nil, queue: .main
            ) { _ in
                MainActor.assumeIsolated { openAtLogin.read() }
            },
            // Numbers read before the Mac slept say nothing about now.
            NSWorkspace.shared.notificationCenter.addObserver(
                forName: NSWorkspace.didWakeNotification, object: nil, queue: .main
            ) { _ in
                MainActor.assumeIsolated { model.send(.woke) }
            },
            // The menu bar item's menu is the glance the whole app exists for, and SwiftUI
            // says nothing when it opens. AppKit says it of every menu, so a top-level one is
            // taken to be it.
            NotificationCenter.default.addObserver(
                forName: NSMenu.didBeginTrackingNotification, object: nil, queue: .main
            ) { note in
                let topLevel = (note.object as? NSMenu)?.supermenu == nil
                MainActor.assumeIsolated {
                    if topLevel { model.send(.glanced) }
                }
            },
        ]
        model.send(.start)
        openAtLogin.read()
    }

    public func applicationWillTerminate(_ notification: Notification) {
        model.shutdown()
        quitting()
    }

    /// macOS lists the windows open above this menu.
    public func applicationDockMenu(_ sender: NSApplication) -> NSMenu? {
        let menu = NSMenu()
        for entry in windows.menus {
            switch entry {
            case .one(let title, let account):
                menu.addItem(item(title, opening: account))
            case .several(let title, _, let accounts):
                let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
                let submenu = NSMenu()
                for account in accounts {
                    submenu.addItem(self.item(account.title, opening: account))
                }
                item.submenu = submenu
                menu.addItem(item)
            }
        }
        if !menu.items.isEmpty { menu.addItem(.separator()) }
        let main = NSMenuItem(
            title: words.menu.openPitboard, action: #selector(openMainWindow),
            keyEquivalent: "")
        main.target = self
        menu.addItem(main)
        return menu
    }

    private func item(_ title: String, opening account: WindowAccount) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: #selector(openAccount), keyEquivalent: "")
        item.target = self
        item.representedObject = account.id
        return item
    }

    @objc private func openAccount(_ sender: NSMenuItem) {
        guard let store = sender.representedObject as? UUID,
            let account = windows.account(store)
        else { return }
        windows.request(account)
    }

    @objc private func openMainWindow() {
        model.send(.show(place: nil))
    }

    /// A Dock click with every window minimised brings one back, as AppKit does by itself.
    public func applicationShouldHandleReopen(
        _ sender: NSApplication, hasVisibleWindows flag: Bool
    ) -> Bool {
        if !flag, !sender.windows.contains(where: \.isMiniaturized) {
            model.send(.show(place: nil))
        }
        return true
    }

    /// Quitting stops every download still running, so it asks first, as Safari does, in the
    /// model's words: of a download started a moment before Quit too, which no snapshot may
    /// list yet.
    public func applicationShouldTerminate(_ sender: NSApplication)
        -> NSApplication.TerminateReply
    {
        guard let question = windows.downloads.quitQuestion else { return .terminateNow }
        let alert = NSAlert()
        alert.messageText = question.title
        alert.informativeText = question.message
        alert.addButton(withTitle: question.confirm)
        alert.addButton(withTitle: words.window.cancel)
        // Quit is often chosen from the menu bar item's menu, which leaves the app behind
        // whatever is in front, and an alert from it would come up behind as well.
        NSApp.activate()
        guard alert.runModal() == .alertFirstButtonReturn else { return .terminateCancel }
        windows.downloads.stopAll()
        return .terminateNow
    }
}
