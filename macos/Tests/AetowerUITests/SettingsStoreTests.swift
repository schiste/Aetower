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
        XCTAssertEqual(
            SettingsStore.normalizedChau7AgentCommand("  "),
            SettingsStore.defaultChau7AgentCommand
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
        XCTAssertEqual(store.chau7AgentCommand, SettingsStore.defaultChau7AgentCommand)
        XCTAssertEqual(store.collectionProfile, .balanced)
        XCTAssertEqual(store.repositoryRoots, SettingsStore.defaultRepositoryRoots)
        XCTAssertFalse(store.telemetryEnabled)
    }

    func testPublicPreviewDefaultsStayConservative() {
        let store = makeStore()

        XCTAssertFalse(store.privilegedHelperEnabled)
        XCTAssertFalse(store.telemetryEnabled)
        XCTAssertFalse(store.autoRegisterLocalMcpClientsEnabled)
        XCTAssertFalse(store.storageScheduledScansEnabled)
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

    func testRepositoryRootsDefaultToRepositoryOnlyLocations() {
        let store = makeStore()

        XCTAssertEqual(
            store.repositoryRoots,
            [
                "~/Repositories",
                "~/Downloads/Repositories",
                "~/Developer",
                "~/Projects",
            ]
        )
    }

    func testRepositoryRootsPersistAndDeduplicate() {
        let suiteName = "AetowerSettingsTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = SettingsStore(defaults: defaults)
        store.repositoryRoots = [
            " ~/Wikimedia/ ",
            "~/Wikimedia",
            "~/pentagi",
            "",
            "~/Downloads/Aerie",
        ]

        let reloaded = SettingsStore(defaults: defaults)
        XCTAssertEqual(reloaded.repositoryRoots, ["~/Wikimedia", "~/pentagi", "~/Downloads/Aerie"])
    }

    func testGitHubOAuthConfigurationPersistsAndNormalizesScopes() {
        let suiteName = "AetowerSettingsTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = SettingsStore(defaults: defaults)
        store.githubOAuthClientID = " client-123 "
        store.githubOAuthScopes = " repo   read:user repo "

        let reloaded = SettingsStore(defaults: defaults)
        XCTAssertEqual(reloaded.githubOAuthClientID, "client-123")
        XCTAssertEqual(reloaded.githubOAuthScopes, "repo read:user")
    }

    func testCloudflareOAuthConfigurationPersistsAndNormalizesMetadata() {
        let suiteName = "AetowerSettingsTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = SettingsStore(defaults: defaults)
        store.cloudflareOAuthClientID = " cf-client "
        store.cloudflareOAuthAccountID = " account-123 "
        store.cloudflareOAuthScopes = " pages.read   workers.scripts.read pages.read "
        store.cloudflareOAuthRedirectURI = " aetower://oauth/cloudflare/callback "

        let reloaded = SettingsStore(defaults: defaults)
        XCTAssertEqual(reloaded.cloudflareOAuthClientID, "cf-client")
        XCTAssertEqual(reloaded.cloudflareOAuthAccountID, "account-123")
        XCTAssertEqual(reloaded.cloudflareOAuthScopes, "pages.read workers.scripts.read")
        XCTAssertEqual(reloaded.cloudflareOAuthRedirectURI, "aetower://oauth/cloudflare/callback")
    }

    func testMetricRingFixedScalingPreferencePersists() {
        let suiteName = "AetowerSettingsTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = SettingsStore(defaults: defaults)
        store.metricRingsFixedScaling = true

        let reloaded = SettingsStore(defaults: defaults)
        XCTAssertTrue(reloaded.metricRingsFixedScaling)
    }

    func testStorageScheduledScanPreferencesPersistAndClamp() {
        let suiteName = "AetowerSettingsTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = SettingsStore(defaults: defaults)
        store.storageScheduledScansEnabled = true
        store.storageScheduledScanIntervalHours = 0.1

        let reloaded = SettingsStore(defaults: defaults)
        XCTAssertTrue(reloaded.storageScheduledScansEnabled)
        XCTAssertEqual(
            reloaded.storageScheduledScanIntervalHours,
            SettingsStore.minimumStorageScheduledScanIntervalHours
        )
    }

    func testChau7AgentCommandPreferencePersistsAndNormalizesBlankValues() {
        let suiteName = "AetowerSettingsTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = SettingsStore(defaults: defaults)
        store.chau7AgentCommand = "codex --ask-for-approval never"

        let reloaded = SettingsStore(defaults: defaults)
        XCTAssertEqual(reloaded.chau7AgentCommand, "codex --ask-for-approval never")

        reloaded.chau7AgentCommand = " \n "
        let normalized = SettingsStore(defaults: defaults)
        XCTAssertEqual(normalized.chau7AgentCommand, SettingsStore.defaultChau7AgentCommand)
    }

    func testNumericRuntimeNormalizersRejectNonFiniteValues() {
        XCTAssertEqual(
            SettingsStore.normalizedTelemetryExportIntervalSeconds(.nan),
            UInt32(SettingsStore.minimumTelemetryExportIntervalSeconds)
        )
        XCTAssertEqual(
            SettingsStore.normalizedTelemetryExportIntervalSeconds(.infinity),
            UInt32(SettingsStore.minimumTelemetryExportIntervalSeconds)
        )
        XCTAssertEqual(
            SettingsStore.milliseconds(from: .nan, minimumSeconds: 0.5),
            500
        )
        XCTAssertEqual(
            SettingsStore.milliseconds(from: .infinity, minimumSeconds: 0.5),
            500
        )
        XCTAssertEqual(
            SettingsStore.milliseconds(from: 0.1, minimumSeconds: 0.5),
            500
        )
    }

    func testLoadedNumericSettingsSanitizeNonFiniteDefaults() {
        let suiteName = "AetowerSettingsTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = SettingsStore(defaults: defaults)
        store.refreshIntervalSeconds = .nan
        store.telemetryExportIntervalSeconds = .infinity
        store.engineActiveIntervalSeconds = .nan
        store.engineIdleIntervalSeconds = .nan
        store.engineLowPowerIntervalSeconds = .nan
        store.gpuSampleIntervalSeconds = .nan
        store.gpuSampleLowPowerIntervalSeconds = .nan
        store.frictionNotificationThreshold = .nan
        store.electricityPricePerKwh = .nan
        store.gridCarbonIntensityGramsPerKwh = .nan
        store.storageScheduledScanIntervalHours = .nan

        let reloaded = SettingsStore(defaults: defaults)

        XCTAssertEqual(reloaded.refreshIntervalSeconds, 2.0)
        XCTAssertEqual(reloaded.telemetryExportIntervalSeconds, 30.0)
        XCTAssertEqual(reloaded.engineActiveIntervalSeconds, 2.0)
        XCTAssertEqual(reloaded.engineIdleIntervalSeconds, 5.0)
        XCTAssertEqual(reloaded.engineLowPowerIntervalSeconds, 8.0)
        XCTAssertEqual(reloaded.gpuSampleIntervalSeconds, 30.0)
        XCTAssertEqual(reloaded.gpuSampleLowPowerIntervalSeconds, 60.0)
        XCTAssertEqual(reloaded.frictionNotificationThreshold, 60.0)
        XCTAssertEqual(reloaded.electricityPricePerKwh, 0.15)
        XCTAssertEqual(reloaded.gridCarbonIntensityGramsPerKwh, 480.0)
        XCTAssertEqual(reloaded.storageScheduledScanIntervalHours, 24.0)
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
