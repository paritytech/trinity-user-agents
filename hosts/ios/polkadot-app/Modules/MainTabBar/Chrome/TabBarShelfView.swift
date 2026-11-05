import DesignSystem
import ExternalAccessibility
import PolkadotUI
import SwiftUI
import UIKit

/// The shelf's content: the settings and connection status pair at its top edge, then the open products
/// as a grid. The bar supplies the material and the growth; this view supplies what the shelf shows.
struct TabBarShelfView: View {
    let model: TabBarBarViewModel
    /// The product cell holding the selection pill. Set only while the shelf is open, so the pill leaves the cell
    /// as the shelf closes.
    let pillProductId: UUID?

    var body: some View {
        VStack(spacing: DSSpacings.large) {
            TabBarShelfIconRow(model: model)
            if model.shelfChips.isEmpty {
                TabBarShelfEmptyState()
            } else {
                TabBarShelfGrid(model: model, pillProductId: pillProductId)
            }
        }
        .padding(DSSpacings.mediumIncreased)
        .frame(maxWidth: .infinity)
    }
}

/// The connection status and settings buttons at the top edge of the shelf.
private struct TabBarShelfIconRow: View {
    let model: TabBarBarViewModel

    var body: some View {
        HStack(spacing: DSSpacings.large) {
            Spacer()
            iconButton(
                DSTabBarConnectionGlyph.image,
                label: .Common.connectionStatusAccessibilityLabel,
                action: { model.onShelfConnectionStatusTapped?() }
            )
            iconButton(
                UIImage.tabSettings,
                label: .tabSettings,
                action: { model.onShelfSettingsTapped?() }
            )
            .accessibilityId(AccessibilityID.Tab.settings)
        }
    }
}

private extension TabBarShelfIconRow {
    static let iconSize: CGFloat = 28
    static let touchSize: CGFloat = 44

    func iconButton(_ image: UIImage, label: LocalizedStringResource, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(uiImage: image)
                .renderingMode(.template)
                .resizable()
                .scaledToFit()
                .frame(width: Self.iconSize, height: Self.iconSize)
                .foregroundStyle(Color.fgSecondary)
                .frame(width: Self.touchSize, height: Self.touchSize)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text(label))
    }
}

/// The message the shelf shows while no products are open.
private struct TabBarShelfEmptyState: View {
    var body: some View {
        Text(.Products.productTabsEmpty)
            .typography(.bodySmall)
            .foregroundStyle(Color.fgSecondary)
            .lineLimit(1)
    }
}

/// The open products as a grid of cells.
private struct TabBarShelfGrid: View {
    let model: TabBarBarViewModel
    let pillProductId: UUID?

    var body: some View {
        LazyVGrid(columns: Self.columns, spacing: DSSpacings.small) {
            ForEach(model.shelfChips) { chip in
                TabBarShelfCell(
                    chip: chip,
                    isSelected: chip.id == model.mountedChipId,
                    hasPill: chip.id == pillProductId,
                    model: model
                )
            }
        }
    }
}

private extension TabBarShelfGrid {
    static let columns = [GridItem(.adaptive(minimum: 56), spacing: DSSpacings.small, alignment: .top)]
}

/// One open product: its icon with the name beneath, highlighted when it is the mounted product and enclosed by
/// the selection pill while it holds it. A long press opens a context menu holding Close.
private struct TabBarShelfCell: View {
    let chip: TabBarShelfChip
    let isSelected: Bool
    let hasPill: Bool
    let model: TabBarBarViewModel

    var body: some View {
        Button {
            model.onShelfChipTapped?(chip.id)
        } label: {
            VStack(spacing: DSSpacings.small) {
                icon
                Text(verbatim: chip.name)
                    .font(Font(UIFont.labelSmallEmphasized))
                    .foregroundStyle(isSelected ? Color.fgPrimary : Color.fgSecondary)
                    .multilineTextAlignment(.center)
                    .lineLimit(2)
            }
            .padding(DSSpacings.extraSmall)
            .frame(maxWidth: .infinity)
            .dsTabBarSelectionPill(isSelected: hasPill)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(chip.name)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
        .contextMenu {
            Button(role: .destructive) {
                close()
            } label: {
                Label(String(localized: .Common.close), systemImage: "xmark")
            }
        }
        .accessibilityAction(named: String(localized: .Common.close)) {
            close()
        }
    }
}

private extension TabBarShelfCell {
    static let iconSize: CGFloat = 40

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

    func close() {
        model.onShelfChipCloseRequested?(chip.id)
    }
}
