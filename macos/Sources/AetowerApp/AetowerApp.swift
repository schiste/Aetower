import SwiftUI
import AetowerUI

@main
struct AetowerApp: App {
    @StateObject private var state = AppState()
    @StateObject private var settings = SettingsStore()

    var body: some Scene {
        WindowGroup {
            TabView {
                MainListView(state: state)
                    .tabItem {
                        Label("Monitor", systemImage: "gauge.with.needle")
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
                state.start(refreshInterval: settings.refreshIntervalSeconds)
                state.applyIntegrationSettings(settings)
            }
            .onChange(of: settings.refreshIntervalSeconds) { _, newValue in
                state.start(refreshInterval: newValue)
            }
            .onDisappear {
                state.stop()
            }
        }
        MenuBarExtra("Aetower", systemImage: "gauge.with.needle", isInserted: $settings.showMenuBarExtra) {
            MenuBarSummaryView(state: state, settings: settings)
        }
        .windowStyle(.titleBar)
    }
}
