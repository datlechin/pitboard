import PitboardKit
import SwiftUI

/// A window for one site and one account, not a browser: no address field and no tabs of its
/// own. Links that leave the site go to the browser.
struct AccountWindowView: View {
    let windows: AccountWindows
    /// Nil for a window macOS restored without one.
    let store: UUID?

    @State private var session: WebSession?
    @Environment(\.dismissWindow) private var dismissWindow

    private var opened: OpenWindow? { store.flatMap(windows.opened) }

    var body: some View {
        // The model closes the window once a read no longer lists its account, and macOS can
        // restore a window for none at all.
        let gone = store.map(windows.isClosing) ?? true
        content
            .frame(minWidth: 480, minHeight: 360)
            .appWindow(windows.presence)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("account-window")
            .onAppear {
                if let store { windows.opening(store) }
            }
            .onChange(of: opened, initial: true) { _, opened in
                if let opened { session = windows.session(for: opened) }
            }
            .onChange(of: session?.page.url) { _, url in
                if let account = session?.account { windows.remember(url, of: account) }
            }
            .onChange(of: gone, initial: true) { _, gone in
                if gone { dismissWindow() }
            }
            .onDisappear {
                if let store { windows.closed(store) }
            }
    }

    @ViewBuilder private var content: some View {
        if let session {
            SessionView(windows: windows, session: session, account: session.account)
        } else {
            let waiting = windows.model.accountWindows.waiting
            Group {
                switch waiting.shown {
                case .reading(let title):
                    ProgressView(title)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                case .readFailed(let title, let detail, let retry):
                    ContentUnavailableView {
                        Label(title, systemImage: Symbol.warning)
                    } description: {
                        Text(detail)
                    } actions: {
                        Button(retry.title) { windows.model.send(retry.intent) }
                            .disabled(!retry.enabled)
                    }
                }
            }
            .navigationTitle(waiting.windowTitle)
        }
    }
}

private struct SessionView: View {
    let windows: AccountWindows
    @Bindable var session: WebSession
    let account: WindowAccount
    @State private var removingData = false
    @State private var showingDownloads = false

    var body: some View {
        let page = session.page
        let removal = removeDataAlert(account: account)
        VStack(spacing: 0) {
            if let note = session.note {
                NoteBar(note: note) { session.note = nil }
                Divider()
            }
            PageContent(page: page)
        }
        .navigationTitle(account.title)
        .navigationSubtitle(page.title.isEmpty ? account.site.name : page.title)
        .toolbar {
            ToolbarItemGroup(placement: .navigation) {
                Button(words.accountWindow.back, systemImage: Symbol.back) { page.goBack() }
                    .help(words.accountWindow.backHelp)
                    .disabled(!page.canGoBack)
                Button(words.accountWindow.forward, systemImage: Symbol.forward) {
                    page.goForward()
                }
                .help(words.accountWindow.forwardHelp)
                .disabled(!page.canGoForward)
            }
            ToolbarItemGroup(placement: .primaryAction) {
                if windows.model.accountWindows.downloads.contains(where: {
                    $0.store == account.store
                }) {
                    Button(words.accountWindow.downloads, systemImage: Symbol.downloads) {
                        showingDownloads.toggle()
                    }
                    .help(words.accountWindow.downloadsHelp)
                    .accessibilityIdentifier("window.downloads")
                    .popover(isPresented: $showingDownloads, arrowEdge: .bottom) {
                        DownloadsList(windows: windows, store: account.store)
                    }
                }
                if page.isLoading {
                    Button(words.accountWindow.stop, systemImage: Symbol.stop) {
                        page.stopLoading()
                    }
                    .help(words.accountWindow.stopHelp)
                } else {
                    Button(words.accountWindow.reload, systemImage: Symbol.refresh) {
                        page.reload()
                    }
                    .help(words.accountWindow.reloadHelp)
                }
            }
        }
        .focusedSceneValue(session)
        .focusedSceneValue(
            \.refresh,
            RefreshCommand(title: words.accountWindow.reloadPage, disabled: false) {
                page.reload()
            }
        )
        .onChange(of: session.removalAsked) { _, asked in
            if asked {
                session.removalAsked = false
                removingData = true
            }
        }
        .alert(removal.title, isPresented: $removingData) {
            Button(words.accountWindow.remove) {
                Task { await windows.removeWebsiteData(of: session) }
            }
            Button(words.window.cancel, role: .cancel) {}
        } message: {
            Text(removal.message)
        }
    }
}

