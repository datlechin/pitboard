import AppKit
import Foundation
import PitboardKit

/// What the app shows, and the only place that calls the core. Every call runs off the main
/// thread inside `PitboardService`; this only holds the answers.
///
/// The menu, the window and the settings all read this one model, so an account reads the
/// same everywhere and a change made in one shows in the others at once.
@MainActor
@Observable
public final class AppModel {
    private let service: any Core
    private let appControl: any AppControl
    /// Every tool Pitboard handles, in the order a listing shows them.
    let tools: [Tool]
    private(set) var status: Status?
    /// What went wrong with the last read, when it did not answer. The numbers shown are
    /// then the last ones measured.
    private(set) var problem: String?
    /// The stable code behind `problem`, for deciding what to offer. Branching on the
    /// wording of a message is how an offer survives the message changing under it.
    private(set) var problemCode: String?
    /// The account a switch is running for, as its label with its tool, so its row can
    /// show it.
    private(set) var switching: String?
    /// A switch waiting for the person to let Pitboard quit an app first, because the app
    /// keeps the tool's login in memory and would go on with the account switched away
    /// from.
    private(set) var quitting: QuitToSwitch?
    /// How long an app has to quit once asked. Long enough for one that asks about work in
    /// progress to be answered; past it, nothing has changed and the switch is not made.
    var quitWithin: Duration = .seconds(30)
    /// How long what an app leaves running has to go once the app has quit, and how often
    /// to look. Claude's helpers outlive it for a moment, and the core touches none of
    /// Claude's files while any of them runs.
    var closeWithin: Duration = .seconds(5)
    var closeCheckEvery: Duration = .milliseconds(200)
    private(set) var updatedAt: Date?
    /// Whether a read is running, so the refresh buttons can say so. Reads overlap, a timer's
    /// with one somebody asked for, so they are counted rather than flagged: the first to end
    /// would otherwise say none is running while the other still is.
    var reading: Bool { readsInFlight > 0 }
    private var readsInFlight = 0
    /// Counts the changes this app has made or seen made. A read that started before one
    /// lands after it with who was signed in before, and would put away what the change said,
    /// so it is dropped: the read the change starts itself says what is true now.
    private var changesSeen = 0
    /// Accounts that have run out while another of the same tool has room, one per tool at
    /// most. Shown whether or not notifications are allowed, so the advice does not depend
    /// on a permission.
    private(set) var advice: [Advice] = []
    /// What each tool's last switch said that is still true, one per tool at most. Kept
    /// apart from `warnings`, which the read that follows every switch replaces: these are
    /// about the switch, and stay true until the person has done something about them.
    private(set) var lastSwitches: [LastSwitch] = []
    /// Everything that went wrong on the way, not only the first of them. A switch can warn
    /// about an overriding environment variable and a config that did not update at once,
    /// and showing one of those and dropping the other is how a person fixes the wrong
    /// thing.
    private(set) var warnings: [Warning] = []
    /// An interrupted switch nothing can finish, which the app offers a way out of.
    private(set) var stuck = false
    /// What giving up on an interrupted switch kept, until somebody has read it.
    private(set) var abandoned: Abandoned?
    /// A Claude Desktop account being added: the account that was in use is parked and
    /// Claude left signed out, waiting for somebody to sign in to another account in it.
    /// Kept by the core, so an app that quit halfway finds it again.
    private(set) var desktopAwaiting: Awaiting?
    /// Whether Claude was left closed for that sign-in: it was not open when the account was
    /// put aside, so Pitboard did not open it again, and somebody has to.
    private(set) var claudeLeftClosed = false
    /// Whether Claude Desktop's numbers are asked of claude.ai, and whether macOS still
    /// lets Pitboard read the key that takes. Nil until the first read.
    private(set) var liveUsage: LiveUsageState?
    /// The add waiting when the sheet for it was last up, so a read shows it once and not
    /// after every read somebody closed it after. An add this app started is kept here as
    /// it starts: its sheet is already up.
    private var resumedAwaiting: Int64?
    /// Whether Claude Desktop has been read since the app opened. Only that first read puts
    /// up the sheet for an add left halfway, the one this app was quit in the middle of. An
    /// add a later read finds was started somewhere else, a terminal maybe with nobody at
    /// the screen, and the window does not come forward for it uninvited: the menu and a
    /// notice offer to finish it.
    private var desktopReadSinceLaunch = false

    /// The sheet over the main window, when one is asked for.
    var sheet: AccountSheet? {
        didSet {
            // The sheet for another account offers the tools found, and the first answer
            // may have come while the login shell was too slow to say where they are.
            if case .add = sheet, oldValue == nil {
                Task { await askWhatIsInstalled() }
            }
        }
    }
    /// A sign-in in progress, and everything the tool has said about it.
    private(set) var signingIn: SigningIn?
    /// A failure of something asked for away from the window, for the window to say. Set
    /// together with a request for the window, since a menu has nowhere to put a sentence.
    var presentedFailure: ActionFailure?
    /// Counts the requests for the main window. The one view that is always there, the
    /// menu bar item, opens the window whenever this moves, so the model can ask for it
    /// from anywhere, a notification's button included.
    private(set) var windowRequests = 0
    /// The pane the last request for the window wants shown, when it wants one.
    private(set) var requestedPane: WindowPane?

    /// Everything about this machine rather than its accounts: the renewal schedule, the
    /// command line, opening at login, what doctor finds and what Pitboard has changed.
    let machine: MachineModel

    /// Where Pitboard keeps its own preferences, which views read through `@AppStorage`.
    let defaults: UserDefaults

    /// Told each read of who is enrolled that succeeded, the full one or the one a change on
    /// this machine starts: the account windows put away what a forgotten account's window
    /// kept. Each lists every enrolled account, from Pitboard's own index. A read that failed,
    /// or the last numbers shown in its place, says nothing about who was forgotten.
    @ObservationIgnored var afterRead: (@MainActor (Status) -> Void)?

    /// What a tool's last switch said that the read after it does not say again.
    ///
    /// One per tool. A switch of one tool says nothing about another's sessions, and a
    /// Claude Code switch used to put away the warning not to sign out inside a Codex
    /// session still using the account Codex had just parked.
    ///
    /// A sign-in that put a new login in use in place of the account's old one is kept here
    /// too: sessions already running are left on the old login exactly as a switch leaves
    /// them on the old account.
    struct LastSwitch: Equatable {
        /// The tool, as a `Tool`'s code.
        let provider: String
        /// The account switched to, as the core types it. While its tool still has it signed
        /// in, what the switch said is still true, whatever else has written the account
        /// index since: a renewal, an enrolment, a read that renewed a lapsed login.
        var to: String
        /// When sessions already open will have picked it up, for a tool that follows a
        /// switch by itself.
        var adopted: Date?
        /// For a tool whose running sessions never pick a switch up.
        var restart: Restart?
        /// What a change that was not a switch said it did.
        var said: String?
        /// The app to open for the switch to take, for a tool that reads its login as it
        /// starts and was not running to be opened again.
        var opens: String?
        var warnings: [Warning] = []

