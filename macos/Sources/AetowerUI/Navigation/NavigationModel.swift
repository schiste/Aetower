import Foundation
import SwiftUI

// MARK: - Top-level workspace identity

/// Stable, cross-process-addressable identity for every top-level workspace tab.
///
/// Raw values are the canonical slugs shared by four independent drivers:
///   - the `aetower://tab/<slug>` URL scheme (`onOpenURL`),
///   - the `aetower tab <slug>` CLI verb,
///   - the Cmd+1…8 "Navigate" menu shortcuts,
///   - accessibility identifiers (`tab.<slug>`).
///
/// Agents and scripts depend on these slugs — keep them stable. The `allCases`
/// order is also the on-screen tab order and the Cmd+N ordering.
public enum WorkspaceTab: String, CaseIterable, Hashable, Identifiable, Sendable {
    case monitor
    case activity
    case storage
    case repos
    case projects
    case agents
    case system
    case settings

    public var id: Self { self }

    public var title: String {
        switch self {
        case .monitor: return "Monitor"
        case .activity: return "Activity"
        case .storage: return "Storage"
        case .repos: return "Repos"
        case .projects: return "Projects"
        case .agents: return "Agents"
        case .system: return "System"
        case .settings: return "Settings"
        }
    }

    public var systemImage: String {
        switch self {
        case .monitor: return "gauge.with.needle"
        case .activity: return "timeline.selection"
        case .storage: return "externaldrive"
        case .repos: return "folder.badge.gearshape"
        case .projects: return "shippingbox"
        case .agents: return "cpu"
        case .system: return "wrench.and.screwdriver"
        case .settings: return "slider.horizontal.3"
        }
    }

    /// 1-based position, used for the Cmd+N keyboard shortcut and the Navigate menu.
    public var shortcutIndex: Int {
        (Self.allCases.firstIndex(of: self) ?? 0) + 1
    }

    /// Stable accessibility identifier for the tab, e.g. `tab.monitor`.
    public var accessibilityIdentifier: String { "tab.\(rawValue)" }
}

// MARK: - Sub-tab identities (hoisted out of AetowerApp so the router can reach them)

public enum ActivityWorkspaceTab: String, CaseIterable, Hashable, Identifiable, Sendable {
    case overview
    case history
    case timeline
    case storage

    public var id: Self { self }

    public var title: String {
        switch self {
        case .overview: return "Overview"
        case .history: return "History"
        case .timeline: return "Timeline"
        case .storage: return "Storage"
        }
    }

    public var role: String {
        switch self {
        case .overview: return "Time-domain summary"
        case .history: return "Persisted snapshots"
        case .timeline: return "Recent events"
        case .storage: return "Store health"
        }
    }

    public var summary: String {
        switch self {
        case .overview:
            return "Recent changes, event pressure, history coverage, and load status."
        case .history:
            return "Stored snapshots, trends, recurring entities, and before/after comparisons."
        case .timeline:
            return "Live events, alerts, anomalies, regressions, and recently finished processes."
        case .storage:
            return "History DB/WAL size, retention coverage, quarantine, and maintenance state."
        }
    }

    public var systemImage: String {
        switch self {
        case .overview: return "rectangle.grid.2x2"
        case .history: return "clock.arrow.trianglehead.counterclockwise.rotate.90"
        case .timeline: return "timeline.selection"
        case .storage: return "externaldrive"
        }
    }

    public var accessibilityIdentifier: String { "rail.activity.\(rawValue)" }
}

public enum AgentsWorkspaceTab: String, CaseIterable, Hashable, Identifiable, Sendable {
    case chau7
    case aiAgents

    public var id: Self { self }

    public var title: String {
        switch self {
        case .chau7: return "Chau7"
        case .aiAgents: return "AI Agents"
        }
    }

    public var systemImage: String {
        switch self {
        case .chau7: return "terminal"
        case .aiAgents: return "cpu"
        }
    }

    public var accessibilityIdentifier: String { "tab.agents.\(rawValue)" }
}

public enum SystemWorkspaceTab: String, CaseIterable, Hashable, Identifiable, Sendable {
    case sensors
    case persistence
    case diagnostics
    case fleet

    public var id: Self { self }

    public var title: String {
        switch self {
        case .sensors: return "Sensors"
        case .persistence: return "Startup"
        case .diagnostics: return "Diagnostics"
        case .fleet: return "Fleet"
        }
    }

    public var groupTitle: String {
        switch self {
        case .sensors: return "Live Machine"
        case .persistence: return "Trust & Startup"
        case .diagnostics: return "Aetower Ops"
        case .fleet: return "Network"
        }
    }

    public var role: String {
        switch self {
        case .sensors: return "Hardware health"
        case .persistence: return "Startup inventory"
        case .diagnostics: return "Self-observability"
        case .fleet: return "Peer discovery"
        }
    }

