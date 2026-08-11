import SwiftUI

public enum AetowerDesign {

    // MARK: - Spacing Scale (4px base)

    public enum Spacing {
        public static let none: CGFloat = 0
        public static let xxs: CGFloat = 2
        public static let xs: CGFloat = 4
        public static let sm: CGFloat = 8
        public static let storageLegendLabel: CGFloat = 5
        public static let storageLegendItem: CGFloat = 6
        public static let storageSegmentGap: CGFloat = 1.5
        public static let md: CGFloat = 12
        public static let lg: CGFloat = 16
        public static let xl: CGFloat = 20
        public static let xxl: CGFloat = 24
        public static let section: CGFloat = 28
    }

    // MARK: - Corner Radius

    public enum Radius {
        public static let sm: CGFloat = 8
        public static let md: CGFloat = 12
        public static let lg: CGFloat = 16
        public static let pill: CGFloat = 100
    }

    // MARK: - Stroke

    public enum Stroke {
        public static let hairline: CGFloat = 1
        public static let strong: CGFloat = 1.5
    }

    // MARK: - Sizing

    public enum Size {
        public static let controlHeight: CGFloat = 28
        public static let minTouchTarget: CGFloat = 32
        public static let iconSlot: CGFloat = 24
        public static let sidebarWidth: CGFloat = 188
        public static let actionTileMinWidth: CGFloat = 116
        public static let storageBarHeight: CGFloat = 14
        public static let storageLegendMinWidth: CGFloat = 148
        public static let storageOwnerMark: CGFloat = 7
        public static let storageReclaimSeamHeight: CGFloat = 3
        public static let storageReclaimMinimumWidth: CGFloat = 2
        public static let storageReclaimMarkCompactWidth: CGFloat = 9
        public static let storageReclaimMarkRegularWidth: CGFloat = 12
        public static let storageRepositoryStrataHeight: CGFloat = 5
        public static let storageRepositoryStrataMark: CGFloat = 6
        public static let storageRepositoryStrataGap: CGFloat = 1
    }

    // MARK: - Typography

    public enum Typography {
        public static let sectionTitle: Font = .headline
        public static let controlLabel: Font = .subheadline.weight(.medium)
        public static let body: Font = .callout
        public static let caption: Font = .caption
        public static let metadata: Font = .caption2
        public static let metadataStrong: Font = .caption2.weight(.semibold)
        public static let data: Font = .caption.monospacedDigit()
        public static let dataSmall: Font = .caption2.monospacedDigit()

        public static func metricValue(size: CGFloat = 28, weight: Font.Weight = .bold) -> Font {
            .system(size: size, weight: weight, design: .rounded)
        }

        public static func compactData(size: CGFloat = 10, weight: Font.Weight = .medium) -> Font {
            .system(size: size, weight: weight, design: .monospaced)
        }
    }

    // MARK: - Animation

    public enum Motion {
        public static let quick: Animation = .easeInOut(duration: 0.15)
        public static let standard: Animation = .easeInOut(duration: 0.25)
        public static let smooth: Animation = .spring(response: 0.35, dampingFraction: 0.85)
        public static let slow: Animation = .easeInOut(duration: 0.5)
    }

    // MARK: - Friction Colors

    public static func frictionColor(_ score: Float) -> Color {
        switch score {
        case 75...: return .red
        case 40...: return .orange
        case 15...: return .yellow
        default: return .green
        }
    }

    public static func frictionLabel(_ score: Float) -> String {
        switch score {
        case 75...: return "Critical"
        case 40...: return "High"
        case 15...: return "Watch"
        default: return "Stable"
        }
    }

    // MARK: - Metric Tone Colors

    public enum Tone {
        public static let friction: Color = .orange
        public static let cpu: Color = .blue
        public static let memory: Color = .green
        public static let disk: Color = .pink
        public static let network: Color = .teal
        public static let energy: Color = .yellow
        public static let wakeups: Color = .purple
        public static let gpu: Color = .indigo
    }

