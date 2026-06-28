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

public enum MonitorMetricCardPlacement: String, CaseIterable, Hashable, Identifiable {
    case top
    case left
    case right
    case bottom

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .top: return "Top"
        case .left: return "Left"
        case .right: return "Right"
        case .bottom: return "Bottom"
        }
    }

    public var supportsFocusedMetric: Bool {
        switch self {
        case .top, .bottom:
            return true
        case .left, .right:
            return false
        }
    }
}

public enum MonitorMetricCardFocus: String, CaseIterable, Hashable, Identifiable {
    case all
    case friction
    case cpu
    case memory
    case disk
    case network
    case wakeups
    case gpu

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .all: return "All rings"
        case .friction: return "Friction"
        case .cpu: return "CPU"
        case .memory: return "Memory"
        case .disk: return "Disk"
        case .network: return "Network"
        case .wakeups: return "Wakeups"
        case .gpu: return "GPU"
        }
    }
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
    case agentBudget = "agent-budget"

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
        case .agentBudget: return "AI agent budget"
        }
    }

    /// True when this event can be further narrowed by the signing-class /
    /// remote-host predicates (only network-connection rules use those).
    public var supportsConnectionPredicates: Bool {
        self == .networkConnection
    }

    /// True when this event uses the budget metric + threshold fields
    /// (only AI-agent-budget rules do).
    public var supportsBudgetFields: Bool {
        self == .agentBudget
    }
}

/// The metric an agent-budget rule watches.
public enum AgentBudgetMetric: String, CaseIterable, Identifiable, Codable, Sendable {
    case costPerHour = "cost-per-hour"
    case tokensPerHour = "tokens-per-hour"

    public var id: String { rawValue }

