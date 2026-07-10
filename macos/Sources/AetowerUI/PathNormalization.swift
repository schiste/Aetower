import Foundation

/// Canonical directory-path normalization for user-supplied roots: trims
/// whitespace, expands `~` / `~/`, and standardizes through
/// `URL.standardizedFileURL` so paths compare and persist consistently.
/// Every place that needs this rule must call it here — the copy-paste
/// versions kept drifting apart before they were consolidated.
enum PathNormalization {
    static func standardizedDirectoryPath(_ path: String) -> String {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return "" }
        let expanded: String
        if trimmed == "~" {
            expanded = FileManager.default.homeDirectoryForCurrentUser.path
        } else if trimmed.hasPrefix("~/") {
            expanded = FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent(String(trimmed.dropFirst(2)))
                .path
        } else {
            expanded = trimmed
        }
        return URL(fileURLWithPath: expanded, isDirectory: true).standardizedFileURL.path
    }
}
