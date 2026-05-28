import XCTest
@testable import AetowerUI

@MainActor
final class SettingsStoreTests: XCTestCase {
    func testBlankEndpointSettingsNormalizeToRuntimeDefaults() {
        XCTAssertEqual(
            SettingsStore.normalizedDockerSocketPath("  "),
            SettingsStore.defaultDockerSocketPath
        )
        XCTAssertEqual(
            SettingsStore.normalizedTelemetryEndpoint("\n\t"),
            SettingsStore.defaultTelemetryEndpoint
        )
    }

    func testRuntimeIntervalRelationshipsStayVisibleInSettings() {
        let store = makeStore()

        store.engineActiveIntervalSeconds = 5
        store.engineIdleIntervalSeconds = 2
        store.engineLowPowerIntervalSeconds = 3
        store.gpuSampleIntervalSeconds = 120
        store.gpuSampleLowPowerIntervalSeconds = 10

        XCTAssertEqual(store.engineIdleIntervalSeconds, 5)
        XCTAssertEqual(store.engineLowPowerIntervalSeconds, 5)
        XCTAssertEqual(store.gpuSampleLowPowerIntervalSeconds, 120)
    }

    func testResetRestoresSafeDefaultsAndDisablesAutomaticClientRegistration() {
        let store = makeStore()
        store.autoRegisterLocalMcpClientsEnabled = true
        store.telemetryEndpoint = "http://collector.example/v1/metrics"
        store.collectionProfile = .full

        store.resetToDefaults()

        XCTAssertFalse(store.autoRegisterLocalMcpClientsEnabled)
        XCTAssertEqual(store.telemetryEndpoint, SettingsStore.defaultTelemetryEndpoint)
        XCTAssertEqual(store.collectionProfile, .balanced)
        XCTAssertFalse(store.telemetryEnabled)
    }

    func testPublicPreviewDefaultsStayConservative() {
        let store = makeStore()

        XCTAssertFalse(store.privilegedHelperEnabled)
        XCTAssertFalse(store.telemetryEnabled)
        XCTAssertFalse(store.autoRegisterLocalMcpClientsEnabled)
        XCTAssertTrue(store.operatorSafeModeEnabled)
        XCTAssertEqual(store.exportPrivacyTier, .redacted)
        XCTAssertEqual(store.collectionProfile, .balanced)
    }

    func testAutomaticClientRegistrationPreferencePersists() {
        let suiteName = "AetowerSettingsTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = SettingsStore(defaults: defaults)
        store.autoRegisterLocalMcpClientsEnabled = true

        let reloaded = SettingsStore(defaults: defaults)
        XCTAssertTrue(reloaded.autoRegisterLocalMcpClientsEnabled)
    }

    private func makeStore() -> SettingsStore {
        let suiteName = "AetowerSettingsTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        addTeardownBlock {
            defaults.removePersistentDomain(forName: suiteName)
        }
        return SettingsStore(defaults: defaults)
    }
}
