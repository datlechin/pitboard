import Foundation

/// The claude.ai windows: the page each open one shows, the requests to open one, and the
/// stores behind them.
///
/// It has no `Core` and so no path to any login: a claude.ai window is signed in on
/// claude.ai's own page, and WebKit keeps that sign-in in the account's store. What this
/// decides is which stores exist, and it deletes those it made that no enrolled account
/// derives any more.
@MainActor
@Observable
final class WebModel {
    @ObservationIgnored let web: ClaudeWeb
    /// Every window's downloads, kept past the window that started them.
    @ObservationIgnored let downloads: WebDownloads
    /// The page of each claude.ai window open, by its account's store.
    private(set) var pages: [UUID: ClaudePage] = [:]
    /// The windows signing out, which show that in place of a page.
    private(set) var signingOut: Set<UUID> = []
    /// The store whose window the last request asks for.
    private(set) var requested: UUID?
    /// Counts the requests for a claude.ai window. The menu bar item, the one view that is
    /// always there, opens `requested`'s window whenever this moves.
    private(set) var requests = 0
    /// A link each window still opening loads first, in place of claude.ai's home page.
    private(set) var opening: [UUID: URL] = [:]

    /// The deletions under way, one per store, so a forget and a sweep deleting the same
    /// store wait on one attempt rather than fighting over it.
    @ObservationIgnored private var removals: [UUID: Task<(any Error)?, Never>] = [:]
    @ObservationIgnored private var sweeping: Task<Void, Never>?
    @ObservationIgnored private var sweepAgain = false
    /// The stores the last read said to keep, nil before the first read.
    @ObservationIgnored private var kept: Set<UUID>?
    /// Whether the last sweep left a store it could not delete, to try again at the next
    /// read even when the accounts have not changed.
    @ObservationIgnored private var leftovers = false
    /// Counts each store's window closing, so a sign-out that finishes after its window
    /// closed puts no page back for a window that is gone.
    @ObservationIgnored private var closings: [UUID: Int] = [:]

    /// The pauses between attempts to delete a store WebKit says is still in use: its
    /// network process can hold on to a store for a moment after the last page closes.
    static let retryPauses: [Duration] = [
        .milliseconds(250), .milliseconds(500), .seconds(1), .seconds(2),
    ]

    init(web: ClaudeWeb) {
        self.web = web
        downloads = WebDownloads(folder: web.downloads)
        downloads.page = { [weak self] in self?.pages[$0] }
    }

    /// Whether `store`'s window is open.
    func isOpen(_ store: UUID) -> Bool { pages[store] != nil }

    /// Whether the app shows in the Dock and the Command-Tab switcher: while any claude.ai
    /// window is open, since a menu bar app's window behind another app has no other way
    /// back. A window signing out is still open.
    var wantsDockIcon: Bool { !pages.isEmpty || !signingOut.isEmpty }

    // MARK: - Opening a window

    /// Asks for `store`'s window, at `link` when there is one. A window already open loads
    /// the link at once, as a link does in a browser tab; one still opening takes it as its
    /// first page instead of claude.ai's home page.
    func open(_ store: UUID, at link: URL? = nil) {
        if let link {
            if let page = pages[store] {
                page.load(link)
            } else {
                opening[store] = link
            }
        }
        requested = store
        requests += 1
    }

    /// The page of `account`'s window, made when the window first shows. A page for a store
    /// WebKit did not list before says how to sign in, since that store has never been
    /// signed in. Nil when the window closed meanwhile.
    func attach(_ account: ClaudeAccount) async -> ClaudePage? {
        if let page = pages[account.store] {
            page.update(account)
            return page
        }
        let existed = await web.stores.identifiers().contains(account.store)
        guard !Task.isCancelled, !signingOut.contains(account.store) else { return nil }
        if let page = pages[account.store] { return page }
        let page = ClaudePage(
            store: account.store, account: account, web: web, downloads: downloads,
            first: opening.removeValue(forKey: account.store))
        if !existed { page.note = .firstSignIn(email: account.email) }
        pages[account.store] = page
        return page
    }

