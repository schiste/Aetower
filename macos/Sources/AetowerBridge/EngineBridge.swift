import Foundation
import CAetowerFFI

public enum CapabilityKind: String, Codable, CaseIterable, Identifiable {
    case accessibility
    case fullDiskAccess = "full-disk-access"
    case appleAutomation = "apple-automation"
    case chromiumDebug = "chromium-debug"
    case dockerSocket = "docker-socket"

    public var id: String { rawValue }
}

public enum CapabilityState: String, Codable {
    case unknown
    case granted
    case denied
    case requested
    case unavailable
}

public enum EntityKind: String, Codable {
    case app
    case browser
    case daemon
    case terminalSession = "terminal-session"
    case service
    case unknown
}

public enum ComponentKind: String, Codable {
    case process
    case command
    case adapterContext = "adapter-context"
}

public enum TimelineSeverity: String, Codable {
    case info
    case warning
    case critical
}

public struct HostSnapshot: Codable {
    public var cpuPercent: Double
    public var memoryUsedBytes: UInt64
    public var memoryTotalBytes: UInt64
    public var swapUsedBytes: UInt64
    public var networkReceiveBps: UInt64
    public var networkSendBps: UInt64
    public var thermalState: String
    public var onBattery: Bool

    public init(
        cpuPercent: Double,
        memoryUsedBytes: UInt64,
        memoryTotalBytes: UInt64,
        swapUsedBytes: UInt64,
        networkReceiveBps: UInt64,
        networkSendBps: UInt64,
        thermalState: String,
        onBattery: Bool
    ) {
        self.cpuPercent = cpuPercent
        self.memoryUsedBytes = memoryUsedBytes
        self.memoryTotalBytes = memoryTotalBytes
        self.swapUsedBytes = swapUsedBytes
        self.networkReceiveBps = networkReceiveBps
        self.networkSendBps = networkSendBps
        self.thermalState = thermalState
        self.onBattery = onBattery
    }
}

public struct AggregateMetrics: Codable {
    public var cpuPercent: Double
    public var memoryResidentBytes: UInt64
    public var virtualMemoryBytes: UInt64
    public var diskReadBps: UInt64
    public var diskWriteBps: UInt64
    public var networkReceiveBps: UInt64
    public var networkSendBps: UInt64
    public var processCount: UInt32
    public var isForeground: Bool
}

public struct FrictionBreakdown: Codable {
    public var totalScore: Double
    public var cpuScore: Double
    public var memoryScore: Double
    public var diskScore: Double
    public var foregroundBonus: Double
    public var reasons: [String]
}

public struct ComponentSnapshot: Codable, Identifiable {
    public var id: String { "\(kind.rawValue):\(title)" }
    public var kind: ComponentKind
    public var title: String
    public var detail: String
    public var cpuPercent: Double
    public var memoryBytes: UInt64
}

public struct EntitySnapshot: Codable, Identifiable {
    public var id: String { entityId }
    public var entityId: String
    public var displayName: String
    public var bundleId: String?
    public var executablePath: String?
    public var entityKind: EntityKind
    public var metrics: AggregateMetrics
    public var friction: FrictionBreakdown
    public var components: [ComponentSnapshot]
    public var badges: [String]
}

public struct CapabilitySnapshot: Codable, Identifiable {
    public var id: String { kind.rawValue }
    public var kind: CapabilityKind
    public var state: CapabilityState
    public var detail: String
    public var lastUpdatedMillis: UInt64
}

public struct TimelineEvent: Codable, Identifiable {
    public var id: String
    public var timestampMillis: UInt64
    public var severity: TimelineSeverity
    public var entityId: String?
    public var title: String
    public var detail: String
}

public struct SystemSnapshot: Codable {
    public var sequence: UInt64
    public var capturedAtMillis: UInt64
    public var host: HostSnapshot
    public var capabilities: [CapabilitySnapshot]
    public var entities: [EntitySnapshot]
    public var timeline: [TimelineEvent]

    public init(
        sequence: UInt64,
        capturedAtMillis: UInt64,
        host: HostSnapshot,
        capabilities: [CapabilitySnapshot],
        entities: [EntitySnapshot],
        timeline: [TimelineEvent]
    ) {
        self.sequence = sequence
        self.capturedAtMillis = capturedAtMillis
        self.host = host
        self.capabilities = capabilities
        self.entities = entities
        self.timeline = timeline
    }
}

public final class EngineBridge {
    private let handle: UnsafeMutableRawPointer
    private let decoder: JSONDecoder

    public init() {
        self.handle = aetower_engine_new()
        self.decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        aetower_engine_start(handle)
    }

    deinit {
        aetower_engine_stop(handle)
        aetower_engine_free(handle)
    }

    public func latestSnapshot() throws -> SystemSnapshot {
        let json = try withCStringResult {
            aetower_engine_get_snapshot_json(handle)
        }
        let data = Data(json.utf8)
        return try decoder.decode(SystemSnapshot.self, from: data)
    }

    public func setCapability(_ kind: CapabilityKind, state: CapabilityState, detail: String? = nil) {
        kind.rawValue.withCString { kindPtr in
            state.rawValue.withCString { statePtr in
                if let detail {
                    detail.withCString { detailPtr in
                        aetower_engine_set_capability_state(handle, kindPtr, statePtr, detailPtr)
                    }
                } else {
                    aetower_engine_set_capability_state(handle, kindPtr, statePtr, nil)
                }
            }
        }
    }

    private func withCStringResult(_ body: () -> UnsafeMutablePointer<CChar>?) throws -> String {
        guard let pointer = body() else {
            return "{}"
        }
        defer { aetower_engine_free_string(pointer) }
        return String(cString: pointer)
    }
}
