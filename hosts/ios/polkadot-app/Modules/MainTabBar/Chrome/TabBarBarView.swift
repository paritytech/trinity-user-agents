import DesignSystem
import ExternalAccessibility
import PolkadotUI
import SwiftUI
import UIKit

/// The bar row: four fixed slots around the centre hole, with the centre control in the hole and the
/// product shelf growing above it.
struct TabBarBarView: View {
    @Bindable var model: TabBarBarViewModel

    var body: some View {
        DSTabBarContainer(
            isShelfOpen: $model.isShelfOpen,
            availableHeight: model.shelfAvailableHeight,
            first: { slot(at: 0) },
            second: { slot(at: 1) },
            third: { slot(at: 2) },
            fourth: { slot(at: 3) },
            center: {
                DSTabBarCenterControl(progress: $model.centerProgress, contentHeight: model.centerContentHeight) {
                    TabBarScanPanelContent(makeController: model.makeCenterContent)
                }
            },
            shelf: {
                TabBarShelfView(
                    chips: model.shelfChips,
                    mountedChipId: model.mountedChipId,
                    onChipTapped: { model.onShelfChipTapped?($0) },
                    onChipCloseRequested: { model.onShelfChipCloseRequested?($0) },
                    onConnectionStatusTapped: { model.onShelfConnectionStatusTapped?() },
                    onSettingsTapped: { model.onShelfSettingsTapped?() }
                )
            }
        )
        .onChange(of: model.centerProgress) { _, _ in
            model.reportCenterProgressSettled()
        }
        // The hosting view grows upward while the centre panel is open, so the row stays at its bottom.
        .frame(maxHeight: .infinity, alignment: .bottom)
    }

    @ViewBuilder
    private func slot(at index: Int) -> some View {
        if let item = model.items[safe: index] {
            TabBarSlotView(
                item: item,
                isSelected: item.role == .tab && index == model.selectedItemIndex,
                isActive: index == model.activeActionItemIndex,
                action: { model.onSlotTapped?(index) }
            )
        }
    }
}

private struct TabBarSlotView: View {
    private static let iconSize: CGFloat = 28

    let item: DSTabBarItem
    let isSelected: Bool
    let isActive: Bool
    let action: () -> Void

    private var isEmphasized: Bool {
        isSelected || isActive
    }

    var body: some View {
        Button(action: action) {
            VStack(spacing: 0) {
                icon
                if let title = item.title {
                    Text(title)
                        .font(Font(UIFont.labelSmallEmphasized))
                        .foregroundStyle(isEmphasized ? Color.fgPrimary : Color.fgSecondary)
                        .padding(.top, 2)
                }
            }
            .padding(.vertical, item.title == nil ? 0 : 6)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(item.accessibilityLabel)
        .accessibilityId(rawValue: item.accessibilityIdentifier)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
    }

    @ViewBuilder
    private var icon: some View {
        switch item.content {
        case let .icon(image):
            Image(uiImage: image)
                .renderingMode(.template)
                .resizable()
                .scaledToFit()
                .foregroundStyle(isEmphasized ? Color.fgPrimary : Color.fgSecondary)
                .frame(width: Self.iconSize, height: Self.iconSize)
        case let .tabsGlyph(count):
            TabsGlyph(count: count, color: isEmphasized ? .fgPrimary : .fgSecondary)
                .frame(width: Self.iconSize, height: Self.iconSize)
        }
    }
}

/// Builds the scan panel when the centre control first shows content, so the camera starts on the first
/// drag and is released when the control removes its content.
private struct TabBarScanPanelContent: UIViewControllerRepresentable {
    let makeController: () -> UIViewController?

    func makeUIViewController(context _: Context) -> UIViewController {
        makeController() ?? UIViewController()
    }

    func updateUIViewController(_: UIViewController, context _: Context) {}
}

/// Renders the open-apps count glyph from the bar's UIKit component.
private struct TabsGlyph: UIViewRepresentable {
    let count: Int
    let color: UIColor

    func makeUIView(context _: Context) -> DSTabBarTabsGlyphView {
        DSTabBarTabsGlyphView(frame: .zero)
    }

    func updateUIView(_ view: DSTabBarTabsGlyphView, context _: Context) {
        view.count = count
        view.glyphColor = color
    }
}
