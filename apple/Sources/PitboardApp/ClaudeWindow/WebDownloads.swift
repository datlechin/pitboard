import AppKit
import Foundation
import WebKit

/// Every download of every claude.ai window, from its start to its end.
///
/// `WKDownload` keeps its delegate weakly, and nothing in WebKit keeps a download for the
/// app. A page that answered for its own downloads took them with it when its window closed:
/// one still deciding where to save was cancelled, and one already saving finished without a
/// word or its quarantine. So this lives as long as the model, keeps each download until it
/// ends, and says what happens in the window of the download's account while one is open.
///
/// A download holds its account's store, and WebKit refuses to delete a store in use, so
/// signing a window out, forgetting its account or sweeping its store stops its downloads
/// first.
@MainActor
final class WebDownloads: NSObject {
    /// Where downloads are saved.
    let folder: URL
    /// The page of a store's window while one is open, which the note bar and the question
    /// before a download belong to.
    var page: (UUID) -> ClaudePage? = { _ in nil }

    private struct Saving {
        /// Kept here, since nothing else keeps a download.
        let transfer: AnyObject
        let store: UUID
        /// Whether the person is asked before it is saved.
        let asks: Bool
        /// The site the question names.
        let host: String?
        let stop: @MainActor () async -> Void
        /// Where it is being saved, once that is decided.
        var file: URL?
        /// The question showing for it, taken down if the download is stopped meanwhile.
        var question: NSAlert?
    }

    private var saving: [ObjectIdentifier: Saving] = [:]

    init(folder: URL) {
        self.folder = folder
    }

    /// Keeps `download` of `store`'s window until it ends, and answers for it. `asking` says
    /// to ask the person before saving it, naming the site of the frame `origin` names.
    func keep(_ download: WKDownload, of store: UUID, asking: Bool, origin: String? = nil) {
        download.delegate = self
        let host = downloadHost(frame: origin, url: download.originalRequest?.url)
        keep(download, of: store, asking: asking, host: host) { _ = await download.cancel() }
    }

    /// `keep`, for anything that downloads and can be stopped, which a test stands in for
    /// WebKit's.
    func keep(
        _ transfer: AnyObject, of store: UUID, asking: Bool = false, host: String? = nil,
        stop: @escaping @MainActor () async -> Void
    ) {
        saving[ObjectIdentifier(transfer)] = Saving(
            transfer: transfer, store: store, asks: asking, host: host, stop: stop)
    }

    /// Whether a download of `store`'s window is under way.
    func isSaving(for store: UUID) -> Bool {
        saving.values.contains { $0.store == store }
    }

    /// Stops every download of `store`'s window and deletes what each had saved, ahead of
    /// deleting the store. A download stopped here says nothing more.
    func stop(for store: UUID) async {
        for (id, entry) in saving where entry.store == store {
            saving[id] = nil
            if let question = entry.question, let window = question.window.sheetParent {
                window.endSheet(question.window, returnCode: .cancel)
            }
            await entry.stop()
            if let file = entry.file { try? FileManager.default.removeItem(at: file) }
        }
    }

    private func tell(_ store: UUID, _ note: WebNote) {
        page(store)?.note = note
    }

    /// Where a download is saved: the name the server suggests, numbered while a file or
    /// another download has it.
    private func destination(suggested: String) -> URL {
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let reserved = Set(saving.values.compactMap(\.file))
        return downloadDestination(suggested: suggested, in: folder) {
            downloadTaken($0, reserved: reserved)
        }
    }

    /// Asks the person, in the window of the download's account, before a download a frame
    /// or another site started is saved. No window to ask in is a no.
    private func ask(_ id: ObjectIdentifier, name: String) async -> Bool {
        guard let entry = saving[id], let window = page(entry.store)?.window else {
            return false
        }
        let alert = NSAlert()
        alert.messageText = downloadQuestion(name: name, host: entry.host)
        alert.informativeText = "pitboard saves it to your Downloads folder."
        alert.addButton(withTitle: "Download")
        alert.addButton(withTitle: "Cancel")
        saving[id]?.question = alert
        let response = await withCheckedContinuation { continuation in
            alert.beginSheetModal(for: window) { continuation.resume(returning: $0) }
        }
        saving[id]?.question = nil
        return response == .alertFirstButtonReturn
    }
}

extension WebDownloads: WKDownloadDelegate {
    func download(
        _ download: WKDownload, decideDestinationUsing response: URLResponse,
        suggestedFilename: String,
        completionHandler: @escaping @MainActor @Sendable (URL?) -> Void
    ) {
        let id = ObjectIdentifier(download)
        guard let entry = saving[id] else { return completionHandler(nil) }
        guard entry.asks else {
            return save(id, suggested: suggestedFilename, completionHandler)
        }
        Task {
            let name = downloadDestination(suggested: suggestedFilename, in: folder) { _ in
                false
            }
            guard await ask(id, name: name.lastPathComponent), saving[id] != nil else {
                // Nil cancels it, and a download cancelled here says nothing more.
                saving[id] = nil
                return completionHandler(nil)
            }
            save(id, suggested: suggestedFilename, completionHandler)
        }
    }

    private func save(
        _ id: ObjectIdentifier, suggested: String,
        _ completionHandler: @escaping @MainActor @Sendable (URL?) -> Void
    ) {
        guard let store = saving[id]?.store else { return completionHandler(nil) }
        let file = destination(suggested: suggested)
        saving[id]?.file = file
        tell(store, .downloading(name: file.lastPathComponent))
        completionHandler(file)
    }

    func downloadDidFinish(_ download: WKDownload) {
        guard let entry = saving.removeValue(forKey: ObjectIdentifier(download)),
            let file = entry.file
        else { return }
        Self.quarantine(file, from: download.originalRequest?.url)
        tell(entry.store, .downloaded(name: file.lastPathComponent, file: file))
    }

    func download(_ download: WKDownload, didFailWithError error: Error, resumeData: Data?) {
        guard let entry = saving.removeValue(forKey: ObjectIdentifier(download)) else { return }
        let name =
            entry.file?.lastPathComponent ?? download.originalRequest?.url?.lastPathComponent
        tell(
            entry.store,
            .downloadFailed(name: name ?? "Download", reason: error.localizedDescription))
    }

    /// Marks `file` as downloaded from the web, unless it already is.
    private static func quarantine(_ file: URL, from origin: URL?) {
        var file = file
        let existing = try? file.resourceValues(forKeys: [.quarantinePropertiesKey])
        guard existing?.quarantineProperties == nil else { return }
        var values = URLResourceValues()
        values.quarantineProperties = PitboardApp.quarantine(downloadedFrom: origin).properties
        try? file.setResourceValues(values)
    }
}
