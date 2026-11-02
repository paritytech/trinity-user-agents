import UIKit

struct PocketCardClosePull: Equatable {
    let startedAtRest: Bool
    let travel: CGFloat
    let screenHeight: CGFloat

    static func closeDistance(for screenHeight: CGFloat) -> CGFloat {
        if #available(iOS 26, *) {
            max(screenHeight / 4, 1)
        } else {
            max(screenHeight / 6, 1)
        }
    }

    var closes: Bool {
        startedAtRest && travel > Self.closeDistance(for: screenHeight)
    }
}
