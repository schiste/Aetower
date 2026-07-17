import Foundation
import XCTest
@testable import AetowerUI

final class BrowserAttributionSetupTests: XCTestCase {
    func testDedicatedChromeArgumentsUseSeparateProfileAndDebugPort() {
        let profile = URL(fileURLWithPath: "/tmp/aetower-browser-profile", isDirectory: true)

        let arguments = BrowserAttributionSetup.launchArguments(profileDirectory: profile)

        XCTAssertTrue(arguments.contains("--remote-debugging-port=9222"))
        XCTAssertTrue(arguments.contains("--user-data-dir=/tmp/aetower-browser-profile"))
        XCTAssertTrue(arguments.contains("--no-first-run"))
        XCTAssertTrue(arguments.contains("--new-window"))
    }

    func testParseTargetSummaryCountsOnlyPageTargetsAsTabs() throws {
        let json = """
        [
          {"id": "page-1", "type": "page", "title": "Docs"},
          {"id": "worker-1", "type": "service_worker", "title": "Worker"},
          {"id": "page-2", "type": "page", "title": "App"}
        ]
        """

        let summary = try BrowserAttributionSetup.parseTargetSummary(
            from: Data(json.utf8),
            endpoint: BrowserAttributionSetup.defaultEndpoint
        )

        XCTAssertEqual(summary.endpoint, "http://127.0.0.1:9222/json/list")
        XCTAssertEqual(summary.pageTargetCount, 2)
        XCTAssertEqual(summary.totalTargetCount, 3)
    }

    func testDedicatedProfileLivesUnderAetowerApplicationSupport() {
        let path = BrowserAttributionSetup.dedicatedProfileDirectory.path

        XCTAssertTrue(path.contains("/Application Support/Aetower/BrowserProfiles/ChromeDebug"))
    }
}