private struct NoteBar: View {
    let note: WindowNote
    let dismiss: () -> Void

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: Design.iconSpacing) {
            Image(systemName: note.kind == .signIn ? Symbol.signIn : Symbol.note)
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            Text(note.text)
                .explanatory()
                .textSelection(.enabled)
                .accessibilityIdentifier("window.note.\(caseName(note.kind))")
            Spacer(minLength: 0)
            Button(words.window.dismiss, systemImage: "xmark", action: dismiss)
                .labelStyle(.iconOnly)
                .buttonStyle(.borderless)
                .foregroundStyle(.secondary)
                .help(words.window.dismiss)
                .accessibilityIdentifier("window.note.dismiss")
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.bar)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("window.note")
        // Said when it appears, since it appears without the person doing anything here.
        .onAppear { AccessibilityNotification.Announcement(note.text).post() }
        .onChange(of: note) { _, said in
            AccessibilityNotification.Announcement(said.text).post()
        }
    }
}

private struct DownloadsList: View {
    let windows: AccountWindows
    let store: String

    var body: some View {
        let downloads = windows.downloads
        let shown = windows.model.accountWindows
        let transfers = shown.downloads.filter { $0.store == store }
        let clear = shown.open.first { $0.store == store }?.clearDownloads
        let rows = VStack(alignment: .leading, spacing: 0) {
            ForEach(transfers) { transfer in
                TransferRow(
                    transfer: transfer, progress: downloads.progress(of: transfer.id),
                    reveal: downloads.reveal
                ) { downloads.cancel(transfer.id) }
                .padding(.horizontal, 12)
                Divider()
            }
        }
        VStack(alignment: .leading, spacing: 0) {
            // As tall as its rows, and scrolling once they would be taller than 300 points.
            ViewThatFits(in: .vertical) {
                rows
                ScrollView { rows }
            }
            .frame(width: 340)
            .frame(maxHeight: 300)
            if let clear {
                HStack {
                    Spacer()
                    Button(clear.title) { windows.model.send(clear.intent) }
                        .disabled(!clear.enabled)
                }
                .padding(8)
            }
        }
    }
}

private struct TransferRow: View {
    let transfer: DownloadShown
    let progress: Progress?
    let reveal: (String) -> Void
    let cancel: () -> Void

    var body: some View {
        HStack(spacing: Design.iconSpacing) {
            VStack(alignment: .leading, spacing: 2) {
                Text(transfer.name).lineLimit(1).truncationMode(.middle)
                if transfer.running {
                    if let progress {
                        ProgressView(progress).labelsHidden()
                    } else {
                        ProgressView().progressViewStyle(.linear).labelsHidden()
                    }
                } else if let said = transfer.said {
                    let failed: Bool = {
                        if case .failed = transfer.state { return true }
                        return false
                    }()
                    Text(said).font(.caption).foregroundStyle(.secondary)
                        .lineLimit(failed ? 2 : nil)
                }
            }
            Spacer(minLength: 0)
            if transfer.running {
                Button(words.window.cancel, systemImage: "xmark.circle.fill", action: cancel)
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .help(words.accountWindow.stopDownloadHelp)
            } else if case .finished(let file) = transfer.state {
                Button(
                    words.accountWindow.showFile, systemImage: "magnifyingglass.circle.fill"
                ) {
                    reveal(file)
                }
                .labelStyle(.iconOnly)
                .buttonStyle(.borderless)
                .help(words.accountWindow.showFileHelp)
            }
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("download")
    }
}
