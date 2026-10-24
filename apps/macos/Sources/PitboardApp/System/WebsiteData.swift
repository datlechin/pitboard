import Foundation
import WebKit

/// Where each account's window keeps what its site stores: cookies, local storage and
/// everything else a browser keeps for a site, in one WebKit store per account.
///
/// Pitboard makes a store, wipes one and deletes one. It never reads what is in one, copies
/// it or changes it: WebKit keeps a sign-in exactly as a browser would.
@MainActor
protocol WebsiteDataStores: AnyObject {
    /// The store for `id`, made if there is none. While an object for an identifier is
    /// alive, WebKit hands back that same object, so every page of an account shares it.
    func store(for id: UUID) -> WKWebsiteDataStore
    /// Deletes the store `id` names and everything in it. Throws while any object, web view
    /// or WebKit process still holds it; succeeds for an identifier with no store.
    func remove(_ id: UUID) async throws
}

/// WebKit's persistent stores, under `~/Library/WebKit/<bundle id>/WebsiteDataStore`.
@MainActor
final class WebKitDataStores: WebsiteDataStores {
    private var ready = false

    func store(for id: UUID) -> WKWebsiteDataStore {
        ready = true
        return WKWebsiteDataStore(forIdentifier: id)
    }

    func remove(_ id: UUID) async throws {
        setUpWebKit()
        try await WKWebsiteDataStore.remove(forIdentifier: id)
    }

    /// Makes sure WebKit is set up before a deletion asks it to remove a store.
    // `remove(forIdentifier:)` as a process's first WebKit call crashes (docs/facts/webkit.md).
    private func setUpWebKit() {
        guard !ready else { return }
        _ = WKWebsiteDataStore.nonPersistent()
        ready = true
    }
}
