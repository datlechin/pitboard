import Accessibility
import AppKit
import SwiftUI
import WebKit

/// One account's claude.ai window: claude.ai's own pages, signed in as that account, with
/// the account's label as the window's title.
///
/// The window's value is the account's store, so each account has at most one window and
/// opening it again brings it forward. It shows nothing and makes no store until a read has
/// shown an enrolled account deriving that store, and it closes itself once a read shows none
/// does: forgotten here, from the command line, or before a restored window came back.
struct ClaudeWindow: View {
    /// The scene's id, named once so the menu bar item and the scene cannot drift apart.
    static let id = "claude"

    let model: AppModel
    let store: UUID?
    @Environment(\.dismissWindow) private var dismissWindow
    @State private var openingLink = false
    @State private var confirmingSignOut = false
    @State private var failure: ActionFailure?

    var body: some View {
        // A container of its own, so what the window shows can change, from reading to its
        // page, without the window's own appearing and disappearing, which closes the page.
        ZStack { content }
            .frame(minWidth: 480, minHeight: 360)
            .navigationTitle(account?.label ?? "claude.ai")
            .navigationSubtitle(subtitle)
            .toolbar { toolbar }
            .focusedSceneValue(\.claudeWindow, actions)
            .task(id: account) {
                if let account { _ = await model.claudePage(for: account.store) }
            }
            .onChange(of: gone, initial: true) {
                if gone { close() }
            }
            .onDisappear {
                if let store { model.web.detach(store) }
            }
            .sheet(isPresented: $openingLink) {
                if let account, let page {
                    OpenLinkSheet(label: account.label, home: model.web.web.home) { link in
                        page.load(link)
                    }
                }
            }
            .alert("Sign out of claude.ai in this window?", isPresented: $confirmingSignOut) {
                Button("Sign Out", role: .destructive, action: signOut)
                Button("Cancel", role: .cancel) {}
            } message: {
                Text(signOutMessage)
            }
            .failureAlert($failure)
    }

