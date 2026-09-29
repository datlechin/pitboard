import Accessibility
import PitboardLinks
import SwiftUI

/// The account picker: which account's claude.ai window opens a link from outside.
///
/// Every way in ends here, and nothing opens until the person chooses an account and clicks
/// Open, even with one account. So no web page can open an account's window by itself.
///
/// A small window of its own rather than a sheet, which would need the main window open to
/// hang from, or an alert, which cannot hold a list to choose from with the keyboard. It is
/// laid out like the app's sheets, with no title in its title bar: its headline is the title.
/// It opens only for a request, closes itself when there is none, and closing it any way
/// cancels the request.
struct LinkPicker: View {
    /// The scene's id, named once so the menu bar item and the scene cannot drift apart.
    static let id = "link-picker"

    let model: AppModel
    @Environment(\.dismissWindow) private var dismissWindow
    @State private var selection: UUID?
    @State private var typed = ""
    @FocusState private var focus: Focus?

    private enum Focus: Hashable {
        case list
        case field
    }

    var body: some View {
        let pick = self.pick
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: Design.rowSpacing) {
                Text(pick.title)
                    .font(.headline)
                    .accessibilityAddTraits(.isHeader)
                    .accessibilityIdentifier("picker.title")
                Text(pick.message).explanatory().textSelection(.enabled)
            }
            .padding([.horizontal, .top], 20)
            content(pick)
                .padding(.horizontal, 20)
                .padding(.vertical, 12)
            HStack {
                Spacer()
                buttons(pick)
            }
            .padding([.horizontal, .bottom], 20)
        }
        .frame(width: 440)
        .onChange(of: model.links.requests, initial: true) { old, new in
            chooseDefault()
            if new != old { AccessibilityNotification.Announcement("Link changed").post() }
        }
        .onChange(of: pick.accounts) { chooseDefault() }
        .onChange(of: model.links.pending == nil, initial: true) {
            if model.links.pending == nil { dismissWindow(id: Self.id) }
        }
        .onChange(of: pick.title) { announce(pick) }
        .onDisappear { model.links.cancel() }
    }

    private var pick: LinkPick {
        linkPick(pending: model.links.pending, status: model.status, problem: model.problem)
    }

    // MARK: - What each state shows

    @ViewBuilder private func content(_ pick: LinkPick) -> some View {
        switch pick {
        case .idle, .readFailed, .refused:
            EmptyView()
        case .reading(let link):
            VStack(alignment: .leading, spacing: Design.rowSpacing) {
                if let link { LinkLine(link: link) }
                ProgressView("Reading accounts…")
                    .controlSize(.small)
            }
        case .noAccount(let link), .unnamed(_, let link):
            if let link { LinkLine(link: link) }
        case .choose(let link, let accounts):
            VStack(alignment: .leading, spacing: Design.rowSpacing) {
                LinkLine(link: link)
                list(accounts)
            }
        case .enter(let accounts):
            VStack(alignment: .leading, spacing: Design.rowSpacing) {
                TextField("Link", text: $typed, prompt: Text("https://claude.ai/…"))
                    .accessibilityIdentifier("picker.field")
                    .focused($focus, equals: .field)
                    .onSubmit(open)
                if case .failure(let refusal) = entered, !trimmed(typed).isEmpty {
                    Text(refusal.message).explanatory()
                }
                list(accounts)
            }
        }
    }

    private func list(_ accounts: [ClaudeAccount]) -> some View {
        List(accounts, selection: $selection) { account in
            AccountChoice(account: account, windowOpen: model.web.isOpen(account.store))
                .tag(account.store)
        }
        .listStyle(.bordered(alternatesRowBackgrounds: false))
        .frame(height: min(CGFloat(accounts.count) * 52 + 8, 220))
        .focused($focus, equals: .list)
        .contextMenu(forSelectionType: UUID.self) { _ in
        } primaryAction: { chosen in
            if let chosen = chosen.first {
                selection = chosen
                open()
            }
        }
        .accessibilityLabel("Accounts")
    }

    @ViewBuilder private func buttons(_ pick: LinkPick) -> some View {
        switch pick {
        case .refused:
            Button("OK") { model.links.cancel() }
                .keyboardShortcut(.defaultAction)
        case .idle, .reading, .readFailed:
            Button("Cancel", role: .cancel) { model.links.cancel() }
                .keyboardShortcut(.cancelAction)
        case .noAccount(let link):
            Button("Cancel", role: .cancel) { model.links.cancel() }
                .keyboardShortcut(.cancelAction)
            openInBrowser(link)
            Button("Add Account…") {
                model.links.cancel()
                model.present(.add(provider: defaultProvider))
            }
            .keyboardShortcut(.defaultAction)
        case .unnamed(let email, let link):
            Button("Cancel", role: .cancel) { model.links.cancel() }
                .keyboardShortcut(.cancelAction)
            openInBrowser(link)
            Button("Name…") {
                model.links.cancel()
                model.present(.name(provider: defaultProvider, email: email))
            }
            .keyboardShortcut(.defaultAction)
        case .choose, .enter:
            Button("Cancel", role: .cancel) { model.links.cancel() }
                .keyboardShortcut(.cancelAction)
            Button("Open", action: open)
                .keyboardShortcut(.defaultAction)
                .disabled(chosen == nil)
        }
    }

    /// Offered only where pitboard has no window to open the link in.
    @ViewBuilder private func openInBrowser(_ link: URL?) -> some View {
        if let link {
            Button("Open in Browser") {
                model.links.cancel()
                model.web.web.openElsewhere(link)
            }
        }
    }

    // MARK: - Choosing

    /// The link typed into the field, as `claudeLink` takes it.
    private var entered: Result<URL, LinkRefusal> {
        claudeLink(from: typed, home: model.links.home)
    }

    /// The account and link Open would open, when both are there.
    private var chosen: (store: UUID, link: URL)? {
        let pick = self.pick
        guard let selection, pick.accounts.contains(where: { $0.store == selection }) else {
            return nil
        }
        switch pick {
        case .choose(let link, _): return (selection, link)
        case .enter: return (try? entered.get()).map { (selection, $0) }
        default: return nil
        }
    }

    private func open() {
        guard let chosen else { return }
        model.links.choose(chosen.store, link: chosen.link)
    }

    /// Keeps the selection when a second link arrives, and chooses the default when there is
    /// none, or the one there was is no longer listed.
    private func chooseDefault() {
        let accounts = pick.accounts
        if let selection, accounts.contains(where: { $0.store == selection }) { return }
        selection = defaultChoice(in: accounts, lastChosen: model.links.lastChosen)
        if case .enter = pick {
            focus = .field
        } else {
            focus = .list
        }
    }

    private func announce(_ pick: LinkPick) {
        switch pick {
        case .reading, .readFailed, .refused, .noAccount, .unnamed:
            AccessibilityNotification.Announcement("\(pick.title). \(pick.message)").post()
        case .idle, .choose, .enter:
            break
        }
    }
}

/// The link the picker opens, one line, cut in the middle when it is long, with the whole
/// link as its help tag. VoiceOver reads it as one element, "Link", whose value is the whole
/// link, however much of it the line shows.
private struct LinkLine: View {
    let link: URL

    var body: some View {
        LabeledContent("Link") {
            Text(shownLink(link))
                .lineLimit(1)
                .truncationMode(.middle)
                .textSelection(.enabled)
                .help(link.absoluteString)
                .accessibilityLabel("Link")
                .accessibilityValue(shownLink(link))
                .accessibilityIdentifier("picker.link")
        }
    }
}

/// One account in the picker: its label, its address, and whether its window is open.
private struct AccountChoice: View {
    let account: ClaudeAccount
    let windowOpen: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: Design.lineSpacing) {
            Text(account.label)
            Text(account.email)
                .font(.callout)
                .foregroundStyle(.secondary)
            if windowOpen {
                Text(windowOpenNote)
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(account.label), \(account.email)")
        .accessibilityValue(windowOpen ? "window open" : "")
        // What the line under an open window's account says, which VoiceOver would drop.
        .accessibilityHint(windowOpen ? windowOpenNote : "")
        .accessibilityIdentifier("picker.account.\(account.label)")
    }
}
