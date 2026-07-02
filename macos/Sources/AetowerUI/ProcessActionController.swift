import Foundation
import Observation
import AetowerBridge

private struct ProcessActionControllerRequestEnvelopePayload: Encodable {
    let aetowerProcessActionContext = 1
    let actionID: String?
    let reason: String?
    let expectedTargets: [ProcessActionControllerTargetIdentityPayload]
    let restoreNiceValue: Int?
    let privilegedHelperApproved: Bool
}

private struct ProcessActionControllerTargetIdentityPayload: Encodable {
    let pid: UInt32
    let startTimeMillis: UInt64?
    let executablePath: String?
    let displayName: String?
    let niceValue: Int?

    init(_ identity: ProcessActionTargetIdentityModel) {
        self.pid = identity.pid
        self.startTimeMillis = identity.startTimeMillis
        self.executablePath = identity.executablePath
        self.displayName = identity.displayName
        self.niceValue = identity.niceValue
    }
}

@MainActor
@Observable
final class ProcessActionController {
    private(set) var processActionPreviewReports: [String: ProcessActionReportModel] = [:]
    private(set) var processActionReports: [UInt32: ProcessActionReportModel] = [:]
    private(set) var processActionHistory: ProcessActionHistoryReportModel?

    @ObservationIgnored
    private let bridge: EngineBridge
    @ObservationIgnored
    private var processActionPreviewActionIDs: [String: String] = [:]
    @ObservationIgnored
    private var loadingKeys = Set<String>()
    @ObservationIgnored
    private var errorMessages: [String: String] = [:]
    @ObservationIgnored
    private var updatedAtByKey: [String: Date] = [:]

    init(bridge: EngineBridge) {
        self.bridge = bridge
    }

    func isLoading(_ entityID: String, kind: EntityAnalysisKind) -> Bool {
        loadingKeys.contains(entityAnalysisKey(entityID, kind: kind))
    }

    func error(_ entityID: String, kind: EntityAnalysisKind) -> String? {
        errorMessages[entityAnalysisKey(entityID, kind: kind)]
    }

    func updatedAt(_ entityID: String, kind: EntityAnalysisKind) -> Date? {
        updatedAtByKey[entityAnalysisKey(entityID, kind: kind)]
    }

    func runProcessAction(
        pid: UInt32,
        action: ProcessActionKind,
        reason: String? = nil,
        actionID: String? = nil,
        expectedTargets: [ProcessActionTargetIdentityModel] = [],
        restoreNiceValue: Int? = nil,
        privilegedHelperApproved: Bool = false
    ) {
        let encodedReason = Self.processActionRequestEnvelope(
            reason: reason,
            actionID: actionID,
            expectedTargets: expectedTargets,
            restoreNiceValue: restoreNiceValue,
            privilegedHelperApproved: privilegedHelperApproved
        )
        processActionReports[pid] = nil
        setLoading(processAnalysisKey(pid), kind: .processAction, isLoading: true)
        Task(priority: .utility) { [weak self, bridge] in
            let result = bridge.processActionJSON(
                pid: pid,
                action: action.rawValue,
                dryRun: false,
                reason: encodedReason
            )
            let historyResult = bridge.processActionHistoryJSON()
            await MainActor.run {
                guard let self else { return }
                self.processActionReports[pid] = self.decodeJsonQueryResult(
                    result,
                    as: ProcessActionReportModel.self
                )
                self.finish(
                    self.processAnalysisKey(pid),
                    kind: .processAction,
                    result: result,
                    fallback: "Process action failed."
                )
                self.processActionHistory = self.decodeJsonQueryResult(
                    historyResult,
                    as: ProcessActionHistoryReportModel.self
                )
            }
        }
    }

