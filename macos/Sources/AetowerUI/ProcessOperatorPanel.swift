import SwiftUI
import AetowerBridge

struct ProcessOperatorPanel: View {
    let entity: EntitySnapshot
    let processEntities: [EntitySnapshot]
    let state: AppState
    let quickRequest: ProcessOperatorRequest?

    @State private var selectedPID: UInt32?
    @State private var pendingAction: ProcessActionKind?
    @State private var actionReason = ""

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
            if selectedPID == nil {
                selectedPID = processes.first?.id
            }
            if quickRequest != nil {
                applyQuickRequest()
                state.refreshProcessActionHistory()
                return
            }
            if let pid = selectedPID {
                state.runProcessInspection(pid: pid)
            }
            state.refreshProcessActionHistory()
        }
        .onChange(of: selectedPID) { _, pid in
            guard let pid else { return }
            state.runProcessInspection(pid: pid)
        }
        .onChange(of: quickRequest?.id) { _, _ in
            applyQuickRequest()
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
                    selectedPID = process.id
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
            selectedPID = pid
            state.runProcessInspection(pid: pid)
        }
        Button("Open files & sockets") {
            selectedPID = pid
            state.runProcessOpenResources(pid: pid)
        }
        Button("Run 3s sample") {
            selectedPID = pid
            state.runProcessSample(pid: pid)
        }
        Divider()
        Menu("Preview action") {
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

            if let pendingAction {
                actionPreview(pid: pid, action: pendingAction)
            }

            if let report = state.processActionReports[pid] {
                StatusLine(
                    icon: report.success ? "checkmark.circle.fill" : "exclamationmark.triangle.fill",
                    color: report.success ? .green : .orange,
                    text: "\(targetSummary(report.targetPids, fallbackPID: report.pid)) · \(report.message) · \(report.command)"
                )
            }

            if let error = state.entityAnalysisError(processAnalysisKey(pid), kind: .processAction) {
                StatusLine(icon: "exclamationmark.triangle.fill", color: .red, text: error)
            }
        }
    }

    @ViewBuilder
    private func actionPreview(pid: UInt32, action: ProcessActionKind) -> some View {
        if let preview = state.processActionPreview(pid: pid, action: action) {
            VStack(alignment: .leading, spacing: 8) {
                SectionHeader("Action preview", detail: action.label)
                Text(action.confirmationDetail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                MetricLabel("Targets", targetList(preview.targetPids, fallbackPID: preview.pid))
                MonospaceBlock(preview.command)
                ForEach(preview.safetyNotes, id: \.self) { note in
                    StatusLine(icon: "shield.lefthalf.filled", color: .orange, text: note)
                }
                HStack {
                    Button(action.label, role: action.isDestructive ? .destructive : nil) {
                        state.runProcessAction(
                            pid: pid,
                            action: action,
                            reason: actionReason.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty
                        )
                        state.clearProcessActionPreview(pid: pid, action: action)
                        pendingAction = nil
                        actionReason = ""
                    }
                    .disabled(isLoading(pid, .processAction))

                    Button("Cancel") {
                        state.clearProcessActionPreview(pid: pid, action: action)
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
                SectionHeader("Code signature", detail: signatureStatus(signature))
                LazyVGrid(columns: operatorColumns, alignment: .leading, spacing: 8) {
                    MetricLabel("Team ID", signature.teamId ?? "—")
                    MetricLabel("Identifier", signature.signingId ?? "—")
                    MetricLabel("Notarized", notarizedLabel(signature.notarized))
                }
                if let authority = signature.authority.first {
                    MetricLabel("Authority", authority)
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

    private func signatureStatus(_ signature: ProcessSignatureInfoModel) -> String {
        signature.signed ? "signed" : "unsigned"
    }

    private func notarizedLabel(_ notarized: Bool?) -> String {
        switch notarized {
        case .some(true): return "Yes"
        case .some(false): return "No"
        case .none: return "Unknown"
        }
    }

    @ViewBuilder
    private func openResourcesSummary(pid: UInt32) -> some View {
        if let resources = state.processOpenResources[pid] {
            VStack(alignment: .leading, spacing: 8) {
                SectionHeader(
                    "Open files & sockets",
                    detail: "\(resources.fileCount) files · \(resources.socketCount) sockets · \(resources.returned)/\(resources.resourceCount) shown"
                )
                ForEach(resources.resources.prefix(10)) { resource in
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
                        Image(systemName: action.success ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
                            .foregroundStyle(action.success ? .green : .orange)
                        Text(action.action ?? "process-action")
                            .font(.caption)
                        Text(targetSummary(action.targetPids, fallbackPID: action.pid))
                            .font(.system(size: 10, design: .monospaced))
                            .foregroundStyle(.secondary)
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
        pendingAction = action
        state.runProcessActionPreview(
            pid: pid,
            action: action,
            reason: actionReason.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty
        )
    }

    private func applyQuickRequest() {
        guard let quickRequest else { return }
        selectedPID = quickRequest.pid
        switch quickRequest.operation {
        case .inspect:
            state.runProcessInspection(pid: quickRequest.pid)
        case .resources:
            state.runProcessOpenResources(pid: quickRequest.pid)
        case .sample:
            state.runProcessSample(pid: quickRequest.pid)
        case .previewAction(let action):
            previewAction(action, pid: quickRequest.pid)
        }
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

private struct SectionHeader: View {
    let title: String
    let detail: String

    init(_ title: String, detail: String) {
        self.title = title
        self.detail = detail
    }

    var body: some View {
        HStack {
            Text(title)
                .font(.subheadline.weight(.semibold))
            Spacer()
            Text(detail)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }
}

private struct MetricLabel: View {
    let title: String
    let value: String

    init(_ title: String, _ value: String) {
        self.title = title
        self.value = value
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.caption2)
                .foregroundStyle(.secondary)
            Text(value)
                .font(.caption)
                .lineLimit(1)
        }
    }
}

private struct MonospaceBlock: View {
    let text: String

    init(_ text: String) {
        self.text = text
    }

    var body: some View {
        Text(text)
            .font(.system(size: 10, design: .monospaced))
            .foregroundStyle(.secondary)
            .lineLimit(2)
            .padding(8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.secondary.opacity(0.08), in: RoundedRectangle(cornerRadius: 8))
    }
}

private struct StatusLine: View {
    let icon: String
    let color: Color
    let text: String

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: icon)
                .foregroundStyle(color)
            Text(text)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

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

private extension String {
    var nilIfEmpty: String? {
        isEmpty ? nil : self
    }
}
