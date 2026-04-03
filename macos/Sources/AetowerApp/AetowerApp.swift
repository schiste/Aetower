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
    @StateObject private var state = AppState()
    @StateObject private var settings = SettingsStore()
    @State private var menuBarExtraInserted = false

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

                SettingsView(state: state, settings: settings)
                    .tabItem {
                        Label("Settings", systemImage: "slider.horizontal.3")
                    }
            }
            .frame(minWidth: 1180, minHeight: 760)
            .task {
                menuBarExtraInserted = settings.showMenuBarExtra
                state.requestNotificationPermission()
                Task { @MainActor in
                    state.applyIntegrationSettings(settings)
                }
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) {
                    state.start(refreshInterval: settings.refreshIntervalSeconds)
                }
            }
            .onChange(of: settings.refreshIntervalSeconds) { _, newValue in
                DispatchQueue.main.async {
                    state.start(refreshInterval: newValue)
                }
            }
            .onChange(of: settings.showMenuBarExtra) { _, newValue in
                if menuBarExtraInserted != newValue {
                    menuBarExtraInserted = newValue
                }
            }
            .onDisappear {
                state.stop()
            }
        }
        MenuBarExtra("Aetower", systemImage: "gauge.with.needle", isInserted: $menuBarExtraInserted) {
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
