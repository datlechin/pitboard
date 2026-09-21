// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "Pitboard",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "PitboardKit", targets: ["PitboardKit"]),
        .executable(name: "Pitboard", targets: ["Pitboard"]),
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
        .executableTarget(name: "Pitboard", dependencies: ["PitboardKit"]),
        .testTarget(name: "PitboardKitTests", dependencies: ["PitboardKit"]),
    ]
)