        /// What a switch means for a tool's running sessions, said only when the core did
        /// not count them. When it did, its own warning says the same with the count and
        /// with what not to do in them, and the same fact twice is once too many.
        var notice: String? {
            if let opens { return "Open \(opens) to use \(split(to).label)." }
            guard let restart,
                !warnings.contains(where: { $0.code == "sessions_still_running" })
            else { return nil }
            return restartNotice(program: restart.program, from: restart.from)
        }
    }

    /// A switch that waits for an app to be quit first.
    struct QuitToSwitch {
        /// The account to switch to, its label with its tool.
        let qualified: String
        /// The app, as the core names it from what it runs.
        let bundleID: String
        let name: String
    }

    /// The account a switch is running for, or waiting on an answer about quitting an app
    /// for. Nothing else is switched meanwhile, so the menu and the rows hold back.
    var switchUnderWay: String? { switching ?? quitting?.qualified }

    /// A tool's running sessions keep the account they started with.
    struct Restart: Equatable {
        /// The command a person quits and starts again.
        let program: String
        /// The account they keep using, by its label alone: the notice names the tool
        /// already, by its program.
        var from: String
    }

    /// Usage is asked of each tool's service for every account, so it is asked sparingly: on
    /// opening a menu when the numbers are a minute old, and in the background every five
    /// minutes.
    static let staleAfter: TimeInterval = 60
    private static let refreshEvery: Duration = .seconds(300)

    private let notifier: Notifier
    /// The codes of the tools whose program was found, once the first read has asked. Read
    /// then and when the sheet for another account opens, and not on every read: asking
    /// crossed into the core on every keystroke in the form that reads it. Asked from a read
    /// rather than here, because finding them can mean waiting on the person's login shell.
    private var installed: Set<String> = []
    /// Set before the answer arrives, so two reads at once ask once.
    private var askedWhatIsInstalled = false
    /// The tools somebody said "Not Now" to a second account for, by code.
    private(set) var secondAccountDeclined: Set<String> = []

    /// How often to ask whether anything on this machine has changed. One stat of one
    /// file, so it costs nothing to ask often; before it, a switch typed in a terminal
    /// left the menu bar naming the account the person had just stopped using for as long
    /// as five minutes, with a button offering a switch that had already happened.
    private static let noticeEvery: Duration = .seconds(2)
    /// Nil until something has looked. A machine with no account index reports 0, which
    /// is a real answer and not an absence.
    private var lastChangedAt: Int64?
    /// The same for the usage readings, which every session's status line records into.
    private var lastReadingsAt: Int64?

    public convenience init(dependencies: Dependencies) {
        self.init(
            watching: dependencies.watching, service: dependencies.core,
            defaults: dependencies.defaults, commandLineTool: dependencies.commandLineTool,
            loginItem: dependencies.loginItem, appControl: dependencies.appControl,
            notifies: dependencies.notifies)
    }

    /// `watching` starts what runs by itself: the periodic read, the wake notice, the read
    /// when a menu opens, the poll that notices a change made somewhere else, and the one
    /// repair of a schedule an older app wrote. A test drives those itself, and two of them
    /// firing under a test is how a test stops telling the truth about what set what.
    init(
        watching: Bool = true,
        service: any Core,
        defaults: UserDefaults = .standard,
        commandLineTool: CommandLineTool = CommandLineTool(),
        loginItem: any LoginItem = MainAppLoginItem(),
        // No default: a model made for a test must never quit an app on the machine
        // running it.
        appControl: any AppControl,
        notifies: Bool = false
    ) {
        self.service = service
        self.appControl = appControl
        tools = service.tools()
        self.defaults = defaults
        notifier = Notifier(delivering: notifies)
        var declined = Set(
            defaults.stringArray(forKey: DefaultsKey.secondAccountDeclined) ?? [])
        // Said before there was a second tool, so about the only tool there was.
        if defaults.bool(forKey: "hideSecondAccountNudge") {
            declined.insert("claude")
            defaults.set(declined.sorted(), forKey: DefaultsKey.secondAccountDeclined)
            defaults.removeObject(forKey: "hideSecondAccountNudge")
        }
        secondAccountDeclined = declined
        machine = MachineModel(
            service: service, commandLineTool: commandLineTool, loginItem: loginItem)
        notifier.start()
        notifier.onSwitch = { [weak self] label in
            Task { await self?.switchAsked(to: label) }
        }
        notifier.onLiveUsage = { [weak self] in self?.liveUsageAsked() }
        machine.renewed = { [weak self] in await self?.refresh(asked: true) }
        guard watching else { return }
        // Once per launch: after that the schedule runs a command line, or was never the
        // app's to repair. On the service's queue once the core is made, like every call.
        Task { [weak machine] in await machine?.repairSchedule() }
        Task { [weak self] in
            while !Task.isCancelled {
                await self?.refresh()
                try? await Task.sleep(for: Self.refreshEvery, tolerance: .seconds(60))
            }
        }
        // Numbers read before the machine slept say nothing about now.
        Task { [weak self] in
            let woke = NSWorkspace.shared.notificationCenter.notifications(
                named: NSWorkspace.didWakeNotification)
            for await _ in woke {
                await self?.refresh()
            }
        }
        // A menu of this app opening is somebody looking at what it says, and the menu bar
        // item's menu is the glance the whole app exists for. SwiftUI says nothing when a
        // menu bar extra's menu opens, and AppKit says it of every menu, so a top-level one
        // is taken to be it. A menu that is open shows the read as soon as it lands.
        Task { [weak self] in
            let opened = NotificationCenter.default.notifications(
                named: NSMenu.didBeginTrackingNotification)
            for await note in opened {
                guard (note.object as? NSMenu)?.supermenu == nil else { continue }
                await self?.refresh(ifOlderThan: Self.staleAfter)
            }
        }
        // Somebody else on this machine changing something. Reading it costs no network
        // and no keychain, so it can follow a terminal switch within a second or two.
        Task { [weak self] in
            while !Task.isCancelled {
                await self?.noticeOtherChanges()
                try? await Task.sleep(for: Self.noticeEvery, tolerance: .seconds(1))
            }
        }
    }

    #if DEBUG
        /// The change poll, for a test that must not wait two seconds for a timer.
        func noticeOtherChangesForTesting() async { await noticeOtherChanges() }
        /// What has been told about, for a test that must see a run-out told once.
        var toldForTesting: [String: Int64] { notifier.told }
        /// How often live usage was said to be paused, for a test that must see it once.
        var liveUsagePausedToldForTesting: Int { notifier.liveUsagePausedTold }
    #endif

    // MARK: - Reading

