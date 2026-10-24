import AppKit
import Foundation
import PitboardKit
import WebKit

/// An account window's page, in `WebPage`'s shape.
// `WebPage` cannot host an account window (docs/facts/webkit.md). Moving to it once it can is a
// matter of conformances.
@MainActor
@Observable
final class Page {
    let role: PageRole
    @ObservationIgnored let webView: WKWebView

    private(set) var title = ""
    private(set) var url: URL?
    private(set) var isLoading = false
    private(set) var canGoBack = false
    private(set) var canGoForward = false
    /// Why the page is not shown, after its last load failed. WebKit shows no error page of
    /// its own, so without this the window would stay blank.
    private(set) var failure: String?
    private(set) var zoom = 1.0

    @ObservationIgnored private var failedURL: URL?
    /// When the page's content process last ended, so a page that keeps ending it says so
    /// rather than loading forever.
    @ObservationIgnored private var lastEnded: Date?
    @ObservationIgnored private let now: () -> Date
    @ObservationIgnored private var observations: [NSKeyValueObservation] = []
    @ObservationIgnored private var leaving: CheckedContinuation<Void, Never>?

    init(
        role: PageRole, configuration: WKWebViewConfiguration,
        now: @escaping () -> Date = Date.init
    ) {
        self.role = role
        self.now = now
        webView = PageWebView(frame: .zero, configuration: configuration)
        // As Safari: two-finger swipes go back and forward, and a pinch zooms.
        webView.allowsBackForwardNavigationGestures = true
        webView.allowsMagnification = true
        // A force click previews a link through Quick Look, which loads it outside the
        // account's store, signed in as nobody.
        webView.allowsLinkPreview = false
        #if DEBUG
            webView.isInspectable = true
        #endif
        observe()
    }

    static func configuration(
        store: WKWebsiteDataStore, configure: (WKWebViewConfiguration) -> Void
    ) -> WKWebViewConfiguration {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = store
        // Safari's popup blocker: a page opens a window only when a person clicked.
        configuration.preferences.javaScriptCanOpenWindowsAutomatically = false
        // Videos and other elements can fill the screen, as in Safari.
        configuration.preferences.isElementFullscreenEnabled = true
        configure(configuration)
        return configuration
    }

    private func observe() {
        func track<Value>(
            _ path: KeyPath<WKWebView, Value>,
            _ update: @escaping @MainActor @Sendable (Page, WKWebView) -> Void
        ) -> NSKeyValueObservation {
            webView.observe(path, options: [.initial, .new]) { [weak self] webView, _ in
                // WebKit changes these on the main thread, where it lives.
                MainActor.assumeIsolated {
                    guard let self else { return }
                    update(self, webView)
                }
            }
        }
        observations = [
            track(\.title) { $0.title = $1.title ?? "" },
            track(\.url) { $0.url = $1.url },
            track(\.isLoading) { $0.isLoading = $1.isLoading },
            track(\.canGoBack) { $0.canGoBack = $1.canGoBack },
            track(\.canGoForward) { $0.canGoForward = $1.canGoForward },
        ]
    }

    // MARK: - Commands

    func load(_ url: URL) {
        failure = nil
        failedURL = nil
        webView.load(URLRequest(url: url))
    }

    /// Loads `request` as a page asked for it, keeping its method, body and referrer.
    func load(_ request: URLRequest) {
        failure = nil
        failedURL = nil
        webView.load(request)
    }

    func goBack() { webView.goBack() }
    func goForward() { webView.goForward() }

    func reload() {
        guard failure != nil else {
            webView.reload()
            return
        }
        failure = nil
        if let failedURL {
            webView.load(URLRequest(url: failedURL))
        } else {
            webView.reload()
        }
    }

    func stopLoading() { webView.stopLoading() }

    func zoom(_ change: ZoomChange) {
        zoom = Self.zoom(from: zoom, toward: change)
        webView.pageZoom = zoom
        // A pinch zooms on its own, and Actual Size undoes both, as in Safari: setting the
        // page zoom leaves a pinch's magnification as it was (docs/facts/webkit.md).
        if change == .actualSize { webView.magnification = 1 }
    }

