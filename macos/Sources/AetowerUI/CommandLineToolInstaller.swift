import Foundation

/// Installs the bundled `aetower` operator CLI onto the user's `$PATH`.
///
/// The binary ships inside the app bundle at `Contents/Helpers/aetower`, so it
/// arrives through every channel (pkg/dmg/zip/brew). Getting it onto `$PATH` is
/// a channel-specific link step: the flagship PKG writes `/usr/local/bin`,
/// Homebrew uses its cask `binary` stanza, and DMG/ZIP/manual installs use this
/// helper (the "Install Command Line Tool" Settings action). The helper also
/// repairs broken or missing symlink state after upgrades.
///
/// Aetower is not sandboxed, so it can create the symlink directly. When
/// `/usr/local/bin` needs administrator rights we don't silently escalate —
/// we surface the exact `sudo ln -sf …` command for the user to run.
enum CommandLineToolInstaller {
    static let linkPath = "/usr/local/bin/aetower"

    enum State: Equatable {
        /// The symlink exists and points at this bundle's CLI.
        case installed
        /// Nothing is installed at the link path.
        case notInstalled
        /// Something else occupies the link path (a different file or symlink).
        case conflict(target: String)
        /// The bundled CLI binary is missing (should never happen in a real build).
        case unavailable
    }

    /// The bundled CLI binary, if present.
    static func bundledCLIURL() -> URL? {
        let helpers = Bundle.main.bundleURL
            .appendingPathComponent("Contents/Helpers/aetower")
        return FileManager.default.isExecutableFile(atPath: helpers.path) ? helpers : nil
    }

    private static var linkURL: URL { URL(fileURLWithPath: linkPath) }

    /// Current install state, without modifying anything.
    static func currentState() -> State {
        guard let cli = bundledCLIURL() else { return .unavailable }
        let fileManager = FileManager.default
        if let destination = try? fileManager.destinationOfSymbolicLink(atPath: linkPath) {
            let resolved = URL(fileURLWithPath: destination).standardizedFileURL.path
            return resolved == cli.standardizedFileURL.path ? .installed : .conflict(target: destination)
        }
        if fileManager.fileExists(atPath: linkPath) {
            return .conflict(target: linkPath)
        }
        return .notInstalled
    }

    /// Create (or refresh) the `/usr/local/bin/aetower` symlink.
    @discardableResult
    static func install() throws -> String {
        guard let cli = bundledCLIURL() else {
            throw InstallError.unavailable
        }
        let fileManager = FileManager.default

        let dir = linkURL.deletingLastPathComponent()
        if !fileManager.fileExists(atPath: dir.path) {
            do {
                try fileManager.createDirectory(at: dir, withIntermediateDirectories: true)
            } catch {
                throw InstallError.needsAdmin(command: sudoCommand(cli: cli))
            }
        }

        // Replace only a prior symlink; never clobber an unrelated regular file.
        if (try? fileManager.destinationOfSymbolicLink(atPath: linkPath)) != nil {
            try? fileManager.removeItem(at: linkURL)
        } else if fileManager.fileExists(atPath: linkPath) {
            throw InstallError.conflict(path: linkPath)
        }

        do {
            try fileManager.createSymbolicLink(at: linkURL, withDestinationURL: cli)
        } catch {
            throw InstallError.needsAdmin(command: sudoCommand(cli: cli))
        }
        return "Installed: \(linkPath) → \(cli.path)"
    }

    /// Remove the symlink if it is one we created.
    @discardableResult
    static func uninstall() throws -> String {
        let fileManager = FileManager.default
        guard (try? fileManager.destinationOfSymbolicLink(atPath: linkPath)) != nil else {
            if fileManager.fileExists(atPath: linkPath) {
                throw InstallError.conflict(path: linkPath)
            }
            return "Not installed (\(linkPath) absent)."
        }
        do {
            try fileManager.removeItem(at: linkURL)
        } catch {
            throw InstallError.needsAdmin(command: "sudo rm \"\(linkPath)\"")
        }
        return "Removed \(linkPath)."
    }

    private static func sudoCommand(cli: URL) -> String {
        "sudo mkdir -p /usr/local/bin && sudo ln -sf \"\(cli.path)\" \"\(linkPath)\""
    }

    enum InstallError: LocalizedError {
        case unavailable
        case conflict(path: String)
        case needsAdmin(command: String)

        var errorDescription: String? {
            switch self {
            case .unavailable:
                return "The bundled aetower CLI wasn't found in this app."
            case let .conflict(path):
                return "\(path) already exists and isn't an aetower symlink; leaving it untouched."
            case let .needsAdmin(command):
                return "Couldn't write to /usr/local/bin. Run this in Terminal:\n\(command)"
            }
        }

        /// A copy-pasteable command for the admin-rights case, if any.
        var recoveryCommand: String? {
            if case let .needsAdmin(command) = self { return command }
            return nil
        }
    }
}
