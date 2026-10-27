import Foundation
import Security

// Where the app keeps the glance for its widget extension: the App Group the running program
// is signed into under its own team, which macOS lets it into without asking. An ad hoc
// build has no team, and macOS would ask the app and silently refuse the extension.
public enum AppGroup {
    public static let own: String? = {
        var running: SecCode?
        var code: SecStaticCode?
        var signing: CFDictionary?
        let wanted = SecCSFlags(
            rawValue: kSecCSSigningInformation | kSecCSRequirementInformation)
        guard SecCodeCopySelf([], &running) == errSecSuccess, let running,
            SecCodeCopyStaticCode(running, [], &code) == errSecSuccess, let code,
            SecCodeCopySigningInformation(code, wanted, &signing) == errSecSuccess,
            let signing = signing as? [String: Any]
        else { return nil }
        let entitlements = signing[kSecCodeInfoEntitlementsDict as String] as? [String: Any]
        return named(
            groups: entitlements?["com.apple.security.application-groups"] as? [String] ?? [],
            team: signing[kSecCodeInfoTeamIdentifier as String] as? String)
    }()

    public static func named(groups: [String], team: String?) -> String? {
        guard let team else { return nil }
        return groups.first { $0.hasPrefix("\(team).") }
    }

    public static func glanceFile(in group: String) -> URL? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group)?
            .appending(path: "glance.json")
    }
}

extension GlanceSource {
    // What the app last wrote, for a widget extension, which reads nothing else.
    public static let group = GlanceSource {
        guard let group = AppGroup.own, let file = AppGroup.glanceFile(in: group),
            let data = try? Data(contentsOf: file)
        else { return nil }
        return Glance(data)
    }
}
