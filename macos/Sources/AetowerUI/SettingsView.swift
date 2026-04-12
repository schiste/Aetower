import Observation
import SwiftUI
import AetowerBridge

public struct SettingsView: View {
    let state: AppState
    let settings: SettingsStore
    @FocusState private var focusedField: SettingsField?
    @State private var applyConfirmation: String?

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
                            applyConfirmation = "Settings applied to the running engine."
                            // Clear the confirmation after 4 seconds so the
                            // message doesn't persist indefinitely.
                            Task {
                                try? await Task.sleep(nanoseconds: 4_000_000_000)
                                applyConfirmation = nil
                            }
                        }
                        .buttonStyle(.borderedProminent)

                        if let applyConfirmation {
                            Text(applyConfirmation)
                                .font(.caption)
                                .foregroundStyle(AetowerDesign.Status.success)
                        }

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

                GroupBox("AI Clients") {
                    VStack(alignment: .leading, spacing: 12) {
                        Text("Aetower can register its bundled MCP proxy for supported local AI clients. Automatic registration only runs when the client exposes a stable user-owned config file.")
                            .font(.caption)
                            .foregroundStyle(.secondary)

                        HStack(spacing: 10) {
                            Button("Register supported clients") {
                                focusedField = nil
                                state.registerSupportedLocalMcpClients()
                            }
                            .buttonStyle(.borderedProminent)

                            Button("Refresh client status") {
                                state.refreshLocalMcpClientStatuses()
                            }
                            .buttonStyle(.bordered)
                        }

                        if let message = state.localMcpRegistrationStatusMessage {
                            Text(message)
                                .font(.caption)
                                .foregroundStyle(message.contains("Registered")
                                    ? AetowerDesign.Status.neutral
                                    : AetowerDesign.Status.warning)
                        }

                        ForEach(state.localMcpClientStatuses) { status in
                            VStack(alignment: .leading, spacing: 6) {
                                HStack(alignment: .firstTextBaseline, spacing: 8) {
                                    Text(status.displayName)
                                        .font(.headline)
                                    Text(registrationLabel(status.state))
                                        .font(.caption.weight(.semibold))
                                        .foregroundStyle(registrationColor(status.state))
                                }

                                Text(status.detail)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)

                                if let configPath = status.configPath {
                                    Text(configPath)
                                        .font(.caption.monospaced())
                                        .foregroundStyle(.tertiary)
                                        .textSelection(.enabled)
                                }

                                HStack(spacing: 10) {
                                    if status.supportsAutomaticRegistration,
                                       status.isInstalled,
                                       status.state != .registered {
                                        Button("Register") {
                                            focusedField = nil
                                            state.registerSupportedLocalMcpClients()
                                        }
                                        .buttonStyle(.bordered)
                                    }

                                    if status.manualSnippet != nil {
                                        Button("Copy MCP snippet") {
                                            focusedField = nil
                                            state.copyLocalMcpConfigSnippet(providerId: status.id)
                                        }
                                        .buttonStyle(.bordered)
                                    }
                                }
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.vertical, 2)
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
                                Text(capabilityKindLabel(capability.kind))
                                    .font(.headline)
                                Spacer()
                                HStack(spacing: 6) {
                                    Text(capabilityStateLabel(capability))
                                        .font(.caption.weight(.semibold))
                                        .foregroundStyle(capabilityStateColor(capability))
                                        .padding(.horizontal, 8)
                                        .padding(.vertical, 4)
                                        .background(capabilityStateColor(capability).opacity(0.14), in: Capsule())
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
                            Text(capabilityConsequenceText(capability))
                                .font(.caption)
                                .foregroundStyle(.primary)
                                .fixedSize(horizontal: false, vertical: true)
                            Text(capabilityRemediationText(capability))
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                                .fixedSize(horizontal: false, vertical: true)
                            HStack {
                                Button(capabilityActionLabel(capability)) {
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

private func registrationLabel(_ state: LocalMcpClientRegistrationState) -> String {
    switch state {
    case .registered:
        return "Registered"
    case .availableForAutomaticRegistration:
        return "Automatic"
    case .manualConfigurationRequired:
        return "Manual"
    case .unavailable:
        return "Unavailable"
    case .notInstalled:
        return "Not installed"
    }
}

private func registrationColor(_ state: LocalMcpClientRegistrationState) -> Color {
    switch state {
    case .registered:
        return AetowerDesign.Status.success
    case .availableForAutomaticRegistration:
        return AetowerDesign.Status.ready
    case .manualConfigurationRequired:
        return AetowerDesign.Status.warning
    case .unavailable:
        return AetowerDesign.Status.error
    case .notInstalled:
        return AetowerDesign.Status.neutral
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

private func capabilityKindLabel(_ kind: CapabilityKind) -> String {
    switch kind {
    case .accessibility:
        return "Accessibility"
    case .fullDiskAccess:
        return "Full Disk Access"
    case .appleAutomation:
        return "Apple Automation"
    case .chromiumDebug:
        return "Chromium Debug"
    case .dockerSocket:
        return "Docker Socket"
    case .privilegedHelper:
        return "Privileged Helper"
    case .chau7:
        return "Chau7"
    case .endpointSecurity:
        return "Endpoint Security"
    }
}

private func capabilityStateLabel(_ capability: CapabilitySnapshot) -> String {
    switch capability.state {
    case .granted:
        return "Granted"
    case .denied:
        return "Denied"
    case .requested:
        return "Pending"
    case .unknown:
        return "Not checked"
    case .unavailable:
        let detail = capability.detail.localizedLowercase
        if detail.contains("disabled") || detail.contains("configure") || detail.contains("set ") {
            return "Not configured"
        }
        if detail.contains("missing") || detail.contains("not found") || detail.contains("not detected") {
            return "Missing"
        }
        if detail.contains("requires") || detail.contains("unavailable on this system") {
            return "Not available"
        }
        return "Unavailable"
    }
}

private func capabilityStateColor(_ capability: CapabilitySnapshot) -> Color {
    switch capability.state {
    case .granted:
        return AetowerDesign.Status.success
    case .denied:
        return AetowerDesign.Status.error
    case .requested:
        return AetowerDesign.Status.ready
    case .unknown:
        return AetowerDesign.Status.neutral
    case .unavailable:
        return capabilityStateLabel(capability) == "Not configured"
            ? AetowerDesign.Status.warning
            : AetowerDesign.Status.neutral
    }
}

private func capabilityActionLabel(_ capability: CapabilitySnapshot) -> String {
    switch capability.kind {
    case .accessibility, .fullDiskAccess, .appleAutomation:
        return "Request Access"
    default:
        return "Refresh"
    }
}
