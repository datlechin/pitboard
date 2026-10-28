import Foundation
import Observation

/// What the app shows, as the Rust model last said it, and the one way to ask that model for
/// anything.
///
/// The model in `pitboard-model` holds every rule: what is read and when, what a switch does,
/// what each sentence and row says, and which parts of a snapshot changed. This holds its
/// newest snapshot on the main actor, a property for each part, so a view reads what it shows
/// and SwiftUI draws again only what changed. A view asks for something by sending an
/// `Intent`, which returns at once: what comes of it arrives as a newer snapshot.
///
/// The menu, the window and the settings all read this one model, so an account reads the
/// same everywhere and a change made in one shows in the others at once.
@MainActor
@Observable
public final class AppModel {
    @ObservationIgnored private let model: any PitboardModelProtocol

    /// The revision of the snapshot shown. A snapshot that is not newer is dropped, so one
    /// that arrives late never puts back what a newer one replaced.
    public private(set) var revision: UInt64
    /// Whether a read of the accounts is under way, so a Refresh button can say so.
    public private(set) var reading: Bool
    /// A switch waiting for the person to let Pitboard quit an app first.
    public private(set) var quitQuestion: WindowQuestion?
    /// A switch waiting for the person to let Pitboard replace a login its service refuses.
    public private(set) var replaceQuestion: WindowQuestion?
    /// What the model wants of the main window: it opens each time the serial moves.
    public private(set) var windowRequest: WindowRequest
    /// The sign-in under way, and everything its tool has said.
    public private(set) var signingIn: RunningSignIn?
    /// The sheet over the main window, while one is asked for.
    public private(set) var sheet: Sheet?
    /// What went wrong in the sheet that is up, said inside it.
    public private(set) var sheetFailure: FailureText?
    /// What the menu bar item says beside its mark, in each form its setting can choose. The
    /// model says it from launch to quit: it is nil only on a front with no status item.
    public private(set) var statusItem: StatusItemShown?
    /// The accounts, a section per tool once there is more than one.
    public private(set) var sections: [AccountSection]
    /// The Account menu's items while no account is selected.
    public private(set) var accountMenu: AccountMenuShown
    /// Everything to tell somebody that is not an account, the most pressing first.
    public private(set) var notices: [PanelNotice]
    /// What the menu says of the notices.
    public private(set) var menuNotices: MenuNotices
    /// How far setting Pitboard up this machine is.
    public private(set) var footing: Footing
    /// The one next thing to do on a machine that is not set up yet.
    public private(set) var setup: SetupStep?
    /// What the window's accounts pane shows: its list, or what stands in for one.
    public private(set) var accountsShown: AccountsShown
    /// What the menu says where it has no accounts to list.
    public private(set) var menuAccountsNote: String?
    /// When the accounts were read, as the menu's Refresh item says it.
    public private(set) var updatedMenu: String
    /// The same, as the window's subtitle says it.
    public private(set) var updatedWindow: String
    /// What the sheet over the main window says, while one is up that names an account.
    public private(set) var sheetText: SheetText?
    /// What a sign-in under way says.
    public private(set) var signingInText: SigningInText?
    /// What the sheet for putting away a login left in a file says, while it is up.
    public private(set) var stowText: StowText?
    /// The alert for something asked for that did not happen, until somebody has read it.
    public private(set) var failureAlert: AlertText?
    /// What the settings and the window's other panes show of this machine.
    public private(set) var machine: MachineShown
    /// The account windows: which accounts have one, the windows open and what each loads,
    /// the windows to close and the stores to delete, the link waiting for an account, and
    /// the downloads.
    public private(set) var accountWindows: AccountWindowsShown
    public private(set) var uninstall: ItemOffer
    public private(set) var removal: RemovalShown?
    public private(set) var welcome: WelcomeShown

