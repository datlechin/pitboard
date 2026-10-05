import AppKit
import PitboardKit
import SwiftUI

/// Every account, a section per tool, with what Pitboard has to say above them.
struct AccountsPane: View {
    @Bindable var model: AppModel
    let windows: AccountWindows
    @State private var selection: String?
    @State private var forgetting: Account?
    @State private var givingUp = false
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        content
            .navigationTitle("Accounts")
            .navigationSubtitle(updated)
            .toolbar {
                ToolbarItemGroup {
                    Button {
                        Task { await model.refresh(asked: true) }
                    } label: {
                        Label("Refresh", systemImage: Symbol.refresh)
                    }
                    .help("Read every account’s usage again")
                    .disabled(model.reading)
                    // Command-N is the window's own command, so it works from every pane.
                    Button {
                        model.present(.add(provider: nil))
                    } label: {
                        Label("Add Account", systemImage: Symbol.add)
                    }
                    .help("Sign in to another account and park its login")
                }
            }
            .focusedSceneValue(
                \.refresh,
                RefreshCommand(title: "Refresh", disabled: model.reading) {
                    Task { await model.refresh(asked: true) }
                }
            )
            .task { await model.refresh(ifOlderThan: AppModel.staleAfter) }
            .alert(
                "Forget “\(forgetting.map(model.name(of:)) ?? "")”?",
                isPresented: Binding(
                    get: { forgetting != nil }, set: { if !$0 { forgetting = nil } }),
                presenting: forgetting
            ) { account in
                Button("Forget", role: .destructive) { forget(account) }
                Button("Cancel", role: .cancel) {}
            } message: { account in
                Text(forgetMessage(account: account, accounts: model.status?.accounts ?? []))
            }
            .alert("Give up on the interrupted switch?", isPresented: $givingUp) {
                Button("Give Up", role: .destructive) {
                    Task { model.present(await model.abandonStuckSwitch()) }
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text(
                    "Every login is kept, and nothing is deleted. Pitboard stops trying to finish it."
                )
            }
    }

