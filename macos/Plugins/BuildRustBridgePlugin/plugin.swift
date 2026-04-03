import Foundation
import PackagePlugin

@main
struct BuildRustBridgePlugin: BuildToolPlugin {
    func createBuildCommands(
        context: PluginContext,
        target: Target
    ) throws -> [Command] {
        let repoRoot = context.package.directoryURL.deletingLastPathComponent()
        let script = repoRoot.appendingPathComponent("scripts/build-rust-ffi-for-swiftpm.sh")

        return [
            .prebuildCommand(
                displayName: "Build Rust bridge",
                executable: URL(fileURLWithPath: "/bin/zsh"),
                arguments: ["-lc", "sh '\(script.path)' '\(context.pluginWorkDirectoryURL.path)'"],
                outputFilesDirectory: context.pluginWorkDirectoryURL
            ),
        ]
    }
}
