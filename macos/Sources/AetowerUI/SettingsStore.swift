import Foundation
import Observation
import ServiceManagement

public enum ExportPrivacyTier: String, CaseIterable, Identifiable {
    case redacted
    case operatorMode = "operator"
    case full

    public var id: String { rawValue }
}

public enum CollectionProfile: String, CaseIterable, Identifiable {
    case balanced
    case full

    public var id: String { rawValue }
}

@MainActor
@Observable
public final class SettingsStore {
    public static let defaultDockerSocketPath = "/var/run/docker.sock"
    public static let defaultTelemetryEndpoint = "http://localhost:4318/v1/metrics"
    public static let minimumTelemetryExportIntervalSeconds = 5.0
    public static let minimumEngineTickSeconds = 0.5
    public static let minimumGPUSampleIntervalSeconds = 5.0

    public var showMenuBarExtra: Bool {
        didSet { persist() }
    }
    public var refreshIntervalSeconds: Double {
        didSet { persist() }
    }
    public var chromiumEndpoint: String {
        didSet { persist() }
    }
    public var dockerSocketPath: String {
        didSet { persist() }
    }
    public var privilegedHelperPath: String {
        didSet { persist() }
    }
    public var privilegedHelperEnabled: Bool {
        didSet { persist() }
    }
    public var chau7Endpoint: String {
        didSet { persist() }
    }
    public var telemetryEnabled: Bool {
        didSet { persist() }
    }
    public var telemetryEndpoint: String {
        didSet { persist() }
    }
    public var telemetryExportIntervalSeconds: Double {
        didSet { persist() }
    }
    public var collectionProfile: CollectionProfile {
        didSet { persist() }
    }
    public var adaptiveCadenceEnabled: Bool {
        didSet { persist() }
    }
    public var engineActiveIntervalSeconds: Double {
        didSet {
            enforceRuntimeIntervalRelationships()
            persist()
        }
    }
    public var engineIdleIntervalSeconds: Double {
        didSet {
            enforceRuntimeIntervalRelationships()
            persist()
        }
    }
    public var engineLowPowerIntervalSeconds: Double {
        didSet {
            enforceRuntimeIntervalRelationships()
            persist()
        }
    }
    public var gpuSampleIntervalSeconds: Double {
        didSet {
            enforceRuntimeIntervalRelationships()
            persist()
        }
    }
    public var gpuSampleLowPowerIntervalSeconds: Double {
        didSet {
            enforceRuntimeIntervalRelationships()
            persist()
        }
    }
    public var notificationsEnabled: Bool {
        didSet { persist() }
    }
    public var frictionNotificationThreshold: Double {
        didSet { persist() }
    }
    public var appearanceMode: String {
        didSet { persist() }
    }
    public var operatorSafeModeEnabled: Bool {
        didSet { persist() }
    }
    public var exportPrivacyTier: ExportPrivacyTier {
        didSet { persist() }
    }
    public var autoRegisterLocalMcpClientsEnabled: Bool {
        didSet { persist() }
    }
    public private(set) var launchAtLoginEnabled: Bool
    public private(set) var launchAtLoginError: String?

