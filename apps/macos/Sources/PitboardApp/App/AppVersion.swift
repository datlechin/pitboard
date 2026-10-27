import AppKit

public enum AppVersion {
    static let key = "PitboardVersion"

    public static func whole(in bundle: Bundle = .main) -> String? {
        let version = bundle.object(forInfoDictionaryKey: key) as? String
        return version?.isEmpty == false ? version : nil
    }

    @MainActor
    static func showAboutPanel() {
        NSApp.activate()
        NSApp.orderFrontStandardAboutPanel(options: aboutPanel(in: .main))
    }

    static func aboutPanel(in bundle: Bundle) -> [NSApplication.AboutPanelOptionKey: Any] {
        whole(in: bundle).map { [.applicationVersion: $0] } ?? [:]
    }

    // SPUStandardVersionDisplay's rule, with the whole version in place of Sparkle's X.Y.Z.
    public static func inSparkle(
        update: (version: String, build: String), copy: (version: String, build: String),
        in bundle: Bundle = .main
    ) -> (update: String, copy: String) {
        let mine = whole(in: bundle) ?? copy.version
        guard update.version == mine else { return (update.version, mine) }
        return ("\(update.version) (\(update.build))", "\(mine) (\(copy.build))")
    }
}
