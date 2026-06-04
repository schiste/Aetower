import AetowerBridge
import AetowerUI
import Sparkle
import SwiftUI

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
        false
    }

    func applicationShouldSaveApplicationState(_ app: NSApplication) -> Bool {
        false
    }

    func applicationShouldRestoreApplicationState(_ app: NSApplication) -> Bool {
        false
    }
}

private enum AgentsWorkspaceTab: String, CaseIterable, Hashable, Identifiable {
    case chau7
    case aiAgents

    var id: Self { self }

    var title: String {
        switch self {
        case .chau7: return "Chau7"
        case .aiAgents: return "AI Agents"
        }
    }

    var systemImage: String {
        switch self {
        case .chau7: return "terminal"
        case .aiAgents: return "cpu"
        }
    }
}

private enum SystemWorkspaceTab: String, CaseIterable, Hashable, Identifiable {
    case sensors
    case persistence
    case diagnostics
    case fleet

    var id: Self { self }

    var title: String {
        switch self {
        case .sensors: return "Sensors"
        case .persistence: return "Startup"
        case .diagnostics: return "Diagnostics"
        case .fleet: return "Fleet"
        }
    }

    var groupTitle: String {
        switch self {
        case .sensors: return "Live Machine"
        case .persistence: return "Trust & Startup"
        case .diagnostics: return "Aetower Ops"
        case .fleet: return "Network"
        }
    }

    var role: String {
        switch self {
        case .sensors: return "Hardware health"
        case .persistence: return "Startup inventory"
        case .diagnostics: return "Self-observability"
        case .fleet: return "Peer discovery"
        }
    }

    var summary: String {
        switch self {
        case .sensors:
            return "Thermals, fans, power, storage, battery, Bluetooth, and per-core load."
        case .persistence:
            return "What starts on this Mac, what is active now, and what deserves review."
        case .diagnostics:
            return "Aetower runtime health, MCP state, payload sizes, memory, and logs."
        case .fleet:
            return "Optional local-network visibility across other Aetower instances."
        }
    }

    var detailTitle: String {
        switch self {
        case .persistence: return "Startup & persistence"
        default: return title
        }
    }

    var systemImage: String {
        switch self {
        case .sensors: return "thermometer.medium"
        case .persistence: return "lock.shield"
        case .diagnostics: return "waveform.path.ecg.rectangle"
        case .fleet: return "network"
        }
    }
}

private struct AgentsWorkspaceView: View {
    let state: AppState
    @State private var selectedTab: AgentsWorkspaceTab = .chau7

    private enum Chau7Status: Hashable {
        case enriched
        case running
        case configured
        case unavailable
    }

