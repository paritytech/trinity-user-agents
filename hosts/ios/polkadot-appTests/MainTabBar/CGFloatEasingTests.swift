import CoreGraphics
import Testing
@testable import polkadot_app

@Suite("CGFloat easing")
struct CGFloatEasingTests {
    @Test("Smoothstep eases from 0 to 1 and clamps values outside that range")
    func smoothstepEasesAndClamps() {
        #expect(CGFloat(0).smoothstepped == 0)
        #expect(CGFloat(0.5).smoothstepped == 0.5)
        #expect(CGFloat(1).smoothstepped == 1)
        #expect(CGFloat(-1).smoothstepped == 0)
        #expect(CGFloat(2).smoothstepped == 1)
    }

    @Test("Rubber banding follows its slope near zero and saturates at its limit")
    func rubberBandingStartsWithSlopeAndSaturates() {
        let nearZero: CGFloat = 1e-9
        #expect(abs(nearZero.rubberBanded(limit: 0.25, slope: 0.55) / nearZero - 0.55) < 1e-6)
        #expect(abs(nearZero.rubberBanded(limit: 0.25) / nearZero - 1) < 1e-6)
        #expect(abs(CGFloat(1_000).rubberBanded(limit: 0.25) - 0.25) < 1e-3)
    }

    @Test("Rubber banding with limit 0.25 and slope 0.55 gives the overdrag curve the Docklet has always used")
    func rubberBandingMatchesDockletOverdrag() {
        let limit: CGFloat = 0.25
        for overdrag: CGFloat in [0.1, 0.5, 2] {
            let legacy = limit * (1 - 1 / (1 + 2.2 * overdrag))
            #expect(abs(overdrag.rubberBanded(limit: limit, slope: 0.55) - legacy) < 1e-12)
        }
    }

    @Test("Clamping to the unit interval keeps values inside it and pins the rest to its ends")
    func clampedToUnitPinsToEnds() {
        #expect(CGFloat(-0.5).clampedToUnit == 0)
        #expect(CGFloat(0.3).clampedToUnit == 0.3)
        #expect(CGFloat(1.5).clampedToUnit == 1)
    }
}
