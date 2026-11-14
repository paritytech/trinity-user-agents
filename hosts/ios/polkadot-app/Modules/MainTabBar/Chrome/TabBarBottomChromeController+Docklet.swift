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