    /// Told the account windows' part each time a snapshot shown names it, once that snapshot
    /// is shown, for what only the app's web code can do: delete the stores the model asks it
    /// to. Which stores those are, and when, is the model's.
    @ObservationIgnored public var accountWindowsChanged:
        (@MainActor (AccountWindowsShown) -> Void)?

    @ObservationIgnored private var lastRequest: UInt64 = 0
    @ObservationIgnored private var answering: [UInt64: Answering] = [:]
    @ObservationIgnored private var lastFeedWait = 0
    @ObservationIgnored private var feedWaits: [Int: CheckedContinuation<Void, Never>] = [:]

    // Told each time what a widget shows of the widget feed changes, once its snapshot is shown.
    @ObservationIgnored public var widgetFeedChanged: (@MainActor () -> Void)?

    /// The app's model over `model`, showing its last snapshot until a newer one comes.
    public init(model: any PitboardModelProtocol) {
        self.model = model
        let first = model.snapshot()
        revision = first.revision
        reading = first.reading
        quitQuestion = first.quitQuestion
        replaceQuestion = first.replaceQuestion
        windowRequest = first.windowRequest
        signingIn = first.signingIn
        sheet = first.sheet
        sheetFailure = first.sheetFailure
        statusItem = first.statusItem
        sections = first.sections
        accountMenu = first.accountMenu
        notices = first.notices
        menuNotices = first.menuNotices
        footing = first.footing
        setup = first.setup
        accountsShown = first.accountsShown
        menuAccountsNote = first.menuAccountsNote
        updatedMenu = first.updatedMenu
        updatedWindow = first.updatedWindow
        sheetText = first.sheetText
        signingInText = first.signingInText
        stowText = first.stowText
        failureAlert = first.failureAlert
        machine = first.machine
        accountWindows = first.accountWindows
        uninstall = first.uninstall
        removal = first.removal
        welcome = first.welcome
    }

    /// The model `make` makes, told of its snapshots through a listener that hands each to
    /// this app's model on the main actor, in order. The listener holds the app's model
    /// weakly: the model holds the listener, and the app's model holds the model.
    public static func listening(
        _ make: (any Listener) throws -> any PitboardModelProtocol
    ) rethrows -> AppModel {
        let listener = ModelListening()
        let app = AppModel(model: try make(listener))
        listener.deliver = { [weak app] snapshot in app?.apply(snapshot) }
        return app
    }

    /// Asks the model for `intent`. Returns at once, whatever it asks for: what comes of it
    /// arrives as a newer snapshot.
    public func send(_ intent: Intent) {
        model.send(intent: intent)
    }

    // As a widget, a control or a shortcut asks, answered by the model's snapshot, or nil once
    // `patience` is up.
    public func ask(_ ask: OutsideAsk, within patience: Duration) async -> OutsideAnswer? {
        lastRequest += 1
        let request = lastRequest
        let late = Task {
            try? await Task.sleep(for: patience)
            answering.removeValue(forKey: request)?.resume(returning: nil)
        }
        defer { late.cancel() }
        return await withCheckedContinuation { answer in
            answering[request] = answer
            send(.outside(request: request, ask: ask))
        }
    }

    // What a widget reads, for an intent in the app: after the launch's first read, which it
    // waits for as long as `patience`.
    public func widgetFeed(within patience: Duration) async -> String? {
        if let feed = model.widgetFeed() { return feed }
        lastFeedWait += 1
        let waiting = lastFeedWait
        let late = Task {
            try? await Task.sleep(for: patience)
            feedWaits.removeValue(forKey: waiting)?.resume()
        }
        defer { late.cancel() }
        await withCheckedContinuation { feedWaits[waiting] = $0 }
        return model.widgetFeed()
    }

    /// Stops the model, a sign-in under way with it, for the app to call as it quits.
    public func shutdown() {
        model.shutdown()
    }

