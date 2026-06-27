import SwiftUI
import AetowerBridge

/// Type filter for the open files & sockets list.
private enum OpenResourceTypeFilter: Hashable {
    case all, files, sockets
}

struct ProcessOperatorPanel: View {
    let entity: EntitySnapshot
    let processEntities: [EntitySnapshot]
    let state: AppState
    let quickRequest: ProcessOperatorRequest?

    @State private var selectedPID: UInt32?
    @State private var pendingAction: ProcessActionDraft?
    @State private var executingAction: ProcessActionSubmission?
    @State private var actionReason = ""
    @State private var openResourceFilter = ""
    @State private var openResourceTypeFilter: OpenResourceTypeFilter = .all
    @State private var holderQuery = ""
    @State private var holderQueryIsPort = true

    init(
        entity: EntitySnapshot,
        state: AppState,
        processEntities: [EntitySnapshot]? = nil,
        quickRequest: ProcessOperatorRequest? = nil
    ) {
        self.entity = entity
        self.processEntities = processEntities ?? [entity]
        self.state = state
        self.quickRequest = quickRequest
    }

    var body: some View {
        GroupBox("Process operator") {
            VStack(alignment: .leading, spacing: 14) {
                header

                networkConnectionsSection

                reputationSection

                if processes.isEmpty {
                    ContentUnavailableView(
                        "No live process controls",
                        systemImage: "slider.horizontal.3",
                        description: Text("Aetower has no concrete PID for this entity yet.")
                    )
                } else {
                    processPicker
                    if let pid = selectedPID {
                        operatorActions(pid: pid)
                        inspectionSummary(pid: pid)
                        openResourcesSummary(pid: pid)
                        sampleSummary(pid: pid)
                        actionHistory
                    }
                }
            }
            .padding(.top, 4)
        }
        .task(id: entity.entityId) {
            let availablePIDs = Set(processes.map(\.id))
            if let selectedPID, !availablePIDs.contains(selectedPID) {
                self.selectedPID = processes.first?.id
                pendingAction = nil
                executingAction = nil
            } else if selectedPID == nil {
                selectedPID = processes.first?.id
            }
            state.refreshProcessActionHistory()
        }
        .task(id: quickRequest?.id) {
            guard quickRequest != nil else { return }
            applyQuickRequest()
            state.refreshProcessActionHistory()
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Label("Inspect and operate on concrete PIDs", systemImage: "cpu")
                    .font(.headline)
                Spacer()
                Text("\(processes.count) processes")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Text("Read-only actions are safe. Signals require explicit confirmation and are recorded in diagnostics history.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private var processPicker: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Select process")
                .font(.caption)
                .foregroundStyle(.secondary)

            ForEach(processes) { process in
                Button {
                    selectProcessForNavigation(process.id)
                } label: {
                    HStack(spacing: 8) {
                        Image(systemName: selectedPID == process.id ? "largecircle.fill.circle" : "circle")
                            .foregroundStyle(selectedPID == process.id ? Color.accentColor : .secondary)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(process.component.title)
                                .font(.system(size: 12, weight: .medium))
                                .lineLimit(1)
                            Text(processMetadata(process))
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                        Spacer()
                        Text("PID \(process.id)")
                            .font(.system(size: 10, design: .monospaced))
                            .foregroundStyle(.tertiary)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .contextMenu {
                    processContextMenu(for: process.id)
                }
            }
        }
    }

    @ViewBuilder
    private func processContextMenu(for pid: UInt32) -> some View {
        Button("Inspect PID \(pid)") {
            selectProcessForNavigation(pid)
            state.runProcessInspection(pid: pid)
        }
        Button("Open files & sockets") {
            selectProcessForNavigation(pid)
            state.runProcessOpenResources(pid: pid)
        }
        Button("Run 3s sample") {
            selectProcessForNavigation(pid)
            state.runProcessSample(pid: pid)
        }
        Divider()
        Button("Terminate PID \(pid)…", role: .destructive) {
            previewAction(.terminate, pid: pid)
        }
        Button("Force kill PID \(pid)…", role: .destructive) {
            previewAction(.forceKill, pid: pid)
        }
        Menu("Actions") {
            ForEach(quickPreviewActions) { action in
                Button(action.label, role: action.isDestructive ? .destructive : nil) {
                    previewAction(action, pid: pid)
                }
            }
        }
    }

    private func operatorActions(pid: UInt32) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                Button("Inspect") {
                    state.runProcessInspection(pid: pid)
                }
                .disabled(isLoading(pid, .processInspect))

                Button("Open files & sockets") {
                    state.runProcessOpenResources(pid: pid)
                }
                .disabled(isLoading(pid, .processResources))

                Button("Run 3s sample") {
                    state.runProcessSample(pid: pid)
                }
                .disabled(isLoading(pid, .processSample))

                Spacer()
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("Signals")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                LazyVGrid(columns: actionColumns, alignment: .leading, spacing: 8) {
                    ForEach(signalActions) { action in
                        actionButton(action, pid: pid)
                    }
                }
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("Scheduling and process trees")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                LazyVGrid(columns: actionColumns, alignment: .leading, spacing: 8) {
                    ForEach(schedulingAndTreeActions) { action in
                        actionButton(action, pid: pid)
                    }
                }
            }

            TextField("Optional reason for action history", text: $actionReason)
                .textFieldStyle(.roundedBorder)
                .font(.caption)
                .aetowerUtilityTextInput()

            if let pendingAction,
               pendingAction.pid == pid {
                actionPreview(pendingAction)
            }

            if let executingAction,
                executingAction.pid == pid,
                isLoading(pid, .processAction)
            {
                actionSubmissionStatus(executingAction)
            }

            if let report = state.processActionReports[pid] {
                actionResult(report)
            }

            if let error = state.entityAnalysisError(processAnalysisKey(pid), kind: .processAction) {
                StatusLine(icon: "exclamationmark.triangle.fill", color: .red, text: error)
            }
        }
    }

    @ViewBuilder
    private func actionPreview(_ draft: ProcessActionDraft) -> some View {
        let pid = draft.pid
        let action = draft.action
        if let preview = state.processActionPreview(
            pid: pid,
            action: action,
            matchingActionID: draft.actionID
        ) {
            let executionBlocker = processActionExecutionBlocker(preview: preview, draft: draft)
            VStack(alignment: .leading, spacing: 8) {
                SectionHeader("Action preview", detail: action.label)
                Text(action.confirmationDetail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                MetricLabel("Targets", targetList(preview.targetPids, fallbackPID: preview.pid))
                if let blastRadius = preview.blastRadius,
                   blastRadius.confirmationRequired {
                    StatusLine(
                        icon: "scope",
                        color: AetowerDesign.Status.warning,
                        text: "Blast radius: \(blastRadius.summary)"
                    )
                }
                if let actionID = preview.actionId {
                    MetricLabel("Action ID", actionID)
                }
                MonospaceBlock(preview.command)
                ForEach(preview.safetyNotes, id: \.self) { note in
                    StatusLine(icon: "shield.lefthalf.filled", color: .orange, text: note)
                }
                if let executionBlocker {
                    StatusLine(
                        icon: "lock.trianglebadge.exclamationmark",
                        color: AetowerDesign.Status.warning,
                        text: executionBlocker
                    )
                }
                HStack {
                    Button(action.label, role: action.isDestructive ? .destructive : nil) {
                        let actionID = draft.actionID
                        let expectedTargets = preview.targetIdentities ?? []
                        executingAction = ProcessActionSubmission(
                            pid: pid,
                            action: action,
                            actionID: actionID,
                            submittedAt: Date()
                        )
                        state.runProcessAction(
                            pid: pid,
                            action: action,
                            reason: actionReason.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty,
                            actionID: actionID,
                            expectedTargets: expectedTargets
                        )
                        state.clearProcessActionPreview(pid: pid, action: action)
                        pendingAction = nil
                        actionReason = ""
                    }
                    .disabled(isLoading(pid, .processAction) || executionBlocker != nil)

                    Button("Cancel") {
                        state.clearProcessActionPreview(pid: pid, action: action, cancelLoading: true)
                        pendingAction = nil
                    }
                    .buttonStyle(.borderless)

                    Spacer()
                }
            }
            .padding(10)
            .background(AetowerDesign.Surface.alertInfo, in: RoundedRectangle(cornerRadius: 10))
        } else if isLoading(pid, .processAction) {
            ProgressView("Previewing \(action.label.lowercased()) targets…")
        }
    }

    private func actionSubmissionStatus(_ submission: ProcessActionSubmission) -> some View {
        StatusLine(
            icon: "paperplane.fill",
            color: AetowerDesign.Status.ready,
            text: "\(submission.action.label) \(shortActionID(submission.actionID)) sent at \(operatorClockTime(submission.submittedAt)); waiting for macOS response and Aetower verification."
        )
    }

    private func actionResult(_ report: ProcessActionReportModel) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionHeader(
                "Action result",
                detail: actionResultDetail(report)
            )
            StatusLine(
                icon: actionStatusIcon(success: report.success, verification: report.verification),
                color: actionStatusColor(success: report.success, verification: report.verification),
                text: "\(targetSummary(report.targetPids, fallbackPID: report.pid)) · \(report.message)"
            )
            if let verification = report.verification {
                StatusLine(
                    icon: verificationIcon(verification),
                    color: verificationColor(verification, success: report.success),
                    text: "Verification: \(verificationLabel(verification))"
                )
            }
            if let actionID = report.actionId {
                MetricLabel("Action ID", actionID)
            }
            if let privilegedStatus = report.privilegedHelperStatus,
               privilegedStatus != "not-requested" {
                StatusLine(
                    icon: "lock.shield",
                    color: AetowerDesign.Status.warning,
                    text: "Privileged helper: \(verificationLabel(privilegedStatus))"
                )
            }
            if let commandResult = report.commandResult {
                LazyVGrid(columns: operatorColumns, alignment: .leading, spacing: 8) {
                    MetricLabel("Exit status", commandResult.exitStatus.map(String.init) ?? "signal/unknown")
                    MetricLabel("Executed", report.executed ? "Yes" : "No")
                }
                if !commandResult.stderr.isEmpty {
                    StatusLine(
                        icon: "text.bubble",
                        color: AetowerDesign.Status.warning,
                        text: "macOS stderr: \(commandResult.stderr)"
                    )
                }
                if !commandResult.stdout.isEmpty {
                    MonospaceBlock(commandResult.stdout)
                }
            }
            MonospaceBlock(report.command)
            actionTargetOutcomes(report.targetOutcomes ?? [])
            actionFollowUpChecks(report.followUpChecks ?? [])
            restorePriorityAction(report)
        }
        .padding(10)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 10))
    }

    @ViewBuilder
    private func actionTargetOutcomes(_ outcomes: [ProcessActionTargetOutcomeModel]) -> some View {
        if !outcomes.isEmpty {
            VStack(alignment: .leading, spacing: 6) {
                Text("Target verification")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                ForEach(outcomes.prefix(8)) { outcome in
                    HStack(alignment: .top, spacing: 8) {
                        Text("PID \(outcome.pid)")
                            .font(.system(size: 10, design: .monospaced))
                            .frame(width: 64, alignment: .leading)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(verificationLabel(outcome.verification))
                                .font(.caption.weight(.medium))
                            Text(outcome.detail)
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                            Text(targetOutcomeMetadata(outcome))
                                .font(.system(size: 10, design: .monospaced))
                                .foregroundStyle(.tertiary)
                        }
                        Spacer()
                    }
                }
                if outcomes.count > 8 {
                    Text("+\(outcomes.count - 8) more targets")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
            }
        }
    }

    @ViewBuilder
    private func actionFollowUpChecks(_ checks: [ProcessActionFollowUpCheckModel]) -> some View {
        if !checks.isEmpty {
            VStack(alignment: .leading, spacing: 6) {
                Text("Follow-up verification")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                ForEach(checks) { check in
                    StatusLine(
                        icon: verificationIcon(check.verification),
                        color: verificationColor(check.verification, success: true),
                        text: "\(check.delayMillis / 1_000)s: \(verificationLabel(check.verification))"
                    )
                }
            }
        }
    }

    @ViewBuilder
    private func restorePriorityAction(_ report: ProcessActionReportModel) -> some View {
        if report.action == "lower-priority",
           let restoreNiceValue = report.targetOutcomes?.first?.niceBefore {
            let targetPID = report.targetPids.first ?? report.pid
            if let expectedTargets = report.targetIdentities,
               !expectedTargets.isEmpty,
               targetIdentitiesHaveStableIdentity(expectedTargets) {
                Button {
                    let restoreActionID = UUID().uuidString
                    executingAction = ProcessActionSubmission(
                        pid: targetPID,
                        action: .normalPriority,
                        actionID: restoreActionID,
                        submittedAt: Date()
                    )
                    state.runProcessAction(
                        pid: targetPID,
                        action: .normalPriority,
                        reason: "Restore nice value \(restoreNiceValue) from action \(report.actionId ?? "unknown").",
                        actionID: restoreActionID,
                        expectedTargets: expectedTargets,
                        restoreNiceValue: restoreNiceValue
                    )
                } label: {
                    Label("Restore previous priority (\(restoreNiceValue))", systemImage: "arrow.uturn.backward.circle")
                }
                .buttonStyle(.bordered)
            } else {
                StatusLine(
                    icon: "lock.trianglebadge.exclamationmark",
                    color: AetowerDesign.Status.warning,
                    text: "Restore blocked: the original action did not record stable target identities."
                )
            }
        }
    }

    @ViewBuilder
    private func inspectionSummary(pid: UInt32) -> some View {
        if let inspection = state.processInspections[pid] {
            VStack(alignment: .leading, spacing: 8) {
                SectionHeader("Inspection", detail: inspection.alive ? "alive" : "not visible")
                LazyVGrid(columns: operatorColumns, alignment: .leading, spacing: 8) {
                    MetricLabel("Owner", inspection.displayName ?? "Unattributed")
                    MetricLabel("Parent", inspection.parentPid.map { String($0) } ?? "Unknown")
                    MetricLabel("CPU", inspection.cpuPercent.map { String(format: "%.1f%%", $0) } ?? "Unknown")
                    MetricLabel("Memory", inspection.memoryBytes.map(formatBytes) ?? "Unknown")
                    MetricLabel("Children", inspection.childPids.isEmpty ? "None" : inspection.childPids.map(String.init).joined(separator: ", "))
                    MetricLabel("User", inspection.user ?? "Unknown")
                }
                if let command = inspection.commandLine {
                    MonospaceBlock(command)
                }
                inspectionMetadata(inspection)
                ForEach(inspection.safetyNotes, id: \.self) { note in
                    StatusLine(icon: "shield.lefthalf.filled", color: .orange, text: note)
                }
            }
        } else if isLoading(pid, .processInspect) {
            ProgressView("Inspecting PID \(pid)…")
        }
    }

    /// Static, on-demand metadata sections (ProcessSpy parity): bundle identity,
    /// code signature, entitlements, environment, and launchd startup entry.
    /// Each section renders only when the engine returned data for it.
    @ViewBuilder
    private var networkConnectionsSection: some View {
        if !entity.networkConnections.isEmpty {
            VStack(alignment: .leading, spacing: 6) {
                SectionHeader("Network connections", detail: "\(entity.networkConnections.count)")
                ForEach(Array(entity.networkConnections.prefix(12).enumerated()), id: \.offset) { _, connection in
                    HStack(spacing: 8) {
                        Text(connection.`protocol`.isEmpty ? "—" : connection.`protocol`)
                            .font(.system(size: 9, weight: .bold, design: .monospaced))
                            .foregroundStyle(AetowerDesign.Tone.network)
                            .frame(width: 34, alignment: .leading)
                        Text(connection.remote ?? connection.local)
                            .font(.system(size: 11, design: .monospaced))
                            .lineLimit(1)
                            .truncationMode(.middle)
                            .textSelection(.enabled)
                        Spacer()
                        Text(connection.state)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                }
                if entity.networkConnections.count > 12 {
                    Text("+\(entity.networkConnections.count - 12) more")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
            }
        }
    }

    /// Entity-level VirusTotal reputation (opt-in). Sourced from the snapshot —
    /// no lookup happens here. Shown only when a verdict has been resolved for a
    /// risky binary.
    @ViewBuilder
    private var reputationSection: some View {
        if let reputation = entity.binaryReputation {
            let detections = reputation.malicious + reputation.suspicious
            VStack(alignment: .leading, spacing: 6) {
                SectionHeader(
                    "Binary reputation",
                    detail: reputationVerdictLabel(reputation.verdict)
                )
                LazyVGrid(columns: operatorColumns, alignment: .leading, spacing: 8) {
                    MetricLabel("Detections", "\(detections)/\(reputation.totalEngines)")
                    MetricLabel("Malicious", "\(reputation.malicious)")
                    MetricLabel("Suspicious", "\(reputation.suspicious)")
                    MetricLabel("Harmless", "\(reputation.harmless)")
                }
                switch reputation.verdict {
                case .malicious:
                    StatusLine(
                        icon: "exclamationmark.octagon.fill",
                        color: AetowerDesign.Status.error,
                        text: "Flagged as malicious by \(detections) engine(s)."
                    )
                case .suspicious:
                    StatusLine(
                        icon: "exclamationmark.triangle.fill",
                        color: AetowerDesign.Status.warning,
                        text: "Flagged as suspicious by \(detections) engine(s)."
                    )
                case .unknown:
                    StatusLine(
                        icon: "questionmark.circle",
                        color: .secondary,
                        text: "Not present in VirusTotal's corpus."
                    )
                default:
                    EmptyView()
                }
                if !reputation.permalink.isEmpty,
                    let url = URL(string: reputation.permalink)
                {
                    Link("View on VirusTotal", destination: url)
                        .font(.caption)
                }
            }
        }
    }

    private func reputationVerdictLabel(_ verdict: ReputationVerdict) -> String {
        switch verdict {
        case .clean: return "clean"
        case .suspicious: return "suspicious"
        case .malicious: return "malicious"
        case .unknown: return "unknown"
        @unknown default: return "unknown"
        }
    }

    @ViewBuilder
    private func inspectionMetadata(_ inspection: ProcessInspectionReportModel) -> some View {
        if let bundle = inspection.bundle, bundle.bundleId != nil || bundle.versionLabel != nil {
            VStack(alignment: .leading, spacing: 8) {
                SectionHeader("Bundle", detail: bundle.name ?? "app")
                LazyVGrid(columns: operatorColumns, alignment: .leading, spacing: 8) {
                    MetricLabel("Bundle ID", bundle.bundleId ?? "Unknown")
                    MetricLabel("Version", bundle.versionLabel ?? "Unknown")
                }
                if let path = bundle.bundlePath {
                    MonospaceBlock(path)
                }
            }
        }

        if let signature = inspection.signature {
            VStack(alignment: .leading, spacing: 8) {
                SectionHeader("Code signature", detail: signature.classificationLabel)
                LazyVGrid(columns: operatorColumns, alignment: .leading, spacing: 8) {
                    MetricLabel("Type", signature.classificationLabel)
                    MetricLabel("Team ID", signature.teamId ?? "—")
                    MetricLabel("Identifier", signature.signingId ?? "—")
                    MetricLabel("Notarized", notarizedLabel(signature.notarized))
                }
                if let authority = signature.authority.first {
                    MetricLabel("Authority", authority)
                }
                if signature.isAdhoc == true {
                    StatusLine(
                        icon: "exclamationmark.triangle.fill",
                        color: AetowerDesign.Status.warning,
                        text: "Ad-hoc signed (no Developer ID) — unusual for distributed software."
                    )
                }
                if let note = signature.note {
                    StatusLine(icon: "info.circle", color: .secondary, text: note)
                }
            }
        }

        if let entitlements = inspection.entitlements, !entitlements.isEmpty {
            DisclosureGroup {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(entitlements, id: \.self) { entitlement in
                        Text(entitlement)
                            .font(.system(size: 10, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .textSelection(.enabled)
                    }
                }
                .padding(.top, 4)
            } label: {
                SectionHeader("Entitlements", detail: "\(entitlements.count)")
            }
        }

        if let startup = inspection.startupEntry {
            VStack(alignment: .leading, spacing: 6) {
                SectionHeader("Startup entry", detail: startup.kind)
                if let label = startup.label {
                    MetricLabel("Label", label)
                }
                MonospaceBlock(startup.plistPath)
            }
        }

        environmentSection(inspection)
        dylibSection(inspection)
    }

    @ViewBuilder
    private func dylibSection(_ inspection: ProcessInspectionReportModel) -> some View {
        let dylibs = inspection.loadedDylibs ?? []
        if !dylibs.isEmpty {
            let summary = inspection.dylibSummary
            let detail = summary.map {
                "\($0.total) total · \($0.thirdParty) 3rd-party"
                    + ($0.injected > 0 ? " · \($0.injected) injected" : "")
            } ?? "\(dylibs.count)"
            DisclosureGroup {
                VStack(alignment: .leading, spacing: 3) {
                    ForEach(dylibs) { dylib in
                        HStack(spacing: 8) {
                            Image(systemName: dylib.injected
                                ? "exclamationmark.triangle.fill"
                                : (dylib.isSystem ? "lock.shield" : "shippingbox"))
                                .font(.system(size: 10))
                                .foregroundStyle(dylib.injected
                                    ? AetowerDesign.Status.warning
                                    : (dylib.isSystem ? Color.secondary : AetowerDesign.Status.ready))
                                .frame(width: 16)
                            Text(dylib.name)
                                .font(.system(size: 11, weight: dylib.injected ? .semibold : .regular))
                                .lineLimit(1)
                            if dylib.injected {
                                Text("INJECTED")
                                    .font(.system(size: 8, weight: .bold))
                                    .foregroundStyle(AetowerDesign.Status.warning)
                            }
                            Spacer()
                            Text(dylib.path)
                                .font(.system(size: 9, design: .monospaced))
                                .foregroundStyle(.tertiary)
                                .lineLimit(1)
                                .truncationMode(.head)
                                .frame(maxWidth: 220, alignment: .trailing)
                                .textSelection(.enabled)
                        }
                    }
                }
                .padding(.top, 4)
            } label: {
                SectionHeader("Loaded libraries", detail: detail)
            }
        }
    }

    @ViewBuilder
    private func environmentSection(_ inspection: ProcessInspectionReportModel) -> some View {
        let environment = inspection.environment ?? []
        if !environment.isEmpty {
            DisclosureGroup {
                VStack(alignment: .leading, spacing: 3) {
                    ForEach(environment) { entry in
                        HStack(alignment: .firstTextBaseline, spacing: 8) {
                            Text(entry.key)
                                .font(.system(size: 10, weight: .medium, design: .monospaced))
                                .frame(maxWidth: 160, alignment: .leading)
                                .lineLimit(1)
                            Text(entry.value)
                                .font(.system(size: 10, design: .monospaced))
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                                .textSelection(.enabled)
                            Spacer()
                        }
                    }
                }
                .padding(.top, 4)
            } label: {
                SectionHeader(
                    "Environment",
                    detail: inspection.environmentNote ?? "\(environment.count) variables"
                )
            }
        } else if let note = inspection.environmentNote {
            StatusLine(icon: "lock.shield", color: .secondary, text: note)
        }
    }

    private func notarizedLabel(_ notarized: Bool?) -> String {
        switch notarized {
        case .some(true): return "Yes"
        case .some(false): return "No"
        case .none: return "Unknown"
        }
    }

    /// Apply the text + type filter to a process's open resources.
    private func filteredOpenResources(
        _ report: ProcessOpenResourcesReportModel
    ) -> [ProcessOpenResourceModel] {
        let needle = openResourceFilter.trimmingCharacters(in: .whitespaces).lowercased()
        return report.resources.filter { resource in
            switch openResourceTypeFilter {
            case .all: break
            case .files where resource.isSocket: return false
            case .sockets where !resource.isSocket: return false
            default: break
            }
            guard !needle.isEmpty else { return true }
            return resource.name.lowercased().contains(needle)
                || resource.resourceType.lowercased().contains(needle)
        }
    }

    /// Pivot from an open-resource row to "who else holds this": sockets pivot
    /// by local port, files by path. Falls back to prefilling the lookup field
    /// when a socket's port can't be parsed.
    private func pivotToHolders(for resource: ProcessOpenResourceModel) {
        if resource.isSocket {
            if let port = Self.localPort(from: resource.name) {
                holderQueryIsPort = true
                holderQuery = String(port)
                state.runResourceHolders(.port(port))
            } else {
                holderQueryIsPort = true
                holderQuery = resource.name
            }
        } else {
            holderQueryIsPort = false
            holderQuery = resource.name
            state.runResourceHolders(.file(resource.name))
        }
    }

    /// Run the manual reverse-lookup from the text field.
    private func runHolderLookup() {
        let raw = holderQuery.trimmingCharacters(in: .whitespaces)
        guard !raw.isEmpty else { return }
        if holderQueryIsPort {
            let digits = raw.hasPrefix(":") ? String(raw.dropFirst()) : raw
            if let port = UInt32(digits) {
                state.runResourceHolders(.port(port))
            }
        } else {
            state.runResourceHolders(.file(raw))
        }
    }

    /// Extract the local port from an lsof socket name such as
    /// "TCP 127.0.0.1:8080->1.2.3.4:443" or "TCP *:443 (LISTEN)".
    static func localPort(from name: String) -> UInt32? {
        let local = name.split(separator: ">").first.map(String.init) ?? name
        guard let colon = local.lastIndex(of: ":") else { return nil }
        let digits = local[local.index(after: colon)...].prefix { $0.isNumber }
        return UInt32(digits)
    }

    /// Reverse-pivot lookup: type a port or file path and list every holder.
    private var resourceHolderLookup: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Picker("", selection: $holderQueryIsPort) {
                    Text("Port").tag(true)
                    Text("File").tag(false)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .frame(width: 120)
                TextField(holderQueryIsPort ? "Port e.g. 443" : "File path", text: $holderQuery)
                    .aetowerUtilityTextInput()
                    .onSubmit(runHolderLookup)
                Button("Find holders", action: runHolderLookup)
                    .buttonStyle(.bordered)
                    .controlSize(.small)
            }
            if state.resourceHoldersIsLoading {
                ProgressView().controlSize(.small)
            } else if let error = state.resourceHoldersError {
                StatusLine(
                    icon: "exclamationmark.triangle",
                    color: AetowerDesign.Status.warning,
                    text: error
                )
            } else if let holders = state.resourceHolders {
                Text("\(holders.holderCount) holder\(holders.holderCount == 1 ? "" : "s") of \(holders.query)")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                ForEach(holders.holders.prefix(12)) { holder in
                    HStack(spacing: 8) {
                        Text(holder.command)
                            .font(.caption)
                        Text("pid \(holder.pid)")
                            .font(.system(size: 10, design: .monospaced))
                            .foregroundStyle(.secondary)
                        Text(holder.user)
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                        Text(holder.name)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                        Spacer()
                    }
                }
            }
        }
        .padding(8)
        .background(
            AetowerDesign.Surface.card,
            in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm)
        )
    }

    @ViewBuilder
    private func openResourcesSummary(pid: UInt32) -> some View {
        if let resources = state.processOpenResources[pid] {
            VStack(alignment: .leading, spacing: 8) {
                SectionHeader(
                    "Open files & sockets",
                    detail: "\(resources.fileCount) files · \(resources.socketCount) sockets · \(resources.returned)/\(resources.resourceCount) shown"
                )
                HStack(spacing: 6) {
                    TextField("Filter files & sockets", text: $openResourceFilter)
                        .aetowerUtilityTextInput()
                    Picker("", selection: $openResourceTypeFilter) {
                        Text("All").tag(OpenResourceTypeFilter.all)
                        Text("Files").tag(OpenResourceTypeFilter.files)
                        Text("Sockets").tag(OpenResourceTypeFilter.sockets)
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                    .frame(width: 170)
                }
                resourceHolderLookup
                ForEach(filteredOpenResources(resources).prefix(40)) { resource in
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text(resource.fd)
                            .font(.system(size: 10, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .frame(width: 42, alignment: .leading)
                        Text(resource.isSocket ? "socket" : resource.resourceType)
                            .font(.caption2)
                            .foregroundStyle(resource.isSocket ? .blue : .secondary)
                            .frame(width: 54, alignment: .leading)
                        Text(resource.name)
                            .font(.caption)
                            .lineLimit(1)
                        Spacer()
                        Button {
                            pivotToHolders(for: resource)
                        } label: {
                            Image(systemName: "arrow.up.right.circle")
                                .font(.system(size: 11))
                        }
                        .buttonStyle(.plain)
                        .help("Find every process holding this \(resource.isSocket ? "port" : "file")")
                    }
                }
            }
        } else if isLoading(pid, .processResources) {
            ProgressView("Reading open files and sockets…")
        }
    }

    @ViewBuilder
    private func sampleSummary(pid: UInt32) -> some View {
        if let sample = state.processSamples[pid] {
            VStack(alignment: .leading, spacing: 8) {
                SectionHeader("Process sample", detail: "\(sample.durationSeconds)s · \(sample.threadCount) sampled stacks")
                Text(sample.summary)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                ForEach(sample.topStacks.prefix(4)) { stack in
                    CompactStackRow(stack: stack)
                }
            }
        } else if isLoading(pid, .processSample) {
            ProgressView("Sampling PID \(pid)…")
        }
    }

    private var actionHistory: some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionHeader("Recent actions", detail: "diagnostics-backed")
            if let history = state.processActionHistory, !history.actions.isEmpty {
                ForEach(history.actions.prefix(5)) { action in
                    HStack(spacing: 8) {
                        Image(systemName: actionStatusIcon(success: action.success, verification: action.verification))
                            .foregroundStyle(actionStatusColor(success: action.success, verification: action.verification))
                        Text(action.action ?? "process-action")
                            .font(.caption)
                        if let actionID = action.actionId {
                            Text(shortActionID(actionID))
                                .font(.system(size: 10, design: .monospaced))
                                .foregroundStyle(.tertiary)
                        }
                        Text(targetSummary(action.targetPids, fallbackPID: action.pid))
                            .font(.system(size: 10, design: .monospaced))
                            .foregroundStyle(.secondary)
                        if let verification = action.verification {
                            Text(verificationLabel(verification))
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Text(operatorRelativeTime(action.timestampMillis))
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                    }
                }
            } else {
                Text("No process actions recorded in the recent window.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var processes: [OperatorProcess] {
        processEntities.flatMap(\.components)
            .filter { $0.kind != .adapterContext }
            .compactMap { component in
                component.processId.map { OperatorProcess(id: $0, component: component) }
            }
            .reduce(into: [UInt32: OperatorProcess]()) { processes, process in
                let existing = processes[process.id]
                if existing == nil || process.component.cpuPercent > (existing?.component.cpuPercent ?? 0) {
                    processes[process.id] = process
                }
            }
            .values
            .sorted {
                if $0.component.cpuPercent != $1.component.cpuPercent {
                    return $0.component.cpuPercent > $1.component.cpuPercent
                }
                return $0.component.memoryBytes > $1.component.memoryBytes
            }
    }

    private var operatorColumns: [GridItem] {
        [GridItem(.adaptive(minimum: 140), spacing: 10)]
    }

    private var actionColumns: [GridItem] {
        [GridItem(.adaptive(minimum: 138), spacing: 8)]
    }

    private var signalActions: [ProcessActionKind] {
        [.suspend, .resume, .terminate, .forceKill]
    }

    private var schedulingAndTreeActions: [ProcessActionKind] {
        [.lowerPriority, .normalPriority, .terminateTree, .forceKillTree]
    }

    private var quickPreviewActions: [ProcessActionKind] {
        [.suspend, .resume, .lowerPriority, .normalPriority, .terminate, .forceKill, .terminateTree, .forceKillTree]
    }

    private func actionButton(_ action: ProcessActionKind, pid: UInt32) -> some View {
        Button(role: action.isDestructive ? .destructive : nil) {
            previewAction(action, pid: pid)
        } label: {
            Label(action.label, systemImage: action.systemImage)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .disabled(isLoading(pid, .processAction))
    }

    private func previewAction(_ action: ProcessActionKind, pid: UInt32) {
        selectedPID = pid
        let actionID = UUID().uuidString
        pendingAction = ProcessActionDraft(
            pid: pid,
            action: action,
            actionID: actionID
        )
        state.runProcessActionPreview(
            pid: pid,
            action: action,
            reason: actionReason.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty,
            actionID: actionID
        )
    }

    private func processActionExecutionBlocker(
        preview: ProcessActionReportModel,
        draft: ProcessActionDraft
    ) -> String? {
        if preview.actionId != draft.actionID {
            return "Execution blocked: this preview does not match the current action UUID."
        }
        if !preview.dryRun || preview.executed {
            return "Execution blocked: expected a dry-run preview before executing."
        }
        let targetPids = preview.targetPids.isEmpty ? [preview.pid] : preview.targetPids
        guard let identities = preview.targetIdentities,
              !identities.isEmpty
        else {
            return "Execution blocked: preview did not include target identities. Refresh and preview again."
        }
        let expectedPIDs = Set(targetPids)
        let identityPIDs = Set(identities.map(\.pid))
        if expectedPIDs != identityPIDs {
            return "Execution blocked: target identities do not cover every planned PID."
        }
        if !targetIdentitiesHaveStableIdentity(identities) {
            return "Execution blocked: at least one target lacks a stable start time or executable path for PID-reuse protection."
        }
        if (preview.targetOutcomes ?? []).contains(where: { !$0.visibleBefore }) {
            return "Execution blocked: preview says at least one target is not visible right now."
        }
        return nil
    }

    private func targetIdentitiesHaveStableIdentity(
        _ identities: [ProcessActionTargetIdentityModel]
    ) -> Bool {
        !identities.contains { identity in
            (identity.startTimeMillis ?? 0) == 0
                && identity.executablePath?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty != false
        }
    }

    private func applyQuickRequest() {
        guard let quickRequest else { return }
        switch quickRequest.operation {
        case .inspect:
            selectProcessForNavigation(quickRequest.pid)
            state.runProcessInspection(pid: quickRequest.pid)
        case .resources:
            selectProcessForNavigation(quickRequest.pid)
            state.runProcessOpenResources(pid: quickRequest.pid)
        case .sample:
            selectProcessForNavigation(quickRequest.pid)
            state.runProcessSample(pid: quickRequest.pid)
        case .previewAction(let action):
            previewAction(action, pid: quickRequest.pid)
        }
    }

    private func selectProcessForNavigation(_ pid: UInt32) {
        if selectedPID != pid {
            pendingAction = nil
            executingAction = nil
        }
        selectedPID = pid
    }

    private func targetSummary(_ targetPids: [UInt32], fallbackPID: UInt32?) -> String {
        if targetPids.count > 1 {
            return "\(targetPids.count) targets"
        }
        if let targetPID = targetPids.first ?? fallbackPID {
            return "PID \(targetPID)"
        }
        return "unknown PID"
    }

    private func targetList(_ targetPids: [UInt32], fallbackPID: UInt32?) -> String {
        let targets = targetPids.isEmpty ? fallbackPID.map { [$0] } ?? [] : targetPids
        if targets.isEmpty {
            return "unknown PID"
        }
        if targets.count <= 8 {
            return targets.map { "PID \($0)" }.joined(separator: ", ")
        }
        let visible = targets.prefix(8).map { "PID \($0)" }.joined(separator: ", ")
        return "\(visible), +\(targets.count - 8) more"
    }

    private func actionResultDetail(_ report: ProcessActionReportModel) -> String {
        let outcome = report.verification.map(verificationLabel) ?? (report.success ? "accepted" : "failed")
        return "\(report.action) · \(outcome)"
    }

    private func verificationLabel(_ verification: String) -> String {
        switch verification {
        case "preview": return "Preview only"
        case "target-visible": return "Target visible"
        case "target-not-visible": return "Target missing"
        case "verified-exited", "exited": return "Exited / no longer running"
        case "targets-still-visible", "still-visible": return "Still visible"
        case "verified-suspended", "suspended": return "Suspended"
        case "suspend-not-confirmed", "not-suspended": return "Suspend not confirmed"
        case "verified-running", "running": return "Running"
        case "resume-not-confirmed": return "Resume not confirmed"
        case "still-suspended": return "Still suspended"
        case "verified-priority", "priority-lowered", "priority-normal": return "Priority verified"
        case "priority-not-confirmed": return "Priority not confirmed"
        case "command-failed": return "Command failed"
        case "command-accepted": return "Command accepted"
        case "used": return "Used"
        case "approved-not-needed": return "Approved, not needed"
        case "approved-but-unavailable": return "Approved but unavailable"
        case "approved-but-failed": return "Approved but failed"
        case "not-requested": return "Not requested"
        case "not-visible": return "No longer visible"
        default: return verification.replacingOccurrences(of: "-", with: " ")
        }
    }

    private func verificationIcon(_ verification: String) -> String {
        switch verification {
        case "verified-exited", "exited", "verified-suspended", "suspended",
            "verified-running", "running", "verified-priority", "priority-lowered",
            "priority-normal", "command-accepted":
            return "checkmark.seal.fill"
        case "command-failed":
            return "xmark.octagon.fill"
        case "targets-still-visible", "still-visible", "suspend-not-confirmed",
            "not-suspended", "resume-not-confirmed", "still-suspended",
            "priority-not-confirmed":
            return "exclamationmark.triangle.fill"
        default:
            return "info.circle"
        }
    }

    private func verificationColor(_ verification: String, success: Bool) -> Color {
        if verification == "command-failed" || !success {
            return AetowerDesign.Status.error
        }
        switch verification {
        case "verified-exited", "exited", "verified-suspended", "suspended",
            "verified-running", "running", "verified-priority", "priority-lowered",
            "priority-normal", "command-accepted":
            return AetowerDesign.Status.success
        case "targets-still-visible", "still-visible", "suspend-not-confirmed",
            "not-suspended", "resume-not-confirmed", "still-suspended",
            "priority-not-confirmed":
            return AetowerDesign.Status.warning
        default:
            return .secondary
        }
    }

    private func actionStatusIcon(success: Bool, verification: String?) -> String {
        if !success {
            return "xmark.octagon.fill"
        }
        guard let verification else {
            return "checkmark.circle.fill"
        }
        return verificationIsConfirmed(verification)
            ? "checkmark.circle.fill"
            : "exclamationmark.triangle.fill"
    }

    private func actionStatusColor(success: Bool, verification: String?) -> Color {
        if !success {
            return AetowerDesign.Status.error
        }
        guard let verification else {
            return AetowerDesign.Status.success
        }
        return verificationIsConfirmed(verification)
            ? AetowerDesign.Status.success
            : AetowerDesign.Status.warning
    }

    private func verificationIsConfirmed(_ verification: String) -> Bool {
        switch verification {
        case "verified-exited", "verified-suspended", "verified-running",
            "verified-priority", "command-accepted", "preview":
            return true
        default:
            return false
        }
    }

    private func targetOutcomeMetadata(_ outcome: ProcessActionTargetOutcomeModel) -> String {
        var parts = [
            "before \(outcome.visibleBefore ? "visible" : "missing")",
        ]
        if let visibleAfter = outcome.visibleAfter {
            parts.append("after \(visibleAfter ? "visible" : "missing")")
        }
        if let niceBefore = outcome.niceBefore {
            parts.append("nice before \(niceBefore)")
        }
        if let statusAfter = outcome.statusAfter {
            parts.append("stat \(statusAfter)")
        }
        if let niceAfter = outcome.niceAfter {
            parts.append("nice \(niceAfter)")
        }
        return parts.joined(separator: " · ")
    }

    private func shortActionID(_ actionID: String) -> String {
        let suffix = actionID.suffix(8)
        return "#\(suffix)"
    }

    private func isLoading(_ pid: UInt32, _ kind: EntityAnalysisKind) -> Bool {
        state.entityAnalysisIsLoading(processAnalysisKey(pid), kind: kind)
    }

    private func processAnalysisKey(_ pid: UInt32) -> String {
        "pid:\(pid)"
    }

    private func processMetadata(_ process: OperatorProcess) -> String {
        let component = process.component
        var parts = [
            String(format: "%.1f%% CPU", component.cpuPercent),
            formatBytes(component.memoryBytes),
        ]
        if let user = component.user {
            parts.append(user)
        }
        if let cwd = component.cwd?.split(separator: "/").last.map(String.init) {
            parts.append(cwd)
        }
        return parts.joined(separator: " · ")
    }
}

private struct OperatorProcess: Identifiable {
    let id: UInt32
    let component: ComponentSnapshot
}

private struct ProcessActionDraft: Equatable {
    let pid: UInt32
    let action: ProcessActionKind
    let actionID: String
}

private struct ProcessActionSubmission: Equatable {
    let pid: UInt32
    let action: ProcessActionKind
    let actionID: String
    let submittedAt: Date
}

private typealias SectionHeader = AetowerDetailHeader
private typealias MetricLabel = AetowerKeyValue
private typealias MonospaceBlock = AetowerMonospaceBlock
private typealias StatusLine = AetowerStatusLine

private struct CompactStackRow: View {
    let stack: SampledStackReportModel

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(stack.queueLabel ?? stack.threadLabel)
                    .font(.caption.weight(.medium))
                    .lineLimit(1)
                Spacer()
                Text("\(stack.sampleCount) samples")
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(.secondary)
            }
            Text(stack.classification)
                .font(.caption2)
                .foregroundStyle(.secondary)
            if let frame = stack.topFrames.first {
                Text(frame)
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
            }
        }
        .padding(8)
        .background(.secondary.opacity(0.08), in: RoundedRectangle(cornerRadius: 8))
    }
}

private func operatorRelativeTime(_ timestampMillis: UInt64) -> String {
    let date = Date(timeIntervalSince1970: TimeInterval(timestampMillis) / 1000)
    let delta = max(0, Date().timeIntervalSince(date))
    if delta < 60 {
        return "just now"
    }
    if delta < 3600 {
        return "\(Int(delta / 60))m ago"
    }
    return "\(Int(delta / 3600))h ago"
}

private let processActionClockFormatter: DateFormatter = {
    let formatter = DateFormatter()
    formatter.timeStyle = .medium
    formatter.dateStyle = .none
    return formatter
}()

private func operatorClockTime(_ date: Date) -> String {
    processActionClockFormatter.string(from: date)
}

private extension String {
    var nilIfEmpty: String? {
        isEmpty ? nil : self
    }
}
