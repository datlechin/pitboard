import AppKit
import Foundation
import WebKit

/// The page of one account's claude.ai window: its web view, and everything WebKit asks of
/// the app about it.
///
/// A plain object until the window first hosts it, and only then does it ask for the
/// account's store and make a web view. That is what lets the model, and a test, make, keep
/// and close pages without starting WebKit.
///
/// It decides where each navigation goes (`route`), hands downloads to the model's
/// `WebDownloads`, shows the Open dialog for an upload, and says what it has to say in the
/// window's note bar. What the page shows it reads from the web view as a browser's toolbar
/// does: its title, its address, and whether it can go back or forward. It reads nothing else
/// of the page, and adds nothing to it.
@MainActor
@Observable
final class ClaudePage: NSObject {
    /// The account's store, which is also the window's value.
    let store: UUID
    /// The account, for what the note bar says. Its label is the window's title, read from
    /// the model so a rename shows at once.
    private(set) var account: ClaudeAccount
    private(set) var title: String?
    private(set) var url: URL?
    private(set) var canGoBack = false
    private(set) var canGoForward = false
    private(set) var isLoading = false
    /// What the window says above the page, until it is put away or replaced.
    var note: WebNote?
    /// The page's zoom, kept for the life of the window.
    private(set) var zoom: Double = 1
    /// Closed pages make no web view again.
    private(set) var closed = false
    /// What the web view loads first: claude.ai's home page, or a link the window was opened
    /// for. Once the web view exists, a link loads in it straight away instead.
    private(set) var first: URL

    @ObservationIgnored private let web: ClaudeWeb
    /// Where this window's downloads are kept until they end, whether or not it is open.
    @ObservationIgnored private let downloads: WebDownloads
    @ObservationIgnored private var webView: WKWebView?
    @ObservationIgnored private var observations: [NSKeyValueObservation] = []
    /// Whether the page's process has ended since the last page finished loading, so a page
    /// that keeps ending it is reloaded once and not forever.
    @ObservationIgnored private var reloadedAfterEnding = false

    init(
        store: UUID, account: ClaudeAccount, web: ClaudeWeb, downloads: WebDownloads,
        first: URL? = nil
    ) {
        self.store = store
        self.account = account
        self.web = web
        self.downloads = downloads
        self.first = first ?? web.home
    }

    /// Keeps the account's details as the latest read has them.
    func update(_ account: ClaudeAccount) {
        if self.account != account { self.account = account }
    }

    // MARK: - The web view

    /// How every claude.ai window's web view is set up, as a browser's would be: the
    /// account's store, set before the web view is made as WebKit requires, and windows
    /// opened only by a person's action. Nothing is added to the page, and the page sees
    /// WebKit's own user agent.
    static func configuration(
        store: WKWebsiteDataStore, web: ClaudeWeb
    ) -> WKWebViewConfiguration {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = store
        configuration.preferences.javaScriptCanOpenWindowsAutomatically = false
        web.prepare(configuration)
        return configuration
    }

    /// This window's configuration, on its own account's store, which asking for makes.
    func configuration() -> WKWebViewConfiguration {
        Self.configuration(store: web.stores.store(for: store), web: web)
    }

    /// The window the web view is in, for a question that belongs to it.
    var window: NSWindow? { webView?.window }

    /// The web view the window shows, made the first time it is asked for. Nil once the
    /// page is closed.
    func hostedWebView() -> WKWebView? {
        if let webView { return webView }
        guard !closed else { return nil }
        let made = ClaudeWebView(frame: .zero, configuration: configuration())
        made.navigationDelegate = self
        made.uiDelegate = self
        made.allowsBackForwardNavigationGestures = true
        made.allowsMagnification = true
        made.pageZoom = zoom
        #if DEBUG
            made.isInspectable = true
        #endif
        observe(made)
        webView = made
        made.load(URLRequest(url: first))
        return made
    }

