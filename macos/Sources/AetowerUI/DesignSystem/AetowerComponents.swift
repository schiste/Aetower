import SwiftUI

enum AetowerComponentSize {
    case compact
    case regular

    var horizontalPadding: CGFloat {
        switch self {
        case .compact: return AetowerDesign.Spacing.sm
        case .regular: return AetowerDesign.Spacing.md
        }
    }

    var verticalPadding: CGFloat {
        switch self {
        case .compact: return AetowerDesign.Spacing.xs
        case .regular: return AetowerDesign.Spacing.sm
        }
    }

    var labelFont: Font {
        switch self {
        case .compact: return AetowerDesign.Typography.metadata
        case .regular: return AetowerDesign.Typography.caption
        }
    }
}

enum AetowerSurfaceLevel {
    case quiet
    case card
    case selected
    case warning
    case critical

    var fill: Color {
        switch self {
        case .quiet: return Color.clear
        case .card: return AetowerDesign.Surface.card
        case .selected: return AetowerDesign.Surface.rowSelected
        case .warning: return AetowerDesign.Surface.alertWarning
        case .critical: return AetowerDesign.Surface.alertCritical
        }
    }

    var stroke: Color {
        switch self {
        case .quiet: return Color.clear
        case .card: return AetowerDesign.Surface.divider
        case .selected: return Color.accentColor.opacity(0.24)
        case .warning: return AetowerDesign.Status.warning.opacity(0.28)
        case .critical: return AetowerDesign.Status.error.opacity(0.28)
        }
    }
}

struct AetowerSurface<Content: View>: View {
    let level: AetowerSurfaceLevel
    let padding: CGFloat
    let cornerRadius: CGFloat
    let content: Content

    init(
        level: AetowerSurfaceLevel = .card,
        padding: CGFloat = AetowerDesign.Spacing.md,
        cornerRadius: CGFloat = AetowerDesign.Radius.md,
        @ViewBuilder content: () -> Content
    ) {
        self.level = level
        self.padding = padding
        self.cornerRadius = cornerRadius
        self.content = content()
    }

    var body: some View {
        content
            .padding(padding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .fill(level.fill)
            )
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .stroke(level.stroke, lineWidth: AetowerDesign.Stroke.hairline)
            )
    }
}

enum AetowerBadgeStyle {
    case soft
    case outline
    case solid
}

struct AetowerBadge: View {
    let label: String
    let systemImage: String?
    let tone: Color
    let style: AetowerBadgeStyle
    let size: AetowerComponentSize

    init(
        _ label: String,
        systemImage: String? = nil,
        tone: Color = AetowerDesign.Status.neutral,
        style: AetowerBadgeStyle = .soft,
        size: AetowerComponentSize = .compact
    ) {
        self.label = label
        self.systemImage = systemImage
        self.tone = tone
        self.style = style
        self.size = size
    }

    var body: some View {
        HStack(spacing: AetowerDesign.Spacing.xs) {
            if let systemImage {
                Image(systemName: systemImage)
            }
            Text(label)
                .lineLimit(1)
        }
        .font(size.labelFont.weight(.semibold))
        .foregroundStyle(foreground)
        .padding(.horizontal, size.horizontalPadding)
        .padding(.vertical, size.verticalPadding)
        .background(background, in: Capsule())
        .overlay {
            Capsule()
                .stroke(border, lineWidth: AetowerDesign.Stroke.hairline)
        }
    }

    private var foreground: Color {
        switch style {
        case .solid: return AetowerDesign.Ink.inverse
        case .soft, .outline: return tone
        }
    }

    private var background: Color {
        switch style {
        case .soft: return tone.opacity(0.10)
        case .outline: return Color.clear
        case .solid: return tone
        }
    }

    private var border: Color {
        switch style {
        case .soft: return tone.opacity(0.16)
        case .outline: return tone.opacity(0.40)
        case .solid: return Color.clear
        }
    }
}

struct AetowerMetricReadout: View {
    let label: String
    let value: String
    let detail: String?
    let tone: Color

    init(
        label: String,
        value: String,
        detail: String? = nil,
        tone: Color = AetowerDesign.Status.neutral
    ) {
        self.label = label
        self.value = value
        self.detail = detail
        self.tone = tone
    }

    var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
            Text(label.uppercased())
                .font(AetowerDesign.Typography.metadataStrong)
                .foregroundStyle(AetowerDesign.Ink.tertiary)
            Text(value)
                .font(AetowerDesign.Typography.metricValue(size: 22, weight: .semibold))
                .foregroundStyle(tone)
                .monospacedDigit()
                .lineLimit(1)
                .minimumScaleFactor(0.75)
            if let detail, !detail.isEmpty {
                Text(detail)
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct AetowerEmptyState: View {
    let title: String
    let detail: String
    let systemImage: String
    let tone: Color

    init(
        title: String,
        detail: String,
        systemImage: String = "tray",
        tone: Color = AetowerDesign.Status.neutral
    ) {
        self.title = title
        self.detail = detail
        self.systemImage = systemImage
        self.tone = tone
    }

    var body: some View {
        AetowerSurface(level: .quiet, padding: AetowerDesign.Spacing.lg) {
            VStack(alignment: .center, spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: systemImage)
                    .font(.title3.weight(.semibold))
                    .foregroundStyle(tone)
                Text(title)
                    .font(AetowerDesign.Typography.controlLabel)
                    .foregroundStyle(AetowerDesign.Ink.primary)
                Text(detail)
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .multilineTextAlignment(.center)
            }
            .frame(maxWidth: .infinity)
        }
    }
}

struct AetowerSection<Actions: View, Content: View>: View {
    let title: String?
    let subtitle: String?
    let actions: Actions
    let content: Content

    init(
        _ title: String? = nil,
        subtitle: String? = nil,
        @ViewBuilder actions: () -> Actions,
        @ViewBuilder content: () -> Content
    ) {
        self.title = title
        self.subtitle = subtitle
        self.actions = actions()
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            if title != nil || subtitle != nil {
                HStack(alignment: .firstTextBaseline, spacing: AetowerDesign.Spacing.md) {
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                        if let title {
                            Text(title)
                                .font(AetowerDesign.Typography.sectionTitle)
                                .foregroundStyle(AetowerDesign.Ink.primary)
                        }
                        if let subtitle {
                            Text(subtitle)
                                .font(AetowerDesign.Typography.caption)
                                .foregroundStyle(AetowerDesign.Ink.secondary)
                        }
                    }
                    Spacer(minLength: AetowerDesign.Spacing.md)
                    actions
                }
            }
            content
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

extension AetowerSection where Actions == EmptyView {
    init(
        _ title: String? = nil,
        subtitle: String? = nil,
        @ViewBuilder content: () -> Content
    ) {
        self.title = title
        self.subtitle = subtitle
        self.actions = EmptyView()
        self.content = content()
    }
}
