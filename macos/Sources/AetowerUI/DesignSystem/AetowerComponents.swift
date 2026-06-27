import SwiftUI

public enum AetowerComponentSize {
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

public enum AetowerSurfaceLevel {
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

public struct AetowerSurface<Content: View>: View {
    let level: AetowerSurfaceLevel
    let padding: CGFloat
    let cornerRadius: CGFloat
    let content: Content

    public init(
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

    public var body: some View {
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

public enum AetowerBadgeStyle {
    case soft
    case outline
    case solid
}

public struct AetowerBadge: View {
    let label: String
    let systemImage: String?
    let tone: Color
    let style: AetowerBadgeStyle
    let size: AetowerComponentSize

    public init(
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

    public var body: some View {
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

public struct AetowerMetricReadout: View {
    let label: String
    let value: String
    let detail: String?
    let tone: Color

    public init(
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

    public var body: some View {
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

public struct AetowerEmptyState: View {
    let title: String
    let detail: String
    let systemImage: String
    let tone: Color

    public init(
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

    public var body: some View {
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

public struct AetowerSection<Actions: View, Content: View>: View {
    let title: String?
    let subtitle: String?
    let actions: Actions
    let content: Content

    public init(
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

    public var body: some View {
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

public extension AetowerSection where Actions == EmptyView {
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

public extension View {
    func aetowerControlChrome(
        minHeight: CGFloat = AetowerDesign.Size.controlHeight,
        cornerRadius: CGFloat = AetowerDesign.Radius.sm
    ) -> some View {
        self
            .frame(minHeight: minHeight)
            .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: cornerRadius))
    }
}

public struct AetowerTabToolBand<LayoutTools: View, FilterTools: View, Badges: View, Actions: View>: View {
    let searchText: Binding<String>
    let searchPrompt: String
    let searchWidth: CGFloat
    let layoutTools: LayoutTools
    let filterTools: FilterTools
    let badges: Badges
    let actions: Actions

    public init(
        searchText: Binding<String>,
        searchPrompt: String = "Search this page",
        searchWidth: CGFloat = 260,
        @ViewBuilder layoutTools: () -> LayoutTools,
        @ViewBuilder filterTools: () -> FilterTools,
        @ViewBuilder badges: () -> Badges,
        @ViewBuilder actions: () -> Actions
    ) {
        self.searchText = searchText
        self.searchPrompt = searchPrompt
        self.searchWidth = searchWidth
        self.layoutTools = layoutTools()
        self.filterTools = filterTools()
        self.badges = badges()
        self.actions = actions()
    }

    public var body: some View {
        ViewThatFits(in: .horizontal) {
            horizontalBand
            wrappedBand
        }
        .padding(.horizontal, AetowerDesign.Spacing.md)
        .padding(.vertical, AetowerDesign.Spacing.sm)
        .background(.bar)
    }

    private var horizontalBand: some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            layoutTools
                .fixedSize(horizontal: true, vertical: false)

            filterTools
                .fixedSize(horizontal: true, vertical: false)

            AetowerTabSearchField(text: searchText, prompt: searchPrompt)
                .frame(width: searchWidth)
                .layoutPriority(1)

            Spacer(minLength: AetowerDesign.Spacing.sm)

            badges
                .fixedSize(horizontal: true, vertical: false)

            actions
                .fixedSize(horizontal: true, vertical: false)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var wrappedBand: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                layoutTools
                filterTools
                Spacer(minLength: AetowerDesign.Spacing.sm)
                actions
            }
            HStack(spacing: AetowerDesign.Spacing.sm) {
                AetowerTabSearchField(text: searchText, prompt: searchPrompt)
                    .frame(maxWidth: .infinity)
                badges
                    .fixedSize(horizontal: true, vertical: false)
            }
        }
    }
}

public extension AetowerTabToolBand where FilterTools == EmptyView {
    init(
        searchText: Binding<String>,
        searchPrompt: String = "Search this page",
        searchWidth: CGFloat = 260,
        @ViewBuilder layoutTools: () -> LayoutTools,
        @ViewBuilder badges: () -> Badges,
        @ViewBuilder actions: () -> Actions
    ) {
        self.searchText = searchText
        self.searchPrompt = searchPrompt
        self.searchWidth = searchWidth
        self.layoutTools = layoutTools()
        self.filterTools = EmptyView()
        self.badges = badges()
        self.actions = actions()
    }
}

public extension AetowerTabToolBand where Actions == EmptyView {
    init(
        searchText: Binding<String>,
        searchPrompt: String = "Search this page",
        searchWidth: CGFloat = 260,
        @ViewBuilder layoutTools: () -> LayoutTools,
        @ViewBuilder filterTools: () -> FilterTools,
        @ViewBuilder badges: () -> Badges
    ) {
        self.searchText = searchText
        self.searchPrompt = searchPrompt
        self.searchWidth = searchWidth
        self.layoutTools = layoutTools()
        self.filterTools = filterTools()
        self.badges = badges()
        self.actions = EmptyView()
    }
}

public extension AetowerTabToolBand where FilterTools == EmptyView, Actions == EmptyView {
    init(
        searchText: Binding<String>,
        searchPrompt: String = "Search this page",
        searchWidth: CGFloat = 260,
        @ViewBuilder layoutTools: () -> LayoutTools,
        @ViewBuilder badges: () -> Badges
    ) {
        self.searchText = searchText
        self.searchPrompt = searchPrompt
        self.searchWidth = searchWidth
        self.layoutTools = layoutTools()
        self.filterTools = EmptyView()
        self.badges = badges()
        self.actions = EmptyView()
    }
}

public struct AetowerTabSearchField: View {
    @Binding var text: String
    let prompt: String

    public var body: some View {
        HStack(spacing: AetowerDesign.Spacing.xs) {
            Image(systemName: "magnifyingglass")
                .font(AetowerDesign.Typography.compactData(size: 9))
                .foregroundStyle(AetowerDesign.Ink.tertiary)
            TextField(prompt, text: $text)
                .textFieldStyle(.plain)
                .aetowerUtilityTextInput()
                .font(AetowerDesign.Typography.caption)
            if !text.isEmpty {
                Button {
                    text = ""
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(AetowerDesign.Ink.tertiary)
                }
                .buttonStyle(.plain)
                .help("Clear search")
            }
        }
        .padding(.horizontal, AetowerDesign.Spacing.sm)
        .padding(.vertical, AetowerDesign.Spacing.xs)
        .frame(minHeight: AetowerDesign.Size.controlHeight)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm))
    }
}

public struct AetowerToolBadge: View {
    let title: String
    let value: String
    let systemImage: String
    let tone: Color

    public init(
        _ title: String,
        value: String,
        systemImage: String,
        tone: Color = AetowerDesign.Status.neutral
    ) {
        self.title = title
        self.value = value
        self.systemImage = systemImage
        self.tone = tone
    }

    public var body: some View {
        HStack(spacing: AetowerDesign.Spacing.xs) {
            Image(systemName: systemImage)
                .foregroundStyle(tone)
            Text(title.uppercased())
                .font(AetowerDesign.Typography.metadata)
                .foregroundStyle(AetowerDesign.Ink.tertiary)
                .lineLimit(1)
            Text(value)
                .font(AetowerDesign.Typography.metadataStrong)
                .foregroundStyle(AetowerDesign.Ink.primary)
                .lineLimit(1)
                .minimumScaleFactor(0.8)
        }
        .padding(.horizontal, AetowerDesign.Spacing.sm)
        .frame(height: AetowerDesign.Size.controlHeight)
        .background(tone.opacity(0.08), in: Capsule())
        .overlay {
            Capsule()
                .stroke(tone.opacity(0.14), lineWidth: AetowerDesign.Stroke.hairline)
        }
    }
}

public struct AetowerNavigationRail<Content: View>: View {
    let width: CGFloat
    let content: Content

    public init(
        width: CGFloat = 292,
        @ViewBuilder content: () -> Content
    ) {
        self.width = width
        self.content = content()
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
                content
                Spacer(minLength: AetowerDesign.Spacing.none)
            }
            .padding(AetowerDesign.Spacing.md)
        }
        .frame(width: width)
        .background(.regularMaterial)
    }
}

public struct AetowerRailGroup<Content: View>: View {
    let title: String?
    let content: Content

    public init(
        _ title: String? = nil,
        @ViewBuilder content: () -> Content
    ) {
        self.title = title
        self.content = content()
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            if let title {
                Text(title.uppercased())
                    .font(AetowerDesign.Typography.metadataStrong)
                    .foregroundStyle(AetowerDesign.Ink.tertiary)
                    .padding(.horizontal, AetowerDesign.Spacing.md)
            }
            content
        }
    }
}

public struct AetowerRailButton: View {
    let title: String
    let role: String
    let summary: String?
    let signal: String?
    let systemImage: String
    let signalTone: Color
    let isSelected: Bool
    let action: () -> Void

    public init(
        title: String,
        role: String,
        summary: String? = nil,
        signal: String? = nil,
        systemImage: String,
        signalTone: Color = AetowerDesign.Status.neutral,
        isSelected: Bool,
        action: @escaping () -> Void
    ) {
        self.title = title
        self.role = role
        self.summary = summary
        self.signal = signal
        self.systemImage = systemImage
        self.signalTone = signalTone
        self.isSelected = isSelected
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                    Image(systemName: systemImage)
                        .font(AetowerDesign.Typography.compactData(size: 15, weight: .semibold))
                        .foregroundStyle(isSelected ? Color.accentColor : AetowerDesign.Ink.secondary)
                        .frame(width: AetowerDesign.Size.iconSlot)

                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                        Text(title)
                            .font(AetowerDesign.Typography.controlLabel)
                            .foregroundStyle(AetowerDesign.Ink.primary)
                            .lineLimit(1)
                        Text(role)
                            .font(AetowerDesign.Typography.metadata)
                            .foregroundStyle(isSelected ? Color.accentColor : AetowerDesign.Ink.secondary)
                    }

                    Spacer(minLength: AetowerDesign.Spacing.xs)
                }

                if let summary {
                    Text(summary)
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }

                if let signal {
                    Text(signal)
                        .font(AetowerDesign.Typography.metadataStrong)
                        .foregroundStyle(signalTone)
                        .lineLimit(1)
                }
            }
            .padding(AetowerDesign.Spacing.md)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                isSelected ? AetowerDesign.Surface.rowSelected : AetowerDesign.Surface.card,
                in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md)
            )
            .overlay {
                RoundedRectangle(cornerRadius: AetowerDesign.Radius.md)
                    .stroke(
                        isSelected ? Color.accentColor.opacity(0.32) : Color.clear,
                        lineWidth: AetowerDesign.Stroke.hairline
                    )
            }
        }
        .buttonStyle(.plain)
    }
}

public struct AetowerSettingsSidebarButton: View {
    let title: String
    let systemImage: String
    let status: String?
    let statusTone: Color
    let isSelected: Bool
    let action: () -> Void

