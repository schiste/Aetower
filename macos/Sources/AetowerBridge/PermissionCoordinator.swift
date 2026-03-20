import AppKit
import ApplicationServices
import Foundation

public struct FrontmostAppObservation {
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
    public init() {}

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
        }
    }

    public func currentFrontmostAppObservation() -> FrontmostAppObservation? {
        guard let app = NSWorkspace.shared.frontmostApplication else {
            return nil
        }

        return FrontmostAppObservation(
            appName: app.localizedName ?? app.bundleIdentifier ?? "Unknown App",
            bundleId: app.bundleIdentifier,
            executablePath: app.executableURL?.path,
            windowTitle: currentFocusedWindowTitle(for: app)
        )
    }

    private func requestAccessibility() -> PermissionResult {
        let promptKey = kAXTrustedCheckOptionPrompt.takeRetainedValue() as String
        let options = [promptKey: true] as CFDictionary
        let trusted = AXIsProcessTrustedWithOptions(options)
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

    private func currentFocusedWindowTitle(for app: NSRunningApplication) -> String? {
        guard AXIsProcessTrusted() else {
            return nil
        }

        let applicationElement = AXUIElementCreateApplication(app.processIdentifier)
        var focusedWindow: CFTypeRef?
        let focusedWindowResult = AXUIElementCopyAttributeValue(
            applicationElement,
            kAXFocusedWindowAttribute as CFString,
            &focusedWindow
        )
        guard focusedWindowResult == .success, let focusedWindow else {
            return nil
        }

        let windowElement = focusedWindow as! AXUIElement
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
}