    public var summary: String {
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

    public var detailTitle: String {
        switch self {
        case .persistence: return "Startup & persistence"
        default: return title
        }
    }

    public var systemImage: String {
        switch self {
        case .sensors: return "thermometer.medium"
        case .persistence: return "lock.shield"
        case .diagnostics: return "waveform.path.ecg.rectangle"
        case .fleet: return "network"
        }
    }

    public var accessibilityIdentifier: String { "rail.system.\(rawValue)" }
}

// MARK: - Navigation model

/// Single source of truth for which workspace (and sub-tab) is showing.
///
/// Owning navigation in one `@Observable` — instead of scattered private
/// `@State` — is what makes the app driveable from outside a mouse click:
/// the URL router, the Cmd+N menu, and the views all read and write the same
/// properties, and the "default startup tab" override lives in one place an
/// agent can also set with `defaults write com.aeptus.aetower <key> <slug>`.
@Observable
@MainActor
public final class NavigationModel {
    /// The visible top-level workspace. Transient by design: per the
    /// "fixed home + override" policy, clicking around does NOT change what
    /// the app opens on next launch. Only the explicit default override does.
    public var workspace: WorkspaceTab

    /// Sub-tab selections, hoisted here so `aetower://tab/<workspace>/<subtab>`
    /// and future keyboard/CLI drivers can address them.
    public var activity: ActivityWorkspaceTab = .overview
    public var agents: AgentsWorkspaceTab = .chau7
    public var system: SystemWorkspaceTab = .sensors

    private let defaults: UserDefaults

    /// UserDefaults key for the startup override. Public and documented so it is
    /// a supported automation surface, not an implementation detail:
    ///   `defaults write com.aeptus.aetower nav.defaultWorkspaceTab system`
    public static let defaultWorkspaceKey = "nav.defaultWorkspaceTab"

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        self.workspace = Self.resolveInitialTab(defaults: defaults)
    }

    // MARK: Launch policy

    /// Resolve the tab to open on launch under the "fixed home + override"
    /// policy the app was configured with:
    ///
    ///   - If a valid override slug is persisted, open that tab.
    ///   - Otherwise fall back to the fixed home tab (`.monitor`).
    ///
    /// An unrecognized or empty override is ignored (treated as "no override")
    /// rather than trapping — a stale `defaults write` from an old build must
    /// never wedge the app on a nonexistent tab.
    static func resolveInitialTab(defaults: UserDefaults) -> WorkspaceTab {
        guard
            let slug = defaults.string(forKey: defaultWorkspaceKey),
            let tab = WorkspaceTab(rawValue: slug)
        else {
            return .monitor
        }
        return tab
    }

    // MARK: Default override (settable by agents, humans, CLI, or URL)

    /// The persisted startup override, or `nil` when the app uses fixed home.
    public var defaultTab: WorkspaceTab? {
        get {
            guard let slug = defaults.string(forKey: Self.defaultWorkspaceKey) else { return nil }
            return WorkspaceTab(rawValue: slug)
        }
        set {
            if let newValue {
                defaults.set(newValue.rawValue, forKey: Self.defaultWorkspaceKey)
            } else {
                defaults.removeObject(forKey: Self.defaultWorkspaceKey)
            }
        }
    }

    // MARK: Selection

    public func select(_ tab: WorkspaceTab) {
        workspace = tab
    }

    // MARK: URL routing

    /// Route an `aetower://tab/<workspace>[/<subtab>]` URL.
    ///
    /// Only `host == "tab"` URLs are claimed; anything else (e.g. the OAuth
    /// callback `aetower://oauth/...`) is left untouched by returning `false`.
    /// A `?default=true` (or `1`) query also persists the workspace as the
    /// startup override.
    ///
    /// - Returns: `true` when the URL was a tab-navigation URL this model handled.
    @discardableResult
    public func handleURL(_ url: URL) -> Bool {
        guard
            url.scheme?.lowercased() == "aetower",
            let components = URLComponents(url: url, resolvingAgainstBaseURL: false),
            components.host?.lowercased() == "tab"
        else {
            return false
        }

        let segments = components.path
            .split(separator: "/", omittingEmptySubsequences: true)
            .map { $0.lowercased() }
        guard let workspaceSlug = segments.first,
              let target = WorkspaceTab(rawValue: workspaceSlug)
        else {
            return false
        }

        workspace = target
        if segments.count > 1 {
            applySubtab(segments[1], in: target)
        }

        let wantsDefault = components.queryItems?.contains { item in
            item.name.lowercased() == "default"
                && ["1", "true", "yes"].contains((item.value ?? "").lowercased())
        } ?? false
        if wantsDefault {
            defaultTab = target
        }

        return true
    }

    private func applySubtab(_ slug: String, in workspace: WorkspaceTab) {
        switch workspace {
        case .activity:
            if let tab = ActivityWorkspaceTab(rawValue: slug) { activity = tab }
        case .agents:
            if let tab = AgentsWorkspaceTab(rawValue: slug) { agents = tab }
        case .system:
            if let tab = SystemWorkspaceTab(rawValue: slug) { system = tab }
        default:
            break
        }
    }
}
