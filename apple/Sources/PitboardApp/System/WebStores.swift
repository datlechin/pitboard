import AppKit
import Foundation
import PitboardLinks
import WebKit

/// Where each claude.ai window keeps what claude.ai stores: cookies, local storage and
/// everything else a browser keeps for a site, one store per account.
///
/// pitboard makes a store, lists which exist, and deletes one. It never reads what is in
/// one, copies it or changes it: WebKit keeps the sign-in exactly as a browser would.
@MainActor
protocol WebStores: AnyObject {
    /// The store for `id`, made if there is none, and recorded as made by this pitboard
    /// before it is. One object per identifier: two objects for one identifier stop their
    /// web views sharing processes.
    func store(for id: UUID) -> WKWebsiteDataStore
    /// Every store made by identifier there is, whoever made it.
    func identifiers() async -> Set<UUID>
    /// The stores this pitboard recorded making, the only ones the sweep deletes.
    func recorded() -> Set<UUID>
    /// Forgets that this pitboard made `id`, once it is gone.
    func unrecord(_ id: UUID)
    /// Deletes the store and everything in it. Throws while a web view still uses it.
    func remove(_ id: UUID) async throws
}

/// The stores one pitboard directory made, kept in the app's preferences under that
/// directory's path.
///
/// WebKit keeps every store of one app under the person's own Library, whatever `HOME`
/// says, while the accounts come from `PITBOARD_HOME` or `HOME`. So a copy of the app run
/// with another home, as a test of a build is, sees the stores of the copy installed and
/// none of its accounts. A store it did not record making is not evidence of a forgotten
/// account, and deleting it would sign that account's window out.
@MainActor
struct WebStoreRecord {
    let defaults: UserDefaults
    /// The pitboard directory the app reads its accounts from.
    let directory: String

    var ids: Set<UUID> {
        Set((all[directory] ?? []).compactMap(UUID.init(uuidString:)))
    }

    func add(_ id: UUID) {
        guard !ids.contains(id) else { return }
        save(ids.union([id]))
    }

    func remove(_ id: UUID) {
        guard ids.contains(id) else { return }
        save(ids.subtracting([id]))
    }

    private var all: [String: [String]] {
        defaults.object(forKey: DefaultsKey.webStores) as? [String: [String]] ?? [:]
    }

    private func save(_ ids: Set<UUID>) {
        var all = self.all
        all[directory] = ids.isEmpty ? nil : ids.map(\.uuidString).sorted()
        defaults.set(all, forKey: DefaultsKey.webStores)
    }

    /// The pitboard directory the core reads, as the app hands it the environment:
    /// `PITBOARD_HOME`, or `.pitboard` in `HOME`.
    static func directory(environment: [String: String]) -> String {
        if let set = environment["PITBOARD_HOME"] {
            return URL(fileURLWithPath: set).standardizedFileURL.path
        }
        let home = environment["HOME"] ?? FileManager.default.homeDirectoryForCurrentUser.path
        return URL(fileURLWithPath: home).appendingPathComponent(".pitboard")
            .standardizedFileURL.path
    }
}

/// WebKit's persistent stores, under `~/Library/WebKit/<bundle id>/WebsiteDataStore`.
@MainActor
final class WebKitStores: WebStores {
    private var stores: [UUID: WKWebsiteDataStore] = [:]
    private let record: WebStoreRecord

    init(record: WebStoreRecord) {
        self.record = record
    }

    func store(for id: UUID) -> WKWebsiteDataStore {
        if let store = stores[id] { return store }
        // Recorded first: a store WebKit has made and nobody recorded would never be swept.
        record.add(id)
        let store = WKWebsiteDataStore(forIdentifier: id)
        stores[id] = store
        return store
    }

    func identifiers() async -> Set<UUID> {
        Set(await WKWebsiteDataStore.allDataStoreIdentifiers)
    }

    func recorded() -> Set<UUID> { record.ids }

    func unrecord(_ id: UUID) { record.remove(id) }

    func remove(_ id: UUID) async throws {
        // WebKit refuses a store an object still holds, the one kept here included.
        stores[id] = nil
        try await WKWebsiteDataStore.remove(forIdentifier: id)
    }
}

/// Everything a claude.ai window reaches outside the app through, so a launch decides once
/// which world the windows run in: claude.ai itself, or a fixture's stand-in page that
/// reaches no network.
@MainActor
struct ClaudeWeb {
    /// Where a window starts, and the one origin it keeps.
    let home: URL
    /// WebKit's persistent stores, or a stand-in.
    let stores: any WebStores
    /// Where downloads are saved.
    let downloads: URL
    /// Hands a link to macOS: the default browser for a web page, the default email app for
    /// an address.
    let openElsewhere: @MainActor (URL) -> Void
    /// Anything a configuration needs before a web view is made from it.
    let prepare: @MainActor (WKWebViewConfiguration) -> Void
    /// Waits between attempts to delete a store WebKit still says is in use.
    let pause: @MainActor (Duration) async -> Void

    /// claude.ai, WebKit's stores recorded in `defaults` for the pitboard directory this
    /// launch reads, the Downloads folder and the default browser.
    static func live(
        defaults: UserDefaults,
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> ClaudeWeb {
        let home = FileManager.default.homeDirectoryForCurrentUser
        let record = WebStoreRecord(
            defaults: defaults, directory: WebStoreRecord.directory(environment: environment))
        return ClaudeWeb(
            home: claudeHome,
            stores: WebKitStores(record: record),
            downloads: FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask)
                .first ?? home.appendingPathComponent("Downloads"),
            openElsewhere: { NSWorkspace.shared.open($0) },
            prepare: { _ in },
            pause: { try? await Task.sleep(for: $0) })
    }
}
