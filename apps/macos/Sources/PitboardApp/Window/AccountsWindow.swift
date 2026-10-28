import PitboardKit
import SwiftUI

struct AccountsWindow: View {
    let model: AppModel
    let windows: AccountWindows
    @State private var selection: String?
    @State private var asking: Asking?
    @SceneStorage("inspector") private var inspector = true
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        content
            // Full height, so the banners sit at the window's top over an empty state too.
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .safeAreaInset(edge: .top, spacing: 0) { banners }
            // On the list, so the inspector widens the window (docs/facts/macos-app.md).
            .frame(minWidth: 560, minHeight: 400)
            // Only beside rows: the empty states are the window's whole content.
            .inspector(
                isPresented: Binding(get: { inspector && listed }, set: { inspector = $0 })
            ) {
                AccountInspector(item: selected, act: act) { model.send($0) }
                    .inspectorColumnWidth(min: 240, ideal: 280, max: 360)
            }
            .toolbar {
                ToolbarItemGroup {
                    Button {
                        model.send(.refresh(asked: true))
                    } label: {
                        Label(words.window.refresh, systemImage: Symbol.refresh)
                    }
                    .help(words.window.refreshHelp)
                    .disabled(model.reading)
                    .accessibilityIdentifier("toolbar.refresh")
                    // Command-N is the window's own command, so it works from every window.
                    Button {
                        model.send(.presentSheet(sheet: .add(provider: nil)))
                    } label: {
                        Label(words.window.addAccount, systemImage: Symbol.add)
                    }
                    .help(words.window.addAccountHelp)
                    .accessibilityIdentifier("toolbar.addAccount")
                }
                // Apart from the list's commands, which SwiftUI draws over the inspector too.
                if #available(macOS 26, *) {
                    ToolbarSpacer(.fixed)
                }
                ToolbarItem {
                    Button {
                        inspector.toggle()
                    } label: {
                        Label(words.window.inspector ?? "", systemImage: Symbol.inspector)
                    }
                    .help(words.window.inspectorHelp ?? "")
                    .disabled(!listed)
                    .accessibilityIdentifier("toolbar.inspector")
                }
            }
            .navigationTitle(words.window.accounts)
            .navigationSubtitle(model.updatedWindow)
            .focusedSceneValue(
                \.refresh,
                RefreshCommand(title: words.window.refresh, disabled: model.reading) {
                    model.send(.refresh(asked: true))
                }
            )
            // Command-Delete in a sheet's text field deletes to the start of the line.
            .focusedSceneValue(
                \.account,
                model.sheet == nil ? selected.map { AccountCommand(item: $0, act: act) } : nil
            )
            .task { model.send(.paneShown(pane: .accounts)) }
            .asking($asking, of: model)
            .appWindow(windows.presence)
            .accountSheets(model, shown: !model.overWelcome)
    }

    private var listed: Bool {
        guard case .list = model.accountsShown else { return false }
        return !model.sections.isEmpty
    }

    private var selected: AccountItem? {
        selection.flatMap(model.item)
    }

    private var selectedEmail: [String] {
        guard let email = selected?.email, !email.isEmpty else { return [] }
        return [email]
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
            // Why the read failed, with a way to try again, not a spinner that never stops.
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
        // Afresh as a tool comes or goes: macOS 27 crashed updating rows and sections at once
        // (docs/facts/macos-app.md).
        .id(model.sections.map { [$0.id, $0.heading ?? ""] })
        .contextMenu(forSelectionType: String.self) { ids in
            if let item = ids.first.flatMap(model.item) {
                AccountMenuItems(
                    command: AccountCommand(item: item, act: act), place: .shortcutMenu)
            }
        } primaryAction: { ids in
            if let action = ids.first.flatMap(model.item)?.action {
                model.send(action.intent)
            }
        }
        .onDeleteCommand {
            if let remove = selected?.actions.first(where: { $0.kind == .remove }),
                remove.enabled
            {
                act(remove)
            }
        }
        .copyable(selectedEmail)
        // The inspector shows the account in use until another is chosen.
        .onAppear {
            if selection == nil {
                selection = model.sections.lazy.flatMap(\.accounts).first(where: \.inUse)?.id
            }
        }
    }

    @ViewBuilder private var banners: some View {
        if !model.notices.isEmpty || model.setup != nil {
            VStack(spacing: Design.rowSpacing) {
                ForEach(model.notices, id: \.id) { notice in
                    NoticeBanner(notice: notice) { send($0.intent, askingFirst: $0.confirm) }
                }
                if let step = model.setup {
                    TipBanner(step: step, footing: model.footing) { model.send($0) }
                }
            }
            .padding([.horizontal, .top])
            .padding(.bottom, Design.rowSpacing)
            .background(.background)
        }
    }

    private func rows(of section: AccountSection) -> some View {
        ForEach(section.accounts, id: \.id) { item in
            AccountRow(item: item, perform: { model.send($0) }, act: act)
                .tag(item.id)
        }
    }

    private func act(_ action: AccountAction) {
        if let window = action.window {
            windows.presence.activate()
            openWindow(id: AccountWindowScene.id, value: window.id)
        } else if let intent = action.intent {
            send(intent, askingFirst: action.confirm)
        }
    }

    private func send(_ intent: Intent, askingFirst question: Question?) {
        $asking.ask(intent, first: question, of: model)
    }
}

extension AppModel {
    // The welcome's Add starts its sheet there (plan 7), until the welcome goes on.
    var overWelcome: Bool { windowRequest.place == .welcome }
}

extension View {
    // The model keeps the sheet; this one goes however it goes, and the model is told.
    func accountSheets(_ model: AppModel, shown: Bool) -> some View {
        sheet(
            item: Binding(
                get: { shown ? model.sheet : nil },
                set: { if $0 == nil { model.send(.closeSheet) } })
        ) { sheet in
            AccountSheetView(model: model, sheet: sheet)
        }
        .windowAlerts(model, shown: shown && model.sheet == nil)
    }
}

// Told apart from another, so that one replacing it is put up afresh.
extension Sheet: Identifiable {
    public var id: String {
        switch self {
        case .add(let provider): "add/\(provider ?? "")"
        case .signInAgain(let provider, let label): "again/\(provider)/\(label)"
        case .name(let provider, let label, _): "name/\(provider)/\(label)"
        case .rename(let provider, let label): "rename/\(provider)/\(label)"
        case .stow: "stow"
        }
    }
}
