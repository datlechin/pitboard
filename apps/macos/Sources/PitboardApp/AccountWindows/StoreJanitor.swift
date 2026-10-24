import Foundation
import PitboardKit
import WebKit

/// Which stores go, and when, is the model's, in `AccountWindowsShown.deleting`. The model
/// records a store before it says a window opens, so every store a window uses is one it can
/// ask for.
@MainActor
final class StoreJanitor {
    private let stores: any WebsiteDataStores
    private let pause: @MainActor (Duration) async -> Void
    /// The asks taken, while the model still lists them: each is deleted once, however many
    /// snapshots list it before the model has heard how it went.
    private var taken: Set<StoreDeletion> = []

    /// How long to wait between attempts to delete a store.
    // WebKit holds one for a moment after its last web view and object go
    // (docs/facts/webkit.md).
    nonisolated static let retries: [Duration] = [50, 100, 200, 400, 800, 1600].map {
        .milliseconds($0)
    }

    init(stores: any WebsiteDataStores, pause: @escaping @MainActor (Duration) async -> Void) {
        self.stores = stores
        self.pause = pause
    }

    /// Removes everything `store` keeps, while its windows go on using it.
    // It clears a store a page is using, cookies and storage included (docs/facts/webkit.md).
    func wipe(_ store: WKWebsiteDataStore) async {
        await store.removeData(
            ofTypes: WKWebsiteDataStore.allWebsiteDataTypes(), modifiedSince: .distantPast)
    }

    /// A store still in use is tried again a few times, then told as held, for the model to
    /// ask for again after its next read.
    func delete(
        _ asked: [StoreDeletion], told: @escaping @MainActor (UUID, Bool) -> Void
    ) {
        taken.formIntersection(asked)
        for ask in asked where !taken.contains(ask) {
            taken.insert(ask)
            // The model drops a recorded store id that is not a UUID as it reads the records,
            // so it never asks for one.
            guard let id = UUID(uuidString: ask.store) else { continue }
            Task {
                told(id, await delete(id))
            }
        }
    }

    private func delete(_ id: UUID) async -> Bool {
        var waits = Self.retries[...]
        while true {
            do {
                try await stores.remove(id)
                return true
            } catch {
                guard let wait = waits.popFirst() else { return false }
                await pause(wait)
            }
        }
    }
}