    var body: some View {
        VStack(spacing: 0) {
            agentsPrompt
            mergedTabHeader(selection: $selectedTab)
            Divider()
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .task(id: chau7Status) {
            selectPreferredAgentView(for: chau7Status)
        }
    }

    private var chau7Status: Chau7Status {
        if !state.snapshot.chau7Sessions.isEmpty || !chau7LinkedEntities.isEmpty {
            return .enriched
        }
        if chau7Entity != nil {
            return .running
        }
        if let capability = chau7Capability,
           capability.health == .configured || capability.health == .cached || capability.health == .live
        {
            return .configured
        }
        return .unavailable
    }

    private var chau7Capability: CapabilitySnapshot? {
        state.snapshot.capabilities.first { $0.kind == .chau7 }
    }

    private var chau7Entity: EntitySnapshot? {
        state.snapshot.entities.first { entity in
            entity.bundleId == "local.chau7"
                || entity.entityId == "bundle-path:/applications/chau7.app"
                || entity.executablePath == "/Applications/Chau7.app/Contents/MacOS/Chau7"
                || entity.displayName.localizedCaseInsensitiveContains("Chau7")
        }
    }

    private var chau7LinkedEntities: [EntitySnapshot] {
        state.snapshot.entities.filter { entity in
            entity.badges.contains("chau7-live")
                || entity.components.contains { $0.adapterContext?.kind == .chau7Session }
        }
    }

    private var aiAgentCount: Int {
        state.snapshot.entities.filter { $0.entityKind == .aiAgent }.count
    }

    private var promptIcon: String {
        switch chau7Status {
        case .enriched: return "sparkles"
        case .running: return "terminal"
        case .configured: return "powerplug"
        case .unavailable: return "wand.and.stars"
        }
    }

    private var promptTint: Color {
        switch chau7Status {
        case .enriched: return .green
        case .running, .configured: return .orange
        case .unavailable: return .accentColor
        }
    }

    private var promptTitle: String {
        switch chau7Status {
        case .enriched:
            return "Chau7 enrichment is active"
        case .running:
            return "Chau7 is running; waiting for session context"
        case .configured:
            return "Chau7 integration is configured"
        case .unavailable:
            return "Use Chau7 for richer agent context"
        }
    }

    private var promptDetail: String {
        switch chau7Status {
        case .enriched:
            let sessionCount = state.snapshot.chau7Sessions.count
            let linkedCount = chau7LinkedEntities.count
            return "\(sessionCount) Chau7 session(s) and \(linkedCount) linked process group(s) are available. Inspect tab names, repositories, branches, approvals, and child-process pressure from the enriched view."
        case .running:
            return "Aetower sees Chau7, but not its session catalog yet. Open a Chau7 agent tab or enable the Chau7 MCP bridge to attach tabs, repositories, branches, approvals, and child processes."
        case .configured:
            return "Start Chau7 to turn process-only agent telemetry into session-aware telemetry with tab names, repository and branch context, approvals, and per-session process attribution."
        case .unavailable:
            return "Aetower can monitor agents by process today. Chau7 adds the missing workflow context: terminal tabs, sessions, repositories, branches, approvals, and child-process attribution."
        }
    }

    private var capabilityLabel: String {
        guard let capability = chau7Capability else {
            return "No adapter signal"
        }
        return "\(capabilityStateLabel(capability)) · \(capabilityHealthLabel(capability.health))"
    }

    private var agentsPrompt: some View {
        HStack(alignment: .top, spacing: 14) {
            Image(systemName: promptIcon)
                .font(.system(size: 22, weight: .semibold))
                .foregroundStyle(promptTint)
                .frame(width: 34, height: 34)
                .background(promptTint.opacity(0.12), in: Circle())

            VStack(alignment: .leading, spacing: 8) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(promptTitle)
                        .font(.headline)
                    Text(promptDetail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }

                promptMetrics
            }

            Spacer(minLength: 16)

            VStack(alignment: .trailing, spacing: 8) {
                Button {
                    selectedTab = .chau7
                } label: {
                    Label("Enriched view", systemImage: "terminal")
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)

                Button {
                    selectedTab = .aiAgents
                } label: {
                    Label("Process-only view", systemImage: "cpu")
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .background(promptTint.opacity(0.07))
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(promptTint.opacity(0.16))
                .frame(height: 1)
        }
    }

    private var promptMetrics: some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 92), spacing: 8, alignment: .leading)],
            alignment: .leading,
            spacing: 6
        ) {
            promptMetric("Sessions", "\(state.snapshot.chau7Sessions.count)")
            promptMetric("Linked groups", "\(chau7LinkedEntities.count)")
            promptMetric("AI agents", "\(aiAgentCount)")
            promptMetric("Adapter", capabilityLabel)
        }
    }

    private func promptMetric(_ label: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(label.uppercased())
                .font(.caption2)
                .foregroundStyle(.tertiary)
            Text(value)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.primary)
                .lineLimit(1)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 5)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
    }

    @ViewBuilder
    private var content: some View {
        switch selectedTab {
        case .chau7:
            Chau7View(state: state)
        case .aiAgents:
            AIAgentsView(state: state)
        }
    }

    private func mergedTabHeader(selection: Binding<AgentsWorkspaceTab>) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Agents")
                    .font(.headline)
                Text("Use Chau7 when available; fall back to process-only AI agent telemetry.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Picker("Agents section", selection: selection) {
                ForEach(AgentsWorkspaceTab.allCases) { tab in
                    Label(tab.title, systemImage: tab.systemImage)
                        .tag(tab)
                }
            }
            .pickerStyle(.segmented)
            .frame(width: 260)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(.bar)
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
            return "Unavailable"
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

    private func selectPreferredAgentView(for status: Chau7Status) {
        switch status {
        case .enriched, .running:
            selectedTab = .chau7
        case .configured, .unavailable:
            if selectedTab == .chau7 {
                selectedTab = .aiAgents
            }
        }
    }
}