    /// Loads `link`, which has been through `claudeLink` already, and goes through the
    /// navigation policy like any other.
    func load(_ link: URL) {
        guard !closed else { return }
        if let webView {
            webView.load(URLRequest(url: link))
        } else {
            first = link
        }
    }

    func goBack() { webView?.goBack() }
    func goForward() { webView?.goForward() }

    func reload() {
        guard let webView else { return }
        if webView.url == nil {
            webView.load(URLRequest(url: first))
        } else {
            webView.reload()
        }
    }

    func setZoom(_ change: ZoomChange) {
        zoom = PitboardApp.zoom(from: zoom, toward: change)
        webView?.pageZoom = zoom
    }

    /// Lets the web view go, so its store can be deleted and its process ends: stops
    /// loading, clears every delegate, takes it out of the window and forgets it. Downloads
    /// are the model's, and go on.
    func close() {
        closed = true
        observations = []
        guard let webView else { return }
        webView.stopLoading()
        webView.navigationDelegate = nil
        webView.uiDelegate = nil
        webView.removeFromSuperview()
        self.webView = nil
    }

    /// What a browser shows in its tab and toolbar, and nothing else of the page.
    private func observe(_ view: WKWebView) {
        observations = [
            view.observe(\.title, options: [.initial, .new]) { [weak self] view, _ in
                MainActor.assumeIsolated { self?.title = view.title }
            },
            view.observe(\.url, options: [.initial, .new]) { [weak self] view, _ in
                MainActor.assumeIsolated { self?.url = view.url }
            },
            view.observe(\.canGoBack, options: [.initial, .new]) { [weak self] view, _ in
                MainActor.assumeIsolated { self?.canGoBack = view.canGoBack }
            },
            view.observe(\.canGoForward, options: [.initial, .new]) { [weak self] view, _ in
                MainActor.assumeIsolated { self?.canGoForward = view.canGoForward }
            },
            view.observe(\.isLoading, options: [.initial, .new]) { [weak self] view, _ in
                MainActor.assumeIsolated { self?.isLoading = view.isLoading }
            },
        ]
    }

    /// Hands a route's outcome on: to macOS, and to the note bar when there is something to
    /// say about it.
    private func follow(_ route: WebRoute, for request: WebRequest) {
        if case .elsewhere(let url) = route { web.openElsewhere(url) }
        if let said = routeNote(route, for: request, email: account.email, label: account.label)
        {
            note = said
        }
    }

    /// What the policy sees of `action`, into `target`. Whether claude.ai's own page asked
    /// matters only for a download, so it is read only for one.
    private func request(for action: WKNavigationAction, target: WebRequest.Target)
        -> WebRequest?
    {
        guard let url = action.request.url else { return nil }
        var request = WebRequest(
            url: url, target: target, clicked: action.person,
            download: action.shouldPerformDownload)
        if request.download, let frame = action.askingFrame {
            let origin = frame.securityOrigin
            request.fromHome =
                frame.isMainFrame
                && isHome(
                    scheme: origin.protocol, host: origin.host, port: origin.port, web.home)
        }
        return request
    }

    /// A load failed. WebKit shows no error page, so the window says so, unless it was
    /// stopped on purpose.
    private func failed(_ error: any Error) {
        let error = error as NSError
        guard loadFailureShown(domain: error.domain, code: error.code) else { return }
        note = .loadFailed(reason: error.localizedDescription)
    }
}

/// A claude.ai window's web view, which leaves out of WebKit's shortcut menu the items that
/// cannot work in it: WebKit hands the downloads they start to the app only through private
/// calls, and cancels them without a word.
final class ClaudeWebView: WKWebView {
    override func willOpenMenu(_ menu: NSMenu, with event: NSEvent) {
        super.willOpenMenu(menu, with: event)
        for item in menu.items where !keepsMenuItem(item.identifier?.rawValue ?? "") {
            menu.removeItem(item)
        }
    }
}

// MARK: - Navigation

