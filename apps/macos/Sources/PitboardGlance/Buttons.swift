import AppIntents

// What a control's button asks of the app, where the system runs it; the widget extension,
// which it never runs in, gives one that does nothing.
public struct GlanceActions: Sendable {
    public let open: @Sendable () async -> Void

    public init(open: @escaping @Sendable () async -> Void) {
        self.open = open
    }
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
