import AppKit
import Foundation
import UniformTypeIdentifiers
import AetowerBridge

struct ExportSupportBundleContext {
    let diagnosticsOverview: DiagnosticsOverview
    let diagnosticsRecentWarningCount: Int
    let diagnosticsRecentErrorCount: Int
    let runtimeLagMetrics: RuntimeLagMetrics
    let uiPerformanceBudgetDiagnostics: UiPerformanceBudgetDiagnosticsSummary
    let notificationAuthorizationStatus: String
    let lastDiagnosticsQueryDate: Date?
    let lastSessionLogAnalysisCompletedDate: Date?
    let sessionLogAnalysisError: String?
    let sessionLogSummary: SessionLogSummary?
    let historyWindowSeconds: TimeInterval
    let historySnapshots: [SystemSnapshot]
    let historyLoadError: String?
    let historyRangeSummary: HistoryRangeSummary?
    let historyMaintenanceReport: HistoryMaintenanceReport?
    let telemetryVerificationStatus: String?
}

enum ExportSupportBundleResult {
    case success(URL)
    case cancelled
    case failure(String)
}

@MainActor
final class ExportController {
    private let bridge: EngineBridge

    init(bridge: EngineBridge) {
        self.bridge = bridge
    }

    func exportSnapshotJSON() -> String {
        bridge.exportSnapshotJSON()
    }

