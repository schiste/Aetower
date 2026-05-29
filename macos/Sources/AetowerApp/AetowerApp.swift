import SwiftUI
import AetowerUI
import Sparkle

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

    @ViewBuilder
    private var primaryTabs: some View {
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

        Chau7View(state: state)
            .tabItem {
                Label("Chau7", systemImage: "terminal")
            }
    }

    @ViewBuilder
    private var secondaryTabs: some View {
        AIAgentsView(state: state)
            .tabItem {
                Label("AI Agents", systemImage: "cpu")
            }

        SensorDashboardView(state: state, settings: settings)
            .tabItem {
                Label("Sensors", systemImage: "thermometer.medium")
            }

        DiagnosticsView(state: state, settings: settings)
            .tabItem {
                Label("Diagnostics", systemImage: "waveform.path.ecg.rectangle")
            }

        FleetView(state: state)
            .tabItem {
                Label("Fleet", systemImage: "network")
            }

        SettingsView(state: state, settings: settings)
            .environment(updater)
            .tabItem {
                Label("Settings", systemImage: "slider.horizontal.3")
            }
    }

    var body: some Scene {
        WindowGroup {
            TabView {
                primaryTabs
                secondaryTabs
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
