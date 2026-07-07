import Foundation
import XCTest
@testable import AetowerUI

final class TrashServiceTests: XCTestCase {
    func testPrivilegedDeveloperCachePathIsBlockedBeforeTrash() {
        let blocker = TrashService.privilegedCleanupBlocker(
            for: "/Library/Developer/CoreSimulator/Caches"
        )

        XCTAssertNotNil(blocker)
        XCTAssertTrue(blocker?.contains("administrator permission") == true)
        XCTAssertNil(
            TrashService.privilegedCleanupBlocker(
                for: "/tmp/Library/Developer/CoreSimulator/Caches"
            )
        )
    }

    func testTrashBlocksPathWithActiveWriterHolder() throws {
        let url = try makeTemporaryTrashFixture(name: "active-holder")
        let outcome = TrashService.trash(url.path) { _ in
            .checked([
                TrashService.ActiveWriterHolder(
                    pid: 42,
                    command: "tail",
                    fd: "3r",
                    name: url.path
                )
            ])
        }

        XCTAssertFalse(outcome.succeeded)
        XCTAssertTrue(outcome.message.contains("Active writer protection blocked cleanup"))
        XCTAssertTrue(FileManager.default.fileExists(atPath: url.path))
        try? FileManager.default.removeItem(at: url)
    }

    func testTrashFailsClosedWhenActiveWriterProbeIsUnavailable() throws {
        let url = try makeTemporaryTrashFixture(name: "probe-unavailable")
        let outcome = TrashService.trash(url.path) { _ in
            .unavailable("lsof timed out")
        }

        XCTAssertFalse(outcome.succeeded)
        XCTAssertTrue(outcome.message.contains("could not verify"))
        XCTAssertTrue(FileManager.default.fileExists(atPath: url.path))
        try? FileManager.default.removeItem(at: url)
    }

    func testTrashAllowsPathWithoutActiveWriters() throws {
        let url = try makeTemporaryTrashFixture(name: "no-holder")
        let outcome = TrashService.trash(url.path) { _ in
            .checked([])
        }

        XCTAssertTrue(outcome.succeeded, outcome.message)
        XCTAssertFalse(FileManager.default.fileExists(atPath: url.path))
        if let trashURL = outcome.trashURL {
            try? FileManager.default.removeItem(at: trashURL)
        }
    }

    private func makeTemporaryTrashFixture(name: String) throws -> URL {
        let directory = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("aetower-trash-service-tests", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let url = directory.appendingPathComponent("\(name)-\(UUID().uuidString)")
        try Data("fixture".utf8).write(to: url)
        return url
    }
}