    @ObservationIgnored
    private let defaults: UserDefaults

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        self.showMenuBarExtra = defaults.object(forKey: Self.showMenuBarExtraKey) as? Bool ?? true
        self.refreshIntervalSeconds = defaults.object(forKey: Self.refreshIntervalKey) as? Double ?? 2.0
        self.chromiumEndpoint = defaults.string(forKey: Self.chromiumEndpointKey) ?? ""
        self.dockerSocketPath = defaults.string(forKey: Self.dockerSocketPathKey) ?? Self.defaultDockerSocketPath
        self.privilegedHelperPath = defaults.string(forKey: Self.privilegedHelperPathKey)
            ?? Self.defaultPrivilegedHelperPath()
        self.privilegedHelperEnabled = defaults.object(forKey: Self.privilegedHelperEnabledKey) as? Bool ?? false
        self.chau7Endpoint = defaults.string(forKey: Self.chau7EndpointKey) ?? ""
        self.telemetryEnabled = defaults.object(forKey: Self.telemetryEnabledKey) as? Bool ?? false
        self.telemetryEndpoint = defaults.string(forKey: Self.telemetryEndpointKey) ?? Self.defaultTelemetryEndpoint
        self.telemetryExportIntervalSeconds = defaults.object(forKey: Self.telemetryExportIntervalKey) as? Double ?? 30.0
        self.collectionProfile = CollectionProfile(
            rawValue: defaults.string(forKey: Self.collectionProfileKey) ?? ""
        ) ?? .balanced
        self.adaptiveCadenceEnabled = defaults.object(forKey: Self.adaptiveCadenceEnabledKey) as? Bool ?? true
        self.engineActiveIntervalSeconds = defaults.object(forKey: Self.engineActiveIntervalKey) as? Double ?? 2.0
        self.engineIdleIntervalSeconds = defaults.object(forKey: Self.engineIdleIntervalKey) as? Double ?? 5.0
        self.engineLowPowerIntervalSeconds = defaults.object(forKey: Self.engineLowPowerIntervalKey) as? Double ?? 8.0
        self.gpuSampleIntervalSeconds = defaults.object(forKey: Self.gpuSampleIntervalKey) as? Double ?? 30.0
        self.gpuSampleLowPowerIntervalSeconds = defaults.object(forKey: Self.gpuSampleLowPowerIntervalKey) as? Double ?? 60.0
        self.notificationsEnabled = defaults.object(forKey: Self.notificationsEnabledKey) as? Bool ?? false
        self.frictionNotificationThreshold = defaults.object(forKey: Self.frictionNotificationThresholdKey) as? Double ?? 60.0
        self.appearanceMode = defaults.string(forKey: Self.appearanceModeKey) ?? "system"
        self.operatorSafeModeEnabled = defaults.object(forKey: Self.operatorSafeModeEnabledKey) as? Bool ?? true
        let persistedTier = defaults.string(forKey: Self.exportPrivacyTierKey)
        let legacySensitive = defaults.object(forKey: Self.includeSensitiveExportsKey) as? Bool ?? false
        self.exportPrivacyTier = ExportPrivacyTier(rawValue: persistedTier ?? "")
            ?? (legacySensitive ? .full : .redacted)
        self.autoRegisterLocalMcpClientsEnabled = defaults.object(
            forKey: Self.autoRegisterLocalMcpClientsEnabledKey
        ) as? Bool ?? false
        self.launchAtLoginEnabled = false
        self.launchAtLoginError = nil
        normalizeLoadedValues()
        syncLaunchAtLoginState()
    }

    public func setLaunchAtLogin(_ enabled: Bool) {
        let service = SMAppService.mainApp
        do {
            if enabled {
                try service.register()
            } else {
                try service.unregister()
            }
            syncLaunchAtLoginState()
            if enabled && service.status == .requiresApproval {
                launchAtLoginError = "Launch at login requires approval in System Settings."
            } else {
                launchAtLoginError = nil
            }
        } catch {
            syncLaunchAtLoginState()
            launchAtLoginError = error.localizedDescription
        }
    }

    private func syncLaunchAtLoginState() {
        launchAtLoginEnabled = SMAppService.mainApp.status == .enabled
    }

    private static let showMenuBarExtraKey = "settings.showMenuBarExtra"
    private static let refreshIntervalKey = "settings.refreshIntervalSeconds"
    private static let chromiumEndpointKey = "settings.chromiumEndpoint"
    private static let dockerSocketPathKey = "settings.dockerSocketPath"
    private static let privilegedHelperPathKey = "settings.privilegedHelperPath"
    private static let privilegedHelperEnabledKey = "settings.privilegedHelperEnabled"
    private static let chau7EndpointKey = "settings.chau7Endpoint"
    private static let telemetryEnabledKey = "settings.telemetryEnabled"
    private static let telemetryEndpointKey = "settings.telemetryEndpoint"
    private static let telemetryExportIntervalKey = "settings.telemetryExportIntervalSeconds"
    private static let collectionProfileKey = "settings.collectionProfile"
    private static let adaptiveCadenceEnabledKey = "settings.adaptiveCadenceEnabled"
    private static let engineActiveIntervalKey = "settings.engineActiveIntervalSeconds"
    private static let engineIdleIntervalKey = "settings.engineIdleIntervalSeconds"
    private static let engineLowPowerIntervalKey = "settings.engineLowPowerIntervalSeconds"
    private static let gpuSampleIntervalKey = "settings.gpuSampleIntervalSeconds"
    private static let gpuSampleLowPowerIntervalKey = "settings.gpuSampleLowPowerIntervalSeconds"
    private static let notificationsEnabledKey = "settings.notificationsEnabled"
    private static let frictionNotificationThresholdKey = "settings.frictionNotificationThreshold"
    private static let appearanceModeKey = "settings.appearanceMode"
    private static let operatorSafeModeEnabledKey = "settings.operatorSafeModeEnabled"
    private static let exportPrivacyTierKey = "settings.exportPrivacyTier"
    private static let autoRegisterLocalMcpClientsEnabledKey = "settings.autoRegisterLocalMcpClientsEnabled"
    private static let includeSensitiveExportsKey = "settings.includeSensitiveExports"

    /// Read the persisted export privacy tier directly from UserDefaults.
    /// Used by AppState to check the tier without going through the
    /// SettingsStore instance (which may not be available in all contexts).
    public static func persistedExportPrivacyTier(
        defaults: UserDefaults = .standard
    ) -> ExportPrivacyTier {
        if let raw = defaults.string(forKey: exportPrivacyTierKey) {
            return ExportPrivacyTier(rawValue: raw) ?? .redacted
        }
        let legacySensitive = defaults.object(forKey: includeSensitiveExportsKey) as? Bool ?? false
        return legacySensitive ? .full : .redacted
    }

    public static func normalizedDockerSocketPath(_ value: String) -> String {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? defaultDockerSocketPath : trimmed
    }

    public static func normalizedTelemetryEndpoint(_ value: String) -> String {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? defaultTelemetryEndpoint : trimmed
    }

    public static func normalizedTelemetryExportIntervalSeconds(_ value: Double) -> UInt32 {
        UInt32(max(Int(minimumTelemetryExportIntervalSeconds), Int(value.rounded())))
    }

    public static func milliseconds(from seconds: Double, minimumSeconds: Double) -> UInt64 {
        UInt64(max(Int((minimumSeconds * 1000).rounded()), Int((seconds * 1000).rounded())))
    }

    private func normalizeLoadedValues() {
        refreshIntervalSeconds = max(1.0, refreshIntervalSeconds)
        telemetryExportIntervalSeconds = max(Self.minimumTelemetryExportIntervalSeconds, telemetryExportIntervalSeconds)
        engineActiveIntervalSeconds = max(Self.minimumEngineTickSeconds, engineActiveIntervalSeconds)
        engineIdleIntervalSeconds = max(Self.minimumEngineTickSeconds, engineIdleIntervalSeconds)
        engineLowPowerIntervalSeconds = max(Self.minimumEngineTickSeconds, engineLowPowerIntervalSeconds)
        gpuSampleIntervalSeconds = max(Self.minimumGPUSampleIntervalSeconds, gpuSampleIntervalSeconds)
        gpuSampleLowPowerIntervalSeconds = max(Self.minimumGPUSampleIntervalSeconds, gpuSampleLowPowerIntervalSeconds)
        enforceRuntimeIntervalRelationships()
    }

    private func enforceRuntimeIntervalRelationships() {
        if engineIdleIntervalSeconds < engineActiveIntervalSeconds {
            engineIdleIntervalSeconds = engineActiveIntervalSeconds
        }
        if engineLowPowerIntervalSeconds < engineActiveIntervalSeconds {
            engineLowPowerIntervalSeconds = engineActiveIntervalSeconds
        }
        if gpuSampleLowPowerIntervalSeconds < gpuSampleIntervalSeconds {
            gpuSampleLowPowerIntervalSeconds = gpuSampleIntervalSeconds
        }
    }
}

