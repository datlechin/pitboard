#if DEBUG
    import Foundation
    import WebKit

    /// What a fixture's claude.ai windows handed to macOS, and how long they were told to
    /// wait, kept so a test can see it and nothing is opened.
    @MainActor
    final class WebRecord {
        private(set) var opened: [URL] = []
        private(set) var pauses: [Duration] = []

        func open(_ url: URL) { opened.append(url) }
        func pause(_ duration: Duration) { pauses.append(duration) }
    }

    extension ClaudeWeb {
        /// The scheme a fixture's stand-in claude.ai is served under. WebKit does not let an
        /// app serve `https` itself, so the stand-in has an origin of its own, and the
        /// navigation policy is the same code in a fixture as in a live run.
        static let fixtureScheme = "pitboard-fixture"

        /// A fixture's claude.ai windows: one stand-in page for every address on
        /// `pitboard-fixture://claude.ai`, stores that live in memory, downloads in the
        /// fixture's temporary folder, and links to anywhere else recorded and opened
        /// nowhere. Nothing reaches claude.ai, and nothing is written under
        /// `~/Library/WebKit` of whoever runs the tests.
        static func fixture(record: WebRecord = WebRecord()) -> ClaudeWeb {
            ClaudeWeb(
                home: URL(string: "\(fixtureScheme)://claude.ai/")!,
                stores: FixtureStores(),
                downloads: FileManager.default.temporaryDirectory
                    .appendingPathComponent("pitboard-fixture/Downloads"),
                openElsewhere: { record.open($0) },
                prepare: { configuration in
                    configuration.setURLSchemeHandler(
                        FixturePageHandler(), forURLScheme: fixtureScheme)
                },
                pause: { duration in
                    record.pause(duration)
                    try? await Task.sleep(for: duration)
                })
        }
    }

    /// Stores that live in memory, one per identifier, gone when the app quits.
    @MainActor
    final class FixtureStores: WebStores {
        private var stores: [UUID: WKWebsiteDataStore] = [:]

        func store(for id: UUID) -> WKWebsiteDataStore {
            if let store = stores[id] { return store }
            let store = WKWebsiteDataStore.nonPersistent()
            stores[id] = store
            return store
        }

        func identifiers() async -> Set<UUID> { Set(stores.keys) }

        /// Every store here was made by this run.
        func recorded() -> Set<UUID> { Set(stores.keys) }

        func unrecord(_ id: UUID) {}

        func remove(_ id: UUID) async throws { stores[id] = nil }
    }

    /// Answers every request on the fixture's scheme with one page, which links outside
    /// claude.ai and to Google's sign-in so the policy can be seen at work by hand.
    @MainActor
    final class FixturePageHandler: NSObject, WKURLSchemeHandler {
        static let page = """
            <!doctype html><html><head><meta charset="utf-8"><title>claude.ai stand-in</title>\
            </head><body><h1>claude.ai stand-in</h1><p>A pitboard fixture page. Nothing here \
            reaches the network.</p><p><a href="https://example.com/">A link outside \
            claude.ai</a></p><p><a href="https://accounts.google.com/o/oauth2/v2/auth">\
            Continue with Google</a></p></body></html>
            """

        func webView(_ webView: WKWebView, start urlSchemeTask: any WKURLSchemeTask) {
            let body = Data(Self.page.utf8)
            let url = urlSchemeTask.request.url ?? URL(string: "\(ClaudeWeb.fixtureScheme):")!
            urlSchemeTask.didReceive(
                URLResponse(
                    url: url, mimeType: "text/html", expectedContentLength: body.count,
                    textEncodingName: "utf-8"))
            urlSchemeTask.didReceive(body)
            urlSchemeTask.didFinish()
        }

        func webView(_ webView: WKWebView, stop urlSchemeTask: any WKURLSchemeTask) {}
    }
#endif