    /// `asked` means somebody asked for this reading rather than a timer producing it, and
    /// is what tells the core to ask each service again whatever it read moments ago. The
    /// first read also asks which tools are installed, for the sheet for a new account, and
    /// each read asks again while none has been found: the service asks a login shell that
    /// was too slow to answer once more, and finding none is what says to install a tool.
    func refresh(ifOlderThan seconds: TimeInterval = 0, asked: Bool = false) async {
        if !askedWhatIsInstalled || installed.isEmpty {
            askedWhatIsInstalled = true
            await askWhatIsInstalled()
        }
        if let updatedAt, Date().timeIntervalSince(updatedAt) < seconds { return }
        readsInFlight += 1
        defer { readsInFlight -= 1 }
        let started = changesSeen
        // As they stood before the read, because a session can record newer numbers, and a
        // terminal can switch, while the read waits on a service, and the read then shows
        // nothing of either. Taken after, they counted as seen though nothing had shown them.
        // What the read writes itself costs the next look one read of what is known.
        let readingsBefore = await service.readingsChangedAt()
        let changedBefore = await service.changedAt()
        do {
            let read = try await service.status(fresh: asked)
            // After the read, which is when the core reads Claude's key and can find macOS
            // no longer lets it: read before, that is said one read late.
            await readDesktop()
            guard changesSeen == started else { return }
            status = read
            forgetSwitchesUndone(by: read)
            warnings = read.warnings
            problem = nil
            problemCode = nil
            stuck = read.warnings.contains { $0.code == "recovery_undetermined" }
            updatedAt = Date()
            lastChangedAt = changedBefore
            lastReadingsAt = readingsBefore
            advise(from: read)
            afterRead?(read)
        } catch {
            await readDesktop()
            guard changesSeen == started else { return }
            problem = Self.saying(error)
            problemCode = Self.code(of: error)
            // What went wrong this time, in place of what was wrong last time. A failure
            // carries its own warnings, and leaving the previous read's in place showed a
            // fresh network error above warnings that may have been fixed since.
            warnings = Self.warnings(of: error)
            // A read that could not reach a service still has something true to show: the
            // last numbers measured, and who each tool's own files say is signed in. An
            // empty list says the accounts are gone, which is not what happened.
            if status == nil, let known = try? await service.statusOffline() {
                status = known
            }
            stuck = Self.code(of: error) == "recovery_undetermined"
        }
    }

    /// Has anything on this machine changed since the last look. Reads only what is already
    /// known: no network, no keychain, and no request of any service.
    ///
    /// Two things are looked at, because they mean different things. The account index
    /// changing can be a switch made somewhere else, so who is signed in is read again. The
    /// readings changing is only numbers, newer ones a session or the command line has seen,
    /// so only the numbers are taken. Read again every time, who is signed in would come from
    /// each tool's own files several times a minute, and a switch that could not update
    /// Claude Code's config leaves it naming the account before.
    private func noticeOtherChanges() async {
        let changed = await service.changedAt()
        let measured = await service.readingsChangedAt()
        // The first look only records where things stand; there is nothing to compare to.
        guard let seen = lastChangedAt, let seenReadings = lastReadingsAt else {
            lastChangedAt = changed
            lastReadingsAt = measured
            return
        }
        // A switch this app has in flight is its own change and not somebody else's, and
        // taking it for one put away what the switch had just said. What changed meanwhile
        // stays unseen until it is shown: a switch that fails reads nothing after it, and one
        // that failed after finishing an interrupted switch has still moved who is signed in.
        guard switching == nil else { return }
        lastChangedAt = changed
        if seen != changed {
            changesSeen += 1
            await readDesktop()
            guard let read = try? await service.statusOffline() else { return }
            status = read
            lastReadingsAt = measured
            forgetSwitchesUndone(by: read)
            advise(from: read)
            afterRead?(read)
        } else if seenReadings != measured, let shown = status,
            let read = try? await service.statusOffline()
        {
            let overlaid = numbers(of: read, onto: shown)
            status = overlaid
            lastReadingsAt = measured
            advise(from: overlaid)
        }
    }

    /// Advice about a read, the app's own or numbers taken from what is recorded. What is new
    /// is told, and what was said before stays for as long as the numbers bear it out.
    /// Advice worked out afresh leaves out what has been told, so it was put away at the next
    /// read, seconds after it was said once numbers moved with every session's response,
    /// with the account still out. Still one per tool, the newer first.
    private func advise(from read: Status) {
        let new = Advice.about(read, tools: tools, unless: notifier.told)
        new.forEach(notifier.tell)
        let standing = new + advice.compactMap { $0.renewed(in: read, tools: tools) }
        advice = inOrder(standing.map(\.provider), by: tools).compactMap { provider in
            standing.first { $0.provider == provider }
        }
    }

    /// Which tools the service found a program for. It asks the login shell once more where
    /// that was too slow to answer before, so a later answer can find more than the first.
    private func askWhatIsInstalled() async {
        installed = Set(await service.installed().map(\.code))
    }

    // MARK: - Asking for the window

    /// Opens the main window, from anywhere: the menu, a notification, the model itself,
    /// on `pane` when it matters which.
    func showWindow(_ pane: WindowPane? = nil) {
        requestedPane = pane
        windowRequests += 1
    }

    /// Puts `sheet` over the main window, opening it first on the accounts it is about. A
    /// sign-in that is running keeps its sheet: replacing it would leave the tool's sign-in
    /// running with nothing on screen to finish or stop it.
    ///
    /// Claude left closed by an add is looked at again: somebody may have opened it since,
    /// and the sheet would offer to open it.
    func present(_ sheet: AccountSheet) {
        if signingIn == nil {
            self.sheet = sheet
        }
        if claudeLeftClosed, appControl.running(Self.claudeApp.bundleID) != nil {
            claudeLeftClosed = false
        }
        showWindow(.accounts)
    }

    /// Says a failure of something asked for away from the window, in the window.
    func present(_ failure: ActionFailure?) {
        guard let failure else { return }
        presentedFailure = failure
        showWindow()
    }

    /// A switch asked for from the window, the menu or a notification, with any failure said
    /// in the window.
    ///
    /// Where an app runs the tool with its login in memory, as ChatGPT runs Codex, the switch
    /// waits for the person to let Pitboard quit it first: switched under it, the app would go
    /// on with the account left behind, and its own sign-out would revoke the login Pitboard
    /// has just parked.
    func switchAsked(to qualified: String) async {
        // One switch at a time: the second would wait behind the first anyway, and its
        // choice was made from a menu that did not yet show the first. A question waiting
        // to be answered comes back to the front rather than being left behind unseen.
        guard switchUnderWay == nil else {
            if quitting != nil { showWindow(.accounts) }
            return
        }
        // Claimed before anything is awaited, so a second request made meanwhile waits too.
        switching = qualified
        if let app = await appHolding(split(qualified).provider) {
            switching = nil
            quitting = QuitToSwitch(
                qualified: qualified, bundleID: app.bundleID, name: app.name)
            showWindow(.accounts)
            return
        }
        present(await use(qualified))
    }