extension SettingsStore {
    /// Reset all settings to factory defaults. Does NOT call
    /// applyIntegrationSettings — the caller should trigger that
    /// separately to push the reset values to the running engine.
    public func resetToDefaults() {
        showMenuBarExtra = true
        refreshIntervalSeconds = 2.0
        chromiumEndpoint = ""
        dockerSocketPath = Self.defaultDockerSocketPath
        privilegedHelperPath = Self.defaultPrivilegedHelperPath()
        privilegedHelperEnabled = false
        chau7Endpoint = ""
        telemetryEnabled = false
        telemetryEndpoint = Self.defaultTelemetryEndpoint
        telemetryExportIntervalSeconds = 30.0
        collectionProfile = .balanced
        adaptiveCadenceEnabled = true
        engineActiveIntervalSeconds = 2.0
        engineIdleIntervalSeconds = 5.0
        engineLowPowerIntervalSeconds = 8.0
        gpuSampleIntervalSeconds = 30.0
        gpuSampleLowPowerIntervalSeconds = 60.0
        notificationsEnabled = false
        frictionNotificationThreshold = 60.0
        appearanceMode = "system"
        operatorSafeModeEnabled = true
        exportPrivacyTier = .redacted
        autoRegisterLocalMcpClientsEnabled = false
    }

