import AppKit
import PitboardLinkTarget
import PitboardShareBindings
import SwiftUI
import UniformTypeIdentifiers

/// **Pitboard** in the system Share menu: hands the page being shared to the app it came in,
/// whose account picker asks which account's window opens it.
///
/// Sandboxed, with no network, no files and no group shared with the app. The link check is
/// pitboard-share-ffi's, the rule the app reads the link by. The app checks the link again,
/// since anything on the Mac can open a Pitboard link.
final class ShareViewController: NSViewController {
    private let state = ShareState()
    private var started = false

    override func loadView() {
        let host = NSHostingController(
            rootView: ShareView(state: state) { [weak self] in self?.cancel() })
        // The host's size follows the view's, and the share sheet follows the host's.
        host.sizingOptions = .preferredContentSize
        addChild(host)
        view = host.view
        preferredContentSize = host.preferredContentSize
    }

    override func preferredContentSizeDidChange(for viewController: NSViewController) {
        super.preferredContentSizeDidChange(for: viewController)
        preferredContentSize = viewController.preferredContentSize
    }

    override func viewDidAppear() {
        super.viewDidAppear()
        guard !started else { return }
        started = true
        Task { await share() }
    }

    private func share() async {
        let scheme = LinkTarget.scheme(in: .main)
        let pitboardLink: String
        do {
            // A page Pitboard does not open is said so before the app is looked for.
            pitboardLink = try shareLink(
                text: await sharedURL()?.absoluteString ?? "", scheme: scheme ?? "")
        } catch ShareRefusal.Refused(let reason) {
            state.phase = .refused(reason)
            return
        } catch {
            state.phase = .failed(error.localizedDescription)
            return
        }
        // The app this extension came in, not whichever copy Launch Services picks: a debug
        // extension reaches the debug app.
        guard let app = LinkTarget.containingApp(of: Bundle.main.bundleURL), scheme != nil,
            let link = URL(string: pitboardLink)
        else {
            state.phase = .failed(words.noApp)
            return
        }
        do {
            let configuration = NSWorkspace.OpenConfiguration()
            configuration.activates = true
            try await NSWorkspace.shared.open(
                [link], withApplicationAt: app, configuration: configuration)
            extensionContext?.completeRequest(returningItems: [], completionHandler: nil)
        } catch {
            state.phase = .failed(error.localizedDescription)
        }
    }

    private func sharedURL() async -> URL? {
        let items = extensionContext?.inputItems.compactMap { $0 as? NSExtensionItem } ?? []
        let providers = items.flatMap { $0.attachments ?? [] }
        guard
            let provider = providers.first(where: {
                $0.hasItemConformingToTypeIdentifier(UTType.url.identifier)
            })
        else { return nil }
        // It returns its progress, so it comes into Swift with no async form.
        return await withCheckedContinuation { done in
            _ = provider.loadTransferable(type: URL.self) {
                done.resume(returning: try? $0.get())
            }
        }
    }

    private func cancel() {
        extensionContext?.cancelRequest(
            withError: NSError(domain: NSCocoaErrorDomain, code: NSUserCancelledError))
    }
}
