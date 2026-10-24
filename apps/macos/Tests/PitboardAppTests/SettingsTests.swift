import Foundation
import PitboardKit
import PitboardTestSupport
import SwiftUI
import Testing
import UserNotifications

@testable import PitboardApp

// Daily renewal and which `pitboard` a terminal runs are the model's, tested in `pitboard-ffi`.

/// Answers the way macOS can: a registration may wait for approval or be refused, and the
/// person may change it in System Settings while the app is running.
@MainActor
private final class ScriptedLoginItem: LoginItem {
    var state: LoginItemState
    var needsApproval = false
    var refusing: Error?
    private(set) var settingsOpened = 0

    init(_ state: LoginItemState = .disabled) {
        self.state = state
    }

    func register() throws {
        if let refusing { throw refusing }
        state = needsApproval ? .requiresApproval : .enabled
    }
    func unregister() throws {
        if let refusing { throw refusing }
        state = .disabled
    }
    func openSystemSettings() { settingsOpened += 1 }
}

// MARK: - Opening at login

/// The switch shows what macOS has, not what was last asked for, and the person can change
/// it in System Settings at any time, so it is read again when the settings ask.
@MainActor
@Test func openingAtLoginIsWhatMacOSSays() {
    let item = ScriptedLoginItem(.enabled)
    let openAtLogin = OpenAtLogin(item)
    #expect(openAtLogin.state == .enabled)

    openAtLogin.set(false)
    #expect(openAtLogin.state == .disabled)
    openAtLogin.set(true)
    #expect(openAtLogin.state == .enabled)
    #expect(openAtLogin.failed == nil)

    item.state = .disabled
    #expect(openAtLogin.state == .enabled, "not read again until something asks")
    openAtLogin.read()
    #expect(openAtLogin.state == .disabled)
}

/// The model's setup tip asks to open at login only while macOS says the app does not, so
/// each read and each change says what macOS has: a login item waiting for approval is not
/// one to ask about again.
@MainActor
@Test func openingAtLoginIsSaidToTheModelAsMacOSHasIt() {
    let item = ScriptedLoginItem()
    var said: [Bool] = []
    let openAtLogin = OpenAtLogin(item) { said.append($0) }
    #expect(said.isEmpty, "nothing said before anything asks")

    openAtLogin.read()
    item.needsApproval = true
    openAtLogin.set(true)
    item.state = .disabled
    openAtLogin.read()
    #expect(said == [false, true, false])
}

/// macOS can hold a new login item until the person allows it, and until then the app does
/// not open at login. That is not a failure.
@MainActor
@Test func aLoginItemWaitingForApprovalSaysSo() {
    let item = ScriptedLoginItem()
    item.needsApproval = true
    let openAtLogin = OpenAtLogin(item)

    openAtLogin.set(true)
    #expect(openAtLogin.state == .requiresApproval)
    #expect(openAtLogin.failed == nil)
    openAtLogin.openSystemSettings()
    #expect(item.settingsOpened == 1)

    item.state = .enabled
    openAtLogin.read()
    #expect(openAtLogin.state == .enabled)
}

@MainActor
@Test func aRefusedLoginItemSaysWhyAndShowsWhatMacOSHas() {
    let item = ScriptedLoginItem()
    item.refusing = NSError(
        domain: "LoginItems", code: 1,
        userInfo: [NSLocalizedDescriptionKey: "Operation not permitted"])
    let openAtLogin = OpenAtLogin(item)

    openAtLogin.set(true)
    #expect(openAtLogin.failed == "Operation not permitted")
    #expect(openAtLogin.state == .disabled)

    item.state = .enabled
    openAtLogin.set(false)
    #expect(openAtLogin.failed == "Operation not permitted")
    #expect(openAtLogin.state == .enabled)

    item.refusing = nil
    openAtLogin.set(false)
    #expect(openAtLogin.failed == nil)
    #expect(openAtLogin.state == .disabled)
}

// MARK: - Notifications turned off

/// What macOS has is read on each visit and as the app comes forward, since the person turns
/// notifications on and off in System Settings. Only a refusal is said: not asked yet, or a
/// build that posts nothing, is nothing to say.
@MainActor
private final class Authorization {
    var status: UNAuthorizationStatus? = .denied
}

@MainActor
@Test func notificationsTurnedOffAreSaidUntilTurnedOnAgain() async {
    let macOS = Authorization()
    let notifications = NotificationsAllowed(status: { macOS.status })
    #expect(!notifications.off, "not before it is read")

    await notifications.read()
    #expect(notifications.off)
    for allowed in [UNAuthorizationStatus.authorized, .provisional, .notDetermined] {
        macOS.status = allowed
        await notifications.read()
        #expect(!notifications.off, "\(allowed)")
    }
    macOS.status = nil
    await notifications.read()
    #expect(!notifications.off)
}

// MARK: - Fitting the screen