    /// Shows `snapshot` where it is newer than the one shown, and drops it otherwise. Where it
    /// changed since the one shown, it assigns only the parts it names, and every part where
    /// it changed since another, comparing none: which parts differ is the Rust model's to
    /// say. Returns the parts it assigned, none for a snapshot dropped.
    @discardableResult
    public func apply(_ snapshot: Snapshot) -> [Part] {
        guard snapshot.revision > revision else { return [] }
        let parts = snapshot.changed.since == revision ? snapshot.changed.parts : everyPart()
        revision = snapshot.revision
        for part in parts { assign(part, of: snapshot) }
        if parts.contains(.accountWindows) { accountWindowsChanged?(accountWindows) }
        if parts.contains(.answers) {
            for answer in snapshot.answers {
                answering.removeValue(forKey: answer.request)?.resume(returning: answer)
            }
        }
        // The model makes its widget feed before it tells the snapshot that moved its revision.
        if parts.contains(.widgetRevision) {
            for waiting in feedWaits.values { waiting.resume() }
            feedWaits = [:]
            widgetFeedChanged?()
        }
        return parts
    }

    private func assign(_ part: Part, of snapshot: Snapshot) {
        switch part {
        case .reading: reading = snapshot.reading
        case .quitQuestion: quitQuestion = snapshot.quitQuestion
        case .replaceQuestion: replaceQuestion = snapshot.replaceQuestion
        case .windowRequest: windowRequest = snapshot.windowRequest
        case .signingIn: signingIn = snapshot.signingIn
        case .sheet: sheet = snapshot.sheet
        case .sheetFailure: sheetFailure = snapshot.sheetFailure
        case .statusItem: statusItem = snapshot.statusItem
        case .sections: sections = snapshot.sections
        case .accountMenu: accountMenu = snapshot.accountMenu
        case .notices: notices = snapshot.notices
        case .menuNotices: menuNotices = snapshot.menuNotices
        case .footing: footing = snapshot.footing
        case .setup: setup = snapshot.setup
        case .accountsShown: accountsShown = snapshot.accountsShown
        case .menuAccountsNote: menuAccountsNote = snapshot.menuAccountsNote
        case .updatedMenu: updatedMenu = snapshot.updatedMenu
        case .updatedWindow: updatedWindow = snapshot.updatedWindow
        case .sheetText: sheetText = snapshot.sheetText
        case .signingInText: signingInText = snapshot.signingInText
        case .stowText: stowText = snapshot.stowText
        case .failureAlert: failureAlert = snapshot.failureAlert
        case .machine: machine = snapshot.machine
        case .accountWindows: accountWindows = snapshot.accountWindows
        case .uninstall: uninstall = snapshot.uninstall
        case .removal: removal = snapshot.removal
        case .welcome: welcome = snapshot.welcome
        // Answered and told by `apply`, or nothing this app shows: the menu says when the
        // accounts were read in words, and the Jump List is Windows'.
        case .answers, .widgetRevision, .updatedAt, .quickActions: break
        }
    }

    // MARK: - What the views ask of what is shown

    /// The row shown for the account with `id`.
    public func item(_ id: String) -> AccountItem? {
        sections.lazy.flatMap(\.accounts).first { $0.id == id }
    }
}

private typealias Answering = CheckedContinuation<OutsideAnswer?, Never>

/// Hands each snapshot the model tells of to the main actor, in the order it is told.
///
/// The model tells its listener on a thread of its own, one call at a time, and the next
/// waits for this one to return, so this only queues the snapshot on the main queue, which
/// runs what it is given in the order it was given, and returns.
public final class ModelListening: Listener {
    /// Where each snapshot goes, on the main actor.
    @MainActor public var deliver: (@MainActor (Snapshot) -> Void)?

    public init() {}

    public func changed(snapshot: Snapshot) throws {
        DispatchQueue.main.async {
            MainActor.assumeIsolated { self.deliver?(snapshot) }
        }
    }
}
