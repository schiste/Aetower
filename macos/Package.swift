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
let rustDebugLibrary = URL(fileURLWithPath: rustDebugLibraryPath)
    .appendingPathComponent("libaetower_ffi.dylib")
    .path

func ensureRustBridgeLibrary() {
    guard !FileManager.default.fileExists(atPath: rustDebugLibrary) else {
        return
    }

    let rootDirectory = URL(fileURLWithPath: packageDirectory).deletingLastPathComponent().path
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    process.arguments = ["cargo", "build", "--manifest-path", "\(rootDirectory)/rust/Cargo.toml", "-p", "aetower-ffi"]
    process.currentDirectoryURL = URL(fileURLWithPath: rootDirectory)

    do {
        try process.run()
        process.waitUntilExit()
    } catch {
        fatalError("Failed to start Rust bridge build: \(error)")
    }

    guard process.terminationStatus == 0 else {
        fatalError("Rust bridge build failed with status \(process.terminationStatus)")
    }
}

ensureRustBridgeLibrary()

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
