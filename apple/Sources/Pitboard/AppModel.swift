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
    private(set) var status: Status?
    private(set) var problem: String?
    /// The account a switch is running for, so its row can say so.
    private(set) var switching: String?
    private(set) var updatedAt: Date?
    /// An account has run out and another has room. Shown in the panel whether or not
    /// notifications are allowed, so the advice does not depend on a permission.
    private(set) var advice: Advice?
    /// When sessions that were already open will have picked up the last switch.
    private(set) var adopted: Date?
    /// Every check pitboard makes about this machine, once someone asks for them.
    private(set) var checks: [Check] = []
    /// A label being typed, when the panel is asking for one, and what it is for.
    var naming: Naming?
    /// A sign-in in progress, and everything Claude Code has said about it.
    private(set) var signingIn: SigningIn?
    /// Everything that went wrong on the way, not only the first of them. A switch can warn
    /// about an overriding environment variable and a config that did not update at once,
    /// and showing one of those and dropping the other is how a person fixes the wrong
    /// thing.
    private(set) var warnings: [Warning] = []
    /// An interrupted switch nothing can finish, which the panel offers a way out of.
    private(set) var stuck: Bool = false

    enum Naming: Equatable {
        /// Record the account signed in now: no browser, so the app does it itself.
        case theOneInUse
        /// A different account, which means Claude Code's own sign-in in a browser.
        case another
    }

    /// Usage is asked of Anthropic for every account, so it is asked sparingly: on opening the
    /// menu when the numbers are a minute old, and in the background every five minutes.
    static let staleAfter: TimeInterval = 60
    private static let refreshEvery: Duration = .seconds(300)

    private let notifier = Notifier()

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
        service: any Core = PitboardService(settings: .forCurrentUser())
    ) {
        self.service = service
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
    /// known: no network, no keychain, and no request of Anthropic.
    private func noticeOtherChanges() async {
        let now = await service.changedAt()
        let seen = lastChangedAt
        lastChangedAt = now
        // The first look only records where things stand; there is nothing to compare to.
        guard let seen, seen != now, let read = try? await service.statusOffline() else {
            return
        }
        status = read
        // A switch made somewhere else is not the one this app is counting down for.
        adopted = nil
    }

    /// The account in use and its tightest limit, as the menu bar reads it.
    var title: String { menuTitle(for: status) }

    var updated: String {
        guard let updatedAt else { return "not read yet" }
        let ago = Int(Date().timeIntervalSince(updatedAt))
        return ago < 60 ? "updated just now" : "updated \(ago / 60)m ago"
    }

    /// `asked` means somebody asked for this reading rather than a timer producing it, and
    /// is what tells the core to go to Anthropic whatever it read moments ago.
    func refresh(ifOlderThan seconds: TimeInterval = 0, asked: Bool = false) async {
        if let updatedAt, Date().timeIntervalSince(updatedAt) < seconds { return }
        do {
            let read = try await service.status(fresh: asked)
            status = read
            warnings = read.warnings
            problem = read.warnings.first?.message
            stuck = read.warnings.contains { $0.code == "recovery_undetermined" }
            updatedAt = Date()
            lastChangedAt = await service.changedAt()
            advice = Advice.about(read, unless: notifier.told)
            if let advice { notifier.tell(advice) }
        } catch {
            problem = Self.saying(error)
            // A read that could not reach Anthropic still has something true to show: the
            // last numbers measured, and who Claude Code's config says is signed in. An
            // empty panel says the accounts are gone, which is not what happened.
            if status == nil, let known = try? await service.statusOffline() {
                status = known
            }
            stuck = Self.code(of: error) == "recovery_undetermined"
        }
    }

    /// Give up on an interrupted switch that cannot be finished, keeping every login. The
    /// way out when recovery cannot reach Anthropic, which used to mean opening a terminal.
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

    func use(_ label: String) async {
        switching = label
        defer { switching = nil }
        do {
            let done = try await service.switchTo(label)
            if case .switched(_, _, let ceiling) = done.outcome {
                adopted = Date().addingTimeInterval(TimeInterval(ceiling))
            }
            advice = nil
            updatedAt = nil
            await refresh()
        } catch {
            problem = Self.saying(error)
        }
    }

    /// The account signed in now, if it is not enrolled. Everything else about adding an
    /// account needs a browser and somewhere to print what Claude Code says, which is the
    /// command line's job.
    var unenrolled: Bool {
        status?.accounts.contains { $0.signedIn && $0.label == nil } ?? false
    }

    /// Records the account signed in now under a name.
    func enrol(as label: String) async {
        naming = nil
        do {
            _ = try await service.enrollCurrent(label)
            updatedAt = nil
            await refresh()
        } catch {
            problem = Self.saying(error)
        }
    }

    /// Runs Claude Code's own sign-in and shows what it says. It opens the browser itself
    /// and finishes through a loopback callback, so there is nothing to hand a terminal;
    /// stdin is only its fallback, which is what the code field is for.
    func signIn(as label: String) async {
        naming = nil
        signingIn = SigningIn(label: label)
        do {
            let session = try await service.signIn(label)
            signingIn?.session = session
            await watch(session)
        } catch {
            signingIn = nil
            problem = Self.saying(error)
        }
    }

    /// Reads what Claude Code says until it stops, then records what it signed in to.
    private func watch(_ session: SignIn) async {
        while let said = await Task.detached(
            priority: .utility,
            operation: {
                session.nextLine()
            }
        ).value {
            signingIn?.add(said)
        }
        do {
            _ = try await Task.detached(priority: .utility) { try session.finish() }.value
            signingIn = nil
            updatedAt = nil
            await refresh()
        } catch {
            signingIn = nil
            problem = Self.saying(error)
        }
    }

    /// Types the fallback code back, for a browser that could not reach the callback.
    func paste(_ code: String) {
        guard let session = signingIn?.session else { return }
        try? session.paste(line: code)
        signingIn?.pasted = true
    }

    func cancelSignIn() {
        signingIn?.session?.cancel()
        signingIn = nil
    }

    /// Drops an account and the login parked for it.
    func forget(_ label: String) async {
        do {
            _ = try await service.forget(label)
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

    /// The stable code behind an error, for deciding what to offer rather than reading the
    /// wording of a message.
    private static func code(of error: Error) -> String? {
        if case PitboardError.Failed(let code, _, _, _) = error {
            return code
        }
        return nil
    }
}

/// A sign-in as the panel sees it: the name it will be enrolled under, what Claude Code
/// has said so far, and the session to type back to.
@MainActor
@Observable
final class SigningIn {
    let label: String
    private(set) var said = ""
    var pasted = false
    @ObservationIgnored var session: SignIn?

    init(label: String) {
        self.label = label
    }

    func add(_ text: String) {
        said += text
    }

    /// The address Claude Code printed, for a browser that did not open by itself.
    var url: URL? {
        guard let found = said.range(of: "https://[^ \n\"]+", options: .regularExpression)
        else {
            return nil
        }
        return URL(string: String(said[found]))
    }

    /// Claude Code asks for a code only when its callback could not be reached.
    var wantsCode: Bool { said.contains("Paste code") && !pasted }
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
func menuTitle(for status: Status?) -> String {
    guard let account = status?.accounts.first(where: \.signedIn) else { return "" }
    // Long labels are bounded, because this sits in a bar someone else also wants space in.
    let full = account.label ?? "unenrolled"
    let name = full.count > 12 ? full.prefix(11) + "…" : full[...]
    guard let tightest = headline(of: account.usage?.windows ?? []) else {
        return String(name)
    }
    return "\(name) \(Int(tightest.percent.rounded()))%"
}
