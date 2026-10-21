import DesignSystem
import ExternalAccessibility
import PolkadotUI
import SwiftUI
import UIKit

/// The bar row: four fixed slots around the centre hole, with the centre control in the hole and the
/// shelf growing above it with open products.
struct TabBarBarView: View {
    @Bindable var model: TabBarBarViewModel

    var body: some View {
        DSTabBarContainer(
            isShelfOpen: $model.isShelfOpen,
            shelfContentHeight: $model.shelfContentHeight,
            availableHeight: model.availablePanelHeight,
            selectedSlot: model.selectedSlot,
            isBarHidden: model.isBarHidden,
            playsBarHiddenFeedback: model.playsBarHiddenFeedback,
            openProgress: model.centerProgress,
            first: { slot(at: 0) },
            second: { slot(at: 1) },
            third: { slot(at: 2) },
            fourth: { slot(at: 3) },
            center: {
                DSTabBarCenterControl(
                    progress: $model.centerProgress,
                    contentHeight: model.centerContentHeight,
                    accessibilityLabel: String(localized: .Products.productTabsAccessibilityScanner),
                    openAccessibilityLabel: String(localized: .Products.productTabsAccessibilityCloseSearch),
                    onSettled: { model.onCenterPanelOpenChanged?($0) },
                    onCloseRequested: { model.onCenterPanelCloseRequested?($0) },
                    onSwipeOpenBegan: model.onCenterPanelSwipeOpenBegan,
                    onSwipeOpenLanding: model.onCenterPanelSwipeOpenLanding,
                    content: {
                        TabBarScanPanelContent(makeController: model.makeCenterContent)
                    }
                )
            },
            shelf: {
                TabBarShelfView(
                    chips: model.shelfChips,
                    mountedChipId: model.mountedChipId,
                    pillProductId: pillProductId,
                    onChipTapped: { model.onShelfChipTapped?($0) },
                    onChipCloseRequested: { model.onShelfChipCloseRequested?($0) },
                    onConnectionStatusTapped: { model.onShelfConnectionStatusTapped?() },
                    onSettingsTapped: { model.onShelfSettingsTapped?() }
                )
            }
        )
        // The bar sits at the bottom of a host sized once to the room above it.
        .frame(maxHeight: .infinity, alignment: .bottom)
    }

    private var pillProductId: UUID? {
        guard case let .productCell(id) = model.selectionPlacement else { return nil }
        return id
    }

    @ViewBuilder
    private func slot(at index: Int) -> some View {
        if let item = model.items[safe: index] {
            TabBarSlotView(
                item: item,
                isSelected: index == model.selectedSlot,
                isActive: index == model.activeActionItemIndex,
                action: { model.onSlotTapped?(index) }
            )
        }
    }
}

private struct TabBarSlotView: View {
    private static let iconSize: CGFloat = 28

    let item: TabBarBarItem
    let isSelected: Bool
    let isActive: Bool
    let action: () -> Void

    private var isEmphasized: Bool {
        isSelected || isActive
    }

    private var iconColor: Color {
        isEmphasized ? .fgPrimary : .fgSecondary
    }

    var body: some View {
        Button(action: action) {
            VStack(spacing: 0) {
                icon
                if let title = item.title {
                    Text(title)
                        .font(Font(UIFont.labelSmallEmphasized))
                        .foregroundStyle(iconColor)
                        .padding(.top, 2)
                }
            }
            .padding(.vertical, item.title == nil ? 0 : 6)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .contentShape(Rectangle())
            .animation(DSTabBarStyle.selectionCurve, value: isEmphasized)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(item.accessibilityLabel)
        .accessibilityValue(item.accessibilityValue ?? "")
        .accessibilityId(item.accessibilityID)
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
                .foregroundStyle(iconColor)
                .frame(width: Self.iconSize, height: Self.iconSize)
        case let .tabsGlyph(count):
            TabsGlyph(count: count, color: UIColor(iconColor))
                .frame(width: Self.iconSize, height: Self.iconSize)
        }
    }
}

/// Builds the scan panel when the centre control first shows content, so the camera starts on the first
/// drag and is released when the control removes its content.
private struct TabBarScanPanelContent: UIViewControllerRepresentable {
    let makeController: () -> UIViewController?
    @Environment(\.centerPanelDrag) private var drag

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeUIViewController(context: Context) -> UIViewController {
        let controller = makeController() ?? UIViewController()
        context.coordinator.drag = drag
        context.coordinator.install(on: controller.view)
        return controller
    }

    func updateUIViewController(_: UIViewController, context: Context) {
        context.coordinator.drag = drag
    }

    static func dismantleUIViewController(_: UIViewController, coordinator: Coordinator) {
        coordinator.uninstall()
    }
}

// MARK: - Drag

private extension TabBarScanPanelContent {
    /// Recognises a mostly vertical pan over the panel's content and forwards it to the centre control. Only
    /// mostly vertical motion begins the pan, so taps and horizontal drags reach the content. Once begun, the
    /// pan cancels the content's touches, so a dragged result cell is never selected. Translations are in
    /// window coordinates, because the panel moves under the finger and a view-local translation would shrink
    /// as the panel follows it.
    final class Coordinator: NSObject, UIGestureRecognizerDelegate {
        var drag: DSTabBarCenterDrag?
        private var recognizer: UIPanGestureRecognizer?
        private weak var trackedScrollView: UIScrollView?
        private var panelOwnsDrag = false
        private var panelStartTranslation: CGFloat = 0
        private var lastTranslation: CGFloat = 0
        private var panelReceivedInput = false

