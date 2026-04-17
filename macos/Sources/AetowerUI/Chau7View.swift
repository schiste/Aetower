import AetowerBridge
import SwiftUI

public struct Chau7View: View {
    let state: AppState
    @State private var compareBeforeMillis: UInt64?
    @State private var compareAfterMillis: UInt64?

    public init(state: AppState) {
        self.state = state
    }

    private struct SessionSummary: Identifiable {
        let id: String
        let sessionId: String?
        let title: String
        let provider: String?
        let workspace: String?
        let status: String?
        let entities: [EntitySnapshot]
        let totalCpuPercent: Float
        let totalResidentBytes: UInt64
        let totalFootprintBytes: UInt64
        let totalWakeups: Float
        let totalDiskBps: UInt64
        let totalFriction: Float
        let approvalNeeded: Bool
        let detail: String?
    }

    private struct BuildIdentity {
        let version: String?
        let sha: String?
        let timestamp: String?
        let channel: String?

        var label: String {
            var parts: [String] = []
            if let version, !version.isEmpty {
                parts.append("v\(version)")
            }
            if let channel, !channel.isEmpty {
                parts.append(channel.capitalized)
            }
            if let sha, !sha.isEmpty {
                parts.append("#\(String(sha.prefix(7)))")
            }
            return parts.isEmpty ? "Unknown build" : parts.joined(separator: " · ")
        }
    }

    private struct Recommendation: Identifiable {
        let id: String
        let title: String
        let detail: String
        let tone: Color
        let footer: String
    }

    private struct ForensicsItem: Identifiable {
        let id: String
        let title: String
        let detail: String
        let tone: Color
        let timestampMillis: UInt64?
    }

    private struct CompareChoice: Identifiable {
        let id: UInt64
        let millis: UInt64
        let label: String
    }

    private struct CompareReport {
        let beforeMillis: UInt64
        let afterMillis: UInt64
        let cpuBefore: Double
        let cpuAfter: Double
        let residentBefore: UInt64
        let residentAfter: UInt64
        let footprintBefore: UInt64
        let footprintAfter: UInt64
        let wakeupsBefore: Double
        let wakeupsAfter: Double
        let diskBefore: UInt64
        let diskAfter: UInt64
        let frictionBefore: Double
        let frictionAfter: Double
        let sessionCountBefore: Int
        let sessionCountAfter: Int
    }

    private var chau7Entity: EntitySnapshot? {
        state.snapshot.entities.first { entity in
            entity.bundleId == "local.chau7"
                || entity.entityId == "bundle-path:/applications/chau7.app"
                || entity.executablePath == "/Applications/Chau7.app/Contents/MacOS/Chau7"
        }
    }

    private var chau7Capability: CapabilitySnapshot? {
        state.snapshot.capabilities.first { $0.kind == .chau7 }
    }

    private var linkedEntities: [EntitySnapshot] {
        state.snapshot.entities.filter(isChau7Linked)
    }

    private var linkedEntityIDs: Set<String> {
        Set(linkedEntities.map(\.entityId))
    }

    private var buildIdentity: BuildIdentity? {
        let contexts = linkedEntities
            .flatMap(\.components)
            .compactMap(\.adapterContext)
            .filter { $0.kind == .chau7Session }

        guard let chosen = contexts.first(where: { $0.buildSha != nil || $0.appVersion != nil }) ?? contexts.first else {
            return nil
        }

        return BuildIdentity(
            version: chosen.appVersion,
            sha: chosen.buildSha,
            timestamp: chosen.buildTimestamp,
            channel: chosen.buildChannel
        )
    }

    private var sessionSummaries: [SessionSummary] {
        let grouped = Dictionary(grouping: linkedEntities.filter { $0.entityKind == .aiAgent || $0.entityKind == .terminalSession }) { entity in
            sessionContext(for: entity)?.sessionId ?? entity.entityId
        }

        return grouped.compactMap { key, members in
            guard let primary = members.max(by: { $0.friction.totalScore < $1.friction.totalScore }) else {
                return nil
            }
            let contexts = members.compactMap(sessionContext)
            let detail = members.compactMap { sessionComponent(for: $0)?.detail }.first { !$0.isEmpty }
            return SessionSummary(
                id: key,
                sessionId: contexts.compactMap(\.sessionId).first,
                title: primary.displayName,
                provider: providerLabel(for: primary),
                workspace: projectContext(for: primary),
                status: contexts.compactMap(\.status).first,
                entities: members,
                totalCpuPercent: members.reduce(0) { $0 + $1.metrics.cpuPercent },
                totalResidentBytes: members.reduce(0) { $0 + $1.metrics.memoryResidentBytes },
                totalFootprintBytes: members.reduce(0) { $0 + $1.metrics.memoryPhysicalFootprintBytes },
                totalWakeups: members.reduce(0) { $0 + $1.metrics.wakeupsPerSecond },
                totalDiskBps: members.reduce(0) { $0 + $1.metrics.diskReadBps + $1.metrics.diskWriteBps },
                totalFriction: members.reduce(0) { $0 + $1.friction.totalScore },
                approvalNeeded: members.contains { $0.badges.contains("approval-needed") },
                detail: detail
            )
        }
        .sorted {
            if $0.totalFriction != $1.totalFriction {
                return $0.totalFriction > $1.totalFriction
            }
            if $0.totalResidentBytes != $1.totalResidentBytes {
                return $0.totalResidentBytes > $1.totalResidentBytes
            }
            return $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending
        }
    }

