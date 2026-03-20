import SwiftUI
import AetowerBridge

public struct SettingsView: View {
    @ObservedObject private var state: AppState
    @ObservedObject private var settings: SettingsStore

    public init(state: AppState, settings: SettingsStore) {
        self.state = state
        self.settings = settings
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                GroupBox("Behavior") {
                    VStack(alignment: .leading, spacing: 14) {
                        Toggle("Show menu bar extra", isOn: $settings.showMenuBarExtra)

                        VStack(alignment: .leading, spacing: 8) {
                            Text("Refresh interval")
                                .font(.headline)
                            Picker("Refresh interval", selection: $settings.refreshIntervalSeconds) {
                                Text("0.5s").tag(0.5)
                                Text("1.0s").tag(1.0)
                                Text("2.0s").tag(2.0)
                            }
                            .pickerStyle(.segmented)
                        }

                        Toggle(
                            "Launch at login",
                            isOn: Binding(
                                get: { settings.launchAtLoginEnabled },
                                set: { settings.setLaunchAtLogin($0) }
                            )
                        )

                        if let error = settings.launchAtLoginError {
                            Text(error)
                                .font(.caption)
                                .foregroundStyle(.orange)
                        }
                    }
                }

                GroupBox("Integrations") {
                    VStack(alignment: .leading, spacing: 12) {
                        TextField("Chromium endpoint", text: $settings.chromiumEndpoint)
                            .textFieldStyle(.roundedBorder)
                        TextField("Docker socket path", text: $settings.dockerSocketPath)
                            .textFieldStyle(.roundedBorder)
                        Toggle("Enable privileged helper", isOn: $settings.privilegedHelperEnabled)
                        TextField("Privileged helper path", text: $settings.privilegedHelperPath)
                            .textFieldStyle(.roundedBorder)
                        Text("The privileged helper is optional. It is intended to run with elevated rights when you want deeper socket attribution.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Button("Apply integration settings") {
                            state.applyIntegrationSettings(settings)
                        }
                        .buttonStyle(.borderedProminent)
                    }
                }

                Text("Capabilities")
                    .font(.largeTitle.weight(.semibold))
                Text("Aetower keeps core monitoring useful without invasive access, and exposes richer integrations behind explicit capability gates.")
                    .foregroundStyle(.secondary)

                ForEach(Array(state.snapshot.capabilities.enumerated()), id: \.offset) { _, capability in
                    GroupBox {
                        VStack(alignment: .leading, spacing: 10) {
                            HStack {
                                Text(String(describing: capability.kind))
                                    .font(.headline)
                                Spacer()
                                Text(String(describing: capability.state))
                                    .font(.caption.monospaced())
                                    .padding(.horizontal, 8)
                                    .padding(.vertical, 4)
                                    .background(Color.secondary.opacity(0.1), in: Capsule())
                            }
                            Text(capability.detail)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            HStack {
                                Button("Request / Refresh") {
                                    state.requestCapability(capability)
                                }
                                .buttonStyle(.borderedProminent)

                                Text(Date(timeIntervalSince1970: TimeInterval(capability.lastUpdatedMillis) / 1000), style: .time)
                                    .font(.caption2)
                                    .foregroundStyle(.tertiary)
                            }
                        }
                    }
                }
            }
            .padding(24)
        }
        .navigationTitle("Settings")
    }
}
