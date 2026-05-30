import AetowerBridge
import Foundation
import Network
import Observation

@MainActor
@Observable
public final class FleetService {
    public private(set) var peers: [FleetPeer] = []
    public private(set) var isEnabled = false

    private var browser: NWBrowser?
    private var listener: NWListener?
    private weak var state: AppState?
    private var pollTimer: Timer?
    private var discoveredEndpoints: [String: NWEndpoint] = [:]
    /// Active poll connections keyed by peer ID. Each tick cancels the
    /// previous connection before dialing a new one, preventing the
    /// unbounded accumulation that occurred when slow connections
    /// outlived the 5-second poll interval.
    private var activeConnections: [String: NWConnection] = [:]

    private static let serviceType = "_aetower._tcp"
    /// Peers whose lastSeen is older than this are visually marked stale.
    nonisolated static let stalenessThreshold: TimeInterval = 30

    public struct FleetPeer: Identifiable {
        public let id: String
        public var name: String
        public var friction: Float = 0
        public var cpuPercent: Float = 0
        public var entityCount: Int = 0
        /// Number of running entities whose binary is unsigned or ad-hoc signed
        /// — the fleet-wide security signal. Counts only resolved verdicts;
        /// "unknown" (not yet classified) is excluded.
        public var unsignedCount: Int = 0
        public var lastSeen: Date = .now
        public var isLocal: Bool = false

        /// True when the peer has not responded to polls within the
        /// staleness threshold. The view renders these with a dimmed
        /// style so the user knows the data is stale.
        public var isStale: Bool {
            !isLocal && Date.now.timeIntervalSince(lastSeen) > FleetService.stalenessThreshold
        }
    }

