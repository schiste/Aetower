import SwiftUI
import AetowerUI

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
    @State private var menuBarExtraInserted = false
    @State private var hudPanel: CompactHUDPanel?

    private var resolvedColorScheme: ColorScheme? {
        switch settings.appearanceMode {
        case "light": return .light
        case "dark": return .dark
        default: return nil
        }
    }

    private var menuBarTitle: String {
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
                MainListView(state: state)
                    .tabItem {
                        Label("Monitor", systemImage: "gauge.with.needle")
                    }

                HistoryView(state: state)
                    .tabItem {
                        Label("History", systemImage: "clock.arrow.trianglehead.counterclockwise.rotate.90")
                    }

                TimelineView(events: state.snapshot.timeline)
                    .padding()
                    .tabItem {
                        Label("Timeline", systemImage: "timeline.selection")
                    }

                DiagnosticsView(state: state, settings: settings)
                    .tabItem {
                        Label("Diagnostics", systemImage: "waveform.path.ecg.rectangle")
                    }

                ContentUnavailableView(
                    "Fleet monitoring is not available yet.",
                    systemImage: "network",
                    description: Text("Local peer discovery and shared monitoring are not live in this build.")
                )
                .padding()
                .tabItem {
                    Label("Fleet monitoring", systemImage: "network")
                }
                .disabled(true)

                SettingsView(state: state, settings: settings)
                    .tabItem {
                        Label("Settings", systemImage: "slider.horizontal.3")
                    }
            }
            .frame(minWidth: 1180, minHeight: 760)
            .preferredColorScheme(resolvedColorScheme)
            .task {
                menuBarExtraInserted = settings.showMenuBarExtra
                state.applyNotificationSettings(settings)
                state.applyIntegrationSettings(settings)
                try? await Task.sleep(nanoseconds: 1_000_000_000)
                state.start(refreshInterval: settings.refreshIntervalSeconds)
            }
            .onChange(of: settings.refreshIntervalSeconds) { _, newValue in
                state.start(refreshInterval: newValue)
            }
            .onChange(of: settings.showMenuBarExtra) { _, newValue in
                if menuBarExtraInserted != newValue {
                    menuBarExtraInserted = newValue
                }
            }
            .onChange(of: settings.notificationsEnabled) { _, _ in
                state.applyNotificationSettings(settings)
            }
            .onChange(of: settings.frictionNotificationThreshold) { _, _ in
                state.applyNotificationSettings(settings)
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
        }
        MenuBarExtra(menuBarTitle, systemImage: "bolt.fill", isInserted: $menuBarExtraInserted) {
            MenuBarSummaryView(state: state)
        }
        .onChange(of: menuBarExtraInserted) { _, newValue in
            if settings.showMenuBarExtra != newValue {
                settings.showMenuBarExtra = newValue
            }
        }
        .windowStyle(.titleBar)
    }
}
