import Observation
import SwiftUI
import AetowerBridge

public struct SettingsView: View {
    let state: AppState
    let settings: SettingsStore
    @Environment(UpdaterController.self) private var updater
    @FocusState private var focusedField: SettingsField?
    @State private var selectedSection: SettingsSection = .general
    @State private var appliedIntegrationSnapshot: SettingsIntegrationSnapshot?
    @State private var applyConfirmation: String?
    @State private var showAdvancedCollectionControls = false

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

    private enum SettingsSection: String, CaseIterable, Identifiable {
        case general
        case collection
        case integrations
        case aiClients
        case notifications
        case privacy
        case updates
        case advanced

        var id: Self { self }

        var title: String {
            switch self {
            case .general: return "General"
            case .collection: return "Collection"
            case .integrations: return "Integrations"
            case .aiClients: return "AI Clients"
            case .notifications: return "Notifications"
            case .privacy: return "Privacy"
            case .updates: return "Updates"
            case .advanced: return "Advanced"
            }
        }

        var subtitle: String {
            switch self {
            case .general: return "App behavior and launch"
            case .collection: return "Runtime sampling policy"
            case .integrations: return "External data sources"
            case .aiClients: return "MCP registration"
            case .notifications: return "Operator alerts"
            case .privacy: return "Export controls"
            case .updates: return "Direct-download releases"
            case .advanced: return "Capabilities and reset"
            }
        }

        var systemImage: String {
            switch self {
            case .general: return "slider.horizontal.2.square"
            case .collection: return "gauge.with.needle"
            case .integrations: return "point.3.connected.trianglepath.dotted"
            case .aiClients: return "cpu"
            case .notifications: return "bell.badge"
            case .privacy: return "hand.raised"
            case .updates: return "sparkles"
            case .advanced: return "wrench.and.screwdriver"
            }
        }
    }

