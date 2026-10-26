import AppKit
import Foundation
import Testing

@testable import PitboardApp

private func withApp(_ info: [String: String], _ body: (Bundle) throws -> Void) throws {
    let folder = FileManager.default.temporaryDirectory
        .appending(path: "AppVersionTests-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: folder) }
    let contents = folder.appending(path: "Pitboard.app/Contents")
    try FileManager.default.createDirectory(at: contents, withIntermediateDirectories: true)
    var plist = info
    plist["CFBundleIdentifier"] = "com.usepitboard.tests.\(UUID().uuidString)"
    try PropertyListSerialization.data(fromPropertyList: plist, format: .xml, options: 0)
        .write(to: contents.appending(path: "Info.plist"))
    try body(try #require(Bundle(url: folder.appending(path: "Pitboard.app"))))
}

private let beta = [
    "CFBundleShortVersionString": "1.0.0", "CFBundleVersion": "1000001",
    AppVersion.key: "1.0.0-beta.1",
]
private let release = [
    "CFBundleShortVersionString": "0.15.0", "CFBundleVersion": "150099",
    AppVersion.key: "0.15.0",
]

@Test func aBetaShowsItsWholeVersion() throws {
    try withApp(beta) { bundle in
        #expect(AppVersion.whole(in: bundle) == "1.0.0-beta.1")
        let about = AppVersion.aboutPanel(in: bundle)
        #expect(about.count == 1)
        #expect(about[.applicationVersion] as? String == "1.0.0-beta.1")
    }
}

@Test func aReleaseShowsItsVersion() throws {
    try withApp(release) { bundle in
        #expect(AppVersion.whole(in: bundle) == "0.15.0")
        #expect(
            AppVersion.aboutPanel(in: bundle)[.applicationVersion] as? String == "0.15.0"
        )
    }
}

@Test func aBuildWithNoVersionShowsNone() throws {
    try withApp([AppVersion.key: ""]) { bundle in
        #expect(AppVersion.whole(in: bundle) == nil)
        #expect(AppVersion.aboutPanel(in: bundle).isEmpty)
    }
    try withApp([:]) { bundle in
        #expect(AppVersion.whole(in: bundle) == nil)
    }
}

@Test func sparkleNamesABetaByItsWholeVersion() throws {
    try withApp(beta) { bundle in
        let shown = AppVersion.inSparkle(
            update: ("1.0.0", "1000099"), copy: ("1.0.0", "1000001"), in: bundle)
        #expect(shown.update == "1.0.0")
        #expect(shown.copy == "1.0.0-beta.1")
    }
}

// What Sparkle's own display says, which a release keeps.
@Test func sparkleNamesAReleaseAsItsOwnDisplayDoes() throws {
    try withApp(release) { bundle in
        let newer = AppVersion.inSparkle(
            update: ("0.16.0", "160099"), copy: ("0.15.0", "150099"), in: bundle)
        #expect(newer.update == "0.16.0")
        #expect(newer.copy == "0.15.0")
        let same = AppVersion.inSparkle(
            update: ("0.15.0", "150099"), copy: ("0.15.0", "150099"), in: bundle)
        #expect(same.update == "0.15.0 (150099)")
        #expect(same.copy == "0.15.0 (150099)")
    }
}

@Test func theAppDeclaresItsWholeVersionUnderItsOwnKey() throws {
    let macos = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    let data = try Data(contentsOf: macos.appending(path: "App/Info.plist"))
    let info = try #require(
        try PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any])
    #expect(info["CFBundleShortVersionString"] as? String == "$(MARKETING_VERSION)")
    #expect(info[AppVersion.key] as? String == "$(PITBOARD_VERSION)")
}
