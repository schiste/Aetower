import Foundation
import SwiftUI
import AetowerBridge

enum ProcessOriginKind: String, CaseIterable, Hashable, Identifiable {
    case app
    case cli
    case helper
    case system
    case service
    case mixed
    case unknown

    var id: String { rawValue }

    var chipLabel: String {
        switch self {
        case .app: return "App"
        case .cli: return "CLI"
        case .helper: return "Helper"
        case .system: return "System"
        case .service: return "Service"
        case .mixed: return "Mixed"
        case .unknown: return "Unknown"
        }
    }

    var label: String {
        switch self {
        case .app: return "App process"
        case .cli: return "CLI process"
        case .helper: return "App helper"
        case .system: return "System process"
        case .service: return "Launch service"
        case .mixed: return "Mixed origins"
        case .unknown: return "Unknown origin"
        }
    }

    var systemImage: String {
        switch self {
        case .app: return "app.fill"
        case .cli: return "terminal.fill"
        case .helper: return "puzzlepiece.extension.fill"
        case .system: return "gearshape.2.fill"
        case .service: return "server.rack"
        case .mixed: return "square.stack.3d.up.fill"
        case .unknown: return "questionmark.circle.fill"
        }
    }

    var tint: Color {
        switch self {
        case .app: return .blue
        case .cli: return .green
        case .helper: return .indigo
        case .system: return .gray
        case .service: return .teal
        case .mixed: return .orange
        case .unknown: return .secondary
        }
    }
}

enum ProcessOriginFilter: String, CaseIterable, Hashable, Identifiable {
    case all
    case apps
    case cli
    case helpers
    case system
    case services
    case unknown

    var id: String { rawValue }

    var label: String {
        switch self {
        case .all: return "All origins"
        case .apps: return "Apps"
        case .cli: return "CLI"
        case .helpers: return "Helpers"
        case .system: return "System"
        case .services: return "Services"
        case .unknown: return "Unknown"
        }
    }

    var menuLabel: String {
        switch self {
        case .all: return "Origin: All"
        default: return "Origin: \(label)"
        }
    }

    func matches(_ kind: ProcessOriginKind) -> Bool {
        switch self {
        case .all: return true
        case .apps: return kind == .app
        case .cli: return kind == .cli
        case .helpers: return kind == .helper
        case .system: return kind == .system
        case .services: return kind == .service
        case .unknown: return kind == .unknown
        }
    }
}

struct ProcessOriginSummary: Hashable {
    let kind: ProcessOriginKind
    let label: String
    let subtitle: String
    let detailLines: [String]
    let searchTokens: [String]

    var chipLabel: String { kind.chipLabel }

    func matches(_ filter: ProcessOriginFilter) -> Bool {
        filter.matches(kind)
    }
}

struct ProcessOriginSignals {
    var entityKind: EntityKind
    var displayName: String
    var bundleId: String?
    var executablePath: String?
    var componentExecutablePaths: [String]
    var commandLines: [String]
    var parentSummaries: [String]
    var launchedByValues: [String]
    var provenanceLabels: [String]
    var provenanceKindNames: [String]
    var adapterKindNames: [String]
    var users: [String]
}

func processOrigin(for entity: EntitySnapshot) -> ProcessOriginSummary {
    processOrigin(from: processOriginSignals(for: entity))
}

