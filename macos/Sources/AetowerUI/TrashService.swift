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
    struct ActiveWriterHolder: Sendable {
        let pid: UInt32
        let command: String
        let fd: String
        let name: String
    }

    enum ActiveWriterProbeResult: Sendable {
        case checked([ActiveWriterHolder])
        case unavailable(String)
    }

    typealias ActiveWriterProbe = @Sendable (String) -> ActiveWriterProbeResult

    /// Outcome of trashing a single path. `trashURL` is where Finder placed the
    /// item (used to restore it); nil means the move failed, with `message`
    /// explaining why.
    struct SingleOutcome: Sendable {
        let trashURL: URL?
        let message: String
        var succeeded: Bool { trashURL != nil }
    }

    struct MovedItem: Sendable {
        let originalPath: String
        let trashURL: URL
    }

    /// Per-path outcome of a batch move. Batch results are per-path so one
    /// root-owned or locked entry never reports the others as failed.
    struct BatchOutcome: Sendable {
        let movedItems: [MovedItem]
        let failedPaths: [String: String]

        var movedPaths: [String] { movedItems.map(\.originalPath) }
        var succeeded: Bool { failedPaths.isEmpty }
        var partiallySucceeded: Bool { !movedItems.isEmpty && !failedPaths.isEmpty }
        var summaryLine: String {
            "Moved \(movedItems.count) item\(movedItems.count == 1 ? "" : "s") to Trash; "
                + "\(failedPaths.count) issue\(failedPaths.count == 1 ? "" : "s")."
        }
    }

    struct PermanentBatchOutcome: Sendable {
        let reclaimedItems: [MovedItem]
        let pendingTrashItems: [MovedItem]
        let alreadyMissingPaths: [String]
        let failedPaths: [String: String]
    }

    static func trash(_ path: String, activeWriterProbe: ActiveWriterProbe? = nil) -> SingleOutcome {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return SingleOutcome(trashURL: nil, message: "Empty path") }
        let url = URL(fileURLWithPath: trimmed)
        if let blocker = privilegedCleanupBlocker(for: url.path) {
            return SingleOutcome(trashURL: nil, message: blocker)
        }
        guard FileManager.default.fileExists(atPath: url.path) else {
            return SingleOutcome(trashURL: nil, message: "Path no longer exists")
        }
        if let blocker = activeWriterBlocker(for: trimmed, activeWriterProbe: activeWriterProbe) {
            return SingleOutcome(trashURL: nil, message: blocker)
        }
        do {
            var resultingURL: NSURL?
            try FileManager.default.trashItem(at: url, resultingItemURL: &resultingURL)
            return SingleOutcome(trashURL: resultingURL as URL?, message: "Moved to Trash")
        } catch {
            return SingleOutcome(trashURL: nil, message: error.localizedDescription)
        }
    }

    static func trash(paths: [String], activeWriterProbe: ActiveWriterProbe? = nil) -> BatchOutcome {
        var movedItems: [MovedItem] = []
        var failedPaths: [String: String] = [:]
        for path in paths {
            let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty else { continue }
            let outcome = trash(trimmed, activeWriterProbe: activeWriterProbe)
            if let trashURL = outcome.trashURL {
                movedItems.append(MovedItem(originalPath: trimmed, trashURL: trashURL))
            } else {
                failedPaths[trimmed] = outcome.message
            }
        }
        return BatchOutcome(movedItems: movedItems, failedPaths: failedPaths)
    }

    /// Permanently reclaim verified paths without touching unrelated Trash
    /// contents. Each path first goes through the normal Trash and active-writer
    /// protections. Only the resulting URL is then deleted; failures remain in
    /// Trash and are returned to the caller for tracking and recovery.
    static func permanentlyDelete(
        paths: [String],
        activeWriterProbe: ActiveWriterProbe? = nil
    ) -> PermanentBatchOutcome {
        let trashed = trash(paths: paths, activeWriterProbe: activeWriterProbe)
        var reclaimedItems: [MovedItem] = []
        var pendingTrashItems: [MovedItem] = []
        let alreadyMissingPaths = trashed.failedPaths.compactMap { path, reason in
            reason == "Path no longer exists" ? path : nil
        }
        var failedPaths = trashed.failedPaths.filter { _, reason in
            reason != "Path no longer exists"
        }

        for item in trashed.movedItems {
            do {
                try FileManager.default.removeItem(at: item.trashURL)
                reclaimedItems.append(item)
            } catch {
                pendingTrashItems.append(item)
                failedPaths[item.originalPath] =
                    "Moved to Trash, but permanent deletion failed: \(error.localizedDescription)"
            }
        }

        return PermanentBatchOutcome(
            reclaimedItems: reclaimedItems,
            pendingTrashItems: pendingTrashItems,
            alreadyMissingPaths: alreadyMissingPaths.sorted(),
            failedPaths: failedPaths
        )
    }

    static func privilegedCleanupBlocker(for path: String) -> String? {
        let normalized = URL(fileURLWithPath: path).standardizedFileURL.path.lowercased()
        guard normalized == "/library/developer"
            || normalized.hasPrefix("/library/developer/")
        else { return nil }
        return "System-level Developer cache requires administrator permission; Aetower direct Trash cleanup is disabled."
    }

    private static func activeWriterBlocker(
        for path: String,
        activeWriterProbe: ActiveWriterProbe?
    ) -> String? {
        guard let activeWriterProbe else { return nil }
        switch activeWriterProbe(path) {
        case let .checked(holders):
            guard !holders.isEmpty else { return nil }
            return "Active writer protection blocked cleanup: \(activeWriterSummary(holders)) currently holds this path."
        case let .unavailable(message):
            return "Active writer protection could not verify this path: \(message)"
        }
    }

    private static func activeWriterSummary(_ holders: [ActiveWriterHolder]) -> String {
        var parts = holders.prefix(3).map { holder in
            "\(holder.command) pid \(holder.pid) fd \(holder.fd)"
        }
        if holders.count > parts.count {
            parts.append("+\(holders.count - parts.count) more")
        }
        return parts.joined(separator: ", ")
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
        let missing: Int
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
            return EmptyOutcome(
                removed: 0,
                missing: 0,
                failed: 1,
                firstError: "Home Trash is not readable."
            )
        }
        return emptyTrashItems(entries)
    }

    static func emptyTrashItems(_ urls: [URL]) -> EmptyOutcome {
        let fm = FileManager.default
        var removed = 0
        var missing = 0
        var failed = 0
        var firstError: String?
        for entry in urls {
            guard fm.fileExists(atPath: entry.path) else {
                missing += 1
                continue
            }
            do {
                try fm.removeItem(at: entry)
                removed += 1
            } catch {
                failed += 1
                if firstError == nil { firstError = error.localizedDescription }
            }
        }
        return EmptyOutcome(removed: removed, missing: missing, failed: failed, firstError: firstError)
    }
}