private struct SystemWorkspaceView: View {
    let state: AppState
    let settings: SettingsStore
    @State private var selectedTab: SystemWorkspaceTab = .sensors

    private let groupedTabs: [(title: String, tabs: [SystemWorkspaceTab])] = [
        ("Live Machine", [.sensors]),
        ("Trust & Startup", [.persistence]),
        ("Aetower Ops", [.diagnostics]),
        ("Network", [.fleet]),
    ]

    var body: some View {
        VStack(spacing: 0) {
            systemHeader
            Divider()
            HStack(spacing: 0) {
                navigationRail
                Divider()
                content
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
    }

    private var systemHeader: some View {
        HStack(alignment: .top, spacing: 18) {
            VStack(alignment: .leading, spacing: 4) {
                Text("System")
                    .font(.title3.weight(.semibold))
                Text("Hardware health first, with startup review, Aetower diagnostics, and local fleet tools one level deeper.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Spacer()

            HStack(spacing: 8) {
                statusPill(
                    title: "Thermal",
                    value: thermalLabel,
                    systemImage: "thermometer.medium",
                    tint: thermalTint
                )
                statusPill(
                    title: "Sensors",
                    value: sensorCoverageLabel,
                    systemImage: "sensor",
                    tint: sensorCoverageTint
                )
                statusPill(
                    title: "Diagnostics",
                    value: diagnosticsLabel,
                    systemImage: "waveform.path.ecg.rectangle",
                    tint: diagnosticsTint
                )
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .background(.bar)
    }

    private var navigationRail: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                ForEach(groupedTabs, id: \.title) { group in
                    VStack(alignment: .leading, spacing: 7) {
                        Text(group.title.uppercased())
                            .font(.caption2.weight(.semibold))
                            .foregroundStyle(.tertiary)
                            .padding(.horizontal, 12)

                        ForEach(group.tabs) { tab in
                            moduleButton(tab)
                        }
                    }
                }

                Spacer(minLength: 0)
            }
            .padding(12)
        }
        .frame(width: 292)
        .background(.regularMaterial)
    }

    private func moduleButton(_ tab: SystemWorkspaceTab) -> some View {
        let isSelected = selectedTab == tab
        return Button {
            selectedTab = tab
        } label: {
            VStack(alignment: .leading, spacing: 8) {
                HStack(alignment: .top, spacing: 10) {
                    Image(systemName: tab.systemImage)
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(isSelected ? Color.accentColor : .secondary)
                        .frame(width: 22)

                    VStack(alignment: .leading, spacing: 2) {
                        Text(tab.detailTitle)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.primary)
                            .lineLimit(1)
                        Text(tab.role)
                            .font(.caption2)
                            .foregroundStyle(isSelected ? Color.accentColor : .secondary)
                    }

                    Spacer()
                }

                Text(tab.summary)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)

                Text(moduleSignal(for: tab))
                    .font(.caption2.weight(.medium))
                    .foregroundStyle(signalTint(for: tab))
                    .lineLimit(1)
            }
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                isSelected ? Color.accentColor.opacity(0.11) : Color.secondary.opacity(0.055),
                in: RoundedRectangle(cornerRadius: 12, style: .continuous)
            )
            .overlay {
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .stroke(isSelected ? Color.accentColor.opacity(0.32) : Color.clear, lineWidth: 1)
            }
        }
        .buttonStyle(.plain)
    }