    /// Quits the app, switches, and opens the same copy of the app again: Pitboard closed it,
    /// so Pitboard opens it, whether or not the switch worked, leaving the person where they
    /// were, unless the switch stopped partway. An app that is already gone is not opened.
    /// One that does not quit, because it was busy or its person said no, stops everything
    /// before anything has changed.
    ///
    /// Takes the switch it was asked about rather than reading `quitting`: the alert that
    /// asks is gone, and has said so, before this runs.
    func quitAndSwitch(_ pending: QuitToSwitch) async {
        quitting = nil
        switching = pending.qualified
        defer { switching = nil }
        switch await appControl.quit(pending.bundleID, within: quitWithin) {
        case .stillRunning:
            present(
                ActionFailure(
                    "Couldn’t switch to \(split(pending.qualified).label)",
                    message: "\(pending.name) is still open, so nothing has changed. Quit it, "
                        + "then switch again."))
        case .notRunning:
            present(await use(pending.qualified))
        case .quit(let copy):
            if split(pending.qualified).provider == desktopProvider,
                !(await closed(pending.bundleID, of: desktopProvider))
            {
                appControl.open(copy)
                present(
                    Self.stillClosing(
                        "Couldn’t switch to \(split(pending.qualified).label)", pending.name))
                return
            }
            // Opened as soon as the switch is made, not after the read that follows it,
            // which can wait on the network.
            let failure = await switchWithoutReading(to: pending.qualified, reopening: true)
            if !Self.leftUnfinished(failure) { appControl.open(copy) }
            if failure == nil { await refresh() }
            present(failure)
        }
    }

    /// Whether everything that runs from the app `bundleID` and holds `provider`'s login has
    /// gone, looked at until it has or `closeWithin` passes. Only asked once Pitboard has
    /// quit the app: its helpers can outlive it for a moment, and Claude Desktop's files are
    /// not touched while any runs.
    private func closed(_ bundleID: String, of provider: String) async -> Bool {
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: closeWithin)
        while true {
            let held = await service.holding(provider).contains { held in
                if case .reopenApp(let id, _) = held.remedy { return id == bundleID }
                return false
            }
            guard held else { return true }
            guard clock.now < deadline else { return false }
            try? await Task.sleep(for: closeCheckEvery)
        }
    }

    /// An app that quit but left something of itself running past `closeWithin`. Nothing
    /// has changed, and the app is opened again: Pitboard quit it.
    nonisolated static func stillClosing(_ title: String, _ name: String) -> ActionFailure {
        ActionFailure(
            title,
            message: "\(name) took too long to finish closing, so nothing has changed. Try "
                + "again in a moment.",
            code: "app_still_open")
    }

    /// The question about quitting an app is gone: answered, or cancelled. An answer to quit
    /// has already passed its switch on.
    func closeQuitQuestion() {
        quitting = nil
    }

    /// An app on this Mac running `provider`'s tool with its login in memory, that Pitboard
    /// may quit and open again. Asked of the core, which reads the process list, and only
    /// taken where the app is running: what is left of one that has gone cannot be quit.
    private func appHolding(_ provider: String) async -> (bundleID: String, name: String)? {
        for held in await service.holding(provider) {
            if case .reopenApp(let bundleID, let name) = held.remedy,
                appControl.running(bundleID) != nil
            {
                return (bundleID, name)
            }
        }
        return nil
    }

    // MARK: - What the app shows

    /// The panel's accounts, a section per tool once there is more than one.
    var groups: [AccountGroup] { grouped(status?.accounts ?? [], by: tools) }

    /// Whether accounts of more than one tool are shown, which is when anything says which
    /// tool an account is for. A machine with one tool looks exactly as it did before there
    /// were two.
    var showsTools: Bool { Set(status?.accounts.map(\.provider) ?? []).count > 1 }

    func tool(_ code: String) -> Tool? { tools.first { $0.code == code } }

    /// The account in use and its tightest limit, as the menu bar reads it.
    var title: String { menuTitle(for: status, order: tools) }

    /// The menu bar as VoiceOver says it. Once more than one tool is shown it names the tool
    /// too: the bar follows whichever account is closest to running out, and two tools can
    /// each have a `work`.
    var spokenTitle: String {
        guard !title.isEmpty else { return "Pitboard" }
        guard showsTools, let account = titled(status?.accounts ?? [], order: tools),
            let tool = tool(account.provider)
        else { return "Pitboard, \(title)" }
        return "Pitboard, \(title), \(tool.name)"
    }

    /// The tools a new account can be added for: each whose program was found or that
    /// already has an account here, and Claude Code, the tool a bare label means, when that
    /// is none of them.
    ///
    /// Not every tool then. A program the app did not find where it looks is almost never
    /// on the `PATH` of an app opened from Finder either, so that offered sign-ins that could
    /// not start, and it asked somebody who only ever had Claude Code about Codex too.
    var addable: [Tool] {
        let known = Set(status?.accounts.map(\.provider) ?? [])
        let some = tools.filter { installed.contains($0.code) || known.contains($0.code) }
        return some.isEmpty ? tools.filter { $0.code == defaultProvider } : some
    }

    /// Why a tool is missing from the sheet for a new account, rather than leaving it out
    /// without a word. Nil when every tool is offered.
    var notOffered: String? {
        let missing = tools.filter { !addable.contains($0) }
        guard !missing.isEmpty else { return nil }
        let names = missing.map(\.name).joined(separator: " and ")
        let programs = missing.map(\.program).joined(separator: " or ")
        return "\(names) \(missing.count == 1 ? "is" : "are") not offered: Pitboard did not "
            + "find \(programs) on this Mac."
    }

    /// The tool the sheet for `sheet` starts on.
    func provider(for sheet: AccountSheet) -> String {
        switch sheet {
        case .add(let code): code ?? addable.first?.code ?? defaultProvider
        case .signInAgain(let code, _), .name(let code, _), .rename(let code, _): code
        case .liveUsage: desktopProvider
        }
    }

    /// Who is asked about the accounts shown, as a sentence names them: "Anthropic", or
    /// "Anthropic or OpenAI". Before there are accounts, whoever could be.
    var services: String {
        let shown = Set(status?.accounts.map(\.provider) ?? [])
        let asked = shown.isEmpty ? addable : tools.filter { shown.contains($0.code) }
        return asked.map(\.service).joined(separator: " or ")
    }

    /// An account named where nothing around it says which tool it is for: its label, and
    /// once more than one tool is shown, its tool.
    func name(of account: Account) -> String {
        let label = accountName(label: account.label, email: account.email)
        guard showsTools else { return label }
        return "\(label) (\(tool(account.provider)?.name ?? account.provider))"
    }

    /// The warnings said beside `problem`: every one but the one it already says. A
    /// failure's message is not one of its warnings, and dropping the first of them hid one
    /// that was.
    var otherWarnings: [Warning] {
        warnings.filter { $0.message != problem }
    }

    /// What `last` warned about that the read after it did not already show, so nothing is
    /// said twice.
    func warnings(after last: LastSwitch) -> [Warning] {
        last.warnings.filter { !warnings.contains($0) }
    }

    /// Logins signed in to a tool and not enrolled: the ones the app can name by itself.
    /// A login Pitboard could not read or cannot switch is not one of them, however it
    /// looks: naming it would enrol something that can never be switched to.
    var unnamed: [Account] {
        status?.accounts.filter { $0.signedIn && $0.label == nil && !$0.unplaced } ?? []
    }

    /// A login signed in now that is not enrolled, in any tool.
    var unenrolled: Bool { !unnamed.isEmpty }

    /// How far along setting Pitboard up this machine is.
    ///
    /// Somebody who installed only the app has never typed a Pitboard command and may never
    /// want to. Every state before `ready` used to show either a line naming a command to run
    /// or nothing at all, which is the same as telling them the app does not work.
    enum Footing: Equatable {
        /// No tool Pitboard works with was found on this machine, and none has an account
        /// or a login here. Nothing Pitboard does means anything without a tool, and
        /// Pitboard cannot install one. Claude Code is the tool it names.
        case noClaudeCode
        /// No tool has anybody signed in, and nothing is enrolled.
        case noOneSignedIn
        /// Somebody is signed in to a tool and Pitboard has not been told what to call them.
        /// Their login cannot be parked until it has a name. The tool's code, and the email.
        case unnamed(provider: String, email: String)
        /// A tool has one account, so there is nothing yet to switch to in it, and nobody
        /// has said they keep it that way on purpose. The tool's code, and the account's
        /// label.
        case onlyOne(provider: String, label: String)
        /// Set up, or too early to say.
        case ready
    }

    /// Worked out across every tool: somebody signed in to Codex is not somebody nobody is
    /// signed in to, and a Claude Code account beside a Codex one still has nothing to
    /// switch to.
    var footing: Footing {
        // Before the first read there is nothing to go on, and guessing at this point
        // shows somebody a setup step they may have finished years ago.
        guard let accounts = status?.accounts else { return .ready }
        // Said only where nothing else is: a login or an account means a tool is here
        // however it was installed, and a program the app could not find may still be.
        // The core's read does not fail for a missing tool, so this is the one place it
        // shows. It used to be told by a failure no read gives, and was never shown.
        if accounts.isEmpty, askedWhatIsInstalled, installed.isEmpty {
            return .noClaudeCode
        }
        guard accounts.contains(where: \.signedIn) else {
            // Enrolled accounts with nobody signed in is a machine mid-switch or one whose
            // login was signed out from elsewhere, not a machine that needs setting up.
            return accounts.isEmpty ? .noOneSignedIn : .ready
        }
        if let login = unnamed.first {
            return .unnamed(provider: login.provider, email: login.email)
        }
        // Per tool: an account can only be switched to another account of its own tool.
        for provider in inOrder(accounts.map(\.provider), by: tools)
        where !secondAccountDeclined.contains(provider) {
            let enrolled = accounts.filter { $0.provider == provider && $0.label != nil }
            if enrolled.count == 1, let only = enrolled.first, only.signedIn,
                let label = only.label
            {
                return .onlyOne(provider: provider, label: label)
            }
        }
        return .ready
    }

    /// Somebody keeps one account of `provider`'s tool on purpose. Per tool: that says
    /// nothing about another tool, and one flag for every tool hid the prompt for a tool
    /// nobody had been asked about.
    func declineSecondAccount(for provider: String) {
        secondAccountDeclined.insert(provider)
        defaults.set(secondAccountDeclined.sorted(), forKey: DefaultsKey.secondAccountDeclined)
    }

    // MARK: - Errors

    /// Pitboard's errors already say what to do, so they are shown as they are.
    nonisolated static func saying(_ error: Error) -> String {
        if case PitboardError.Failed(_, _, let message, _) = error {
            return message
        }
        return error.localizedDescription
    }

    /// Everything a failure warns about, not only what stopped it. A switch can fail and
    /// still have something to say about an overriding environment variable.
    nonisolated static func warnings(of error: Error) -> [Warning] {
        if case PitboardError.Failed(_, _, _, let warnings) = error {
            return warnings
        }
        return []
    }

    /// The stable code behind an error, for deciding what to offer rather than reading the
    /// wording of a message.
    nonisolated static func code(of error: Error) -> String? {
        if case PitboardError.Failed(let code, _, _, _) = error {
            return code
        }
        return nil
    }

    /// A failure's warnings said beside the ones already shown, first, with nothing the last
    /// read found dropped to make room for them.
    func keep(_ failure: ActionFailure) {
        warnings = failure.warnings + warnings.filter { !failure.warnings.contains($0) }
    }

    /// In place of what the same tool's last switch said, or after the others.
    private func remember(_ said: LastSwitch) {
        if let at = lastSwitches.firstIndex(where: { $0.provider == said.provider }) {
            lastSwitches[at] = said
        } else {
            lastSwitches.append(said)
        }
    }

    /// Puts away what a tool's last switch said, once somebody has read it.
    func forgetSwitch(of provider: String) {
        lastSwitches.removeAll { $0.provider == provider }
    }

    /// Puts away what a switch said once its tool no longer has the account it switched to
    /// signed in: a switch made somewhere else, or a sign-out. Anything else that writes the
    /// account index leaves the sessions it describes exactly as they were, and taking every
    /// write for a switch put away the one warning that keeps somebody from revoking the
    /// login a Codex switch had just parked.
    private func forgetSwitchesUndone(by read: Status) {
        lastSwitches.removeAll { last in
            !read.accounts.contains {
                $0.provider == last.provider && $0.signedIn && typed($0) == last.to
            }
        }
    }
}