extension ClaudePage: WKNavigationDelegate {
    func webView(
        _ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction,
        decisionHandler: @escaping @MainActor @Sendable (WKNavigationActionPolicy) -> Void
    ) {
        let target = WebRequest.Target(frameIsMain: navigationAction.targetFrame?.isMainFrame)
        guard let request = request(for: navigationAction, target: target) else {
            return decisionHandler(.cancel)
        }
        let decided = route(request, home: web.home)
        switch decided {
        case .load:
            decisionHandler(.allow)
        case .loadHere(let here):
            decisionHandler(.cancel)
            webView.load(URLRequest(url: here))
        case .download, .askToDownload:
            decisionHandler(.download)
        case .elsewhere, .refuse, .drop:
            decisionHandler(.cancel)
            follow(decided, for: request)
        }
    }

    func webView(
        _ webView: WKWebView, decidePolicyFor navigationResponse: WKNavigationResponse,
        decisionHandler: @escaping @MainActor @Sendable (WKNavigationResponsePolicy) -> Void
    ) {
        switch responseRoute(for: navigationResponse) {
        case .show: decisionHandler(.allow)
        case .download, .askToDownload: decisionHandler(.download)
        }
    }

    /// Kept by the model, which asks first for one claude.ai's own page did not start.
    func webView(
        _ webView: WKWebView, navigationAction: WKNavigationAction,
        didBecome download: WKDownload
    ) {
        let target = WebRequest.Target(frameIsMain: navigationAction.targetFrame?.isMainFrame)
        let asks = request(for: navigationAction, target: target).map {
            route($0, home: web.home) != .download
        }
        downloads.keep(
            download, of: store, asking: asks ?? true,
            origin: navigationAction.askingFrame?.securityOrigin.host)
    }

    func webView(
        _ webView: WKWebView, navigationResponse: WKNavigationResponse,
        didBecome download: WKDownload
    ) {
        let asks = responseRoute(for: navigationResponse) != .download
        downloads.keep(download, of: store, asking: asks)
    }

    private func responseRoute(for response: WKNavigationResponse) -> ResponseRoute {
        let disposition = (response.response as? HTTPURLResponse)?
            .value(forHTTPHeaderField: "Content-Disposition")
        return PitboardApp.responseRoute(
            canShow: response.canShowMIMEType, disposition: disposition,
            mainFrame: response.isForMainFrame)
    }

    /// A page started to show: a failed load before it is over.
    func webView(_ webView: WKWebView, didCommit navigation: WKNavigation!) {
        if note?.isLoadFailure == true { note = nil }
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        reloadedAfterEnding = false
    }

    func webView(
        _ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!,
        withError error: any Error
    ) {
        failed(error)
    }

    func webView(
        _ webView: WKWebView, didFail navigation: WKNavigation!, withError error: any Error
    ) {
        failed(error)
    }

    /// The page's process ended, which leaves a blank window: load it again, once, and say
    /// so when it ends again before a page has finished loading.
    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        guard !reloadedAfterEnding else {
            note = .loadFailed(reason: "The page stopped working.")
            return
        }
        reloadedAfterEnding = true
        reload()
    }
}

// MARK: - Windows, panels and permissions

extension ClaudePage: WKUIDelegate {
    /// pitboard never makes a popup window. A claude.ai link asked for in a new window loads
    /// here, and anything else goes where the policy sends it, which the window says when
    /// nobody clicked.
    func webView(
        _ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration,
        for navigationAction: WKNavigationAction, windowFeatures: WKWindowFeatures
    ) -> WKWebView? {
        guard let request = request(for: navigationAction, target: .newWindow) else {
            return nil
        }
        let decided = route(request, home: web.home)
        switch decided {
        case .loadHere(let here):
            webView.load(URLRequest(url: here))
        case .download, .askToDownload:
            let (downloads, store) = (downloads, store)
            let origin = navigationAction.askingFrame?.securityOrigin.host
            webView.startDownload(using: navigationAction.request) { download in
                downloads.keep(
                    download, of: store, asking: decided == .askToDownload, origin: origin)
            }
        case .load, .elsewhere, .refuse, .drop:
            follow(decided, for: request)
        }
        return nil
    }