    @ViewBuilder private var content: some View {
        switch model.footing {
        case .noClaudeCode:
            ContentUnavailableView {
                Label("Claude Code Isn’t Installed", systemImage: Symbol.terminal)
            } description: {
                Text(
                    "Pitboard switches the logins of Claude Code and Codex, so there is "
                        + "nothing for it to do until one of them is installed and signed in "
                        + "once.")
            } actions: {
                Link("How to Install Claude Code", destination: Links.installClaudeCode)
                    .buttonStyle(.borderedProminent)
            }
        case _ where model.problem != nil && model.status?.accounts.isEmpty != false:
            // Nothing to list, and the read said why: that is the thing to say, in full,
            // with a way to try again, and not a spinner that never stops.
            ContentUnavailableView {
                Label("Couldn’t Read Accounts", systemImage: Notice.Severity.error.symbol)
            } description: {
                Text(model.problem ?? "")
            } actions: {
                Button("Try Again") { Task { await model.refresh(asked: true) } }
                    .disabled(model.reading)
            }
        case .noOneSignedIn:
            ContentUnavailableView {
                Label("No Accounts", systemImage: Symbol.accounts)
            } description: {
                Text(
                    "Sign in once here and Pitboard parks that login, so signing in to "
                        + "another account doesn’t cost you the first.")
            } actions: {
                Button("Add Account…") { model.present(.add(provider: nil)) }
                    .buttonStyle(.borderedProminent)
            }
        default:
            if model.status == nil {
                ProgressView("Reading accounts…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                list
            }
        }
    }

    private var list: some View {
        List(selection: $selection) {
            ForEach(model.notices()) { notice in
                NoticeRow(
                    notice: notice, switching: model.switchUnderWay != nil, perform: perform
                )
                .selectionDisabled()
            }
            SetupTip(model: model)
            ForEach(model.groups) { group in
                // A heading per tool once there is more than one, and no section at all
                // before: a section with no heading still takes a heading's room.
                if let name = group.name {
                    Section(name) { rows(of: group) }
                } else {
                    rows(of: group)
                }
            }
        }
        .listStyle(.inset)
        .contextMenu(forSelectionType: String.self) { ids in
            if let account = account(ids.first) {
                menu(for: account)
            }
        } primaryAction: { ids in
            if let account = account(ids.first) {
                perform(description(of: account).action)
            }
        }
        .onDeleteCommand {
            if let account = account(selection), canForget(account) {
                forgetting = account
            }
        }
    }

    private func rows(of group: AccountGroup) -> some View {
        ForEach(group.accounts, id: \.id) { account in
            AccountRow(
                account: account, description: description(of: account),
                spokenName: model.name(of: account), perform: perform
            )
            .tag(account.id)
        }
    }

    // MARK: - What can be done to an account

    @ViewBuilder private func menu(for account: Account) -> some View {
        let said = description(of: account)
        if case .use = said.action {
            Button("Use \(account.label ?? "")") { perform(said.action) }
        }
        if case .name = said.action {
            Button("Name…") { perform(said.action) }
        }
        if let label = account.label, !account.unplaced {
            Button("Sign In Again…") {
                model.present(.signInAgain(provider: account.provider, label: label))
            }
            .disabled(model.signingIn != nil)
            Button("Rename…") {
                model.present(.rename(provider: account.provider, label: label))
            }
        }
        let sites = windowsOf(account: account, accounts: model.status?.accounts ?? [])
        if !sites.isEmpty {
            Divider()
            ForEach(sites) { window in
                Button("Open \(window.site.name)") {
                    windows.presence.activate()
                    openWindow(id: AccountWindowScene.id, value: window.id)
                }
            }
        }
        if !account.email.isEmpty {
            Divider()
            Button("Copy Email Address") {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(account.email, forType: .string)
            }
        }
        if canForget(account) {
            Divider()
            Button("Forget…", role: .destructive) { forgetting = account }
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

    private func perform(_ action: Notice.Action) {
        switch action {
        case .use(let qualified, _):
            Task { await model.switchAsked(to: qualified) }
        case .dismissSwitch(let provider):
            model.forgetSwitch(of: provider)
        case .giveUp:
            givingUp = true
        case .dismissAbandoned:
            model.forgetAbandoned()
        case .allowLiveUsage:
            model.liveUsageAsked()
        case .finishDesktopAdd:
            model.finishDesktopAddAsked()
        }
    }

    /// Only an enrolled account that is not the one in use: forgetting that one would throw
    /// away the only record of who is signed in, and the core refuses it.
    private func canForget(_ account: Account) -> Bool {
        account.label != nil && !account.signedIn
    }

    private func forget(_ account: Account) {
        guard let qualified = account.qualified else { return }
        Task { model.present(await model.forget(qualified)) }
    }

    private func account(_ id: String?) -> Account? {
        guard let id else { return nil }
        return model.status?.accounts.first { $0.id == id }
    }

    private func description(of account: Account) -> AccountDescription {
        AccountDescription(
            account, switching: model.switchUnderWay, busy: model.signingIn != nil)
    }

    private var updated: String {
        if model.reading { return "Reading…" }
        guard let at = model.updatedAt else { return "" }
        return "Updated \(clockTime(at))"
    }
}

/// The one next thing to do on a machine that is not set up yet, above its accounts.
///
/// Somebody who opens the app has usually never run a command and may never want to. The
/// nudge to add a second account is true but not urgent: somebody may keep one account on
/// purpose and watch its limits, so it can be declined, for its tool alone.
private struct SetupTip: View {
    let model: AppModel

    var body: some View {
        switch model.footing {
        case .unnamed(let provider, let email):
            Tip(
                symbol: "tag",
                title: "Give this account a name",
                detail: "\(whoIsSignedIn(email)) is signed in\(to(provider)). Pitboard parks "
                    + "logins under a name you choose, and can’t park this one until it has "
                    + "one."
            ) {
                Button("Name…") { model.present(.name(provider: provider, email: email)) }
                    .buttonStyle(.borderedProminent)
            }
        case .onlyOne(let provider, let label):
            Tip(
                symbol: Symbol.switchAccount,
                title: "Add a second \(tool(provider))account",
                detail: "\(label) is the only \(tool(provider))account Pitboard knows, so "
                    + "there’s nothing to switch to. Adding another signs in to it and "
                    + "parks its login beside this one."
            ) {
                Button("Add Account…") { model.present(.add(provider: provider)) }
                    .buttonStyle(.borderedProminent)
                Button("Not Now") { model.declineSecondAccount(for: provider) }
            }
        default:
            EmptyView()
        }
    }

    /// " to Codex", once accounts of more than one tool are shown, and nothing before.
    private func to(_ provider: String) -> String {
        model.showsTools ? " to \(model.tool(provider)?.name ?? provider)" : ""
    }

    /// "Codex ", once accounts of more than one tool are shown, and nothing before.
    private func tool(_ provider: String) -> String {
        model.showsTools ? "\(model.tool(provider)?.name ?? provider) " : ""
    }
}

/// One step, said once: a symbol, a line, the reason, and what to press.
private struct Tip<Actions: View>: View {
    let symbol: String
    let title: String
    let detail: String
    @ViewBuilder let actions: Actions

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: Design.iconSpacing) {
            Image(systemName: symbol)
                .foregroundStyle(.tint)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: Design.rowSpacing) {
                Text(title).fontWeight(.medium)
                Text(detail).explanatory()
                HStack { actions }
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 4)
        .selectionDisabled()
    }
}