    func persist() {
        defaults.set(showMenuBarExtra, forKey: Self.showMenuBarExtraKey)
        defaults.set(refreshIntervalSeconds, forKey: Self.refreshIntervalKey)
        defaults.set(chromiumEndpoint, forKey: Self.chromiumEndpointKey)
        defaults.set(dockerSocketPath, forKey: Self.dockerSocketPathKey)
        defaults.set(privilegedHelperPath, forKey: Self.privilegedHelperPathKey)
        defaults.set(privilegedHelperEnabled, forKey: Self.privilegedHelperEnabledKey)
        defaults.set(chau7Endpoint, forKey: Self.chau7EndpointKey)
        defaults.set(telemetryEnabled, forKey: Self.telemetryEnabledKey)
        defaults.set(telemetryEndpoint, forKey: Self.telemetryEndpointKey)
        defaults.set(telemetryExportIntervalSeconds, forKey: Self.telemetryExportIntervalKey)
        defaults.set(collectionProfile.rawValue, forKey: Self.collectionProfileKey)
        defaults.set(adaptiveCadenceEnabled, forKey: Self.adaptiveCadenceEnabledKey)
        defaults.set(engineActiveIntervalSeconds, forKey: Self.engineActiveIntervalKey)
        defaults.set(engineIdleIntervalSeconds, forKey: Self.engineIdleIntervalKey)
        defaults.set(engineLowPowerIntervalSeconds, forKey: Self.engineLowPowerIntervalKey)
        defaults.set(gpuSampleIntervalSeconds, forKey: Self.gpuSampleIntervalKey)
        defaults.set(gpuSampleLowPowerIntervalSeconds, forKey: Self.gpuSampleLowPowerIntervalKey)
        defaults.set(notificationsEnabled, forKey: Self.notificationsEnabledKey)
        defaults.set(frictionNotificationThreshold, forKey: Self.frictionNotificationThresholdKey)
        defaults.set(appearanceMode, forKey: Self.appearanceModeKey)
        defaults.set(operatorSafeModeEnabled, forKey: Self.operatorSafeModeEnabledKey)
        defaults.set(exportPrivacyTier.rawValue, forKey: Self.exportPrivacyTierKey)
        defaults.set(autoRegisterLocalMcpClientsEnabled, forKey: Self.autoRegisterLocalMcpClientsEnabledKey)
        defaults.set(exportPrivacyTier == .full, forKey: Self.includeSensitiveExportsKey)
    }

    private static func defaultPrivilegedHelperPath(
        fileManager: FileManager = .default
    ) -> String {
        let path = Bundle.main.bundleURL
            .appendingPathComponent("Contents/Helpers/aetower-helper").path
        return fileManager.isExecutableFile(atPath: path) ? path : ""
    }
}
