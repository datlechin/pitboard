// What SwiftUI gives no identifier (docs/facts/macos-app.md), found by the words words.rs pins.
enum Words {
    // The menu bar item's menu and the app's own menus.
    static let openPitboard = "Open Pitboard"
    static let settings = "Settings…"
    static let quit = "Quit Pitboard"
    static let addAccount = "Add Account…"
    static let refresh = "Refresh"
    static let copyEmail = "Copy Email Address"
    static let accountMenu = "Account"
    static let fileMenu = "File"
    static let removeWebsiteData = "Remove Website Data…"
    static let claudeCodeMissing = "Claude Code Isn’t Installed"
    static let couldNotReadUsage = "Couldn’t read usage"
    static let switchWaiting = "A switch didn’t finish"

    static func openSite(_ site: String) -> String { "Open \(site)" }

    static func openSite(_ site: String, as label: String) -> String {
        "Open \(site) as \(label)"
    }

    static func inMenu(_ label: String, plan: String) -> String { "\(label) · \(plan)" }

    // An account's own menu and the Account menu.
    static func use(_ label: String) -> String { "Switch to \(label)" }
    static let rename = "Rename…"
    static let forget = "Remove Account…"
    static let warmUpNow = "Warm Up Now"

    // The Settings window's panes, and what a pop-up there offers.
    static let general = "General"
    static let limits = "Limits"
    static let advanced = "Advanced"
    static let notifyMe = "Notify Me"
    static let switchAutomatically = "Switch Automatically"
    static let doNothing = "Do Nothing"

    // What a row and a download say once something is over, which nothing else shows.
    static let warmsUpAgain = "Warms up again at "
    static let downloaded = "Downloaded"
}
