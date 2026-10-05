import AppKit
import PitboardKit
import SwiftUI

/// The menu the menu bar item opens.
///
/// A menu and not a popover, as the platform asks of a menu bar item: it opens at once,
/// closes predictably, reads well to VoiceOver, and works from the keyboard. It is the
/// glance and the switch: which account is in use in each tool, what each has left, and
/// anything that needs attention. Everything that needs typing or room happens in the
/// window, which every item here that needs one opens.
struct MenuBarContent: View {
    let model: AppModel
    let windows: AccountWindows
    let updates: any Updates
    @Environment(\.openURL) private var openURL
    @Environment(\.openSettings) private var openSettings

    var body: some View {
        attention
        accounts
        Section {
            Button("Add Account…") { model.present(.add(provider: nil)) }
                .keyboardShortcut("n")
            // An add left halfway leaves Claude signed out, which is worth a way back.
            if model.desktopAwaiting != nil {
                Button("Finish Adding a Claude Account…") { model.finishDesktopAddAsked() }
            }
            Button {
                Task { await model.refresh(asked: true) }
            } label: {
                Text("Refresh")
                Text(updated)
            }
            .keyboardShortcut("r")
            .disabled(model.reading)
        }
        if !windows.menus.isEmpty {
            Section {
                SiteMenuItems(windows: windows)
            }
        }
        Section {
            Button("Open Pitboard") { model.showWindow() }
                .keyboardShortcut("0")
            Button("Settings…") {
                // Choosing an item of a menu bar item's menu does not make the app active,
                // and settings already open would come forward behind the app in front.
                windows.presence.activate()
                openSettings()
            }
            .keyboardShortcut(",")
            if updates.available {
                Button("Check for Updates…") { updates.check() }
            }
        }
        Section {
            Button("About Pitboard") {
                NSApp.activate()
                NSApp.orderFrontStandardAboutPanel(nil)
            }
            Link("Pitboard Help", destination: Links.documentation)
            Button("Quit Pitboard") { NSApp.terminate(nil) }
                .keyboardShortcut("q")
        }
    }

    // MARK: - What needs attention

    /// Advice to switch, as items that switch; anything else to know about, as one item
    /// that opens the window where it is said in full; an update that is ready; and the one
    /// thing to do on a machine that is not set up.
    @ViewBuilder private var attention: some View {
        let notices = model.notices().filter { $0.severity != .info }
        let advice = notices.filter { $0.switchesTo != nil }
        let others = notices.filter { $0.switchesTo == nil }
        let waiting = updates.available ? updates.waiting : nil
        if !notices.isEmpty || waiting != nil || model.footing == .noClaudeCode {
            Section {
                if model.footing == .noClaudeCode {
                    Button {
                        openURL(Links.installClaudeCode)
                    } label: {
                        Image(systemName: "questionmark.circle")
                        Text("Claude Code isn’t installed")
                        Text("Learn how to install it")
                    }
                }
                ForEach(advice) { notice in
                    if let (qualified, label) = notice.switchesTo {
                        Button {
                            Task { await model.switchAsked(to: qualified) }
                        } label: {
                            Image(systemName: Symbol.switchAccount)
                            Text("Switch to \(label)")
                            Text(notice.title)
                        }
                        .disabled(model.switchUnderWay != nil)
                    }
                }
                if let first = others.first {
                    Button {
                        model.showWindow(.accounts)
                    } label: {
                        Image(systemName: first.severity.symbol)
                        Text(
                            others.count == 1
                                ? first.title : "\(others.count) things to look at")
                        Text(others.count == 1 ? "Show in Pitboard" : first.title)
                    }
                    .help(others.count == 1 ? first.lines.joined(separator: " ") : "")
                }
                if let waiting {
                    Button {
                        updates.check()
                    } label: {
                        Image(systemName: Symbol.update)
                        Text("Install Pitboard \(waiting)…")
                    }
                }
            }
        }
    }

    // MARK: - Accounts

    /// A section per tool once there is more than one, headed with its name. The account in
    /// use in each is checked, and choosing another switches to it.
    @ViewBuilder private var accounts: some View {
        if model.status == nil {
            Section {
                Text(model.problem == nil ? "Reading accounts…" : "No accounts to show")
            }
        } else if model.groups.isEmpty {
            if model.footing != .noClaudeCode {
                Section {
                    Text("No accounts yet")
                }
            }
        } else {
            ForEach(model.groups) { group in
                if let name = group.name {
                    Section(name) { items(of: group) }
                } else {
                    Section { items(of: group) }
                }
            }
        }
    }

    private func items(of group: AccountGroup) -> some View {
        ForEach(group.accounts, id: \.id) { account in
            AccountMenuItem(
                description: AccountDescription(
                    account, switching: model.switchUnderWay, busy: model.signingIn != nil),
                perform: perform)
        }
    }

    private func perform(_ action: AccountAction) {
        switch action {
        case .use(let qualified):
            Task { await model.switchAsked(to: qualified) }
        case .signInAgain(let provider, let label):
            model.present(.signInAgain(provider: provider, label: label))
        case .name(let provider, let email):
            model.present(.name(provider: provider, email: email))
        case .none:
            break
        }
    }

    /// When the numbers were read, as a time rather than an age: a menu can stay open, and
    /// "just now" would still say so ten minutes later.
    private var updated: String {
        if model.reading { return "Reading…" }
        if let at = model.updatedAt { return "Updated \(clockTime(at))" }
        return model.problem == nil ? "Not read yet" : "Showing the last numbers measured"
    }
}

/// One account in the menu: its name, what its limits stand at, and a check mark when it is
/// the one in use. Choosing it does what pressing it means: switch, sign in again, or name.
private struct AccountMenuItem: View {
    let description: AccountDescription
    let perform: (AccountAction) -> Void

    var body: some View {
        Toggle(
            isOn: Binding(
                get: { description.inUse },
                // Choosing it does what pressing it means, whichever way the check mark
                // would go: the one in use stays in use until another is chosen, and a
                // login in use with no name yet is named from its own checked item.
                set: { _ in perform(description.action) })
        ) {
            Text(description.title)
            Text(description.summary)
        }
        .disabled(!description.inUse && description.action == .none)
        .help(description.problem ?? description.staleNote ?? "")
    }
}

extension Notice {
    /// The account advice offers, when this is advice.
    var switchesTo: (qualified: String, label: String)? {
        for action in actions {
            if case .use(let qualified, let label) = action { return (qualified, label) }
        }
        return nil
    }
}

/// Addresses the app links to, named once.
enum Links {
    static let documentation = URL(string: "https://docs.usepitboard.com")!
    static let installClaudeCode = URL(
        string: "https://docs.claude.com/en/docs/claude-code/setup")!
}
