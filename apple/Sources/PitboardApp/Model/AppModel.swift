import AppKit
import Foundation
import PitboardKit

/// SwiftUI has a `Window` of its own, so a limit's window is named for what it is here.
typealias Limits = PitboardBindings.Window

/// What the app shows, and the only place that calls the core. Every call runs off the main
/// thread inside `PitboardService`; this only holds the answers.
///
/// The menu, the window and the settings all read this one model, so an account reads the
/// same everywhere and a change made in one shows in the others at once.
@MainActor
@Observable
public final class AppModel {
    private let service: any Core
    /// Every tool pitboard handles, in the order a listing shows them.
    let tools: [Tool]
    /// What the last read found. Every read that assigns it also tells the claude.ai windows
    /// which accounts are enrolled, so the store of one forgotten anywhere is deleted, and a
    /// read that fails with nothing known assigns nothing and deletes nothing.
    private(set) var status: Status? {
        didSet { web.keep(Set(claudeWindows.map(\.store))) }
    }
    /// What went wrong with the last read, when it did not answer. The numbers shown are
    /// then the last ones measured.
    private(set) var problem: String?
    /// The stable code behind `problem`, for deciding what to offer. Branching on the
    /// wording of a message is how an offer survives the message changing under it.
    private(set) var problemCode: String?
    /// The account a switch is running for, as its label with its tool, so its row can
    /// show it.
    private(set) var switching: String?
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
    /// command line, opening at login, what doctor finds and what pitboard has changed.
    let machine: MachineModel

    /// Where pitboard keeps its own preferences, which views read through `@AppStorage`.
    let defaults: UserDefaults

    /// The claude.ai windows. Given no `Core`, so nothing in them has a path to any login.
    let web: WebModel
    /// claude.ai links asked to be opened from outside, until somebody chooses an account.
    let links: LinkModel
    /// Whether the app tells macOS about its Service.
    let registersServices: Bool

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
        var warnings: [Warning] = []

