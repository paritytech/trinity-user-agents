import CoreGraphics
import SwiftUI
@testable import PolkadotUI

/// Example values the style and feel tests check against: a 13-stop band of light, and a feel with a press, a damped
/// spring home, an approach and a pop table.
enum StyleFixtures {
    static let shimmer = CustomMessageWidgetNode.Shimmer(
        color: .white,
        stops: [0, 0.011, 0.054, 0.181, 0.44, 0.766, 0.97, 0.766, 0.44, 0.181, 0.054, 0.011, 0],
        band: 186,
        lead: 70,
        speed: 212,
        rest: 1
    )

    static let feel: CustomMessageWidgetNode.PanFeel = {
        var feel = CustomMessageWidgetNode.PanFeel()
        feel.pressScale = 1.059
        feel.pressDuration = 0.133
        feel.unpressDuration = 0.15
        feel.springOmega = 18
        feel.springDamping = 0.8
        feel.approachDuration = 0.117
        feel.approachReference = 42
        feel.pop = [
            1.059, 1.132, 1.192, 1.225, 1.235, 1.237, 1.237, 1.237, 1.232, 1.226, 1.219, 1.198, 1.149, 1.099,
            1.050, 1.023, 1.005, 0.991, 0.981, 0.977, 0.976, 0.977, 0.983, 0.988, 0.990, 0.994, 0.998, 1.0,
        ]
        feel.popOffset = 3.33
        return feel
    }()
}
