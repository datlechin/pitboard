import AppKit
import Foundation
import PitboardKit

/// SwiftUI has a `Window` of its own, so a limit's window is named for what it is here.
typealias Limits = PitboardBindings.Window

/// What the menu shows, and the only place that calls the core. Every call runs off the main
/// thread inside `PitboardService`; this only holds the answers.
@MainActor
@Observable
final class AppModel {
    private let service: any Core
    /// Every tool pitboard handles, in the order a listing shows them.
    let tools: [Tool]
    private(set) var status: Status?
    private(set) var problem: String?
    /// The account a switch is running for, as its label with its tool, so its row can
    /// show it.
    private(set) var switching: String?
    private(set) var updatedAt: Date?
    /// Accounts that have run out while another of the same tool has room, one per tool at
    /// most. Shown in the panel whether or not notifications are allowed, so the advice does
    /// not depend on a permission.
    private(set) var advice: [Advice] = []
    /// What each tool's last switch said that is still true, one per tool at most. Kept
    /// apart from `warnings`, which the read that follows every switch replaces: these are
    /// about the switch, and stay true until the person has done something about them.
    private(set) var lastSwitches: [LastSwitch] = []
    /// Every check pitboard makes about this machine, once someone asks for them.
    private(set) var checks: [Check] = []
    /// A label being typed, when the panel is asking for one, and what it is for.
    var naming: Naming? {
        didSet {
            // The form for another account offers the tools found, and the first answer may
            // have come while the login shell was too slow to say where they are.
            if case .another = naming, oldValue == nil {
                Task { await askWhatIsInstalled() }
            }
        }
    }
    /// A sign-in in progress, and everything Claude Code has said about it.
    private(set) var signingIn: SigningIn?
    /// Everything that went wrong on the way, not only the first of them. A switch can warn
    /// about an overriding environment variable and a config that did not update at once,
    /// and showing one of those and dropping the other is how a person fixes the wrong
    /// thing.
    private(set) var warnings: [Warning] = []
    /// An interrupted switch nothing can finish, which the panel offers a way out of.
    private(set) var stuck: Bool = false
    /// What pitboard has changed, once someone asks for it.
    private(set) var changes: [Change] = []
    /// Whether anything keeps parked logins alive without a command being run.
    private(set) var schedule: Schedule = .absent
    /// What the last renewal came to, for the settings pane that started it.
    private(set) var renewals: [Renewed]?
    /// The stable code behind `problem`, for deciding what to offer. Branching on the
    /// wording of a message is how an offer survives the message changing under it.
    private(set) var problemCode: String?
    /// This app's own command line, and where a terminal would find it.
    let commandLineTool: CommandLineTool
    /// The `pitboard` a terminal runs, once the settings have looked.
    private(set) var commandLine: CommandLineTool.Found?
    /// Why the command line could not be linked, said beside the button that tried.
    private(set) var linkFailed: String?