        func install(on view: UIView) {
            let recognizer = UIPanGestureRecognizer(target: self, action: #selector(handlePan(_:)))
            recognizer.delegate = self
            view.addGestureRecognizer(recognizer)
            self.recognizer = recognizer
        }

        func uninstall() {
            guard let recognizer else { return }
            recognizer.view?.removeGestureRecognizer(recognizer)
            self.recognizer = nil
        }

        func gestureRecognizerShouldBegin(_ recognizer: UIGestureRecognizer) -> Bool {
            let velocity = (recognizer as? UIPanGestureRecognizer)?.velocity(in: nil) ?? .zero
            return abs(velocity.y) > abs(velocity.x)
        }

        /// Lets the list's own pan run with this one, so the list scrolls under the drag until its top.
        func gestureRecognizer(
            _ recognizer: UIGestureRecognizer,
            shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer
        ) -> Bool {
            guard let scrollView = other.view as? UIScrollView,
                  other === scrollView.panGestureRecognizer,
                  let contentView = recognizer.view,
                  scrollView.isDescendant(of: contentView)
            else {
                return false
            }
            return true
        }

        /// A drag that starts on the search field belongs to the field, so the panel stays where it is.
        func gestureRecognizer(_: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
            !isInTextField(touch.view)
        }

        private func isInTextField(_ view: UIView?) -> Bool {
            guard let view else { return false }
            return sequence(first: view) { $0.superview }.contains { $0 is UITextField }
        }

        @objc private func handlePan(_ recognizer: UIPanGestureRecognizer) {
            switch recognizer.state {
            case .began:
                beginGesture(recognizer)
            case .changed:
                changeGesture(recognizer)
            case .ended:
                finishGesture(recognizer)
            case .cancelled,
                 .failed:
                cancelGesture()
            default:
                break
            }
        }

        private func beginGesture(_ recognizer: UIPanGestureRecognizer) {
            recognizer.setTranslation(.zero, in: nil)
            trackedScrollView = scrollView(under: recognizer)
            panelOwnsDrag = !canScroll(trackedScrollView)
            if panelOwnsDrag {
                forwardToPanel(0)
            }
        }

        /// The nearest scroll view at the pan's location, searching from the hit view up to the content view.
        private func scrollView(under recognizer: UIPanGestureRecognizer) -> UIScrollView? {
            guard let contentView = recognizer.view,
                  let hitView = contentView.hitTest(recognizer.location(in: contentView), with: nil)
            else {
                return nil
            }
            return sequence(first: hitView) { $0 === contentView ? nil : $0.superview }
                .compactMap { $0 as? UIScrollView }
                .first
        }

        private func changeGesture(_ recognizer: UIPanGestureRecognizer) {
            let translation = recognizer.translation(in: nil).y
            let delta = translation - lastTranslation
            lastTranslation = translation
            updateOwnership(translation: translation, delta: delta)
            if panelOwnsDrag {
                movePanel(translation: translation)
            }
        }

        /// The list keeps the drag until it is at its top and the finger moves down, then the panel takes it as a
        /// system sheet does. An upward drag at the fully open panel hands the drag back to the list.
        private func updateOwnership(translation: CGFloat, delta: CGFloat) {
            if panelOwnsDrag {
                guard canScroll(trackedScrollView), translation - panelStartTranslation <= 0, delta < 0 else { return }
                forwardToPanel(0)
                panelOwnsDrag = false
            } else {
                guard let scrollView = trackedScrollView, isAtTop(scrollView), delta > 0 else { return }
                panelOwnsDrag = true
                panelStartTranslation = translation
            }
        }

        private func movePanel(translation: CGFloat) {
            forwardToPanel(translation - panelStartTranslation)
            guard let scrollView = trackedScrollView else { return }
            scrollView.contentOffset.y = -scrollView.adjustedContentInset.top
        }

        private func finishGesture(_ recognizer: UIPanGestureRecognizer) {
            if panelReceivedInput {
                drag?.ended(velocityHeight: panelOwnsDrag ? recognizer.velocity(in: nil).y : 0)
            }
            resetGesture()
        }

        private func cancelGesture() {
            if panelReceivedInput {
                drag?.cancelled()
            }
            resetGesture()
        }

        private func forwardToPanel(_ translationHeight: CGFloat) {
            panelReceivedInput = true
            drag?.changed(translationHeight: translationHeight)
        }

        private func resetGesture() {
            trackedScrollView = nil
            panelOwnsDrag = false
            panelStartTranslation = 0
            lastTranslation = 0
            panelReceivedInput = false
        }

        private func canScroll(_ scrollView: UIScrollView?) -> Bool {
            guard let scrollView else { return false }
            let insets = scrollView.adjustedContentInset
            return scrollView.contentSize.height + insets.top + insets.bottom > scrollView.bounds.height
        }

        private func isAtTop(_ scrollView: UIScrollView) -> Bool {
            scrollView.contentOffset.y <= -scrollView.adjustedContentInset.top
        }
    }
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