    public init(
        title: String,
        systemImage: String,
        status: String? = nil,
        statusTone: Color = AetowerDesign.Status.neutral,
        isSelected: Bool,
        action: @escaping () -> Void
    ) {
        self.title = title
        self.systemImage = systemImage
        self.status = status
        self.statusTone = statusTone
        self.isSelected = isSelected
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            HStack(spacing: AetowerDesign.Spacing.md) {
                Image(systemName: systemImage)
                    .foregroundStyle(isSelected ? Color.accentColor : AetowerDesign.Ink.secondary)
                    .frame(width: 18)
                Text(title)
                    .font(AetowerDesign.Typography.body.weight(.semibold))
                    .foregroundStyle(isSelected ? AetowerDesign.Ink.primary : AetowerDesign.Ink.secondary)
                    .lineLimit(1)
                Spacer(minLength: AetowerDesign.Spacing.xs)
                if let status {
                    AetowerBadge(status, tone: statusTone)
                }
            }
            .padding(.horizontal, AetowerDesign.Spacing.md)
            .padding(.vertical, AetowerDesign.Spacing.sm)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                isSelected ? AetowerDesign.Surface.rowSelected : Color.clear,
                in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md)
            )
        }
        .buttonStyle(.plain)
    }
}

public struct AetowerSelectableTile: View {
    let title: String
    let detail: String?
    let signal: String?
    let systemImage: String
    let signalTone: Color
    let isSelected: Bool
    let minHeight: CGFloat
    let action: () -> Void

