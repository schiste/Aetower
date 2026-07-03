import Foundation

/// Single source of truth for moving paths to the Finder Trash and emptying it.
/// Extracted so every surface that reclaims disk (Storage hygiene, Repository
/// artifact cleanup) shares one audited implementation instead of each rolling
/// its own FileManager calls.
///
/// Trash is reversible (Put Back / undo), which is what makes one-click
/// reclaim defensible; emptying the Trash is the only permanent step and is
/// always gated by an explicit confirmation at the call site.
enum TrashService {
    /// Outcome of trashing a single path. `trashURL` is where Finder placed the
    /// item (used to restore it); nil means the move failed, with `message`
    /// explaining why.
    struct SingleOutcome: Sendable {
        let trashURL: URL?
        let message: String
        var succeeded: Bool { trashURL != nil }
    }

    /// Per-path outcome of a batch move. Batch results are per-path so one
    /// root-owned or locked entry never reports the others as failed.
    struct BatchOutcome: Sendable {
        let movedPaths: [String]
        let failedPaths: [String: String]

        var succeeded: Bool { failedPaths.isEmpty }
        var partiallySucceeded: Bool { !movedPaths.isEmpty && !failedPaths.isEmpty }
        var summaryLine: String {
            "Moved \(movedPaths.count) item\(movedPaths.count == 1 ? "" : "s") to Trash; "
                + "\(failedPaths.count) issue\(failedPaths.count == 1 ? "" : "s")."
        }
    }

    static func trash(_ path: String) -> SingleOutcome {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return SingleOutcome(trashURL: nil, message: "Empty path") }
        let url = URL(fileURLWithPath: trimmed)
        guard FileManager.default.fileExists(atPath: url.path) else {
            return SingleOutcome(trashURL: nil, message: "Path no longer exists")
        }
        do {
            var resultingURL: NSURL?
            try FileManager.default.trashItem(at: url, resultingItemURL: &resultingURL)
            return SingleOutcome(trashURL: resultingURL as URL?, message: "Moved to Trash")
        } catch {
            return SingleOutcome(trashURL: nil, message: error.localizedDescription)
        }
    }

    static func trash(paths: [String]) -> BatchOutcome {
        var movedPaths: [String] = []
        var failedPaths: [String: String] = [:]
        for path in paths {
            let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty else { continue }
            let outcome = trash(trimmed)
            if outcome.succeeded {
                movedPaths.append(trimmed)
            } else {
                failedPaths[trimmed] = outcome.message
            }
        }
        return BatchOutcome(movedPaths: movedPaths, failedPaths: failedPaths)
    }

    /// Restore a previously-trashed item to its original path.
    static func restore(from trashURL: URL, to originalPath: String) throws {
        try FileManager.default.moveItem(at: trashURL, to: URL(fileURLWithPath: originalPath))
    }

    /// Empty the home Trash by deleting its entries directly (no Finder Apple
    /// Events — avoids the Automation consent prompt and the all-or-nothing
    /// AppleScript failure mode). Per-entry, so one un-deletable item never
    /// blocks the rest.
    struct EmptyOutcome: Sendable {
        let removed: Int
        let failed: Int
        let firstError: String?
    }

    static func emptyHomeTrash() -> EmptyOutcome {
        let fm = FileManager.default
        let trash = URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".Trash")
        guard let entries = try? fm.contentsOfDirectory(
            at: trash,
            includingPropertiesForKeys: nil,
            options: []
        ) else {
            return EmptyOutcome(removed: 0, failed: 0, firstError: nil)
        }
        var removed = 0
        var failed = 0
        var firstError: String?
        for entry in entries {
            do {
                try fm.removeItem(at: entry)
                removed += 1
            } catch {
                failed += 1
                if firstError == nil { firstError = error.localizedDescription }
            }
        }
        return EmptyOutcome(removed: removed, failed: failed, firstError: firstError)
    }
}