        /// What a switch means for a tool's running sessions, said only when the core did
        /// not count them. When it did, its own warning says the same with the count and
        /// with what not to do in them, and the same fact twice is once too many.
        var notice: String? {
            guard let restart,
                !warnings.contains(where: { $0.code == "sessions_still_running" })
            else { return nil }
            return restartNotice(program: restart.program, from: restart.from)
        }
    }

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
            loginItem: dependencies.loginItem, notifies: dependencies.notifies,
            web: dependencies.web, linkScheme: dependencies.linkScheme,
            registersServices: dependencies.registersServices)
    }

    /// `watching` starts what runs by itself: the periodic read, the wake notice, the read
    /// when a menu opens, the poll that notices a change made somewhere else, and the one
    /// repair of a schedule an older app wrote. A test drives those itself, and two of them
    /// firing under a test is how a test stops telling the truth about what set what.
    ///
    /// `web` has no default: WebKit's own stores are deleted by what a read says, and a model
    /// made without saying which stores it keeps must not reach them.
    init(
        watching: Bool = true,
        service: any Core,
        defaults: UserDefaults = .standard,
        commandLineTool: CommandLineTool = CommandLineTool(),
        loginItem: any LoginItem = MainAppLoginItem(),
        notifies: Bool = false,
        web: ClaudeWeb,
        linkScheme: String,
        registersServices: Bool = false
    ) {
        self.service = service
        tools = service.tools()
        self.defaults = defaults
        let windows = WebModel(web: web)
        self.web = windows
        links = LinkModel(scheme: linkScheme, web: windows)
        self.registersServices = registersServices
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
        } catch {
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
            guard let read = try? await service.statusOffline() else { return }
            status = read
            lastReadingsAt = measured
            forgetSwitchesUndone(by: read)
            advise(from: read)
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
    func present(_ sheet: AccountSheet) {
        if signingIn == nil {
            self.sheet = sheet
        }
        showWindow(.accounts)
    }

    // MARK: - claude.ai windows

    /// Every enrolled Claude Code account, each with the claude.ai window of its own.
    var claudeWindows: [ClaudeAccount] { PitboardApp.claudeWindows(in: status) }

    /// Asks for `store`'s claude.ai window, at `link` when there is one: from the menus, an
    /// account's shortcut menu, or the account picker.
    func openClaude(_ store: UUID, at link: URL? = nil) {
        web.open(store, at: link)
    }

    /// The page of `store`'s window, once a read shows an enrolled account deriving it. Nil
    /// before, since asking WebKit for the store would make one.
    func claudePage(for store: UUID) async -> ClaudePage? {
        guard let account = claudeWindows.first(where: { $0.store == store }) else {
            return nil
        }
        return await web.attach(account)
    }

    /// Says a failure of something asked for away from the window, in the window.
    func present(_ failure: ActionFailure?) {
        guard let failure else { return }
        presentedFailure = failure
        showWindow()
    }

    /// A switch asked for from the menu or a notification, where a failure has nowhere to be
    /// said but the window.
    func switchAsked(to qualified: String) async {
        // One switch at a time: the second would wait behind the first anyway, and its
        // choice was made from a menu that did not yet show the first.
        guard switching == nil else { return }
        present(await use(qualified))
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
        guard !title.isEmpty else { return "pitboard" }
        guard showsTools, let account = titled(status?.accounts ?? [], order: tools),
            let tool = tool(account.provider)
        else { return "pitboard, \(title)" }
        return "pitboard, \(title), \(tool.name)"
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
        return "\(names) \(missing.count == 1 ? "is" : "are") not offered: pitboard did not "
            + "find \(programs) on this Mac."
    }

    /// The tool the sheet for `sheet` starts on.
    func provider(for sheet: AccountSheet) -> String {
        switch sheet {
        case .add(let code): code ?? addable.first?.code ?? defaultProvider
        case .signInAgain(let code, _), .name(let code, _), .rename(let code, _): code
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
        let label = account.label ?? account.email
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
    /// A login pitboard could not read or cannot switch is not one of them, however it
    /// looks: naming it would enrol something that can never be switched to.
    var unnamed: [Account] {
        status?.accounts.filter { $0.signedIn && $0.label == nil && !$0.unplaced } ?? []
    }

    /// A login signed in now that is not enrolled, in any tool.
    var unenrolled: Bool { !unnamed.isEmpty }

    /// How far along setting pitboard up this machine is.
    ///
    /// Somebody who installed only the app has never typed a pitboard command and may never
    /// want to. Every state before `ready` used to show either a line naming a command to run
    /// or nothing at all, which is the same as telling them the app does not work.
    enum Footing: Equatable {
        /// No tool pitboard works with was found on this machine, and none has an account
        /// or a login here. Nothing pitboard does means anything without a tool, and
        /// pitboard cannot install one. Claude Code is the tool it names.
        case noClaudeCode
        /// No tool has anybody signed in, and nothing is enrolled.
        case noOneSignedIn
        /// Somebody is signed in to a tool and pitboard has not been told what to call them.
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

    /// pitboard's errors already say what to do, so they are shown as they are.
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
            await refresh()
            return nil
        } catch {
            // Nothing moved here either, so what the last switch said stands.
            let failure = ActionFailure(
                "Couldn’t switch to \(split(qualified).label)", error: error)
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

    /// Drops the account `qualified` names, and the login parked for it. A Claude Code
    /// account's claude.ai window goes with it, and everything the window keeps, but only
    /// once the core has forgotten the account: a forget that fails leaves both.
    ///
    /// The window's page closes before the read that follows, so WebKit can let the store
    /// go. A store that still cannot be deleted is said, and the account is forgotten all
    /// the same; the sweep tries again at every later read.
    @discardableResult
    func forget(_ qualified: String) async -> ActionFailure? {
        let label = split(qualified).label
        let forgotten = status?.accounts.first { $0.qualified == qualified }
        let store = forgotten.flatMap(webStoreID(for:))
        do {
            _ = try await service.forget(qualified)
        } catch {
            return ActionFailure("Couldn’t forget \(label)", error: error)
        }
        changesSeen += 1
        if let store { web.close(store) }
        updatedAt = nil
        await refresh()
        guard let store, let failed = await web.discard(store) else { return nil }
        return ActionFailure(
            "Couldn’t delete \(label)’s claude.ai data",
            message: "pitboard forgot \(label), but WebKit could not delete what its claude.ai "
                + "window keeps: \(failed.localizedDescription) pitboard tries again the next "
                + "time it reads your accounts.")
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
    /// there is nothing to hand a terminal. Claude Code's reads a code typed back when the
    /// callback cannot be reached, which is what the code field is for; Codex's prints an
    /// address and reads nothing.
    ///
    /// Returns once the sign-in has finished, failed or been cancelled. The sheet that
    /// started it closes when it finishes, and says the failure when it fails. A cancelled
    /// sign-in is not a failure: somebody asked for it to stop.
    @discardableResult
    func signIn(_ name: String, for provider: String) async -> ActionFailure? {
        guard signingIn == nil else { return nil }
        let shown = SigningIn(label: name, tool: tool(provider)?.name ?? provider, from: sheet)
        signingIn = shown
        let title = "Couldn’t sign in to \(name)"
        do {
            let session = try await service.signIn(qualified(name, for: provider))
            // Cancelled while it was starting: stop what started rather than watch it.
            guard signingIn === shown else {
                SignInCalls.run { session.cancel() }
                return nil
            }
            shown.takesACode = session.takesACode()
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
        guard case .inUse(let again) = done.enrolled else { return done.warnings }
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

    /// Types the fallback code back, for a browser that could not reach the callback. Off
    /// the main thread, like everything that waits on the tool.
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
    /// The tool's name, as the app says it.
    let tool: String
    private(set) var said = ""
    var pasted = false
    /// Whether this tool's sign-in reads a code typed back. False until the session has
    /// started, so no field is offered for a sign-in that could not take what is typed.
    var takesACode = false
    @ObservationIgnored var session: SignIn?
    /// The sheet it was started from, which is the one to close when it finishes.
    let from: AccountSheet?

    init(label: String, tool: String, from: AccountSheet? = nil) {
        self.label = label
        self.tool = tool
        self.from = from
    }

    func add(_ text: String) {
        said += text
    }

    /// The address the tool printed, for a browser that did not open by itself. Only
    /// `https`, which leaves out the loopback address Codex also prints: that one is where
    /// the browser comes back to, not where a person goes.
    var url: URL? {
        guard
            let found = said.range(
                of: "https://[^\\s\"'<>\\e]+", options: .regularExpression)
        else {
            return nil
        }
        return URL(string: String(said[found]))
    }

    /// Claude Code asks for a code only when its callback could not be reached, and Codex
    /// never does.
    var wantsCode: Bool { takesACode && said.contains("Paste code") && !pasted }
}
