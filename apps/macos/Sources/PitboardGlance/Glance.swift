import Foundation

// The accounts as pitboard-ffi's present/glance.rs writes them, for what links no Rust.
public struct Glance: Decodable, Equatable, Sendable {
    public let readAt: Date?
    public let accounts: [GlanceAccount]

    // Nil for another schema, which an older or a newer Pitboard wrote.
    public init?(_ data: Data) {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        decoder.dateDecodingStrategy = .secondsSince1970
        guard (try? decoder.decode(Schema.self, from: data))?.schema == 1,
            let glance = try? decoder.decode(Glance.self, from: data)
        else { return nil }
        self = glance
    }

    private struct Schema: Decodable {
        let schema: Int
    }
}

public struct GlanceAccount: Decodable, Equatable, Sendable, Identifiable {
    // the same across renames
    public let id: String
    public let tool: String
    public let toolName: String
    public let name: String
    public let inUse: Bool
    public let standing: Standing
    public let plan: String?
    // the menu's line: "5-hour 42% used, Weekly 12%"
    public let said: String
    public let limits: [GlanceLimit]

    public enum Standing: String, Decodable, Sendable {
        case inUse = "in_use"
        case ready
        case expiring
        case needsSignIn = "needs_sign_in"
        case notAdded = "not_added"
        case unusable
        case switching
    }
}

public struct GlanceLimit: Decodable, Equatable, Sendable {
    public let kind: Kind
    // the model a weekly limit of one model's is for
    public let model: String?
    public let name: String
    // percent
    public let used: Double
    public let level: Level
    public let resetsAt: Date?

    public enum Kind: String, Decodable, Sendable {
        case fiveHour = "five_hour"
        case weekly
        case weeklyModel = "weekly_model"
        case other
    }

    public enum Level: String, Decodable, Sendable {
        case plenty
        case low
        case out
    }
}