    private func statusPill(title: String, value: String, systemImage: String, tint: Color) -> some View {
        HStack(spacing: 7) {
            Image(systemName: systemImage)
                .foregroundStyle(tint)
            VStack(alignment: .leading, spacing: 1) {
                Text(title.uppercased())
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                Text(value)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.primary)
                    .lineLimit(1)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
        .background(tint.opacity(0.08), in: Capsule())
    }

    @ViewBuilder
    private var content: some View {
        switch selectedTab {
        case .sensors:
            SensorDashboardView(state: state, settings: settings)
        case .persistence:
            PersistenceScannerView(state: state, settings: settings)
        case .diagnostics:
            DiagnosticsView(state: state, settings: settings)
        case .fleet:
            FleetView(state: state)
        }
    }

    private var thermalLabel: String {
        switch state.snapshot.host.thermalState {
        case .nominal: return "Nominal"
        case .fair: return "Fair"
        case .serious: return "Serious"
        case .critical: return "Critical"
        }
    }

    private var thermalTint: Color {
        switch state.snapshot.host.thermalState {
        case .nominal: return .green
        case .fair: return .orange
        case .serious, .critical: return .red
        }
    }

    private var sensorCoverageLabel: String {
        let host = state.snapshot.host
        let channels = [
            !host.perCoreCpu.isEmpty,
            !host.fans.isEmpty,
            !host.cpuTemperatures.isEmpty || host.gpuTemperatureCelsius != nil,
            !host.powerReadings.isEmpty,
            !host.disks.isEmpty,
            host.batteryHealth != nil,
            !host.bluetoothDevices.isEmpty,
        ].filter { $0 }.count
        return channels == 0 ? "Waiting" : "\(channels)/7 live"
    }

    private var sensorCoverageTint: Color {
        sensorCoverageLabel == "Waiting" ? .orange : .green
    }

    private var diagnosticsLabel: String {
        let overview = state.diagnosticsOverview
        if overview.errorCount > 0 {
            return "\(overview.errorCount) errors"
        }
        if overview.warnCount > 0 {
            return "\(overview.warnCount) warnings"
        }
        return "Quiet"
    }

    private var diagnosticsTint: Color {
        let overview = state.diagnosticsOverview
        if overview.errorCount > 0 { return .red }
        if overview.warnCount > 0 { return .orange }
        return .green
    }

    private func moduleSignal(for tab: SystemWorkspaceTab) -> String {
        switch tab {
        case .sensors:
            return "\(thermalLabel) thermal · \(sensorCoverageLabel)"
        case .persistence:
            return "Fast scan opens lazily · deep audit on demand"
        case .diagnostics:
            return "\(diagnosticsLabel) · \(state.diagnosticsOverview.currentSize) buffered events"
        case .fleet:
            return "Opt-in Bonjour discovery"
        }
    }

    private func signalTint(for tab: SystemWorkspaceTab) -> Color {
        switch tab {
        case .sensors:
            return thermalTint
        case .persistence:
            return .orange
        case .diagnostics:
            return diagnosticsTint
        case .fleet:
            return .blue
        }
    }
}

@main
struct AetowerApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @State private var state = AppState()
    @State private var settings = SettingsStore()
    @State private var updater = UpdaterController()
    @State private var menuBarExtraInserted = false
    @State private var hudPanel: CompactHUDPanel?
    @State private var menuBarDisplayTitle = "Aetower"
    @State private var lastMenuBarTitleUpdate = Date.distantPast

    private var resolvedColorScheme: ColorScheme? {
        switch settings.appearanceMode {
        case "light": return .light
        case "dark": return .dark
        default: return nil
        }
    }

    private var computedMenuBarTitle: String {
        let entities = state.snapshot.entities
        if entities.isEmpty { return "Aetower" }
        let topFriction = entities.first?.friction.totalScore ?? 0
        return String(format: "%.0f", topFriction)
    }

    init() {
        UserDefaults.standard.set(false, forKey: "NSQuitAlwaysKeepsWindows")
        UserDefaults.standard.set(true, forKey: "ApplePersistenceIgnoreState")
    }

