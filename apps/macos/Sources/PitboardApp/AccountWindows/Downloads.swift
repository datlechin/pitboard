import AppKit
import Foundation
import PitboardKit
import WebKit

/// Downloads carry on after their window closes: the person asked for the file, not for the
/// window to stay open. WebKit quarantines each file itself, as a browser's are, and makes a
/// suggested name safe. What is left here is never overwriting a file, whose name the core's
/// `downloadDestination` gives, and asking before a frame or a page off the site saves one.
@MainActor
final class DownloadCenter: NSObject, WKDownloadDelegate {
    private struct Live {
        let download: WKDownload
        /// Whether to ask the person first, until WebKit asks where it goes.
        var asking: Bool?
        var destination: URL?
    }

    private let folder: URL
    private let model: AppModel
    private var live: [String: Live] = [:]
    /// Destinations given to downloads still running, which no other may be given.
    private var reserved: Set<URL> = []

    init(folder: URL, model: AppModel) {
        self.folder = folder
        self.model = model
    }

    func start(_ download: WKDownload, for account: WindowAccount, asking: Bool) {
        let id = UUID().uuidString
        live[id] = Live(download: download, asking: asking)
        download.delegate = self
        model.send(
            .downloadStarted(
                id: id, store: account.store,
                name: download.originalRequest?.url?.lastPathComponent))
    }

    func progress(of id: String) -> Progress? {
        live[id]?.download.progress
    }

    /// Counted here rather than from the snapshot: a download started a moment before Quit is
    /// under way before the snapshot that lists it.
    var quitQuestion: Question? {
        downloadsQuitQuestion(front: front, running: UInt32(clamping: live.count))
    }

    func reveal(_ file: String) {
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: file)])
    }

    func stopAll() {
        for (id, running) in live {
            running.download.cancel()
            if let file = running.destination { try? FileManager.default.removeItem(at: file) }
            model.send(.downloadEnded(id: id, end: .cancelled))
        }
        live = [:]
    }

    /// WebKit says nothing more about a download the app cancels.
    func cancel(_ id: String) {
        guard let running = live.removeValue(forKey: id) else { return }
        model.send(.downloadEnded(id: id, end: .cancelled))
        let file = running.destination
        running.download.cancel { [weak self] _ in
            // WebKit does not say which thread this is called on.
            Task { @MainActor [weak self] in
                guard let file else { return }
                try? FileManager.default.removeItem(at: file)
                self?.reserved.remove(file)
            }
        }
    }

    // MARK: - WebKit's questions

    func download(
        _ download: WKDownload, decideDestinationUsing response: URLResponse,
        suggestedFilename: String
    ) async -> URL? {
        guard let id = id(of: download), let ask = live[id]?.asking else { return nil }
        live[id]?.asking = nil
        if ask {
            let host = Self.host(of: download)
            let allowed = await PageDialogs.allowDownload(
                suggestedFilename, from: host, in: download.webView?.window)
            guard allowed else {
                if live.removeValue(forKey: id) != nil {
                    model.send(.downloadEnded(id: id, end: .cancelled))
                }
                return nil
            }
        }
        // It may have been cancelled, or failed, while the question was up.
        guard live[id] != nil else { return nil }
        let destination = URL(
            fileURLWithPath: downloadDestination(
                front: front, folder: folder.path, suggested: suggestedFilename,
                reserved: reserved.map(\.path)))
        reserved.insert(destination)
        live[id]?.destination = destination
        model.send(.downloadSaving(id: id, file: destination.path))
        return destination
    }

    func downloadDidFinish(_ download: WKDownload) {
        guard let id = id(of: download), let file = live[id]?.destination else { return }
        live[id] = nil
        reserved.remove(file)
        model.send(.downloadEnded(id: id, end: .finished))
    }

    func download(_ download: WKDownload, didFailWithError error: Error, resumeData: Data?) {
        guard let id = id(of: download), let failed = live.removeValue(forKey: id) else {
            return
        }
        let cancelled = (error as NSError).code == NSURLErrorCancelled
        if let file = failed.destination {
            // Part of a file under its real name would pass for the whole of it.
            if !cancelled { try? FileManager.default.removeItem(at: file) }
            reserved.remove(file)
        }
        model.send(
            .downloadEnded(
                id: id,
                end: cancelled ? .cancelled : .failed(reason: error.localizedDescription))
        )
    }

    private func id(of download: WKDownload) -> String? {
        live.first { $0.value.download === download }?.key
    }

    // MARK: - Names

    /// The site a download comes from, as its question names it: the host of the frame that
    /// started it where macOS says, else the core's reading of its address, which reads the
    /// host inside a `blob:` link.
    static func host(of download: WKDownload) -> String? {
        // A download the app started has an empty frame, whose host is empty.
        if #available(macOS 15.2, *) {
            let host = download.originatingFrame.securityOrigin.host
            if !host.isEmpty { return host }
        }
        return downloadHost(url: download.originalRequest?.url?.absoluteString)
    }
}