    @ViewBuilder private var content: some View {
        if model.status == nil {
            ProgressView("Reading accounts…")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if let store, model.web.signingOut.contains(store) {
            ProgressView("Signing out…")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if let page {
            VStack(spacing: 0) {
                if let note = page.note {
                    WebNoteBar(note: note) { page.note = nil }
                    Divider()
                }
                WebViewHost(page: page)
            }
        } else {
            ProgressView("Opening claude.ai…")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    /// The toolbar's buttons carry no shortcut: the View menu's commands do, since a toolbar
    /// can be hidden and cannot be the only place a command is.
    @ToolbarContentBuilder private var toolbar: some ToolbarContent {
        ToolbarItemGroup(placement: .navigation) {
            Button {
                page?.goBack()
            } label: {
                Label("Back", systemImage: Symbol.back)
            }
            .help("Show the previous page")
            .disabled(page?.canGoBack != true)
            Button {
                page?.goForward()
            } label: {
                Label("Forward", systemImage: Symbol.forward)
            }
            .help("Show the next page")
            .disabled(page?.canGoForward != true)
        }
        ToolbarItem(placement: .primaryAction) {
            Button {
                page?.reload()
            } label: {
                Label("Reload", systemImage: Symbol.refresh)
            }
            .help("Load this page again")
            .disabled(page == nil)
        }
    }

    // MARK: - What the window is about

    /// The enrolled account this window's store belongs to, as the last read has it.
    private var account: ClaudeAccount? {
        guard let store else { return nil }
        return model.claudeWindows.first { $0.store == store }
    }

    private var page: ClaudePage? {
        store.flatMap { model.web.pages[$0] }
    }

    /// A read has shown no enrolled account for this window, or it was opened with no store.
    private var gone: Bool {
        store == nil || model.status != nil && account == nil
    }

    /// The page's title, such as a conversation's name, or the site before one has loaded.
    private var subtitle: String {
        guard let title = page?.title, !title.isEmpty else { return "claude.ai" }
        return title
    }

    private var actions: ClaudeWindowActions? {
        guard let page, let account else { return nil }
        return ClaudeWindowActions(
            label: account.label,
            canGoBack: page.canGoBack,
            canGoForward: page.canGoForward,
            goBack: { page.goBack() },
            goForward: { page.goForward() },
            reload: { page.reload() },
            openLink: { openingLink = true },
            signOut: { confirmingSignOut = true },
            zoom: { page.setZoom($0) })
    }

    private var signOutMessage: String {
        PitboardApp.signOutMessage(label: account?.label)
    }

    private func signOut() {
        guard let store else { return }
        Task { failure = await model.web.signOut(store) }
    }

    private func close() {
        if let store {
            model.web.detach(store)
            dismissWindow(id: Self.id, value: store)
        } else {
            dismissWindow()
        }
    }
}

/// What the main menu does to the claude.ai window in front, which the window publishes
/// while it is focused. The menu's items are disabled when no claude.ai window is.
struct ClaudeWindowActions {
    /// The account's label.
    let label: String
    let canGoBack: Bool
    let canGoForward: Bool
    let goBack: () -> Void
    let goForward: () -> Void
    let reload: () -> Void
    let openLink: () -> Void
    let signOut: () -> Void
    let zoom: (ZoomChange) -> Void
}

struct ClaudeWindowKey: FocusedValueKey {
    typealias Value = ClaudeWindowActions
}

extension FocusedValues {
    var claudeWindow: ClaudeWindowActions? {
        get { self[ClaudeWindowKey.self] }
        set { self[ClaudeWindowKey.self] = newValue }
    }
}

/// One line above the page for what pitboard has to say about this window. Not an alert,
/// since nothing needs deciding.
struct WebNoteBar: View {
    let note: WebNote
    let dismiss: () -> Void

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: Design.iconSpacing) {
            if note.inProgress {
                // The note's own words say what is under way.
                ProgressView().controlSize(.small)
                    .accessibilityHidden(true)
            } else {
                Image(systemName: note.symbol)
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
            }
            Text(note.text)
                .font(.callout)
                .fixedSize(horizontal: false, vertical: true)
                .textSelection(.enabled)
            Spacer(minLength: Design.iconSpacing)
            if let file = note.file {
                Button("Show in Finder") {
                    NSWorkspace.shared.activateFileViewerSelecting([file])
                }
            }
            Button("Dismiss", systemImage: "xmark", action: dismiss)
                .labelStyle(.iconOnly)
                .buttonStyle(.borderless)
                .foregroundStyle(.secondary)
                .help("Dismiss")
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.bar)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("claude.note")
        // It appears where nobody's focus is, and VoiceOver does not read what appears by
        // itself.
        .onAppear(perform: announce)
        .onChange(of: note, announce)
    }

    private func announce() {
        AccessibilityNotification.Announcement(note.text).post()
    }
}

/// Hosts a page's web view. The view never owns WebKit objects: the page does, so a body
/// evaluated again can never make a second web view, and closing the page takes its web view
/// out of the window.
struct WebViewHost: NSViewRepresentable {
    let page: ClaudePage

    func makeNSView(context: Context) -> WebViewContainer {
        let container = WebViewContainer()
        container.host(page.hostedWebView())
        return container
    }

    func updateNSView(_ container: WebViewContainer, context: Context) {
        container.host(page.hostedWebView())
    }
}

/// Holds the web view of whichever page the window shows, sized to fill it.
final class WebViewContainer: NSView {
    private weak var hosted: WKWebView?

    func host(_ webView: WKWebView?) {
        guard webView !== hosted || webView?.superview !== self else { return }
        for view in subviews { view.removeFromSuperview() }
        hosted = webView
        guard let webView else { return }
        webView.frame = bounds
        webView.autoresizingMask = [.width, .height]
        addSubview(webView)
        window?.makeFirstResponder(webView)
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        // The page takes the keyboard, so Edit's commands and typing reach it at once.
        if let hosted { window?.makeFirstResponder(hosted) }
    }
}