/// `shown` with each account's numbers as `read` has them, and everything else as it was. An
/// account `read` has no numbers for keeps its own.
func numbers(of read: Status, onto shown: Status) -> Status {
    let measured = Dictionary(
        read.accounts.compactMap { account in account.usage.map { (account.id, $0) } },
        uniquingKeysWith: { first, _ in first })
    return Status(
        now: shown.now,
        accounts: shown.accounts.map { account in
            guard let usage = measured[account.id] else { return account }
            return Account(
                id: account.id, provider: account.provider, label: account.label,
                qualified: account.qualified, unplaced: account.unplaced, email: account.email,
                accountUuid: account.accountUuid, signedIn: account.signedIn,
                switchable: account.switchable, parked: account.parked, usage: usage,
                stale: account.stale, staleExplanation: account.staleExplanation,
                lastsSeconds: account.lastsSeconds, lastsBurning: account.lastsBurning)
        },
        warnings: shown.warnings)
}

// MARK: - Changing accounts

extension AppModel {
    /// Switches to the account `qualified` names, which is its label with its tool: two
    /// tools can each have a `work`, and a bare one then names neither.
    ///
    /// What the switch means for sessions already running depends on the tool. One that
    /// follows by itself gets a countdown; one that never does gets said so, since a
    /// countdown there would promise something that is not going to happen.
    @discardableResult
    func use(_ qualified: String) async -> ActionFailure? {
        switching = qualified
        defer { switching = nil }
        let failure = await switchWithoutReading(to: qualified, reopening: false)
        if failure == nil { await refresh() }
        return failure
    }