    public init(
        title: String,
        detail: String? = nil,
        signal: String? = nil,
        systemImage: String,
        signalTone: Color = AetowerDesign.Status.neutral,
        isSelected: Bool,
        minHeight: CGFloat = 88,
        action: @escaping () -> Void
    ) {
        self.title = title
        self.detail = detail
        self.signal = signal
        self.systemImage = systemImage
        self.signalTone = signalTone
        self.isSelected = isSelected
        self.minHeight = minHeight
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                HStack(spacing: AetowerDesign.Spacing.sm) {
                    Image(systemName: systemImage)
                        .foregroundStyle(isSelected ? Color.accentColor : AetowerDesign.Ink.secondary)
                    Text(title)
                        .font(AetowerDesign.Typography.controlLabel)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                        .lineLimit(1)
                    Spacer(minLength: AetowerDesign.Spacing.xs)
                }
                if let signal {
                    Text(signal)
                        .font(AetowerDesign.Typography.metadataStrong)
                        .foregroundStyle(signalTone)
                        .lineLimit(1)
                }
                if let detail {
                    Text(detail)
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(2)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .padding(AetowerDesign.Spacing.sm)
            .frame(maxWidth: .infinity, minHeight: minHeight, alignment: .topLeading)
            .background(
                isSelected ? AetowerDesign.Surface.rowSelected : AetowerDesign.Surface.card,
                in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md, style: .continuous)
            )
            .overlay {
                RoundedRectangle(cornerRadius: AetowerDesign.Radius.md, style: .continuous)
                    .stroke(
                        isSelected ? Color.accentColor.opacity(0.32) : Color.clear,
                        lineWidth: AetowerDesign.Stroke.hairline
                    )
            }
        }
        .buttonStyle(.plain)
    }
}

