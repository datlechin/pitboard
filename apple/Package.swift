// swift-tools-version: 6.2
import PackageDescription

// Everything the app does, as libraries its tests can load without starting the app. The app
// itself is Pitboard.xcodeproj, which links PitboardApp, adds Sparkle, and carries the UI
// tests and the Share extension. The extension links PitboardLinks alone: it is sandboxed,
// and has no business with the core, its bindings or anything else the app links.
let package = Package(
    name: "Pitboard",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "PitboardKit", targets: ["PitboardKit"]),
        .library(name: "PitboardApp", targets: ["PitboardApp"]),
        .library(name: "PitboardLinks", targets: ["PitboardLinks"]),
    ],
    targets: [
        // Both built by scripts/build-xcframework.sh and not committed.
        .binaryTarget(name: "PitboardFFI", path: "PitboardFFI.xcframework"),
        .target(
            name: "PitboardBindings",
            dependencies: ["PitboardFFI"],
            // UniFFI's generated code does not yet meet Swift 6's strict concurrency checks.
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
        .target(name: "PitboardKit", dependencies: ["PitboardBindings"]),
        // What a claude.ai link from outside the app may be, and the pitboard link that
        // carries one: Foundation only, and nothing an app extension may not use.
        .target(name: "PitboardLinks"),
        .target(name: "PitboardApp", dependencies: ["PitboardKit", "PitboardLinks"]),
        .testTarget(name: "PitboardKitTests", dependencies: ["PitboardKit"]),
        .testTarget(name: "PitboardLinksTests", dependencies: ["PitboardLinks"]),
        .testTarget(name: "PitboardAppTests", dependencies: ["PitboardApp", "PitboardLinks"]),
    ]
)
