// swift-tools-version: 6.0
import PackageDescription
import Foundation

let packageDirectory = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path
// The Rust build runs explicitly before Swift. Link its outputs directly:
// staging them through a plugin also made SwiftPM treat dylibs as resources.
let rustTargetDirectory = URL(fileURLWithPath: packageDirectory)
    .appendingPathComponent("../rust/target")
    .standardizedFileURL
    .path
let rustDebugLibraryPath = URL(fileURLWithPath: rustTargetDirectory)
    .appendingPathComponent("debug")
    .path
let rustReleaseLibraryPath = URL(fileURLWithPath: rustTargetDirectory)
    .appendingPathComponent("release")
    .path

let package = Package(
    name: "AetowerMac",
    platforms: [
        .macOS(.v15)
    ],
    products: [
        .executable(name: "AetowerApp", targets: ["AetowerApp"])
    ],
    dependencies: [
        .package(url: "https://github.com/sparkle-project/Sparkle", from: "2.9.0")
    ],
    targets: [
        .systemLibrary(
            name: "aetower_ffiFFI",
            path: "Sources/aetower_ffiFFI"
        ),
        .target(
            name: "AetowerBindings",
            dependencies: ["aetower_ffiFFI"],
            path: "Sources/AetowerBindings",
            linkerSettings: [
                // Match the Rust profile to the Swift configuration; listing both
                // (debug first) made release builds link and load the debug engine.
                .unsafeFlags([
                    "-L", rustDebugLibraryPath,
                    "-Xlinker", "-rpath",
                    "-Xlinker", rustDebugLibraryPath,
                ], .when(configuration: .debug)),
                .unsafeFlags([
                    "-L", rustReleaseLibraryPath,
                    "-Xlinker", "-rpath",
                    "-Xlinker", rustReleaseLibraryPath,
                ], .when(configuration: .release)),
                .linkedLibrary("aetower_ffi")
            ]
        ),
        .target(
            name: "AetowerBridge",
            dependencies: ["AetowerBindings"],
            path: "Sources/AetowerBridge"
        ),
        .target(
            name: "AetowerUI",
            dependencies: [
                "AetowerBridge",
                .product(name: "Sparkle", package: "Sparkle"),
            ],
            path: "Sources/AetowerUI",
            resources: [
                .copy("Resources/Aethyme")
            ]
        ),
        .executableTarget(
            name: "AetowerApp",
            dependencies: ["AetowerBridge", "AetowerUI"],
            path: "Sources/AetowerApp"
        ),
        .testTarget(
            name: "AetowerUITests",
            dependencies: ["AetowerUI"],
            path: "Tests/AetowerUITests"
        )
    ]
)
