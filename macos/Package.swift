// swift-tools-version: 5.10
import PackageDescription
import Foundation

let packageDirectory = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path
let rustDebugLibraryPath = URL(fileURLWithPath: packageDirectory)
    .deletingLastPathComponent()
    .appendingPathComponent("rust/target/debug")
    .path
let rustReleaseLibraryPath = URL(fileURLWithPath: packageDirectory)
    .deletingLastPathComponent()
    .appendingPathComponent("rust/target/release")
    .path

let package = Package(
    name: "AetowerMac",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "AetowerApp", targets: ["AetowerApp"])
    ],
    targets: [
        .systemLibrary(
            name: "CAetowerFFI",
            path: "Sources/CAetowerFFI"
        ),
        .target(
            name: "AetowerBridge",
            dependencies: ["CAetowerFFI"],
            path: "Sources/AetowerBridge",
            linkerSettings: [
                .unsafeFlags(["-L", rustDebugLibraryPath, "-L", rustReleaseLibraryPath]),
                .linkedLibrary("aetower_ffi")
            ]
        ),
        .target(
            name: "AetowerUI",
            dependencies: ["AetowerBridge"],
            path: "Sources/AetowerUI"
        ),
        .executableTarget(
            name: "AetowerApp",
            dependencies: ["AetowerBridge", "AetowerUI"],
            path: "Sources/AetowerApp"
        )
    ]
)
