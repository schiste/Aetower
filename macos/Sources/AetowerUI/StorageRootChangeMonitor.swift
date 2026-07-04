import CoreServices
import Foundation

struct StorageRootChangeEventRecord: Codable {
    let timestampMillis: UInt64
    let path: String
    let eventId: UInt64?
    let flags: UInt64?
    let source: String
}

struct StorageDirtyPathSummary: Sendable {
    let lastChangeMillis: UInt64?
    let dirtyPathCount: Int
    let samplePaths: [String]

    var hasChanges: Bool {
        lastChangeMillis != nil && dirtyPathCount > 0
    }
}

enum StorageRootChangeJournal {
    private static let key = "aetower.storageHygiene.lastRootChangeMillis.v1"
    private static let dirtyPathsKey = "aetower.storageHygiene.dirtyPaths.v1"
    private static let maxDirtyPaths = 256
    private static let maxEventLedgerBytes: UInt64 = 2 * 1_024 * 1_024
    private static let maxEventLedgerLines = 2_048

    static func recordChange(paths: [String] = []) {
        let timestampMillis = currentMillis()
        let events = paths.map {
            StorageRootChangeEventRecord(
                timestampMillis: timestampMillis,
                path: normalizedPath($0),
                eventId: nil,
                flags: nil,
                source: "aetower-fsevents"
            )
        }
        recordEvents(events)
    }

    static func recordEvents(_ events: [StorageRootChangeEventRecord]) {
        let normalized = events.filter { !$0.path.isEmpty }
        guard !normalized.isEmpty else { return }
        UserDefaults.standard.set(currentMillis(), forKey: key)
        recordDirtyPaths(normalized.map(\.path))
        appendEventLedger(normalized)
    }

    private static func recordDirtyPaths(_ paths: [String]) {
        let normalized = paths.map(normalizedPath).filter { !$0.isEmpty }
        guard !normalized.isEmpty else { return }
        var existing = Set(UserDefaults.standard.stringArray(forKey: dirtyPathsKey) ?? [])
        for path in normalized {
            existing.insert(path)
        }
        let retained = Array(existing)
            .sorted()
            .suffix(maxDirtyPaths)
        UserDefaults.standard.set(Array(retained), forKey: dirtyPathsKey)
    }

    static func lastChangeMillis() -> UInt64? {
        let value = UserDefaults.standard.object(forKey: key)
        if let number = value as? NSNumber {
            return number.uint64Value
        }
        return nil
    }

    static func summary(sampleLimit: Int = 3) -> StorageDirtyPathSummary {
        let paths = dirtyPaths()
        return StorageDirtyPathSummary(
            lastChangeMillis: lastChangeMillis(),
            dirtyPathCount: paths.count,
            samplePaths: Array(paths.prefix(max(0, sampleLimit)))
        )
    }

    static func dirtyPaths() -> [String] {
        UserDefaults.standard.stringArray(forKey: dirtyPathsKey) ?? []
    }

    static func clearDirtyPaths() {
        UserDefaults.standard.removeObject(forKey: dirtyPathsKey)
    }

    private static func appendEventLedger(_ events: [StorageRootChangeEventRecord]) {
        guard let path = eventLedgerPath() else { return }
        do {
            try FileManager.default.createDirectory(
                at: path.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            let encoder = JSONEncoder()
            encoder.keyEncodingStrategy = .convertToSnakeCase
            let payload = try events
                .compactMap { event -> Data? in
                    var data = try encoder.encode(event)
                    data.append(0x0A)
                    return data
                }
                .reduce(into: Data()) { partial, data in
                    partial.append(data)
                }
            if FileManager.default.fileExists(atPath: path.path) {
                let handle = try FileHandle(forWritingTo: path)
                try handle.seekToEnd()
                try handle.write(contentsOf: payload)
                try handle.close()
            } else {
                try payload.write(to: path, options: .atomic)
            }
            trimEventLedgerIfNeeded(path)
        } catch {
            // Best effort only: FSEvents should never make Storage unusable.
        }
    }

    private static func trimEventLedgerIfNeeded(_ path: URL) {
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: path.path),
              let size = attributes[.size] as? NSNumber,
              size.uint64Value > maxEventLedgerBytes,
              let data = try? Data(contentsOf: path),
              let content = String(data: data, encoding: .utf8)
        else {
            return
        }
        let retained = content
            .split(separator: "\n", omittingEmptySubsequences: true)
            .suffix(maxEventLedgerLines)
            .joined(separator: "\n")
        let output = retained.isEmpty ? "" : retained + "\n"
        try? output.data(using: .utf8)?.write(to: path, options: .atomic)
    }