func processOrigin(for entities: [EntitySnapshot]) -> ProcessOriginSummary {
    let summaries = entities.map(processOrigin(for:))
    guard let first = summaries.first else {
        return ProcessOriginSummary(
            kind: .unknown,
            label: ProcessOriginKind.unknown.label,
            subtitle: "No process metadata is available.",
            detailLines: ["Origin: Unknown"],
            searchTokens: ["origin:unknown", "unknown"]
        )
    }
    let uniqueKinds = Set(summaries.map(\.kind))
    guard uniqueKinds.count > 1 else { return first }

    let counts = Dictionary(summaries.map { ($0.kind, 1) }, uniquingKeysWith: +)
    let ordered = counts.sorted { lhs, rhs in
        if lhs.value != rhs.value { return lhs.value > rhs.value }
        return lhs.key.chipLabel < rhs.key.chipLabel
    }
    let subtitle = ordered
        .map { "\($0.key.chipLabel) \($0.value)" }
        .joined(separator: " · ")
    return ProcessOriginSummary(
        kind: .mixed,
        label: ProcessOriginKind.mixed.label,
        subtitle: subtitle,
        detailLines: ["Origin mix: \(subtitle)"],
        searchTokens: ["origin:mixed", "mixed"] + summaries.flatMap(\.searchTokens)
    )
}

func processOrigin(from signals: ProcessOriginSignals) -> ProcessOriginSummary {
    let allPaths = ([signals.executablePath].compactMap { $0 } + signals.componentExecutablePaths)
        .filter { !$0.isEmpty }
    let allText = (
        [signals.displayName, signals.bundleId, signals.executablePath].compactMap { $0 }
            + signals.componentExecutablePaths
            + signals.commandLines
            + signals.parentSummaries
            + signals.launchedByValues
            + signals.provenanceLabels
            + signals.provenanceKindNames
            + signals.adapterKindNames
    )
    let loweredText = allText.map { $0.localizedLowercase }
    let parentSummary = firstNonEmpty(signals.parentSummaries)
    let executable = signals.executablePath ?? firstNonEmpty(signals.componentExecutablePaths)
    let launcher = firstNonEmpty(signals.launchedByValues)
    let host = originHostSummary(from: signals)

    if isAppHelper(signals: signals, paths: allPaths, loweredText: loweredText) {
        return originSummary(
            kind: .helper,
            subtitle: helperSubtitle(executable: executable, parent: parentSummary),
            parent: parentSummary,
            executable: executable,
            launcher: launcher,
            host: host,
            extraTokens: ["helper"]
        )
    }

    if isCliProcess(signals: signals, paths: allPaths, loweredText: loweredText) {
        return originSummary(
            kind: .cli,
            subtitle: cliSubtitle(parent: parentSummary, host: host),
            parent: parentSummary,
            executable: executable,
            launcher: launcher,
            host: host,
            extraTokens: ["cli", "terminal", "shell"]
        )
    }

    if isAppProcess(signals: signals, paths: allPaths, loweredText: loweredText) {
        return originSummary(
            kind: .app,
            subtitle: appSubtitle(bundleId: signals.bundleId, executable: executable),
            parent: parentSummary,
            executable: executable,
            launcher: launcher,
            host: host,
            extraTokens: ["app", "application", signals.bundleId].compactMap { $0 }
        )
    }

    if isSystemProcess(signals: signals, paths: allPaths, loweredText: loweredText) {
        return originSummary(
            kind: .system,
            subtitle: systemSubtitle(executable: executable, parent: parentSummary),
            parent: parentSummary,
            executable: executable,
            launcher: launcher,
            host: host,
            extraTokens: ["system"]
        )
    }

    if isServiceProcess(signals: signals, loweredText: loweredText) {
        return originSummary(
            kind: .service,
            subtitle: serviceSubtitle(parent: parentSummary),
            parent: parentSummary,
            executable: executable,
            launcher: launcher,
            host: host,
            extraTokens: ["service", "daemon", "launchd"]
        )
    }

    return originSummary(
        kind: .unknown,
        subtitle: "Aetower does not have enough launch metadata yet.",
        parent: parentSummary,
        executable: executable,
        launcher: launcher,
        host: host,
        extraTokens: ["unknown"]
    )
}

