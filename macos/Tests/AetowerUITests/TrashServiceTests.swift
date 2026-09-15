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
        let identity = try XCTUnwrap(TrashService.identity(for: url.path))
        let outcome = TrashService.trash(url.path, expectedIdentity: identity) { _ in
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
        let identity = try XCTUnwrap(TrashService.identity(for: url.path))
        let outcome = TrashService.trash(url.path, expectedIdentity: identity) { _ in
            .unavailable("lsof timed out")
        }

        XCTAssertFalse(outcome.succeeded)
        XCTAssertTrue(outcome.message.contains("could not verify"))
        XCTAssertTrue(FileManager.default.fileExists(atPath: url.path))
        try? FileManager.default.removeItem(at: url)
    }

    func testTrashAllowsPathWithoutActiveWriters() throws {
        let url = try makeTemporaryTrashFixture(name: "no-holder")
        let identity = try XCTUnwrap(TrashService.identity(for: url.path))
        let outcome = TrashService.trash(url.path, expectedIdentity: identity) { _ in
            .checked([])
        }

        XCTAssertTrue(outcome.succeeded, outcome.message)
        XCTAssertFalse(FileManager.default.fileExists(atPath: url.path))
        if let trashURL = outcome.trashURL {
            try? FileManager.default.removeItem(at: trashURL)
        }
    }

    func testPermanentDeleteReclaimsOnlyRequestedVerifiedPath() throws {
        let requested = try makeTemporaryTrashFixture(name: "permanent-requested")
        let unrelated = try makeTemporaryTrashFixture(name: "permanent-unrelated")
        defer { try? FileManager.default.removeItem(at: unrelated) }
        let identity = try XCTUnwrap(TrashService.identity(for: requested.path))

        let outcome = TrashService.permanentlyDelete(
            paths: [requested.path],
            expectedIdentities: [requested.path: identity]
        ) { _ in
            .checked([])
        }

        XCTAssertEqual(outcome.reclaimedItems.map(\.originalPath), [requested.path])
        XCTAssertTrue(outcome.pendingTrashItems.isEmpty)
        XCTAssertTrue(outcome.alreadyMissingPaths.isEmpty)
        XCTAssertTrue(outcome.failedPaths.isEmpty)
        XCTAssertFalse(FileManager.default.fileExists(atPath: requested.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: unrelated.path))
        if let trashURL = outcome.reclaimedItems.first?.trashURL {
            XCTAssertFalse(FileManager.default.fileExists(atPath: trashURL.path))
        }
    }

    func testPermanentDeleteReportsMissingPathWithoutClaimingReclamation() {
        let missing = NSTemporaryDirectory() + "/aetower-missing-\(UUID().uuidString)"

        let outcome = TrashService.permanentlyDelete(paths: [missing]) { _ in
            .checked([])
        }

        XCTAssertTrue(outcome.reclaimedItems.isEmpty)
        XCTAssertTrue(outcome.pendingTrashItems.isEmpty)
        XCTAssertEqual(outcome.alreadyMissingPaths, [missing])
        XCTAssertTrue(outcome.failedPaths.isEmpty)
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
