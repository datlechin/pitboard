import AppIntents

// What Shortcuts, Spotlight and a widget know of an account: what the glance holds, never an
// email. Use Model hands every property to a model as text.
public struct AccountEntity: AppEntity {
    public static var typeDisplayRepresentation: TypeDisplayRepresentation { "Account" }
    public static var defaultQuery: AccountQuery { AccountQuery() }

    public let id: String
    @Property(title: "Name") public var name: String
    @Property(title: "Tool") public var tool: String
    @Property(title: "Plan") public var plan: String?
    @Property(title: "In Use") public var inUse: Bool
    @Property(title: "Usage") public var usage: String
    @Property(title: "5-Hour Limit Used") public var fiveHourUsed: Double?
    @Property(title: "5-Hour Limit Resets") public var fiveHourResets: Date?
    @Property(title: "Weekly Limit Used") public var weeklyUsed: Double?
    @Property(title: "Weekly Limit Resets") public var weeklyResets: Date?

    public var displayRepresentation: DisplayRepresentation {
        DisplayRepresentation(title: "\(name)", subtitle: "\(usage)")
    }

    public init(_ account: GlanceAccount) {
        id = account.id
        name = account.name
        tool = account.toolName
        plan = account.plan
        inUse = account.inUse
        usage = account.said
        let fiveHour = account.limits.first { $0.kind == .fiveHour }
        let weekly = account.limits.first { $0.kind == .weekly }
        fiveHourUsed = fiveHour?.used
        fiveHourResets = fiveHour?.resetsAt
        weeklyUsed = weekly?.used
        weeklyResets = weekly?.resetsAt
    }
}

public struct AccountQuery: EnumerableEntityQuery {
    @Dependency private var source: GlanceSource

    public init() {}

    public func allEntities() async -> [AccountEntity] {
        Self.entities(in: await source.read())
    }

    public func entities(for identifiers: [String]) async -> [AccountEntity] {
        Self.entities(identifiers, in: await source.read())
    }

    public static func entities(in glance: Glance?) -> [AccountEntity] {
        (glance?.accounts ?? []).map(AccountEntity.init)
    }

    // An account Pitboard no longer has is left out, which Shortcuts says.
    public static func entities(_ ids: [String], in glance: Glance?) -> [AccountEntity] {
        entities(in: glance).filter { ids.contains($0.id) }
    }
}

// Where a process reads the glance: the app from its model, a widget from the group's file.
public struct GlanceSource: Sendable {
    public let read: @Sendable () async -> Glance?

    public init(read: @escaping @Sendable () async -> Glance?) {
        self.read = read
    }
}

// The app's own package lists it, so the system finds the entity and its query in the app.
public struct PitboardGlancePackage: AppIntentsPackage {}