    /// Without this, WebKit turns file upload off on macOS. The Open dialog, as a sheet.
    func webView(
        _ webView: WKWebView, runOpenPanelWith parameters: WKOpenPanelParameters,
        initiatedByFrame frame: WKFrameInfo,
        completionHandler: @escaping @MainActor @Sendable ([URL]?) -> Void
    ) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = parameters.allowsDirectories
        panel.allowsMultipleSelection = parameters.allowsMultipleSelection
        guard let window = webView.window else {
            completionHandler(panel.runModal() == .OK ? panel.urls : nil)
            return
        }
        panel.beginSheetModal(for: window) { response in
            completionHandler(response == .OK ? panel.urls : nil)
        }
    }

    /// A page's alert, which WebKit otherwise answers in silence.
    func webView(
        _ webView: WKWebView, runJavaScriptAlertPanelWithMessage message: String,
        initiatedByFrame frame: WKFrameInfo,
        completionHandler: @escaping @MainActor @Sendable () -> Void
    ) {
        let alert = Self.alert(message, from: frame)
        alert.addButton(withTitle: "OK")
        present(alert, over: webView) { _ in completionHandler() }
    }

    func webView(
        _ webView: WKWebView, runJavaScriptConfirmPanelWithMessage message: String,
        initiatedByFrame frame: WKFrameInfo,
        completionHandler: @escaping @MainActor @Sendable (Bool) -> Void
    ) {
        let alert = Self.alert(message, from: frame)
        alert.addButton(withTitle: "OK")
        alert.addButton(withTitle: "Cancel")
        present(alert, over: webView) { completionHandler($0 == .alertFirstButtonReturn) }
    }

    /// A page's prompt gets no answer.
    func webView(
        _ webView: WKWebView, runJavaScriptTextInputPanelWithPrompt prompt: String,
        defaultText: String?, initiatedByFrame frame: WKFrameInfo,
        completionHandler: @escaping @MainActor @Sendable (String?) -> Void
    ) {
        completionHandler(nil)
    }

    /// The microphone and the camera are refused before WebKit asks anyone, so the app needs
    /// neither permission. Voice mode does not work in a claude.ai window.
    func webView(
        _ webView: WKWebView, requestMediaCapturePermissionFor origin: WKSecurityOrigin,
        initiatedByFrame frame: WKFrameInfo, type: WKMediaCaptureType,
        decisionHandler: @escaping @MainActor @Sendable (WKPermissionDecision) -> Void
    ) {
        decisionHandler(.deny)
    }

    private static func alert(_ message: String, from frame: WKFrameInfo) -> NSAlert {
        let alert = NSAlert()
        let host = frame.securityOrigin.host
        alert.messageText = host.isEmpty ? "claude.ai" : host
        alert.informativeText = message
        return alert
    }

    private func present(
        _ alert: NSAlert, over webView: WKWebView,
        then done: @escaping @MainActor (NSApplication.ModalResponse) -> Void
    ) {
        guard let window = webView.window else {
            done(alert.runModal())
            return
        }
        alert.beginSheetModal(for: window) { response in
            MainActor.assumeIsolated { done(response) }
        }
    }
}

extension WKNavigationAction {
    /// A person clicked a link or submitted a form, rather than the page going somewhere by
    /// itself.
    fileprivate var person: Bool {
        navigationType == .linkActivated || navigationType == .formSubmitted
    }

    /// The frame the navigation came from. The header promises one, but WebKit leaves it out
    /// for a load the app started, and Swift would take a missing one on trust, so it is
    /// read as something that can be absent.
    fileprivate var askingFrame: WKFrameInfo? {
        value(forKey: #keyPath(WKNavigationAction.sourceFrame)) as? WKFrameInfo
    }
}
