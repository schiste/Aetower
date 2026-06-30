import AetowerBridge
import Foundation

private final class StorageScanMainActorPublisher: @unchecked Sendable {
    weak var state: AppState?

    init(_ state: AppState?) {
        self.state = state
    }

    @MainActor
    func publishJob(_ job: StorageScanJobResponseModel) {
        state?.publishStorageScanJob(job)
    }

    @MainActor
    func publishPrepared(_ prepared: PreparedStorageHygieneResult) {
        state?.publishPreparedStorageHygieneResult(prepared)
    }

    @MainActor
    func publishFailure(_ message: String) {
        state?.publishStorageScanFailure(message)
    }
}

@MainActor
final class StorageScanController {
    private let bridge: EngineBridge
    private weak var state: AppState?
    private var pollTask: Task<Void, Never>?
    private var activeJobId: String?

    init(bridge: EngineBridge) {
        self.bridge = bridge
    }

    func attach(_ state: AppState) {
        self.state = state
    }

    func start(
        roots: [String],
        maxDepth: UInt32,
        limit: UInt32,
        mode: String,
        throttleHint: String,
        dirtyPaths: [String]
    ) {
        if let activeJobId {
            _ = bridge.storageScanCancelJSON(jobId: activeJobId)
        }
        pollTask?.cancel()
        let bridge = self.bridge
        let publisher = StorageScanMainActorPublisher(state)
        pollTask = Task.detached(priority: .background) {
            let startResult = bridge.storageScanStartJSON(
                roots: roots,
                maxDepth: maxDepth,
                limit: limit,
                mode: mode,
                throttleHint: throttleHint,
                dirtyPaths: dirtyPaths
            )
            guard let startJob = Self.decodeJob(startResult) else {
                await publisher.publishFailure(
                    startResult.errorMessage ?? "Storage scan job could not be started."
                )
                return
            }
            await publisher.publishJob(startJob)
            await StorageScanController.selfPoll(
                bridge: bridge,
                publisher: publisher,
                jobId: startJob.jobId,
                roots: roots,
                maxDepth: maxDepth,
                limit: limit,
                mode: mode
            )
        }
    }

    func pause() {
        guard let activeJobId else { return }
        let bridge = self.bridge
        let publisher = StorageScanMainActorPublisher(state)
        Task.detached(priority: .utility) {
            guard let job = Self.decodeJob(bridge.storageScanPauseJSON(jobId: activeJobId)) else {
                await publisher.publishFailure("Storage scan could not be paused.")
                return
            }
            await publisher.publishJob(job)
        }
    }

    func resume() {
        guard let activeJobId else { return }
        let bridge = self.bridge
        let publisher = StorageScanMainActorPublisher(state)
        Task.detached(priority: .utility) {
            guard let job = Self.decodeJob(bridge.storageScanResumeJSON(jobId: activeJobId)) else {
                await publisher.publishFailure("Storage scan could not be resumed.")
                return
            }
            await publisher.publishJob(job)
        }
    }

    func cancel() {
        guard let activeJobId else { return }
        pollTask?.cancel()
        let bridge = self.bridge
        let publisher = StorageScanMainActorPublisher(state)
        Task.detached(priority: .utility) {
            guard let job = Self.decodeJob(bridge.storageScanCancelJSON(jobId: activeJobId)) else {
                await publisher.publishFailure("Storage scan could not be cancelled.")
                return
            }
            await publisher.publishJob(job)
        }
    }

    func stop() {
        pollTask?.cancel()
        if let activeJobId {
            _ = bridge.storageScanCancelJSON(jobId: activeJobId)
        }
        activeJobId = nil
    }

    func setActiveJobId(_ jobId: String?) {
        activeJobId = jobId
    }

    nonisolated private static func selfPoll(
        bridge: EngineBridge,
        publisher: StorageScanMainActorPublisher,
        jobId: String,
        roots: [String],
        maxDepth: UInt32,
        limit: UInt32,
        mode: String
    ) async {
        while !Task.isCancelled {
            try? await Task.sleep(nanoseconds: 350_000_000)
            guard !Task.isCancelled else { return }
            let statusResult = bridge.storageScanStatusJSON(jobId: jobId)
            guard let job = decodeJob(statusResult) else {
                await publisher.publishFailure(
                    statusResult.errorMessage ?? "Storage scan status could not be decoded."
                )
                return
            }
            await publisher.publishJob(job)
            if job.status == "complete" {
                let result = bridge.storageScanResultJSON(jobId: jobId)
                let prepared = StorageHygieneDecodePipeline.prepare(
                    result,
                    roots: roots,
                    maxDepth: maxDepth,
                    limit: limit,
                    mode: mode,
                    saveCache: true
                )
                await publisher.publishPrepared(prepared)
                return
            }
            if job.status == "failed" {
                await publisher.publishFailure(job.errorMessage ?? "Storage scan failed.")
                return
            }
            if job.status == "cancelled" {
                await publisher.publishFailure("Storage scan cancelled.")
                return
            }
        }
    }

    nonisolated private static func decodeJob(_ result: JsonQueryResult) -> StorageScanJobResponseModel? {
        guard let json = result.json, let data = json.data(using: .utf8) else {
            return nil
        }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try? decoder.decode(StorageScanJobResponseModel.self, from: data)
    }
}