    private var topBranches: [ProcessTreeNodeReportModel] {
        guard let report = chau7Entity.flatMap({ state.entityProcessTreeReports[$0.entityId] }),
              let root = report.roots.first
        else {
            return []
        }

        return root.children.sorted {
            if $0.subtreeMemoryBytes != $1.subtreeMemoryBytes {
                return $0.subtreeMemoryBytes > $1.subtreeMemoryBytes
            }
            return $0.subtreeCpuPercent > $1.subtreeCpuPercent
        }
    }

    private var compareChoices: [CompareChoice] {
        state.historySnapshots
            .sorted { $0.capturedAtMillis < $1.capturedAtMillis }
            .map {
                CompareChoice(
                    id: $0.capturedAtMillis,
                    millis: $0.capturedAtMillis,
                    label: "\($0.sequence) · \(chau7Timestamp($0.capturedAtMillis))"
                )
            }
    }

    private var compareReport: CompareReport? {
        guard let beforeMillis = compareBeforeMillis,
              let afterMillis = compareAfterMillis,
              beforeMillis != afterMillis,
              let beforeSnapshot = state.historySnapshots.first(where: { $0.capturedAtMillis == beforeMillis }),
              let afterSnapshot = state.historySnapshots.first(where: { $0.capturedAtMillis == afterMillis }),
              let chau7ID = chau7Entity?.entityId
        else {
            return nil
        }

        func entity(in snapshot: SystemSnapshot) -> EntitySnapshot? {
            snapshot.entities.first { $0.entityId == chau7ID }
        }

        func linkedCount(in snapshot: SystemSnapshot) -> Int {
            snapshot.entities.filter(isChau7Linked).count
        }

        let beforeEntity = entity(in: beforeSnapshot)
        let afterEntity = entity(in: afterSnapshot)

        return CompareReport(
            beforeMillis: beforeMillis,
            afterMillis: afterMillis,
            cpuBefore: Double(beforeEntity?.metrics.cpuPercent ?? 0),
            cpuAfter: Double(afterEntity?.metrics.cpuPercent ?? 0),
            residentBefore: beforeEntity?.metrics.memoryResidentBytes ?? 0,
            residentAfter: afterEntity?.metrics.memoryResidentBytes ?? 0,
            footprintBefore: beforeEntity?.metrics.memoryPhysicalFootprintBytes ?? 0,
            footprintAfter: afterEntity?.metrics.memoryPhysicalFootprintBytes ?? 0,
            wakeupsBefore: Double(beforeEntity?.metrics.wakeupsPerSecond ?? 0),
            wakeupsAfter: Double(afterEntity?.metrics.wakeupsPerSecond ?? 0),
            diskBefore: (beforeEntity?.metrics.diskReadBps ?? 0) + (beforeEntity?.metrics.diskWriteBps ?? 0),
            diskAfter: (afterEntity?.metrics.diskReadBps ?? 0) + (afterEntity?.metrics.diskWriteBps ?? 0),
            frictionBefore: Double(beforeEntity?.friction.totalScore ?? 0),
            frictionAfter: Double(afterEntity?.friction.totalScore ?? 0),
            sessionCountBefore: linkedCount(in: beforeSnapshot),
            sessionCountAfter: linkedCount(in: afterSnapshot)
        )
    }

    private var recommendations: [Recommendation] {
        guard let entity = chau7Entity else { return [] }

        var items: [Recommendation] = entity.recommendations.enumerated().map { index, recommendation in
            Recommendation(
                id: "native-\(index)",
                title: recommendation.title,
                detail: recommendation.detail,
                tone: AetowerDesign.Status.warning,
                footer: "Derived from the current Chau7 app friction contributors."
            )
        }

        if let topBranch = topBranches.first, topBranch.subtreeMemoryBytes > entity.metrics.memoryResidentBytes / 2 {
            items.append(
                Recommendation(
                    id: "branch-memory",
                    title: "Largest subtree is carrying most of the memory",
                    detail: "\(topBranch.title) is holding \(formatBytes(topBranch.subtreeMemoryBytes)) across \(topBranch.subtreeProcessCount) processes.",
                    tone: AetowerDesign.Tone.memory,
                    footer: "Check whether this branch is app core, one embedded agent subtree, or retained session state."
                )
            )
        }

        if entity.metrics.diskReadBps + entity.metrics.diskWriteBps >= 20 * 1024 * 1024 {
            items.append(
                Recommendation(
                    id: "disk-churn",
                    title: "Disk churn is a first-class contributor right now",
                    detail: "Chau7 is currently moving about \(formatRate(entity.metrics.diskReadBps + entity.metrics.diskWriteBps)) across the app entity.",
                    tone: AetowerDesign.Tone.network,
                    footer: "Audit retained frame generation, cache writes, and session materialization paths."
                )
            )
        }

        if let breakdown = state.entityMemoryBreakdowns[entity.entityId],
           let mallocLarge = breakdown.regions.first(where: { $0.regionType == "MALLOC_LARGE" }),
           mallocLarge.swapBytes > 512 * 1024 * 1024
        {
            items.append(
                Recommendation(
                    id: "malloc-large",
                    title: "Large allocations are swapping aggressively",
                    detail: "MALLOC_LARGE currently has \(formatBytes(mallocLarge.swapBytes)) swapped and \(formatBytes(mallocLarge.residentBytes)) resident.",
                    tone: AetowerDesign.Status.error,
                    footer: "Investigate large retained buffers or repeated large-object churn before focusing on smaller optimizations."
                )
            )
        }

        if sessionSummaries.contains(where: \.approvalNeeded) {
            items.append(
                Recommendation(
                    id: "approval-queue",
                    title: "Pending approvals are blocking live Chau7 sessions",
                    detail: "\(sessionSummaries.filter(\.approvalNeeded).count) live Chau7 session(s) are waiting on approval.",
                    tone: AetowerDesign.Status.warning,
                    footer: "Resolve blocked sessions before concluding that the app itself is idle or healthy."
                )
            )
        }

        return Array(items.prefix(6))
    }

