#if DEBUG
    import Foundation
    import PitboardKit

    /// A core that answers from memory, the way the real one answers about a machine in the
    /// fixture's state, and changes that state the way the real one would: a switch moves
    /// who is in use, a sign-in adds an account, a forget drops one.
    ///
    /// Everything is under one lock, because the app calls it from whatever thread it likes.
    final class FixtureCore: Core, @unchecked Sendable {
        private let lock = NSLock()
        private let fixture: Fixture
        /// The apps running on the fixture's machine, which run Codex as ChatGPT does.
        private let apps: FixtureApps
        private var accounts: [Account]
        private var stuck: Bool
        private var changes: [Change] = []
        private var scheduled = false
        private var changedAt: Int64 = 1
        /// An add of a Claude Desktop account left halfway, as the core keeps one.
        private var awaiting: Awaiting?
        private var live: LiveUsageState

        init(_ fixture: Fixture, apps: FixtureApps = FixtureApps(), now: Date = Date()) {
            self.fixture = fixture
            self.apps = apps
            accounts = Self.accounts(for: fixture, now: now)
            stuck = fixture == .stuck
            changes = Self.history(now: now)
            // Turned on once, and stopped since by macOS, which an update to Claude can do.
            live =
                fixture == .claudeDesktop
                ? LiveUsageState(
                    enabled: true, approval: "needs_approval", reason: "item_changed",
                    lastOkAt: Int64(now.timeIntervalSince1970) - 86_400)
                : LiveUsageState(
                    enabled: false, approval: "unknown", reason: nil, lastOkAt: nil)
        }

        /// Claude Desktop only where the fixture is about it, so every other fixture shows
        /// what its UI tests were written against.
        func tools() -> [Tool] {
            fixture == .claudeDesktop ? Self.tools + [Self.desktop] : Self.tools
        }

        func installed() async -> [Tool] {
            fixture == .noClaudeCode ? [] : tools()
        }

        func searchPath() async -> String? { "/usr/bin:/bin" }

        func status(fresh: Bool) async throws -> Status {
            try lock.withLock {
                switch fixture {
                case .readFailure:
                    throw PitboardError.Failed(
                        code: "unreachable", cause: nil,
                        message: "Anthropic could not be reached.",
                        warnings: [])
                case .stuck where stuck:
                    throw PitboardError.Failed(
                        code: "recovery_undetermined", cause: nil,
                        message: "A switch from work to personal was interrupted, and it "
                            + "cannot be finished until Anthropic answers.",
                        warnings: [])
                default:
                    return currentStatus()
                }
            }
        }

        func statusOffline() async throws -> Status {
            lock.withLock { currentStatus() }
        }

        /// Claude's app and ChatGPT's two `codex` processes while each is open, as the
        /// process list shows them, named as the real core names them.
        func holding(_ provider: String) async -> [Holding] {
            if provider == "desktop", apps.isRunning(FixtureApps.claude) {
                return [
                    Holding(
                        kind: "claude_desktop_app", phrase: "the Claude app", pids: [5151],
                        remedy: .reopenApp(bundleId: FixtureApps.claude, name: "Claude"))
                ]
            }
            guard provider == "codex", apps.isRunning(FixtureApps.chatGPT) else { return [] }
            return [
                Holding(
                    kind: "chatgpt_app", phrase: "the ChatGPT app", pids: [4242, 4243],
                    remedy: .reopenApp(bundleId: FixtureApps.chatGPT, name: "ChatGPT"))
            ]
        }

        func doctor() async -> Diagnosis {
            Diagnosis(
                checks: [
                    Check(
                        code: "claude_program", name: "Claude Code", level: .ok,
                        detail: "/opt/homebrew/bin/claude", advice: ""),
                    Check(
                        code: "keychain", name: "Keychain", level: .ok,
                        detail: "login keychain, unlocked", advice: ""),
                    Check(
                        code: "schedule", name: "Daily renewal", level: .warn,
                        detail: "not scheduled",
                        advice:
                            "Turn on daily renewal in Pitboard's settings, so parked logins "
                            + "you leave alone do not expire."),
                ],
                healthy: true)
        }

        func switchTo(_ label: String) async throws -> Switched {
            try lock.withLock {
                guard let target = accounts.firstIndex(where: { $0.qualified == label }) else {
                    throw Self.failed("unknown_account", "There is no account called \(label).")
                }
                let provider = accounts[target].provider
                guard !accounts[target].signedIn else {
                    return Switched(
                        outcome: .alreadyActive(label: Self.typed(label)), warnings: [])
                }
                guard accounts[target].switchable else {
                    throw Self.failed(
                        "parked_login_expired",
                        "\(accounts[target].label ?? label)'s parked login has expired. Sign "
                            + "in to it again to use it.")
                }
                // The account left is parked, and so can be switched back to; every other
                // account of the tool stays as it was, an expired one included.
                let from = accounts.firstIndex { $0.provider == provider && $0.signedIn }
                if let from {
                    accounts[from] = with(accounts[from], signedIn: false, switchable: true)
                }
                accounts[target] = with(accounts[target], signedIn: true, switchable: false)
                record("switch", Self.typed(label))
                if provider == "desktop" { awaiting = nil }
                // Labels as the core types them: bare for Claude Code, with the tool for any
                // other, which is what the app compares an account with.
                return Switched(
                    outcome: .switched(
                        provider: provider,
                        from: from.flatMap { accounts[$0].qualified }.map(Self.typed) ?? "",
                        to: Self.typed(label),
                        adoption: provider == "codex"
                            ? .restart(program: "codex")
                            : provider == "desktop"
                                ? .nextLaunch(program: "Claude") : .follows(withinSeconds: 45)),
                    warnings: [])
            }
        }

        func enrollCurrent(_ label: String) async throws -> Enrolled {
            try lock.withLock {
                let (provider, name) = Self.parts(of: label)
                // Somebody signed in to another account in Claude while Pitboard waited.
                if provider == "desktop", awaiting != nil,
                    !accounts.contains(where: { $0.provider == provider && $0.signedIn })
                {
                    accounts.append(
                        Self.account(
                            nil, of: provider, email: "dana@new.example", signedIn: true,
                            windows: [Self.window("five_hour", 5, length: 18_000, in: 18_000)]))
                }
                guard
                    let index = accounts.firstIndex(where: {
                        $0.provider == provider && $0.signedIn && $0.label == nil
                    })
                else { throw Self.failed("nothing_signed_in", "Nobody is signed in to name.") }
                try requireUnused(name, of: provider)
                accounts[index] = with(accounts[index], label: name)
                record("enroll", Self.typed(label))
                if provider == "desktop" { awaiting = nil }
                return Enrolled(email: accounts[index].email, outcome: .current, warnings: [])
            }
        }

        func forget(_ label: String) async throws -> Changed {
            try lock.withLock {
                guard let index = accounts.firstIndex(where: { $0.qualified == label }) else {
                    throw Self.failed("unknown_account", "There is no account called \(label).")
                }
                guard !accounts[index].signedIn else {
                    throw Self.failed(
                        "cannot_forget_active_account",
                        "\(accounts[index].label ?? label) is the account in use, so it "
                            + "cannot be forgotten. Switch to another account first.")
                }
                let email = accounts.remove(at: index).email
                record("forget", Self.typed(label))
                return Changed(email: email, warnings: [])
            }
        }

        func rename(_ from: String, to: String) async throws -> Changed {
            try lock.withLock {
                guard let index = accounts.firstIndex(where: { $0.qualified == from }) else {
                    throw Self.failed("unknown_account", "There is no account called \(from).")
                }
                try requireUnused(to, of: accounts[index].provider)
                accounts[index] = with(accounts[index], label: to)
                record("rename", "\(Self.typed(from)) -> \(to)")
                return Changed(email: accounts[index].email, warnings: [])
            }
        }

        func signIn(_ label: String) async throws -> SignIn {
            let (provider, name) = Self.parts(of: label)
            try lock.withLock {
                if !accounts.contains(where: { $0.provider == provider && $0.label == name }) {
                    try requireUnused(name, of: provider)
                }
            }
            return FixtureSignIn(provider: provider) { [weak self] in
                self?.signedIn(name, of: provider)
                    ?? Enrolled(
                        email: "", outcome: .signedIn, warnings: [])
            }
        }

        /// Parks the Claude Desktop account in use and leaves Claude signed out, waiting for
        /// a sign-in to another account.
        func switchToSignedOut(_ tool: String) async throws -> Switched {
            try lock.withLock {
                guard tool == "desktop" else {
                    throw Self.failed(
                        "sign_out_unsupported", "Only Claude Desktop can be left signed out.")
                }
                let from = accounts.firstIndex { $0.provider == tool && $0.signedIn }
                if let from {
                    accounts[from] = with(accounts[from], signedIn: false, switchable: true)
                }
                let fromLabel = from.flatMap { accounts[$0].label }
                awaiting = Awaiting(
                    fromLabel: fromLabel, startedAt: Int64(Date().timeIntervalSince1970))
                record("sign-out", tool)
                return Switched(
                    outcome: .switched(
                        provider: tool, from: from.flatMap { accounts[$0].qualified } ?? "",
                        to: "", adoption: .nextLaunch(program: "Claude")),
                    warnings: [])
            }
        }

        func awaitingSignIn() async -> Awaiting? { lock.withLock { awaiting } }

        func liveUsage() async -> LiveUsageState { lock.withLock { live } }

        func enableLiveUsage() async throws -> LiveUsageState {
            lock.withLock {
                live = LiveUsageState(
                    enabled: true, approval: "granted", reason: nil,
                    lastOkAt: Int64(Date().timeIntervalSince1970))
                return live
            }
        }

        func disableLiveUsage() async throws -> LiveUsageState {
            lock.withLock {
                live = LiveUsageState(
                    enabled: false, approval: "unknown", reason: nil, lastOkAt: nil)
                return live
            }
        }

        func abandonRecovery() async throws -> Abandoned? {
            lock.withLock {
                guard stuck else { return nil }
                stuck = false
                record("abandon", "work -> personal")
                return Abandoned(from: "work", to: "personal", loginsKept: 2)
            }
        }

        func log(limit: UInt32) async -> [Change] {
            lock.withLock { Array(changes.suffix(Int(limit))) }
        }

        func renew() async -> [Renewed] {
            [Renewed(label: "work", provider: "claude", outcome: "renewed")]
        }

        func schedule() async -> Schedule {
            lock.withLock {
                scheduled
                    ? .installed(
                        path: "~/Library/LaunchAgents/com.usepitboard.renew.plist",
                        everySeconds: 86_400)
                    : .absent
            }
        }

        func scheduleInstall() async throws -> String {
            lock.withLock {
                scheduled = true
                return "~/Library/LaunchAgents/com.usepitboard.renew.plist"
            }
        }

        func scheduleUninstall() async throws -> Bool {
            lock.withLock {
                defer { scheduled = false }
                return scheduled
            }
        }

        func scheduleRepair() async throws -> Bool { false }

        func changedAt() async -> Int64 { lock.withLock { changedAt } }

        func readingsChangedAt() async -> Int64 { 1 }

        // MARK: Changing the state

        /// What finishing a sign-in enrols: a new account is parked beside the one in use,
        /// and one that was enrolled already has its parked login renewed.
        private func signedIn(_ name: String, of provider: String) -> Enrolled {
            lock.withLock {
                record("enroll", Self.typed("\(provider)/\(name)"))
                if let index = accounts.firstIndex(where: {
                    $0.provider == provider && $0.label == name
                }) {
                    // The account in use gets its new login in use, parked for nobody; any
                    // other has its parked login renewed.
                    let inUse = accounts[index].signedIn
                    accounts[index] = with(
                        accounts[index], switchable: !inUse, signedInAgain: true)
                    return Enrolled(
                        email: accounts[index].email,
                        outcome: inUse ? .inUse(again: true) : .renewed, warnings: [])
                }
                let email = "\(name)@example.com"
                accounts.append(
                    Self.account(
                        name, of: provider, email: email,
                        windows: [Self.window("five_hour", 3, length: 18_000, in: 18_000)]))
                return Enrolled(email: email, outcome: .signedIn, warnings: [])
            }
        }

        private func requireUnused(_ name: String, of provider: String) throws {
            if accounts.contains(where: { $0.provider == provider && $0.label == name }) {
                throw Self.failed(
                    "label_taken", "There is already an account called \(name).")
            }
        }

        private func record(_ verb: String, _ subject: String) {
            changedAt += 1
            changes.append(
                Change(
                    at: Date().formatted(.iso8601), caller: "app", verb: verb,
                    subject: subject, outcome: "ok"))
        }

        /// A label as the log writes it: bare for Claude Code, with its tool for any other.
        private static func typed(_ label: String) -> String {
            label.hasPrefix("claude/") ? String(label.dropFirst("claude/".count)) : label
        }

        private func currentStatus() -> Status {
            Status(now: Int64(Date().timeIntervalSince1970), accounts: accounts, warnings: [])
        }

        // MARK: What each fixture starts with

        static let tools = [
            Tool(code: "claude", name: "Claude Code", program: "claude", service: "Anthropic"),
            Tool(code: "codex", name: "Codex", program: "codex", service: "OpenAI"),
        ]

        static let desktop = Tool(
            code: "desktop", name: "Claude Desktop", program: "Claude", service: "Anthropic")

        private static func accounts(for fixture: Fixture, now: Date) -> [Account] {
            let work = account(
                "work", email: "dana@work.example", signedIn: true,
                windows: [
                    window("five_hour", 42, length: 18_000, in: 7_800),
                    window("seven_day", 12, length: 604_800, in: 356_000),
                ])
            let personal = account(
                "personal", email: "dana@home.example",
                windows: [
                    window("five_hour", 100, length: 18_000, in: 4_800),
                    window("seven_day", 61, length: 604_800, in: 190_000),
                ],
                parkedFor: 11 * 86_400)
            switch fixture {
            case .twoTools, .chatGPTOpen:
                let old = account(
                    "old", email: "dana@old.example", switchable: false, expired: true)
                let codexMain = account(
                    "main", of: "codex", email: "dana@work.example", signedIn: true,
                    windows: [
                        window("primary", 18, length: 18_000, in: 12_000),
                        window("secondary", 7, length: 604_800, in: 500_000),
                    ])
                let codexSpare = account(
                    "spare", of: "codex", email: "dana@home.example",
                    windows: [window("primary", 0, length: 18_000, in: 18_000)],
                    parkedFor: 20 * 86_400)
                return [work, personal, old, codexMain, codexSpare]
            case .oneTool, .readFailure, .stuck:
                return [work, personal]
            case .claudeDesktop:
                return [work, personal] + desktopAccounts(now: now)
            case .onlyOne:
                return [work]
            case .unnamed:
                return [account(nil, email: "dana@work.example", signedIn: true)]
            case .empty, .firstLaunch, .noClaudeCode:
                return []
            }
        }

        /// Claude Desktop's two accounts: `personal` in use, its numbers from Claude's own
        /// history since macOS stopped live usage, and `work` parked, its sign-in lapsing in
        /// two days.
        private static func desktopAccounts(now: Date) -> [Account] {
            let seconds = Int64(now.timeIntervalSince1970)
            let history = Usage(
                source: .desktopHistory, observedAt: seconds - 600,
                windows: [window("five_hour", 23, length: 18_000, in: 9_000)], verified: false)
            let inUse = Account(
                id: "desktop:personal", provider: "desktop", label: "personal",
                qualified: "desktop/personal", unplaced: false, email: "dana@home.example",
                accountUuid: "desktop-personal", signedIn: true, switchable: false,
                parked: nil, usage: history, stale: "live_usage_needs_approval",
                staleExplanation:
                    "macOS stopped letting Pitboard read Claude’s key, so these numbers come "
                    + "from Claude’s own history.",
                lastsSeconds: nil, lastsBurning: false)
            let parked = Account(
                id: "desktop:work", provider: "desktop", label: "work",
                qualified: "desktop/work", unplaced: false, email: "dana@work.example",
                accountUuid: "desktop-work", signedIn: false, switchable: true,
                parked: Parked(
                    parkedAt: seconds - 5 * 86_400, accessExpiresAt: nil,
                    refreshExpiresAt: seconds + 2 * 86_400),
                usage: nil, stale: nil, staleExplanation: nil, lastsSeconds: nil,
                lastsBurning: false)
            return [inUse, parked]
        }

        private static func history(now: Date) -> [Change] {
            [
                ("enroll", "work", "cli", 86_400 * 3),
                ("enroll", "personal", "app", 86_400 * 2),
                ("switch", "personal", "cli", 7_200), ("switch", "work", "app", 3_600),
            ].map { verb, subject, caller, ago in
                Change(
                    at: now.addingTimeInterval(-Double(ago)).formatted(.iso8601),
                    caller: caller, verb: verb, subject: subject, outcome: "ok")
            }
        }

        /// An account as the core reports one. `expired` is one whose parked login ran out a
        /// day ago, which the core says with a code and no explanation: the app's note comes
        /// from the parked login's expiry.
        static func account(
            _ label: String?, of provider: String = "claude", email: String,
            signedIn: Bool = false, switchable: Bool? = nil, expired: Bool = false,
            windows: [Limit] = [], parkedFor seconds: Int64? = nil
        ) -> Account {
            let now = Int64(Date().timeIntervalSince1970)
            let parked =
                expired
                ? Parked(
                    parkedAt: now - 31 * 86_400, accessExpiresAt: now - 30 * 86_400,
                    refreshExpiresAt: now - 86_400)
                : seconds.map {
                    Parked(
                        parkedAt: now - 3600, accessExpiresAt: nil, refreshExpiresAt: now + $0)
                }
            return Account(
                id: "\(provider):\(email):\(label ?? "")", provider: provider, label: label,
                qualified: label.map { "\(provider)/\($0)" }, unplaced: false, email: email,
                accountUuid: email, signedIn: signedIn,
                switchable: switchable ?? !signedIn, parked: parked,
                usage: windows.isEmpty
                    ? nil : Usage(source: .live, observedAt: now, windows: windows),
                stale: expired ? "parked_access_expired" : nil, staleExplanation: nil,
                lastsSeconds: signedIn && !windows.isEmpty ? 11_000 : nil,
                lastsBurning: signedIn)
        }

        static func window(
            _ kind: String, _ percent: Double, length: Int64, in seconds: Int64
        ) -> Limit {
            Limit(
                kind: kind, lengthSeconds: length, scope: nil, percent: percent,
                resetsAt: Int64(Date().timeIntervalSince1970) + seconds, severity: nil,
                isActive: true)
        }

        /// `signedInAgain` is a sign-in to the account: the core parks the new login for any
        /// account but the one in use, which keeps none, and says nothing is stale.
        private func with(
            _ account: Account, label: String? = nil, signedIn: Bool? = nil,
            switchable: Bool? = nil, signedInAgain: Bool = false
        ) -> Account {
            let label = label ?? account.label
            let inUse = signedIn ?? account.signedIn
            let now = Int64(Date().timeIntervalSince1970)
            // Codex states no expiry for its refresh token; Claude Code's lasts 30 days, and
            // so, as far as the fixture is concerned, does Claude Desktop's.
            let renewed: Parked? =
                inUse
                ? nil
                : Parked(
                    parkedAt: now, accessExpiresAt: nil,
                    refreshExpiresAt: account.provider == "codex" ? nil : now + 30 * 86_400)
            return Account(
                id: account.id, provider: account.provider, label: label,
                qualified: label.map { "\(account.provider)/\($0)" },
                unplaced: account.unplaced,
                email: account.email, accountUuid: account.accountUuid, signedIn: inUse,
                switchable: switchable ?? account.switchable,
                parked: signedInAgain ? renewed : account.parked, usage: account.usage,
                stale: signedInAgain ? nil : account.stale,
                staleExplanation: signedInAgain ? nil : account.staleExplanation,
                lastsSeconds: account.lastsSeconds, lastsBurning: account.lastsBurning)
        }

        private static func parts(of label: String) -> (provider: String, name: String) {
            let parts = label.split(separator: "/", maxSplits: 1).map(String.init)
            return parts.count == 2 ? (parts[0], parts[1]) : ("claude", label)
        }

        private static func failed(_ code: String, _ message: String) -> PitboardError {
            .Failed(code: code, cause: nil, message: message, warnings: [])
        }
    }

    /// A sign-in that prints what the tool prints and finishes the way the tool does.
    ///
    /// Claude Code's prints its address and then asks for the code the browser shows, as
    /// 2.1.289 does from the start whatever becomes of its callback, and finishes once one is
    /// pasted. Codex's prints its address and finishes on its own a moment later.
    final class FixtureSignIn: SignIn, @unchecked Sendable {
        private let lock = NSLock()
        private let wake = DispatchSemaphore(value: 0)
        private var lines: [String]
        private var ended = false
        private var cancelled = false
        private let code: Bool
        private let enrol: @Sendable () -> Enrolled

        init(provider: String, enrol: @escaping @Sendable () -> Enrolled) {
            code = provider == "claude"
            self.enrol = enrol
            lines =
                code
                ? [
                    "Opening your browser to sign in…\n",
                    "If it did not open: https://claude.ai/oauth/authorize?fixture=1\n",
                    "Paste code here if prompted > ",
                ]
                : [
                    "Starting local login server on http://localhost:1455.\n",
                    "If your browser did not open, navigate to this URL to authenticate:\n",
                    "https://auth.openai.com/oauth/authorize?fixture=1\n",
                ]
            super.init(noHandle: NoHandle())
        }

        required init(unsafeFromHandle handle: UInt64) { fatalError("not from the core") }

        override func nextLine() -> String? {
            let next: String? = lock.withLock { lines.isEmpty ? nil : lines.removeFirst() }
            if let next {
                Thread.sleep(forTimeInterval: 0.2)
                return next
            }
            // Codex finishes by itself; Claude Code waits for the code.
            if code {
                wake.wait()
            } else {
                _ = wake.wait(timeout: .now() + 1.5)
            }
            return nil
        }

        override func paste(line: String) throws {
            lock.withLock { ended = true }
            wake.signal()
        }

        override func finish() throws -> Enrolled {
            if lock.withLock({ cancelled }) {
                throw PitboardError.Failed(
                    code: "sign_in_cancelled", cause: nil, message: "The sign-in was stopped.",
                    warnings: [])
            }
            return enrol()
        }

        override func cancel() {
            lock.withLock { cancelled = true }
            wake.signal()
        }
    }
#endif
