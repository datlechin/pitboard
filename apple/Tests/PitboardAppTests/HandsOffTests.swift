import Foundation
import Testing
import WebKit

@testable import PitboardApp

/// `apple/`, found from this file.
private let apple = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()

/// Every Swift file under `folder`, relative to `apple/`, with its text.
private func sources(in folder: String) throws -> [(path: String, text: String)] {
    let root = apple.appendingPathComponent(folder)
    let files =
        FileManager.default.enumerator(at: root, includingPropertiesForKeys: nil)?
        .compactMap { $0 as? URL }.filter { $0.pathExtension == "swift" } ?? []
    return try files.map { file in
        let path = String(
            file.standardizedFileURL.path.dropFirst(apple.standardizedFileURL.path.count + 1))
        return (path, try String(contentsOf: file, encoding: .utf8))
    }
}

/// What would read, copy, change or script what claude.ai keeps in a window, pose as
/// another browser, or share a store with the default one. Swift adds a message handler
/// with `add(_:name:)`, which names none of these, so the controller it is added to and the
/// handler's protocol are named instead.
private let webNames = [
    "httpCookieStore", "WKHTTPCookieStore", "fetchDataRecords", "removeData(ofTypes",
    "fetchData(of", "restoreData", "evaluateJavaScript", "callAsyncJavaScript", "WKUserScript",
    "addScriptMessageHandler", "customUserAgent", "applicationNameForUserAgent",
    "userContentController", "WKScriptMessageHandler", "WKContentWorld",
    "WKWebsiteDataStore.default",
]

/// What would read a browser's own data, or the keychain.
private let browserNames = [
    "HTTPCookieStorage", "Cookies.binarycookies", "cookies.sqlite", "Library/Safari",
    "Library/Cookies", "Application Support/Google", "Application Support/Firefox",
    "Application Support/Arc", "/usr/bin/security", "SecItem",
]

/// The brief's hands-off rule, guarded the way CI guards a secret: pitboard never reads,
/// copies or changes what claude.ai keeps, runs no script in the page, poses as no other
/// browser, and no way in reads any browser's data. A name here in any source file, a
/// comment included, fails.
@Test func theAppNeverTouchesWhatClaudeKeeps() throws {
    let app = try sources(in: "Sources/PitboardApp")
    let links = try sources(in: "Sources/PitboardLinks")
    let share = try sources(in: "ShareExtension")
    let target = try sources(in: "App")
    let kit = try sources(in: "Sources/PitboardKit")
    #expect(app.count > 20)
    #expect(!links.isEmpty)
    #expect(!share.isEmpty)
    #expect(!target.isEmpty)
    #expect(!kit.isEmpty)
    for (path, text) in app + links + share + target + kit {
        for name in webNames + browserNames {
            #expect(!text.contains(name), "\(path) names \(name)")
        }
        // The one AppleScript the app runs asks for an administrator to link the command
        // line; nothing else runs one.
        if path != "Sources/PitboardApp/System/CommandLineTool.swift" {
            #expect(!text.contains("NSAppleScript"), "\(path) runs AppleScript")
        }
    }
}

/// Set up as a browser's would be, and nothing more: the account's store, no script and no
/// message handler added to the page, windows opened only by a person, and WebKit's own user
/// agent.
@MainActor
@Test func theWebViewIsConfiguredAsABrowserWouldBe() {
    let store = WKWebsiteDataStore.nonPersistent()
    let configuration = ClaudePage.configuration(store: store, web: .standIn())
    #expect(configuration.websiteDataStore === store)
    #expect(configuration.userContentController.userScripts.isEmpty)
    #expect(!configuration.preferences.javaScriptCanOpenWindowsAutomatically)
    #expect(
        configuration.applicationNameForUserAgent
            == WKWebViewConfiguration().applicationNameForUserAgent)
}
