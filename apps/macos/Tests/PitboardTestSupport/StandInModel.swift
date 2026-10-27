import Foundation
import PitboardKit

// Hands out the snapshot a test gives it and keeps every intent, so nothing a test sends
// reaches a core or the machine running it.
package final class StandInModel: PitboardModelProtocol, @unchecked Sendable {
    // Unchecked because every read and write holds `lock`.
    private let lock = NSLock()
    private let first: Snapshot
    private var intents: [Intent] = []
    private var stops = 0
    private var glanced: String?

    package init(_ first: Snapshot) {
        self.first = first
    }

    package var sent: [Intent] { lock.withLock { intents } }
    package var stopped: Int { lock.withLock { stops } }
    package var glanceText: String? {
        get { lock.withLock { glanced } }
        set { lock.withLock { glanced = newValue } }
    }

    package func send(intent: Intent) { lock.withLock { intents.append(intent) } }
    package func shutdown() { lock.withLock { stops += 1 } }
    package func snapshot() -> Snapshot { first }
    package func glance() -> String? { glanceText }
}
