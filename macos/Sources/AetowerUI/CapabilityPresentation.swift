import Foundation
import AetowerBridge

func capabilityKindDisplayName(_ kind: CapabilityKind) -> String {
    switch kind {
    case .accessibility:
        return "Accessibility"
    case .fullDiskAccess:
        return "Full Disk Access"
    case .appleAutomation:
        return "Apple Automation"
    case .chromiumDebug:
        return "Chromium Debug"
    case .dockerSocket:
        return "Docker Socket"
    case .privilegedHelper:
        return "Privileged Helper"
    case .chau7:
        return "Chau7"
    case .endpointSecurity:
        return "Endpoint Security"
    }
}