private func processOriginSignals(for entity: EntitySnapshot) -> ProcessOriginSignals {
    let components = entity.components.filter { $0.kind != .adapterContext }
    let adapterContexts = entity.components.compactMap(\.adapterContext)
    let provenances = ([entity.primaryProvenance].compactMap { $0 } + entity.components.compactMap(\.provenance))
    return ProcessOriginSignals(
        entityKind: entity.entityKind,
        displayName: entity.displayName,
        bundleId: entity.bundleId,
        executablePath: entity.executablePath,
        componentExecutablePaths: components.compactMap(\.executablePath),
        commandLines: components.compactMap(\.commandLine),
        parentSummaries: components.compactMap(\.parentSummary),
        launchedByValues: components.compactMap(\.launchedBy),
        provenanceLabels: provenances.map(\.label),
        provenanceKindNames: provenances.map { String(describing: $0.kind) },
        adapterKindNames: adapterContexts.map { String(describing: $0.kind) },
        users: components.compactMap(\.user)
    )
}

private func isCliProcess(
    signals: ProcessOriginSignals,
    paths: [String],
    loweredText: [String]
) -> Bool {
    if signals.entityKind == .terminalSession || signals.entityKind == .aiAgent {
        if !paths.contains(where: isAppBundleExecutablePath) {
            return true
        }
    }
    if loweredText.contains(where: containsShellProvenance) {
        return !paths.contains(where: isAppBundleMainExecutablePath)
    }
    return paths.contains(where: isCommandLineToolPath)
}

private func isAppProcess(
    signals: ProcessOriginSignals,
    paths: [String],
    loweredText: [String]
) -> Bool {
    if signals.entityKind == .app || signals.entityKind == .browser {
        return true
    }
    if signals.bundleId?.isEmpty == false {
        return true
    }
    if paths.contains(where: isAppBundleMainExecutablePath) {
        return true
    }
    return loweredText.contains { $0.contains("appbundle") || $0.contains("app bundle") }
}

private func isAppHelper(
    signals: ProcessOriginSignals,
    paths: [String],
    loweredText: [String]
) -> Bool {
    if signals.entityKind == .app || signals.entityKind == .browser {
        return false
    }
    if paths.contains(where: isAppBundleHelperPath) {
        return true
    }
    return loweredText.contains {
        $0.contains("helpertree")
            || $0.contains("helper tree")
            || $0.contains(" helper")
    }
}

private func isSystemProcess(
    signals: ProcessOriginSignals,
    paths: [String],
    loweredText: [String]
) -> Bool {
    if paths.contains(where: { path in
        path.hasPrefix("/System/")
            || path.hasPrefix("/usr/libexec/")
            || path.hasPrefix("/usr/sbin/")
            || path.hasPrefix("/sbin/")
    }) {
        return true
    }
    return loweredText.contains { $0.contains("system") && $0.contains("launchd") }
        || signals.users.contains("root")
}

private func isServiceProcess(signals: ProcessOriginSignals, loweredText: [String]) -> Bool {
    if signals.entityKind == .daemon || signals.entityKind == .service {
        return true
    }
    return loweredText.contains {
        $0.contains("servicemanager")
            || $0.contains("service manager")
            || $0.contains("launchd")
            || $0.contains("loginitem")
            || $0.contains("login item")
    }
}

private func originSummary(
    kind: ProcessOriginKind,
    subtitle: String,
    parent: String?,
    executable: String?,
    launcher: String?,
    host: String?,
    extraTokens: [String]
) -> ProcessOriginSummary {
    var detailLines = ["Origin: \(kind.label)"]
    if let host { detailLines.append("Host: \(host)") }
    if let parent { detailLines.append("Parent: \(parent)") }
    if let launcher { detailLines.append("Launched by: \(launcher)") }
    if let executable { detailLines.append("Executable: \(executable)") }

    let baseTokens = [
        "origin:\(kind.rawValue)",
        kind.rawValue,
        kind.chipLabel.localizedLowercase,
        subtitle.localizedLowercase,
        parent?.localizedLowercase,
        launcher?.localizedLowercase,
        executable?.localizedLowercase,
        host?.localizedLowercase,
    ].compactMap { $0 }

    return ProcessOriginSummary(
        kind: kind,
        label: kind.label,
        subtitle: subtitle,
        detailLines: detailLines,
        searchTokens: baseTokens + extraTokens.map(\.localizedLowercase)
    )
}

