import XCTest
@testable import AetowerUI

@MainActor
final class NavigationModelTests: XCTestCase {
    /// A throwaway UserDefaults suite so tests never touch the real app domain.
    private func makeDefaults(_ function: String = #function) -> UserDefaults {
        let name = "nav.tests.\(function).\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        return defaults
    }

    // MARK: Launch policy — "fixed home + override"

    func testResolvesToMonitorWithoutOverride() {
        let nav = NavigationModel(defaults: makeDefaults())
        XCTAssertEqual(nav.workspace, .monitor)
        XCTAssertNil(nav.defaultTab)
    }

    func testHonorsValidOverride() {
        let defaults = makeDefaults()
        defaults.set("system", forKey: NavigationModel.defaultWorkspaceKey)
        let nav = NavigationModel(defaults: defaults)
        XCTAssertEqual(nav.workspace, .system)
        XCTAssertEqual(nav.defaultTab, .system)
    }

    func testIgnoresUnknownOverrideInsteadOfTrapping() {
        let defaults = makeDefaults()
        defaults.set("does-not-exist", forKey: NavigationModel.defaultWorkspaceKey)
        let nav = NavigationModel(defaults: defaults)
        XCTAssertEqual(nav.workspace, .monitor, "A stale slug must fall back to home, not wedge the app.")
        XCTAssertNil(nav.defaultTab)
    }

    func testSettingAndClearingDefault() {
        let nav = NavigationModel(defaults: makeDefaults())
        nav.defaultTab = .repos
        XCTAssertEqual(nav.defaultTab, .repos)
        nav.defaultTab = nil
        XCTAssertNil(nav.defaultTab)
    }

    // MARK: URL routing

    func testRoutesTopLevelWorkspace() {
        let nav = NavigationModel(defaults: makeDefaults())
        XCTAssertTrue(nav.handleURL(URL(string: "aetower://tab/storage")!))
        XCTAssertEqual(nav.workspace, .storage)
    }

    func testRoutesSubtab() {
        let nav = NavigationModel(defaults: makeDefaults())
        XCTAssertTrue(nav.handleURL(URL(string: "aetower://tab/activity/timeline")!))
        XCTAssertEqual(nav.workspace, .activity)
        XCTAssertEqual(nav.activity, .timeline)
    }

    func testRoutesCamelCaseSubtabSlug() {
        // aiAgents is the only sub-tab whose rawValue is not already
        // lowercase; handleURL lowercases segments, so this only routes if
        // applySubtab matches case-insensitively.
        let nav = NavigationModel(defaults: makeDefaults())
        XCTAssertTrue(nav.handleURL(URL(string: "aetower://tab/agents/aiAgents")!))
        XCTAssertEqual(nav.workspace, .agents)
        XCTAssertEqual(nav.agents, .aiAgents)
    }

    func testRoutesSystemSubtabCaseInsensitively() {
        let nav = NavigationModel(defaults: makeDefaults())
        XCTAssertTrue(nav.handleURL(URL(string: "aetower://tab/System/Diagnostics")!))
        XCTAssertEqual(nav.workspace, .system)
        XCTAssertEqual(nav.system, .diagnostics)
    }

    func testDefaultQueryPersistsOverride() {
        let nav = NavigationModel(defaults: makeDefaults())
        XCTAssertTrue(nav.handleURL(URL(string: "aetower://tab/agents?default=true")!))
        XCTAssertEqual(nav.workspace, .agents)
        XCTAssertEqual(nav.defaultTab, .agents)
    }

    func testUnknownSubtabLeavesSubSelectionUntouched() {
        let nav = NavigationModel(defaults: makeDefaults())
        nav.activity = .history
        XCTAssertTrue(nav.handleURL(URL(string: "aetower://tab/activity/bogus")!))
        XCTAssertEqual(nav.workspace, .activity)
        XCTAssertEqual(nav.activity, .history, "An unknown sub-tab is ignored, not reset.")
    }

    // MARK: Non-tab URLs are left alone (must not hijack the OAuth callback)

    func testIgnoresOAuthCallbackURL() {
        let nav = NavigationModel(defaults: makeDefaults())
        let start = nav.workspace
        XCTAssertFalse(nav.handleURL(URL(string: "aetower://oauth/cloudflare/callback?code=abc")!))
        XCTAssertEqual(nav.workspace, start)
    }

    func testIgnoresUnknownWorkspaceSlug() {
        let nav = NavigationModel(defaults: makeDefaults())
        XCTAssertFalse(nav.handleURL(URL(string: "aetower://tab/nope")!))
        XCTAssertEqual(nav.workspace, .monitor)
    }

    func testIgnoresForeignScheme() {
        let nav = NavigationModel(defaults: makeDefaults())
        XCTAssertFalse(nav.handleURL(URL(string: "https://tab/system")!))
        XCTAssertEqual(nav.workspace, .monitor)
    }

    // MARK: Slug stability (agents depend on these exact strings)

    func testWorkspaceSlugsAreStable() {
        XCTAssertEqual(
            WorkspaceTab.allCases.map(\.rawValue),
            ["monitor", "activity", "storage", "repos", "projects", "agents", "system", "settings"]
        )
    }

    func testShortcutIndexIsOneBasedInDeclarationOrder() {
        XCTAssertEqual(WorkspaceTab.monitor.shortcutIndex, 1)
        XCTAssertEqual(WorkspaceTab.settings.shortcutIndex, 8)
    }

    func testAccessibilityIdentifiersAreNamespaced() {
        XCTAssertEqual(WorkspaceTab.system.accessibilityIdentifier, "tab.system")
        XCTAssertEqual(ActivityWorkspaceTab.timeline.accessibilityIdentifier, "rail.activity.timeline")
        XCTAssertEqual(SystemWorkspaceTab.diagnostics.accessibilityIdentifier, "rail.system.diagnostics")
    }
}
