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

    private let serviceType = "_aetower._tcp"

    public struct FleetPeer: Identifiable {
        public let id: String
        public let name: String
        public let host: String
        public let port: UInt16
        public var friction: Float = 0
        public var cpuPercent: Float = 0
        public var entityCount: Int = 0
        public var lastSeen: Date = .now
    }

    public func start(state: AppState) {
        guard !isEnabled else { return }
        self.state = state
        isEnabled = true
        startAdvertising(state: state)
        startBrowsing()
        pollTimer = Timer.scheduledTimer(withTimeInterval: 10, repeats: true) { [weak self] _ in
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
    }

    // MARK: - Advertise local snapshot

    private func startAdvertising(state: AppState) {
        do {
            let listener = try NWListener(using: .tcp)
            listener.service = NWListener.Service(type: serviceType)
            listener.newConnectionHandler = { [weak self, weak state] connection in
                Task { @MainActor in
                    self?.handleIncomingConnection(connection, state: state)
                }
            }
            listener.start(queue: .main)
            self.listener = listener
        } catch {
            // Best-effort: fleet works without advertising
        }
    }

    private func handleIncomingConnection(_ connection: NWConnection, state: AppState?) {
        connection.start(queue: .main)
        connection.receive(minimumIncompleteLength: 1, maximumLength: 1024) { [weak state] _, _, _, _ in
            guard let state else {
                connection.cancel()
                return
            }
            let json = state.exportSnapshotJSON()
            let data = Data(json.utf8)
            let header = "HTTP/1.1 200 OK\r\nContent-Length: \(data.count)\r\nContent-Type: application/json\r\n\r\n"
            connection.send(content: Data(header.utf8) + data, completion: .contentProcessed { _ in
                connection.cancel()
            })
        }
    }

    // MARK: - Discover peers

    private func startBrowsing() {
        let browser = NWBrowser(for: .bonjour(type: serviceType, domain: nil), using: .tcp)
        browser.browseResultsChangedHandler = { [weak self] results, _ in
            Task { @MainActor in
                self?.updatePeers(from: results)
            }
        }
        browser.start(queue: .main)
        self.browser = browser
    }

    private func updatePeers(from results: Set<NWBrowser.Result>) {
        var updated: [FleetPeer] = []
        for result in results {
            guard case let .service(name, _, _, _) = result.endpoint else { continue }
            let id = "\(name)-\(result.hashValue)"
            if let existing = peers.first(where: { $0.id == id }) {
                updated.append(existing)
            } else {
                updated.append(FleetPeer(id: id, name: name, host: "", port: 0))
            }
        }
        peers = updated
    }

    // MARK: - Poll peer snapshots

    private func pollPeers() {
        for (index, peer) in peers.enumerated() {
            guard !peer.host.isEmpty else { continue }
            Task {
                if let data = try? await fetchSnapshot(host: peer.host, port: peer.port) {
                    await MainActor.run {
                        if index < self.peers.count {
                            self.peers[index].friction = data.friction
                            self.peers[index].cpuPercent = data.cpu
                            self.peers[index].entityCount = data.entityCount
                            self.peers[index].lastSeen = .now
                        }
                    }
                }
            }
        }
    }

    private struct PeerSummary {
        let friction: Float
        let cpu: Float
        let entityCount: Int
    }

    private func fetchSnapshot(host: String, port: UInt16) async throws -> PeerSummary {
        let url = URL(string: "http://\(host):\(port)/")!
        let (data, _) = try await URLSession.shared.data(from: url)
        let json = try JSONSerialization.jsonObject(with: data) as? [String: Any] ?? [:]
        let host = json["host"] as? [String: Any] ?? [:]
        let entities = json["entities"] as? [[String: Any]] ?? []
        return PeerSummary(
            friction: (host["cpu_percent"] as? Float) ?? 0,
            cpu: (host["cpu_percent"] as? Float) ?? 0,
            entityCount: entities.count
        )
    }
}