    func exportSnapshot(privacyTier: ExportPrivacyTier) {
        let json = exportControlledJson(
            bridge.exportSnapshotJSON(),
            privacyTier: privacyTier
        )
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.json]
        panel.nameFieldStringValue = "aetower-snapshot.json"
        if panel.runModal() == .OK, let url = panel.url {
            try? json.write(to: url, atomically: true, encoding: .utf8)
        }
    }

    func exportSnapshotCSV(snapshot: SystemSnapshot) {
        let csv = snapshotCSV(snapshot: snapshot)
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.commaSeparatedText]
        panel.nameFieldStringValue = "aetower-processes.csv"
        if panel.runModal() == .OK, let url = panel.url {
            try? csv.write(to: url, atomically: true, encoding: .utf8)
        }
    }

    func snapshotCSV(snapshot: SystemSnapshot) -> String {
        let header = [
            "entity", "entity_id", "pid", "process", "cpu_percent", "memory_bytes",
            "memory_footprint_bytes", "user", "start_time_millis", "executable_path",
            "command_line", "entity_friction", "entity_energy_nj_per_s",
        ]
        var rows = [header.map(csvField).joined(separator: ",")]
        for entity in snapshot.entities {
            for component in entity.components where component.kind != .adapterContext {
                guard let pid = component.processId else { continue }
                let fields = [
                    entity.displayName,
                    entity.entityId,
                    String(pid),
                    component.title,
                    String(format: "%.1f", component.cpuPercent),
                    String(component.memoryBytes),
                    String(component.memoryPhysicalFootprintBytes),
                    component.user ?? "",
                    String(component.startTimeMillis),
                    component.executablePath ?? "",
                    component.commandLine ?? "",
                    String(format: "%.1f", entity.friction.totalScore),
                    String(format: "%.0f", entity.metrics.energyNjPerS),
                ]
                rows.append(fields.map(csvField).joined(separator: ","))
            }
        }
        return rows.joined(separator: "\n")
    }

    func exportDiagnostics(privacyTier: ExportPrivacyTier, limit: UInt32 = 1000) {
        let json = exportControlledJson(
            bridge.exportDiagnosticsQueryJSON(
                DiagnosticsQuery(
                    limit: limit,
                    minimumLevel: nil,
                    subsystem: nil,
                    search: nil,
                    sinceMillis: nil,
                    includePersisted: true
                )
            ),
            privacyTier: privacyTier
        )
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.json]
        panel.nameFieldStringValue = "aetower-diagnostics.json"
        if panel.runModal() == .OK, let url = panel.url {
            try? json.write(to: url, atomically: true, encoding: .utf8)
        }
    }

    func exportSupportBundle(
        _ settings: SettingsStore,
        diagnosticsLimit: UInt32,
        context: ExportSupportBundleContext
    ) -> ExportSupportBundleResult {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = supportBundleDirectoryName()
        guard panel.runModal() == .OK, let url = panel.url else {
            return .cancelled
        }

        do {
            let packageURL = normalizedSupportBundleURL(url)
            try writeSupportBundlePackage(
                to: packageURL,
                settings: settings,
                diagnosticsLimit: diagnosticsLimit,
                context: context
            )
            return .success(packageURL)
        } catch {
            return .failure("Support bundle export failed: \(error.localizedDescription)")
        }
    }

    private func csvField(_ value: String) -> String {
        if value.contains(",") || value.contains("\"") || value.contains("\n") || value.contains("\r") {
            return "\"\(value.replacingOccurrences(of: "\"", with: "\"\""))\""
        }
        return value
    }

    private func supportBundlePayload(
        _ settings: SettingsStore,
        diagnosticsLimit: UInt32,
        context: ExportSupportBundleContext
    ) throws -> [String: Any] {
        let privacyTier = settings.exportPrivacyTier
        let snapshotJson = exportControlledJson(
            bridge.exportSnapshotJSON(),
            privacyTier: privacyTier
        )
        let diagnosticsJson = exportControlledJson(
            bridge.exportDiagnosticsQueryJSON(
                DiagnosticsQuery(
                    limit: diagnosticsLimit,
                    minimumLevel: nil,
                    subsystem: nil,
                    search: nil,
                    sinceMillis: nil,
                    includePersisted: true
                )
            ),
            privacyTier: privacyTier
        )

        return [
            "generatedAtMillis": UInt64(Date().timeIntervalSince1970 * 1000),
            "privacyTier": privacyTier.rawValue,
            "redacted": privacyTier != .full,
            "app": appMetadata(),
            "settings": settingsSummary(settings, telemetryVerificationStatus: context.telemetryVerificationStatus),
            "diagnosticsOverview": diagnosticsOverviewSummary(context),
            "runtimeLagMetrics": runtimeLagSummary(context.runtimeLagMetrics),
            "uiPerformanceBudget": uiPerformanceBudgetSummary(context.uiPerformanceBudgetDiagnostics),
            "sessionHealth": sessionHealthSummary(context),
            "historySummary": historySummary(context),
            "snapshot": try parseJsonObject(snapshotJson),
            "diagnostics": try parseJsonValue(diagnosticsJson),
        ]
    }

    private func appMetadata() -> [String: Any] {
        let info = Bundle.main.infoDictionary ?? [:]
        return [
            "bundleIdentifier": Bundle.main.bundleIdentifier ?? "unknown",
            "buildVersion": info["CFBundleVersion"] as? String ?? "unknown",
            "shortVersion": info["CFBundleShortVersionString"] as? String ?? "unknown",
            "processIdentifier": ProcessInfo.processInfo.processIdentifier,
            "processName": ProcessInfo.processInfo.processName,
        ]
    }

    private func settingsSummary(
        _ settings: SettingsStore,
        telemetryVerificationStatus: String?
    ) -> [String: Any] {
        let privacyTier = settings.exportPrivacyTier
        return [
            "refreshIntervalSeconds": settings.refreshIntervalSeconds,
            "showMenuBarExtra": settings.showMenuBarExtra,
            "notificationsEnabled": settings.notificationsEnabled,
            "frictionNotificationThreshold": settings.frictionNotificationThreshold,
            "appearanceMode": settings.appearanceMode,
            "operatorSafeModeEnabled": settings.operatorSafeModeEnabled,
            "monitorMetricCardPlacement": settings.monitorMetricCardPlacement.rawValue,
            "monitorMetricCardFocus": settings.monitorMetricCardFocus.rawValue,
            "chromiumEndpointConfigured": !settings.chromiumEndpoint.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
            "dockerSocketPath": exportControlledValue(
                settings.dockerSocketPath,
                privacyTier: privacyTier,
                key: "dockerSocketPath"
            ),
            "privilegedHelperEnabled": settings.privilegedHelperEnabled,
            "privilegedHelperPathConfigured": !settings.privilegedHelperPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
            "chau7EndpointConfigured": !settings.chau7Endpoint.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
            "chau7AgentCommand": exportControlledValue(
                settings.chau7AgentCommand,
                privacyTier: privacyTier,
                key: "chau7AgentCommand"
            ),
            "telemetryEnabled": settings.telemetryEnabled,
            "telemetryEndpoint": exportControlledValue(
                settings.telemetryEndpoint,
                privacyTier: privacyTier,
                key: "telemetryEndpoint"
            ),
            "telemetryExportIntervalSeconds": settings.telemetryExportIntervalSeconds,
            "telemetryVerificationStatus": telemetryVerificationStatus ?? "not-run",
            "collectionProfile": settings.collectionProfile.rawValue,
            "adaptiveCadenceEnabled": settings.adaptiveCadenceEnabled,
            "engineActiveIntervalSeconds": settings.engineActiveIntervalSeconds,
            "engineIdleIntervalSeconds": settings.engineIdleIntervalSeconds,
            "engineLowPowerIntervalSeconds": settings.engineLowPowerIntervalSeconds,
            "gpuSampleIntervalSeconds": settings.gpuSampleIntervalSeconds,
            "gpuSampleLowPowerIntervalSeconds": settings.gpuSampleLowPowerIntervalSeconds,
            "exportPrivacyTier": settings.exportPrivacyTier.rawValue,
            "autoRegisterLocalMcpClientsEnabled": settings.autoRegisterLocalMcpClientsEnabled,
            "localMcpOperatorActionsEnabled": settings.localMcpOperatorActionsEnabled,
        ]
    }

    private func diagnosticsOverviewSummary(_ context: ExportSupportBundleContext) -> [String: Any] {
        let overview = context.diagnosticsOverview
        return [
            "ringCapacity": overview.ringCapacity,
            "currentSize": overview.currentSize,
            "droppedEvents": overview.droppedEvents,
            "warnCount": overview.warnCount,
            "errorCount": overview.errorCount,
            "recentWarnCount": context.diagnosticsRecentWarningCount,
            "recentErrorCount": context.diagnosticsRecentErrorCount,
            "lastEventMillis": jsonOptional(overview.lastEventMillis),
            "lastErrorMillis": jsonOptional(overview.lastErrorMillis),
            "lastErrorMessage": jsonOptional(overview.lastErrorMessage),
            "persistedEvents": overview.persistedEvents,
            "persistedPath": jsonOptional(overview.persistedPath),
            "persistedBytes": overview.persistedBytes,
            "persistenceError": jsonOptional(overview.persistenceError),
        ]
    }

    private func runtimeLagSummary(_ runtime: RuntimeLagMetrics) -> [String: Any] {
        [
            "updatedAtMillis": runtime.updatedAtMillis,
            "engineTickMillis": runtime.engineTickMillis,
            "collectMillis": runtime.collectMillis,
            "identityMillis": runtime.identityMillis,
            "attributionMillis": runtime.attributionMillis,
            "frictionMillis": runtime.frictionMillis,
            "enrichMillis": runtime.enrichMillis,
            "historyMillis": runtime.historyMillis,
            "persistMillis": runtime.persistMillis,
            "gpuSampleMillis": runtime.gpuSampleMillis,
            "targetTickMillis": runtime.targetTickMillis,
            "historyQueueDepth": runtime.historyQueueDepth,
            "diagnosticsQueueDepth": runtime.diagnosticsQueueDepth,
            "selfCpuPercent": runtime.selfCpuPercent,
            "selfMemoryBytes": runtime.selfMemoryBytes,
            "selfMemoryPhysicalFootprintBytes": runtime.selfMemoryPhysicalFootprintBytes,
            "selfWakeupsPerSecond": runtime.selfWakeupsPerSecond,
            "selfEnergyNjPerS": runtime.selfEnergyNjPerS,
            "mcpHelperCount": runtime.mcpHelperCount,
            "staleMcpHelperCount": runtime.staleMcpHelperCount,
            "oldestMcpHelperAgeMillis": runtime.oldestMcpHelperAgeMillis,
            "mcpTotalConnections": runtime.mcpTotalConnections,
            "mcpActiveClientCount": runtime.mcpActiveClientCount,
            "mcpTotalRequests": runtime.mcpTotalRequests,
            "mcpRequestsPerSecond": runtime.mcpRequestsPerSecond,
            "mcpObservedAtMillis": runtime.mcpObservedAtMillis,
            "bridgeFetchMillis": runtime.bridgeFetchMillis,
            "uiRefreshMillis": runtime.uiRefreshMillis,
            "snapshotToUiMillis": runtime.snapshotToUiMillis,
            "snapshotToRenderMillis": runtime.snapshotToRenderMillis,
            "renderCommitMillis": runtime.renderCommitMillis,
            "displayFrameIntervalMillis": runtime.displayFrameIntervalMillis,
            "displayRefreshHz": runtime.displayRefreshHz,
            "displayDroppedFrames": runtime.displayDroppedFrames,
            "inputAvgLatencyMillis": runtime.inputAvgLatencyMillis,
            "inputMaxLatencyMillis": runtime.inputMaxLatencyMillis,
            "inputSampleCount": runtime.inputSampleCount,
        ]
    }

    private func uiPerformanceBudgetSummary(
        _ budget: UiPerformanceBudgetDiagnosticsSummary
    ) -> [String: Any] {
        [
            "updatedAtMillis": jsonOptional(budget.updatedAt.map { UInt64($0.timeIntervalSince1970 * 1000) }),
            "status": budget.status,
            "ffiFetchMillis": budget.ffiFetchMillis,
            "decodeMillis": budget.decodeMillis,
            "rowBuildMillis": budget.rowBuildMillis,
            "renderPublishMillis": budget.renderPublishMillis,
            "totalMeasuredMillis": budget.totalMeasuredMillis,
            "visibleRowCount": budget.visibleRowCount,
            "snapshotBytes": budget.snapshotBytes,
            "compactPayloadKind": budget.compactPayloadKind,
        ]
    }

    private func sessionHealthSummary(_ context: ExportSupportBundleContext) -> [String: Any] {
        [
            "notificationAuthorizationStatus": context.notificationAuthorizationStatus,
            "diagnosticsQueriedAtMillis": jsonOptional(context.lastDiagnosticsQueryDate.map {
                UInt64($0.timeIntervalSince1970 * 1000)
            }),
            "sessionLogAnalyzedAtMillis": jsonOptional(context.lastSessionLogAnalysisCompletedDate.map {
                UInt64($0.timeIntervalSince1970 * 1000)
            }),
            "sessionLogAnalysisError": jsonOptional(context.sessionLogAnalysisError),
            "sessionLogSummary": jsonOptional(context.sessionLogSummary.map { summary in
                [
                    "windowMinutes": summary.windowMinutes,
                    "notificationLogEntries": summary.notificationLogEntries,
                    "notificationAuthorizationFailures": summary.notificationAuthorizationFailures,
                    "metalLoadFailures": summary.metalLoadFailures,
                    "nonActiveWindowWarnings": summary.nonActiveWindowWarnings,
                    "cursorUiEntries": summary.cursorUiEntries,
                    "tccAccessRequests": summary.tccAccessRequests,
                    "viewBridgeCancellationCount": summary.viewBridgeCancellationCount,
                    "invalidDisplayIdentifierCount": summary.invalidDisplayIdentifierCount,
                ]
            }),
        ]
    }

    private func historySummary(_ context: ExportSupportBundleContext) -> [String: Any] {
        let recentSnapshots = context.historySnapshots.suffix(24).map { snapshot in
            [
                "sequence": snapshot.sequence,
                "capturedAtMillis": snapshot.capturedAtMillis,
                "entityCount": snapshot.entities.count,
                "topEntity": jsonOptional(snapshot.entities.first?.displayName),
                "topFriction": jsonOptional(snapshot.entities.first.map { Double($0.friction.totalScore) }),
            ]
        }
        return [
            "windowSeconds": context.historyWindowSeconds,
            "loadedSnapshots": context.historySnapshots.count,
            "historyLoadError": jsonOptional(context.historyLoadError),
            "rangeSummary": jsonOptional(context.historyRangeSummary.map { summary in
                [
                    "storeBytes": summary.storeBytes,
                    "walBytes": summary.walBytes,
                    "snapshotCount": summary.snapshotCount,
                    "quarantineCount": summary.quarantineCount,
                    "rangeCount": summary.rangeCount,
                ]
            }),
            "maintenance": jsonOptional(context.historyMaintenanceReport.map { maintenance in
                [
                    "storeBytesBefore": maintenance.storeBytesBefore,
                    "walBytesBefore": maintenance.walBytesBefore,
                    "storeBytesAfter": maintenance.storeBytesAfter,
                    "walBytesAfter": maintenance.walBytesAfter,
                    "checkpointed": maintenance.checkpointed,
                    "vacuumed": maintenance.vacuumed,
                    "prunedRows": maintenance.prunedRows,
                    "aggressiveReason": jsonOptional(maintenance.aggressiveReason),
                ]
            }),
            "recentSnapshots": recentSnapshots,
        ]
    }

    private func jsonOptional(_ value: Any?) -> Any {
        value ?? NSNull()
    }

    private func parseJsonObject(_ json: String) throws -> [String: Any] {
        guard let object = try parseJsonValue(json) as? [String: Any] else {
            throw NSError(
                domain: "AetowerSupportBundle",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "Expected JSON object while building support bundle."]
            )
        }
        return object
    }

    private func parseJsonValue(_ json: String) throws -> Any {
        try JSONSerialization.jsonObject(with: Data(json.utf8))
    }

    private func exportControlledJson(_ json: String, privacyTier: ExportPrivacyTier) -> String {
        guard privacyTier != .full else {
            return json
        }
        guard let value = try? parseJsonValue(json) else {
            return json
        }
        let redacted = exportControlledValue(value, privacyTier: privacyTier)
        guard let data = try? JSONSerialization.data(
            withJSONObject: redacted,
            options: [.prettyPrinted, .sortedKeys]
        ),
              let encoded = String(data: data, encoding: .utf8)
        else {
            return json
        }
        return encoded
    }

    private func exportControlledValue(
        _ value: Any,
        privacyTier: ExportPrivacyTier,
        key: String? = nil
    ) -> Any {
        let normalizedKey = key?.lowercased() ?? ""
        if privacyTier == .redacted && isSensitiveExportKey(normalizedKey) {
            return "<redacted>"
        }

        if let dictionary = value as? [String: Any] {
            var redacted = [String: Any](minimumCapacity: dictionary.count)
            let eventIsSensitive = (dictionary["sensitive"] as? Bool) ?? false
            for (childKey, childValue) in dictionary {
                if eventIsSensitive, childKey == "message" {
                    redacted[childKey] = privacyTier == .operatorMode
                        ? "<sensitive event>"
                        : "<redacted sensitive event>"
                    continue
                }
                if eventIsSensitive, childKey == "fields" {
                    redacted[childKey] = privacyTier == .operatorMode
                        ? redactSensitiveFieldsOperatorMode(dictionary["fields"])
                        : []
                    continue
                }
                redacted[childKey] = exportControlledValue(
                    childValue,
                    privacyTier: privacyTier,
                    key: childKey
                )
            }
            return redacted
        }

        if let array = value as? [Any] {
            return array.map { child in
                exportControlledValue(child, privacyTier: privacyTier, key: key)
            }
        }

        if let string = value as? String {
            switch privacyTier {
            case .full:
                return string
            case .redacted:
                if shouldRedactStringValue(string, key: normalizedKey) {
                    return "<redacted>"
                }
            case .operatorMode:
                if isSensitiveExportKey(normalizedKey) || shouldRedactStringValue(string, key: normalizedKey) {
                    return sanitizedOperatorStringValue(string, key: normalizedKey)
                }
            }
        }

        return value
    }

    private func isSensitiveExportKey(_ key: String) -> Bool {
        if key.isEmpty {
            return false
        }
        let exactMatches: Set<String> = [
            "commandline",
            "cwd",
            "executablepath",
            "frontmostwindowtitle",
            "activewindowtitle",
            "windowtitle",
            "workspacepath",
            "reporoot",
            "sessionid",
            "telemetryendpoint",
            "path",
            "url",
            "ports",
            "entities_preview",
        ]
        if exactMatches.contains(key) {
            return true
        }
        return key.contains("command")
            || key.contains("windowtitle")
            || key.contains("workspace")
            || key.contains("repo")
            || key.contains("endpoint")
            || key.contains("executable")
            || key.contains("path")
    }

    private func shouldRedactStringValue(_ value: String, key: String) -> Bool {
        guard !value.isEmpty else {
            return false
        }
        if isSensitiveExportKey(key) {
            return true
        }
        if value.hasPrefix("/") || value.hasPrefix("file://") || value.hasPrefix("http://") || value.hasPrefix("https://") {
            return true
        }
        if value.contains("/Users/") || value.contains("/var/") || value.contains(".sock") {
            return true
        }
        return false
    }

    private func redactSensitiveFieldsOperatorMode(_ value: Any?) -> Any {
        guard let fields = value as? [[String: Any]] else {
            return []
        }
        return fields.map { field in
            let key = (field["key"] as? String) ?? "field"
            let rawValue = (field["value"] as? String) ?? ""
            return [
                "key": key,
                "value": sanitizedOperatorStringValue(rawValue, key: key.lowercased()),
            ]
        }
    }

    private func sanitizedOperatorStringValue(_ value: String, key: String) -> String {
        if value.isEmpty {
            return value
        }
        if key.contains("windowtitle") || key == "message" {
            return "<redacted>"
        }
        if key.contains("command") {
            let executable = value
                .split(separator: " ")
                .first
                .map(String.init)
                .map { URL(fileURLWithPath: $0).lastPathComponent }
                ?? "command"
            return executable
        }
        if key.contains("url") || key.contains("endpoint") {
            return sanitizedHostString(value)
        }
        if key.contains("path") || key.contains("cwd") || key.contains("workspace") || key.contains("repo") {
            return sanitizedPathString(value)
        }
        if key == "ports" || key == "sessionid" {
            return "<redacted>"
        }
        if value.hasPrefix("http://") || value.hasPrefix("https://") {
            return sanitizedHostString(value)
        }
        if value.hasPrefix("/") || value.hasPrefix("file://") || value.contains("/Users/") {
            return sanitizedPathString(value)
        }
        return value
    }

    private func sanitizedHostString(_ value: String) -> String {
        guard let components = URLComponents(string: value), let host = components.host else {
            return "<redacted host>"
        }
        let scheme = components.scheme ?? "https"
        return "\(scheme)://\(host)"
    }

    private func sanitizedPathString(_ value: String) -> String {
        let cleaned = value.replacingOccurrences(of: "file://", with: "")
        let lastPath = URL(fileURLWithPath: cleaned).lastPathComponent
        return lastPath.isEmpty ? "<redacted path>" : ".../\(lastPath)"
    }

    private func normalizedSupportBundleURL(_ url: URL) -> URL {
        if url.pathExtension == "aetower-supportbundle" {
            return url
        }
        return url.appendingPathExtension("aetower-supportbundle")
    }

    private func writeSupportBundlePackage(
        to packageURL: URL,
        settings: SettingsStore,
        diagnosticsLimit: UInt32,
        context: ExportSupportBundleContext
    ) throws {
        let payload = try supportBundlePayload(settings, diagnosticsLimit: diagnosticsLimit, context: context)
        let fileManager = FileManager.default
        if fileManager.fileExists(atPath: packageURL.path) {
            try fileManager.removeItem(at: packageURL)
        }
        try fileManager.createDirectory(at: packageURL, withIntermediateDirectories: true)

        let snapshotJson = exportControlledJson(
            bridge.exportSnapshotJSON(),
            privacyTier: settings.exportPrivacyTier
        )
        let diagnosticsJson = exportControlledJson(
            bridge.exportDiagnosticsQueryJSON(
                DiagnosticsQuery(
                    limit: diagnosticsLimit,
                    minimumLevel: nil,
                    subsystem: nil,
                    search: nil,
                    sinceMillis: nil,
                    includePersisted: true
                )
            ),
            privacyTier: settings.exportPrivacyTier
        )
        let manifest: [String: Any] = [
            "format": "aetower-supportbundle/v2",
            "generatedAtMillis": UInt64(Date().timeIntervalSince1970 * 1000),
            "privacyTier": settings.exportPrivacyTier.rawValue,
            "files": [
                "bundle.json",
                "snapshot.json",
                "diagnostics.json",
            ],
        ]

        try writeJsonObject(payload, to: packageURL.appendingPathComponent("bundle.json"))
        try snapshotJson.write(
            to: packageURL.appendingPathComponent("snapshot.json"),
            atomically: true,
            encoding: .utf8
        )
        try diagnosticsJson.write(
            to: packageURL.appendingPathComponent("diagnostics.json"),
            atomically: true,
            encoding: .utf8
        )
        try writeJsonObject(manifest, to: packageURL.appendingPathComponent("manifest.json"))
    }

    private func writeJsonObject(_ value: Any, to url: URL) throws {
        let data = try JSONSerialization.data(
            withJSONObject: value,
            options: [.prettyPrinted, .sortedKeys]
        )
        try data.write(to: url, options: .atomic)
    }

    private func supportBundleDirectoryName() -> String {
        let formatter = DateFormatter()
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone.current
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        return "aetower-support-bundle-\(formatter.string(from: Date())).aetower-supportbundle"
    }
}
