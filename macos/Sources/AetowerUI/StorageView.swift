import AppKit
import SwiftUI

private struct StorageGrowthTimelineEvent: Identifiable {
    let id: String
    let timestampMillis: UInt64?
    let repoName: String?
    let repoRoot: String?
    let branch: String?
    let displayName: String
    let path: String
    let cleanupTier: String
    let deltaBytes: Int64
    let previousBytes: UInt64
    let currentBytes: UInt64
    let command: String?
    let processTree: String?
    let aiAgentSession: String?
}

public struct StorageView: View {
    let state: AppState
    @State private var selectedFilter: StorageFilter = .attention
    @State private var searchText = ""
    @State private var customRoot = ""
    @State private var maxDepth = 5.0

    public init(state: AppState) {
        self.state = state
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
                header
                controls
                safetyBanner

                if let error = state.storageHygieneError {
                    warningBanner(error)
                }

                if let report = state.storageHygieneReport {
                    summaryGrid(report)
                    cleanupPreviewSection(report)
                    repoFootprintDashboard(report)
                    storageGrowthTimeline(report)
                    if report.truncated {
                        warningBanner("The scan hit a cap or time budget. Results are partial; narrow the root or refresh when the machine is idle.")
                    }
                    itemSection(report)
                    rootsSection(report)
                    caveatsSection(report)
                } else if state.storageHygieneIsLoading {
                    loadingSection
                } else {
                    emptySection
                }
            }
            .frame(maxWidth: 1120, alignment: .leading)
            .padding(AetowerDesign.Spacing.xxl)
        }
        .task {
            state.ensureStorageHygieneScan()
        }
    }

    private var header: some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.lg) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                Text("Storage")
                    .font(.system(size: 28, weight: .semibold, design: .rounded))
                Text("Find local build artifacts, logs, caches, and dependency trees that make development machines drift over time.")
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer()
            if state.storageHygieneIsLoading {
                ProgressView()
                    .controlSize(.small)
            }
        }
    }

    private var controls: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                TextField("Optional root, for example ~/Repositories", text: $customRoot)
                    .aetowerUtilityTextInput()
                    .textFieldStyle(.roundedBorder)
                Stepper(
                    "Depth \(Int(maxDepth))",
                    value: $maxDepth,
                    in: 1...12,
                    step: 1
                )
                .frame(width: 130)
                Button {
                    runScan()
                } label: {
                    Label("Scan", systemImage: "arrow.clockwise")
                }
                .buttonStyle(.borderedProminent)
                .disabled(state.storageHygieneIsLoading)
            }

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Picker("Filter", selection: $selectedFilter) {
                    ForEach(StorageFilter.allCases) { filter in
                        Text(filter.label).tag(filter)
                    }
                }
                .pickerStyle(.segmented)
                .frame(maxWidth: 680)

                TextField("Search paths or kinds", text: $searchText)
                    .aetowerUtilityTextInput()
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: 260)
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private var safetyBanner: some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: "shield.checkered")
                .foregroundStyle(AetowerDesign.Status.ready)
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                Text("Read-only inventory")
                    .font(.headline)
                Text("Aetower does not delete files from this view. It estimates size, age, and cleanup confidence, then gives reveal/copy actions so operators stay in control.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Status.ready.opacity(0.10))
    }

    private func summaryGrid(_ report: StorageHygieneReportModel) -> some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 170), spacing: AetowerDesign.Spacing.md)],
            alignment: .leading,
            spacing: AetowerDesign.Spacing.md
        ) {
            summaryCard(
                "Reclaimable",
                value: formatBytes(report.summary.totalReclaimableBytes),
                detail: "bounded estimate",
                systemImage: "externaldrive.badge.minus",
                tone: AetowerDesign.Tone.disk
            )
            summaryCard(
                "Candidates",
                value: "\(report.summary.itemCount)",
                detail: "\(report.summary.safeCandidateCount) expected artifacts",
                systemImage: "shippingbox",
                tone: AetowerDesign.Tone.cpu
            )
            summaryCard(
                "Review",
                value: "\(report.summary.reviewCandidateCount)",
                detail: "needs operator decision",
                systemImage: "eye",
                tone: report.summary.reviewCandidateCount > 0 ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
            )
            summaryCard(
                "Stale",
                value: "\(report.summary.staleCandidateCount)",
                detail: "older than 7 days",
                systemImage: "calendar.badge.clock",
                tone: report.summary.staleCandidateCount > 0 ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
            )
            summaryCard(
                "Scanned",
                value: "\(report.summary.scannedDirectoryCount)",
                detail: "\(report.scanDurationMillis) ms",
                systemImage: "folder.badge.gearshape",
                tone: AetowerDesign.Tone.network
            )
            summaryCard(
                "Largest",
                value: formatBytes(report.summary.largestItemBytes),
                detail: report.summary.largestItemPath.map(lastPathComponent) ?? "none",
                systemImage: "arrow.up.left.and.arrow.down.right",
                tone: AetowerDesign.Tone.energy
            )
            summaryCard(
                "Attributed",
                value: "\(report.summary.attributedRepoCount)",
                detail: "repo/branch-linked artifacts",
                systemImage: "point.3.connected.trianglepath.dotted",
                tone: report.summary.attributedRepoCount > 0 ? AetowerDesign.Status.ready : .secondary
            )
        }
    }

    private func cleanupPreviewSection(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                Text("Safe cleanup preview")
                    .font(.headline)
                Text("Aetower classifies candidates by cleanup risk. This view never deletes files; it only explains what should be safe, rebuildable, expensive, or risky.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 210), spacing: AetowerDesign.Spacing.md)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.md
            ) {
                ForEach(report.cleanupTiers) { tier in
                    cleanupTierCard(tier)
                }
            }
        }
    }

    private func repoFootprintDashboard(_ report: StorageHygieneReportModel) -> some View {
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                Text("Repo footprint dashboard")
                    .font(.headline)
                Text("Per repository artifact footprint, top growth sources, branch context, last writer evidence, and estimated rebuild cost.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            if report.repoFootprints.isEmpty {
                Label("No artifacts could be tied to an enclosing Git repository.", systemImage: "questionmark.folder")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(AetowerDesign.Spacing.md)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            } else {
                LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    ForEach(report.repoFootprints) { footprint in
                        repoFootprintCard(footprint)
                    }
                }
            }
        }
    }

    private func repoFootprintCard(_ footprint: StorageRepoFootprintModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: "folder.badge.gearshape")
                    .foregroundStyle(AetowerDesign.Tone.disk)
                    .frame(width: 22)

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        Text(footprint.repoName)
                            .font(.subheadline.weight(.semibold))
                        if let branch = footprint.lastBranchTouched {
                            Text(branch)
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(AetowerDesign.Status.ready)
                                .padding(.horizontal, 7)
                                .padding(.vertical, 3)
                                .background(AetowerDesign.Status.ready.opacity(0.12), in: Capsule())
                        }
                    }
                    Text(footprint.repoRoot)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }

                Spacer(minLength: AetowerDesign.Spacing.md)

                VStack(alignment: .trailing, spacing: AetowerDesign.Spacing.xs) {
                    Text(formatBytes(footprint.currentSizeBytes))
                        .font(.system(size: 20, weight: .semibold, design: .rounded))
                    Text(storageGrowthLabel(for: footprint))
                        .font(.caption2)
                        .foregroundStyle(storageGrowthTone(for: footprint))
                }
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 150), spacing: AetowerDesign.Spacing.sm)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.sm
            ) {
                footprintMetric(
                    "Artifacts",
                    value: "\(footprint.itemCount)",
                    detail: formatBytes(footprint.artifactBytes)
                )
                footprintMetric(
                    "Rebuild cost",
                    value: footprint.estimatedRebuildCost,
                    detail: rebuildTimeLabel(footprint.estimatedRebuildSeconds)
                )
                footprintMetric(
                    "Last writer",
                    value: footprint.lastWriterProcess ?? "Unknown",
                    detail: footprint.lastWriterPid.map { "pid \($0)" } ?? "needs file-event journal"
                )
                footprintMetric(
                    "Growth",
                    value: storageGrowthCompactValue(for: footprint),
                    detail: storageGrowthWindow(for: footprint)
                )
            }

            if !footprint.topArtifactFolders.isEmpty {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Top artifact folders")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    ForEach(footprint.topArtifactFolders.prefix(3)) { folder in
                        HStack(spacing: AetowerDesign.Spacing.sm) {
                            Image(systemName: cleanupTierIcon(folder.cleanupTier))
                                .foregroundStyle(tone(forCleanupTier: folder.cleanupTier))
                                .frame(width: 16)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(folder.displayName)
                                    .font(.caption.weight(.semibold))
                                Text(folder.path)
                                    .font(.system(size: 10, design: .monospaced))
                                    .foregroundStyle(.tertiary)
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                            }
                            Spacer()
                            Text(formatBytes(folder.sizeBytes))
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func storageGrowthTimeline(_ report: StorageHygieneReportModel) -> some View {
        let events = storageGrowthEvents(from: report)
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                Text("Why did disk usage jump?")
                    .font(.headline)
                Text("Positive artifact-size deltas since the previous scan, correlated with repo, branch, command, process tree, and AI session when the scan has direct evidence.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            if state.previousStorageHygieneReport == nil {
                Label("Run a second scan to establish a growth timeline. The first scan becomes the baseline.", systemImage: "timeline.selection")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(AetowerDesign.Spacing.md)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            } else if events.isEmpty {
                Label("No meaningful storage jumps were detected since the previous scan.", systemImage: "checkmark.circle")
                    .font(.caption)
                    .foregroundStyle(AetowerDesign.Status.ready)
                    .padding(AetowerDesign.Spacing.md)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            } else {
                LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    ForEach(events) { event in
                        storageGrowthTimelineRow(event)
                    }
                }
            }
        }
    }

    private func storageGrowthTimelineRow(_ event: StorageGrowthTimelineEvent) -> some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
            VStack(spacing: 4) {
                Circle()
                    .fill(tone(forCleanupTier: event.cleanupTier))
                    .frame(width: 9, height: 9)
                Rectangle()
                    .fill(Color.secondary.opacity(0.18))
                    .frame(width: 1)
            }
            .frame(width: 18)

            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                HStack(spacing: AetowerDesign.Spacing.sm) {
                    Text(storageGrowthEventTime(event))
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    Text("+\(formatBytes(UInt64(event.deltaBytes)))")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(AetowerDesign.Status.warning)
                    if let branch = event.branch {
                        Text(branch)
                            .font(.caption2.weight(.semibold))
                            .foregroundStyle(AetowerDesign.Status.ready)
                            .padding(.horizontal, 7)
                            .padding(.vertical, 3)
                            .background(AetowerDesign.Status.ready.opacity(0.12), in: Capsule())
                    }
                }

                Text(storageGrowthEventTitle(event))
                    .font(.subheadline.weight(.semibold))
                    .fixedSize(horizontal: false, vertical: true)

                Text(event.path)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .lineLimit(1)
                    .truncationMode(.middle)

                Text(storageGrowthCorrelationDetail(event))
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Spacer(minLength: AetowerDesign.Spacing.md)

            VStack(alignment: .trailing, spacing: AetowerDesign.Spacing.xs) {
                Text(formatBytes(event.currentBytes))
                    .font(.caption.weight(.semibold))
                Text("was \(formatBytes(event.previousBytes))")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func itemSection(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Artifacts")
                        .font(.headline)
                    Text("\(filteredItems(from: report).count) visible of \(report.items.count) reported candidate(s).")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }

            if filteredItems(from: report).isEmpty {
                ContentUnavailableView(
                    "No matching artifacts",
                    systemImage: "line.3.horizontal.decrease.circle",
                    description: Text("Change the filter, search text, root, or depth and scan again.")
                )
            } else {
                LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    ForEach(filteredItems(from: report)) { item in
                        artifactRow(item)
                    }
                }
            }
        }
    }

    private func artifactRow(_ item: StorageHygieneItemModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: icon(for: item))
                    .foregroundStyle(tone(for: item))
                    .frame(width: 22)

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        Text(item.displayName)
                            .font(.subheadline.weight(.semibold))
                        cleanupTierBadge(item)
                        safetyBadge(item)
                        if item.stale {
                            Text("Stale")
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(AetowerDesign.Status.warning)
                                .padding(.horizontal, 7)
                                .padding(.vertical, 3)
                                .background(AetowerDesign.Status.warning.opacity(0.12), in: Capsule())
                        }
                        if item.sizeTruncated {
                            Text("Partial size")
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(.secondary)
                                .padding(.horizontal, 7)
                                .padding(.vertical, 3)
                                .background(AetowerDesign.Surface.badge, in: Capsule())
                        }
                    }

                    Text(item.path)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                        .lineLimit(1)
                        .truncationMode(.middle)

                    Text(item.reason)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(item.recommendation)
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                    Label(attributionSummary(for: item), systemImage: "point.3.connected.trianglepath.dotted")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                        .lineLimit(2)
                }

                Spacer(minLength: AetowerDesign.Spacing.md)

                VStack(alignment: .trailing, spacing: AetowerDesign.Spacing.xs) {
                    Text(formatBytes(item.sizeBytes))
                        .font(.system(size: 18, weight: .semibold, design: .rounded))
                    Text(ageLabel(item))
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Button("Reveal") { reveal(path: item.path) }
                Button("Copy path") { copy(item.path) }
                Button("Copy command") { copy(item.commandHint) }
                Spacer()
                Text(item.kind)
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func rootsSection(_ report: StorageHygieneReportModel) -> some View {
        DisclosureGroup("Scanned and skipped roots") {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                ForEach(report.roots, id: \.self) { root in
                    rootLine(root, detail: "scanned", systemImage: "checkmark.circle")
                }
                ForEach(report.skippedRoots) { root in
                    rootLine(root.path, detail: root.reason, systemImage: "exclamationmark.triangle")
                }
            }
            .padding(.top, AetowerDesign.Spacing.sm)
        }
        .font(.caption)
    }

    private func caveatsSection(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            Text("Caveats")
                .font(.headline)
            ForEach(report.caveats, id: \.self) { caveat in
                Label(caveat, systemImage: "info.circle")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private var loadingSection: some View {
        VStack(spacing: AetowerDesign.Spacing.md) {
            ProgressView()
            Text("Scanning bounded developer storage roots...")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, minHeight: 220)
    }

    private var emptySection: some View {
        ContentUnavailableView(
            "No storage report yet",
            systemImage: "externaldrive",
            description: Text("Run a read-only scan to inventory local development artifacts.")
        )
        .frame(maxWidth: .infinity, minHeight: 220)
    }

    private func summaryCard(
        _ title: String,
        value: String,
        detail: String,
        systemImage: String,
        tone: Color
    ) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Image(systemName: systemImage)
                    .foregroundStyle(tone)
                Text(title.uppercased())
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            Text(value)
                .font(.system(size: 22, weight: .semibold, design: .rounded))
                .lineLimit(1)
                .minimumScaleFactor(0.75)
            Text(detail)
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .lineLimit(2)
        }
        .padding(AetowerDesign.Spacing.md)
        .frame(maxWidth: .infinity, minHeight: 112, alignment: .topLeading)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func cleanupTierCard(_ tier: StorageCleanupTierModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Image(systemName: cleanupTierIcon(tier.tier))
                    .foregroundStyle(tone(forCleanupTier: tier.tier))
                Text(tier.label.uppercased())
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            HStack(alignment: .firstTextBaseline) {
                Text(formatBytes(tier.bytes))
                    .font(.system(size: 21, weight: .semibold, design: .rounded))
                    .lineLimit(1)
                    .minimumScaleFactor(0.75)
                Spacer()
                Text("\(tier.itemCount)")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(tone(forCleanupTier: tier.tier))
            }
            Text(tier.description)
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .lineLimit(3)
        }
        .padding(AetowerDesign.Spacing.md)
        .frame(maxWidth: .infinity, minHeight: 126, alignment: .topLeading)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func footprintMetric(
        _ title: String,
        value: String,
        detail: String
    ) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title.uppercased())
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.tertiary)
            Text(value)
                .font(.caption.weight(.semibold))
                .lineLimit(1)
                .minimumScaleFactor(0.75)
            Text(detail)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(2)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }

    private func cleanupTierBadge(_ item: StorageHygieneItemModel) -> some View {
        Text(cleanupTierLabel(item.cleanupTier))
            .font(.caption2.weight(.semibold))
            .foregroundStyle(tone(forCleanupTier: item.cleanupTier))
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(tone(forCleanupTier: item.cleanupTier).opacity(0.12), in: Capsule())
    }

    private func safetyBadge(_ item: StorageHygieneItemModel) -> some View {
        Text(item.safety == "safe" ? "Expected artifact" : "Review")
            .font(.caption2.weight(.semibold))
            .foregroundStyle(tone(for: item))
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(tone(for: item).opacity(0.12), in: Capsule())
    }

    private func rootLine(_ path: String, detail: String, systemImage: String) -> some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: systemImage)
                .foregroundStyle(detail == "scanned" ? AetowerDesign.Status.ready : AetowerDesign.Status.warning)
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                Text(path)
                    .font(.system(size: 11, design: .monospaced))
                    .textSelection(.enabled)
                Text(detail)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func warningBanner(_ text: String) -> some View {
        Label(text, systemImage: "exclamationmark.triangle")
            .font(.caption)
            .foregroundStyle(AetowerDesign.Status.warning)
            .padding(AetowerDesign.Spacing.md)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(AetowerDesign.Status.warning.opacity(0.12))
    }

    private func filteredItems(from report: StorageHygieneReportModel) -> [StorageHygieneItemModel] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return report.items.filter { item in
            selectedFilter.matches(item)
                && (
                    query.isEmpty
                        || item.path.lowercased().contains(query)
                        || item.kind.lowercased().contains(query)
                        || item.reason.lowercased().contains(query)
                        || item.cleanupTier.lowercased().contains(query)
                        || (item.attribution.repoName?.lowercased().contains(query) ?? false)
                        || (item.attribution.gitBranch?.lowercased().contains(query) ?? false)
                )
        }
    }

    private func runScan() {
        let root = customRoot.trimmingCharacters(in: .whitespacesAndNewlines)
        state.runStorageHygieneScan(
            roots: root.isEmpty ? [] : [root],
            maxDepth: UInt32(maxDepth),
            limit: 120
        )
    }

    private func attributionSummary(for item: StorageHygieneItemModel) -> String {
        var parts: [String] = []
        if let repoName = item.attribution.repoName {
            parts.append("repo \(repoName)")
        }
        if let branch = item.attribution.gitBranch {
            parts.append("branch \(branch)")
        } else if let head = item.attribution.gitHead {
            parts.append("head \(head)")
        }
        if let command = item.attribution.command {
            parts.append("command \(command)")
        }
        if let session = item.attribution.aiAgentSession {
            parts.append("session \(session)")
        }
        if parts.isEmpty {
            return "No repo/session attribution available"
        }
        if item.attribution.command == nil && item.attribution.aiAgentSession == nil {
            parts.append("runtime link unavailable")
        }
        return parts.joined(separator: " · ")
    }

    private func storageGrowthEvents(
        from report: StorageHygieneReportModel
    ) -> [StorageGrowthTimelineEvent] {
        guard let previousReport = state.previousStorageHygieneReport else {
            return []
        }
        let previousItemsByID = previousReport.items.reduce(into: [String: StorageHygieneItemModel]()) {
            $0[$1.id] = $1
        }
        let minimumDeltaBytes: Int64 = 8 * 1_024 * 1_024
        return report.items.compactMap { item in
            let previousBytes = previousItemsByID[item.id]?.sizeBytes ?? 0
            let delta = Int64(clamping: item.sizeBytes) - Int64(clamping: previousBytes)
            guard delta >= minimumDeltaBytes else {
                return nil
            }
            return StorageGrowthTimelineEvent(
                id: item.id,
                timestampMillis: item.modifiedMillis ?? report.capturedAtMillis,
                repoName: item.attribution.repoName,
                repoRoot: item.attribution.repoRoot,
                branch: item.attribution.gitBranch ?? item.attribution.gitHead,
                displayName: item.displayName,
                path: item.path,
                cleanupTier: item.cleanupTier,
                deltaBytes: delta,
                previousBytes: previousBytes,
                currentBytes: item.sizeBytes,
                command: item.attribution.command,
                processTree: item.attribution.processTree,
                aiAgentSession: item.attribution.aiAgentSession
            )
        }
        .sorted {
            if ($0.timestampMillis ?? 0) == ($1.timestampMillis ?? 0) {
                return $0.deltaBytes > $1.deltaBytes
            }
            return ($0.timestampMillis ?? 0) > ($1.timestampMillis ?? 0)
        }
        .prefix(10)
        .map { $0 }
    }

    private func storageGrowthEventTitle(_ event: StorageGrowthTimelineEvent) -> String {
        let repo = event.repoName ?? event.repoRoot.map(lastPathComponent) ?? "unattributed workspace"
        return "\(repo) added \(formatBytes(UInt64(event.deltaBytes))) to \(event.displayName)"
    }

    private func storageGrowthCorrelationDetail(_ event: StorageGrowthTimelineEvent) -> String {
        var parts: [String] = []
        if let command = event.command {
            parts.append("command \(command)")
        }
        if let processTree = event.processTree {
            parts.append("process tree \(processTree)")
        }
        if let session = event.aiAgentSession {
            parts.append("AI session \(session)")
        }
        if parts.isEmpty {
            parts.append("writer unknown: Aetower needs a file-event journal to tie this jump to a command/process/session")
        }
        return parts.joined(separator: " · ")
    }

    private func storageGrowthEventTime(_ event: StorageGrowthTimelineEvent) -> String {
        guard let timestampMillis = event.timestampMillis else {
            return "time unknown"
        }
        let date = Date(timeIntervalSince1970: Double(timestampMillis) / 1000.0)
        return date.formatted(date: .omitted, time: .shortened)
    }

    private func storageGrowthDelta(for footprint: StorageRepoFootprintModel) -> Int64? {
        if let backendGrowth = footprint.growthBytes {
            return backendGrowth
        }
        guard let previous = state.previousStorageHygieneReport?.repoFootprints.first(where: {
            $0.repoRoot == footprint.repoRoot
        }) else {
            return nil
        }
        return Int64(clamping: footprint.currentSizeBytes) - Int64(clamping: previous.currentSizeBytes)
    }

    private func storageGrowthLabel(for footprint: StorageRepoFootprintModel) -> String {
        guard let delta = storageGrowthDelta(for: footprint) else {
            return "baseline pending"
        }
        if delta == 0 {
            return "no growth since last scan"
        }
        let absolute = formatBytes(UInt64(abs(delta)))
        return delta > 0 ? "+\(absolute) since last scan" : "-\(absolute) since last scan"
    }

    private func storageGrowthCompactValue(for footprint: StorageRepoFootprintModel) -> String {
        guard let delta = storageGrowthDelta(for: footprint) else {
            return "Pending"
        }
        if delta == 0 {
            return "Flat"
        }
        let absolute = formatBytes(UInt64(abs(delta)))
        return delta > 0 ? "+\(absolute)" : "-\(absolute)"
    }

    private func storageGrowthWindow(for footprint: StorageRepoFootprintModel) -> String {
        storageGrowthDelta(for: footprint) == nil ? footprint.growthWindow : "since previous scan"
    }

    private func storageGrowthTone(for footprint: StorageRepoFootprintModel) -> Color {
        guard let delta = storageGrowthDelta(for: footprint) else {
            return .secondary
        }
        if delta > 0 {
            return AetowerDesign.Status.warning
        }
        if delta < 0 {
            return AetowerDesign.Status.ready
        }
        return .secondary
    }

    private func rebuildTimeLabel(_ seconds: UInt64?) -> String {
        guard let seconds else {
            return "manual review"
        }
        if seconds == 0 {
            return "no rebuild expected"
        }
        if seconds < 60 {
            return "~\(seconds)s"
        }
        return "~\(seconds / 60)m"
    }

    private func tone(for item: StorageHygieneItemModel) -> Color {
        item.safety == "safe" ? tone(forCleanupTier: item.cleanupTier) : AetowerDesign.Status.warning
    }

    private func tone(forCleanupTier tier: String) -> Color {
        switch tier {
        case "safe":
            return AetowerDesign.Status.ready
        case "rebuildable":
            return AetowerDesign.Tone.disk
        case "expensive":
            return AetowerDesign.Status.warning
        case "risky":
            return AetowerDesign.Status.error
        default:
            return .secondary
        }
    }

    private func cleanupTierLabel(_ tier: String) -> String {
        switch tier {
        case "safe":
            return "Safe"
        case "rebuildable":
            return "Rebuildable"
        case "expensive":
            return "Expensive"
        case "risky":
            return "Risky"
        default:
            return tier.capitalized
        }
    }

    private func cleanupTierIcon(_ tier: String) -> String {
        switch tier {
        case "safe":
            return "checkmark.shield"
        case "rebuildable":
            return "hammer"
        case "expensive":
            return "clock.badge.exclamationmark"
        case "risky":
            return "exclamationmark.triangle"
        default:
            return "folder"
        }
    }

    private func icon(for item: StorageHygieneItemModel) -> String {
        switch item.kind {
        case "log-file", "logs": return "doc.text"
        case "node-dependencies", "python-environment": return "shippingbox"
        case let kind where kind.contains("cache"): return "tray"
        default: return "folder"
        }
    }

    private func ageLabel(_ item: StorageHygieneItemModel) -> String {
        if let ageDays = item.ageDays {
            return ageDays == 0 ? "modified today" : "\(ageDays)d old"
        }
        if let modifiedMillis = item.modifiedMillis {
            let date = Date(timeIntervalSince1970: Double(modifiedMillis) / 1000.0)
            return date.formatted(date: .abbreviated, time: .omitted)
        }
        return "age unknown"
    }

    private func formatBytes(_ bytes: UInt64) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file)
    }

    private func lastPathComponent(_ path: String) -> String {
        URL(fileURLWithPath: path).lastPathComponent
    }

    private func reveal(path: String) {
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
    }

    private func copy(_ text: String) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
    }
}

private enum StorageFilter: String, CaseIterable, Identifiable {
    case attention
    case safe
    case rebuildable
    case expensive
    case risky
    case all

    var id: String { rawValue }

    var label: String {
        switch self {
        case .attention: return "Attention"
        case .safe: return "Safe"
        case .rebuildable: return "Rebuildable"
        case .expensive: return "Expensive"
        case .risky: return "Risky"
        case .all: return "All"
        }
    }

    func matches(_ item: StorageHygieneItemModel) -> Bool {
        switch self {
        case .attention:
            item.cleanupTier == "risky"
                || item.cleanupTier == "expensive"
                || item.safety != "safe"
                || item.sizeTruncated
                || item.sizeBytes >= 100 * 1024 * 1024
        case .safe:
            item.cleanupTier == "safe"
        case .rebuildable:
            item.cleanupTier == "rebuildable"
        case .expensive:
            item.cleanupTier == "expensive"
        case .risky:
            item.cleanupTier == "risky"
        case .all:
            true
        }
    }
}
