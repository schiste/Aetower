// swift-tools-version: 6.0
import PackageDescription
import Foundation

let packageDirectory = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path
let pluginOutputDirectory = URL(fileURLWithPath: packageDirectory)
    .appendingPathComponent(".build/plugins/outputs/macos/AetowerBindings/destination/BuildRustBridgePlugin")
    .path
let rustDebugLibraryPath = URL(fileURLWithPath: pluginOutputDirectory)
    .appendingPathComponent("debug")
    .path
let rustReleaseLibraryPath = URL(fileURLWithPath: pluginOutputDirectory)
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
        .plugin(
            name: "BuildRustBridgePlugin",
            capability: .buildTool(),
            path: "Plugins/BuildRustBridgePlugin"
        ),
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
            ],
            plugins: ["BuildRustBridgePlugin"]
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
