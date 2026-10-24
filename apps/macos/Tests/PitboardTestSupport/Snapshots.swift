import PitboardKit

package func account(
    _ label: String, of provider: String = "claude", email: String? = nil,
    signedIn: Bool = false
) -> Account {
    Account(
        id: "\(provider):\(label)", provider: provider, label: label,
        qualified: "\(provider)/\(label)", unplaced: false,
        email: email ?? "\(label)@example.com", accountId: label, signedIn: signedIn,
        switchable: !signedIn, parked: nil,
        usage: Usage(
            source: .live, observedAt: 0, windows: [], listsEveryLimit: provider == "claude"),
        stale: nil, staleExplanation: nil, plan: nil, readAt: nil)
}

package func status(_ accounts: [Account]) -> Status {
    Status(now: 0, accounts: accounts, warnings: [], awaiting: [])
}

package func status(_ labels: String...) -> Status {
    status(labels.enumerated().map { account($1, signedIn: $0 == 0) })
}

extension Site {
    // A site as a snapshot carries one, for a test of what the app does with it.
    package static let claude = site("claude.ai", of: "claude")
    package static let chatGPT = site("chatgpt.com", of: "codex")

    private static func site(_ host: String, of provider: String) -> Site {
        Site(
            host: host, name: host, provider: provider, aliases: [], hosts: [host],
            signInHosts: [], blockedHosts: [], signInPaths: [], storeName: provider,
            signInSteps: "", blockedServices: "")
    }
}

package func window(
    _ label: String, on site: Site = .claude, email: String? = nil, inUse: Bool = false,
    store: String
) -> WindowAccount {
    WindowAccount(
        site: site, label: label, email: email ?? "\(label)@example.com", store: store,
        inUse: inUse, title: label)
}

package func windowsShown(
    _ accounts: [WindowAccount] = [], menus: [SiteMenu] = [], closing: [String] = [],
    deleting: [StoreDeletion] = []
) -> AccountWindowsShown {
    AccountWindowsShown(
        accounts: accounts, menus: menus, open: [],
        waiting: WindowWaiting(
            windowTitle: "Account", shown: .reading(title: "Reading accounts…")),
        closing: closing, deleting: deleting, picker: nil, downloads: [])
}

// What a model that has read nothing yet would make, apart from what a test says. `now`
// moves with `revision`, as each of a real model's reads comes later than the last.
package func snapshot(
    _ revision: UInt64, status: Status? = nil, reading: Bool = false,
    windows: AccountWindowsShown = windowsShown(), warming: [WarmAccountShown] = []
) -> Snapshot {
    Snapshot(
        revision: revision, now: Int64(revision), reading: reading, updatedAt: nil,
        status: status, warnings: [], readFailure: nil, stuck: false, installed: nil,
        switchUnderWay: nil, quitQuestion: nil, replaceQuestion: nil, lastSwitches: [],
        abandoned: nil, failure: nil,
        windowRequest: WindowRequest(serial: 0, pane: nil), signingIn: nil, sheet: nil,
        sheetFailure: nil,
        menuBar: MenuBarText(
            nameAndUsage: "", usage: "", spoken: "Pitboard", level: nil, attention: false,
            tip: "Pitboard"),
        sections: [],
        accountMenu: AccountMenuShown(
            offers: ["Use", "Sign In Again…", "Rename…"], forget: "Forget…"),
        showsTools: false, notices: [],
        menuNotices: MenuNotices(install: nil, switches: [], others: nil), footing: .ready,
        setup: nil, accountsShown: .reading(title: "Reading accounts…"),
        menuAccountsNote: nil, updatedMenu: "", updatedWindow: "", sheetText: nil,
        signingInText: nil, stowText: nil, quitConfirmation: nil, failureAlert: nil,
        machine: MachineShown(
            schedule: ScheduleShown(
                schedule: nil, on: false, changing: false, enabled: true, note: nil,
                failed: nil,
                about: ""),
            checks: ChecksShown(
                lines: [], summary: nil, level: .ok, checking: false, checked: nil,
                waiting: nil),
            activity: ActivityShown(lines: [], empty: nil),
            commandLine: CommandLineShown(
                found: nil, inTerminal: nil, updateNote: nil, offersLink: false,
                cannotLink: nil),
            atLimit: AtLimitShown(
                title: "Before an account runs out", choice: .notify,
                choices: [
                    AtLimitChoice(choice: .nothing, title: "Do Nothing"),
                    AtLimitChoice(choice: .notify, title: "Notify Me"),
                    AtLimitChoice(choice: .switch, title: "Switch Automatically"),
                ], at: 95, lowest: 50, highest: 99, enabled: true, shareEnabled: true,
                heading: "When a limit reaches", atLabel: "When a limit reaches 95%",
                shareLabel: "Share of a limit", note: nil,
                standing: nil, terms: nil),
            warm: WarmShown(
                title: "Warm up limits", from: 360, to: 1320, rangeLabel: "06:00 to 22:00",
                accounts: warming,
                noneEnrolled: warming.isEmpty ? "No Claude Code account is enrolled yet." : nil,
                standing: nil, note: "", enabled: false,
                terms: TermsShown(
                    sentence: "Anthropic's terms may cover automatic switching and warm-ups.",
                    linkTitle: "Is this allowed?",
                    link: "https://docs.usepitboard.com/concepts/limits#is-this-allowed"))),
        accountWindows: windows)
}
