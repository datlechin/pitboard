import AppIntents
import PitboardApp
import PitboardGlance
import PitboardKit

// In the app target with the App Shortcuts, which Shortcuts reads only from the app's own
// metadata; a widget extension that links PitboardGlance then declares none of them.

struct SwitchAccountIntent: AppIntent {
    static let title: LocalizedStringResource = "Switch Account"
    static let description = IntentDescription(
        "Switches to the account, as Switch does in Pitboard.")

    @available(macOS 26.0, *)
    static var supportedModes: IntentModes { .background }

    static var parameterSummary: some ParameterSummary {
        Summary("Switch to \(\.$account)")
    }

    @Parameter(title: "Account") var account: AccountEntity
    @Dependency private var model: AppModel

    func perform() async throws -> some IntentResult & ReturnsValue<AccountEntity>
        & ProvidesDialog
    {
        let asking: Confirming?
        if #available(macOS 15.0, *) {
            asking = Actions.confirming(in: self)
        } else {
            asking = nil
        }
        let said = try await Actions.switching(to: account, with: model, asking: asking)
        return .result(value: account, dialog: "\(said)")
    }
}

struct GetUsageIntent: AppIntent {
    static let title: LocalizedStringResource = "Get Usage"
    static let description = IntentDescription(
        "Tells how much of each limit an account, or each account in use, has used.")

    @available(macOS 26.0, *)
    static var supportedModes: IntentModes { .background }

    static var parameterSummary: some ParameterSummary {
        Summary("Get usage of \(\.$account)")
    }

    @Parameter(title: "Account") var account: AccountEntity?
    @Dependency private var model: AppModel

    func perform() async throws -> some IntentResult & ReturnsValue<[AccountEntity]>
        & ProvidesDialog
    {
        let (accounts, said) = try await Actions.usage(of: account, with: model)
        return .result(value: accounts, dialog: "\(said)")
    }
}

struct OpenPitboardIntent: AppIntent {
    static let title: LocalizedStringResource = "Open Pitboard"
    static let description = IntentDescription("Opens Pitboard’s Accounts window.")
    static let openAppWhenRun = true

    @available(macOS 26.0, *)
    static var supportedModes: IntentModes { .foreground(.immediate) }

    @Dependency private var model: AppModel

    func perform() async -> some IntentResult {
        await Actions.opening(model)
        return .result()
    }
}

// Each phrase names the app, as Siri and Spotlight need; one with an account makes a shortcut
// for every account the query has, which updateAppShortcutParameters() reads again.
struct PitboardShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: SwitchAccountIntent(),
            phrases: [
                "Switch \(.applicationName) to \(\.$account)",
                "Switch to \(\.$account) in \(.applicationName)",
                "Switch \(.applicationName) account",
            ],
            shortTitle: "Switch Account",
            systemImageName: "arrow.left.arrow.right")
        AppShortcut(
            intent: GetUsageIntent(),
            phrases: [
                "Get \(.applicationName) usage",
                "How much have I used in \(.applicationName)",
            ],
            shortTitle: "Get Usage",
            systemImageName: "gauge.open.with.lines.needle.33percent")
        AppShortcut(
            intent: OpenPitboardIntent(),
            phrases: [
                "Open \(.applicationName) accounts",
                "Show my \(.applicationName) accounts",
            ],
            shortTitle: "Open Pitboard",
            systemImageName: "person.2")
    }
}

// The entity and its query are PitboardGlance's.
struct PitboardIntents: AppIntentsPackage {
    static var includedPackages: [any AppIntentsPackage.Type] { [PitboardGlancePackage.self] }
}
