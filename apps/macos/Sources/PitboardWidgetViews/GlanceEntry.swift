import Foundation
import PitboardGlance
import WidgetKit

public struct GlanceEntry: TimelineEntry, Sendable {
    public let date: Date
    public let glance: Glance?
    // the account a small widget was given, by its id
    public let chosen: String?

    public init(date: Date, glance: Glance?, chosen: String? = nil) {
        self.date = date
        self.glance = glance
        self.chosen = chosen
    }

    // Now, and each reset to come, where its countdown gives way to "Resetting now". Only the
    // app has newer numbers, and it reloads the timeline itself when they move.
    public static func timeline(
        of glance: Glance?, chosen: String? = nil, at now: Date
    ) -> Timeline<GlanceEntry> {
        let resets = Set((glance?.accounts ?? []).flatMap { $0.limits.compactMap(\.resetsAt) })
        let dates = [now] + resets.filter { $0 > now }.sorted()
        return Timeline(
            entries: dates.map { GlanceEntry(date: $0, glance: glance, chosen: chosen) },
            policy: .never)
    }
}

extension Glance {
    // The account chosen while Pitboard still has it, or else the first in use.
    public func account(_ chosen: String?) -> GlanceAccount? {
        accounts.first { $0.id == chosen } ?? accounts.first(where: \.inUse) ?? accounts.first
    }

    public var inUse: [GlanceAccount] { accounts.filter(\.inUse) }

    // What a medium widget has room for: two accounts in use, one per tool, leaving out those
    // with no usage, as Claude Desktop's, while another has some.
    public var medium: [GlanceAccount] {
        let read = inUse.filter { !$0.limits.isEmpty }
        return Array((read.isEmpty ? inUse : read).prefix(2))
    }
}

// A control's title and value.
public struct ControlShown: Equatable, Sendable {
    public let name: String
    public let value: String

    public init(name: String, value: String) {
        self.name = name
        self.value = value
    }

    public init(_ glance: Glance?, chosen: String?, app: String) {
        let account = glance?.account(chosen)
        name = account?.name ?? glance?.words.noAccounts ?? app
        value = account?.value ?? ""
    }
}
