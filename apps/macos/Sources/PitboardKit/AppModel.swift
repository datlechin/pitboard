import Foundation
import Observation

/// What the app shows, as the Rust model last said it, and the one way to ask that model for
/// anything.
///
/// The model in `pitboard-ffi` holds every rule: what is read and when, what a switch does,
/// what each sentence and row says. This holds its newest snapshot on the main actor, a
/// property for each part, so a view reads what it shows and SwiftUI draws again only what
/// changed. A view asks for something by sending an `Intent`, which returns at once: what
/// comes of it arrives as a newer snapshot.
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
    /// When the snapshot shown was made, in epoch seconds.
    public private(set) var now: Int64
    /// Whether a read of the accounts is under way, so a Refresh button can say so.
    public private(set) var reading: Bool
    /// When the accounts were last read, in epoch seconds.
    public private(set) var updatedAt: Int64?
    /// The accounts, as the model last had them. Nil before anything is known, which is not
    /// a machine without accounts.
    public private(set) var status: Status?
    /// Everything the last read or change warned about.
    public private(set) var warnings: [Warning]
    /// Why the last read did not answer, when it did not.
    public private(set) var readFailure: ReadFailure?
    /// An interrupted switch nothing can finish, which the app offers a way out of.
    public private(set) var stuck: Bool
    /// The tools whose program was found, once the first read has asked.
    public private(set) var installed: [Tool]?
    /// The account a switch is under way for, or waiting on the quit question for.
    public private(set) var switchUnderWay: String?
    /// A switch waiting for the person to let Pitboard quit an app first.
    public private(set) var quitQuestion: QuitQuestion?
    /// What each tool's last switch said that is still true.
    public private(set) var lastSwitches: [LastSwitch]
    /// What giving up on an interrupted switch kept, until somebody has read it.
    public private(set) var abandoned: Abandoned?
    /// The last thing asked for that did not happen, for the window to say once.
    public private(set) var failure: Failure?
    /// What the model wants of the main window: it opens each time the serial moves.
    public private(set) var windowRequest: WindowRequest
    /// The sign-in under way, and everything its tool has said.
    public private(set) var signingIn: RunningSignIn?
    /// The sheet over the main window, while one is asked for.
    public private(set) var sheet: Sheet?
    /// What went wrong in the sheet that is up, said inside it.
    public private(set) var sheetFailure: Failure?
    /// What the menu bar item says beside its mark, in each form its setting can choose.
    public private(set) var menuBar: MenuBarText
    /// The accounts, a section per tool once there is more than one.
    public private(set) var sections: [AccountSection]
    /// Whether accounts of more than one tool are shown.
    public private(set) var showsTools: Bool
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
    /// What the quit question asks, while it is asked.
    public private(set) var quitConfirmation: Question?
    /// The alert for `failure`, while there is one to say.
    public private(set) var failureAlert: AlertText?
    /// What the settings and the window's other panes show of this machine.
    public private(set) var machine: MachineShown
    /// The account windows: which accounts have one, the windows open and what each loads,
    /// the windows to close and the stores to delete, the link waiting for an account, and
    /// the downloads.
    public private(set) var accountWindows: AccountWindowsShown

    /// Told the account windows' part each time a snapshot changes it, once that snapshot is
    /// shown, for what only the app's web code can do: delete the stores the model asks it
    /// to. Which stores those are, and when, is the model's.
    @ObservationIgnored public var accountWindowsChanged:
        (@MainActor (AccountWindowsShown) -> Void)?

    /// The app's model over `model`, showing its last snapshot until a newer one comes.
    public init(model: any PitboardModelProtocol) {
        self.model = model
        let first = model.snapshot()
        revision = first.revision
        now = first.now
        reading = first.reading
        updatedAt = first.updatedAt
        status = first.status
        warnings = first.warnings
        readFailure = first.readFailure
        stuck = first.stuck
        installed = first.installed
        switchUnderWay = first.switchUnderWay
        quitQuestion = first.quitQuestion
        lastSwitches = first.lastSwitches
        abandoned = first.abandoned
        failure = first.failure
        windowRequest = first.windowRequest
        signingIn = first.signingIn
        sheet = first.sheet
        sheetFailure = first.sheetFailure
        menuBar = first.menuBar
        sections = first.sections
        showsTools = first.showsTools
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
        quitConfirmation = first.quitConfirmation
        failureAlert = first.failureAlert
        machine = first.machine
        accountWindows = first.accountWindows
    }

    /// The model `make` makes, told of its snapshots through a listener that hands each to
    /// this app's model on the main actor, in order. The listener holds the app's model
    /// weakly: the model holds the listener, and the app's model holds the model.
    public static func listening(
        _ make: (any ModelListener) throws -> any PitboardModelProtocol
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

    /// Stops the model, a sign-in under way with it, for the app to call as it quits.
    public func shutdown() {
        model.shutdown()
    }

    /// Shows `snapshot` where it is newer than the one shown, and drops it otherwise. Each
    /// part is assigned only where it differs, so a view that reads a part that stayed the
    /// same is not drawn again. Returns the parts it assigned, none for a snapshot dropped,
    /// so a test sees which, whatever Observation makes of an assignment.
    @discardableResult
    public func apply(_ snapshot: Snapshot) -> [PartialKeyPath<AppModel>] {
        guard snapshot.revision > revision else { return [] }
        revision = snapshot.revision
        var assigned: [PartialKeyPath<AppModel>] = [\.revision]
        func update<Value: Equatable>(
            _ part: ReferenceWritableKeyPath<AppModel, Value>, _ value: Value
        ) {
            guard self[keyPath: part] != value else { return }
            self[keyPath: part] = value
            assigned.append(part)
        }
        update(\.now, snapshot.now)
        update(\.reading, snapshot.reading)
        update(\.updatedAt, snapshot.updatedAt)
        update(\.status, snapshot.status)
        update(\.warnings, snapshot.warnings)
        update(\.readFailure, snapshot.readFailure)
        update(\.stuck, snapshot.stuck)
        update(\.installed, snapshot.installed)
        update(\.switchUnderWay, snapshot.switchUnderWay)
        update(\.quitQuestion, snapshot.quitQuestion)
        update(\.lastSwitches, snapshot.lastSwitches)
        update(\.abandoned, snapshot.abandoned)
        update(\.failure, snapshot.failure)
        update(\.windowRequest, snapshot.windowRequest)
        update(\.signingIn, snapshot.signingIn)
        update(\.sheet, snapshot.sheet)
        update(\.sheetFailure, snapshot.sheetFailure)
        update(\.menuBar, snapshot.menuBar)
        update(\.sections, snapshot.sections)
        update(\.showsTools, snapshot.showsTools)
        update(\.notices, snapshot.notices)
        update(\.menuNotices, snapshot.menuNotices)
        update(\.footing, snapshot.footing)
        update(\.setup, snapshot.setup)
        update(\.accountsShown, snapshot.accountsShown)
        update(\.menuAccountsNote, snapshot.menuAccountsNote)
        update(\.updatedMenu, snapshot.updatedMenu)
        update(\.updatedWindow, snapshot.updatedWindow)
        update(\.sheetText, snapshot.sheetText)
        update(\.signingInText, snapshot.signingInText)
        update(\.stowText, snapshot.stowText)
        update(\.quitConfirmation, snapshot.quitConfirmation)
        update(\.failureAlert, snapshot.failureAlert)
        update(\.machine, snapshot.machine)
        let windowsMoved = accountWindows != snapshot.accountWindows
        update(\.accountWindows, snapshot.accountWindows)
        if windowsMoved { accountWindowsChanged?(accountWindows) }
        return assigned
    }

    // MARK: - What the views ask of what is shown

    /// Why the last read did not answer, in Pitboard's own words.
    public var problem: String? { readFailure?.message }

    /// The row shown for the account with `id`.
    public func item(_ id: String) -> AccountItem? {
        sections.lazy.flatMap(\.accounts).first { $0.id == id }
    }
}

/// Hands each snapshot the model tells of to the main actor, in the order it is told.
///
/// The model tells its listener on a thread of its own, one call at a time, and the next
/// waits for this one to return, so this only queues the snapshot on the main queue, which
/// runs what it is given in the order it was given, and returns.
public final class ModelListening: ModelListener {
    /// Where each snapshot goes, on the main actor.
    @MainActor public var deliver: (@MainActor (Snapshot) -> Void)?

    public init() {}

    public func changed(snapshot: Snapshot) throws {
        DispatchQueue.main.async {
            MainActor.assumeIsolated { self.deliver?(snapshot) }
        }
    }
}
