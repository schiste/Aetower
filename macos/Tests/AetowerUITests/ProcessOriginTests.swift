import XCTest
import AetowerBridge
@testable import AetowerUI

final class ProcessOriginTests: XCTestCase {
    func testShellParentClassifiesClaudeCodeAsCli() {
        let origin = processOrigin(from: ProcessOriginSignals(
            entityKind: .aiAgent,
            displayName: "Claude Code",
            bundleId: nil,
            executablePath: "/opt/homebrew/bin/claude",
            componentExecutablePaths: ["/opt/homebrew/bin/claude"],
            commandLines: ["claude --resume abc"],
            parentSummaries: ["zsh (pid 30341)"],
            launchedByValues: [],
            provenanceLabels: ["Shell session"],
            provenanceKindNames: ["shellSession"],
            adapterKindNames: ["chau7Session"],
            users: ["christophehenner"]
        ))

        XCTAssertEqual(origin.kind, .cli)
        XCTAssertTrue(origin.subtitle.contains("CLI"))
        XCTAssertTrue(origin.subtitle.contains("Chau7"))
        XCTAssertTrue(origin.searchTokens.contains("origin:cli"))
        XCTAssertTrue(origin.searchTokens.contains("host:chau7-session"))
    }

    func testApplicationBundleClassifiesClaudeDesktopAsApp() {
        let origin = processOrigin(from: ProcessOriginSignals(
            entityKind: .app,
            displayName: "Claude",
            bundleId: "com.anthropic.claudefordesktop",
            executablePath: "/Applications/Claude.app/Contents/MacOS/Claude",
            componentExecutablePaths: ["/Applications/Claude.app/Contents/MacOS/Claude"],
            commandLines: [],
            parentSummaries: ["launchd (pid 1)"],
            launchedByValues: [],
            provenanceLabels: ["Application bundle"],
            provenanceKindNames: ["appBundle"],
            adapterKindNames: [],
            users: ["christophehenner"]
        ))

        XCTAssertEqual(origin.kind, .app)
        XCTAssertTrue(origin.subtitle.contains("com.anthropic.claudefordesktop"))
        XCTAssertTrue(origin.searchTokens.contains("origin:app"))
    }

    func testBundledHelperClassifiesAsHelperBeforeApp() {
        let origin = processOrigin(from: ProcessOriginSignals(
            entityKind: .unknown,
            displayName: "Chrome Helper",
            bundleId: nil,
            executablePath: "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper",
            componentExecutablePaths: [
                "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper"
            ],
            commandLines: [],
            parentSummaries: ["Google Chrome (pid 700)"],
            launchedByValues: [],
            provenanceLabels: ["Helper tree"],
            provenanceKindNames: ["helperTree"],
            adapterKindNames: [],
            users: ["christophehenner"]
        ))

        XCTAssertEqual(origin.kind, .helper)
        XCTAssertTrue(origin.subtitle.contains("Helper"))
        XCTAssertTrue(origin.searchTokens.contains("origin:helper"))
    }
}
