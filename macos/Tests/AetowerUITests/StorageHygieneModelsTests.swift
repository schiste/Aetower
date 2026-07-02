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

    func testGrowthInsightsDecodeSnakeCaseJSON() throws {
        let json = """
        {
          "window_days": 30,
          "per_repo_rates": [
            {
              "scope": "/Users/example/Repositories/Aetower",
              "scope_kind": "repo",
              "repo_name": "Aetower",
              "window_days": 30,
              "total_delta_bytes": 83886080,
              "daily_rate_bytes": 20971520,
              "trend": "accelerating",
              "day_bucket_count": 4
            }
          ],
          "per_root_rates": [
            {
              "scope": "/Users/example/Repositories",
              "scope_kind": "source_root",
              "total_delta_bytes": -1024,
              "daily_rate_bytes": -256,
              "trend": "shrinking",
              "day_bucket_count": 4
            }
          ],
          "volume_forecasts": [
            {
              "volume_path": "/",
              "free_now_bytes": 367001600,
              "daily_rate_bytes": 36700160,
              "days_to_full": 10.0,
              "confidence": "low"
            }
          ],
          "since_last_scan": {
            "latest_scan_millis": 1782860000000,
            "appeared_count": 1,
            "appeared_total_bytes": 3145728,
            "appeared": [
              {
                "path": "/Users/example/Repositories/Aetower/target/f3.bin",
                "display_name": "f3.bin",
                "source_root": "/Users/example/Repositories",
                "kind": "rust-build",
                "cleanup_tier": "safe",
                "previous_cleanup_tier": "",
                "physical_bytes": 3145728,
                "delta_bytes": 3145728,
                "scan_millis": 1782860000000
              }
            ],
            "tier_changed_count": 1,
            "tier_changed": [
              {
                "path": "/Users/example/Repositories/Aetower/target/f1.bin",
                "display_name": "f1.bin",
                "source_root": "/Users/example/Repositories",
                "kind": "rust-build",
                "cleanup_tier": "review",
                "previous_cleanup_tier": "rebuildable",
                "physical_bytes": 4194304,
                "delta_bytes": 0,
                "scan_millis": 1782860000000
              }
            ],
            "disappeared": [],
            "disappeared_note": "Disappeared items are not cleanly derivable."
          }
        }
        """

        let insights = try AetowerJSON.snakeCaseDecoder().decode(
            StorageGrowthInsightsModel.self,
            from: Data(json.utf8)
        )

        XCTAssertEqual(insights.windowDays, 30)
        XCTAssertEqual(insights.perRepoRates.count, 1)
        XCTAssertEqual(insights.perRepoRates.first?.repoName, "Aetower")
        XCTAssertEqual(insights.perRepoRates.first?.dailyRateBytes, 20_971_520)
        XCTAssertEqual(insights.perRepoRates.first?.trend, "accelerating")
        XCTAssertEqual(insights.perRootRates.first?.trend, "shrinking")
        XCTAssertEqual(insights.perRootRates.first?.totalDeltaBytes, -1_024)
        XCTAssertEqual(insights.volumeForecasts.first?.volumePath, "/")
        XCTAssertEqual(insights.volumeForecasts.first?.daysToFull ?? 0, 10.0, accuracy: 0.001)
        let diff = try XCTUnwrap(insights.sinceLastScan)
        XCTAssertEqual(diff.appearedCount, 1)
        XCTAssertEqual(diff.appeared.first?.displayName, "f3.bin")
        XCTAssertEqual(diff.tierChanged.first?.previousCleanupTier, "rebuildable")
        XCTAssertEqual(diff.tierChanged.first?.cleanupTier, "review")
        XCTAssertTrue(diff.disappeared.isEmpty)
        XCTAssertFalse(diff.disappearedNote.isEmpty)
    }

    func testGrowthInsightsDecodeMinimalEnvelopeWithDefaults() throws {
        let insights = try AetowerJSON.snakeCaseDecoder().decode(
            StorageGrowthInsightsModel.self,
            from: Data("{}".utf8)
        )

        XCTAssertEqual(insights.windowDays, 30)
        XCTAssertTrue(insights.perRepoRates.isEmpty)
        XCTAssertTrue(insights.perRootRates.isEmpty)
        XCTAssertTrue(insights.volumeForecasts.isEmpty)
        XCTAssertNil(insights.sinceLastScan)
    }

    func testColdDataDecodesSnakeCaseJSON() throws {
        let json = """
        {
          "bands": [
            {
              "id": "cold-1y",
              "label": "Untouched 365+ days",
              "min_age_days": 365,
              "item_count": 3,
              "total_bytes": 5242880,
              "top_items": [
                {
                  "id": "/Users/example/old.bin",
                  "path": "/Users/example/old.bin",
                  "display_name": "old.bin",
                  "kind": "large-file",
                  "safety": "safe",
                  "cleanup_tier": "safe",
                  "size_bytes": 5242880,
                  "size_truncated": false,
                  "stale": true,
                  "reason": "Cold file",
                  "recommendation": "Review before reclaim",
                  "command_hint": "du -sh '/Users/example/old.bin'",
                  "recommendation_score": 4.5,
                  "attribution": { "confidence": "low", "notes": [] }
                }
              ]
            },
            {
              "id": "cold-90d",
              "label": "Untouched 90-365 days",
              "min_age_days": 90,
              "max_age_days": 365,
              "item_count": 0,
              "total_bytes": 0,
              "top_items": []
            }
          ],
          "caveat": "Age uses max(accessed, modified)."
        }
        """

        let coldData = try AetowerJSON.snakeCaseDecoder().decode(
            StorageColdDataModel.self,
            from: Data(json.utf8)
        )

        XCTAssertEqual(coldData.bands.count, 2)
        XCTAssertEqual(coldData.bands.first?.minAgeDays, 365)
        XCTAssertNil(coldData.bands.first?.maxAgeDays)
        XCTAssertEqual(coldData.bands.first?.itemCount, 3)
        XCTAssertEqual(coldData.bands.first?.totalBytes, 5_242_880)
        XCTAssertEqual(coldData.bands.first?.topItems.first?.displayName, "old.bin")
        XCTAssertEqual(coldData.bands.first?.topItems.first?.recommendationScore ?? 0, 4.5, accuracy: 0.001)
        XCTAssertEqual(coldData.bands.last?.maxAgeDays, 365)
        XCTAssertFalse(coldData.caveat.isEmpty)
    }

    func testGrowthDeltaDecodesWriterIdentityFields() throws {
        let json = """
        {
          "bucket_millis": 1782860000000,
          "scan_millis": 1782860001000,
          "path": "/Users/example/Repositories/Aetower/target",
          "source_root": "/Users/example/Repositories",
          "repo_root": "/Users/example/Repositories/Aetower",
          "kind": "rust-build",
          "cleanup_tier": "rebuildable",
          "previous_physical_bytes": 0,
          "current_physical_bytes": 33554432,
          "delta_bytes": 33554432,
          "provider": "claude",
          "session_id": "chau7-tab-a",
          "tab_name": "aetower-fix",
          "chau7_session_id": "chau7-session-9",
          "writer_display": "Claude Code session chau7-tab-a in tab 'aetower-fix'",
          "attribution_confidence": "high",
          "attribution_confidence_score": 92,
          "attribution_ambiguous": false,
          "attribution_summary": "Single writer ledger record matched this growth window.",
          "attribution_evidence": []
        }
        """

        let delta = try AetowerJSON.snakeCaseDecoder().decode(
            StorageGrowthDeltaModel.self,
            from: Data(json.utf8)
        )

        XCTAssertEqual(delta.provider, "claude")
        XCTAssertEqual(delta.sessionId, "chau7-tab-a")
        XCTAssertEqual(delta.tabName, "aetower-fix")
        XCTAssertEqual(delta.chau7SessionId, "chau7-session-9")
        XCTAssertEqual(delta.writerDisplay, "Claude Code session chau7-tab-a in tab 'aetower-fix'")
        XCTAssertEqual(delta.deltaBytes, 33_554_432)
    }

    func testItemDecodesRecommendationScoreAndDefaultsToZero() throws {
        let scoredJSON = """
        {
          "id": "/Users/example/target",
          "path": "/Users/example/target",
          "display_name": "target",
          "kind": "rust-build",
          "safety": "safe",
          "cleanup_tier": "safe",
          "size_bytes": 123,
          "size_truncated": false,
          "stale": false,
          "reason": "r",
          "recommendation": "r",
          "command_hint": "du",
          "recommendation_score": 6.25,
          "attribution": {
            "confidence": "high",
            "notes": [],
            "writer_display": "Claude Code session 3f2a in tab 'aetower-fix'"
          }
        }
        """
        let scored = try AetowerJSON.snakeCaseDecoder().decode(
            StorageHygieneItemModel.self,
            from: Data(scoredJSON.utf8)
        )
        XCTAssertEqual(scored.recommendationScore, 6.25, accuracy: 0.001)
        XCTAssertEqual(scored.attribution.writerDisplay, "Claude Code session 3f2a in tab 'aetower-fix'")

        let legacyJSON = """
        {
          "id": "/Users/example/target",
          "path": "/Users/example/target",
          "display_name": "target",
          "kind": "rust-build",
          "safety": "safe",
          "cleanup_tier": "safe",
          "size_bytes": 123,
          "size_truncated": false,
          "stale": false,
          "reason": "r",
          "recommendation": "r",
          "command_hint": "du",
          "attribution": { "confidence": "high", "notes": [] }
        }
        """
        let legacy = try AetowerJSON.snakeCaseDecoder().decode(
            StorageHygieneItemModel.self,
            from: Data(legacyJSON.utf8)
        )
        XCTAssertEqual(legacy.recommendationScore, 0)
        XCTAssertNil(legacy.attribution.writerDisplay)
    }
}
