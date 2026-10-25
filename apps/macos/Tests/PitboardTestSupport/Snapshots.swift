import PitboardKit

extension Site {
    // pitboard-sites' sites, with the hosts and paths a snapshot carries.
    package static let claude = Site(
        host: "claude.ai", name: "claude.ai", provider: "claude", aliases: [],
        hosts: ["claude.ai"], signInHosts: [], blockedHosts: ["accounts.google.com"],
        signInPaths: [["magic-link"]], storeName: "claude", signInSteps: "",
        blockedServices: "")
    package static let chatGPT = Site(
        host: "chatgpt.com", name: "chatgpt.com", provider: "codex",
        aliases: ["chat.openai.com", "www.chatgpt.com", "chat.com"],
        hosts: ["chatgpt.com", "chat.openai.com", "www.chatgpt.com", "chat.com"],
        signInHosts: [
            "auth.openai.com", "login.live.com", "login.microsoftonline.com",
            "appleid.apple.com",
        ],
        blockedHosts: ["accounts.google.com"], signInPaths: [["api", "auth"]],
        storeName: "codex", signInSteps: "", blockedServices: "")
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
        closing: closing, deleting: deleting, picker: nil, downloads: [],
        noLink: EmptyList(
            title: "No Link to Open",
            detail: "Share a claude.ai or chatgpt.com page with Pitboard from your browser’s "
                + "Share menu to open it as one of your accounts."))
}

// What a model that has read nothing yet would make, apart from what a test says.
package func snapshot(
    _ revision: UInt64, updated: String = "", reading: Bool = false,
    windows: AccountWindowsShown = windowsShown(), warming: [WarmAccountShown] = []
) -> Snapshot {
    Snapshot(
        revision: revision, reading: reading, quitQuestion: nil, replaceQuestion: nil,
        windowRequest: WindowRequest(serial: 0, pane: nil), signingIn: nil, sheet: nil,
        sheetFailure: nil,
        menuBar: MenuBarText(
            nameAndUsage: "", usage: "", spoken: "Pitboard", level: nil, attention: false,
            tip: "Pitboard"),
        sections: [],
        accountMenu: AccountMenuShown(
            offers: ["Use", "Sign In Again…", "Rename…"], forget: "Forget…"),
        notices: [],
        menuNotices: MenuNotices(install: nil, switches: [], others: nil), footing: .ready,
        setup: nil, accountsShown: .reading(title: "Reading accounts…"),
        menuAccountsNote: nil, updatedAt: nil, updatedMenu: updated, updatedWindow: updated,
        sheetText: nil,
        signingInText: nil, stowText: nil, failureAlert: nil,
        machine: MachineShown(
            schedule: ScheduleShown(
                on: false, enabled: true, note: nil, failed: nil, about: ""),
            checks: ChecksShown(
                lines: [], summary: nil, level: .ok, checking: false, checked: nil,
                waiting: nil),
            activity: ActivityShown(lines: [], empty: nil),
            commandLine: CommandLineShown(
                found: nil, inTerminal: nil, updateNote: nil, offersLink: false,
                cannotLink: nil, terminalNote: nil),
            atLimit: AtLimitShown(
                title: "Before an account runs out", choice: .notify,
                choices: [
                    AtLimitChoice(choice: .nothing, title: "Do Nothing"),
                    AtLimitChoice(choice: .notify, title: "Notify Me"),
                    AtLimitChoice(choice: .switch, title: "Switch Automatically"),
                ], at: 95, lowest: 50, highest: 99, enabled: true, shareEnabled: true,
                heading: "When a limit reaches", atLabel: "When a limit reaches 95%",
                shareLabel: "Share of a limit", shareRange: "From 50% to 99%", note: nil,
                standing: nil, terms: nil),
            warm: WarmShown(
                title: "Warm up limits", from: 360, to: 1320, rangeLabel: "06:00 to 22:00",
                accounts: warming,
                noneEnrolled: warming.isEmpty ? "No Claude Code account is enrolled yet." : nil,
                standing: nil, note: "", enabled: false,
                terms: TermsShown(
                    sentence: "Anthropic’s terms may cover automatic switching and warm-ups.",
                    linkTitle: "Is this allowed?",
                    link: "https://docs.usepitboard.com/concepts/limits#is-this-allowed"))),
        accountWindows: windows)
}
