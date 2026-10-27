import Foundation

// What SwiftUI gives no identifier (docs/facts/macos-app.md), found by the words words.rs pins.
enum Words {
    // The menu bar item's menu and the app's own menus.
    static let openPitboard = "Open Pitboard"
    static let settings = "Settings…"
    static let quit = "Quit Pitboard"
    // A debug build's CFBundleDisplayName: the app menu's Hide and Quit, and from macOS 27 the menu.
    static let debugBuild = "Pitboard Debug"
    // CFBundleName names the app menu on macOS 26.
    static let appMenu =
        ProcessInfo.processInfo.isOperatingSystemAtLeast(
            OperatingSystemVersion(majorVersion: 27, minorVersion: 0, patchVersion: 0))
        ? debugBuild : "Pitboard"
    static let addAccount = "Add Account…"
    static let refresh = "Refresh"
    static let copyEmail = "Copy Email Address"
    static let accountMenu = "Account"
    static let fileMenu = "File"
    static let editMenu = "Edit"
    static let viewMenu = "View"
    static let copyReport = "Copy Report"
    static let removeWebsiteData = "Remove Website Data…"
    static let windowMenu = "Window"
    static let diagnostics = "Diagnostics"
    static let activity = "Activity"
    static let claudeCodeMissing = "Claude Code Isn’t Installed"
    // The menu's item for a notice, with the notice's heading under it.
    static let oneIssue = "1 Issue"

    static func openSite(_ site: String) -> String { "Open \(site)" }

    static func openSite(_ site: String, as label: String) -> String {
        "Open \(site) as \(label)"
    }

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

    // The app's own questions, AppKit alerts: Uninstall's, then the one once Pitboard's folder
    // is gone.
    static let uninstall = "Uninstall"
    static let moveToTrash = "Move to Trash"

    // The inspector with nothing selected.
    static let noAccountSelected = "No Account Selected"

    // What Name and Rename say under an empty field.
    static let typeAName = "Type a name for this account."

    // What the inspector and a download say once something is over, which nothing else shows.
    static let warmsUpAgain = "Warms up again at "
    static let downloaded = "Downloaded"
}