    var body: some Scene {
        WindowGroup {
            TabView {
                MainListView(state: state, settings: settings)
                    .tabItem {
                        Label("Monitor", systemImage: "gauge.with.needle")
                    }

                HistoryView(state: state, settings: settings)
                    .tabItem {
                        Label("History", systemImage: "clock.arrow.trianglehead.counterclockwise.rotate.90")
                    }

                TimelineView(state: state, settings: settings)
                    .tabItem {
                        Label("Timeline", systemImage: "timeline.selection")
                    }

                AgentsWorkspaceView(state: state)
                    .tabItem {
                        Label("Agents", systemImage: "cpu")
                    }

                SystemWorkspaceView(state: state, settings: settings)
                    .tabItem {
                        Label("System", systemImage: "wrench.and.screwdriver")
                    }

                SettingsView(state: state, settings: settings)
                    .environment(updater)
                    .tabItem {
                        Label("Settings", systemImage: "slider.horizontal.3")
                    }
            }
            .frame(minWidth: 1180, minHeight: 760)
            .preferredColorScheme(resolvedColorScheme)
            .task {
                menuBarExtraInserted = settings.showMenuBarExtra
                refreshMenuBarTitle(force: true)
                state.startLocalMcpServer(autoRegisterClients: settings.autoRegisterLocalMcpClientsEnabled)
                state.applyNotificationSettings(settings)
                state.applyRuntimeCollectionSettings(settings)
                state.applyIntegrationSettings(settings)
                // NSApplication.terminate exits before SwiftUI @State deinit,
                // so rely on the will-terminate notification to tear down the
                // MCP socket explicitly — otherwise the socket file lingers
                // between runs and the next launch must rebind over a stale
                // entry.
                NotificationCenter.default.addObserver(
                    forName: NSApplication.willTerminateNotification,
                    object: nil,
                    queue: .main
                ) { _ in
                    MainActor.assumeIsolated {
                        state.stopLocalMcpServer()
                    }
                }
                try? await Task.sleep(nanoseconds: 1_000_000_000)
                state.start(refreshInterval: settings.refreshIntervalSeconds)
            }
            .onChange(of: state.snapshot.sequence) { _, _ in
                refreshMenuBarTitle()
            }
            .onChange(of: settings.refreshIntervalSeconds) { _, newValue in
                state.start(refreshInterval: newValue)
            }
            .onChange(of: settings.showMenuBarExtra) { _, newValue in
                if menuBarExtraInserted != newValue {
                    menuBarExtraInserted = newValue
                }
                refreshMenuBarTitle(force: true)
            }
            .onChange(of: settings.notificationsEnabled) { _, _ in
                state.applyNotificationSettings(settings)
            }
            .onChange(of: settings.frictionNotificationThreshold) { _, _ in
                state.applyNotificationSettings(settings)
            }
            .onChange(of: settings.collectionProfile) { _, _ in
                state.applyRuntimeCollectionSettings(settings)
            }
            .onChange(of: settings.adaptiveCadenceEnabled) { _, _ in
                state.applyRuntimeCollectionSettings(settings)
            }
            .onChange(of: settings.engineActiveIntervalSeconds) { _, _ in
                state.applyRuntimeCollectionSettings(settings)
            }
            .onChange(of: settings.engineIdleIntervalSeconds) { _, _ in
                state.applyRuntimeCollectionSettings(settings)
            }
            .onChange(of: settings.engineLowPowerIntervalSeconds) { _, _ in
                state.applyRuntimeCollectionSettings(settings)
            }
            .onChange(of: settings.gpuSampleIntervalSeconds) { _, _ in
                state.applyRuntimeCollectionSettings(settings)
            }
            .onChange(of: settings.gpuSampleLowPowerIntervalSeconds) { _, _ in
                state.applyRuntimeCollectionSettings(settings)
            }
            .onChange(of: settings.autoRegisterLocalMcpClientsEnabled) { _, _ in
                state.applyLocalMcpClientRegistrationSettings(settings)
            }
            .onDisappear {
                state.stop()
            }
        }
        .commands {
            CommandMenu("View") {
                Button("Toggle Compact HUD") {
                    if hudPanel == nil { hudPanel = CompactHUDPanel(state: state) }
                    hudPanel?.toggle()
                }
                .keyboardShortcut("h", modifiers: [.command, .shift])
            }
            CommandMenu("Updates") {
                Button("Check for Updates…") {
                    updater.checkForUpdates()
                }
                .disabled(!updater.isConfigured)
            }
        }
        MenuBarExtra(menuBarDisplayTitle, systemImage: "bolt.fill", isInserted: $menuBarExtraInserted) {
            MenuBarSummaryView(state: state)
        }
        .onChange(of: menuBarExtraInserted) { _, newValue in
            if settings.showMenuBarExtra != newValue {
                settings.showMenuBarExtra = newValue
            }
        }
        .windowStyle(.titleBar)
    }

    private func refreshMenuBarTitle(force: Bool = false) {
        if !menuBarExtraInserted && !force {
            return
        }
        let now = Date()
        let nextTitle = computedMenuBarTitle
        let previousTitle = menuBarDisplayTitle
        let elapsed = now.timeIntervalSince(lastMenuBarTitleUpdate)

        if !force {
            if previousTitle == nextTitle {
                return
            }
            if elapsed < 20,
               let previousValue = Double(previousTitle),
               let nextValue = Double(nextTitle),
               abs(previousValue - nextValue) < 5
            {
                return
            }
        }

        menuBarDisplayTitle = nextTitle
        lastMenuBarTitleUpdate = now
    }
}
