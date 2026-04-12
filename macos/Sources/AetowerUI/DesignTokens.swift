import SwiftUI

enum AetowerDesign {

    // MARK: - Spacing Scale (4px base)

    enum Spacing {
        static let xxs: CGFloat = 2
        static let xs: CGFloat = 4
        static let sm: CGFloat = 8
        static let md: CGFloat = 12
        static let lg: CGFloat = 16
        static let xl: CGFloat = 20
        static let xxl: CGFloat = 24
        static let section: CGFloat = 28
    }

    // MARK: - Corner Radius

    enum Radius {
        static let sm: CGFloat = 8
        static let md: CGFloat = 12
        static let lg: CGFloat = 16
        static let pill: CGFloat = 100
    }

    // MARK: - Animation

    enum Motion {
        static let quick: Animation = .easeInOut(duration: 0.15)
        static let standard: Animation = .easeInOut(duration: 0.25)
        static let smooth: Animation = .spring(response: 0.35, dampingFraction: 0.85)
        static let slow: Animation = .easeInOut(duration: 0.5)
    }

    // MARK: - Friction Colors

    static func frictionColor(_ score: Float) -> Color {
        switch score {
        case 75...: return .red
        case 40...: return .orange
        case 15...: return .yellow
        default: return .green
        }
    }

    static func frictionLabel(_ score: Float) -> String {
        switch score {
        case 75...: return "Critical"
        case 40...: return "High"
        case 15...: return "Watch"
        default: return "Stable"
        }
    }

    // MARK: - Metric Tone Colors

    enum Tone {
        static let friction: Color = .orange
        static let cpu: Color = .blue
        static let memory: Color = .green
        static let disk: Color = .pink
        static let network: Color = .teal
        static let energy: Color = .yellow
        static let wakeups: Color = .purple
        static let gpu: Color = .indigo
    }

    enum Status {
        static let success: Color = Tone.memory
        static let ready: Color = Tone.cpu
        static let warning: Color = Tone.friction
        static let error: Color = .red
        static let neutral: Color = .secondary
    }

    // MARK: - Surface Colors

    enum Surface {
        static let rowIdle = Color.secondary.opacity(0.04)
        static let rowHover = Color.secondary.opacity(0.08)
        static let rowSelected = Color.accentColor.opacity(0.10)
        static let card = Color.secondary.opacity(0.05)
        static let cardHover = Color.secondary.opacity(0.09)
        static let badge = Color.secondary.opacity(0.08)
        static let alertWarning = Color.orange.opacity(0.10)
        static let alertCritical = Color.red.opacity(0.10)
        static let alertInfo = Color.blue.opacity(0.10)
    }

    // MARK: - Trend Direction

    static func trendArrow(_ samples: [Float]) -> (symbol: String, color: Color) {
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

    static func frictionBarWidth(_ score: Float, maxWidth: CGFloat) -> CGFloat {
        let normalized = min(max(Double(score) / 100.0, 0), 1)
        return CGFloat(normalized) * maxWidth
    }

    // MARK: - Agent Status

    static func agentColor(_ provider: String) -> Color {
        switch provider.lowercased() {
        case "claude": return .blue
        case "codex": return .green
        case "chatgpt": return .orange
        default: return .purple
        }
    }

    static func agentLabel(_ provider: String) -> String {
        switch provider.lowercased() {
        case "claude": return "Claude"
        case "codex": return "Codex"
        case "chatgpt": return "ChatGPT"
        default: return "AI"
        }
    }
}