    func runVerifiedProcessAction(
        pid: UInt32,
        action: ProcessActionKind,
        reason: String? = nil,
        actionID: String? = nil,
        privilegedHelperApproved: Bool = false
    ) {
        let resolvedActionID = Self.trimmedNonEmpty(actionID) ?? UUID().uuidString
        let previewReason = Self.processActionRequestEnvelope(
            reason: reason,
            actionID: resolvedActionID,
            expectedTargets: [],
            restoreNiceValue: nil,
            privilegedHelperApproved: false
        )
        processActionReports[pid] = nil
        setLoading(processAnalysisKey(pid), kind: .processAction, isLoading: true)
        Task(priority: .utility) { [weak self, bridge] in
            let previewResult = bridge.processActionJSON(
                pid: pid,
                action: action.rawValue,
                dryRun: true,
                reason: previewReason
            )
            let decoder = AetowerJSON.snakeCaseDecoder()

            guard previewResult.errorMessage == nil,
                  let previewJson = previewResult.json,
                  let previewData = previewJson.data(using: .utf8),
                  let preview = try? decoder.decode(ProcessActionReportModel.self, from: previewData)
            else {
                await MainActor.run {
                    guard let self else { return }
                    self.finish(
                        self.processAnalysisKey(pid),
                        kind: .processAction,
                        result: previewResult,
                        fallback: "Process action preview failed before the signal was sent."
                    )
                }
                return
            }

            guard let expectedTargets = preview.targetIdentities,
                  !expectedTargets.isEmpty
            else {
                await MainActor.run {
                    guard let self else { return }
                    self.finishWithError(
                        self.processAnalysisKey(pid),
                        kind: .processAction,
                        message: "Signal not sent: Aetower could not capture target identity for PID \(pid)."
                    )
                }
                return
            }

            let encodedReason = Self.processActionRequestEnvelope(
                reason: reason,
                actionID: resolvedActionID,
                expectedTargets: expectedTargets,
                restoreNiceValue: nil,
                privilegedHelperApproved: privilegedHelperApproved
            )
            let result = bridge.processActionJSON(
                pid: pid,
                action: action.rawValue,
                dryRun: false,
                reason: encodedReason
            )
            let historyResult = bridge.processActionHistoryJSON()
            await MainActor.run {
                guard let self else { return }
                self.processActionReports[pid] = self.decodeJsonQueryResult(
                    result,
                    as: ProcessActionReportModel.self
                )
                self.finish(
                    self.processAnalysisKey(pid),
                    kind: .processAction,
                    result: result,
                    fallback: "Process action failed."
                )
                self.processActionHistory = self.decodeJsonQueryResult(
                    historyResult,
                    as: ProcessActionHistoryReportModel.self
                )
            }
        }
    }

    func runProcessActionPreview(
        pid: UInt32,
        action: ProcessActionKind,
        reason: String? = nil,
        actionID: String? = nil
    ) {
        let previewKey = processActionPreviewKey(pid: pid, action: action)
        let resolvedActionID = Self.trimmedNonEmpty(actionID) ?? UUID().uuidString
        let encodedReason = Self.processActionRequestEnvelope(
            reason: reason,
            actionID: resolvedActionID,
            expectedTargets: [],
            restoreNiceValue: nil,
            privilegedHelperApproved: false
        )
        processActionPreviewActionIDs[previewKey] = resolvedActionID
        processActionPreviewReports[previewKey] = nil
        setLoading(processAnalysisKey(pid), kind: .processAction, isLoading: true)
        Task(priority: .utility) { [weak self, bridge] in
            let result = bridge.processActionJSON(
                pid: pid,
                action: action.rawValue,
                dryRun: true,
                reason: encodedReason
            )
            await MainActor.run {
                guard let self else { return }
                guard self.processActionPreviewActionIDs[previewKey] == resolvedActionID else {
                    return
                }
                self.processActionPreviewReports[previewKey] = self.decodeJsonQueryResult(
                    result,
                    as: ProcessActionReportModel.self
                )
                self.finish(
                    self.processAnalysisKey(pid),
                    kind: .processAction,
                    result: result,
                    fallback: "Process action preview failed."
                )
            }
        }
    }

    func processActionPreview(
        pid: UInt32,
        action: ProcessActionKind,
        matchingActionID: String? = nil
    ) -> ProcessActionReportModel? {
        let report = processActionPreviewReports[processActionPreviewKey(pid: pid, action: action)]
        guard let matchingActionID else { return report }
        return report?.actionId == matchingActionID ? report : nil
    }