    private var forensicsItems: [ForensicsItem] {
        var items: [ForensicsItem] = []

        if let capability = chau7Capability {
            items.append(
                ForensicsItem(
                    id: "adapter-health",
                    title: "Adapter health",
                    detail: capability.detail,
                    tone: capability.health == .live ? AetowerDesign.Status.success : AetowerDesign.Status.warning,
                    timestampMillis: capability.lastUpdatedMillis
                )
            )
        }

        if let entity = chau7Entity {
            let peakCpu = entity.trend.cpuPercent.max() ?? entity.metrics.cpuPercent
            let peakWakeups = entity.trend.wakeupsPerSecond.max() ?? entity.metrics.wakeupsPerSecond
            let peakResident = entity.trend.memoryResidentBytes.max() ?? entity.metrics.memoryResidentBytes
            items.append(
                ForensicsItem(
                    id: "recent-peaks",
                    title: "Recent peak load",
                    detail: "CPU peaked at \(String(format: "%.0f%%", peakCpu)), wakeups at \(String(format: "%.0f/s", peakWakeups)), and resident memory at \(formatBytes(peakResident)) in the recent trend window.",
                    tone: AetowerDesign.Status.warning,
                    timestampMillis: state.snapshot.capturedAtMillis
                )
            )
        }

        items.append(contentsOf: chau7TimelineEvents.prefix(5).map { event in
            ForensicsItem(
                id: event.id,
                title: event.title,
                detail: event.detail.isEmpty ? "Aetower recorded a Chau7-related event." : event.detail,
                tone: severityColor(event.severity),
                timestampMillis: event.timestampMillis
            )
        })

        return items
    }

