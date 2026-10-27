import AppKit
import SwiftUI
import WebKit

struct PageContent: View {
    let page: Page

    var body: some View {
        if let failure = page.failure {
            ContentUnavailableView {
                Label(words.accountWindow.couldNotLoad, systemImage: Symbol.warning)
            } description: {
                Text(failure)
            } actions: {
                Button(words.accountWindow.tryAgain) { page.reload() }
                    .keyboardShortcut(.defaultAction)
            }
        } else {
            WebViewHost(page: page)
        }
    }
}

/// It only places the view. The page owns it and everything it does, so the view outlives a
/// SwiftUI update that remakes this host, and the window's page is never loaded twice.
private struct WebViewHost: NSViewRepresentable {
    let page: Page

    func makeNSView(context: Context) -> WebContainer {
        WebContainer(webView: page.webView)
    }

    func updateNSView(_ container: WebContainer, context: Context) {
        container.show(page.webView)
    }

    static func dismantleNSView(_ container: WebContainer, coordinator: ()) {
        container.tearDown()
    }
}

/// The view around a web view that holds the system's find bar, as SwiftUI's `WebView` does.
// A web view shows no find bar of its own, so its `NSTextFinder`'s client is the web view
// (docs/facts/webkit.md).
final class WebContainer: NSView, @preconcurrency NSTextFinderBarContainer, NSMenuItemValidation
{
    private var webView: WKWebView
    private let finder = NSTextFinder()

    init(webView: WKWebView) {
        self.webView = webView
        super.init(frame: .zero)
        finder.client = webView
        finder.findBarContainer = self
        // Incremental searching leaves the page's selection where it was when Find Next is
        // chosen, so a search runs when Return is pressed (docs/facts/webkit.md).
        finder.isIncrementalSearchingEnabled = false
        addSubview(webView)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not made from a nib") }

    func show(_ webView: WKWebView) {
        guard webView !== self.webView else { return }
        self.webView.removeFromSuperview()
        self.webView = webView
        finder.client = webView
        addSubview(webView)
        takeKeyboard()
        needsLayout = true
    }

    /// Lets go of the find bar before the web view goes, so nothing finds in a page that is
    /// gone.
    func tearDown() {
        finder.findBarContainer = nil
        finder.client = nil
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        takeKeyboard()
    }

    /// The page takes the keyboard when its window opens, or when it replaces a page that
    /// had it, so typing and the Edit menu reach it.
    private func takeKeyboard() {
        if let window, window.firstResponder === window || window.firstResponder == nil {
            window.makeFirstResponder(webView)
        }
    }

    override func layout() {
        super.layout()
        var below = bounds.height
        if isFindBarVisible, let bar = findBarView {
            let height = bar.frame.height
            bar.frame = NSRect(
                x: 0, y: bounds.height - height, width: bounds.width, height: height)
            below -= height
        }
        webView.frame = NSRect(x: 0, y: 0, width: bounds.width, height: below)
    }

    // MARK: - The find bar

    // The bar is in the view only while it shows: a finder whose bar is already in place,
    // hidden, never says to show it (docs/facts/webkit.md).
    var findBarView: NSView? {
        didSet {
            oldValue?.removeFromSuperview()
            placeFindBar()
        }
    }

    var isFindBarVisible = false {
        didSet {
            placeFindBar()
            if !isFindBarVisible { window?.makeFirstResponder(webView) }
        }
    }

    private func placeFindBar() {
        if let bar = findBarView {
            if isFindBarVisible, bar.superview !== self {
                addSubview(bar)
            } else if !isFindBarVisible {
                bar.removeFromSuperview()
            }
        }
        needsLayout = true
    }

    func findBarViewDidChangeHeight() {
        needsLayout = true
    }

    func contentView() -> NSView? {
        webView
    }

    // MARK: - Edit > Find

    /// What the Find menu's items send, with an `NSTextFinder.Action` as their tag.
    @objc func performFindPanelAction(_ sender: Any?) {
        perform(sender)
    }

    override func performTextFinderAction(_ sender: Any?) {
        perform(sender)
    }

    private func perform(_ sender: Any?) {
        guard let tag = (sender as? NSValidatedUserInterfaceItem)?.tag,
            let action = NSTextFinder.Action(rawValue: tag)
        else { return }
        finder.performAction(action)
    }

    func validateMenuItem(_ menuItem: NSMenuItem) -> Bool {
        guard
            menuItem.action == #selector(performFindPanelAction(_:))
                || menuItem.action == #selector(performTextFinderAction(_:))
        else { return true }
        guard let action = NSTextFinder.Action(rawValue: menuItem.tag) else { return false }
        return finder.validateAction(action)
    }
}