    /// The switch itself, and what it says, without the read that follows it. `reopening`
    /// says Pitboard quit the tool's app for it and opens it again, so there is nothing to
    /// tell anybody to open.
    private func switchWithoutReading(
        to qualified: String, reopening: Bool
    ) async -> ActionFailure? {
        do {
            let done = try await service.switchTo(qualified)
            changesSeen += 1
            switch done.outcome {
            case .switched(let provider, let from, let to, let adoption):
                var said = LastSwitch(provider: provider, to: to, warnings: done.warnings)
                switch adoption {
                case .follows(let within):
                    said.adopted = Date().addingTimeInterval(TimeInterval(within))
                case .restart(let program):
                    said.restart = Restart(program: program, from: split(from).label)
                case .nextLaunch(let program):
                    // Claude Desktop reads its sign-in as it starts. Quit for the switch, it
                    // is opened again on the new account and there is nothing to say; not
                    // running, it has to be opened for the switch to mean anything.
                    said.opens = reopening ? nil : program
                }
                remember(said)
            case .alreadyActive(let label):
                // Nothing moved, so what this tool's last switch said still stands, and
                // anything this one warned about is said beside it.
                guard !done.warnings.isEmpty else { break }
                let provider = split(qualified).provider
                var said =
                    lastSwitches.first { $0.provider == provider && $0.to == label }
                    ?? LastSwitch(provider: provider, to: label)
                said.warnings += done.warnings.filter { !said.warnings.contains($0) }
                remember(said)
            }
            // Advice about this tool is about the account it has just left. Another tool's
            // stays: it is as true as it was, and it is never told again.
            let provider = split(qualified).provider
            advice.removeAll { $0.provider == provider }
            updatedAt = nil
            return nil
        } catch {
            // Nothing moved here either, so what the last switch said stands.
            let title = "Couldn’t switch to \(split(qualified).label)"
            let failure =
                split(qualified).provider == desktopProvider
                ? Self.desktopFailure(title, error)
                : ActionFailure(title, error: error)
            keep(failure)
            return failure
        }
    }

    /// Records the login signed in now to `provider`'s tool under a name, with the tool's
    /// prefix, so a Codex login is enrolled as Codex's and not as a Claude Code account.
    /// The sheet stays open with the name in it when this fails, and closes when it works,
    /// if it is still the sheet showing.
    @discardableResult
    func enrol(_ name: String, for provider: String) async -> ActionFailure? {
        if provider == desktopProvider { return await enrolDesktop(name) }
        do {
            _ = try await service.enrollCurrent(qualified(name, for: provider))
            changesSeen += 1
            if case .name(provider, _)? = sheet { sheet = nil }
            updatedAt = nil
            await refresh()
            return nil
        } catch {
            return ActionFailure("Couldn’t name this account", error: error)
        }
    }

    /// Gives an enrolled account a new name. A rename stays inside the account's tool, so
    /// the new name is given bare.
    ///
    /// Everything said about the account is said about it under its new name: what its
    /// tool's last switch said, and advice about it running out. Keyed by the old name, the
    /// read after the rename would take the switch for undone and the advice for new, and
    /// tell it again.
    @discardableResult
    func rename(_ label: String, of provider: String, to name: String) async -> ActionFailure? {
        do {
            _ = try await service.rename(qualified(label, for: provider), to: name)
            changesSeen += 1
            carry(label, of: provider, to: name)
            if sheet == .rename(provider: provider, label: label) { sheet = nil }
            updatedAt = nil
            await refresh()
            return nil
        } catch {
            return ActionFailure("Couldn’t rename \(label)", error: error)
        }
    }

    /// What was said about `label` of `provider`, said about it as `name`.
    private func carry(_ label: String, of provider: String, to name: String) {
        let typedOld = provider == defaultProvider ? label : qualified(label, for: provider)
        let typedNew = provider == defaultProvider ? name : qualified(name, for: provider)
        for index in lastSwitches.indices where lastSwitches[index].provider == provider {
            if lastSwitches[index].to == typedOld { lastSwitches[index].to = typedNew }
            if lastSwitches[index].restart?.from == label {
                lastSwitches[index].restart?.from = name
            }
        }
        advice = advice.map { $0.provider == provider ? $0.renaming(label, to: name) : $0 }
        notifier.rename(label, of: provider, to: name)
    }

    /// Drops the account `qualified` names, and the login parked for it.
    @discardableResult
    func forget(_ qualified: String) async -> ActionFailure? {
        do {
            _ = try await service.forget(qualified)
            changesSeen += 1
            updatedAt = nil
            await refresh()
            return nil
        } catch {
            return ActionFailure("Couldn’t forget \(split(qualified).label)", error: error)
        }
    }

    /// Give up on an interrupted switch that cannot be finished, keeping every login. The
    /// way out when recovery cannot reach the tool's service, which used to mean opening a
    /// terminal.
    @discardableResult
    func abandonStuckSwitch() async -> ActionFailure? {
        do {
            abandoned = try await service.abandonRecovery()
            changesSeen += 1
            stuck = false
            await refresh(asked: true)
            return nil
        } catch {
            return ActionFailure("Couldn’t give up on the interrupted switch", error: error)
        }
    }

    /// Puts away what giving up on an interrupted switch said, once somebody has read it.
    func forgetAbandoned() {
        abandoned = nil
    }

    /// Runs the tool's own sign-in and shows what it says, then records what it signed in to.
    /// Both tools open the browser themselves and finish through a loopback callback, so
    /// there is nothing to hand a terminal. Claude Code's also reads a code typed back, from
    /// the start and whether or not the callback is reached: the page at the address it
    /// prints shows one once somebody signs in, and the code field is for it. Codex's prints
    /// an address and reads nothing.
    ///
    /// Returns once the sign-in has finished, failed or been cancelled. The sheet that
    /// started it closes when it finishes, and says the failure when it fails. A cancelled
    /// sign-in is not a failure: somebody asked for it to stop.
    @discardableResult
    func signIn(_ name: String, for provider: String) async -> ActionFailure? {
        guard signingIn == nil else { return nil }
        let named = tool(provider)?.name ?? provider
        let shown = SigningIn(label: name, provider: provider, tool: named, from: sheet)
        signingIn = shown
        let title = "Couldn’t sign in to \(name)"
        do {
            let session = try await service.signIn(qualified(name, for: provider))
            // Cancelled while it was starting: stop what started rather than watch it.
            guard signingIn === shown else {
                SignInCalls.run { session.cancel() }
                return nil
            }
            shown.session = session
            return await watch(session, shown: shown, for: provider, failing: title)
        } catch {
            guard signingIn === shown else { return nil }
            signingIn = nil
            let failure = ActionFailure(title, error: error)
            keep(failure)
            return failure
        }
    }

    /// Reads what the tool says until it stops, then records what it signed in to.
    private func watch(
        _ session: SignIn, shown: SigningIn, for provider: String, failing title: String
    ) async -> ActionFailure? {
        while let said = await SignInCalls.value({ session.nextLine() }) {
            shown.add(said)
        }
        // Cancelled. The tool was stopped because somebody asked, so the sign-in that is
        // "no longer running" is not something that went wrong, and nothing is enrolled.
        guard signingIn === shown else { return nil }
        do {
            let done = try await SignInCalls.value({ try session.finish() })
            changesSeen += 1
            // Cancelled while it was being finished, too late to stop the tool: what it
            // signed in to is enrolled all the same, so it is read, and nothing else is
            // touched. A sheet or a sign-in started since is somebody else's.
            guard signingIn === shown else {
                updatedAt = nil
                await refresh()
                return nil
            }
            signingIn = nil
            if sheet == shown.from { sheet = nil }
            let said = enrolled(done, as: shown.label, for: provider)
            updatedAt = nil
            await refresh()
            // Said after the read that follows, which would otherwise put it away.
            warnings += said.filter { !warnings.contains($0) }
            return nil
        } catch {
            guard signingIn === shown else { return nil }
            signingIn = nil
            let failure = ActionFailure(title, error: error)
            keep(failure)
            return failure
        }
    }