    /// Waits, up to two seconds, until the page has left for a blank one: its hide and unload
    /// handlers run then, and not after what comes next.
    func leave() async {
        webView.stopLoading()
        webView.load(URLRequest(url: URL(string: "about:blank")!))
        await withCheckedContinuation { done in
            leaving = done
            Task { [weak self] in
                try? await Task.sleep(for: .seconds(2))
                self?.loadFinished()
            }
        }
    }

    func close() {
        for observation in observations { observation.invalidate() }
        observations = []
        webView.stopLoading()
        webView.navigationDelegate = nil
        webView.uiDelegate = nil
        webView.removeFromSuperview()
    }

    // MARK: - What happened, as the page's delegate says

    func loadProgressed() {
        failure = nil
    }

    /// The page's load ended, finished or failed.
    func loadFinished() {
        leaving?.resume()
        leaving = nil
    }

    func loadFailed(_ url: URL?, reason: String) {
        failedURL = url ?? self.url
        failure = reason
    }

    func contentEnded() {
        let ended = now()
        defer { lastEnded = ended }
        switch afterContentProcessEnded(
            lastEnded: lastEnded?.timeIntervalSince1970, now: ended.timeIntervalSince1970)
        {
        case .reload: webView.reload()
        case .stopped(let reason): loadFailed(url, reason: reason)
        }
    }

    // MARK: - Zoom

    enum ZoomChange: Equatable {
        case actualSize
        case zoomIn
        case zoomOut
    }

    /// Safari's zoom steps, as fractions of the page's size. WebKit has no list of its own.
    nonisolated static let zoomSteps: [Double] = [
        0.5, 0.75, 0.85, 1, 1.15, 1.25, 1.5, 1.75, 2, 2.5, 3,
    ]

    nonisolated static func zoom(from current: Double, toward change: ZoomChange) -> Double {
        let near = 0.001
        switch change {
        case .actualSize: return 1
        case .zoomIn: return zoomSteps.first { $0 > current + near } ?? zoomSteps.last!
        case .zoomOut: return zoomSteps.last { $0 < current - near } ?? zoomSteps.first!
        }
    }

    // MARK: - Failures

    /// Whether a failed load is worth saying. A load stopped for another,
    /// `NSURLErrorCancelled`, is not, and neither is one a policy decision ended, which every
    /// download and every stopped navigation gives: WebKit's frame load interrupted by a
    /// policy change, code 102 in `WebKitErrorDomain`.
    nonisolated static func isShown(_ error: Error) -> Bool {
        let error = error as NSError
        switch (error.domain, error.code) {
        case (NSURLErrorDomain, NSURLErrorCancelled): return false
        case (webKitErrorDomain, frameLoadInterruptedByPolicyChange): return false
        default: return true
        }
    }

    /// `WebKitErrorDomain`, which only the deprecated legacy WebKit header names.
    nonisolated static let webKitErrorDomain = "WebKitErrorDomain"
    /// `WebKitErrorFrameLoadInterruptedByPolicyChange`, from the same header.
    nonisolated static let frameLoadInterruptedByPolicyChange = 102
}

/// A page's web view, whose shortcut menu leaves out WebKit's own download items.
///
/// WebKit hands a download started from Download Linked File, Download Image or Download
/// Video to the app only through a private delegate call, and with no delegate it cancels the
/// download without a word: those items would do nothing. The site's own download buttons
/// work. The items are told apart by identifiers WebKit has used since macOS 10.12
/// (`WKMenuItemIdentifiersPrivate.h` in WebKit's source); macOS has no public way to adapt
/// them, or to learn which link the menu was opened on.
final class PageWebView: WKWebView {
    override func willOpenMenu(_ menu: NSMenu, with event: NSEvent) {
        super.willOpenMenu(menu, with: event)
        menu.items.removeAll { !Self.keeps($0.identifier?.rawValue ?? "") }
    }

    nonisolated static func keeps(_ identifier: String) -> Bool {
        ![
            "WKMenuItemIdentifierDownloadLinkedFile", "WKMenuItemIdentifierDownloadImage",
            "WKMenuItemIdentifierDownloadMedia",
        ].contains(identifier)
    }
}