    /// The window closed: its page lets its web view go, which ends its process. Its
    /// downloads go on.
    func detach(_ store: UUID) {
        closings[store, default: 0] += 1
        pages.removeValue(forKey: store)?.close()
        opening[store] = nil
    }

    // MARK: - Deleting a store

    /// Deletes this window's sign-in and everything claude.ai keeps for the account on this
    /// Mac, its downloads under way stopped first, then opens claude.ai's sign-in page on a
    /// fresh store. claude.ai is not told, so the account stays signed in everywhere else.
    /// When the store cannot be deleted, the window opens again on it, still signed in, and
    /// the failure is said. A window closed meanwhile gets no page back.
    func signOut(_ store: UUID) async -> ActionFailure? {
        guard let old = pages[store], !signingOut.contains(store) else { return nil }
        let account = old.account
        signingOut.insert(store)
        defer { signingOut.remove(store) }
        pages[store] = nil
        old.close()
        let closed = closings[store]
        let error = await remove(store)
        let failure = error.map {
            ActionFailure("Couldn’t sign out of this window", message: $0.localizedDescription)
        }
        guard closings[store] == closed else { return failure }
        let page = ClaudePage(
            store: store, account: account, web: web, downloads: downloads,
            first: opening.removeValue(forKey: store))
        if failure == nil { page.note = .firstSignIn(email: account.email) }
        pages[store] = page
        return failure
    }

    /// Closes `store`'s window's page, ahead of deleting the store.
    func close(_ store: UUID) {
        detach(store)
    }

    /// Deletes a forgotten account's store: closes its page and stops its downloads, then
    /// deletes it, trying again while WebKit says it is in use. What still fails is returned,
    /// and the sweep tries again at every later read.
    func discard(_ store: UUID) async -> (any Error)? {
        close(store)
        return await remove(store)
    }

    /// Keeps the stores `ids` names and deletes every other this pitboard recorded making:
    /// the stores of accounts forgotten here, from the command line, or while the app was
    /// closed. Run whenever a read assigns the accounts, and never before one has. A store
    /// this pitboard did not record is left alone: it can belong to another pitboard home.
    func keep(_ ids: Set<UUID>) {
        guard ids != kept || leftovers else { return }
        kept = ids
        guard sweeping == nil else {
            sweepAgain = true
            return
        }
        sweeping = Task { await sweep() }
    }

    /// Waits for the sweep and every deletion under way, for a test.
    func settle() async {
        while let sweeping { await sweeping.value }
        for removal in removals.values { _ = await removal.value }
    }

    /// One sweep at a time. A request that arrives while one runs is answered by one more
    /// run once it ends. A deletion that fails is left for the next read and says nothing.
    private func sweep() async {
        repeat {
            sweepAgain = false
            var failed = false
            for id in web.stores.recorded() where kept?.contains(id) == false {
                close(id)
                if await remove(id) != nil { failed = true }
            }
            leftovers = failed
        } while sweepAgain
        sweeping = nil
    }

    /// Deletes `store` if WebKit has it, after stopping its downloads, trying again while
    /// WebKit says it is still in use. Nil when it is gone, and then it is no longer
    /// recorded.
    private func remove(_ store: UUID) async -> (any Error)? {
        if let running = removals[store] { return await running.value }
        let web = web
        let downloads = downloads
        let task = Task { () -> (any Error)? in
            await downloads.stop(for: store)
            guard await web.stores.identifiers().contains(store) else {
                web.stores.unrecord(store)
                return nil
            }
            var last: (any Error)?
            for attempt in 0...Self.retryPauses.count {
                if attempt > 0 { await web.pause(Self.retryPauses[attempt - 1]) }
                do {
                    try await web.stores.remove(store)
                    web.stores.unrecord(store)
                    return nil
                } catch {
                    last = error
                }
            }
            return last
        }
        removals[store] = task
        let failed = await task.value
        removals[store] = nil
        return failed
    }
}