    public func start(state: AppState) {
        // Guard on listener presence, not isEnabled. Since isEnabled
        // is now set asynchronously in the listener's stateUpdateHandler,
        // a rapid stop/start could pass a guard on isEnabled twice and
        // create duplicate listeners, browsers, and timers.
        guard listener == nil else { return }
        self.state = state
        startAdvertising(state: state)
        startBrowsing()
        pollTimer = Timer.scheduledTimer(withTimeInterval: 5, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.pollPeers() }
        }
    }

    public func stop() {
        isEnabled = false
        listener?.cancel()
        listener = nil
        browser?.cancel()
        browser = nil
        pollTimer?.invalidate()
        pollTimer = nil
        // Cancel every in-flight poll connection so they don't outlive
        // the service and write to deallocated state.
        for connection in activeConnections.values {
            connection.cancel()
        }
        activeConnections.removeAll()
        peers.removeAll()
        discoveredEndpoints.removeAll()
    }

    // MARK: - Advertise

    private func startAdvertising(state: AppState) {
        do {
            let listener = try NWListener(using: .tcp)
            listener.service = NWListener.Service(type: Self.serviceType)
            listener.newConnectionHandler = { [weak self, weak state] connection in
                Task { @MainActor in
                    self?.handleIncoming(connection, state: state)
                }
            }
            // Only set isEnabled after the listener confirms it is
            // ready. Previously isEnabled was set before this call,
            // so the UI showed "On" even when the listener failed.
            listener.stateUpdateHandler = { [weak self] listenerState in
                Task { @MainActor in
                    if case .ready = listenerState {
                        self?.isEnabled = true
                    }
                }
            }
            listener.start(queue: .main)
            self.listener = listener
        } catch {
            // Listener allocation failed (port conflict, entitlement).
            // isEnabled stays false so the UI shows the real state.
        }
    }

    private func handleIncoming(_ connection: NWConnection, state: AppState?) {
        connection.start(queue: .main)
        connection.receive(minimumIncompleteLength: 1, maximumLength: 1024) { [weak state] _, _, _, _ in
            Task { @MainActor in
                guard let state else { connection.cancel(); return }
                let json = state.exportSnapshotJSON()
                let data = Data(json.utf8)
                let header = "HTTP/1.1 200 OK\r\nContent-Length: \(data.count)\r\nContent-Type: application/json\r\n\r\n"
                connection.send(content: Data(header.utf8) + data, completion: .contentProcessed { _ in
                    connection.cancel()
                })
            }
        }
    }

    // MARK: - Discover

    private func startBrowsing() {
        let browser = NWBrowser(for: .bonjour(type: Self.serviceType, domain: nil), using: .tcp)
        browser.browseResultsChangedHandler = { [weak self] results, _ in
            Task { @MainActor in self?.updatePeers(from: results) }
        }
        browser.start(queue: .main)
        self.browser = browser
    }

    private func updatePeers(from results: Set<NWBrowser.Result>) {
        let localName = Host.current().localizedName ?? ProcessInfo.processInfo.hostName

        // Rebuild discoveredEndpoints from the current result set so
        // departed peers are pruned (previously the dictionary only
        // grew and never shrank).
        var freshEndpoints: [String: NWEndpoint] = [:]
        var updated: [FleetPeer] = []

        for result in results {
            guard case let .service(name, _, _, _) = result.endpoint else { continue }
            let isLocal = name == localName || name.hasPrefix(localName)

            freshEndpoints[name] = result.endpoint

            if var existing = peers.first(where: { $0.id == name }) {
                existing.isLocal = isLocal
                if isLocal { syncLocalPeerMetrics(&existing) }
                updated.append(existing)
            } else {
                var peer = FleetPeer(id: name, name: name, isLocal: isLocal)
                if isLocal { syncLocalPeerMetrics(&peer) }
                updated.append(peer)
            }
        }

        // Cancel connections for peers that disappeared.
        for id in discoveredEndpoints.keys where freshEndpoints[id] == nil {
            activeConnections[id]?.cancel()
            activeConnections.removeValue(forKey: id)
        }

        discoveredEndpoints = freshEndpoints
        peers = updated.sorted { $0.name < $1.name }
    }

    // MARK: - Poll

    private func pollPeers() {
        // Local peers: read directly from AppState.
        for index in peers.indices where peers[index].isLocal {
            syncLocalPeerMetrics(&peers[index])
        }

        // Remote peers: dial via NWConnection keyed by peer ID.
        for peer in peers where !peer.isLocal {
            guard let endpoint = discoveredEndpoints[peer.id] else { continue }
            let peerId = peer.id

            // Cancel any in-flight connection from the previous tick.
            // Nil out its stateUpdateHandler first so its .cancelled
            // callback doesn't race with the new connection and remove
            // the new entry from activeConnections.
            if let old = activeConnections[peerId] {
                old.stateUpdateHandler = nil
                old.cancel()
            }

            let connection = NWConnection(to: endpoint, using: .tcp)
            activeConnections[peerId] = connection
            connection.start(queue: .main)
            connection.stateUpdateHandler = { [weak self] connState in
                switch connState {
                case .ready:
                    let request = Data("GET / HTTP/1.0\r\nHost: local\r\n\r\n".utf8)
                    connection.send(content: request, completion: .contentProcessed { _ in
                        connection.receive(minimumIncompleteLength: 1, maximumLength: 1_048_576) { data, _, _, _ in
                            connection.cancel()
                            guard let data, let body = Self.extractHTTPBody(data) else { return }
                            Task { @MainActor in
                                self?.applyRemoteSnapshot(body, forPeerId: peerId)
                            }
                        }
                    })
                case .failed, .cancelled:
                    Task { @MainActor in
                        // Only remove if this connection is still the
                        // current one — a newer connection may have
                        // already replaced it in the map.
                        if self?.activeConnections[peerId] === connection {
                            self?.activeConnections.removeValue(forKey: peerId)
                        }
                    }
                default:
                    break
                }
            }
        }
    }

    // MARK: - Helpers

    /// Single source of truth for reading local metrics from AppState.
    /// Previously duplicated in updatePeers (2x) and pollPeers.
    private func syncLocalPeerMetrics(_ peer: inout FleetPeer) {
        guard let state else { return }
        peer.cpuPercent = state.snapshot.host.cpuPercent
        peer.entityCount = state.snapshot.entities.count
        peer.friction = state.snapshot.entities.first?.friction.totalScore ?? 0
        peer.unsignedCount = state.snapshot.entities.filter {
            Self.isUnsigned(classification: $0.signingClassification, isAdhoc: $0.isAdhoc)
        }.count
        peer.lastSeen = .now
    }

    /// A binary is flagged when it is ad-hoc signed or explicitly unsigned.
    /// "unknown" means not-yet-classified (the amortized signing fill hasn't
    /// reached it), so it is deliberately not counted — that keeps the headline
    /// number honest while the engine's cache warms up.
    nonisolated static func isUnsigned(classification: String, isAdhoc: Bool) -> Bool {
        isAdhoc || classification == "unsigned"
    }

    private func applyRemoteSnapshot(_ data: Data, forPeerId peerId: String) {
        guard let index = peers.firstIndex(where: { $0.id == peerId }) else { return }
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }
        let host = json["host"] as? [String: Any] ?? [:]
        let entities = json["entities"] as? [[String: Any]] ?? []
        peers[index].cpuPercent = (host["cpu_percent"] as? NSNumber)?.floatValue ?? 0
        peers[index].entityCount = entities.count
        if let first = entities.first, let friction = first["friction"] as? [String: Any] {
            peers[index].friction = (friction["total_score"] as? NSNumber)?.floatValue ?? 0
        }
        // The peer serves its full snapshot, so each entity already carries its
        // signing verdict — count the flagged ones with no extra round trip.
        peers[index].unsignedCount = entities.filter { entity in
            let classification = entity["signing_classification"] as? String ?? "unknown"
            let isAdhoc = (entity["is_adhoc"] as? NSNumber)?.boolValue ?? false
            return Self.isUnsigned(classification: classification, isAdhoc: isAdhoc)
        }.count
        peers[index].lastSeen = .now
        activeConnections.removeValue(forKey: peerId)
    }

    nonisolated private static func extractHTTPBody(_ data: Data) -> Data? {
        guard let string = String(data: data, encoding: .utf8),
              let range = string.range(of: "\r\n\r\n") else { return nil }
        let bodyStart = string.distance(from: string.startIndex, to: range.upperBound)
        return data.dropFirst(bodyStart) as Data
    }
}
