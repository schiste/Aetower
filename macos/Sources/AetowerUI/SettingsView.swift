import Observation
import SwiftUI
import AetowerBridge

public struct SettingsView: View {
    let state: AppState
    let settings: SettingsStore

    public init(state: AppState, settings: SettingsStore) {
        self.state = state
        self.settings = settings
    }

    public var body: some View {
        @Bindable var settings = settings
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                GroupBox("Behavior") {
                    VStack(alignment: .leading, spacing: 14) {
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Appearance")
                                .font(.headline)
                            Picker("Appearance", selection: $settings.appearanceMode) {
                                Text("System").tag("system")
                                Text("Light").tag("light")
                                Text("Dark").tag("dark")
                            }
                            .pickerStyle(.segmented)
                        }

                        Toggle("Show menu bar extra", isOn: $settings.showMenuBarExtra)

                        VStack(alignment: .leading, spacing: 8) {
                            Text("Refresh interval")
                                .font(.headline)
                            Picker("Refresh interval", selection: $settings.refreshIntervalSeconds) {
                                Text("1.0s").tag(1.0)
                                Text("2.0s").tag(2.0)
                                Text("5.0s").tag(5.0)
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

                GroupBox("Notifications") {
                    VStack(alignment: .leading, spacing: 12) {
                        Toggle("Enable notifications", isOn: $settings.notificationsEnabled)
                        Text("Authorization: \(state.notificationAuthorizationStatus)")
                            .font(.caption.monospaced())
                            .foregroundStyle(.secondary)
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Friction notification threshold")
                                .font(.headline)
                            HStack {
                                Slider(value: $settings.frictionNotificationThreshold, in: 10...100, step: 5)
                                Text(String(format: "%.0f", settings.frictionNotificationThreshold))
                                    .font(.caption.monospacedDigit())
                                    .frame(width: 30, alignment: .trailing)
                            }
                            Text("Notify when an app's friction score exceeds this threshold.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Button("Re-check notification permission") {
                            state.applyNotificationSettings(settings)
                        }
                        .buttonStyle(.bordered)
                    }
                }

                GroupBox("Privacy") {
                    VStack(alignment: .leading, spacing: 12) {
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Export privacy tier")
                                .font(.headline)
                            Picker("Export privacy tier", selection: $settings.exportPrivacyTier) {
                                Text("Redacted").tag(ExportPrivacyTier.redacted)
                                Text("Operator").tag(ExportPrivacyTier.operatorMode)
                                Text("Full").tag(ExportPrivacyTier.full)
                            }
                            .pickerStyle(.segmented)
                        }
                        Text("Redacted strips sensitive titles, paths, URLs, and commands. Operator keeps structural context like executable basenames and hostnames while still hiding secrets. Full exports everything and is intended only for explicit troubleshooting.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }

                GroupBox("Integrations") {
                    VStack(alignment: .leading, spacing: 12) {
                        TextField("Chromium endpoint", text: $settings.chromiumEndpoint)
                            .textFieldStyle(.roundedBorder)
                            .aetowerUtilityTextInput()
                        TextField("Docker socket path", text: $settings.dockerSocketPath)
                            .textFieldStyle(.roundedBorder)
                            .aetowerUtilityTextInput()
                        Toggle("Enable privileged helper", isOn: $settings.privilegedHelperEnabled)
                        TextField("Privileged helper path", text: $settings.privilegedHelperPath)
                            .textFieldStyle(.roundedBorder)
                            .aetowerUtilityTextInput()
                        Text("The privileged helper is optional. It is intended to run with elevated rights when you want deeper socket attribution today and higher-confidence Endpoint Security lineage in enterprise builds.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        TextField("Chau7 socket path", text: $settings.chau7Endpoint, prompt: Text("~/.chau7/mcp.sock"))
                            .textFieldStyle(.roundedBorder)
                            .aetowerUtilityTextInput()
                        Text("Optional. Auto-detected when Chau7 is running. Enriches terminal sessions with AI agent context.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Divider()
                        Toggle("Enable OTLP telemetry export", isOn: $settings.telemetryEnabled)
                        TextField("Telemetry endpoint", text: $settings.telemetryEndpoint)
                            .textFieldStyle(.roundedBorder)
                            .aetowerUtilityTextInput()
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Telemetry export interval")
                                .font(.headline)
                            HStack {
                                Slider(value: $settings.telemetryExportIntervalSeconds, in: 5...120, step: 5)
                                Text(String(format: "%.0fs", settings.telemetryExportIntervalSeconds))
                                    .font(.caption.monospacedDigit())
                                    .frame(width: 44, alignment: .trailing)
                            }
                            Text("Exports host and entity gauges to an OTLP/HTTP collector. Useful when you want to correlate Aetower with the rest of your observability stack.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Button("Apply integration settings") {
                            state.applyIntegrationSettings(settings)
                        }
                        .buttonStyle(.borderedProminent)

                        Button("Verify telemetry export") {
                            state.verifyTelemetryExport(settings)
                        }
                        .buttonStyle(.bordered)

                        if let telemetryVerificationStatus = state.telemetryVerificationStatus {
                            Text(telemetryVerificationStatus)
                                .font(.caption)
                                .foregroundStyle(telemetryVerificationStatus.contains("failed") ? .orange : .secondary)
                        }
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
                                HStack(spacing: 6) {
                                    Text(String(describing: capability.state))
                                        .font(.caption.monospaced())
                                        .padding(.horizontal, 8)
                                        .padding(.vertical, 4)
                                        .background(Color.secondary.opacity(0.1), in: Capsule())
                                    Text(capabilityHealthLabel(capability.health))
                                        .font(.caption.weight(.semibold))
                                        .foregroundStyle(capabilityHealthColor(capability.health))
                                        .padding(.horizontal, 8)
                                        .padding(.vertical, 4)
                                        .background(
                                            capabilityHealthColor(capability.health).opacity(0.14),
                                            in: Capsule()
                                        )
                                }
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

private func capabilityHealthLabel(_ health: CapabilityHealth) -> String {
    switch health {
    case .configured:
        return "Configured"
    case .live:
        return "Live"
    case .cached:
        return "Cached"
    case .degraded:
        return "Degraded"
    }
}

private func capabilityHealthColor(_ health: CapabilityHealth) -> Color {
    switch health {
    case .configured:
        return .secondary
    case .live:
        return .green
    case .cached:
        return .orange
    case .degraded:
        return .red
    }
}
