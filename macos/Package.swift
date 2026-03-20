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
            name: "aetower_ffiFFI",
            path: "Sources/aetower_ffiFFI"
        ),
        .target(
            name: "AetowerBindings",
            dependencies: ["aetower_ffiFFI"],
            path: "Sources/AetowerBindings",
            linkerSettings: [
                .unsafeFlags(["-L", rustDebugLibraryPath, "-L", rustReleaseLibraryPath]),
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
