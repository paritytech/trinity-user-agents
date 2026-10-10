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
    let onConnectionStatusTapped: () -> Void
    let onSettingsTapped: () -> Void

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
    }
}

private extension TabBarShelfView {
    static let iconSize: CGFloat = 28
    static let touchSize: CGFloat = 44
    static let columns = [GridItem(.adaptive(minimum: 84), spacing: DSSpacings.large)]

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
                TabBarShelfCell(chip: chip, isSelected: chip.id == mountedChipId) {
                    onChipTapped(chip.id)
                }
            }
        }
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

/// One open product: its icon with the name beneath, highlighted when it is the mounted product.
private struct TabBarShelfCell: View {
    let chip: DSTabBarChip
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
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
        }
        .buttonStyle(.plain)
        .accessibilityLabel(chip.name)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
    }
}

private extension TabBarShelfCell {
    static let iconSize: CGFloat = 48

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
