import Foundation
import PitboardKit

@testable import Pitboard

/// The tools as the core lists them, written out so no test asks the core for them.
let claudeCode = Tool(
    code: "claude", name: "Claude Code", program: "claude", service: "Anthropic")
let codex = Tool(code: "codex", name: "Codex", program: "codex", service: "OpenAI")
let bothTools = [claudeCode, codex]

func window(
    _ kind: String, _ percent: Double, resets: Int64? = 100, scope: String? = nil,
    active: Bool = true, length: Int64? = nil
) -> Limits {
    Limits(
        kind: kind, lengthSeconds: length, scope: scope, percent: percent, resetsAt: resets,
        severity: nil, isActive: active)
}

/// An account as the core reports one. `label` nil is a login signed in and not enrolled.
/// Switchable unless it is the one signed in, as a real one is.
func account(
    _ label: String?, of provider: String = "claude", signedIn: Bool = false,
    switchable: Bool? = nil, uuid: String? = nil, _ windows: [Limits] = []
) -> Account {
    let uuid = uuid ?? label ?? "someone"
    return Account(
        id: "\(provider):\(uuid)", provider: provider, label: label,
        qualified: label.map { "\(provider)/\($0)" }, unplaced: false,
        email: "\(label ?? uuid)@example.com", accountUuid: uuid, signedIn: signedIn,
        switchable: switchable ?? (!signedIn && label != nil), parked: nil,
        usage: Usage(source: .live, observedAt: 0, windows: windows), stale: nil,
        staleExplanation: nil, lastsSeconds: nil, lastsBurning: false)
}

/// A tool's login that belongs to no account pitboard can name, as the core reports one:
/// no label, no email, no account id, and what is wrong with it.
func unplaced(of provider: String, signedIn: Bool = false) -> Account {
    Account(
        id: "\(provider):login", provider: provider, label: nil, qualified: nil,
        unplaced: true, email: "", accountUuid: "", signedIn: signedIn, switchable: false,
        parked: nil, usage: nil, stale: "login_unreadable",
        staleExplanation: "Codex's login could not be read; run `pitboard doctor`",
        lastsSeconds: nil, lastsBurning: false)
}

func status(_ accounts: [Account], warnings: [Warning] = []) -> Status {
    Status(now: 0, accounts: accounts, warnings: warnings)
}

/// Stands in for AppleScript: keeps each script it is handed and raises what it is told to,
/// so no test runs one that asks for an administrator's password.
final class Scripts: @unchecked Sendable {
    // Unchecked because the tool runs scripts off the main thread; every read and write
    // holds `lock`.
    private let lock = NSLock()
    private var kept: [String] = []
    private var raising: NSDictionary?

    /// Every script handed in, oldest first.
    var ran: [String] { lock.withLock { kept } }

    /// What each script from now on raises: AppleScript's error number, and its message.
    func raise(_ number: Int?, _ message: String? = nil) {
        var error: [String: Any] = [:]
        error[NSAppleScript.errorNumber] = number
        error[NSAppleScript.errorMessage] = message
        lock.withLock { raising = number == nil ? nil : error as NSDictionary }
    }

    func run(_ source: String) -> NSDictionary? {
        lock.withLock {
            kept.append(source)
            return raising
        }
    }
}