@MainActor
private func settingsHeight(_ tab: SettingsView.Tab, warming: Int = 0) -> CGFloat {
    let defaults = TestDefaults()
    defaults.set(tab.rawValue, forKey: DefaultsKey.settingsTab)
    let accounts = (0..<warming).map {
        WarmAccountShown(
            qualified: "claude/\($0)", title: "\($0)", plan: "Max 5x", on: false, standing: nil)
    }
    let model = AppModel(model: StandInModel(snapshot(0, warming: accounts)))
    let settings = SettingsView(
        model: model, openAtLogin: OpenAtLogin(ScriptedLoginItem()),
        notifications: NotificationsAllowed(status: { nil }),
        commandLineLink: CommandLineLink(
            CommandLineTool(helper: nil, link: "/nowhere/pitboard", execute: { _ in nil }),
            model: model),
        presence: AppPresence(apply: { _ in }, bringForward: {}, settle: {}),
        updates: NoUpdates())
    return NSHostingView(rootView: settings.defaultAppStorage(defaults)).fittingSize.height
}

// Only Limits lists accounts, so only it can outgrow a 1024 x 768 screen and scroll.
@MainActor
@Test func aLongPaneScrollsAndAShortOneKeepsItsHeight() {
    #expect(settingsHeight(.limits, warming: 20) == SettingsView.tallestPane)
    for tab in SettingsView.Tab.allCases where tab != .limits {
        #expect(settingsHeight(tab, warming: 20) < SettingsView.tallestPane, "\(tab)")
    }
}

// Renewal and Command Line went to Advanced, and Updates to General.
@Test func aPaneKeptByAnEarlierVersionOpensThePaneThatHoldsItNow() {
    #expect(SettingsView.Tab.allCases == [.general, .limits, .advanced])
    #expect(SettingsView.Tab(rawValue: "general") == .general)
    #expect(SettingsView.Tab(rawValue: "limits") == .limits)
    #expect(SettingsView.Tab(rawValue: "renewal") == .advanced)
    #expect(SettingsView.Tab(rawValue: "commandLine") == .advanced)
    #expect(SettingsView.Tab(rawValue: "updates") == .general)
    #expect(
        SettingsView.Tab.allCases.allSatisfy { SettingsView.Tab(rawValue: $0.rawValue) == $0 })
}

// MARK: - The share a limit is reached at

// Typed past either end, the share is taken at that end, where the stepper stops.
@Test func aShareTypedIsKeptWithinTheStepsRange() {
    let atLimit = snapshot(0).machine.atLimit
    #expect(LimitsSettings.share(80, of: atLimit) == 80)
    #expect(LimitsSettings.share(10, of: atLimit) == 50)
    #expect(LimitsSettings.share(120, of: atLimit) == 99)
}

// MARK: - Linking the command line

/// Once the link is made the model is asked to look for the `pitboard` a terminal runs
/// again, which is how the settings come to show this app's own on the `PATH`.
@MainActor
@Test func linkingSaysSoUntilThePasswordPromptIsAnswered() async throws {
    let app = try StandInApp()
    defer { app.remove() }
    let (prompted, prompting) = AsyncStream.makeStream(of: Void.self)
    let answered = DispatchSemaphore(value: 0)
    let standIn = StandInModel(snapshot(0))
    let link = CommandLineLink(
        app.tool { _ in
            prompting.yield()
            answered.wait()
            app.makeLink()
            return nil
        }, model: AppModel(model: standIn))

    let linking = Task { await link.install() }
    for await _ in prompted { break }
    #expect(link.linking)
    #expect(standIn.sent.isEmpty, "nothing is looked for while the prompt is up")
    answered.signal()
    await linking.value
    #expect(!link.linking)
    #expect(link.failed == nil)
    #expect(standIn.sent == [.lookForCommandLine])
    #expect(FileManager.default.fileExists(atPath: app.link))
}

/// Said beside the button, never as a failure in the window: the model is only asked to look
/// again. A copy with nothing to link says so without asking for a password.
@MainActor
@Test func aLinkThatFailedSaysWhyAndADismissedPromptSaysNothing() async throws {
    let app = try StandInApp()
    defer { app.remove() }
    let scripts = Scripts()
    let standIn = StandInModel(snapshot(0))
    let link = CommandLineLink(app.tool(scripts.run), model: AppModel(model: standIn))

    scripts.raise(1, "ln: \(app.link): Permission denied")
    await link.install()
    #expect(link.failed == "ln: \(app.link): Permission denied")
    #expect(!link.linking)

    scripts.raise(-128, "User canceled.")
    await link.install()
    #expect(link.failed == nil)
    #expect(scripts.ran.count == 2)
    #expect(standIn.sent == [.lookForCommandLine, .lookForCommandLine])

    let unlinkable = CommandLineLink(
        CommandLineTool(helper: nil, link: app.link, execute: scripts.run),
        model: AppModel(model: standIn))
    await unlinkable.install()
    #expect(
        unlinkable.failed == "This copy of Pitboard cannot link the command line inside it.")
    #expect(scripts.ran.count == 2)
}