    /// What a finished sign-in says beyond the row it adds, and what it warned about that is
    /// to be shown beside the read that follows.
    ///
    /// Signing in to the account in use puts its new login in use at once, and what that
    /// means for sessions already running is kept the way a switch's is. The tool did not
    /// switch, so what its last switch said stays true and stays with it; a count of the same
    /// sessions naming this account's old login, beside one naming the account the switch
    /// left, would contradict it.
    private func enrolled(
        _ done: Enrolled, as name: String, for provider: String
    ) -> [Warning] {
        guard case .inUse(let again) = done.outcome else { return done.warnings }
        let to = provider == defaultProvider ? name : qualified(name, for: provider)
        var said =
            lastSwitches.first { $0.provider == provider && $0.to == to }
            ?? LastSwitch(provider: provider, to: to)
        said.said =
            again
            ? "Signed in to \(name) again. Its new login is the one in use now."
            : "Enrolled \(name), the account signed in now. Its new login is the one in use."
        let counted = said.warnings.contains { $0.code == "sessions_still_running" }
        said.warnings += done.warnings.filter {
            !said.warnings.contains($0) && !(counted && $0.code == "sessions_keep_old_login")
        }
        remember(said)
        return []
    }

    /// Types back the code the browser showed after signing in. Off the main thread, like
    /// everything that waits on the tool.
    func paste(_ code: String) {
        guard let shown = signingIn, let session = shown.session else { return }
        shown.pasted = true
        SignInCalls.run { try? session.paste(line: code) }
    }

    /// Stops the sign-in, off the main thread: stopping waits for the tool to exit, and a
    /// Codex sign-in waiting on the browser once kept the whole app waiting with it.
    func cancelSignIn() {
        let session = signingIn?.session
        signingIn = nil
        guard let session else { return }
        SignInCalls.run { session.cancel() }
    }
}

/// Where a sign-in's calls run. Each can block for as long as the tool waits on a person in
/// a browser, so none runs on the main thread or on the threads Swift's tasks share, which
/// a few waiting sign-ins would use up: they get a queue of their own, which makes threads
/// as it needs them.
enum SignInCalls {
    private static let queue = DispatchQueue(
        label: "com.usepitboard.signin.calls", qos: .userInitiated, attributes: .concurrent)

    /// Runs `work` there and waits for its answer without holding a thread meanwhile.
    static func value<T: Sendable>(_ work: @escaping @Sendable () throws -> T) async throws -> T
    {
        try await withCheckedThrowingContinuation { done in
            queue.async { done.resume(with: Result(catching: work)) }
        }
    }

    static func value<T: Sendable>(_ work: @escaping @Sendable () -> T) async -> T {
        await withCheckedContinuation { done in
            queue.async { done.resume(returning: work()) }
        }
    }

    /// Runs `work` there and does not wait for it.
    static func run(_ work: @escaping @Sendable () -> Void) {
        queue.async(execute: work)
    }
}

/// A sign-in as the app sees it: the name it will be enrolled under, the tool it is for,
/// what the tool has said so far, and the session to type back to.
@MainActor
@Observable
final class SigningIn {
    let label: String
    /// Which tool it is for, as a `Tool`'s `code`.
    let provider: String
    /// The tool's name, as the app says it.
    let tool: String
    private(set) var said = ""
    var pasted = false
    @ObservationIgnored var session: SignIn?
    /// The sheet it was started from, which is the one to close when it finishes.
    let from: AccountSheet?

    init(label: String, provider: String, tool: String, from: AccountSheet? = nil) {
        self.label = label
        self.provider = provider
        self.tool = tool
        self.from = from
    }

    func add(_ text: String) {
        said += text
    }

    /// What the tool has said comes to, read by the core in that tool's own words, which
    /// its register holds: the Windows app reads it the same way.
    private var view: SignInView {
        signInView(provider: provider, said: said, pasted: pasted)
    }

    /// The address the tool printed, for a browser that did not open by itself.
    var url: URL? { view.url.flatMap { URL(string: $0) } }

    /// Whether to offer a field for the code the browser shows, which the tool is waiting
    /// to have typed back.
    var wantsCode: Bool { view.wantsCode }
}

// MARK: - Claude Desktop

extension AppModel {
    /// Claude as the core names it, when nothing says otherwise: the core names the app only
    /// while it can see it running.
    private static let claudeApp = (bundleID: "com.anthropic.claudefordesktop", name: "Claude")

    /// Whether Settings has anything to say about Claude Desktop: it is installed here, or
    /// has an account here.
    var desktopShown: Bool {
        installed.contains(desktopProvider)
            || status?.accounts.contains { $0.provider == desktopProvider } == true
    }

    /// The Claude app Pitboard quits around a change to Claude Desktop's sign-in.
    private func desktopApp() async -> (bundleID: String, name: String) {
        for held in await service.holding(desktopProvider) {
            if case .reopenApp(let bundleID, let name) = held.remedy {
                return (bundleID, name)
            }
        }
        return Self.claudeApp
    }

    /// Quits `app` the way Command-Q does, waits for its helpers to go, runs `body`, and
    /// opens the same copy again if Pitboard quit it, whether or not `body` worked, leaving
    /// the person where they were, unless `body` stopped partway. `body` is told whether the
    /// app is being opened again. An
    /// app that does not quit stops everything before anything has changed, and is not
    /// opened: it never closed. Helpers that do not go stop everything too, and the app is
    /// opened again.
    private func quitThen(
        _ app: (bundleID: String, name: String), failing title: String,
        _ body: (Bool) async -> ActionFailure?
    ) async -> ActionFailure? {
        switch await appControl.quit(app.bundleID, within: quitWithin) {
        case .stillRunning:
            return ActionFailure(
                title,
                message: "\(app.name) is still open, so nothing has changed. Quit it, then "
                    + "try again.",
                code: "app_still_open")
        case .notRunning:
            return await body(false)
        case .quit(let copy):
            guard await closed(app.bundleID, of: desktopProvider) else {
                appControl.open(copy)
                return Self.stillClosing(title, app.name)
            }
            let failure = await body(true)
            if !Self.leftUnfinished(failure) { appControl.open(copy) }
            return failure
        }
    }

    /// Whether `failure` left a change to Claude's files partway done: the core keeps a
    /// record of it, and the next change finishes or undoes it. Claude, quit for the change,
    /// is not opened again then, since it would start on half of each account.
    nonisolated static func leftUnfinished(_ failure: ActionFailure?) -> Bool {
        guard let failure else { return false }
        return failure.code == "app_opened_midway"
            || failure.warnings.contains { $0.code == "switch_unfinished" }
    }

    /// A Claude Desktop failure, said for the app rather than for a terminal. `reason` is
    /// the one live usage recorded, where it was refused.
    nonisolated static func desktopFailure(
        _ title: String, _ error: Error, reason: String? = nil
    ) -> ActionFailure {
        let code = code(of: error)
        return ActionFailure(
            title, message: Advice.desktop(code, reason: reason) ?? saying(error), code: code,
            warnings: warnings(of: error))
    }