    public var label: String {
        switch self {
        case .costPerHour: return "Cost per hour ($)"
        case .tokensPerHour: return "Tokens per hour"
        }
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
    /// For `agentBudget` rules: which rate to watch.
    public var budgetMetric: String
    /// For `agentBudget` rules: fire when the watched rate exceeds this value
    /// ($/hr for cost, tokens/hr for tokens). `titleContains` scopes by agent
    /// name (empty = any agent).
    public var budgetThreshold: Double

    public init(
        id: UUID = UUID(),
        enabled: Bool = true,
        name: String = "New rule",
        event: AutomationEvent = .lifecycle,
        titleContains: String = "",
        actionKind: AutomationActionKind = .shortcut,
        actionValue: String = "",
        signingClasses: [String] = [],
        remoteHostContains: String = "",
        budgetMetric: String = AgentBudgetMetric.costPerHour.rawValue,
        budgetThreshold: Double = 0
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
        self.budgetMetric = budgetMetric
        self.budgetThreshold = budgetThreshold
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
        budgetMetric =
            try container.decodeIfPresent(String.self, forKey: .budgetMetric)
            ?? AgentBudgetMetric.costPerHour.rawValue
        budgetThreshold =
            try container.decodeIfPresent(Double.self, forKey: .budgetThreshold) ?? 0
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

/// One snoozed app: notifications for this `bundleId` are suppressed until
/// `untilMillis` (Unix epoch milliseconds). Keyed by bundle id because it is
/// stable across restarts, unlike the per-session entity id.
public struct NotificationSnooze: Codable, Identifiable, Sendable {
    public var bundleId: String
    public var displayName: String
    public var untilMillis: UInt64

    public var id: String { bundleId }

    public init(bundleId: String, displayName: String, untilMillis: UInt64) {
        self.bundleId = bundleId
        self.displayName = displayName
        self.untilMillis = untilMillis
    }
}

/// Standalone UserDefaults persistence for notification snoozes, mirroring
/// `AutomationStore`.
public enum NotificationSnoozeStore {
    private static let key = "aetower.notificationSnoozes"

    public static func load() -> [NotificationSnooze] {
        guard let data = UserDefaults.standard.data(forKey: key) else { return [] }
        return (try? JSONDecoder().decode([NotificationSnooze].self, from: data)) ?? []
    }

    public static func save(_ snoozes: [NotificationSnooze]) {
        guard let data = try? JSONEncoder().encode(snoozes) else { return }
        UserDefaults.standard.set(data, forKey: key)
    }
}

@MainActor
@Observable
public final class SettingsStore {
    public nonisolated static let defaultDockerSocketPath = "/var/run/docker.sock"
    public nonisolated static let defaultTelemetryEndpoint = "http://localhost:4318/v1/metrics"
    public nonisolated static let defaultChau7AgentCommand = "claude"
    public nonisolated static let minimumTelemetryExportIntervalSeconds = 5.0
    public nonisolated static let minimumEngineTickSeconds = 0.5
    public nonisolated static let minimumGPUSampleIntervalSeconds = 5.0

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
    public var chau7AgentCommand: String {
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
    /// Per-category notification toggles (all gated by `notificationsEnabled`).
    /// Drive the unified timeline-notification evaluator.
    public var notifyThermal: Bool {
        didSet { persist() }
    }
    public var notifyRegression: Bool {
        didSet { persist() }
    }
    public var notifyRestartLoop: Bool {
        didSet { persist() }
    }
    public var notifyNetwork: Bool {
        didSet { persist() }
    }
    public var notifyAgentBudget: Bool {
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
    public var monitorMetricCardPlacement: MonitorMetricCardPlacement {
        didSet { persist() }
    }
    public var monitorMetricCardFocus: MonitorMetricCardFocus {
        didSet { persist() }
    }
    /// Draw the Monitor rings on a fixed absolute axis (0–100, or 0 to the
    /// danger threshold for disk/network/wakeups) instead of auto-scaling each
    /// ring to its recent range. Off by default.
    public var metricRingsFixedScaling: Bool {
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
        self.chau7AgentCommand = defaults.string(forKey: Self.chau7AgentCommandKey)
            ?? Self.defaultChau7AgentCommand
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
        self.notifyThermal = defaults.object(forKey: Self.notifyThermalKey) as? Bool ?? true
        self.notifyRegression = defaults.object(forKey: Self.notifyRegressionKey) as? Bool ?? true
        self.notifyRestartLoop = defaults.object(forKey: Self.notifyRestartLoopKey) as? Bool ?? true
        self.notifyNetwork = defaults.object(forKey: Self.notifyNetworkKey) as? Bool ?? false
        self.notifyAgentBudget = defaults.object(forKey: Self.notifyAgentBudgetKey) as? Bool ?? true
        self.electricityPricePerKwh = defaults.object(forKey: Self.electricityPricePerKwhKey) as? Double ?? 0.15
        self.gridCarbonIntensityGramsPerKwh = defaults.object(forKey: Self.gridCarbonIntensityGramsPerKwhKey) as? Double ?? 480.0
        self.energyCurrencySymbol = defaults.string(forKey: Self.energyCurrencySymbolKey) ?? "$"
        self.appearanceMode = defaults.string(forKey: Self.appearanceModeKey) ?? "system"
        self.operatorSafeModeEnabled = defaults.object(forKey: Self.operatorSafeModeEnabledKey) as? Bool ?? true
        self.monitorMetricCardPlacement = MonitorMetricCardPlacement(
            rawValue: defaults.string(forKey: Self.monitorMetricCardPlacementKey) ?? ""
        ) ?? .top
        self.monitorMetricCardFocus = MonitorMetricCardFocus(
            rawValue: defaults.string(forKey: Self.monitorMetricCardFocusKey) ?? ""
        ) ?? .all
        self.metricRingsFixedScaling = defaults.object(forKey: Self.metricRingsFixedScalingKey) as? Bool ?? false
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
    private static let chau7AgentCommandKey = "settings.chau7AgentCommand"
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
    private static let notifyThermalKey = "settings.notifyThermal"
    private static let notifyRegressionKey = "settings.notifyRegression"
    private static let notifyRestartLoopKey = "settings.notifyRestartLoop"
    private static let notifyNetworkKey = "settings.notifyNetwork"
    private static let notifyAgentBudgetKey = "settings.notifyAgentBudget"
    private static let electricityPricePerKwhKey = "settings.electricityPricePerKwh"
    private static let gridCarbonIntensityGramsPerKwhKey = "settings.gridCarbonIntensityGramsPerKwh"
    private static let energyCurrencySymbolKey = "settings.energyCurrencySymbol"
    private static let appearanceModeKey = "settings.appearanceMode"
    private static let operatorSafeModeEnabledKey = "settings.operatorSafeModeEnabled"
    private static let monitorMetricCardPlacementKey = "settings.monitorMetricCardPlacement"
    private static let monitorMetricCardFocusKey = "settings.monitorMetricCardFocus"
    private static let metricRingsFixedScalingKey = "settings.metricRingsFixedScaling"
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

    public nonisolated static func normalizedDockerSocketPath(_ value: String) -> String {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? defaultDockerSocketPath : trimmed
    }

    public nonisolated static func normalizedTelemetryEndpoint(_ value: String) -> String {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? defaultTelemetryEndpoint : trimmed
    }

    public nonisolated static func normalizedChau7AgentCommand(_ value: String) -> String {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? defaultChau7AgentCommand : trimmed
    }

    public nonisolated static func normalizedTelemetryExportIntervalSeconds(_ value: Double) -> UInt32 {
        let seconds = finiteOrDefault(
            value,
            fallback: minimumTelemetryExportIntervalSeconds,
            minimum: minimumTelemetryExportIntervalSeconds
        )
        let bounded = min(Double(UInt32.max), seconds.rounded())
        return UInt32(bounded)
    }

    public nonisolated static func milliseconds(from seconds: Double, minimumSeconds: Double) -> UInt64 {
        let safeMinimumSeconds = finiteOrDefault(
            minimumSeconds,
            fallback: minimumEngineTickSeconds,
            minimum: 0
        )
        let minimumMillis = max(0, (safeMinimumSeconds * 1000).rounded())
        let safeSeconds = finiteOrDefault(seconds, fallback: safeMinimumSeconds, minimum: safeMinimumSeconds)
        let millis = (safeSeconds * 1000).rounded()
        let boundedMillis = min(9_000_000_000_000_000_000, max(minimumMillis, millis))
        return UInt64(boundedMillis)
    }

    private nonisolated static func finiteOrDefault(
        _ value: Double,
        fallback: Double,
        minimum: Double
    ) -> Double {
        let safeFallback = fallback.isFinite ? fallback : minimum
        guard value.isFinite else {
            return max(minimum, safeFallback)
        }
        return max(minimum, value)
    }

    private func normalizeLoadedValues() {
        refreshIntervalSeconds = Self.finiteOrDefault(refreshIntervalSeconds, fallback: 2.0, minimum: 1.0)
        telemetryExportIntervalSeconds = Self.finiteOrDefault(
            telemetryExportIntervalSeconds,
            fallback: 30.0,
            minimum: Self.minimumTelemetryExportIntervalSeconds
        )
        engineActiveIntervalSeconds = Self.finiteOrDefault(
            engineActiveIntervalSeconds,
            fallback: 2.0,
            minimum: Self.minimumEngineTickSeconds
        )
        engineIdleIntervalSeconds = Self.finiteOrDefault(
            engineIdleIntervalSeconds,
            fallback: 5.0,
            minimum: Self.minimumEngineTickSeconds
        )
        engineLowPowerIntervalSeconds = Self.finiteOrDefault(
            engineLowPowerIntervalSeconds,
            fallback: 8.0,
            minimum: Self.minimumEngineTickSeconds
        )
        gpuSampleIntervalSeconds = Self.finiteOrDefault(
            gpuSampleIntervalSeconds,
            fallback: 30.0,
            minimum: Self.minimumGPUSampleIntervalSeconds
        )
        gpuSampleLowPowerIntervalSeconds = Self.finiteOrDefault(
            gpuSampleLowPowerIntervalSeconds,
            fallback: 60.0,
            minimum: Self.minimumGPUSampleIntervalSeconds
        )
        chau7AgentCommand = Self.normalizedChau7AgentCommand(chau7AgentCommand)
        frictionNotificationThreshold = Self.finiteOrDefault(
            frictionNotificationThreshold,
            fallback: 60.0,
            minimum: 0
        )
        electricityPricePerKwh = Self.finiteOrDefault(electricityPricePerKwh, fallback: 0.15, minimum: 0)
        gridCarbonIntensityGramsPerKwh = Self.finiteOrDefault(
            gridCarbonIntensityGramsPerKwh,
            fallback: 480.0,
            minimum: 0
        )
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
        chau7AgentCommand = Self.defaultChau7AgentCommand
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
        notifyThermal = true
        notifyRegression = true
        notifyRestartLoop = true
        notifyNetwork = false
        notifyAgentBudget = true
        electricityPricePerKwh = 0.15
        gridCarbonIntensityGramsPerKwh = 480.0
        energyCurrencySymbol = "$"
        appearanceMode = "system"
        operatorSafeModeEnabled = true
        monitorMetricCardPlacement = .top
        monitorMetricCardFocus = .all
        metricRingsFixedScaling = false
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
        defaults.set(chau7AgentCommand, forKey: Self.chau7AgentCommandKey)
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
        defaults.set(notifyThermal, forKey: Self.notifyThermalKey)
        defaults.set(notifyRegression, forKey: Self.notifyRegressionKey)
        defaults.set(notifyRestartLoop, forKey: Self.notifyRestartLoopKey)
        defaults.set(notifyNetwork, forKey: Self.notifyNetworkKey)
        defaults.set(notifyAgentBudget, forKey: Self.notifyAgentBudgetKey)
        defaults.set(electricityPricePerKwh, forKey: Self.electricityPricePerKwhKey)
        defaults.set(gridCarbonIntensityGramsPerKwh, forKey: Self.gridCarbonIntensityGramsPerKwhKey)
        defaults.set(energyCurrencySymbol, forKey: Self.energyCurrencySymbolKey)
        defaults.set(appearanceMode, forKey: Self.appearanceModeKey)
        defaults.set(operatorSafeModeEnabled, forKey: Self.operatorSafeModeEnabledKey)
        defaults.set(monitorMetricCardPlacement.rawValue, forKey: Self.monitorMetricCardPlacementKey)
        defaults.set(monitorMetricCardFocus.rawValue, forKey: Self.monitorMetricCardFocusKey)
        defaults.set(metricRingsFixedScaling, forKey: Self.metricRingsFixedScalingKey)
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
