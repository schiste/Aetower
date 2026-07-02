import XCTest
@testable import AetowerUI

final class StorageItemsPageModelsTests: XCTestCase {
    func testItemsPageDecodesSnakeCaseJSON() throws {
        let json = """
        {
          "captured_at_millis": 1782860000000,
          "scan_mode": "instant_cached",
          "diagnostics": {
            "mode": "instant_cached",
            "root_walk_millis": 1,
            "size_walk_millis": 2,
            "git_millis": 3,
            "serialize_millis": 4,
            "payload_bytes": 5,
            "decode_millis": 0,
            "scanned_directory_count": 6,
            "discovered_repository_count": 7,
            "sized_entry_count": 8,
            "candidate_seen_count": 9,
            "candidate_retained_count": 10,
            "storage_index_status": "warm",
            "storage_index_hits": 11,
            "storage_index_misses": 12,
            "storage_index_writes": 13,
            "native_metadata_strategy": "getattrlistbulk",
            "fsevents_status": "active",
            "lazy_git_status": true,
            "top_k_retained": false
          },
          "offset": 100,
          "limit": 50,
          "sort_key": "path",
          "sort_descending": false,
          "returned_count": 1,
          "total_available": 4321,
          "has_more": true,
          "items": [
            {
              "id": "/Users/example/Repositories/Repo/target",
              "path": "/Users/example/Repositories/Repo/target",
              "display_name": "target",
              "kind": "rust-build",
              "safety": "safe",
              "cleanup_tier": "safe",
              "size_bytes": 123456789,
              "size_truncated": false,
              "stale": true,
              "reason": "Rust build output",
              "recommendation": "cargo clean",
              "command_hint": "cargo clean",
              "attribution": {
                "confidence": "high",
                "notes": ["repo-linked"]
              }
            }
          ]
        }
        """

        let page = try AetowerJSON.snakeCaseDecoder().decode(
            StorageHygieneItemsPageModel.self,
            from: Data(json.utf8)
        )

        XCTAssertEqual(page.capturedAtMillis, 1_782_860_000_000)
        XCTAssertEqual(page.scanMode, "instant_cached")
        XCTAssertEqual(page.offset, 100)
        XCTAssertEqual(page.limit, 50)
        XCTAssertEqual(page.sortKey, "path")
        XCTAssertFalse(page.sortDescending)
        XCTAssertEqual(page.returnedCount, 1)
        XCTAssertEqual(page.totalAvailable, 4321)
        XCTAssertTrue(page.hasMore)
        XCTAssertEqual(page.items.count, 1)
        XCTAssertEqual(page.items.first?.displayName, "target")
        XCTAssertEqual(page.items.first?.sizeBytes, 123_456_789)
        XCTAssertEqual(page.diagnostics?.storageIndexStatus, "warm")
    }

    func testItemsPageDecodesMinimalEnvelopeWithDefaults() throws {
        let json = """
        {
          "items": []
        }
        """

        let page = try AetowerJSON.snakeCaseDecoder().decode(
            StorageHygieneItemsPageModel.self,
            from: Data(json.utf8)
        )

        XCTAssertEqual(page.capturedAtMillis, 0)
        XCTAssertEqual(page.scanMode, "instant_cached")
        XCTAssertEqual(page.offset, 0)
        XCTAssertEqual(page.sortKey, "size")
        XCTAssertTrue(page.sortDescending)
        XCTAssertEqual(page.returnedCount, 0)
        XCTAssertNil(page.totalAvailable)
        XCTAssertFalse(page.hasMore)
        XCTAssertTrue(page.items.isEmpty)
        XCTAssertNil(page.diagnostics)
    }

    func testItemsPageComputesHasMoreFromTotalWhenMissing() throws {
        let json = """
        {
          "offset": 10,
          "limit": 10,
          "total_available": 25,
          "items": []
        }
        """

        let page = try AetowerJSON.snakeCaseDecoder().decode(
            StorageHygieneItemsPageModel.self,
            from: Data(json.utf8)
        )

        // offset (10) + items (0) < total (25) even though has_more is absent.
        XCTAssertTrue(page.hasMore)
        XCTAssertEqual(page.totalAvailable, 25)
    }

    func testItemsPageToleratesMalformedDiagnostics() throws {
        let json = """
        {
          "offset": 0,
          "limit": 10,
          "total_available": 0,
          "has_more": false,
          "diagnostics": { "mode": 42 },
          "items": []
        }
        """

        let page = try AetowerJSON.snakeCaseDecoder().decode(
            StorageHygieneItemsPageModel.self,
            from: Data(json.utf8)
        )

        XCTAssertNil(page.diagnostics)
        XCTAssertFalse(page.hasMore)
    }
}