private func cliSubtitle(parent: String?, host: String?) -> String {
    let parentLabel = parent.flatMap(shellName(from:))
    if let host, let parentLabel {
        return "CLI via \(parentLabel) in \(host)"
    }
    if let host {
        return "CLI hosted by \(host)"
    }
    if let parentLabel {
        return "CLI launched from \(parentLabel)"
    }
    return "Command-line process"
}

private func appSubtitle(bundleId: String?, executable: String?) -> String {
    if let bundleId, !bundleId.isEmpty {
        return "GUI app · \(bundleId)"
    }
    if let bundleName = appBundleName(from: executable) {
        return "GUI app · \(bundleName)"
    }
    return "GUI application process"
}

private func helperSubtitle(executable: String?, parent: String?) -> String {
    if let bundleName = appBundleName(from: executable) {
        return "Helper inside \(bundleName)"
    }
    if let parent {
        return "Helper launched by \(parent)"
    }
    return "Bundled helper process"
}

private func systemSubtitle(executable: String?, parent: String?) -> String {
    if let executable {
        return "System-owned binary · \((executable as NSString).lastPathComponent)"
    }
    if let parent {
        return "System process · parent \(parent)"
    }
    return "System-owned process"
}

private func serviceSubtitle(parent: String?) -> String {
    if let parent {
        return "Launch-managed service · parent \(parent)"
    }
    return "Launch-managed service"
}

private func originHostSummary(from signals: ProcessOriginSignals) -> String? {
    if signals.adapterKindNames.contains(where: { $0.localizedLowercase.contains("chau7") }) {
        return "Chau7 session"
    }
    if signals.adapterKindNames.contains(where: { $0.localizedLowercase.contains("vscode") }) {
        return "VS Code workspace"
    }
    return nil
}

private func isAppBundleExecutablePath(_ path: String) -> Bool {
    path.contains(".app/Contents/")
}

private func isAppBundleMainExecutablePath(_ path: String) -> Bool {
    path.contains(".app/Contents/MacOS/")
}

private func isAppBundleHelperPath(_ path: String) -> Bool {
    let lowered = path.localizedLowercase
    return lowered.contains(".app/contents/helpers/")
        || lowered.contains(".app/contents/frameworks/")
        || lowered.contains(".app/contents/xpcservices/")
        || lowered.contains(".app/contents/plug-ins/")
}

private func isCommandLineToolPath(_ path: String) -> Bool {
    let lowered = path.localizedLowercase
    guard !isAppBundleExecutablePath(path) else { return false }
    return lowered.hasPrefix("/opt/homebrew/")
        || lowered.hasPrefix("/usr/local/")
        || lowered.contains("/.cargo/bin/")
        || lowered.contains("/.volta/tools/")
        || lowered.contains("/.npm/")
        || lowered.contains("/node_modules/.bin/")
}

private func containsShellProvenance(_ value: String) -> Bool {
    shellName(from: value) != nil
        || value.contains("shellsession")
        || value.contains("shell session")
}

private func shellName(from value: String) -> String? {
    let lowered = value.localizedLowercase
    for shell in ["zsh", "bash", "fish", "tcsh", "csh", "sh"] where lowered.contains(shell) {
        return shell
    }
    return nil
}

private func appBundleName(from path: String?) -> String? {
    guard let path, let range = path.range(of: ".app/") else { return nil }
    let prefix = String(path[..<range.lowerBound])
    let bundleName = (prefix as NSString).lastPathComponent
    return bundleName.isEmpty ? nil : "\(bundleName).app"
}

private func firstNonEmpty(_ values: [String]) -> String? {
    values.first { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
}