    /// The first step of adding a Claude Desktop account: Claude is quit, the account in use
    /// is parked and Claude left signed out, and Claude is opened again for somebody to sign
    /// in to another account. Nothing is signed out on claude.ai. The sheet stays open for
    /// the next step; a sign-out is not a switch to anybody, so nothing is said of one.
    @discardableResult
    func desktopAddAsked() async -> ActionFailure? {
        let title = "Couldn’t put the account in use aside"
        var said: [Warning] = []
        let failure = await quitThen(await desktopApp(), failing: title) { reopening in
            do {
                said = try await service.switchToSignedOut(desktopProvider).warnings
                claudeLeftClosed = !reopening
                changesSeen += 1
                updatedAt = nil
                return nil
            } catch {
                return Self.desktopFailure(title, error)
            }
        }
        if let failure {
            keep(failure)
            return failure
        }
        desktopAwaiting = await service.awaitingSignIn()
        resumedAwaiting = desktopAwaiting?.startedAt
        await refresh()
        warnings += said.filter { !warnings.contains($0) }
        return nil
    }

    /// The last step: Claude is quit again, the account signed in to it now is enrolled as
    /// `label`, and Claude is opened on it.
    @discardableResult
    func desktopEnrollAsked(label: String) async -> ActionFailure? {
        await enrol(label, for: desktopProvider)
    }

    /// Enrols the account Claude is signed in to, with Claude quit: it rewrites its files
    /// while it runs. Opened again before the read that follows, which can wait on the
    /// network. A failure's warnings stay in the window once the sheet is closed.
    private func enrolDesktop(_ name: String) async -> ActionFailure? {
        let title = "Couldn’t name this account"
        let target = qualified(name, for: desktopProvider)
        let failure = await quitThen(await desktopApp(), failing: title) { _ in
            do {
                _ = try await service.enrollCurrent(target)
                changesSeen += 1
                updatedAt = nil
                return nil
            } catch {
                return Self.desktopFailure(title, error)
            }
        }
        if let failure {
            keep(failure)
            return failure
        }
        desktopAwaiting = await service.awaitingSignIn()
        closeDesktopSheet()
        await refresh()
        return nil
    }

    /// Changing one's mind halfway through adding an account: the account parked for it is
    /// switched back to, with Claude quit and opened again like any switch.
    @discardableResult
    func desktopPutBack() async -> ActionFailure? {
        guard let from = desktopAwaiting?.fromLabel, switchUnderWay == nil else { return nil }
        let target = qualified(from, for: desktopProvider)
        switching = target
        defer { switching = nil }
        let failure = await quitThen(await desktopApp(), failing: "Couldn’t put \(from) back") {
            reopening in
            await switchWithoutReading(to: target, reopening: reopening)
        }
        if let failure { return failure }
        desktopAwaiting = await service.awaitingSignIn()
        closeDesktopSheet()
        await refresh()
        return nil
    }

    /// Closes the sheet for a Claude Desktop sign-in or name, once it has done its work.
    private func closeDesktopSheet() {
        guard let shown = sheet else { return }
        switch shown {
        case .add(let code):
            if code == desktopProvider { sheet = nil }
        case .signInAgain(let code, _), .name(let code, _):
            if code == desktopProvider { sheet = nil }
        case .rename, .liveUsage:
            break
        }
    }

    /// What the core keeps about Claude Desktop beside its accounts: whether live usage is
    /// on, and an add left halfway. Neither reads Claude's key or asks anything of macOS, so
    /// this never raises the keychain's prompt. The sheet for an add left halfway comes up
    /// by itself only at the first read after the app opens.
    func readDesktop() async {
        guard tools.contains(where: { $0.code == desktopProvider }) else { return }
        noteLiveUsage(await service.liveUsage())
        let waiting = await service.awaitingSignIn()
        let resuming = !desktopReadSinceLaunch
        desktopReadSinceLaunch = true
        desktopAwaiting = waiting
        if resuming, let waiting, waiting.startedAt != resumedAwaiting, sheet == nil {
            resumedAwaiting = waiting.startedAt
            claudeLeftClosed = appControl.running(Self.claudeApp.bundleID) == nil
            present(.add(provider: desktopProvider))
        }
    }

    /// The sheet for an add left halfway, asked for from the menu or the window's notice.
    func finishDesktopAddAsked() {
        present(.add(provider: desktopProvider))
    }

    /// Opens Claude for the sign-in, where Pitboard left it closed because it was not open
    /// to begin with.
    @discardableResult
    func openClaudeAsked() -> ActionFailure? {
        let bundleID = Self.claudeApp.bundleID
        guard let copy = appControl.running(bundleID) ?? appControl.installed(bundleID) else {
            return ActionFailure(
                "Couldn’t open Claude",
                message: "macOS doesn’t know where Claude is. Open it from Applications.")
        }
        appControl.open(copy)
        claudeLeftClosed = false
        return nil
    }

    /// Keeps `state`, and says once that macOS stopped Pitboard reading Claude's key. Said
    /// again only after live usage has worked in between, or been turned off.
    private func noteLiveUsage(_ state: LiveUsageState) {
        liveUsage = state
        if state.enabled, state.approval == "needs_approval" {
            guard !defaults.bool(forKey: DefaultsKey.liveUsagePauseTold) else { return }
            defaults.set(true, forKey: DefaultsKey.liveUsagePauseTold)
            notifier.tellLiveUsagePaused()
        } else if state.approval == "granted" || !state.enabled {
            defaults.removeObject(forKey: DefaultsKey.liveUsagePauseTold)
        }
    }

    /// Shows the sheet that says what live usage reads and why macOS asks. Nothing else
    /// turns it on: not a read, not a toggle, not a notification.
    func liveUsageAsked() {
        present(.liveUsage)
    }

    /// The sheet's Continue, and the one place live usage is turned on. Reading Claude's key
    /// is what can raise the keychain's prompt, so it happens only here, asked for.
    @discardableResult
    func liveUsageEnableAsked() async -> ActionFailure? {
        let before = await service.liveUsage()
        do {
            noteLiveUsage(try await service.enableLiveUsage())
            if sheet == .liveUsage { sheet = nil }
            updatedAt = nil
            await refresh(asked: true)
            return nil
        } catch {
            // What macOS answered, which the core records as the reason live usage needs
            // approving. A question it could not ask records nothing, and an answer the same
            // as the last one cannot be told from that; both are left to the core's message,
            // which knows which it was.
            let after = await service.liveUsage()
            let reason =
                after != before && after.approval == "needs_approval" ? after.reason : nil
            return Self.desktopFailure("Couldn’t turn on live usage", error, reason: reason)
        }
    }

    /// Turns live usage off, which reads nothing and asks nothing.
    @discardableResult
    func liveUsageDisable() async -> ActionFailure? {
        do {
            noteLiveUsage(try await service.disableLiveUsage())
            updatedAt = nil
            await refresh()
            return nil
        } catch {
            return Self.desktopFailure("Couldn’t turn off live usage", error)
        }
    }
}
