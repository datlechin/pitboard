import AppKit
import PitboardLinks
import SwiftUI
import UniformTypeIdentifiers

/// **pitboard** in the system Share menu: hands the page being shared to the app it came in,
/// whose account picker asks which account's claude.ai window opens it.
///
/// Sandboxed, as an app extension must be, with no network, no files and no group shared
/// with the app. It reads the one link the host hands it, checks it is a claude.ai link, and
/// opens a pitboard link with the app it is inside. It writes nothing anywhere. Anything but
/// a claude.ai link is refused here, so sharing another site never starts pitboard; the app
/// checks again all the same, because anything can open a pitboard link.
final class ShareViewController: NSViewController {
    private let state = ShareState()
    private var started = false

    override func loadView() {
        let host = NSHostingView(
            rootView: ShareView(state: state) { [weak self] in self?.cancel() })
        host.setFrameSize(host.fittingSize)
        view = host
        preferredContentSize = host.fittingSize
    }

    override func viewDidAppear() {
        super.viewDidAppear()
        guard !started else { return }
        started = true
        Task { await share() }
    }

    private func share() async {
        guard let shared = await sharedURL() else {
            state.phase = .refused(LinkRefusal.noLink.message)
            return
        }
        switch claudeLink(from: shared.absoluteString, home: claudeHome) {
        case .failure(let refusal):
            state.phase = .refused(refusal.message)
        case .success(let link):
            let scheme =
                Bundle.main.object(forInfoDictionaryKey: "PitboardURLScheme") as? String
                ?? "pitboard"
            // The app this extension came in, not whichever copy Launch Services picks: a
            // debug extension reaches the debug app.
            guard let app = containingApp(of: Bundle.main.bundleURL) else {
                state.phase = .failed(
                    "pitboard couldn’t find the app this extension came with.")
                return
            }
            do {
                try await open(
                    pitboardLink(opening: link.absoluteString, scheme: scheme), with: app)
                extensionContext?.completeRequest(returningItems: [], completionHandler: nil)
            } catch {
                state.phase = .failed(error.localizedDescription)
            }
        }
    }

    /// The first web address among what the host shares.
    private func sharedURL() async -> URL? {
        let items = extensionContext?.inputItems.compactMap { $0 as? NSExtensionItem } ?? []
        let providers = items.flatMap { $0.attachments ?? [] }
        guard
            let provider = providers.first(where: {
                $0.hasItemConformingToTypeIdentifier(UTType.url.identifier)
            })
        else { return nil }
        return await withCheckedContinuation { done in
            _ = provider.loadObject(ofClass: URL.self) { url, _ in done.resume(returning: url) }
        }
    }

    private func open(_ link: URL, with app: URL) async throws {
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.activates = true
        try await withCheckedThrowingContinuation { (done: CheckedContinuation<Void, Error>) in
            NSWorkspace.shared.open(
                [link], withApplicationAt: app, configuration: configuration
            ) {
                _, error in
                if let error {
                    done.resume(throwing: error)
                } else {
                    done.resume()
                }
            }
        }
    }

    private func cancel() {
        extensionContext?.cancelRequest(
            withError: NSError(domain: NSCocoaErrorDomain, code: NSUserCancelledError))
    }
}
