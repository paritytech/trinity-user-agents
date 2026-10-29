import Observation
import PolkadotUI
import SwiftUI
import UIKit

/// What the SwiftUI bar row shows. The chrome writes it; the bar row reports slot taps back through `onSlotTapped`.
@MainActor
@Observable
final class TabBarBarViewModel {
    var items: [TabBarBarItem] = []
    /// The bar item index of the current tab, the tab beneath any mounted product.
    var selectedItemIndex = 0
    /// The Apps item's bar index, written by the chrome with the items.
    var appsItemIndex: Int?
    var activeActionItemIndex: Int?
    /// Whether the bar row is faded out, leaving only an open centre panel. Set while the panel's search is focused.
    var isBarHidden = false
    /// Whether the next `isBarHidden` change plays the container's medium impact.
    var playsBarHiddenFeedback = true
    var onSlotTapped: ((Int) -> Void)?

    /// Whether the shelf is open. Written by the chrome from the open panel, never by the shelf itself.
    var isShelfOpen = false
    /// The room open panels may take above the bar row, measured from the chrome's own safe area.
    var availablePanelHeight: CGFloat = 0
    /// The shelf content's measured height, written back by the bar row. The shelf is capped at `availablePanelHeight`.
    var shelfContentHeight: CGFloat = 0 {
        didSet { onShelfContentHeightChanged?() }
    }

    var onShelfContentHeightChanged: (() -> Void)?
    var shelfChips: [TabBarShelfChip] = []
    var mountedChipId: UUID?
    var onShelfChipTapped: ((UUID) -> Void)?
    var onShelfChipCloseRequested: ((UUID) -> Void)?
    var onShelfSettingsTapped: (() -> Void)?
    var onShelfConnectionStatusTapped: (() -> Void)?

    var selectionPlacement: TabBarSelectionPlacement {
        TabBarSelectionPolicy.placement(
            tabItemIndex: selectedItemIndex,
            mountedProductId: mountedChipId,
            isShelfOpen: isShelfOpen
        )
    }

    /// The bar slot holding the pill. Nil while the shelf's product cell holds it, so the pill is drawn once.
    var selectedSlot: Int? {
        switch selectionPlacement {
        case let .tab(itemIndex):
            itemIndex
        case .appsItem:
            appsItemIndex
        case .productCell:
            nil
        }
    }

    /// Open fraction of the centre control, 0 closed to 1 open. SwiftUI moves it during a drag or tap, and
    /// UIKit moves it through `setCenterPanelOpen`.
    var centerProgress: CGFloat = 0
    var centerContentHeight: CGFloat = 0
    var makeCenterContent: () -> UIViewController? = { nil }
    /// Fires only when the progress settles at either end, not on every intermediate frame of a drag.
    var onCenterPanelOpenChanged: ((Bool) -> Void)?
    /// Fires when the centre control would settle closed from open. The chrome closes the panel, so the close
    /// animates through `centerProgress` like an outside tap. It receives the close's duration, nil for the default.
    var onCenterPanelCloseRequested: ((TimeInterval?) -> Void)?
    /// Forwarded from the centre control's swipe open. Nil keeps a swipe opening like a tap.
    var onCenterPanelSwipeOpenBegan: (() -> Void)?
    /// Forwarded from the centre control's swipe landing, once the open is nearly done.
    var onCenterPanelSwipeOpenLanding: (() -> Void)?

    /// Matches the centre control's own settle, so a UIKit-driven move looks like a tap. Snaps under Reduce Motion.
    /// Once the move has logically finished it reports through `onCenterPanelOpenChanged`, as a settle from the
    /// control does, and then runs `completion`. `duration` sets the ease of a close, nil for the default.
    func setCenterPanelOpen(_ isOpen: Bool, duration: TimeInterval? = nil, completion: (() -> Void)? = nil) {
        let progress: CGFloat = isOpen ? 1 : 0
        guard centerProgress != progress else {
            completion?()
            return
        }
        let animation = centerAnimation(duration: isOpen ? nil : duration)
        withAnimation(animation, completionCriteria: .logicallyComplete) {
            centerProgress = progress
        } completion: {
            if self.centerProgress == progress {
                self.onCenterPanelOpenChanged?(isOpen)
            }
            completion?()
        }
    }

    /// Opens or closes the shelf on the shelf spring. `completion` runs once the move has logically finished,
    /// or at once when the shelf already matches.
    func setShelfOpen(_ isOpen: Bool, completion: (() -> Void)? = nil) {
        guard isShelfOpen != isOpen else {
            completion?()
            return
        }
        withAnimation(shelfAnimation, completionCriteria: .logicallyComplete) {
            isShelfOpen = isOpen
        } completion: {
            completion?()
        }
    }

    /// Changes the declared height on the centre control's ease, so the open panel moves with the change
    /// the way it moves on a settle. Unanimated changes land before the first settled layout places the bar.
    func setCenterContentHeight(_ height: CGFloat, animated: Bool = true) {
        guard animated else {
            centerContentHeight = height
            return
        }
        withAnimation(centerAnimation()) {
            centerContentHeight = height
        }
    }

    /// The centre control's ease over `duration`, or its default. Snaps under Reduce Motion.
    private func centerAnimation(duration: TimeInterval? = nil) -> Animation? {
        guard !UIAccessibility.isReduceMotionEnabled else {
            return nil
        }
        return duration.map { DSTabBarStyle.centerCurve(duration: $0) } ?? DSTabBarStyle.centerEaseOut
    }

    private var shelfAnimation: Animation? {
        UIAccessibility.isReduceMotionEnabled
            ? nil
            : .spring(response: DSTabBarStyle.shelfSpringResponse, dampingFraction: DSTabBarStyle.shelfSpringDamping)
    }
}
