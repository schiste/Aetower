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

/// Which timeline event category an automation rule reacts to.
public enum AutomationEvent: String, CaseIterable, Identifiable, Codable, Sendable {
    case any
    case lifecycle
    case friction
    case host
    case thermal
    case anomaly
    case networkConnection = "network-connection"

    public var id: String { rawValue }

    public var label: String {
        switch self {
        case .any: return "Any event"
        case .lifecycle: return "Process launch/exit"
        case .friction: return "Friction shift"
        case .host: return "Host change"
        case .thermal: return "Thermal change"
        case .anomaly: return "Anomaly detected"
        case .networkConnection: return "Network connection"
        }
    }

    /// True when this event can be further narrowed by the signing-class /
    /// remote-host predicates (only network-connection rules use those).
    public var supportsConnectionPredicates: Bool {
        self == .networkConnection
    }
}

/// Code-signing class of a process, mirroring the Rust classifier taxonomy
/// (`aetower_model::classify_signature`). Raw values match the strings carried
/// on `EntitySnapshot.signingClassification`, so a rule's stored classes match
/// a live entity by exact string — and an unknown future class still
/// round-trips through persistence.
public enum SigningClass: String, CaseIterable, Identifiable, Codable, Sendable {
    case unsigned
    case adhoc
    case developerID = "developer_id"
    case macAppStore = "mac_app_store"
    case apple
    case other

    public var id: String { rawValue }

    public var label: String {
        switch self {
        case .unsigned: return "Unsigned"
        case .adhoc: return "Ad-hoc"
        case .developerID: return "Developer ID"
        case .macAppStore: return "Mac App Store"
        case .apple: return "Apple system"
        case .other: return "Other"
        }
    }
}

public enum AutomationActionKind: String, CaseIterable, Identifiable, Codable, Sendable {
    case shortcut
    case shell

    public var id: String { rawValue }

    public var label: String {
        switch self {
        case .shortcut: return "Run Shortcut"
        case .shell: return "Run shell command"
        }
    }
}

/// A user-defined automation: when a matching timeline event appears, run a
/// macOS Shortcut or a shell command. Evaluated by `AppState` on each snapshot
/// while the app is running.
public struct AutomationRule: Codable, Identifiable, Sendable {
    public var id: UUID
    public var enabled: Bool
    public var name: String
    public var event: AutomationEvent
    public var titleContains: String
    public var actionKind: AutomationActionKind
    public var actionValue: String
    /// For `networkConnection` rules: match only when the connecting entity's
    /// signing class is one of these (raw `SigningClass` values). Empty = any
    /// signing class. Ignored for non-network events.
    public var signingClasses: [String]
    /// For `networkConnection` rules: match only when a remote endpoint
    /// contains this substring (case-insensitive). Empty = any host.
    public var remoteHostContains: String

    public init(
        id: UUID = UUID(),
        enabled: Bool = true,
        name: String = "New rule",
        event: AutomationEvent = .lifecycle,
        titleContains: String = "",
        actionKind: AutomationActionKind = .shortcut,
        actionValue: String = "",
        signingClasses: [String] = [],
        remoteHostContains: String = ""
    ) {
        self.id = id
        self.enabled = enabled
        self.name = name
        self.event = event
        self.titleContains = titleContains
        self.actionKind = actionKind
        self.actionValue = actionValue
        self.signingClasses = signingClasses
        self.remoteHostContains = remoteHostContains
    }

    // Custom decoding so rules persisted before the connection predicates
    // existed still load (synthesized Codable would reject the missing keys and
    // `AutomationStore` would silently drop every saved rule).
    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(UUID.self, forKey: .id)
        enabled = try container.decode(Bool.self, forKey: .enabled)
        name = try container.decode(String.self, forKey: .name)
        event = try container.decode(AutomationEvent.self, forKey: .event)
        titleContains = try container.decode(String.self, forKey: .titleContains)
        actionKind = try container.decode(AutomationActionKind.self, forKey: .actionKind)
        actionValue = try container.decode(String.self, forKey: .actionValue)
        signingClasses =
            try container.decodeIfPresent([String].self, forKey: .signingClasses) ?? []
        remoteHostContains =
            try container.decodeIfPresent(String.self, forKey: .remoteHostContains) ?? ""
    }
}