    func clearProcessActionPreview(
        pid: UInt32,
        action: ProcessActionKind,
        cancelLoading: Bool = false
    ) {
        let previewKey = processActionPreviewKey(pid: pid, action: action)
        processActionPreviewReports.removeValue(forKey: previewKey)
        processActionPreviewActionIDs.removeValue(forKey: previewKey)
        if cancelLoading {
            setLoading(processAnalysisKey(pid), kind: .processAction, isLoading: false)
        }
    }

    func refreshProcessActionHistory(windowMinutes: UInt32 = 60, limit: UInt32 = 25) {
        setLoading("process-actions", kind: .processActionHistory, isLoading: true)
        Task(priority: .utility) { [weak self, bridge] in
            let result = bridge.processActionHistoryJSON(
                windowMinutes: windowMinutes,
                limit: limit
            )
            await MainActor.run {
                guard let self else { return }
                self.processActionHistory = self.decodeJsonQueryResult(
                    result,
                    as: ProcessActionHistoryReportModel.self
                )
                self.finish(
                    "process-actions",
                    kind: .processActionHistory,
                    result: result,
                    fallback: "Process action history could not be loaded."
                )
            }
        }
    }

    private func processActionPreviewKey(pid: UInt32, action: ProcessActionKind) -> String {
        "\(pid)|\(action.rawValue)"
    }

    private static func processActionRequestEnvelope(
        reason: String?,
        actionID: String?,
        expectedTargets: [ProcessActionTargetIdentityModel],
        restoreNiceValue: Int?,
        privilegedHelperApproved: Bool
    ) -> String? {
        let trimmedReason = trimmedNonEmpty(reason)
        let hasMetadata = trimmedNonEmpty(actionID) != nil
            || !expectedTargets.isEmpty
            || restoreNiceValue != nil
            || privilegedHelperApproved
        guard hasMetadata else {
            return trimmedReason
        }
        let payload = ProcessActionControllerRequestEnvelopePayload(
            actionID: actionID,
            reason: trimmedReason,
            expectedTargets: expectedTargets.map(ProcessActionControllerTargetIdentityPayload.init),
            restoreNiceValue: restoreNiceValue,
            privilegedHelperApproved: privilegedHelperApproved
        )
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        guard let data = try? encoder.encode(payload),
              let json = String(data: data, encoding: .utf8)
        else {
            return trimmedReason
        }
        return json
    }

    private func processAnalysisKey(_ pid: UInt32) -> String {
        "pid:\(pid)"
    }

    private func entityAnalysisKey(_ entityID: String, kind: EntityAnalysisKind) -> String {
        "\(entityID)|\(kind.rawValue)"
    }

    private func setLoading(_ entityID: String, kind: EntityAnalysisKind, isLoading: Bool) {
        let key = entityAnalysisKey(entityID, kind: kind)
        if isLoading {
            loadingKeys.insert(key)
            errorMessages[key] = nil
        } else {
            loadingKeys.remove(key)
        }
    }

    private func finish(
        _ entityID: String,
        kind: EntityAnalysisKind,
        result: JsonQueryResult,
        fallback: String
    ) {
        let key = entityAnalysisKey(entityID, kind: kind)
        loadingKeys.remove(key)
        errorMessages[key] = jsonQueryErrorMessage(result, fallback: fallback)
        if errorMessages[key] == nil {
            updatedAtByKey[key] = Date()
        }
    }

    private func finishWithError(
        _ entityID: String,
        kind: EntityAnalysisKind,
        message: String
    ) {
        let key = entityAnalysisKey(entityID, kind: kind)
        loadingKeys.remove(key)
        errorMessages[key] = message
    }

    private func jsonDecoder() -> JSONDecoder {
        AetowerJSON.snakeCaseDecoder()
    }

    private func decodeJsonQueryResult<T: Decodable>(_ result: JsonQueryResult, as type: T.Type) -> T? {
        guard let json = result.json, let data = json.data(using: .utf8) else {
            return nil
        }
        return try? jsonDecoder().decode(T.self, from: data)
    }

    private func jsonQueryErrorMessage(_ result: JsonQueryResult, fallback: String?) -> String? {
        if let error = result.errorMessage, !error.isEmpty {
            return error
        }
        guard let json = result.json, !json.isEmpty else {
            return fallback
        }
        return nil
    }

    private static func trimmedNonEmpty(_ value: String?) -> String? {
        let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return trimmed.isEmpty ? nil : trimmed
    }
}
