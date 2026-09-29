import Foundation
import Testing

/// `apple/`, found from this file.
private let apple = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()

private func plist(_ path: String) throws -> [String: Any] {
    let data = try Data(contentsOf: apple.appendingPathComponent(path))
    let read = try PropertyListSerialization.propertyList(from: data, format: nil)
    return try #require(read as? [String: Any])
}

/// The app claims one scheme, the project's per configuration, and offers one Service that
/// takes a selection with a link in it, returns nothing and takes no key of its own.
@Test func theAppClaimsItsSchemeAndOffersOneService() throws {
    let info = try plist("App/Info.plist")
    let types = try #require(info["CFBundleURLTypes"] as? [[String: Any]])
    #expect(types.count == 1)
    #expect(types.first?["CFBundleURLSchemes"] as? [String] == ["$(PITBOARD_URL_SCHEME)"])
    #expect(types.first?["CFBundleTypeRole"] as? String == "Viewer")

    let services = try #require(info["NSServices"] as? [[String: Any]])
    #expect(services.count == 1)
    let service = try #require(services.first)
    #expect(service["NSMessage"] as? String == "openClaudeLink")
    #expect(
        service["NSSendTypes"] as? [String] == ["public.utf8-plain-text", "public.url"],
        "Chromium offers a selection only for exactly public.utf8-plain-text")
    #expect(
        (service["NSRequiredContext"] as? [String: String])?["NSTextContent"] == "URL",
        "without a required context the system leaves a service out of the menu")
    #expect(
        (service["NSMenuItem"] as? [String: String])?["default"]
            == "$(PITBOARD_SERVICE_TITLE)")
    #expect(service["NSReturnTypes"] == nil)
    #expect(service["NSKeyEquivalent"] == nil)
    #expect(info["NSDownloadsFolderUsageDescription"] is String)
    #expect(info["NSMicrophoneUsageDescription"] == nil, "the microphone is refused")
    #expect(info["LSUIElement"] as? Bool == true)
}

@Test func theShareExtensionTakesOneWebLink() throws {
    let info = try plist("ShareExtension/Info.plist")
    let point = try #require(info["NSExtension"] as? [String: Any])
    #expect(point["NSExtensionPointIdentifier"] as? String == "com.apple.share-services")
    #expect(
        point["NSExtensionPrincipalClass"] as? String
            == "$(PRODUCT_MODULE_NAME).ShareViewController")
    let attributes = try #require(point["NSExtensionAttributes"] as? [String: Any])
    let rule = try #require(attributes["NSExtensionActivationRule"] as? [String: Int])
    #expect(rule == ["NSExtensionActivationSupportsWebURLWithMaxCount": 1])
    #expect(info["PitboardURLScheme"] as? String == "$(PITBOARD_URL_SCHEME)")
    #expect(info["CFBundlePackageType"] as? String == "XPC!")
    let text = try String(
        contentsOf: apple.appendingPathComponent("ShareExtension/Info.plist"), encoding: .utf8)
    #expect(!text.contains("TRUEPREDICATE"))
}

/// An app extension must be sandboxed, and this one asks for nothing else: no network, no
/// files, no group shared with the app.
@Test func theShareExtensionIsSandboxedAndAsksForNothingElse() throws {
    let entitlements = try plist("ShareExtension/PitboardShare.entitlements")
    #expect(entitlements.count == 1)
    #expect(entitlements["com.apple.security.app-sandbox"] as? Bool == true)
}

/// The project claims `pitboard-debug` for a debug build and `pitboard` for a release, and
/// names the Service and the extension so the two cannot be taken for each other.
@Test func eachBuildClaimsASchemeOfItsOwn() throws {
    let project = try String(
        contentsOf: apple.appendingPathComponent("Pitboard.xcodeproj/project.pbxproj"),
        encoding: .utf8)
    #expect(project.contains("PITBOARD_URL_SCHEME = \"pitboard-debug\";"))
    #expect(project.contains("PITBOARD_URL_SCHEME = pitboard;"))
    #expect(project.contains("PITBOARD_SERVICE_TITLE = \"Open in pitboard Debug\";"))
    #expect(project.contains("PITBOARD_SERVICE_TITLE = \"Open in pitboard\";"))
    #expect(project.contains("PITBOARD_SHARE_NAME = \"pitboard Debug\";"))
    #expect(project.contains("PITBOARD_SHARE_NAME = pitboard;"))
    #expect(
        project.contains("CODE_SIGN_ENTITLEMENTS = ShareExtension/PitboardShare.entitlements;"))
    #expect(project.contains("APPLICATION_EXTENSION_API_ONLY = YES;"))
}
