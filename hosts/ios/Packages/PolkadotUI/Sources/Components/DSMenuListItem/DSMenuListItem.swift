import DesignSystem
import ExternalAccessibility
import SwiftUI

public enum DSMenuListItemPosition: Hashable {
    case first
    case middle
    case last
    case standalone
}

public enum DSMenuListItemStyle: Hashable {
    case `default`
    case attention
}

public struct DSMenuListItem: View, Identifiable {
    public typealias Position = DSMenuListItemPosition
    public typealias Style = DSMenuListItemStyle

    public let id: AnyHashable
    private let title: String
    private let descriptionText: String?
    private let style: Style
    private let icon: ImageResource?
    private let rightSlot: DSMenuListItemRightSlot.Style?
    private let accessibilityId: (any AccessibilityIdentifying)?
    private let action: (() -> Void)?

    @Environment(\.dsMenuListItemPosition) private var position

    public init(
        id: AnyHashable,
        title: String,
        description: String? = nil,
        style: Style = .default,
        icon: ImageResource? = nil,
        rightSlot: DSMenuListItemRightSlot.Style? = nil,
        accessibilityId: (any AccessibilityIdentifying)? = nil,
        action: (() -> Void)? = nil
    ) {
        self.id = id
        self.title = title
        descriptionText = description
        self.style = style
        self.icon = icon
        self.rightSlot = rightSlot
        self.accessibilityId = accessibilityId
        self.action = action
    }

    public var body: some View {
        if let action {
            Button(action: action) {
                rowContent
            }
            .buttonStyle(DSMenuListItemButtonStyle(position: position))
            .accessibilityId(accessibilityId)
        } else {
            rowContent
                .background(Color.bgSurfaceContainer, in: surfaceShape)
                .accessibilityId(accessibilityId)
        }
    }

    private var rowContent: some View {
        HStack(spacing: 0) {
            leftSlot
            centralAndRightSlot
        }
        .padding(.horizontal, DSSpacings.mediumIncreased)
    }

    @ViewBuilder
    private var leftSlot: some View {
        if let icon {
            ZStack {
                Image(icon)
                    .resizable()
                    .scaledToFit()
                    .frame(width: 20, height: 20)
                    .foregroundStyle(Color.fgPrimary)
            }
            .frame(width: 32, height: 32)
            .padding(.trailing, DSSpacings.smallIncreased)
        }
    }

    private var centralAndRightSlot: some View {
        HStack(spacing: DSSpacings.small) {
            centralSlot
            rightSlotContent
        }
        .padding(.vertical, DSSpacings.small)
        .frame(alignment: .leading)
        .overlay(alignment: .bottom) {
            if showsBottomDivider {
                Rectangle()
                    .fill(Color.strokePrimary)
                    .frame(height: 1)
            }
        }
    }

    private var centralSlot: some View {
        VStack(alignment: .leading, spacing: DSSpacings.zero) {
            Text(title)
                .typography(.bodyLarge)
                .foregroundStyle(Color.fgPrimary)
                .lineLimit(1)

            if let descriptionText {
                Text(descriptionText)
                    .typography(.bodySmall.emphasized)
                    .foregroundStyle(descriptionColor)
                    .lineLimit(2)
            }
        }
        .padding(
            .vertical,
            descriptionText == nil ? DSSpacings.extraSmall : DSSpacings.extraTiny
        )
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var rightSlotContent: some View {
        HStack(spacing: DSSpacings.small) {
            if style == .attention {
                Image(.iconAlertCircle)
                    .resizable()
                    .scaledToFit()
                    .frame(width: 20, height: 20)
                    .foregroundStyle(Color.fgWarning)
            }
            if let rightSlot {
                DSMenuListItemRightSlot(rightSlot)
            }
        }
    }

    private var descriptionColor: Color {
        switch style {
        case .default: Color.fgSecondary
        case .attention: Color.fgWarning
        }
    }

    private var showsBottomDivider: Bool {
        switch position {
        case .first,
             .middle: true
        case .last,
             .standalone: false
        }
    }

    private var surfaceShape: UnevenRoundedRectangle {
        UnevenRoundedRectangle(cornerRadii: cornerRadii(for: position), style: .continuous)
    }
}

private func cornerRadii(for position: DSMenuListItemPosition) -> RectangleCornerRadii {
    let top = DSRadii.mediumIncreased
    let bottom = DSRadii.mediumIncreased
    switch position {
    case .first:
        return RectangleCornerRadii(topLeading: top, bottomLeading: 0, bottomTrailing: 0, topTrailing: top)
    case .middle:
        return RectangleCornerRadii(topLeading: 0, bottomLeading: 0, bottomTrailing: 0, topTrailing: 0)
    case .last:
        return RectangleCornerRadii(topLeading: 0, bottomLeading: bottom, bottomTrailing: bottom, topTrailing: 0)
    case .standalone:
        return RectangleCornerRadii(
            topLeading: top,
            bottomLeading: bottom,
            bottomTrailing: bottom,
            topTrailing: top
        )
    }
}

private struct DSMenuListItemButtonStyle: ButtonStyle {
    let position: DSMenuListItemPosition