    private var chau7TimelineEvents: [TimelineEvent] {
        let ids = linkedEntityIDs.union(chau7Entity.map { [$0.entityId] } ?? [])
        return state.snapshot.timeline.filter { event in
            if let entityId = event.entityId, ids.contains(entityId) {
                return true
            }
            return event.title.localizedCaseInsensitiveContains("Chau7")
        }
        .sorted { $0.timestampMillis > $1.timestampMillis }
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
                headerSection

                if let entity = chau7Entity {
                    runtimeSummarySection(entity)
                    sessionMapSection
                    processTreeSection(entity)
                    memorySection(entity)
                    compareSection
                    approvalQueueSection
                    recommendationsSection
                    recentChangesSection
                    freezeForensicsSection
                } else {
                    unavailableState
                }
            }
            .padding(.horizontal, AetowerDesign.Spacing.xl)
            .padding(.vertical, AetowerDesign.Spacing.lg)
        }
        .navigationTitle("Chau7")
        .task {
            if state.historySnapshots.isEmpty && !state.historyIsLoading {
                state.loadHistory(force: true)
            }
            if let entityID = chau7Entity?.entityId {
                state.loadEntityStaticAnalysis(entityID: entityID, force: true)
            }
            initializeComparisonIfNeeded()
        }
        .onChange(of: state.historySnapshots.count) { _, _ in
            initializeComparisonIfNeeded()
        }
    }

    private var headerSection: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
            Text("Chau7")
                .font(.system(size: 28, weight: .semibold, design: .rounded))
            Text("App-focused operational view for Chau7: app core, embedded sessions, memory truth, before/after changes, and freeze forensics.")
                .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private func runtimeSummarySection(_ entity: EntitySnapshot) -> some View {
        let sessionsAtPrompt = sessionSummaries.filter { $0.status?.localizedCaseInsensitiveContains("prompt") == true }.count
        let approvals = sessionSummaries.filter(\.approvalNeeded).count

        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            AnalysisMetadataStrip(
                descriptor: AnalysisDescriptor(
                    label: "Live Chau7 state",
                    confidence: .measured,
                    overhead: .low,
                    detail: "Derived from the current Chau7 app entity, linked session entities, and adapter health."
                ),
                trailing: capabilityHealthLabel(chau7Capability?.health)
            )

            LazyVGrid(columns: [GridItem(.adaptive(minimum: 180), spacing: AetowerDesign.Spacing.md)], spacing: AetowerDesign.Spacing.md) {
                chau7MetricCard(
                    title: "CPU",
                    value: String(format: "%.1f%%", entity.metrics.cpuPercent),
                    subtitle: "peak \(String(format: "%.1f%%", entity.trend.cpuPercent.max() ?? entity.metrics.cpuPercent))",
                    samples: entity.trend.cpuPercent.map(Double.init),
                    tone: AetowerDesign.Tone.cpu
                )
                chau7MetricCard(
                    title: "Resident",
                    value: formatBytes(entity.metrics.memoryResidentBytes),
                    subtitle: "footprint \(formatBytes(entity.metrics.memoryPhysicalFootprintBytes))",
                    samples: entity.trend.memoryResidentBytes.map { Double($0) },
                    tone: AetowerDesign.Tone.memory
                )
                chau7MetricCard(
                    title: "Wakeups",
                    value: String(format: "%.0f/s", entity.metrics.wakeupsPerSecond),
                    subtitle: "peak \(String(format: "%.0f/s", entity.trend.wakeupsPerSecond.max() ?? entity.metrics.wakeupsPerSecond))",
                    samples: entity.trend.wakeupsPerSecond.map(Double.init),
                    tone: AetowerDesign.Status.warning
                )
                chau7MetricCard(
                    title: "Disk",
                    value: formatRate(entity.metrics.diskReadBps + entity.metrics.diskWriteBps),
                    subtitle: "energy \(formatEnergy(njPerS: entity.metrics.energyNjPerS))",
                    samples: entity.trend.diskActivityBps.map { Double($0) },
                    tone: AetowerDesign.Tone.network
                )
                chau7MetricCard(
                    title: "Sessions",
                    value: "\(sessionSummaries.count)",
                    subtitle: "\(sessionsAtPrompt) at prompt · \(approvals) approval",
                    samples: state.historySnapshots.suffix(24).map { Double($0.entities.filter(isChau7Linked).count) },
                    tone: AetowerDesign.Status.ready
                )
                chau7MetricCard(
                    title: "Build",
                    value: buildIdentity?.label ?? "Unknown build",
                    subtitle: buildIdentity?.timestamp ?? (chau7Capability?.detail ?? "No build metadata surfaced yet."),
                    samples: entity.trend.friction.map(Double.init),
                    tone: AetowerDesign.Tone.friction
                )
            }
        }
    }

    private var sessionMapSection: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            AnalysisMetadataStrip(
                descriptor: AnalysisDescriptor(
                    label: "Live session map",
                    confidence: .measured,
                    overhead: .low,
                    detail: "Aggregates Chau7-linked session entities by session id so users can separate app-core load from embedded runtime load."
                )
            )

            Text("Sessions")
                .font(.title3.weight(.semibold))

            if sessionSummaries.isEmpty {
                sectionPlaceholder("No linked Chau7 sessions are visible right now.")
            } else {
                ForEach(sessionSummaries) { session in
                    sessionRow(session)
                }
            }
        }
    }

    @ViewBuilder
    private func processTreeSection(_ entity: EntitySnapshot) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack {
                Text("Process tree")
                    .font(.title3.weight(.semibold))
                Spacer()
                Button("Refresh tree") {
                    state.loadEntityStaticAnalysis(entityID: entity.entityId, force: true)
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }

            AnalysisMetadataStrip(
                descriptor: EntityAnalysisKind.processTree.descriptor,
                updatedAt: state.entityAnalysisUpdatedAt(entity.entityId, kind: .processTree),
                trailing: "expanded family"
            )

            if let report = state.entityProcessTreeReports[entity.entityId] {
                HStack(spacing: AetowerDesign.Spacing.md) {
                    smallStat(label: "Grouped", value: "\(report.groupedProcessCount)")
                    smallStat(label: "Expanded", value: "\(report.expandedProcessCount)")
                    smallStat(label: "Seeds", value: "\(report.seedEntityIds.count)")
                }

                if !report.groupingReasons.isEmpty {
                    Text(report.groupingReasons.joined(separator: " · "))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }

                if !topBranches.isEmpty {
                    ForEach(topBranches.prefix(6)) { branch in
                        branchRow(branch)
                    }
                } else {
                    sectionPlaceholder("No burdened branches were derived from the current Chau7 process tree.")
                }
            } else if let error = state.entityAnalysisError(entity.entityId, kind: .processTree) {
                sectionPlaceholder(error)
            } else if state.entityAnalysisIsLoading(entity.entityId, kind: .processTree) {
                ProgressView("Building Chau7 process tree…")
            }
        }
    }

    @ViewBuilder
    private func memorySection(_ entity: EntitySnapshot) -> some View {
        let breakdown = state.entityMemoryBreakdowns[entity.entityId]
        let graphicsResident = breakdown.map { graphicsResidentBytes(in: $0) } ?? 0
        let mallocResident = breakdown.map { mallocResidentBytes(in: $0) } ?? 0

        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack {
                Text("Memory")
                    .font(.title3.weight(.semibold))
                Spacer()
                Button("Capture vmmap breakdown") {
                    state.runEntityMemoryBreakdown(entityID: entity.entityId)
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
                .disabled(state.entityAnalysisIsLoading(entity.entityId, kind: .memoryBreakdown))
            }

            AnalysisMetadataStrip(
                descriptor: EntityAnalysisKind.memoryBreakdown.descriptor,
                updatedAt: state.entityAnalysisUpdatedAt(entity.entityId, kind: .memoryBreakdown)
            )

            LazyVGrid(columns: [GridItem(.adaptive(minimum: 180), spacing: AetowerDesign.Spacing.md)], spacing: AetowerDesign.Spacing.md) {
                chau7MetricCard(
                    title: "Resident",
                    value: formatBytes(entity.metrics.memoryResidentBytes),
                    subtitle: "app entity resident",
                    samples: entity.trend.memoryResidentBytes.map { Double($0) },
                    tone: AetowerDesign.Tone.memory
                )
                chau7MetricCard(
                    title: "Footprint",
                    value: formatBytes(entity.metrics.memoryPhysicalFootprintBytes),
                    subtitle: "physical footprint",
                    samples: entity.trend.memoryResidentBytes.map { Double($0) },
                    tone: AetowerDesign.Tone.memory
                )
                chau7MetricCard(
                    title: "Malloc-backed",
                    value: formatBytes(mallocResident),
                    subtitle: breakdown == nil ? "capture vmmap for region truth" : "resident malloc regions",
                    samples: entity.trend.memoryResidentBytes.map { Double($0) },
                    tone: AetowerDesign.Status.warning
                )
                chau7MetricCard(
                    title: "Graphics-backed",
                    value: formatBytes(graphicsResident),
                    subtitle: breakdown == nil ? "capture vmmap for graphics split" : "IOAccel + IOSurface + CG image",
                    samples: entity.trend.memoryResidentBytes.map { Double($0) },
                    tone: AetowerDesign.Tone.gpu
                )
            }

            if state.entityAnalysisIsLoading(entity.entityId, kind: .memoryBreakdown) {
                ProgressView("Capturing live memory breakdown…")
            } else if let error = state.entityAnalysisError(entity.entityId, kind: .memoryBreakdown) {
                sectionPlaceholder(error)
            } else if let breakdown {
                Text(breakdown.memoryMetricNote)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)

                ForEach(breakdown.regions.prefix(6)) { region in
                    HStack {
                        Text(region.regionType)
                            .font(.subheadline.weight(.semibold))
                        Spacer()
                        Text(formatBytes(region.residentBytes))
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(.secondary)
                    }
                    .padding(.vertical, 4)
                }
            }
        }
    }

    private var compareSection: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            Text("Before / After")
                .font(.title3.weight(.semibold))

            AnalysisMetadataStrip(
                descriptor: AnalysisDescriptor(
                    label: "Persisted compare",
                    confidence: .persisted,
                    overhead: .low,
                    detail: "Compares Chau7-specific metrics across two selected persisted snapshots."
                )
            )

            if compareChoices.count >= 2 {
                HStack(spacing: AetowerDesign.Spacing.md) {
                    LabeledContent("Before") {
                        Picker("Before", selection: $compareBeforeMillis) {
                            ForEach(compareChoices) { choice in
                                Text(choice.label).tag(Optional(choice.millis))
                            }
                        }
                        .pickerStyle(.menu)
                        .frame(minWidth: 180)
                    }
                    LabeledContent("After") {
                        Picker("After", selection: $compareAfterMillis) {
                            ForEach(compareChoices) { choice in
                                Text(choice.label).tag(Optional(choice.millis))
                            }
                        }
                        .pickerStyle(.menu)
                        .frame(minWidth: 180)
                    }
                }
            }

            if let report = compareReport {
                Text("\(chau7Timestamp(report.beforeMillis)) → \(chau7Timestamp(report.afterMillis))")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                LazyVGrid(columns: [GridItem(.adaptive(minimum: 180), spacing: AetowerDesign.Spacing.md)], spacing: AetowerDesign.Spacing.md) {
                    deltaCard(title: "CPU", before: report.cpuBefore, after: report.cpuAfter, suffix: "%", tone: AetowerDesign.Tone.cpu)
                    deltaCard(title: "Resident", before: Double(report.residentBefore), after: Double(report.residentAfter), bytesMode: true, tone: AetowerDesign.Tone.memory)
                    deltaCard(title: "Footprint", before: Double(report.footprintBefore), after: Double(report.footprintAfter), bytesMode: true, tone: AetowerDesign.Tone.memory)
                    deltaCard(title: "Wakeups", before: report.wakeupsBefore, after: report.wakeupsAfter, rateMode: true, tone: AetowerDesign.Status.warning)
                    deltaCard(title: "Disk", before: Double(report.diskBefore), after: Double(report.diskAfter), bytesMode: true, rateMode: true, tone: AetowerDesign.Tone.network)
                    deltaCard(title: "Friction", before: report.frictionBefore, after: report.frictionAfter, tone: AetowerDesign.Tone.friction)
                }

                Text("Linked session count changed from \(report.sessionCountBefore) to \(report.sessionCountAfter).")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                sectionPlaceholder("Load at least two persisted snapshots to compare Chau7 across a selected window.")
            }
        }
    }

    private var approvalQueueSection: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            Text("Approval queue")
                .font(.title3.weight(.semibold))

            AnalysisMetadataStrip(
                descriptor: AnalysisDescriptor(
                    label: "Live approval state",
                    confidence: .measured,
                    overhead: .low,
                    detail: "Shows Chau7-linked sessions currently blocked on approval so users can separate workflow blockage from runtime load."
                )
            )

            let approvals = sessionSummaries.filter(\.approvalNeeded)
            if approvals.isEmpty {
                sectionPlaceholder("No Chau7-linked sessions are waiting on approval.")
            } else {
                ForEach(approvals) { session in
                    sessionRow(session)
                }
            }
        }
    }

    private var recommendationsSection: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            Text("Recommendations")
                .font(.title3.weight(.semibold))

            if recommendations.isEmpty {
                sectionPlaceholder("No Chau7-specific recommendations are available yet.")
            } else {
                ForEach(recommendations) { recommendation in
                    recommendationRow(recommendation)
                }
            }
        }
    }

    private var recentChangesSection: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            Text("Recent changes")
                .font(.title3.weight(.semibold))

            AnalysisMetadataStrip(
                descriptor: AnalysisDescriptor(
                    label: "Timeline correlation",
                    confidence: .inferred,
                    overhead: .low,
                    detail: "Correlates Chau7-related timeline, anomaly, and friction events from the current snapshot."
                )
            )

            if chau7TimelineEvents.isEmpty {
                sectionPlaceholder("No Chau7-specific timeline events are present in the current snapshot.")
            } else {
                ForEach(chau7TimelineEvents.prefix(10), id: \.id) { event in
                    timelineRow(event)
                }
            }
        }
    }

    private var freezeForensicsSection: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack {
                Text("Freeze / Crash Forensics")
                    .font(.title3.weight(.semibold))
                Spacer()
                if let entity = chau7Entity {
                    Button("Run 5s sample") {
                        state.runEntityProfile(entityID: entity.entityId)
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .disabled(state.entityAnalysisIsLoading(entity.entityId, kind: .profile))

                    Button("Sample wakeups") {
                        state.runEntityWakeupAttribution(entityID: entity.entityId)
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .disabled(state.entityAnalysisIsLoading(entity.entityId, kind: .wakeupAttribution))
                }
            }

            if let entity = chau7Entity {
                AnalysisMetadataStrip(
                    descriptor: EntityAnalysisKind.profile.descriptor,
                    updatedAt: state.entityAnalysisUpdatedAt(entity.entityId, kind: .profile),
                    trailing: "explicit capture"
                )
            }

            ForEach(forensicsItems) { item in
                forensicsRow(item)
            }

            if let entity = chau7Entity {
                if state.entityAnalysisIsLoading(entity.entityId, kind: .profile) {
                    ProgressView("Collecting sampled Chau7 profile…")
                } else if let profile = state.entityProfiles[entity.entityId] {
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                        Text(profile.summary)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                        ForEach(profile.topStacks.prefix(4)) { stack in
                            sampledStackRow(stack)
                        }
                    }
                }

                if state.entityAnalysisIsLoading(entity.entityId, kind: .wakeupAttribution) {
                    ProgressView("Collecting Chau7 wakeup attribution…")
                } else if let attribution = state.entityWakeupAttributions[entity.entityId] {
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                        if let dominantCause = attribution.dominantCause {
                            Text(dominantCause)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        ForEach(attribution.queueBreakdown.prefix(4)) { stack in
                            sampledStackRow(stack)
                        }
                        if !attribution.caveats.isEmpty {
                            Text(attribution.caveats.joined(separator: " "))
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
            }
        }
    }

    private var unavailableState: some View {
        ContentUnavailableView(
            "No live Chau7 entity",
            systemImage: "terminal",
            description: Text(chau7Capability?.detail ?? "Aetower cannot currently see a live Chau7 app entity.")
        )
        .frame(maxWidth: .infinity, minHeight: 320)
    }

    private func chau7MetricCard(
        title: String,
        value: String,
        subtitle: String,
        samples: [Double],
        tone: Color
    ) -> some View {
        MetricCardSurface(tone: tone, samples: samples, minHeight: 124) {
            VStack(alignment: .leading, spacing: 4) {
                Text(title)
                    .font(.caption2.weight(.medium))
                    .foregroundStyle(.secondary)
                Spacer(minLength: 0)
                Text(value)
                    .font(.system(size: 24, weight: .bold, design: .rounded))
                    .lineLimit(1)
                    .minimumScaleFactor(0.75)
                Text(subtitle)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
            }
        } hoverOverlay: {
            EmptyView()
        }
    }

    private func sessionRow(_ session: SessionSummary) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(alignment: .firstTextBaseline) {
                Text(session.title)
                    .font(.headline)
                if let provider = session.provider {
                    Text("·")
                        .foregroundStyle(.tertiary)
                    Text(provider)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                if let status = session.status, !status.isEmpty {
                    stateBadge(status.replacingOccurrences(of: "-", with: " ").capitalized, color: statusTone(status))
                }
                if session.approvalNeeded {
                    stateBadge("Approval needed", color: AetowerDesign.Status.warning)
                }
                if let sessionId = session.sessionId {
                    stateBadge(shortSessionId(sessionId), color: AetowerDesign.Status.ready)
                }
            }

            if let workspace = session.workspace {
                Text(shortenPath(workspace))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            HStack(spacing: AetowerDesign.Spacing.sm) {
                metricPill("CPU \(String(format: "%.0f%%", session.totalCpuPercent))", color: AetowerDesign.Tone.cpu)
                metricPill(formatBytes(session.totalResidentBytes), color: AetowerDesign.Tone.memory)
                if session.totalFootprintBytes > 0 {
                    metricPill("fp \(formatBytes(session.totalFootprintBytes))", color: AetowerDesign.Tone.memory)
                }
                if session.totalWakeups > 0 {
                    metricPill("\(String(format: "%.0f", session.totalWakeups))/s", color: AetowerDesign.Status.warning)
                }
                if session.totalDiskBps > 0 {
                    metricPill(formatRate(session.totalDiskBps), color: AetowerDesign.Tone.network)
                }
            }

            if let detail = session.detail, !detail.isEmpty {
                Text(detail)
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md))
    }

    private func branchRow(_ branch: ProcessTreeNodeReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
            HStack {
                Text(branch.title)
                    .font(.subheadline.weight(.semibold))
                Spacer()
                Text(String(format: "%.1f%% CPU", branch.subtreeCpuPercent))
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
            Text("\(branch.subtreeProcessCount) processes · \(formatBytes(branch.subtreeMemoryBytes)) subtree memory")
                .font(.caption)
                .foregroundStyle(.secondary)
            if !branch.badges.isEmpty {
                Text(branch.badges.joined(separator: " · "))
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                    .lineLimit(2)
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md))
    }

    private func deltaCard(
        title: String,
        before: Double,
        after: Double,
        suffix: String = "",
        bytesMode: Bool = false,
        rateMode: Bool = false,
        tone: Color
    ) -> some View {
        let delta = after - before
        return MetricCardSurface(tone: tone, samples: [before, after], minHeight: 116) {
            VStack(alignment: .leading, spacing: 4) {
                Text(title)
                    .font(.caption2.weight(.medium))
                    .foregroundStyle(.secondary)
                Spacer(minLength: 0)
                Text(deltaLabel(delta, suffix: suffix, bytesMode: bytesMode, rateMode: rateMode))
                    .font(.system(size: 22, weight: .bold, design: .rounded))
                    .lineLimit(1)
                Text("from \(valueLabel(before, suffix: suffix, bytesMode: bytesMode, rateMode: rateMode)) to \(valueLabel(after, suffix: suffix, bytesMode: bytesMode, rateMode: rateMode))")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
            }
        } hoverOverlay: {
            EmptyView()
        }
    }

    private func recommendationRow(_ recommendation: Recommendation) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
            HStack {
                Text(recommendation.title)
                    .font(.headline)
                Spacer()
                Circle()
                    .fill(recommendation.tone)
                    .frame(width: 8, height: 8)
            }
            Text(recommendation.detail)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            Text(recommendation.footer)
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md))
    }

    private func timelineRow(_ event: TimelineEvent) -> some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
            Circle()
                .fill(severityColor(event.severity))
                .frame(width: 7, height: 7)
                .padding(.top, AetowerDesign.Spacing.xs)
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                Text(event.title)
                    .font(.caption.weight(.medium))
                if !event.detail.isEmpty {
                    Text(event.detail)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer()
            Text(chau7Timestamp(event.timestampMillis))
                .font(.caption2)
                .foregroundStyle(.tertiary)
        }
    }

    private func forensicsRow(_ item: ForensicsItem) -> some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
            Rectangle()
                .fill(item.tone)
                .frame(width: 3)
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                Text(item.title)
                    .font(.caption.weight(.medium))
                Text(item.detail)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer()
            if let timestamp = item.timestampMillis {
                Text(chau7Timestamp(timestamp))
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
    }

    private func sampledStackRow(_ stack: SampledStackReportModel) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(stack.queueLabel ?? stack.threadLabel)
                    .font(.subheadline.weight(.semibold))
                Spacer()
                Text("\(stack.sampleCount) samples")
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
            Text(stack.classification)
                .font(.caption2.weight(.medium))
                .foregroundStyle(.tertiary)
            if !stack.topFrames.isEmpty {
                Text(stack.topFrames.prefix(3).joined(separator: " → "))
                    .font(.caption2.monospaced())
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md))
    }

    private func sectionPlaceholder(_ text: String) -> some View {
        Text(text)
            .font(.caption)
            .foregroundStyle(.secondary)
            .padding(AetowerDesign.Spacing.md)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md))
    }

    private func smallStat(label: String, value: String) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
            Text(label)
                .font(.caption2)
                .foregroundStyle(.secondary)
            Text(value)
                .font(.caption.weight(.semibold))
        }
        .padding(.horizontal, AetowerDesign.Spacing.sm)
        .padding(.vertical, AetowerDesign.Spacing.xs)
        .background(Color.secondary.opacity(0.08), in: Capsule())
    }

    private func stateBadge(_ label: String, color: Color) -> some View {
        Text(label)
            .font(.caption2)
            .fontWeight(.medium)
            .foregroundStyle(color)
            .padding(.horizontal, AetowerDesign.Spacing.sm)
            .padding(.vertical, AetowerDesign.Spacing.xxs)
            .background(color.opacity(0.08), in: Capsule())
    }

    private func metricPill(_ label: String, color: Color) -> some View {
        Text(label)
            .font(.caption)
            .fontWeight(.medium)
            .foregroundStyle(color)
            .padding(.horizontal, AetowerDesign.Spacing.sm)
            .padding(.vertical, AetowerDesign.Spacing.xxs)
            .background(color.opacity(0.08), in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm))
    }

    private func initializeComparisonIfNeeded() {
        let sorted = state.historySnapshots.sorted { $0.capturedAtMillis < $1.capturedAtMillis }
        guard compareBeforeMillis == nil || compareAfterMillis == nil else { return }
        compareBeforeMillis = sorted.first?.capturedAtMillis
        compareAfterMillis = sorted.last?.capturedAtMillis
    }

    private func isChau7Linked(_ entity: EntitySnapshot) -> Bool {
        entity.badges.contains("chau7-live")
            || entity.components.contains { $0.adapterContext?.kind == .chau7Session }
    }

    private func sessionComponent(for entity: EntitySnapshot) -> ComponentSnapshot? {
        entity.components.first { $0.adapterContext?.kind == .chau7Session }
    }

    private func sessionContext(for entity: EntitySnapshot) -> AdapterContextSnapshot? {
        sessionComponent(for: entity)?.adapterContext
    }

    private func providerLabel(for entity: EntitySnapshot) -> String? {
        if let title = sessionComponent(for: entity)?.title,
           let prefix = title.components(separatedBy: " · ").first
        {
            let trimmed = prefix.trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmed.isEmpty {
                return trimmed
            }
        }

        let providerBadges = entity.badges.filter {
            $0 != "ai-agent"
                && $0 != "chau7-live"
                && !$0.hasPrefix("ai-session:")
                && ![
                    "approval-needed",
                    "delegating",
                    "cto-active",
                    "at-prompt",
                    "shell-loading",
                    "agent-error",
                    "agent-finished",
                ].contains($0)
        }
        return providerBadges.first.map { $0.replacingOccurrences(of: "-", with: " ").capitalized }
    }

    private func projectContext(for entity: EntitySnapshot) -> String? {
        func rankedPath(_ component: ComponentSnapshot) -> (Int, String)? {
            if let repoRoot = component.adapterContext?.repoRoot, !repoRoot.isEmpty {
                let rank = component.adapterContext?.kind == .chau7Session ? 0 : 1
                return (rank, repoRoot)
            }

            if let workspacePath = component.adapterContext?.workspacePath, !workspacePath.isEmpty {
                return (2, workspacePath)
            }

            if let cwd = component.cwd, !cwd.isEmpty {
                return (3, cwd)
            }

            return nil
        }

        return entity.components
            .compactMap(rankedPath)
            .sorted { lhs, rhs in
                if lhs.0 != rhs.0 {
                    return lhs.0 < rhs.0
                }
                return lhs.1.localizedCaseInsensitiveCompare(rhs.1) == .orderedAscending
            }
            .first?
            .1
    }

    private func graphicsResidentBytes(in breakdown: EntityMemoryBreakdownReportModel) -> UInt64 {
        breakdown.regions
            .filter {
                $0.regionType.contains("IOAccelerator")
                    || $0.regionType.contains("IOSurface")
                    || $0.regionType.contains("CG image")
            }
            .reduce(0) { $0 + $1.residentBytes }
    }

    private func mallocResidentBytes(in breakdown: EntityMemoryBreakdownReportModel) -> UInt64 {
        breakdown.regions
            .filter { $0.regionType.hasPrefix("MALLOC_") }
            .reduce(0) { $0 + $1.residentBytes }
    }

    private func statusTone(_ status: String) -> Color {
        let normalized = status.localizedLowercase
        if normalized.contains("approval") || normalized.contains("error") {
            return AetowerDesign.Status.warning
        }
        if normalized.contains("finished") || normalized.contains("prompt") || normalized.contains("idle") {
            return AetowerDesign.Status.success
        }
        return AetowerDesign.Status.ready
    }

    private func severityColor(_ severity: TimelineSeverity) -> Color {
        switch severity {
        case .critical: return AetowerDesign.Status.error
        case .warning: return AetowerDesign.Status.warning
        case .info: return AetowerDesign.Status.ready
        }
    }

    private func capabilityHealthLabel(_ health: CapabilityHealth?) -> String? {
        guard let health else { return nil }
        switch health {
        case .configured: return "Configured"
        case .live: return "Live"
        case .cached: return "Cached"
        case .degraded: return "Degraded"
        }
    }

    private func valueLabel(_ value: Double, suffix: String, bytesMode: Bool, rateMode: Bool) -> String {
        if bytesMode {
            if rateMode {
                return formatRate(UInt64(max(0, value)))
            }
            return formatBytes(UInt64(max(0, value)))
        }
        if rateMode {
            return String(format: "%.0f/s", value)
        }
        return String(format: "%.1f%@", value, suffix)
    }

    private func deltaLabel(_ delta: Double, suffix: String, bytesMode: Bool, rateMode: Bool) -> String {
        let prefix = delta > 0 ? "+" : delta < 0 ? "-" : ""
        let magnitude = abs(delta)
        return prefix + valueLabel(magnitude, suffix: suffix, bytesMode: bytesMode, rateMode: rateMode)
    }

    private func formatEnergy(njPerS: Double) -> String {
        let mw = njPerS / 1_000_000
        if mw >= 1000 {
            return String(format: "%.1f W", mw / 1000)
        } else if mw >= 1 {
            return String(format: "%.0f mW", mw)
        } else {
            return "0 mW"
        }
    }

    private func shortenPath(_ path: String) -> String {
        if let home = FileManager.default.homeDirectoryForCurrentUser.path.removingPercentEncoding,
           path.hasPrefix(home)
        {
            return "~" + path.dropFirst(home.count)
        }
        return path
    }

    private func shortSessionId(_ sessionID: String) -> String {
        if sessionID.count <= 10 {
            return sessionID
        }
        return String(sessionID.prefix(8))
    }
}

private func chau7Timestamp(_ millis: UInt64?) -> String {
    guard let millis else {
        return "n/a"
    }
    let date = Date(timeIntervalSince1970: TimeInterval(millis) / 1000)
    return date.formatted(date: .abbreviated, time: .shortened)
}
