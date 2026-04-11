import Foundation
import Network
import Observation
import AetowerBridge

@MainActor
@Observable
public final class FleetService {
    public private(set) var peers: [FleetPeer] = []
    public private(set) var isEnabled = false

    private var browser: NWBrowser?
    private var listener: NWListener?
    private var state: AppState?
    private var pollTimer: Timer?
    private var discoveredEndpoints: [String: NWEndpoint] = [:]

    private let serviceType = "_aetower._tcp"

    public struct FleetPeer: Identifiable {
        public let id: String
        public let name: String
        public var friction: Float = 0
        public var cpuPercent: Float = 0
        public var entityCount: Int = 0
        public var lastSeen: Date = .now
        public var isLocal: Bool = false
    }

    public func start(state: AppState) {
        guard !isEnabled else { return }
        self.state = state
        isEnabled = true
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
        peers.removeAll()
        discoveredEndpoints.removeAll()
    }

    // MARK: - Advertise

    private func startAdvertising(state: AppState) {
        do {
            let listener = try NWListener(using: .tcp)
            listener.service = NWListener.Service(type: serviceType)
            listener.newConnectionHandler = { [weak self, weak state] connection in
                Task { @MainActor in
                    self?.handleIncoming(connection, state: state)
                }
            }
            listener.start(queue: .main)
            self.listener = listener
        } catch {
            // Best-effort
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
        let browser = NWBrowser(for: .bonjour(type: serviceType, domain: nil), using: .tcp)
        browser.browseResultsChangedHandler = { [weak self] results, _ in
            Task { @MainActor in self?.updatePeers(from: results) }
        }
        browser.start(queue: .main)
        self.browser = browser
    }

    private func updatePeers(from results: Set<NWBrowser.Result>) {
        let localName = Host.current().localizedName ?? ProcessInfo.processInfo.hostName
        var updated: [FleetPeer] = []

        for result in results {
            guard case let .service(name, _, _, _) = result.endpoint else { continue }
            let id = name
            let isLocal = name == localName || name.hasPrefix(localName)

            discoveredEndpoints[id] = result.endpoint

            if let existing = peers.first(where: { $0.id == id }) {
                var peer = existing
                peer.isLocal = isLocal
                // For local peer, read directly from AppState
                if isLocal, let state {
                    peer.cpuPercent = state.snapshot.host.cpuPercent
                    peer.entityCount = state.snapshot.entities.count
                    peer.friction = state.snapshot.entities.first?.friction.totalScore ?? 0
                    peer.lastSeen = .now
                }
                updated.append(peer)
            } else {
                var peer = FleetPeer(id: id, name: name, isLocal: isLocal)
                if isLocal, let state {
                    peer.cpuPercent = state.snapshot.host.cpuPercent
                    peer.entityCount = state.snapshot.entities.count
                    peer.friction = state.snapshot.entities.first?.friction.totalScore ?? 0
                    peer.lastSeen = .now
                }
                updated.append(peer)
            }
        }

        peers = updated.sorted { $0.name < $1.name }
    }

    // MARK: - Poll

    private func pollPeers() {
        // Update local peer from AppState directly
        for (index, peer) in peers.enumerated() where peer.isLocal {
            guard let state else { continue }
            peers[index].cpuPercent = state.snapshot.host.cpuPercent
            peers[index].entityCount = state.snapshot.entities.count
            peers[index].friction = state.snapshot.entities.first?.friction.totalScore ?? 0
            peers[index].lastSeen = .now
        }

        // For remote peers, connect via NWConnection to their Bonjour endpoint
        for (index, peer) in peers.enumerated() where !peer.isLocal {
            guard let endpoint = discoveredEndpoints[peer.id] else { continue }
            let capturedIndex = index
            let connection = NWConnection(to: endpoint, using: .tcp)
            connection.start(queue: .main)
            connection.stateUpdateHandler = { [weak self] state in
                guard case .ready = state else { return }
                let request = Data("GET / HTTP/1.0\r\nHost: local\r\n\r\n".utf8)
                connection.send(content: request, completion: .contentProcessed { _ in
                    connection.receive(minimumIncompleteLength: 1, maximumLength: 1_048_576) { data, _, _, _ in
                        connection.cancel()
                        guard let data, let body = Self.extractHTTPBody(data) else { return }
                        Task { @MainActor in
                            self?.applyRemoteSnapshot(body, at: capturedIndex)
                        }
                    }
                })
            }
        }
    }

    private func applyRemoteSnapshot(_ data: Data, at index: Int) {
        guard index < peers.count else { return }
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }
        let host = json["host"] as? [String: Any] ?? [:]
        let entities = json["entities"] as? [[String: Any]] ?? []
        peers[index].cpuPercent = (host["cpu_percent"] as? NSNumber)?.floatValue ?? 0
        peers[index].entityCount = entities.count
        if let first = entities.first, let friction = first["friction"] as? [String: Any] {
            peers[index].friction = (friction["total_score"] as? NSNumber)?.floatValue ?? 0
        }
        peers[index].lastSeen = .now
    }

    nonisolated private static func extractHTTPBody(_ data: Data) -> Data? {
        guard let string = String(data: data, encoding: .utf8),
              let range = string.range(of: "\r\n\r\n") else { return nil }
        let bodyStart = string.distance(from: string.startIndex, to: range.upperBound)
        return data.dropFirst(bodyStart) as Data
    }
}
