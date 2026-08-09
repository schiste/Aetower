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
    private static let maxBufferedEvents = 512
    private static let bufferedFlushDelayMillis = 5_000
    private static let journalQueueKey = DispatchSpecificKey<Bool>()
    private static let journalQueue: DispatchQueue = {
        let queue = DispatchQueue(label: "com.aetower.storage.fsevents.journal", qos: .utility)
        queue.setSpecific(key: journalQueueKey, value: true)
        return queue
    }()
    nonisolated(unsafe) private static var pendingEvents: [StorageRootChangeEventRecord] = []
    nonisolated(unsafe) private static var pendingDirtyPaths: Set<String> = []
    nonisolated(unsafe) private static var pendingLastChangeMillis: UInt64?
    nonisolated(unsafe) private static var pendingFlushScheduled = false

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
        guard !events.isEmpty else { return }
        journalQueue.async {
            let normalized = events.compactMap(sanitizedEvent)
            guard !normalized.isEmpty else { return }
            pendingLastChangeMillis = currentMillis()
            pendingEvents.append(contentsOf: normalized)
            for event in normalized {
                pendingDirtyPaths.insert(event.path)
            }
            if pendingEvents.count >= maxBufferedEvents {
                flushPendingEventsOnQueue()
            } else {
                scheduleBufferedFlushOnQueue()
            }
        }
    }

    private static func recordDirtyPaths(_ paths: [String]) {
        let normalized = paths.compactMap { coalescedDirtyPath(normalizedPath($0)) }
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

    private static func sanitizedEvent(_ event: StorageRootChangeEventRecord) -> StorageRootChangeEventRecord? {
        guard let path = coalescedDirtyPath(normalizedPath(event.path)) else { return nil }
        return StorageRootChangeEventRecord(
            timestampMillis: event.timestampMillis,
            path: path,
            eventId: event.eventId,
            flags: event.flags,
            source: event.source
        )
    }

    private static func coalescedDirtyPath(_ path: String) -> String? {
        guard !path.isEmpty, !shouldIgnoreStorageEventPath(path) else { return nil }
        let lowercase = path.lowercased()
        if let gitRange = path.range(of: "/.git/", options: [.caseInsensitive]) {
            return String(path[..<gitRange.lowerBound])
        }
        if let nodeModulesRange = path.range(of: "/node_modules/", options: [.caseInsensitive]) {
            let end = path.index(before: nodeModulesRange.upperBound)
            return String(path[..<end])
        }
        if let chromeSupportRoot = prefixThroughMarker(
            path,
            marker: "/Library/Application Support/Google/Chrome"
        ) {
            return chromeSupportRoot
        }
        if let chromeCacheRoot = prefixThroughMarker(
            path,
            marker: "/Library/Caches/Google/Chrome"
        ) {
            return chromeCacheRoot
        }
        if let chau7RestoreRoot = prefixThroughMarker(
            path,
            marker: "/Library/Application Support/Chau7/TabRestoreBundles"
        ) {
            return chau7RestoreRoot
        }
        if let chau7BackupRoot = prefixThroughMarker(
            path,
            marker: "/Library/Application Support/Chau7/TabStateBackups"
        ) {
            return chau7BackupRoot
        }
        if lowercase.contains(".dat.nosync") {
            return URL(fileURLWithPath: path).deletingLastPathComponent().path
        }
        if let claudeRoot = prefixThroughMarker(path, marker: "/.claude") {
            return claudeRoot
        }
        if let codexRoot = prefixThroughMarker(path, marker: "/.codex") {
            return codexRoot
        }
        return path
    }

    private static func shouldIgnoreStorageEventPath(_ path: String) -> Bool {
        let lowercase = path.lowercased()
        let lastPathComponent = URL(fileURLWithPath: path).lastPathComponent.lowercased()
        if lastPathComponent == ".ds_store" {
            return true
        }
        if lowercase.contains("/library/application support/aetower/")
            || lowercase.contains("/library/caches/aetower/")
        {
            return true
        }
        if lowercase.contains("/library/metadata/corespotlight/")
            || lowercase.contains("/library/biome/")
            || lowercase.contains("/library/duetexpertcenter/")
        {
            return true
        }
        if lowercase.contains("/library/preferences/") && lastPathComponent.hasSuffix(".plist") {
            return true
        }
        if lowercase.contains("/library/applemediaservices/")
            && (
                lastPathComponent.hasSuffix("-wal")
                    || lastPathComponent.hasSuffix("-shm")
                    || lastPathComponent == "cookies.sqlitedb"
                    || lastPathComponent.hasSuffix(".sqlitedb-wal")
                    || lastPathComponent.hasSuffix(".sqlitedb-shm")
            )
        {
            return true
        }
        return false
    }

    private static func prefixThroughMarker(
        _ path: String,
        marker: String
    ) -> String? {
        guard let range = path.range(of: marker, options: [.caseInsensitive]) else { return nil }
        return String(path[..<range.upperBound])
    }

    static func lastChangeMillis() -> UInt64? {
        flushPendingEventsForRead()
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
        flushPendingEventsForRead()
        return UserDefaults.standard.stringArray(forKey: dirtyPathsKey) ?? []
    }

    static func clearDirtyPaths() {
        runOnJournalQueueSynchronously {
            pendingDirtyPaths.removeAll()
            pendingEvents.removeAll()
            pendingLastChangeMillis = nil
            UserDefaults.standard.removeObject(forKey: dirtyPathsKey)
        }
    }

    static func clearChangeStateForTesting() {
        runOnJournalQueueSynchronously {
            pendingEvents.removeAll()
            pendingDirtyPaths.removeAll()
            pendingLastChangeMillis = nil
            pendingFlushScheduled = false
            UserDefaults.standard.removeObject(forKey: key)
            UserDefaults.standard.removeObject(forKey: dirtyPathsKey)
            if let path = eventLedgerPath() {
                try? FileManager.default.removeItem(at: path)
            }
        }
    }

    static func flushPendingEventsForTesting() {
        flushPendingEventsForRead()
    }

    private static func scheduleBufferedFlushOnQueue() {
        guard !pendingFlushScheduled else { return }
        pendingFlushScheduled = true
        journalQueue.asyncAfter(deadline: .now() + .milliseconds(bufferedFlushDelayMillis)) {
            pendingFlushScheduled = false
            flushPendingEventsOnQueue()
        }
    }

    private static func flushPendingEventsForRead() {
        runOnJournalQueueSynchronously {
            flushPendingEventsOnQueue()
        }
    }

    private static func flushPendingEventsOnQueue() {
        guard !pendingEvents.isEmpty || !pendingDirtyPaths.isEmpty else { return }
        let events = pendingEvents
        let dirtyPaths = Array(pendingDirtyPaths)
        let lastChangeMillis = pendingLastChangeMillis ?? currentMillis()
        pendingEvents.removeAll(keepingCapacity: true)
        pendingDirtyPaths.removeAll(keepingCapacity: true)
        pendingLastChangeMillis = nil
        UserDefaults.standard.set(lastChangeMillis, forKey: key)
        recordDirtyPaths(dirtyPaths)
        appendEventLedger(events)
    }

    private static func runOnJournalQueueSynchronously(_ work: @escaping () -> Void) {
        if DispatchQueue.getSpecific(key: journalQueueKey) == true {
            work()
        } else {
            journalQueue.sync(execute: work)
        }
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
        return ((trimmed as NSString).expandingTildeInPath as NSString).standardizingPath
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
