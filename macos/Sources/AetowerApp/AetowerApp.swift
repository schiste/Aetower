import SwiftUI
import AetowerUI

@main
struct AetowerApp: App {
    @StateObject private var state = AppState()

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

                SettingsView(state: state)
                    .tabItem {
                        Label("Settings", systemImage: "slider.horizontal.3")
                    }
            }
            .frame(minWidth: 1180, minHeight: 760)
            .task {
                state.start()
            }
            .onDisappear {
                state.stop()
            }
        }
        .windowStyle(.titleBar)
    }
}
