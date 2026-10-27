import AppIntents

// What a widget's or a control's button asks of the app. The app gives its model's; the
// widget extension, where neither runs, gives `refusing`.
public struct GlanceActions: Sendable {
    public let open: @Sendable () async -> Void
    public let switchTo: @Sendable (AccountEntity) async throws -> Void

    public init(
        open: @escaping @Sendable () async -> Void,
        switchTo: @escaping @Sendable (AccountEntity) async throws -> Void
    ) {
        self.open = open
        self.switchTo = switchTo
    }

    public static func refusing(_ source: GlanceSource) -> GlanceActions {
        GlanceActions(
            open: {},
            switchTo: { _ in
                throw Refused(said: await source.read()?.words.openToSwitch ?? "")
            })
    }
}

// What an action that did not do what it was asked says, in the model's words: Shortcuts stops
// the shortcut there, so nothing after it runs on a switch that did not happen.
public struct Refused: Error, Equatable, CustomLocalizedStringResourceConvertible {
    public let said: String

    public init(said: String) {
        self.said = said
    }

    public var localizedStringResource: LocalizedStringResource { "\(said)" }
}

public enum PitboardPlace: String, AppEnum {
    case accounts

    public static var typeDisplayRepresentation: TypeDisplayRepresentation { "Place" }
    public static var caseDisplayRepresentations: [PitboardPlace: DisplayRepresentation] {
        [.accounts: "Accounts"]
    }
}

// A control's button, which can run in the app on macOS 26 only as an OpenIntent: the system
// opens the app for one.
public struct OpenPlaceIntent: OpenIntent {
    public static let title: LocalizedStringResource = "Open Pitboard"
    public static let isDiscoverable = false

    @Parameter(title: "Place") public var target: PitboardPlace
    @Dependency private var actions: GlanceActions

    public init() {}

    public init(target: PitboardPlace) {
        self.target = target
    }

    public func perform() async throws -> some IntentResult {
        await actions.open()
        return .result()
    }
}

#if compiler(>=6.4)
    // A widget's Switch on macOS 27, which the system runs in the app alone; Xcode 27 is the
    // first to know the property that says so.
    @available(macOS 27.0, *)
    public struct SwitchButtonIntent: AppIntent {
        public static let title: LocalizedStringResource = "Switch Account"
        public static let isDiscoverable = false
        public static var allowedExecutionTargets: IntentExecutionTargets { .main }

        @Parameter(title: "Account") public var account: AccountEntity
        @Dependency private var actions: GlanceActions

        public init() {}

        public init(account: AccountEntity) {
            self.account = account
        }

        public func perform() async throws -> some IntentResult {
            try await actions.switchTo(account)
            return .result()
        }
    }
#endif
