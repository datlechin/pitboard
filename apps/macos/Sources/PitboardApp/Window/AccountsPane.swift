import PitboardKit
import SwiftUI

struct AccountsPane: View {
    let model: AppModel
    let windows: AccountWindows
    @State private var selection: String?
    @State private var asking: Asking?
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        content
            .navigationTitle("Accounts")
            .navigationSubtitle(model.updatedWindow)
            .toolbar {
                ToolbarItemGroup {
                    Button {
                        model.send(.refresh(asked: true))
                    } label: {
                        Label("Refresh", systemImage: Symbol.refresh)
                    }
                    .help("Read every account’s usage again")
                    .disabled(model.reading)
                    .accessibilityIdentifier("toolbar.refresh")
                    // Command-N is the window's own command, so it works from every pane.
                    Button {
                        model.send(.presentSheet(sheet: .add(provider: nil)))
                    } label: {
                        Label("Add Account", systemImage: Symbol.add)
                    }
                    .help("Sign in to another account")
                    .accessibilityIdentifier("toolbar.addAccount")
                }
            }
            .focusedSceneValue(
                \.refresh,
                RefreshCommand(title: "Refresh", disabled: model.reading) {
                    model.send(.refresh(asked: true))
                }
            )
            // Command-Delete in a sheet's text field deletes to the start of the line.
            .focusedSceneValue(
                \.account,
                model.sheet == nil ? selection.flatMap(model.item).map(command(for:)) : nil
            )
            .task { model.send(.paneShown(pane: .accounts)) }
            .asking($asking, of: model)
    }

    @ViewBuilder private var content: some View {
        switch model.accountsShown {
        case .noTool(let title, let detail, let linkTitle, let link):
            ContentUnavailableView {
                Label(title, systemImage: Symbol.terminal)
                    .accessibilityIdentifier("accounts.noTool")
            } description: {
                Text(detail)
            } actions: {
                if let link = URL(string: link) {
                    Link(linkTitle, destination: link)
                        .buttonStyle(.borderedProminent)
                        .accessibilityIdentifier("accounts.install")
                }
            }
        case .readFailed(let title, let detail, let retry):
            // Nothing to list, and the read said why: that is the thing to say, in full,
            // with a way to try again, and not a spinner that never stops.
            ContentUnavailableView {
                Label(title, systemImage: Severity.error.symbol)
                    .accessibilityIdentifier("accounts.readFailed")
            } description: {
                Text(detail)
            } actions: {
                Button(retry.title) { model.send(retry.intent) }
                    .disabled(!retry.enabled)
                    .accessibilityIdentifier("accounts.retry")
            }
        case .noAccounts(let title, let detail, let add):
            ContentUnavailableView {
                Label(title, systemImage: Symbol.accounts)
                    .accessibilityIdentifier("accounts.noAccounts")
            } description: {
                Text(detail)
            } actions: {
                Button(add.title) { model.send(add.intent) }
                    .buttonStyle(.borderedProminent)
                    .disabled(!add.enabled)
                    .accessibilityIdentifier("accounts.add")
            }
        case .reading(let title):
            ProgressView(title)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .accessibilityIdentifier("accounts.reading")
        case .list:
            list
        }
    }

    private var list: some View {
        List(selection: $selection) {
            ForEach(model.notices, id: \.id) { notice in
                NoticeRow(notice: notice) { send($0.intent, askingFirst: $0.confirm) }
                    .selectionDisabled()
            }
            if let step = model.setup {
                SetupTip(step: step, footing: model.footing) { model.send($0) }
            }
            ForEach(model.sections, id: \.id) { section in
                // A heading per tool once there is more than one, and no section at all
                // before: a section with no heading still takes a heading's room.
                if let heading = section.heading {
                    Section(heading) { rows(of: section) }
                } else {
                    rows(of: section)
                }
            }
        }
        .listStyle(.inset)
        .contextMenu(forSelectionType: String.self) { ids in
            if let item = ids.first.flatMap(model.item) {
                AccountMenuItems(command: command(for: item), place: .shortcutMenu)
            }
        } primaryAction: { ids in
            if let action = ids.first.flatMap(model.item)?.action {
                model.send(action.intent)
            }
        }
        .onDeleteCommand {
            if let item = selection.flatMap(model.item) { forget(item) }
        }
        .copyable(selectedEmail)
    }

    private var selectedEmail: [String] {
        guard let email = selection.flatMap(model.item)?.email, !email.isEmpty else {
            return []
        }
        return [email]
    }

    private func rows(of section: AccountSection) -> some View {
        ForEach(section.accounts, id: \.id) { item in
            AccountRow(item: item) { model.send($0) }
                .tag(item.id)
        }
    }

    // MARK: - What can be done to an account

    private func command(for item: AccountItem) -> AccountCommand {
        AccountCommand(
            item: item,
            choose: { [$asking, model] offer in
                if let question = offer.confirm {
                    $asking.wrappedValue = Asking(question: question, intent: offer.intent)
                } else {
                    model.send(offer.intent)
                }
            },
            open: { [windows, openWindow] offer in
                windows.presence.activate()
                openWindow(id: AccountWindowScene.id, value: offer.window.id)
            })
    }

    private func forget(_ item: AccountItem) {
        if let forget = item.forget, forget.enabled {
            send(forget.intent, askingFirst: forget.confirm)
        }
    }

    private func send(_ intent: Intent, askingFirst question: Question?) {
        if let question {
            asking = Asking(question: question, intent: intent)
        } else {
            model.send(intent)
        }
    }
}

private struct SetupTip: View {
    let step: SetupStep
    let footing: Footing
    let perform: (Intent) -> Void

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: Design.iconSpacing) {
            Image(systemName: symbol)
                .foregroundStyle(.tint)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: Design.rowSpacing) {
                Text(step.title).fontWeight(.medium)
                Text(step.detail).explanatory()
                HStack {
                    ForEach(Array(step.actions.enumerated()), id: \.offset) { index, choice in
                        let identifier = "setup.action.\(caseName(choice.intent))"
                        if index == 0 {
                            Button(choice.title) { perform(choice.intent) }
                                .buttonStyle(.borderedProminent)
                                .disabled(!choice.enabled)
                                .accessibilityIdentifier(identifier)
                        } else {
                            Button(choice.title) { perform(choice.intent) }
                                .disabled(!choice.enabled)
                                .accessibilityIdentifier(identifier)
                        }
                    }
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 4)
        .selectionDisabled()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("setup.\(caseName(footing))")
    }

    private var symbol: String {
        switch footing {
        case .atLimitUnchosen: Symbol.limits
        default: Symbol.switchAccount
        }
    }
}
