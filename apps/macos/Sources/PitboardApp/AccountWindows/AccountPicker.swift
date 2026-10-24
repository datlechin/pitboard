import PitboardKit
import SwiftUI

/// Laid out as Photos' Choose Library is: a list, Cancel, and a default button named for what
/// it does.
///
/// Every link from outside waits here until somebody chooses, even with one account, so no
/// page can open an account's window by sharing a link with Pitboard.
struct AccountPicker: View {
    static let id = "open-link"

    let windows: AccountWindows
    @State private var chosen: UUID?
    @Environment(\.openWindow) private var openWindow
    @Environment(\.dismissWindow) private var dismissWindow

    private var picker: LinkPicker? { windows.model.accountWindows.picker }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if let picker {
                content(picker)
            } else {
                let noLink = windows.model.accountWindows.noLink
                ContentUnavailableView(
                    noLink.title, systemImage: Symbol.site, description: Text(noLink.detail))
                HStack {
                    Spacer()
                    Button("Close") { dismissWindow() }
                        .keyboardShortcut(.defaultAction)
                }
            }
        }
        .padding(20)
        .frame(width: 440)
        .appWindow(windows.presence)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("account-picker")
        .onChange(of: picker?.arrival) { before, now in
            if now == nil, before != nil {
                dismissWindow()
            } else if now != nil {
                // The link came from another app, which is in front.
                chosen = nil
                windows.presence.comeForward()
            }
        }
        .onDisappear { dismiss() }
    }

    @ViewBuilder private func content(_ picker: LinkPicker) -> some View {
        switch picker.shown {
        case .reading(let title):
            ProgressView(title)
                .frame(maxWidth: .infinity)
            buttons { cancelButton }
        case .readFailed(let title, let detail, let retry):
            Text(title).font(.headline)
            Text(detail).explanatory()
            buttons {
                cancelButton
                Button(retry.title) { windows.model.send(retry.intent) }
                    .keyboardShortcut(.defaultAction)
                    .disabled(!retry.enabled)
            }
        case .refused(let title, let reason):
            Text(title).font(.headline)
            Text(reason).explanatory()
            buttons {
                Button("OK") { dismiss() }
                    .keyboardShortcut(.defaultAction)
            }
        case .noAccount(let title, let link, let linkText, let detail, let inBrowser, let add):
            Text(title).font(.headline)
            LinkLine(text: linkText, url: link.url)
            Text(detail).explanatory()
            HStack {
                Button(inBrowser) {
                    if let url = URL(string: link.url) { NSWorkspace.shared.open(url) }
                    dismiss()
                }
                Spacer()
                cancelButton
                Button(add.title) { windows.model.send(add.intent) }
                    .keyboardShortcut(.defaultAction)
                    .disabled(!add.enabled)
            }
        case .choose(let title, let link, let linkText, let accounts, let preferred, let open):
            Text(title).font(.headline)
            LinkLine(text: linkText, url: link.url)
            let first = UUID(uuidString: preferred)
            let selection = Binding {
                chosen ?? first
            } set: {
                chosen = $0
            }
            List(accounts, selection: selection) { account in
                AccountChoice(account: account)
                    .tag(account.id)
            }
            .frame(minHeight: 96, idealHeight: 160, maxHeight: 360)
            .contextMenu(forSelectionType: UUID.self) { _ in
            } primaryAction: { stores in
                if picker.armed, let store = stores.first { openLink(picker, as: store) }
            }
            .accessibilityIdentifier("picker.accounts")
            buttons {
                cancelButton
                Button(open) {
                    if let store = chosen ?? first { openLink(picker, as: store) }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(!picker.armed)
            }
        }
    }

    private var cancelButton: some View {
        Button("Cancel", role: .cancel) { dismiss() }
            .keyboardShortcut(.cancelAction)
    }

    private func buttons<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        HStack {
            Spacer()
            content()
        }
    }

    private func dismiss() {
        if let arrival = picker?.arrival { windows.model.send(.dismissLink(arrival: arrival)) }
    }

    private func openLink(_ picker: LinkPicker, as store: UUID) {
        guard case .choose(_, _, _, let accounts, _, _) = picker.shown,
            accounts.contains(where: { $0.id == store })
        else { return }
        windows.model.send(.openLink(arrival: picker.arrival, store: store.uuidString))
        windows.presence.activate()
        openWindow(id: AccountWindowScene.id, value: store)
    }
}

private struct LinkLine: View {
    let text: String
    let url: String

    var body: some View {
        Text(text)
            .font(.callout.monospaced())
            .lineLimit(1)
            .truncationMode(.middle)
            .textSelection(.enabled)
            .help(url)
            .accessibilityLabel("Link")
            .accessibilityValue(url)
    }
}

private struct AccountChoice: View {
    let account: PickerAccount

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(account.window.label)
            Text(account.detail)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("picker.account.\(account.window.label)")
    }
}
