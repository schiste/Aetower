import Observation
import SwiftUI
import AetowerBridge

public struct SettingsView: View {
    let state: AppState
    let settings: SettingsStore
    @FocusState private var focusedField: SettingsField?

    public init(state: AppState, settings: SettingsStore) {
        self.state = state
        self.settings = settings
    }

    private enum SettingsField: Hashable {
        case chromiumEndpoint
        case dockerSocketPath
        case privilegedHelperPath
        case chau7Endpoint
        case telemetryEndpoint
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

                        Divider()

                        VStack(alignment: .leading, spacing: 8) {
                            Text("Collection profile")
                                .font(.headline)
                            Picker("Collection profile", selection: $settings.collectionProfile) {
                                Text("Balanced").tag(CollectionProfile.balanced)
                                Text("Full").tag(CollectionProfile.full)
                            }
                            .pickerStyle(.segmented)
                            Text("Balanced keeps CPU and battery lower by sampling expensive per-process signals like wakeups, usernames, and GPU counters more sparsely while keeping CPU, memory, disk, network, parentage, and provenance live. Full refreshes the expensive signals every engine tick and is intended for short diagnostic sessions.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }

                        Toggle("Adaptive engine cadence", isOn: $settings.adaptiveCadenceEnabled)
                        Text("When enabled, Aetower stays at the active cadence during hotspots and slows down when the machine is quiet or on battery. The tradeoff is extra detection latency during calm periods, up to the idle or low-power interval below.")
                            .font(.caption)
                            .foregroundStyle(.secondary)

                        VStack(alignment: .leading, spacing: 8) {
                            Text("Engine active interval")
                                .font(.headline)
                            HStack {
                                Slider(value: $settings.engineActiveIntervalSeconds, in: 1...5, step: 0.5)
                                Text(String(format: "%.1fs", settings.engineActiveIntervalSeconds))
                                    .font(.caption.monospacedDigit())
                                    .frame(width: 44, alignment: .trailing)
                            }
                        }

                        VStack(alignment: .leading, spacing: 8) {
                            Text("Engine idle interval")
                                .font(.headline)
                            HStack {
                                Slider(value: $settings.engineIdleIntervalSeconds, in: 2...30, step: 1)
                                Text(String(format: "%.0fs", settings.engineIdleIntervalSeconds))
                                    .font(.caption.monospacedDigit())
                                    .frame(width: 44, alignment: .trailing)
                            }
                            Text("Used when adaptive cadence is on and the machine is quiet. Larger values reduce wakeups and battery drain but make anomaly detection and timeline changes arrive later.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }

                        VStack(alignment: .leading, spacing: 8) {
                            Text("Engine low-power interval")
                                .font(.headline)
                            HStack {
                                Slider(value: $settings.engineLowPowerIntervalSeconds, in: 3...45, step: 1)
                                Text(String(format: "%.0fs", settings.engineLowPowerIntervalSeconds))
                                    .font(.caption.monospacedDigit())
                                    .frame(width: 44, alignment: .trailing)
                            }
                            Text("Used on battery or Low Power Mode. This is the strongest direct battery lever, but it also increases the delay before Aetower notices new hotspots.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }

                        VStack(alignment: .leading, spacing: 8) {
                            Text("GPU sample interval")
                                .font(.headline)
                            HStack {
                                Slider(value: $settings.gpuSampleIntervalSeconds, in: 5...120, step: 5)
                                Text(String(format: "%.0fs", settings.gpuSampleIntervalSeconds))
                                    .font(.caption.monospacedDigit())
                                    .frame(width: 44, alignment: .trailing)
                            }
                        }

                        VStack(alignment: .leading, spacing: 8) {
                            Text("GPU sample interval on battery")
                                .font(.headline)
                            HStack {
                                Slider(value: $settings.gpuSampleLowPowerIntervalSeconds, in: 10...180, step: 5)
                                Text(String(format: "%.0fs", settings.gpuSampleLowPowerIntervalSeconds))
                                    .font(.caption.monospacedDigit())
                                    .frame(width: 44, alignment: .trailing)
                            }
                            Text("Longer GPU intervals reduce system-service activity and save power. GPU/ANE values stay available, but they become colder and may lag behind the most recent burst.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
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
                            .focused($focusedField, equals: .chromiumEndpoint)
                        TextField("Docker socket path", text: $settings.dockerSocketPath)
                            .textFieldStyle(.roundedBorder)
                            .aetowerUtilityTextInput()
                            .focused($focusedField, equals: .dockerSocketPath)
                        Toggle("Enable privileged helper", isOn: $settings.privilegedHelperEnabled)
                        TextField("Privileged helper path", text: $settings.privilegedHelperPath)
                            .textFieldStyle(.roundedBorder)
                            .aetowerUtilityTextInput()
                            .focused($focusedField, equals: .privilegedHelperPath)
                        Text("The privileged helper is optional. It is intended to run with elevated rights when you want deeper socket attribution today and higher-confidence Endpoint Security lineage in enterprise builds.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        TextField("Chau7 socket path", text: $settings.chau7Endpoint, prompt: Text("~/.chau7/mcp.sock"))
                            .textFieldStyle(.roundedBorder)
                            .aetowerUtilityTextInput()
                            .focused($focusedField, equals: .chau7Endpoint)
                        Text("Optional. Auto-detected when Chau7 is running. Enriches terminal sessions with AI agent context.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Divider()
                        Toggle("Enable OTLP telemetry export", isOn: $settings.telemetryEnabled)
                        TextField("Telemetry endpoint", text: $settings.telemetryEndpoint)
                            .textFieldStyle(.roundedBorder)
                            .aetowerUtilityTextInput()
                            .focused($focusedField, equals: .telemetryEndpoint)
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
                        Button("Apply runtime and integration settings") {
                            focusedField = nil
                            state.applyIntegrationSettings(settings)
                        }
                        .buttonStyle(.borderedProminent)

                        Button("Verify telemetry export") {
                            focusedField = nil
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
        .onDisappear {
            focusedField = nil
        }
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
