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
    @Published public private(set) var launchAtLoginEnabled: Bool
    @Published public private(set) var launchAtLoginError: String?

    private let defaults: UserDefaults

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        self.showMenuBarExtra = defaults.object(forKey: Self.showMenuBarExtraKey) as? Bool ?? true
        self.refreshIntervalSeconds = defaults.object(forKey: Self.refreshIntervalKey) as? Double ?? 1.0
        self.chromiumEndpoint = defaults.string(forKey: Self.chromiumEndpointKey) ?? ""
        self.dockerSocketPath = defaults.string(forKey: Self.dockerSocketPathKey) ?? "/var/run/docker.sock"
        self.privilegedHelperPath = defaults.string(forKey: Self.privilegedHelperPathKey)
            ?? Bundle.main.bundleURL.appendingPathComponent("Contents/Helpers/aetower-helper").path
        self.privilegedHelperEnabled = defaults.object(forKey: Self.privilegedHelperEnabledKey) as? Bool ?? false
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
}

extension SettingsStore {
    func persist() {
        defaults.set(showMenuBarExtra, forKey: Self.showMenuBarExtraKey)
        defaults.set(refreshIntervalSeconds, forKey: Self.refreshIntervalKey)
        defaults.set(chromiumEndpoint, forKey: Self.chromiumEndpointKey)
        defaults.set(dockerSocketPath, forKey: Self.dockerSocketPathKey)
        defaults.set(privilegedHelperPath, forKey: Self.privilegedHelperPathKey)
        defaults.set(privilegedHelperEnabled, forKey: Self.privilegedHelperEnabledKey)
    }
}
