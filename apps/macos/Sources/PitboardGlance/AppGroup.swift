import Foundation
import Security

// An ad hoc build has no team: macOS would ask the app and refuse the extension.
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

    // What the app last wrote, for the widget extension, which reads nothing else.
    public static func glance() -> Glance? {
        guard let own, let file = glanceFile(in: own), let data = try? Data(contentsOf: file)
        else { return nil }
        return Glance(data)
    }
}

extension GlanceSource {
    public static let group = GlanceSource { AppGroup.glance() }
}
