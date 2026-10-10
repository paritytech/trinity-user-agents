import DesignSystem
import ExternalAccessibility
import PolkadotUI
import SwiftUI
import UIKit

/// The shelf's content: the settings and connection status pair at its top edge, then the open products
/// as a grid. The bar supplies the material and the growth; this view supplies what the shelf shows.
struct TabBarShelfView: View {
    let chips: [DSTabBarChip]
    let mountedChipId: UUID?
    let onChipTapped: (UUID) -> Void
    let onChipCloseRequested: (UUID) -> Void
    let onConnectionStatusTapped: () -> Void
    let onSettingsTapped: () -> Void

    /// Held here so it leaves with the shelf: this view is only in the hierarchy while the shelf is open.
    @State private var revealedChipId: UUID?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(spacing: DSSpacings.large) {
            iconRow
            if chips.isEmpty {
                emptyState
            } else {
                grid
            }
        }
        .padding(DSSpacings.mediumIncreased)
        .frame(maxWidth: .infinity)
        .contentShape(Rectangle())
        .onTapGesture { revealedChipId = nil }
        .sensoryFeedback(.impact(weight: .medium), trigger: revealedChipId) { _, revealed in revealed != nil }
    }
}

private extension TabBarShelfView {
    static let iconSize: CGFloat = 28
    static let touchSize: CGFloat = 44
    static let columns = [GridItem(.adaptive(minimum: 84), spacing: DSSpacings.large)]

    /// A literal because the `DSTabBarStyle` timings are internal to PolkadotUI.
    static let reduceMotionRevealDuration: TimeInterval = 0.2

    var iconRow: some View {
        HStack(spacing: DSSpacings.large) {
            Spacer()
            iconButton(
                DSTabBarConnectionGlyph.image,
                label: String(localized: .Common.connectionStatusAccessibilityLabel),
                action: onConnectionStatusTapped
            )
            iconButton(
                UIImage.tabSettings,
                label: String(localized: .tabSettings),
                action: onSettingsTapped
            )
            .accessibilityId(AccessibilityID.Tab.settings)
        }
    }

    var emptyState: some View {
        Text(.Products.productTabsEmpty)
            .typography(.bodySmall)
            .foregroundStyle(Color.fgSecondary)
            .lineLimit(1)
    }

    var grid: some View {
        LazyVGrid(columns: Self.columns, spacing: DSSpacings.large) {
            ForEach(chips, id: \.id) { chip in
                TabBarShelfCell(
                    chip: chip,
                    isSelected: chip.id == mountedChipId,
                    isRevealed: chip.id == revealedChipId,
                    action: { onChipTapped(chip.id) },
                    onLongPress: { revealedChipId = chip.id },
                    onClose: {
                        revealedChipId = nil
                        onChipCloseRequested(chip.id)
                    }
                )
            }
        }
        .animation(
            reduceMotion ? .easeInOut(duration: Self.reduceMotionRevealDuration) : .snappy,
            value: revealedChipId
        )
    }

    func iconButton(_ image: UIImage, label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(uiImage: image)
                .renderingMode(.template)
                .resizable()
                .scaledToFit()
                .frame(width: Self.iconSize, height: Self.iconSize)
                .foregroundStyle(Color.fgPrimary)
                .frame(width: Self.touchSize, height: Self.touchSize)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }
}

/// One open product: its icon with the name beneath, highlighted when it is the mounted product. A long press
/// reveals a close control over the icon. The Close accessibility action gives the same result without a long press.
private struct TabBarShelfCell: View {
    let chip: DSTabBarChip
    let isSelected: Bool
    let isRevealed: Bool
    let action: () -> Void
    let onLongPress: () -> Void
    let onClose: () -> Void

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        ZStack(alignment: .topTrailing) {
            VStack(spacing: DSSpacings.small) {
                icon
                Text(verbatim: chip.name)
                    .font(Font(UIFont.labelSmallEmphasized))
                    .foregroundStyle(isSelected ? Color.fgPrimary : Color.fgSecondary)
                    .multilineTextAlignment(.center)
                    .lineLimit(2)
            }
            .frame(maxWidth: .infinity)
            .contentShape(Rectangle())
            .onTapGesture(perform: action)
            .onLongPressGesture(perform: onLongPress)
            .accessibilityLabel(chip.name)
            .accessibilityAddTraits(.isButton)
            .accessibilityAddTraits(isSelected ? .isSelected : [])
            .accessibilityAction(named: Text(String(localized: .Common.close)), onClose)

            if isRevealed {
                closeControl
                    .transition(reduceMotion ? .opacity : .scale.combined(with: .opacity))
            }
        }
    }
}

private extension TabBarShelfCell {
    static let iconSize: CGFloat = 48
    static let closeSize: CGFloat = 32
    static let closeTouchSize: CGFloat = 44

    /// Hidden from accessibility because the Close action on the cell reaches the same close.
    var closeControl: some View {
        Button(action: onClose) {
            Image(systemName: "xmark")
                .font(.system(size: 12, weight: .bold))
                .foregroundStyle(Color.fgPrimary)
                .frame(width: Self.closeSize, height: Self.closeSize)
                .background(Circle().fill(Color.bgSurfaceMain))
                .frame(width: Self.closeTouchSize, height: Self.closeTouchSize)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityHidden(true)
    }

    /// Reserves the icon's frame when the product has no icon, so names stay aligned across the grid.
    var icon: some View {
        ZStack {
            if let viewModel = chip.icon {
                AsyncImageView(
                    viewModel: viewModel,
                    settings: ImageViewModelSettings(
                        targetSize: CGSize(width: Self.iconSize, height: Self.iconSize),
                        cornerRadius: Self.iconSize / 2
                    ),
                    animated: false
                )
            }
        }
        .frame(width: Self.iconSize, height: Self.iconSize)
    }
}