/// Standalone UserDefaults persistence for automation rules, independent of
/// `SettingsStore`'s per-property persist machinery.
public enum AutomationStore {
    private static let key = "aetower.automationRules"

    public static func load() -> [AutomationRule] {
        guard let data = UserDefaults.standard.data(forKey: key) else { return [] }
        return (try? JSONDecoder().decode([AutomationRule].self, from: data)) ?? []
    }

    public static func save(_ rules: [AutomationRule]) {
        guard let data = try? JSONEncoder().encode(rules) else { return }
        UserDefaults.standard.set(data, forKey: key)
    }
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
    /// Electricity price used to translate per-process power into running cost
    /// (currency per kWh). Default is the global average.
    public var electricityPricePerKwh: Double {
        didSet { persist() }
    }
    /// Grid carbon intensity (gCO₂ per kWh) for the sustainability translation.
    /// Default is the world-average grid.
    public var gridCarbonIntensityGramsPerKwh: Double {
        didSet { persist() }
    }
    /// Currency symbol shown alongside energy cost figures.
    public var energyCurrencySymbol: String {
        didSet { persist() }
    }
    public var appearanceMode: String {
        didSet { persist() }
    }
    public var operatorSafeModeEnabled: Bool {
        didSet { persist() }
    }
    /// Opt-in consent for VirusTotal binary-reputation lookups (off by default).
    /// The API key itself lives in the Keychain, not here.
    public var binaryReputationEnabled: Bool {
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
        self.electricityPricePerKwh = defaults.object(forKey: Self.electricityPricePerKwhKey) as? Double ?? 0.15
        self.gridCarbonIntensityGramsPerKwh = defaults.object(forKey: Self.gridCarbonIntensityGramsPerKwhKey) as? Double ?? 480.0
        self.energyCurrencySymbol = defaults.string(forKey: Self.energyCurrencySymbolKey) ?? "$"
        self.appearanceMode = defaults.string(forKey: Self.appearanceModeKey) ?? "system"
        self.operatorSafeModeEnabled = defaults.object(forKey: Self.operatorSafeModeEnabledKey) as? Bool ?? true
        self.binaryReputationEnabled = defaults.object(forKey: Self.binaryReputationEnabledKey) as? Bool ?? false
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
    private static let electricityPricePerKwhKey = "settings.electricityPricePerKwh"
    private static let gridCarbonIntensityGramsPerKwhKey = "settings.gridCarbonIntensityGramsPerKwh"
    private static let energyCurrencySymbolKey = "settings.energyCurrencySymbol"
    private static let appearanceModeKey = "settings.appearanceMode"
    private static let operatorSafeModeEnabledKey = "settings.operatorSafeModeEnabled"
    private static let binaryReputationEnabledKey = "settings.binaryReputationEnabled"
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
    /// Reset persisted settings to factory defaults. Runtime application
    /// state still belongs to the caller because SettingsStore intentionally
    /// does not own AppState or the engine bridge.
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
        electricityPricePerKwh = 0.15
        gridCarbonIntensityGramsPerKwh = 480.0
        energyCurrencySymbol = "$"
        appearanceMode = "system"
        operatorSafeModeEnabled = true
        binaryReputationEnabled = false
        KeychainHelper.delete(account: KeychainHelper.binaryReputationAccount)
        exportPrivacyTier = .redacted
        autoRegisterLocalMcpClientsEnabled = false
        if launchAtLoginEnabled {
            setLaunchAtLogin(false)
        } else {
            launchAtLoginError = nil
        }
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
        defaults.set(electricityPricePerKwh, forKey: Self.electricityPricePerKwhKey)
        defaults.set(gridCarbonIntensityGramsPerKwh, forKey: Self.gridCarbonIntensityGramsPerKwhKey)
        defaults.set(energyCurrencySymbol, forKey: Self.energyCurrencySymbolKey)
        defaults.set(appearanceMode, forKey: Self.appearanceModeKey)
        defaults.set(operatorSafeModeEnabled, forKey: Self.operatorSafeModeEnabledKey)
        defaults.set(binaryReputationEnabled, forKey: Self.binaryReputationEnabledKey)
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
