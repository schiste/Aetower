import AppKit
@preconcurrency import ApplicationServices
import Foundation

public struct FrontmostAppObservation: Sendable {
    public let processIdentifier: pid_t
    public let appName: String
    public let bundleId: String?
    public let executablePath: String?
    public let windowTitle: String?
}

public struct PermissionResult {
    public let state: CapabilityState
    public let detail: String
}

public final class PermissionCoordinator {
    private var accessibilityTrustCache: (checkedAt: Date, trusted: Bool)?
    private var cachedWindowTitle: (checkedAt: Date, processIdentifier: pid_t, title: String?)?
    private let accessibilityTrustCacheInterval: TimeInterval = 60
    private let windowTitleCacheInterval: TimeInterval = 10

    public init() {}

    public func currentStatus(_ capability: CapabilityKind) -> PermissionResult {
        switch capability {
        case .accessibility:
            let trusted = isAccessibilityTrusted()
            return PermissionResult(
                state: trusted ? .granted : .denied,
                detail: trusted
                    ? "Accessibility access is currently granted."
                    : "Accessibility access is not granted. Use Request to open the macOS prompt."
            )
        case .fullDiskAccess:
            return currentFullDiskAccessStatus()
        case .appleAutomation:
            return PermissionResult(
                state: .unavailable,
                detail: "Automation is not probed automatically to avoid triggering Apple Events prompts. Use Request to run an explicit probe."
            )
        default:
            return PermissionResult(
                state: .requested,
                detail: "Capability status is managed by the runtime adapter."
            )
        }
    }

    public func request(_ capability: CapabilityKind) -> PermissionResult {
        switch capability {
        case .accessibility:
            return requestAccessibility()
        case .fullDiskAccess:
            openPrivacyPane(anchor: "Privacy_AllFiles")
            return PermissionResult(
                state: .requested,
                detail: "Opened System Settings for Full Disk Access."
            )
        case .appleAutomation:
            return requestAutomationProbe()
        case .chromiumDebug:
            return PermissionResult(
                state: .requested,
                detail: "Enable a Chromium remote debugging endpoint and restart the app."
            )
        case .dockerSocket:
            return PermissionResult(
                state: FileManager.default.fileExists(atPath: "/var/run/docker.sock") ? .granted : .unavailable,
                detail: FileManager.default.fileExists(atPath: "/var/run/docker.sock")
                    ? "Docker socket detected."
                    : "Docker socket not found."
            )
        case .privilegedHelper:
            return PermissionResult(
                state: .requested,
                detail: "Configure a helper path and run the helper with elevated privileges when deeper attribution is required."
            )
        case .endpointSecurity:
            return PermissionResult(
                state: .requested,
                detail: "Endpoint Security requires the enterprise helper to be signed with the Endpoint Security entitlement and approved for privileged event streaming."
            )
        case .chau7:
            let defaultPath = FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent(".chau7/mcp.sock").path
            let exists = FileManager.default.fileExists(atPath: defaultPath)
            return PermissionResult(
                state: exists ? .granted : .unavailable,
                detail: exists
                    ? "Chau7 MCP socket detected at \(defaultPath)."
                    : "Chau7 MCP socket not found. Ensure Chau7 is running."
            )
        }
    }

    public func currentFrontmostAppObservation(includeWindowTitle: Bool = true) -> FrontmostAppObservation? {
        guard let app = NSWorkspace.shared.frontmostApplication else {
            return nil
        }

        return FrontmostAppObservation(
            processIdentifier: app.processIdentifier,
            appName: app.localizedName ?? app.bundleIdentifier ?? "Unknown App",
            bundleId: app.bundleIdentifier,
            executablePath: app.executableURL?.path,
            windowTitle: includeWindowTitle
                ? currentFocusedWindowTitle(
                    processIdentifier: app.processIdentifier,
                    bundleId: app.bundleIdentifier
                )
                : nil
        )
    }

    public func canReadFocusedWindowTitle(bundleId: String?) -> Bool {
        canReadFocusedWindowTitle(bundleId: bundleId, now: Date())
    }

    public func currentFocusedWindowTitle(processIdentifier: pid_t, bundleId: String?) -> String? {
        currentFocusedWindowTitle(
            processIdentifier: processIdentifier,
            bundleId: bundleId,
            now: Date()
        )
    }

    private func requestAccessibility() -> PermissionResult {
        let promptKey = kAXTrustedCheckOptionPrompt.takeRetainedValue() as String
        let options = [promptKey: true] as CFDictionary
        let trusted = AXIsProcessTrustedWithOptions(options)
        accessibilityTrustCache = (Date(), trusted)
        return PermissionResult(
            state: trusted ? .granted : .requested,
            detail: trusted
                ? "Accessibility access granted."
                : "macOS has been asked to prompt for Accessibility access."
        )
    }

