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
