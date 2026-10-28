import Foundation
import PitboardKit

// The real core over a fixture's machine in `folder`.
@MainActor
package func fixtureModel(_ name: String, in folder: URL) throws -> AppModel {
    try AppModel.listening { listener in
        try PitboardModel.fixtureIn(
            name: name, front: .macMenuBar, temporaryDirectory: folder.path, listener: listener,
            otherApps: nil, notifier: Unposted(), localTime: Utc())
    }
}

// What is said of time in a fixture's snapshot, in UTC, as `pitboard-model`'s own tests say it.
private final class Utc: LocalTime {
    func clock(epoch: Int64, withWeekday: Bool) throws -> String { "\(epoch)" }
    func sameDay(first: Int64, second: Int64) throws -> Bool { true }
    func dateAndTime(epoch: Int64) throws -> String { "\(epoch)" }
    func timeOfDay(minutes: UInt16) throws -> String { "\(minutes)" }
    func date(epoch: Int64) throws -> String { "\(epoch)" }
}

private final class Unposted: Notifier {
    func post(notice: Notice) throws {}
}