    enum Naming: Equatable {
        /// Record the login signed in now to this tool: no browser, so the app does it
        /// itself.
        case theOneInUse(String)
        /// A different account, which means the tool's own sign-in in a browser. The tool it
        /// starts on, when something already said which; the form can change it.
        case another(String?)
    }

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
        let to: String
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
        /// with what not to do in them, and the same fact twice is once too many in a
        /// glance.
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
        let from: String
    }

    /// Usage is asked of each tool's service for every account, so it is asked sparingly: on
    /// opening the menu when the numbers are a minute old, and in the background every five
    /// minutes.
    static let staleAfter: TimeInterval = 60
    private static let refreshEvery: Duration = .seconds(300)

    private let notifier = Notifier()
    /// The codes of the tools whose program was found, once the first read has asked. Read
    /// then and when the form for another account opens, and not on every read: asking
    /// crossed into the core on every keystroke in the form that reads it. Asked from a read
    /// rather than here, because finding them can mean waiting on the person's login shell.
    private var installed: Set<String> = []
    /// Set before the answer arrives, so two reads at once ask once.
    private var askedWhatIsInstalled = false
    /// The tools somebody said "Not now" to a second account for, by code.
    private(set) var secondAccountDeclined: Set<String> = []
    private let defaults: UserDefaults
    private static let declinedKey = "secondAccountDeclined"

    /// How often to ask whether anything on this machine has changed. One stat of one
    /// file, so it costs nothing to ask often; before it, a switch typed in a terminal
    /// left the menu bar naming the account the person had just stopped using for as long
    /// as five minutes, with a button offering a switch that had already happened.
    private static let noticeEvery: Duration = .seconds(2)
    /// Nil until something has looked. A machine with no account index reports 0, which
    /// is a real answer and not an absence.
    private var lastChangedAt: Int64?

    /// `watching` starts the timers: the periodic read, the wake notice and the poll that
    /// notices a change made somewhere else. A test drives those itself, and two of them
    /// firing under a test is how a test stops telling the truth about what set what.
    init(
        watching: Bool = true,
        service: any Core = PitboardService(asking: { Settings.forCurrentUserAsked() }),
        defaults: UserDefaults = .standard,
        commandLineTool: CommandLineTool = CommandLineTool()
    ) {
        self.service = service
        tools = service.tools()
        self.defaults = defaults
        self.commandLineTool = commandLineTool
        var declined = Set(defaults.stringArray(forKey: Self.declinedKey) ?? [])
        // Said before there was a second tool, so about the only tool there was.
        if defaults.bool(forKey: "hideSecondAccountNudge") {
            declined.insert("claude")
            defaults.set(declined.sorted(), forKey: Self.declinedKey)
            defaults.removeObject(forKey: "hideSecondAccountNudge")
        }
        secondAccountDeclined = declined
        notifier.start()
        notifier.onSwitch = { [weak self] label in
            Task { await self?.use(label) }
        }
        guard watching else { return }
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
    #endif

    /// Has anything on this machine changed since the last look. Reads only what is already
    /// known: no network, no keychain, and no request of any service.
    private func noticeOtherChanges() async {
        let now = await service.changedAt()
        let seen = lastChangedAt
        lastChangedAt = now
        // The first look only records where things stand; there is nothing to compare to.
        // A switch this app has in flight is its own change and not somebody else's, and
        // taking it for one put away what the switch had just said.
        guard switching == nil, let seen, seen != now,
            let read = try? await service.statusOffline()
        else {
            return
        }
        status = read
        forgetSwitchesUndone(by: read)
    }

    /// The account in use and its tightest limit, as the menu bar reads it.
    var title: String { menuTitle(for: status, order: tools) }

    /// The panel's accounts, a section per tool once there is more than one.
    var groups: [AccountGroup] { grouped(status?.accounts ?? [], by: tools) }

    /// Whether accounts of more than one tool are shown, which is when anything says which
    /// tool an account is for. A machine with one tool looks exactly as it did before there
    /// were two.
    var showsTools: Bool { Set(status?.accounts.map(\.provider) ?? []).count > 1 }

    func tool(_ code: String) -> Tool? { tools.first { $0.code == code } }

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

    /// Why a tool is missing from the form for a new account, rather than leaving it out
    /// without a word. Nil when every tool is offered.
    var notOffered: String? {
        let missing = tools.filter { !addable.contains($0) }
        guard !missing.isEmpty else { return nil }
        let names = missing.map(\.name).joined(separator: " and ")
        let programs = missing.map(\.program).joined(separator: " or ")
        return "\(names) \(missing.count == 1 ? "is" : "are") not offered: pitboard did not "
            + "find \(programs) on this Mac."
    }

    /// The tool a form asking `asking` starts on.
    func provider(for asking: Naming) -> String {
        switch asking {
        case .theOneInUse(let code): code
        case .another(let code): code ?? addable.first?.code ?? defaultProvider
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
        let label = account.label ?? "unenrolled"
        guard showsTools else { return label }
        return "\(label) (\(tool(account.provider)?.name ?? account.provider))"
    }

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

    /// The warnings shown under `problem`: every one but the one it already says. A failure's
    /// message is not one of its warnings, and dropping the first of them hid one that was.
    var otherWarnings: [Warning] {
        warnings.filter { $0.message != problem }
    }

    var updated: String {
        guard let updatedAt else { return "not read yet" }
        let ago = Int(Date().timeIntervalSince(updatedAt))
        return ago < 60 ? "updated just now" : "updated \(ago / 60)m ago"
    }

    /// `asked` means somebody asked for this reading rather than a timer producing it, and
    /// is what tells the core to ask each service again whatever it read moments ago. The
    /// first read also asks which tools are installed, for the form for a new account.
    func refresh(ifOlderThan seconds: TimeInterval = 0, asked: Bool = false) async {
        if !askedWhatIsInstalled {
            askedWhatIsInstalled = true
            await askWhatIsInstalled()
        }
        if let updatedAt, Date().timeIntervalSince(updatedAt) < seconds { return }
        do {
            let read = try await service.status(fresh: asked)
            status = read
            forgetSwitchesUndone(by: read)
            warnings = read.warnings
            problem = read.warnings.first?.message
            stuck = read.warnings.contains { $0.code == "recovery_undetermined" }
            updatedAt = Date()
            lastChangedAt = await service.changedAt()
            problemCode = read.warnings.first?.code
            advice = Advice.about(read, tools: tools, unless: notifier.told)
            advice.forEach(notifier.tell)
        } catch {
            problem = Self.saying(error)
            problemCode = Self.code(of: error)
            // What went wrong this time, in place of what was wrong last time. A failure
            // carries its own warnings, and leaving the previous read's in place showed a
            // fresh network error above warnings that may have been fixed since.
            warnings = Self.warnings(of: error)
            // A read that could not reach a service still has something true to show: the
            // last numbers measured, and who each tool's own files say is signed in. An
            // empty panel says the accounts are gone, which is not what happened.
            if status == nil, let known = try? await service.statusOffline() {
                status = known
            }
            stuck = Self.code(of: error) == "recovery_undetermined"
        }
    }

    /// Which tools the service found a program for. It asks the login shell once more where
    /// that was too slow to answer before, so a later answer can find more than the first.
    private func askWhatIsInstalled() async {
        installed = Set(await service.installed().map(\.code))
    }

    /// What pitboard has changed, newest last. Read when something asks to see it.
    func readChanges(_ limit: UInt32 = 200) async {
        changes = await service.log(limit: limit)
    }

    /// Whether anything keeps parked logins alive without a command being run.
    func readSchedule() async {
        schedule = await service.schedule()
    }

    /// Hand the renewal of parked logins to this computer's own scheduler, or take it back.
    ///
    /// Opt-in, and the caller says what it does before offering it: a background process
    /// that talks to a service on a schedule is the shape most likely to be read as
    /// automation, so it is something a person turns on knowing what it is.
    func setSchedule(on: Bool) async {
        do {
            if on {
                _ = try await service.scheduleInstall()
            } else {
                _ = try await service.scheduleUninstall()
            }
        } catch {
            problem = Self.saying(error)
        }
        await readSchedule()
    }

    /// Looks for the `pitboard` a terminal runs: on the login shell's `PATH`, then where
    /// each way of installing it puts it.
    func findCommandLine() async {
        let path = await service.searchPath()
        let home = ProcessInfo.processInfo.environment["HOME"] ?? NSHomeDirectory()
        let directories =
            (path?.split(separator: ":").map(String.init) ?? [])
            + CommandLineTool.places(home: home)
        let tool = commandLineTool
        commandLine = await Task.detached(priority: .utility) {
            tool.find(in: directories)
        }.value
    }

    /// Links this app's command line onto the `PATH`, once macOS has asked for an
    /// administrator's password.
    func installCommandLine() async {
        linkFailed = nil
        if case .failed(let why) = await commandLineTool.install() {
            linkFailed = why
        }
        await findCommandLine()
    }

    /// Renew every parked login that is due, now. Never switches and never asks for usage.
    func renewNow() async {
        renewals = await service.renew()
        await refresh(asked: true)
    }

    /// Give up on an interrupted switch that cannot be finished, keeping every login. The
    /// way out when recovery cannot reach the tool's service, which used to mean opening a
    /// terminal.
    func abandonStuckSwitch() async {
        do {
            if let given = try await service.abandonRecovery() {
                problem =
                    "Gave up on the switch from \(given.from) to \(given.to). "
                    + "\(given.loginsKept) login(s) kept; nothing was deleted."
            }
            stuck = false
            await refresh(asked: true)
        } catch {
            problem = Self.saying(error)
        }
    }

    /// Switches to the account `qualified` names, which is its label with its tool: two
    /// tools can each have a `work`, and a bare one then names neither.
    ///
    /// What the switch means for sessions already running depends on the tool. One that
    /// follows by itself gets a countdown; one that never does gets said so, since a
    /// countdown there would promise something that is not going to happen.
    func use(_ qualified: String) async {
        switching = qualified
        defer { switching = nil }
        do {
            let done = try await service.switchTo(qualified)
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
            advice = []
            updatedAt = nil
            await refresh()
        } catch {
            // Nothing moved here either, so what the last switch said stands.
            sayFailed(error)
        }
    }

    /// A change that failed, said the way any failure is: with its warnings first, and
    /// nothing the last read found dropped to make room for them.
    private func sayFailed(_ error: Error) {
        problem = Self.saying(error)
        let failed = Self.warnings(of: error)
        warnings = failed + warnings.filter { !failed.contains($0) }
    }

    /// In place of what the same tool's last switch said, or after the others.
    private func remember(_ said: LastSwitch) {
        if let at = lastSwitches.firstIndex(where: { $0.provider == said.provider }) {
            lastSwitches[at] = said
        } else {
            lastSwitches.append(said)
        }
    }

    /// What `last` warned about that the read after it did not already show, so nothing is
    /// said twice.
    func warnings(after last: LastSwitch) -> [Warning] {
        last.warnings.filter { !warnings.contains($0) }
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
        /// Claude Code is not on this machine and no other tool has an account here.
        /// Nothing pitboard does means anything without a tool, and pitboard cannot install
        /// one.
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
        let accounts = status?.accounts
        if problemCode == "claude_program_missing",
            accounts?.allSatisfy({ $0.provider == "claude" }) ?? true
        {
            return .noClaudeCode
        }
        // Before the first read there is nothing to go on, and guessing at this point
        // shows somebody a setup step they may have finished years ago.
        guard let accounts else { return .ready }
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
        defaults.set(secondAccountDeclined.sorted(), forKey: Self.declinedKey)
    }

    /// Records the login signed in now to `provider`'s tool under a name, with the tool's
    /// prefix, so a Codex login is enrolled as Codex's and not as a Claude Code account.
    func enrol(_ name: String, for provider: String) async {
        naming = nil
        do {
            _ = try await service.enrollCurrent(qualified(name, for: provider))
            updatedAt = nil
            await refresh()
        } catch {
            problem = Self.saying(error)
        }
    }

    /// Runs the tool's own sign-in and shows what it says. Both tools open the browser
    /// themselves and finish through a loopback callback, so there is nothing to hand a
    /// terminal. Claude Code's reads a code typed back when the callback cannot be reached,
    /// which is what the code field is for; Codex's prints an address and reads nothing.
    func signIn(_ name: String, for provider: String) async {
        naming = nil
        let shown = SigningIn(label: name, tool: tool(provider)?.name ?? provider)
        signingIn = shown
        do {
            let session = try await service.signIn(qualified(name, for: provider))
            // Cancelled while it was starting: stop what started rather than watch it.
            guard signingIn === shown else {
                Task.detached(priority: .userInitiated) { session.cancel() }
                return
            }
            shown.takesACode = session.takesACode()
            shown.session = session
            await watch(session, shown: shown, for: provider)
        } catch {
            guard signingIn === shown else { return }
            signingIn = nil
            sayFailed(error)
        }
    }

    /// Reads what the tool says until it stops, then records what it signed in to.
    private func watch(_ session: SignIn, shown: SigningIn, for provider: String) async {
        while let said = await Task.detached(
            priority: .utility,
            operation: {
                session.nextLine()
            }
        ).value {
            shown.add(said)
        }
        // Cancelled. The tool was stopped because somebody asked, so the sign-in that is
        // "no longer running" is not something that went wrong, and nothing is enrolled.
        guard signingIn === shown else { return }
        do {
            let done = try await Task.detached(priority: .utility) {
                try session.finish()
            }.value
            signingIn = nil
            let said = enrolled(done, as: shown.label, for: provider)
            updatedAt = nil
            await refresh()
            // Said after the read that follows, which would otherwise put it away.
            warnings += said.filter { !warnings.contains($0) }
        } catch {
            signingIn = nil
            sayFailed(error)
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

    /// Signs in again to an enrolled account whose parked login can no longer be used,
    /// through the same sign-in as a new account, so the address and the code field show
    /// the same way. By its label alone, since `signIn` puts its tool in front.
    func signInAgain(to account: Account) async {
        guard let label = account.label else { return }
        await signIn(label, for: account.provider)
    }

    /// Types the fallback code back, for a browser that could not reach the callback. Off
    /// the main thread, like everything that waits on the tool.
    func paste(_ code: String) {
        guard let shown = signingIn, let session = shown.session else { return }
        shown.pasted = true
        Task.detached(priority: .userInitiated) { try? session.paste(line: code) }
    }

    /// Stops the sign-in, off the main thread: stopping waits for the tool to exit, and a
    /// Codex sign-in waiting on the browser once kept the whole app waiting with it.
    func cancelSignIn() {
        let session = signingIn?.session
        signingIn = nil
        guard let session else { return }
        Task.detached(priority: .userInitiated) { session.cancel() }
    }

    /// Drops the account `qualified` names, and the login parked for it.
    func forget(_ qualified: String) async {
        do {
            _ = try await service.forget(qualified)
            updatedAt = nil
            await refresh()
        } catch {
            problem = Self.saying(error)
        }
    }

    /// What `pitboard doctor` reports, for when something is wrong at machine level and
    /// the one line a failed call carries is not enough to act on.
    func diagnose() async {
        checks = await service.doctor().checks
    }

    func forgetDiagnosis() {
        checks = []
    }

    /// pitboard's errors already say what to do, so they are shown as they are.
    private static func saying(_ error: Error) -> String {
        if case PitboardError.Failed(_, _, let message, _) = error {
            return message
        }
        return error.localizedDescription
    }

    /// Everything a failure warns about, not only what stopped it. A switch can fail and
    /// still have something to say about an overriding environment variable.
    private static func warnings(of error: Error) -> [Warning] {
        if case PitboardError.Failed(_, _, _, let warnings) = error {
            return warnings
        }
        return []
    }

    /// The stable code behind an error, for deciding what to offer rather than reading the
    /// wording of a message.
    static func code(of error: Error) -> String? {
        if case PitboardError.Failed(let code, _, _, _) = error {
            return code
        }
        return nil
    }
}

/// A sign-in as the panel sees it: the name it will be enrolled under, the tool it is for,
/// what the tool has said so far, and the session to type back to.
@MainActor
@Observable
final class SigningIn {
    let label: String
    /// The tool's name, as the panel says it.
    let tool: String
    private(set) var said = ""
    var pasted = false
    /// Whether this tool's sign-in reads a code typed back. False until the session has
    /// started, so no field is offered for a sign-in that could not take what is typed.
    var takesACode = false
    @ObservationIgnored var session: SignIn?

    init(label: String, tool: String) {
        self.label = label
        self.tool = tool
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

/// The limit worth putting in the menu bar: the account's own, not one scoped to a single
/// model, and one the account is working against when the server says which. A scoped row
/// at 98% would otherwise read as though everything had stopped.
func headline(of windows: [Limits]) -> Limits? {
    let ownLimits = windows.filter { $0.scope == nil }
    let candidates = ownLimits.isEmpty ? windows : ownLimits
    let active = candidates.filter(\.isActive)
    return (active.isEmpty ? candidates : active).max { $0.percent < $1.percent }
}

/// What the menu bar says: the account in use and the limit closest to its end. Nothing is
/// known until the first read, and an account signed in but not enrolled has no name here.
///
/// With more than one tool there is an account in use in each, and one bar. It follows the
/// enrolled one whose headline limit is most used, so what it shows is the account closest
/// to running out; a tie goes to the tool `order` lists first.
func menuTitle(for status: Status?, order tools: [Tool] = []) -> String {
    guard let account = titled(status?.accounts ?? [], order: tools) else { return "" }
    // Long labels are bounded, because this sits in a bar someone else also wants space in.
    let full = account.label ?? "unenrolled"
    let name = full.count > 12 ? full.prefix(11) + "…" : full[...]
    guard let tightest = headline(of: account.usage?.windows ?? []) else {
        return String(name)
    }
    return "\(name) \(Int(tightest.percent.rounded()))%"
}

/// The account the menu bar is about.
private func titled(_ accounts: [Account], order tools: [Tool]) -> Account? {
    let providers = inOrder(accounts.map(\.provider), by: tools)
    guard providers.count > 1 else { return accounts.first(where: \.signedIn) }
    let used = { (account: Account) in headline(of: account.usage?.windows ?? [])?.percent ?? -1
    }
    var closest: Account?
    for provider in providers {
        let inUse = accounts.first { $0.provider == provider && $0.signedIn && $0.label != nil }
        if let inUse, closest.map({ used(inUse) > used($0) }) ?? true {
            closest = inUse
        }
    }
    // Nobody enrolled is signed in anywhere: say what is signed in, as one tool would.
    return closest ?? accounts.first(where: \.signedIn)
}

/// One tool's accounts in the panel, under its name.
struct AccountGroup: Identifiable {
    /// The tool's code.
    let id: String
    /// Nil when every account shown is one tool's: a heading there says what nobody asked.
    let name: String?
    let accounts: [Account]
}

/// The accounts a section per tool, in the order `tools` lists them. One section, unnamed
/// and in the order given, when they are all one tool's, so a machine with one tool looks
/// exactly as it always did.
func grouped(_ accounts: [Account], by tools: [Tool]) -> [AccountGroup] {
    let providers = inOrder(accounts.map(\.provider), by: tools)
    guard providers.count > 1 else {
        return providers.map { AccountGroup(id: $0, name: nil, accounts: accounts) }
    }
    return providers.map { code in
        AccountGroup(
            id: code, name: tools.first { $0.code == code }?.name ?? code,
            accounts: accounts.filter { $0.provider == code })
    }
}

/// Tool codes without repeats, in the order `tools` lists them, and any it does not list
/// after those in the order they came. The core already lists rows this way; this keeps a
/// view from depending on it.
func inOrder(_ codes: [String], by tools: [Tool]) -> [String] {
    var seen: [String] = []
    for code in tools.map(\.code) + codes where codes.contains(code) && !seen.contains(code) {
        seen.append(code)
    }
    return seen
}
