import XCTest
@testable import AetowerUI

final class StorageHygieneModelsTests: XCTestCase {
    func testRepositoryInventoryReportDecodesSnakeCaseJSON() throws {
        let json = """
        {
          "captured_at_millis": 1782860000000,
          "scan_duration_millis": 42,
          "roots": ["/Users/example/Repositories"],
          "repository_inventory": [
            {
              "id": "/Users/example/Repositories/Repo",
              "repo_root": "/Users/example/Repositories/Repo",
              "repo_name": "Repo",
              "discovered_root": "/Users/example/Repositories",
              "not_seen_in_latest_scan": true
            }
          ],
          "repository_inventory_complete": true,
          "repository_inventory_truncated": false,
          "repository_inventory_roots": ["/Users/example/Repositories"],
          "repository_inventory_partial_roots": [],
          "repository_inventory_coverage": [
            {
              "id": "/Users/example/Repositories",
              "label": "Repositories",
              "path": "/Users/example/Repositories",
              "status": "scanned",
              "permission_state": "readable",
              "detail": "Readable and scanned by the repository inventory pass.",
              "repository_count": 1,
              "scanned_directory_count": 12,
              "skipped_directory_count": 3,
              "truncated": false,
              "scanned": true
            }
          ],
          "truncated": false,
          "diagnostics": {
            "repository_walk_millis": 12,
            "git_millis": 30,
            "discovered_repository_count": 1,
            "scanned_directory_count": 12,
            "skipped_directory_count": 3
          }
        }
        """

        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase

        let report = try decoder.decode(RepositoryInventoryReportModel.self, from: Data(json.utf8))

        XCTAssertEqual(report.roots, ["/Users/example/Repositories"])
        XCTAssertEqual(report.repositoryInventory.first?.repoName, "Repo")
        XCTAssertEqual(report.repositoryInventory.first?.gitDirtyStatus, "unknown")
        XCTAssertEqual(report.repositoryInventory.first?.notSeenInLatestScan, true)
        XCTAssertTrue(report.repositoryInventoryComplete)
        XCTAssertFalse(report.repositoryInventoryTruncated)
        XCTAssertEqual(report.repositoryInventoryRoots, ["/Users/example/Repositories"])
        XCTAssertEqual(report.repositoryInventoryPartialRoots, [])
        XCTAssertEqual(report.repositoryInventoryCoverage.first?.repositoryCount, 1)
        XCTAssertEqual(report.diagnostics.discoveredRepositoryCount, 1)
        XCTAssertFalse(report.truncated)
    }

    func testRepositoryInventoryCoverageDecodesSnakeCaseJSON() throws {
        let json = """
        {
          "id": "/Users/example/Repositories",
          "label": "Repositories",
          "path": "/Users/example/Repositories",
          "status": "scanned",
          "permission_state": "readable",
          "detail": "Readable and scanned by the repository inventory pass.",
          "repository_count": 34,
          "scanned_directory_count": 128,
          "skipped_directory_count": 7,
          "truncated": false,
          "scanned": true
        }
        """

        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase

        let coverage = try decoder.decode(
            StorageRepositoryInventoryCoverageModel.self,
            from: Data(json.utf8)
        )

        XCTAssertEqual(coverage.id, "/Users/example/Repositories")
        XCTAssertEqual(coverage.label, "Repositories")
        XCTAssertEqual(coverage.status, "scanned")
        XCTAssertEqual(coverage.permissionState, "readable")
        XCTAssertEqual(coverage.repositoryCount, 34)
        XCTAssertEqual(coverage.scannedDirectoryCount, 128)
        XCTAssertEqual(coverage.skippedDirectoryCount, 7)
        XCTAssertFalse(coverage.truncated)
        XCTAssertTrue(coverage.scanned)
    }
}