    private func requestAutomationProbe() -> PermissionResult {
        let scriptSource = """
        tell application "Spotify"
            if it is running then
                return name
            else
                return "Spotify not running"
            end if
        end tell
        """

        guard let script = NSAppleScript(source: scriptSource) else {
            return PermissionResult(state: .denied, detail: "Failed to build AppleScript automation probe.")
        }

        var error: NSDictionary?
        let result = script.executeAndReturnError(&error)
        if let error {
            return PermissionResult(
                state: .requested,
                detail: "Automation prompt attempted. Result: \(error.description)"
            )
        }

        let output = result.stringValue ?? "Automation probe executed."
        return PermissionResult(state: .granted, detail: output)
    }

    private func openPrivacyPane(anchor: String) {
        guard let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?\(anchor)") else {
            return
        }
        NSWorkspace.shared.open(url)
    }

    private func currentFullDiskAccessStatus() -> PermissionResult {
        let home = FileManager.default.homeDirectoryForCurrentUser
        let protectedPaths = [
            home.appendingPathComponent("Library/Mail").path,
            home.appendingPathComponent("Library/Safari").path,
            home.appendingPathComponent("Library/Messages").path,
        ]
        let existingProtectedPaths = protectedPaths.filter {
            FileManager.default.fileExists(atPath: $0)
        }
        guard !existingProtectedPaths.isEmpty else {
            return PermissionResult(
                state: .requested,
                detail: "No standard protected user-data path was available for a non-prompting Full Disk Access probe."
            )
        }
        let readable = existingProtectedPaths.contains {
            (try? FileManager.default.contentsOfDirectory(atPath: $0)) != nil
        }
        return PermissionResult(
            state: readable ? .granted : .denied,
            detail: readable
                ? "A protected user-data path is readable; Full Disk Access appears available."
                : "Protected user-data paths are not readable. Use Request to open Full Disk Access settings."
        )
    }

    private func currentFocusedWindowTitle(
        processIdentifier: pid_t,
        bundleId: String?,
        now: Date
    ) -> String? {
        if let cachedWindowTitle,
           cachedWindowTitle.processIdentifier == processIdentifier,
           now.timeIntervalSince(cachedWindowTitle.checkedAt) < windowTitleCacheInterval
        {
            return cachedWindowTitle.title
        }

        guard canReadFocusedWindowTitle(bundleId: bundleId, now: now) else {
            return nil
        }

        let stringTitle = Self.probeFocusedWindowTitle(processIdentifier: processIdentifier)
        cachedWindowTitle = (now, processIdentifier, stringTitle)
        return stringTitle
    }

    /// Cache-free raw AX probe, safe to call from any thread. The AX
    /// messaging timeout bounds the worst case so an unresponsive target app
    /// cannot stall the caller indefinitely (AX reads are synchronous IPC).
    public static func probeFocusedWindowTitle(processIdentifier: pid_t) -> String? {
        let applicationElement = AXUIElementCreateApplication(processIdentifier)
        AXUIElementSetMessagingTimeout(applicationElement, 1.0)
        var focusedWindow: CFTypeRef?
        let focusedWindowResult = AXUIElementCopyAttributeValue(
            applicationElement,
            kAXFocusedWindowAttribute as CFString,
            &focusedWindow
        )
        guard focusedWindowResult == .success, let focusedWindow else {
            return nil
        }

        guard CFGetTypeID(focusedWindow) == AXUIElementGetTypeID() else {
            return nil
        }
        let windowElement = unsafeDowncast(focusedWindow, to: AXUIElement.self)
        var title: CFTypeRef?
        let titleResult = AXUIElementCopyAttributeValue(
            windowElement,
            kAXTitleAttribute as CFString,
            &title
        )
        guard titleResult == .success else {
            return nil
        }
        return title as? String
    }

    private func canReadFocusedWindowTitle(bundleId: String?, now: Date) -> Bool {
        if bundleId == Bundle.main.bundleIdentifier {
            return false
        }
        return isAccessibilityTrusted(now: now)
    }

    private func isAccessibilityTrusted(now: Date = Date()) -> Bool {
        if let accessibilityTrustCache,
           now.timeIntervalSince(accessibilityTrustCache.checkedAt) < accessibilityTrustCacheInterval
        {
            return accessibilityTrustCache.trusted
        }
        let trusted = AXIsProcessTrusted()
        accessibilityTrustCache = (now, trusted)
        return trusted
    }
}
