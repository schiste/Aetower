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
        didSet { persist() }
    }
    public var engineIdleIntervalSeconds: Double {
        didSet { persist() }
    }
    public var engineLowPowerIntervalSeconds: Double {
        didSet { persist() }
    }
    public var gpuSampleIntervalSeconds: Double {
        didSet { persist() }
    }
    public var gpuSampleLowPowerIntervalSeconds: Double {
        didSet { persist() }
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
    public var exportPrivacyTier: ExportPrivacyTier {
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
        self.dockerSocketPath = defaults.string(forKey: Self.dockerSocketPathKey) ?? "/var/run/docker.sock"
        self.privilegedHelperPath = defaults.string(forKey: Self.privilegedHelperPathKey)
            ?? Bundle.main.bundleURL.appendingPathComponent("Contents/Helpers/aetower-helper").path
        self.privilegedHelperEnabled = defaults.object(forKey: Self.privilegedHelperEnabledKey) as? Bool ?? false
        self.chau7Endpoint = defaults.string(forKey: Self.chau7EndpointKey) ?? ""
        self.telemetryEnabled = defaults.object(forKey: Self.telemetryEnabledKey) as? Bool ?? false
        self.telemetryEndpoint = defaults.string(forKey: Self.telemetryEndpointKey) ?? "http://localhost:4318/v1/metrics"
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
        let persistedTier = defaults.string(forKey: Self.exportPrivacyTierKey)
        let legacySensitive = defaults.object(forKey: Self.includeSensitiveExportsKey) as? Bool ?? false
        self.exportPrivacyTier = ExportPrivacyTier(rawValue: persistedTier ?? "")
            ?? (legacySensitive ? .full : .redacted)
        self.launchAtLoginEnabled = false
        self.launchAtLoginError = nil
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
    static let exportPrivacyTierKey = "settings.exportPrivacyTier"
    static let includeSensitiveExportsKey = "settings.includeSensitiveExports"
}

extension SettingsStore {
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
        defaults.set(exportPrivacyTier.rawValue, forKey: Self.exportPrivacyTierKey)
        defaults.set(exportPrivacyTier == .full, forKey: Self.includeSensitiveExportsKey)
    }
}
