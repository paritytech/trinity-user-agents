import Observation
import PolkadotUI
import SwiftUI
import UIKit

/// What the SwiftUI bar row shows. The chrome writes it; the bar row reports slot taps back through `onSlotTapped`.
@MainActor
@Observable
final class TabBarBarViewModel {
    var items: [DSTabBarItem] = []
    var selectedItemIndex = 0
    var activeActionItemIndex: Int?
    var onSlotTapped: ((Int) -> Void)?

    /// Whether the shelf is open. Written by the chrome from the open panel, never by the shelf itself.
    var isShelfOpen = false
    /// The room the shelf may take above the bar row, measured from the chrome's own safe area.
    var shelfAvailableHeight: CGFloat = 0
    var shelfChips: [DSTabBarChip] = []
    var mountedChipId: UUID?
    var onShelfChipTapped: ((UUID) -> Void)?
    var onShelfSettingsTapped: (() -> Void)?
    var onShelfConnectionStatusTapped: (() -> Void)?

    /// Open fraction of the centre control, 0 closed to 1 open. SwiftUI moves it during a drag or tap, and
    /// UIKit moves it through `setCenterPanelOpen`.
    var centerProgress: CGFloat = 0
    var centerContentHeight: CGFloat = 0
    var makeCenterContent: () -> UIViewController? = { nil }
    /// Fires only when the progress settles at either end, not on every intermediate frame of a drag.
    var onCenterPanelOpenChanged: ((Bool) -> Void)?

    /// Reports the panel through `onCenterPanelOpenChanged` once the progress has settled at one end of
    /// its travel. Anything in between is a drag still in flight and reports nothing.
    func reportCenterProgressSettled() {
        switch centerProgress {
        case 0:
            onCenterPanelOpenChanged?(false)
        case 1:
            onCenterPanelOpenChanged?(true)
        default:
            break
        }
    }

    /// Matches the centre control's own settle, so a UIKit-driven move looks like a tap. Snaps under Reduce Motion.
    func setCenterPanelOpen(_ isOpen: Bool) {
        let progress: CGFloat = isOpen ? 1 : 0
        guard centerProgress != progress else {
            return
        }
        let animation: Animation? = UIAccessibility.isReduceMotionEnabled
            ? nil
            : .spring(response: DSTabBarStyle.centerSpringResponse, dampingFraction: DSTabBarStyle.centerSpringDamping)
        withAnimation(animation) {
            centerProgress = progress
        }
    }
}