    public var body: some View {
        @Bindable var settings = settings
        HStack(spacing: 0) {
            settingsSidebar
                .frame(width: 238)
            Divider()
            ScrollView {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
                    sectionHeader(selectedSection)
                    selectedSectionContent
                }
                .frame(maxWidth: 860, alignment: .leading)
                .padding(AetowerDesign.Spacing.xxl)
            }
        }
        .navigationTitle("Settings")
        .onAppear {
            if appliedIntegrationSnapshot == nil {
                appliedIntegrationSnapshot = SettingsIntegrationSnapshot(settings)
            }
        }
        .onDisappear {
            focusedField = nil
        }
    }

    private var settingsSidebar: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            Text("Settings")
                .font(.title2.weight(.semibold))
                .padding(.bottom, AetowerDesign.Spacing.md)

            ForEach(SettingsSection.allCases) { section in
                Button {
                    selectedSection = section
                } label: {
                    HStack(spacing: AetowerDesign.Spacing.md) {
                        Image(systemName: section.systemImage)
                            .frame(width: 18)
                        VStack(alignment: .leading, spacing: 2) {
                            HStack(spacing: AetowerDesign.Spacing.xs) {
                                Text(section.title)
                                    .font(.callout.weight(.semibold))
                                Spacer(minLength: AetowerDesign.Spacing.xs)
                                let status = status(for: section)
                                SettingsMiniStatusBadge(status.label, color: status.color)
                            }
                            Text(section.subtitle)
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                    }
                    .foregroundStyle(selectedSection == section ? Color.primary : Color.secondary)
                    .padding(.horizontal, AetowerDesign.Spacing.md)
                    .padding(.vertical, AetowerDesign.Spacing.sm)
                    .background(
                        selectedSection == section
                            ? AetowerDesign.Surface.rowSelected
                            : Color.clear,
                        in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md)
                    )
                }
                .buttonStyle(.plain)
            }

            Spacer()
        }
        .padding(AetowerDesign.Spacing.xl)
        .background(
            LinearGradient(
                colors: [
                    AetowerDesign.Surface.card.opacity(0.65),
                    Color.clear,
                ],
                startPoint: .top,
                endPoint: .bottom
            )
        )
    }

    private func sectionHeader(_ section: SettingsSection) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: section.systemImage)
                    .foregroundStyle(.secondary)
                Text(section.title)
                    .font(.system(size: 30, weight: .semibold, design: .rounded))
                let status = status(for: section)
                SettingsBadge(status.label, color: status.color)
            }
            Text(section.subtitle)
                .foregroundStyle(.secondary)
        }
    }

    private func status(for section: SettingsSection) -> SettingsStatus {
        switch section {
        case .general:
            return SettingsStatus("Live", AetowerDesign.Status.success)
        case .collection:
            return SettingsStatus("Live", AetowerDesign.Status.success)
        case .integrations:
            return hasPendingIntegrationChanges
                ? SettingsStatus("Pending Apply", AetowerDesign.Status.warning)
                : SettingsStatus("Applied", AetowerDesign.Status.success)
        case .aiClients:
            return settings.autoRegisterLocalMcpClientsEnabled
                ? SettingsStatus("Auto", AetowerDesign.Status.ready)
                : SettingsStatus("Manual", AetowerDesign.Status.neutral)
        case .notifications:
            return settings.notificationsEnabled
                ? SettingsStatus("Enabled", AetowerDesign.Status.success)
                : SettingsStatus("Off", AetowerDesign.Status.neutral)
        case .privacy:
            return SettingsStatus(settings.exportPrivacyTier.rawValue.capitalized, AetowerDesign.Status.ready)
        case .updates:
            return updater.isConfigured
                ? SettingsStatus("Ready", AetowerDesign.Status.success)
                : SettingsStatus("Disabled", AetowerDesign.Status.neutral)
        case .advanced:
            return SettingsStatus("Advanced", AetowerDesign.Status.warning)
        }
    }

    private var hasPendingIntegrationChanges: Bool {
        guard let appliedIntegrationSnapshot else {
            return false
        }
        return appliedIntegrationSnapshot != SettingsIntegrationSnapshot(settings)
    }

    @ViewBuilder
    private var selectedSectionContent: some View {
        switch selectedSection {
        case .general:
            generalSection
        case .collection:
            collectionSection
        case .integrations:
            integrationsSection
        case .aiClients:
            aiClientsSection
        case .notifications:
            notificationsSection
        case .privacy:
            privacySection
        case .updates:
            updatesSection
        case .advanced:
            advancedSection
        }
    }

    @ViewBuilder
    private var generalSection: some View {
        @Bindable var settings = settings
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            SettingsCard(
                title: "Appearance",
                subtitle: "Controls the app chrome immediately.",
                status: status(for: .general)
            ) {
                Picker("Appearance", selection: $settings.appearanceMode) {
                    Text("System").tag("system")
                    Text("Light").tag("light")
                    Text("Dark").tag("dark")
                }
                .pickerStyle(.segmented)
            }

            SettingsCard(title: "Window and launch behavior", subtitle: "Small controls that change how Aetower shows up on macOS.") {
                Toggle("Show menu bar extra", isOn: $settings.showMenuBarExtra)
                Toggle("Operator-safe mode for History and Timeline", isOn: $settings.operatorSafeModeEnabled)
                Text("Heavy History and Timeline views start from summaries first, smaller visible windows, and manual expansion for large detail lists.")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                SettingDivider()

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    Text("UI refresh interval")
                        .font(.headline)
                    Picker("UI refresh interval", selection: $settings.refreshIntervalSeconds) {
                        Text("1.0s").tag(1.0)
                        Text("2.0s").tag(2.0)
                        Text("5.0s").tag(5.0)
                    }
                    .pickerStyle(.segmented)
                    Text("How often the macOS UI asks the engine for a newer snapshot. Engine collection cadence is controlled separately.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                SettingDivider()

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
                        .foregroundStyle(AetowerDesign.Status.warning)
                }
                Text("Uses macOS Login Items. If macOS requires approval, finish it in System Settings.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder
    private var collectionSection: some View {
        @Bindable var settings = settings
        SettingsCard(
            title: "Runtime collection",
            subtitle: "Choose an intent first. Detailed cadence tuning is available when needed.",
            status: status(for: .collection)
        ) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                Text("Collection presets")
                    .font(.headline)
                LazyVGrid(
                    columns: [GridItem(.adaptive(minimum: 238), spacing: AetowerDesign.Spacing.md)],
                    alignment: .leading,
                    spacing: AetowerDesign.Spacing.md
                ) {
                    ForEach(SettingsCollectionPreset.allCases) { preset in
                        Button {
                            preset.apply(to: settings)
                        } label: {
                            SettingsPresetCard(preset: preset)
                        }
                        .buttonStyle(.plain)
                    }
                }
                Text("Presets update the running engine through the existing live collection settings path.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            SettingDivider()

            DisclosureGroup("Advanced cadence controls", isExpanded: $showAdvancedCollectionControls) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                        Text("Collection mode")
                            .font(.headline)
                        Picker("Collection mode", selection: $settings.collectionProfile) {
                            Text("Balanced").tag(CollectionProfile.balanced)
                            Text("Full detail").tag(CollectionProfile.full)
                        }
                        .pickerStyle(.segmented)
                        Text("Balanced lowers CPU and battery cost by sampling expensive per-process signals less often. Full detail is intended for short diagnostic sessions.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }

                    Toggle("Adaptive engine cadence", isOn: $settings.adaptiveCadenceEnabled)
                    Text("When enabled, Aetower stays active during hotspots and slows down when the machine is quiet or on battery.")
                        .font(.caption)
                        .foregroundStyle(.secondary)

                    intervalSlider(
                        title: "Engine active interval",
                        value: $settings.engineActiveIntervalSeconds,
                        range: 1...5,
                        step: 0.5,
                        format: "%.1fs"
                    )
                    intervalSlider(
                        title: "Engine idle interval",
                        value: $settings.engineIdleIntervalSeconds,
                        range: 2...30,
                        step: 1,
                        format: "%.0fs",
                        note: "Used when adaptive cadence is on and the machine is quiet."
                    )
                    intervalSlider(
                        title: "Engine low-power interval",
                        value: $settings.engineLowPowerIntervalSeconds,
                        range: 3...45,
                        step: 1,
                        format: "%.0fs",
                        note: "Used on battery or Low Power Mode."
                    )
                    intervalSlider(
                        title: "GPU sample interval",
                        value: $settings.gpuSampleIntervalSeconds,
                        range: 5...120,
                        step: 5,
                        format: "%.0fs"
                    )
                    intervalSlider(
                        title: "GPU sample interval on battery",
                        value: $settings.gpuSampleLowPowerIntervalSeconds,
                        range: 10...180,
                        step: 5,
                        format: "%.0fs",
                        note: "Longer GPU intervals reduce system-service activity and save power."
                    )
                }
                .padding(.top, AetowerDesign.Spacing.md)
            }
        }
    }

    @ViewBuilder
    private var integrationsSection: some View {
        @Bindable var settings = settings
        SettingsCard(
            title: "Integration endpoints",
            subtitle: "Endpoint fields are staged until you apply them.",
            status: status(for: .integrations)
        ) {
            if hasPendingIntegrationChanges {
                SettingsNotice(
                    title: "Integration changes are pending",
                    detail: "Apply these staged endpoint and telemetry changes before trusting capability status.",
                    color: AetowerDesign.Status.warning
                )
            }

            TextField(
                "Chromium endpoint",
                text: $settings.chromiumEndpoint,
                prompt: Text("http://127.0.0.1:9222/json/list")
            )
            .textFieldStyle(.roundedBorder)
            .aetowerUtilityTextInput()
            .focused($focusedField, equals: .chromiumEndpoint)
            Text("Optional. Use a Chromium `/json` or `/json/list` endpoint when browser tab attribution is needed.")
                .font(.caption)
                .foregroundStyle(.secondary)

            TextField(
                "Docker socket path",
                text: $settings.dockerSocketPath,
                prompt: Text(SettingsStore.defaultDockerSocketPath)
            )
            .textFieldStyle(.roundedBorder)
            .aetowerUtilityTextInput()
            .focused($focusedField, equals: .dockerSocketPath)
            Text("Leave blank to use the default Docker socket path.")
                .font(.caption)
                .foregroundStyle(.secondary)

            Toggle("Enable advanced privileged helper", isOn: $settings.privilegedHelperEnabled)
            TextField("Advanced helper path", text: $settings.privilegedHelperPath)
                .textFieldStyle(.roundedBorder)
                .aetowerUtilityTextInput()
                .focused($focusedField, equals: .privilegedHelperPath)
            Text("Optional advanced/enterprise integration. Enable only when a signed helper is installed.")
                .font(.caption)
                .foregroundStyle(.secondary)

            TextField("Chau7 socket path", text: $settings.chau7Endpoint, prompt: Text("~/.chau7/mcp.sock"))
                .textFieldStyle(.roundedBorder)
                .aetowerUtilityTextInput()
                .focused($focusedField, equals: .chau7Endpoint)
            Text("Optional. Auto-detected when Chau7 is running. Enriches terminal sessions with AI agent context.")
                .font(.caption)
                .foregroundStyle(.secondary)

            SettingDivider()

            Toggle("Enable OTLP telemetry export", isOn: $settings.telemetryEnabled)
            TextField(
                "Telemetry endpoint",
                text: $settings.telemetryEndpoint,
                prompt: Text(SettingsStore.defaultTelemetryEndpoint)
            )
            .textFieldStyle(.roundedBorder)
            .aetowerUtilityTextInput()
            .focused($focusedField, equals: .telemetryEndpoint)

            intervalSlider(
                title: "Telemetry export interval",
                value: $settings.telemetryExportIntervalSeconds,
                range: 5...120,
                step: 5,
                format: "%.0fs",
                note: "Exports host and entity gauges to an OTLP/HTTP collector."
            )

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Button("Apply integration and telemetry settings") {
                    focusedField = nil
                    applyIntegrationAndTelemetrySettings(settings)
                }
                .buttonStyle(.borderedProminent)

                Button("Verify telemetry export") {
                    focusedField = nil
                    state.verifyTelemetryExport(settings)
                }
                .buttonStyle(.bordered)
            }

            if let applyConfirmation {
                Text(applyConfirmation)
                    .font(.caption)
                    .foregroundStyle(AetowerDesign.Status.success)
            }
            if let telemetryVerificationStatus = state.telemetryVerificationStatus {
                Text(telemetryVerificationStatus)
                    .font(.caption)
                    .foregroundStyle(telemetryVerificationStatus.contains("failed") ? .orange : .secondary)
            }
        }
    }

    @ViewBuilder
    private var aiClientsSection: some View {
        @Bindable var settings = settings
        SettingsCard(
            title: "Local AI client MCP access",
            subtitle: "Aetower can register its bundled MCP proxy for supported local agents.",
            status: status(for: .aiClients)
        ) {
            Toggle(
                "Auto-register supported AI clients on launch",
                isOn: $settings.autoRegisterLocalMcpClientsEnabled
            )
            Text("Off by default. Manual registration is safer for shared machines.")
                .font(.caption)
                .foregroundStyle(.secondary)

            HStack(spacing: AetowerDesign.Spacing.sm) {
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

            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                ForEach(state.localMcpClientStatuses) { status in
                    SettingsRowCard {
                        HStack(alignment: .firstTextBaseline, spacing: AetowerDesign.Spacing.sm) {
                            Text(status.displayName)
                                .font(.headline)
                            SettingsBadge(
                                registrationLabel(status.state),
                                color: registrationColor(status.state)
                            )
                            Spacer()
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

                        HStack(spacing: AetowerDesign.Spacing.sm) {
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
                }
            }
        }
    }

    @ViewBuilder
    private var notificationsSection: some View {
        @Bindable var settings = settings
        SettingsCard(
            title: "Friction alerts",
            subtitle: "Notify when a process group crosses your attention threshold.",
            status: status(for: .notifications)
        ) {
            Toggle("Enable notifications", isOn: $settings.notificationsEnabled)
            Text("Authorization: \(state.notificationAuthorizationStatus)")
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
            intervalSlider(
                title: "Friction notification threshold",
                value: $settings.frictionNotificationThreshold,
                range: 10...100,
                step: 5,
                format: "%.0f",
                valueWidth: 34,
                note: "Notify when an app's friction score exceeds this threshold."
            )
            Button("Re-check notification permission") {
                state.applyNotificationSettings(settings)
            }
            .buttonStyle(.bordered)
        }
    }

    @ViewBuilder
    private var privacySection: some View {
        @Bindable var settings = settings
        SettingsCard(
            title: "Export privacy",
            subtitle: "Controls what leaves the machine in JSON exports and support bundles.",
            status: status(for: .privacy)
        ) {
            Picker("Export privacy tier", selection: $settings.exportPrivacyTier) {
                Text("Redacted").tag(ExportPrivacyTier.redacted)
                Text("Operator").tag(ExportPrivacyTier.operatorMode)
                Text("Full").tag(ExportPrivacyTier.full)
            }
            .pickerStyle(.segmented)
            Text("Redacted strips sensitive titles, paths, URLs, and commands. Operator keeps structural context while still hiding secrets. Full exports everything and is intended only for explicit troubleshooting.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private var updatesSection: some View {
        SettingsCard(
            title: "Direct-download updates",
            subtitle: "Sparkle handles signed release checks outside the Mac App Store.",
            status: status(for: .updates)
        ) {
            Text(updater.statusMessage)
                .font(.caption)
                .foregroundStyle(.secondary)

            Button("Check for Updates") {
                updater.checkForUpdates()
            }
            .buttonStyle(.bordered)
            .disabled(!updater.isConfigured)

            Text("Local development builds stay disabled until the packaged app includes an appcast URL and Sparkle public key.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private var advancedSection: some View {
        @Bindable var settings = settings
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            SettingsCard(
                title: "Capabilities",
                subtitle: "Richer integrations stay behind explicit capability gates.",
                status: status(for: .advanced)
            ) {
                ForEach(state.snapshot.capabilities, id: \.kind) { capability in
                    SettingsRowCard {
                        HStack {
                            Text(capabilityKindDisplayName(capability.kind))
                                .font(.headline)
                            Spacer()
                            SettingsBadge(capabilityStateLabel(capability), color: capabilityStateColor(capability))
                            SettingsBadge(capabilityHealthLabel(capability.health), color: capabilityHealthColor(capability.health))
                        }
                        Text(capability.detail)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Text(capabilityConsequenceText(capability))
                            .font(.caption)
                        Text(capabilityRemediationText(capability))
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                        HStack {
                            Button(capabilityActionLabel(capability)) {
                                state.performCapabilityAction(capability, settings: settings)
                                if isAdapterCapability(capability.kind) {
                                    appliedIntegrationSnapshot = SettingsIntegrationSnapshot(settings)
                                }
                            }
                            .buttonStyle(.borderedProminent)

                            Text(Date(timeIntervalSince1970: TimeInterval(capability.lastUpdatedMillis) / 1000), style: .time)
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                        }
                    }
                }
            }

            SettingsCard(title: "Reset", subtitle: "Restore defaults and push them to the running app.") {
                Text("Restores defaults immediately, stops launch-at-login, disables automatic AI-client registration, and applies runtime, integration, notification, and telemetry defaults.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Button("Reset to defaults", role: .destructive) {
                    settings.resetToDefaults()
                    state.applyNotificationSettings(settings)
                    state.applyRuntimeCollectionSettings(settings)
                    state.applyIntegrationSettings(settings)
                    state.applyLocalMcpClientRegistrationSettings(settings)
                    appliedIntegrationSnapshot = SettingsIntegrationSnapshot(settings)
                    applyConfirmation = "All settings reset and applied."
                    clearApplyConfirmationLater()
                }
                .buttonStyle(.bordered)
                if let applyConfirmation {
                    Text(applyConfirmation)
                        .font(.caption)
                        .foregroundStyle(AetowerDesign.Status.success)
                }
            }
        }
    }

    private func intervalSlider(
        title: String,
        value: Binding<Double>,
        range: ClosedRange<Double>,
        step: Double,
        format: String,
        valueWidth: CGFloat = 44,
        note: String? = nil
    ) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            Text(title)
                .font(.headline)
            HStack {
                Slider(value: value, in: range, step: step)
                Text(String(format: format, value.wrappedValue))
                    .font(.caption.monospacedDigit())
                    .frame(width: valueWidth, alignment: .trailing)
            }
            if let note {
                Text(note)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func clearApplyConfirmationLater() {
        Task {
            try? await Task.sleep(nanoseconds: 4_000_000_000)
            applyConfirmation = nil
        }
    }

    private func applyIntegrationAndTelemetrySettings(_ settings: SettingsStore) {
        state.applyIntegrationSettings(settings)
        appliedIntegrationSnapshot = SettingsIntegrationSnapshot(settings)
        applyConfirmation = "Integration and telemetry settings applied."
        clearApplyConfirmationLater()
    }

    private func isAdapterCapability(_ kind: CapabilityKind) -> Bool {
        switch kind {
        case .accessibility, .fullDiskAccess, .appleAutomation:
            return false
        case .chromiumDebug, .dockerSocket, .privilegedHelper, .endpointSecurity, .chau7:
            return true
        }
    }
}

private func registrationLabel(_ state: LocalMcpClientRegistrationState) -> String {
    switch state {
    case .registered:
        return "Registered"
    case .availableForAutomaticRegistration:
        return "Available"
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
        return "Refresh Status"
    }
}

private struct SettingsStatus {
    let label: String
    let color: Color

    init(_ label: String, _ color: Color) {
        self.label = label
        self.color = color
    }
}

private struct SettingsIntegrationSnapshot: Equatable {
    let chromiumEndpoint: String
    let dockerSocketPath: String
    let privilegedHelperEnabled: Bool
    let privilegedHelperPath: String
    let chau7Endpoint: String
    let telemetryEnabled: Bool
    let telemetryEndpoint: String
    let telemetryExportIntervalSeconds: UInt32

    @MainActor
    init(_ settings: SettingsStore) {
        chromiumEndpoint = settings.chromiumEndpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        dockerSocketPath = SettingsStore.normalizedDockerSocketPath(settings.dockerSocketPath)
        privilegedHelperEnabled = settings.privilegedHelperEnabled
        privilegedHelperPath = settings.privilegedHelperPath.trimmingCharacters(in: .whitespacesAndNewlines)
        chau7Endpoint = settings.chau7Endpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        telemetryEnabled = settings.telemetryEnabled
        telemetryEndpoint = SettingsStore.normalizedTelemetryEndpoint(settings.telemetryEndpoint)
        telemetryExportIntervalSeconds = SettingsStore.normalizedTelemetryExportIntervalSeconds(
            settings.telemetryExportIntervalSeconds
        )
    }
}

private enum SettingsCollectionPreset: String, CaseIterable, Identifiable {
    case batterySaver
    case balanced
    case diagnostic
    case fullDetail

    var id: String { rawValue }

    var title: String {
        switch self {
        case .batterySaver:
            return "Battery Saver"
        case .balanced:
            return "Balanced"
        case .diagnostic:
            return "Diagnostic"
        case .fullDetail:
            return "Full Detail"
        }
    }

    var subtitle: String {
        switch self {
        case .batterySaver:
            return "Slow sampling for long-running observation on battery."
        case .balanced:
            return "Default profile for daily monitoring."
        case .diagnostic:
            return "Sharper sampling for short performance investigations."
        case .fullDetail:
            return "Maximum detail for brief, active debugging sessions."
        }
    }

    var systemImage: String {
        switch self {
        case .batterySaver:
            return "battery.75"
        case .balanced:
            return "dial.medium"
        case .diagnostic:
            return "waveform.path.ecg"
        case .fullDetail:
            return "scope"
        }
    }

    var color: Color {
        switch self {
        case .batterySaver:
            return AetowerDesign.Status.success
        case .balanced:
            return AetowerDesign.Status.ready
        case .diagnostic:
            return AetowerDesign.Status.warning
        case .fullDetail:
            return AetowerDesign.Status.error
        }
    }

    @MainActor
    func apply(to settings: SettingsStore) {
        switch self {
        case .batterySaver:
            settings.collectionProfile = .balanced
            settings.adaptiveCadenceEnabled = true
            settings.engineActiveIntervalSeconds = 3.0
            settings.engineIdleIntervalSeconds = 12.0
            settings.engineLowPowerIntervalSeconds = 20.0
            settings.gpuSampleIntervalSeconds = 90.0
            settings.gpuSampleLowPowerIntervalSeconds = 150.0
        case .balanced:
            settings.collectionProfile = .balanced
            settings.adaptiveCadenceEnabled = true
            settings.engineActiveIntervalSeconds = 2.0
            settings.engineIdleIntervalSeconds = 5.0
            settings.engineLowPowerIntervalSeconds = 8.0
            settings.gpuSampleIntervalSeconds = 30.0
            settings.gpuSampleLowPowerIntervalSeconds = 60.0
        case .diagnostic:
            settings.collectionProfile = .full
            settings.adaptiveCadenceEnabled = true
            settings.engineActiveIntervalSeconds = 1.0
            settings.engineIdleIntervalSeconds = 2.0
            settings.engineLowPowerIntervalSeconds = 5.0
            settings.gpuSampleIntervalSeconds = 10.0
            settings.gpuSampleLowPowerIntervalSeconds = 20.0
        case .fullDetail:
            settings.collectionProfile = .full
            settings.adaptiveCadenceEnabled = false
            settings.engineActiveIntervalSeconds = 1.0
            settings.engineIdleIntervalSeconds = 1.0
            settings.engineLowPowerIntervalSeconds = 3.0
            settings.gpuSampleIntervalSeconds = 5.0
            settings.gpuSampleLowPowerIntervalSeconds = 10.0
        }
    }
}

private struct SettingsCard<Content: View>: View {
    let title: String
    let subtitle: String
    let status: SettingsStatus?
    let content: Content

    init(
        title: String,
        subtitle: String,
        status: SettingsStatus? = nil,
        @ViewBuilder content: () -> Content
    ) {
        self.title = title
        self.subtitle = subtitle
        self.status = status
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                HStack(spacing: AetowerDesign.Spacing.sm) {
                    Text(title)
                        .font(.headline)
                    if let status {
                        SettingsBadge(status.label, color: status.color)
                    }
                }
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            content
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(AetowerDesign.Spacing.xl)
        .background(
            RoundedRectangle(cornerRadius: AetowerDesign.Radius.lg)
                .fill(AetowerDesign.Surface.card)
        )
        .overlay(
            RoundedRectangle(cornerRadius: AetowerDesign.Radius.lg)
                .stroke(Color.secondary.opacity(0.10), lineWidth: 1)
        )
    }
}

private struct SettingsPresetCard: View {
    let preset: SettingsCollectionPreset

    var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: preset.systemImage)
                    .foregroundStyle(preset.color)
                    .frame(width: 20)
                Text(preset.title)
                    .font(.headline)
                Spacer()
            }
            Text(preset.subtitle)
                .font(.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.leading)
        }
        .frame(maxWidth: .infinity, minHeight: 82, alignment: .topLeading)
        .padding(AetowerDesign.Spacing.md)
        .background(
            RoundedRectangle(cornerRadius: AetowerDesign.Radius.md)
                .fill(preset.color.opacity(0.08))
        )
        .overlay(
            RoundedRectangle(cornerRadius: AetowerDesign.Radius.md)
                .stroke(preset.color.opacity(0.20), lineWidth: 1)
        )
    }
}

private struct SettingsRowCard<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            content
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(AetowerDesign.Spacing.md)
        .background(
            RoundedRectangle(cornerRadius: AetowerDesign.Radius.md)
                .fill(Color.secondary.opacity(0.045))
        )
    }
}

private struct SettingsBadge: View {
    let label: String
    let color: Color

    init(_ label: String, color: Color) {
        self.label = label
        self.color = color
    }

    var body: some View {
        Text(label)
            .font(.caption.weight(.semibold))
            .foregroundStyle(color)
            .padding(.horizontal, AetowerDesign.Spacing.sm)
            .padding(.vertical, AetowerDesign.Spacing.xs)
            .background(color.opacity(0.12), in: Capsule())
    }
}

private struct SettingsMiniStatusBadge: View {
    let label: String
    let color: Color

    init(_ label: String, color: Color) {
        self.label = label
        self.color = color
    }

    var body: some View {
        Text(label)
            .font(.caption2.weight(.semibold))
            .foregroundStyle(color)
            .lineLimit(1)
            .padding(.horizontal, AetowerDesign.Spacing.xs)
            .padding(.vertical, 2)
            .background(color.opacity(0.10), in: Capsule())
    }
}

private struct SettingsNotice: View {
    let title: String
    let detail: String
    let color: Color

    var body: some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(color)
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                Text(title)
                    .font(.caption.weight(.semibold))
                Text(detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(color.opacity(0.10), in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md))
    }
}

private struct SettingDivider: View {
    var body: some View {
        Divider()
            .padding(.vertical, AetowerDesign.Spacing.xs)
    }
}