public struct AetowerInfoBanner: View {
    let title: String?
    let detail: String
    let systemImage: String
    let tone: Color
    let level: AetowerSurfaceLevel

    public init(
        _ detail: String,
        title: String? = nil,
        systemImage: String = "info.circle",
        tone: Color = AetowerDesign.Status.neutral,
        level: AetowerSurfaceLevel = .card
    ) {
        self.title = title
        self.detail = detail
        self.systemImage = systemImage
        self.tone = tone
        self.level = level
    }

    public var body: some View {
        AetowerSurface(level: level, padding: AetowerDesign.Spacing.md, cornerRadius: AetowerDesign.Radius.md) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: systemImage)
                    .foregroundStyle(tone)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    if let title {
                        Text(title)
                            .font(AetowerDesign.Typography.sectionTitle)
                            .foregroundStyle(AetowerDesign.Ink.primary)
                    }
                    Text(detail)
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: AetowerDesign.Spacing.sm)
            }
        }
    }
}

public struct AetowerMetricTile: View {
    let title: String
    let value: String
    let detail: String?
    let systemImage: String?
    let tone: Color
    let minHeight: CGFloat
    let valueSize: CGFloat

    public init(
        _ title: String,
        value: String,
        detail: String? = nil,
        systemImage: String? = nil,
        tone: Color = AetowerDesign.Status.neutral,
        minHeight: CGFloat = 96,
        valueSize: CGFloat = 20
    ) {
        self.title = title
        self.value = value
        self.detail = detail
        self.systemImage = systemImage
        self.tone = tone
        self.minHeight = minHeight
        self.valueSize = valueSize
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                if let systemImage {
                    Image(systemName: systemImage)
                        .foregroundStyle(tone)
                }
                Text(title.uppercased())
                    .font(AetowerDesign.Typography.metadataStrong)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .lineLimit(1)
            }
            Text(value)
                .font(AetowerDesign.Typography.metricValue(size: valueSize, weight: .semibold))
                .foregroundStyle(tone)
                .monospacedDigit()
                .lineLimit(1)
                .minimumScaleFactor(0.75)
            if let detail {
                Text(detail)
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.tertiary)
                    .lineLimit(2)
            }
        }
        .padding(AetowerDesign.Spacing.sm)
        .frame(maxWidth: .infinity, minHeight: minHeight, alignment: .topLeading)
        .background(
            AetowerDesign.Surface.card,
            in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md, style: .continuous)
        )
    }
}

public struct AetowerDetailHeader: View {
    let title: String
    let detail: String

    public init(_ title: String, detail: String) {
        self.title = title
        self.detail = detail
    }

    public var body: some View {
        HStack {
            Text(title)
                .font(AetowerDesign.Typography.controlLabel)
                .foregroundStyle(AetowerDesign.Ink.primary)
            Spacer(minLength: AetowerDesign.Spacing.sm)
            Text(detail)
                .font(AetowerDesign.Typography.caption)
                .foregroundStyle(AetowerDesign.Ink.secondary)
        }
    }
}

public struct AetowerKeyValue: View {
    let title: String
    let value: String

    public init(_ title: String, _ value: String) {
        self.title = title
        self.value = value
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
            Text(title)
                .font(AetowerDesign.Typography.metadata)
                .foregroundStyle(AetowerDesign.Ink.secondary)
            Text(value)
                .font(AetowerDesign.Typography.caption)
                .foregroundStyle(AetowerDesign.Ink.primary)
                .lineLimit(1)
        }
    }
}

public struct AetowerMonospaceBlock: View {
    let text: String
    let lineLimit: Int?

    public init(_ text: String, lineLimit: Int? = 2) {
        self.text = text
        self.lineLimit = lineLimit
    }

    public var body: some View {
        Text(text)
            .font(AetowerDesign.Typography.compactData(size: 10))
            .foregroundStyle(AetowerDesign.Ink.secondary)
            .lineLimit(lineLimit)
            .textSelection(.enabled)
            .padding(AetowerDesign.Spacing.sm)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                AetowerDesign.Surface.badge,
                in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm, style: .continuous)
            )
    }
}

public struct AetowerStatusLine: View {
    let icon: String
    let color: Color
    let text: String

    public init(icon: String, color: Color, text: String) {
        self.icon = icon
        self.color = color
        self.text = text
    }

    public var body: some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.xs) {
            Image(systemName: icon)
                .foregroundStyle(color)
            Text(text)
                .font(AetowerDesign.Typography.caption)
                .foregroundStyle(AetowerDesign.Ink.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}