    func makeBody(configuration: Configuration) -> some View {
        let shape = UnevenRoundedRectangle(cornerRadii: cornerRadii(for: position), style: .continuous)
        return configuration.label
            .background(
                configuration.isPressed ? Color.bgSelectionContainerHover : Color.bgSurfaceContainer,
                in: shape
            )
            .contentShape(shape)
    }
}

private struct DSMenuListItemPositionKey: EnvironmentKey {
    static let defaultValue: DSMenuListItemPosition = .standalone
}

extension EnvironmentValues {
    var dsMenuListItemPosition: DSMenuListItemPosition {
        get { self[DSMenuListItemPositionKey.self] }
        set { self[DSMenuListItemPositionKey.self] = newValue }
    }
}

public extension View {
    func dsMenuListItemPosition(_ position: DSMenuListItemPosition) -> some View {
        environment(\.dsMenuListItemPosition, position)
    }

    /// Wraps arbitrary content in the same grouped-cell surface a `DSMenuListItem` uses, so custom
    /// rows (e.g. an inline control) sit flush inside a menu list group instead of looking like a
    /// separate card.
    func dsMenuListCellSurface(
        position: DSMenuListItemPosition,
        showsDivider: Bool = false
    ) -> some View {
        modifier(DSMenuListCellSurface(position: position, showsDivider: showsDivider))
    }
}

private struct DSMenuListCellSurface: ViewModifier {
    let position: DSMenuListItemPosition
    let showsDivider: Bool

    func body(content: Content) -> some View {
        content
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                Color.bgSurfaceContainer,
                in: UnevenRoundedRectangle(cornerRadii: cornerRadii(for: position), style: .continuous)
            )
            .overlay(alignment: .bottom) {
                if showsDivider {
                    Rectangle()
                        .fill(Color.strokePrimary)
                        .frame(height: 1)
                        .padding(.horizontal, DSSpacings.mediumIncreased)
                }
            }
    }
}

#if DEBUG
    #Preview("Grouped — all right-slot variants") {
        DSMenuListItemPreviewWrapper()
    }

    #Preview("Standalone variants") {
        VStack(spacing: 16) {
            DSMenuListItem(
                id: "logout",
                title: "Logout",
                action: {}
            )

            DSMenuListItem(
                id: "network",
                title: "Network",
                description: "Description Here & 2 Line of Description",
                icon: .iconAlertCircle,
                rightSlot: .labelChevron("Polkadot"),
                action: {}
            )

            DSMenuListItem(
                id: "plain",
                title: "Plain row, no action"
            )
        }
        .padding()
        .background(Color.bgSurfaceMain)
    }

    #Preview("Attention style") {
        VStack(spacing: 0) {
            DSMenuListItem(
                id: "backup",
                title: "Backup",
                description: "Not backed up. Set up a backup to stay protected",
                style: .attention,
                icon: .iconAlertCircle,
                rightSlot: .chevron,
                action: {}
            )
            .dsMenuListItemPosition(.first)

            DSMenuListItem(
                id: "apps",
                title: "Apps",
                icon: .iconAlertCircle,
                rightSlot: .chevron,
                action: {}
            )
            .dsMenuListItemPosition(.middle)

            DSMenuListItem(
                id: "currency",
                title: "Currency",
                icon: .iconAlertCircle,
                rightSlot: .labelChevron("USD"),
                action: {}
            )
            .dsMenuListItemPosition(.last)
        }
        .padding()
        .background(Color.bgSurfaceMain)
    }

    private struct DSMenuListItemPreviewWrapper: View {
        @State private var notificationsOn = true
        @State private var soundOn = false
        @State private var selectedNetwork = "Polkadot"

        var body: some View {
            VStack(spacing: 0) {
                DSMenuListItem(
                    id: "network",
                    title: "Network",
                    description: "Description Here & 2 Line of Description",
                    icon: .iconAlertCircle,
                    rightSlot: .labelChevron(selectedNetwork),
                    action: {}
                )
                .dsMenuListItemPosition(.first)

                DSMenuListItem(
                    id: "currency",
                    title: "Currency",
                    icon: .iconAlertCircle,
                    rightSlot: .labelOnly("USD"),
                    action: {}
                )
                .dsMenuListItemPosition(.middle)

                DSMenuListItem(
                    id: "notifications",
                    title: "Notifications",
                    icon: .iconAlertCircle,
                    rightSlot: .toggle($notificationsOn)
                )
                .dsMenuListItemPosition(.middle)

                DSMenuListItem(
                    id: "sound",
                    title: "Sound",
                    icon: .iconAlertCircle,
                    rightSlot: .toggle($soundOn)
                )
                .dsMenuListItemPosition(.middle)

                DSMenuListItem(
                    id: "polkadot",
                    title: "Polkadot",
                    icon: .iconAlertCircle,
                    rightSlot: .radio(isOn: selectedNetwork == "Polkadot"),
                    action: { selectedNetwork = "Polkadot" }
                )
                .dsMenuListItemPosition(.middle)

                DSMenuListItem(
                    id: "kusama",
                    title: "Kusama",
                    icon: .iconAlertCircle,
                    rightSlot: .radio(isOn: selectedNetwork == "Kusama"),
                    action: { selectedNetwork = "Kusama" }
                )
                .dsMenuListItemPosition(.middle)

                DSMenuListItem(
                    id: "about",
                    title: "About",
                    icon: .iconAlertCircle,
                    action: {}
                )
                .dsMenuListItemPosition(.last)
            }
            .padding()
            .background(Color.bgSurfaceMain)
        }
    }
#endif
