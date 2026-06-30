import CoreServices
import Foundation

enum StorageRootChangeJournal {
    private static let key = "aetower.storageHygiene.lastRootChangeMillis.v1"
    private static let dirtyPathsKey = "aetower.storageHygiene.dirtyPaths.v1"
    private static let maxDirtyPaths = 256

    static func recordChange(paths: [String] = []) {
        UserDefaults.standard.set(currentMillis(), forKey: key)
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

    static func dirtyPaths() -> [String] {
        UserDefaults.standard.stringArray(forKey: dirtyPathsKey) ?? []
    }

    static func clearDirtyPaths() {
        UserDefaults.standard.removeObject(forKey: dirtyPathsKey)
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
        let callback: FSEventStreamCallback = { _, info, _, eventPaths, _, _ in
            guard let info else { return }
            let monitor = Unmanaged<StorageRootChangeMonitor>.fromOpaque(info).takeUnretainedValue()
            monitor.recordRootChange(paths: monitor.paths(from: eventPaths))
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

    private func recordRootChange(paths: [String]) {
        StorageRootChangeJournal.recordChange(paths: paths)
    }

    private func paths(from eventPaths: UnsafeMutableRawPointer) -> [String] {
        let array = unsafeBitCast(eventPaths, to: CFArray.self) as NSArray
        return array.compactMap { $0 as? String }
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
