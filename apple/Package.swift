// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "Pitboard",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "PitboardKit", targets: ["PitboardKit"]),
        .executable(name: "Pitboard", targets: ["Pitboard"]),
    ],
    dependencies: [
        .package(url: "https://github.com/sparkle-project/Sparkle", from: "2.6.0")
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
        .executableTarget(
            name: "Pitboard",
            dependencies: ["PitboardKit", .product(name: "Sparkle", package: "Sparkle")],
            // Sparkle.framework is put in the bundle by scripts/build-app.sh.
            linkerSettings: [.unsafeFlags(["-Xlinker", "-rpath", "-Xlinker", "@executable_path/../Frameworks"])]
        ),
        .testTarget(name: "PitboardKitTests", dependencies: ["PitboardKit"]),
        .testTarget(name: "PitboardTests", dependencies: ["Pitboard"]),
    ]
)
