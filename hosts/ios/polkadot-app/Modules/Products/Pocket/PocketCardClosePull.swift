import CoreGraphics
import PolkadotUI

struct PocketCardClosePull: Equatable {
    let startedAtRest: Bool
    let travel: CGFloat
    let screenHeight: CGFloat

    var closes: Bool {
        startedAtRest && travel > DSCardCollapse.distance(in: screenHeight)
    }
}
