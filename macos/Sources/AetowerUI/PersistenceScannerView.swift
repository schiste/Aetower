import SwiftUI

/// KnockKnock-style "Startup & persistence" scanner tab: enumerates launchd
/// agents/daemons, login items, and cron, each with code-signing status, so
/// unsigned/ad-hoc outliers stand out. On-demand (the scan code-signs every
/// entry), with a default "hide Apple-signed" filter to surface the
/// security-relevant third-party surface.
public struct PersistenceScannerView: View {
    let state: AppState
    let settings: SettingsStore
    @State private var hideAppleSigned = true

    public init(state: AppState, settings: SettingsStore) {
        self.state = state
        self.settings = settings
    }

    private var report: PersistenceScanReportModel? { state.persistenceScanReport }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
                header
                if let report {
                    controls(report)
                    ForEach(groupedKinds(report), id: \.self) { kind in
                        section(kind: kind, items: visibleItems(report, kind: kind))
                    }
                    if !report.degraded.isEmpty {
                        degradedSection(report.degraded)
                    }
                } else if state.persistenceScanIsLoading {
                    ProgressView("Scanning startup & persistence…")
                        .padding(.top, AetowerDesign.Spacing.xl)
                } else {
                    emptyState
                }
            }
            .frame(maxWidth: 920, alignment: .leading)
            .padding(AetowerDesign.Spacing.xxl)
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
            Text("Startup & persistence")
                .font(.system(size: 28, weight: .semibold, design: .rounded))
            Text(
                "Launch agents and daemons, login items, and cron — each with code-signing status. Unsigned or ad-hoc items are the ones worth a second look."
            )
            .foregroundStyle(.secondary)
        }
    }

    private var emptyState: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            ContentUnavailableView(
                "No scan yet",
                systemImage: "lock.shield",
                description: Text(
                    "Run a scan to enumerate this Mac's startup and persistence surface.")
            )
            Button {
                state.runPersistenceScan()
            } label: {
                Label("Scan now", systemImage: "magnifyingglass")
            }
            .buttonStyle(.borderedProminent)
            if let error = state.persistenceScanError {
                Text(error)
                    .font(.caption)
                    .foregroundStyle(AetowerDesign.Status.warning)
            }
        }
    }

    private func controls(_ report: PersistenceScanReportModel) -> some View {
        HStack(spacing: AetowerDesign.Spacing.md) {
            Button {
                state.runPersistenceScan()
            } label: {
                Label("Rescan", systemImage: "arrow.clockwise")
            }
            .buttonStyle(.bordered)
            .disabled(state.persistenceScanIsLoading)

            Toggle("Hide Apple-signed", isOn: $hideAppleSigned)
                .toggleStyle(.switch)

            Spacer()

            Text("\(report.items.count) items")
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
            if let completed = state.persistenceScanCompletedAt {
                Text("scanned \(completed.formatted(date: .omitted, time: .shortened))")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
        }
    }

    /// Kinds present in the report, in a stable security-relevant order.
    private func groupedKinds(_ report: PersistenceScanReportModel) -> [String] {
        let order = [
            "user-launch-agent", "launch-agent", "launch-daemon", "login-item", "cron",
            "system-launch-agent", "system-launch-daemon",
        ]
        let present = Set(report.items.map(\.kind))
        return order.filter { present.contains($0) && !visibleItems(report, kind: $0).isEmpty }
    }

    private func visibleItems(_ report: PersistenceScanReportModel, kind: String)
        -> [PersistenceItemModel]
    {
        report.items.filter { $0.kind == kind && !(hideAppleSigned && $0.isApple) }
    }

    private func section(kind: String, items: [PersistenceItemModel]) -> some View {
        let label = items.first?.kindLabel ?? kind
        return GroupBox(label: Text("\(label) · \(items.count)")) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                ForEach(items) { item in
                    itemRow(item)
                }
            }
            .padding(.top, AetowerDesign.Spacing.xs)
        }
    }

    private func itemRow(_ item: PersistenceItemModel) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 8) {
                Text(item.label ?? (item.program ?? item.path))
                    .font(.system(size: 13, weight: .medium))
                    .lineLimit(1)
                Spacer()
                signingChip(item.signature)
            }
            if let program = item.program {
                Text(program)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
            }
            Text(item.path)
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .truncationMode(.middle)
            ForEach(item.notes, id: \.self) { note in
                Text(note)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(AetowerDesign.Spacing.sm)
        .background(
            AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm))
    }

    @ViewBuilder
    private func signingChip(_ signature: ProcessSignatureInfoModel?) -> some View {
        let label = signature?.classificationLabel ?? "No signature"
        let tone = signingTone(signature)
        Text(label)
            .font(.system(size: 10, weight: .bold))
            .foregroundStyle(tone)
            .padding(.horizontal, AetowerDesign.Spacing.xs)
            .padding(.vertical, 2)
            .background(tone.opacity(0.15), in: Capsule())
    }

    private func signingTone(_ signature: ProcessSignatureInfoModel?) -> Color {
        switch signature?.classification {
        case "unsigned": return AetowerDesign.Status.error
        case "adhoc", "other": return AetowerDesign.Status.warning
        case "developer_id": return AetowerDesign.Status.ready
        case "apple", "mac_app_store": return AetowerDesign.Status.neutral
        default: return AetowerDesign.Status.warning
        }
    }

    private func degradedSection(_ degraded: [DegradedSourceModel]) -> some View {
        GroupBox(label: Text("Requires elevated access")) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                ForEach(degraded) { source in
                    HStack(alignment: .top, spacing: 8) {
                        Image(systemName: "lock.slash")
                            .foregroundStyle(.secondary)
                        Text(source.reason)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            .padding(.top, AetowerDesign.Spacing.xs)
        }
    }
}
