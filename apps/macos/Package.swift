// swift-tools-version: 6.2
import PackageDescription

// Everything the app does, as libraries its tests can load without starting the app.
// project.yml makes the app, its Share extension and its UI tests from them.

// Not on the generated bindings, whose warnings are UniFFI's.
let handWritten: [SwiftSetting] = [.treatAllWarnings(as: .error)]

let package = Package(
    name: "Pitboard",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "PitboardApp", targets: ["PitboardApp"]),
        .library(name: "PitboardShareBindings", targets: ["PitboardShareBindings"]),
        .library(name: "PitboardLinkTarget", targets: ["PitboardLinkTarget"]),
    ],
    targets: [
        // Each built with its bindings by scripts/build-xcframework.sh, and not committed.
        .binaryTarget(name: "PitboardFFI", path: "PitboardFFI.xcframework"),
        .binaryTarget(name: "PitboardShareFFI", path: "PitboardShareFFI.xcframework"),
        .target(
            name: "PitboardBindings",
            dependencies: ["PitboardFFI"],
            // UniFFI's generated code does not yet meet Swift 6's strict concurrency checks.
            swiftSettings: [.swiftLanguageMode(.v5)],
            // The core links the system's SQLite, and a static library carries no link line.
            linkerSettings: [.linkedLibrary("sqlite3")]
        ),
        // For the Share extension alone. No test target links it: SwiftPM may build every
        // test target into one bundle with PitboardFFI, and scripts/build-app.sh says why two
        // Rust static libraries must not share a binary.
        .target(
            name: "PitboardShareBindings",
            dependencies: ["PitboardShareFFI"],
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
        .target(
            name: "PitboardKit",
            dependencies: ["PitboardBindings"],
            swiftSettings: handWritten
        ),
        // Foundation only, so the app, its extension and a test target can each link it.
        .target(name: "PitboardLinkTarget", swiftSettings: handWritten),
        // No Rust, so a sandboxed extension that reads the glance can link it.
        .target(name: "PitboardGlance", swiftSettings: handWritten),
        .target(
            name: "PitboardApp",
            dependencies: ["PitboardKit", "PitboardLinkTarget", "PitboardGlance"],
            swiftSettings: handWritten
        ),
        // Only test targets depend on it.
        .target(
            name: "PitboardTestSupport",
            dependencies: ["PitboardKit"],
            path: "Tests/PitboardTestSupport",
            swiftSettings: handWritten
        ),
        .testTarget(
            name: "PitboardKitTests",
            dependencies: ["PitboardKit", "PitboardTestSupport"],
            swiftSettings: handWritten
        ),
        .testTarget(
            name: "PitboardLinkTargetTests",
            dependencies: ["PitboardLinkTarget"],
            swiftSettings: handWritten
        ),
        .testTarget(
            name: "PitboardGlanceTests",
            dependencies: ["PitboardGlance"],
            swiftSettings: handWritten
        ),
        .testTarget(
            name: "PitboardAppTests",
            dependencies: ["PitboardApp", "PitboardTestSupport"],
            swiftSettings: handWritten
        ),
    ]
)
