import Observation
import PolkadotUI
import SwiftUI
import UIKit

/// What the SwiftUI bar row shows. The chrome writes it; the bar row reports slot taps back through `onSlotTapped`.
@MainActor
@Observable
final class TabBarBarViewModel {
    var items: [TabBarBarItem] = []
    var selectedItemIndex = 0
    var activeActionItemIndex: Int?
    /// Whether the bar row is faded out, leaving only an open centre panel. Set while the panel's search is focused.
    var isBarHidden = false
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

    /// Open fraction of the centre control, 0 closed to 1 open. SwiftUI moves it during a drag or tap, and
    /// UIKit moves it through `setCenterPanelOpen`.
    var centerProgress: CGFloat = 0
    var centerContentHeight: CGFloat = 0
    var makeCenterContent: () -> UIViewController? = { nil }
    /// Fires only when the progress settles at either end, not on every intermediate frame of a drag.
    var onCenterPanelOpenChanged: ((Bool) -> Void)?

    /// Matches the centre control's own settle, so a UIKit-driven move looks like a tap. Snaps under Reduce Motion.
    /// Once the move has logically finished it reports through `onCenterPanelOpenChanged`, as a settle from the
    /// control does, and then runs `completion`.
    func setCenterPanelOpen(_ isOpen: Bool, completion: (() -> Void)? = nil) {
        let progress: CGFloat = isOpen ? 1 : 0
        guard centerProgress != progress else {
            completion?()
            return
        }
        withAnimation(centerAnimation, completionCriteria: .logicallyComplete) {
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

    /// Changes the declared height on the centre control's spring, so the open panel moves with the change
    /// the way it moves on a settle. Unanimated changes land before the first settled layout places the bar.
    func setCenterContentHeight(_ height: CGFloat, animated: Bool = true) {
        guard animated else {
            centerContentHeight = height
            return
        }
        withAnimation(centerAnimation) {
            centerContentHeight = height
        }
    }

    private var centerAnimation: Animation? {
        UIAccessibility.isReduceMotionEnabled
            ? nil
            : .spring(response: DSTabBarStyle.centerSpringResponse, dampingFraction: DSTabBarStyle.centerSpringDamping)
    }

    private var shelfAnimation: Animation? {
        UIAccessibility.isReduceMotionEnabled
            ? nil
            : .spring(response: DSTabBarStyle.shelfSpringResponse, dampingFraction: DSTabBarStyle.shelfSpringDamping)
    }
}
