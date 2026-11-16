import UIKit
import SwiftUI

extension TabBarBottomChromeController {
    /// Starts the content fade toward `target` with the animation of the transition that set it. A target with
    /// instant timing lands at once.
    func applyContentFade(_ target: DockletContentFadeTarget) {
        switch target.timing {
        case .instant:
            var transaction = Transaction()
            transaction.disablesAnimations = true
            withTransaction(transaction) {
                barModel.contentFade.opacity = target.opacity
            }
        case let .linear(duration):
            withAnimation(.linear(duration: duration)) {
                barModel.contentFade.opacity = target.opacity
            }
        }
    }

    /// Height the bar occupies at the bottom edge once it has settled in the state `context` resolves to. Drags and
    /// panel animations move the chrome without changing it; it changes when the Docklet settles on a state.
    func restingBarHeight(for context: TabBarChromeContext) -> CGFloat {
        view.safeAreaInsets.bottom + bottomClearance(for: context)
    }

    /// The Clearance the bar publishes for `context`, which is zero unless the bar resolves to shown.
    func bottomClearance(for context: TabBarChromeContext) -> CGFloat {
        resolvedState(for: context).bottomClearance(bottomSafeAreaInset: view.safeAreaInsets.bottom)
    }
}

extension DockletEdge {
    /// The tip arrow side that points from the Docklet tip toward this edge.
    var popoverArrowDirection: UIPopoverArrowDirection {
        switch self {
        case .right:
            .right
        }
    }
}