    private static func eventLedgerPath() -> URL? {
        guard let base = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first else {
            return nil
        }
        return base.appendingPathComponent("Aetower", isDirectory: true)
            .appendingPathComponent("storage-fsevents.ndjson")
    }

    private static func currentMillis() -> UInt64 {
        UInt64(Date().timeIntervalSince1970 * 1000)
    }

    private static func normalizedPath(_ path: String) -> String {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return "" }
        if trimmed == "~" {
            return FileManager.default.homeDirectoryForCurrentUser.standardizedFileURL.path
        }
        if trimmed.hasPrefix("~/") {
            return FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent(String(trimmed.dropFirst(2)))
                .standardizedFileURL
                .path
        }
        return URL(fileURLWithPath: trimmed, isDirectory: false).standardizedFileURL.path
    }
}

final class StorageRootChangeMonitor {
    private var stream: FSEventStreamRef?
    private var watchedRoots: [String] = []
    private let eventQueue = DispatchQueue(label: "com.aetower.storage.fsevents", qos: .utility)

    func startWatching(roots: [String]) {
        let normalized = Array(Set(roots.map(normalizedPath).filter { !$0.isEmpty })).sorted()
        guard normalized != watchedRoots else { return }
        stop()
        watchedRoots = normalized
        guard !normalized.isEmpty else { return }

        var context = FSEventStreamContext(
            version: 0,
            info: Unmanaged.passUnretained(self).toOpaque(),
            retain: nil,
            release: nil,
            copyDescription: nil
        )
        let callback: FSEventStreamCallback = { _, info, eventCount, eventPaths, eventFlags, eventIds in
            guard let info else { return }
            let monitor = Unmanaged<StorageRootChangeMonitor>.fromOpaque(info).takeUnretainedValue()
            monitor.recordRootChange(
                events: monitor.events(
                    from: eventPaths,
                    flags: eventFlags,
                    ids: eventIds,
                    count: eventCount
                )
            )
        }
        guard let stream = FSEventStreamCreate(
            kCFAllocatorDefault,
            callback,
            &context,
            normalized as CFArray,
            FSEventStreamEventId(kFSEventStreamEventIdSinceNow),
            2.0,
            UInt32(kFSEventStreamCreateFlagUseCFTypes | kFSEventStreamCreateFlagFileEvents)
        ) else {
            watchedRoots = []
            return
        }
        self.stream = stream
        FSEventStreamSetDispatchQueue(stream, eventQueue)
        FSEventStreamStart(stream)
    }

    func stop() {
        guard let stream else { return }
        FSEventStreamStop(stream)
        FSEventStreamInvalidate(stream)
        FSEventStreamRelease(stream)
        self.stream = nil
        watchedRoots = []
    }

    private func recordRootChange(events: [StorageRootChangeEventRecord]) {
        StorageRootChangeJournal.recordEvents(events)
    }

    private func events(
        from eventPaths: UnsafeMutableRawPointer,
        flags: UnsafePointer<FSEventStreamEventFlags>,
        ids: UnsafePointer<FSEventStreamEventId>,
        count: Int
    ) -> [StorageRootChangeEventRecord] {
        let array = unsafeBitCast(eventPaths, to: CFArray.self) as NSArray
        let timestampMillis = UInt64(Date().timeIntervalSince1970 * 1000)
        return (0..<min(count, array.count)).compactMap { index in
            guard let path = array[index] as? String else { return nil }
            return StorageRootChangeEventRecord(
                timestampMillis: timestampMillis,
                path: normalizedPath(path),
                eventId: UInt64(ids[index]),
                flags: UInt64(flags[index]),
                source: "aetower-fsevents"
            )
        }
    }

    private func normalizedPath(_ path: String) -> String {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return "" }
        if trimmed == "~" {
            return FileManager.default.homeDirectoryForCurrentUser.standardizedFileURL.path
        }
        if trimmed.hasPrefix("~/") {
            return FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent(String(trimmed.dropFirst(2)))
                .standardizedFileURL
                .path
        }
        return URL(fileURLWithPath: trimmed, isDirectory: true).standardizedFileURL.path
    }
}