    /// Muted, ownership-first colors for the Storage volume map. Reclaimable
    /// bytes deliberately use a separate red seam rather than changing owner.
    public enum StorageOwnership {
        public static let system = Color(red: 0.53, green: 0.57, blue: 0.64)
        public static let repositories = Color(red: 0.40, green: 0.46, blue: 0.66)
        public static let applications = Color(red: 0.64, green: 0.56, blue: 0.45)
        public static let developer = Color(red: 0.33, green: 0.55, blue: 0.54)
        public static let personal = Color(red: 0.38, green: 0.56, blue: 0.70)
        public static let other = Color(red: 0.43, green: 0.45, blue: 0.50)
        public static let reclaimable = Color(red: 0.85, green: 0.36, blue: 0.36)

        public enum RepositoryStrata {
            public static let source = Color(red: 0.40, green: 0.46, blue: 0.66)
            public static let dependencies = Color(red: 0.31, green: 0.39, blue: 0.61)
            public static let builds = Color(red: 0.50, green: 0.42, blue: 0.68)
            public static let git = Color(red: 0.32, green: 0.53, blue: 0.69)
            public static let media = Color(red: 0.52, green: 0.57, blue: 0.74)
            public static let workspace = Color(red: 0.60, green: 0.62, blue: 0.68)
        }
    }

    public enum Status {
        public static let success: Color = Tone.memory
        public static let ready: Color = Tone.cpu
        public static let warning: Color = Tone.friction
        public static let error: Color = .red
        public static let neutral: Color = .secondary
    }

    // MARK: - Text Colors

    public enum Ink {
        public static let primary: Color = .primary
        public static let secondary: Color = .secondary
        public static let tertiary: Color = Color.secondary.opacity(0.68)
        public static let inverse: Color = .white
    }

    // MARK: - Surface Colors

    public enum Surface {
        public static let rowIdle = Color.secondary.opacity(0.04)
        public static let rowHover = Color.secondary.opacity(0.08)
        public static let rowSelected = Color.accentColor.opacity(0.10)
        public static let card = Color.clear
        public static let control = Color.secondary.opacity(0.05)
        public static let cardHover = Color.secondary.opacity(0.09)
        public static let badge = Color.secondary.opacity(0.08)
        public static let badgeStrong = Color.secondary.opacity(0.14)
        public static let divider = Color.secondary.opacity(0.16)
        public static let alertWarning = Color.orange.opacity(0.10)
        public static let alertCritical = Color.red.opacity(0.10)
        public static let alertInfo = Color.blue.opacity(0.10)
    }

    // MARK: - Trend Direction

    public static func trendArrow(_ samples: [Float]) -> (symbol: String, color: Color) {
        guard samples.count >= 3 else { return ("minus", .secondary) }
        let recent = samples.suffix(3)
        let first = recent.first ?? 0
        let last = recent.last ?? 0
        let delta = last - first
        if delta > 2 { return ("arrow.up", .red) }
        if delta < -2 { return ("arrow.down", .green) }
        return ("minus", .secondary)
    }

    // MARK: - Friction Bar Width

    public static func frictionBarWidth(_ score: Float, maxWidth: CGFloat) -> CGFloat {
        let normalized = min(max(Double(score) / 100.0, 0), 1)
        return CGFloat(normalized) * maxWidth
    }

    // MARK: - Agent Status

    public static func agentColor(_ provider: String) -> Color {
        switch provider.lowercased() {
        case "claude": return .blue
        case "codex": return .green
        case "chatgpt": return .orange
        case "aider": return .pink
        case "cursor-agent": return .indigo
        case "ollama": return .teal
        case "mlx-lm": return .cyan
        case "llama-cpp": return .mint
        case "llamafile": return .brown
        case "lm-studio": return .purple
        case "koboldcpp": return .red
        case "whisper": return .yellow
        default: return .purple
        }
    }

    public static func agentLabel(_ provider: String) -> String {
        switch provider.lowercased() {
        case "claude": return "Claude"
        case "codex": return "Codex"
        case "chatgpt": return "ChatGPT"
        case "aider": return "Aider"
        case "cursor-agent": return "Cursor"
        case "ollama": return "Ollama"
        case "mlx-lm": return "MLX"
        case "llama-cpp": return "llama.cpp"
        case "llamafile": return "Llamafile"
        case "lm-studio": return "LM Studio"
        case "koboldcpp": return "KoboldCpp"
        case "whisper": return "Whisper"
        default:
            let label = provider
                .replacingOccurrences(of: "-", with: " ")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            return label.isEmpty ? "AI" : label.capitalized
        }
    }
}
