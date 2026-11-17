import CoreGraphics
import PolkadotUI

/// How far a pull-down has come toward closing an opened card, and how the body under the card shows it.
struct PocketClosePull: Equatable {
    static let maxBlur: CGFloat = 12

    let travel: CGFloat
    let screenHeight: CGFloat

    var progress: CGFloat {
        min(max(travel, 0) / DSCardCollapse.distance(in: screenHeight), 1)
    }

    var bodyAlpha: CGFloat {
        1 - progress
    }

    var closes: Bool {
        travel > DSCardCollapse.distance(in: screenHeight)
    }
}
