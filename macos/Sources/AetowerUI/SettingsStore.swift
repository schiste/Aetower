import Combine
import Foundation
import ServiceManagement

@MainActor
public final class SettingsStore: ObservableObject {
    @Published public var showMenuBarExtra: Bool {
        didSet { persist() }
    }
    @Published public var refreshIntervalSeconds: Double {
        didSet { persist() }
    }
    @Published public var chromiumEndpoint: String {
        didSet { persist() }
    }
    @Published public var dockerSocketPath: String {
        didSet { persist() }
    }
    @Published public var privilegedHelperPath: String {
        didSet { persist() }
    }
    @Published public var privilegedHelperEnabled: Bool {
        didSet { persist() }
    }
    @Published public var chau7Endpoint: String {
        didSet { persist() }
    }
    @Published public var telemetryEnabled: Bool {
        didSet { persist() }
    }
    @Published public var telemetryEndpoint: String {
        didSet { persist() }
    }
    @Published public var telemetryExportIntervalSeconds: Double {
        didSet { persist() }
    }
    @Published public var notificationsEnabled: Bool {
        didSet { persist() }
    }
    @Published public var frictionNotificationThreshold: Double {
        didSet { persist() }
    }
    @Published public var appearanceMode: String {
        didSet { persist() }
    }
    @Published public private(set) var launchAtLoginEnabled: Bool
    @Published public private(set) var launchAtLoginError: String?

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
        self.notificationsEnabled = defaults.object(forKey: Self.notificationsEnabledKey) as? Bool ?? false
        self.frictionNotificationThreshold = defaults.object(forKey: Self.frictionNotificationThresholdKey) as? Double ?? 60.0
        self.appearanceMode = defaults.string(forKey: Self.appearanceModeKey) ?? "system"
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
    private static let notificationsEnabledKey = "settings.notificationsEnabled"
    private static let frictionNotificationThresholdKey = "settings.frictionNotificationThreshold"
    private static let appearanceModeKey = "settings.appearanceMode"
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
        defaults.set(notificationsEnabled, forKey: Self.notificationsEnabledKey)
        defaults.set(frictionNotificationThreshold, forKey: Self.frictionNotificationThresholdKey)
        defaults.set(appearanceMode, forKey: Self.appearanceModeKey)
    }
}
