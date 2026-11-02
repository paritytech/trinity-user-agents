import CoreGraphics
import Foundation
import Testing
import UIKit
@testable import polkadot_app

@Suite("DockletGlowProfile")
struct DockletGlowProfileTests {
    private let screen = CGRect(x: 0, y: 0, width: 402, height: 874)

    private func saturation(of color: UIColor) -> CGFloat {
        var hue: CGFloat = 0
        var saturation: CGFloat = 0
        var brightness: CGFloat = 0
        var alpha: CGFloat = 0
        color.getHue(&hue, saturation: &saturation, brightness: &brightness, alpha: &alpha)
        return saturation
    }

    @Test("The unblurred alpha is 0.56 at the screen edge and only the sharp part remains at the band's inner side")
    func unblurredEdgeAndInnerSide() {
        #expect(abs(DockletGlowProfile.alpha(at: 0) - 0.56) < 1e-9)
        #expect(abs(DockletGlowProfile.alpha(at: 24) - 0.5 * exp(-24 / 4.2)) < 1e-9)
    }

    @Test("The unblurred alpha falls strictly across the band")
    func unblurredFallsAcrossBand() {
        let alphas = stride(from: CGFloat(0), through: 24, by: 1).map { DockletGlowProfile.alpha(at: $0) }
        #expect(zip(alphas, alphas.dropFirst()).allSatisfy { pair in pair.0 > pair.1 })
    }

    @Test("The blurred profile has 17 stops running from the screen edge to the band's inner side")
    func blurredStopsCoverBand() {
        let stops = DockletGlowProfile.blurredStops()
        #expect(stops.count == 17)
        #expect(stops.first?.location == 0)
        #expect(stops.last?.location == 1)
    }

    @Test("The blurred alpha falls from the edge inward, and the blur lowers the edge below 0.56")
    func blurredAlphaFallsInward() {
        let alphas = DockletGlowProfile.blurredStops().map(\.alpha)
        #expect(zip(alphas, alphas.dropFirst()).allSatisfy { pair in pair.0 > pair.1 })
        #expect(alphas[0] < 0.56)
    }

    @Test("The feather is 0 at both ends of a 200 pt glow")
    func featherIsZeroAtEnds() {
        #expect(DockletGlowProfile.feather(at: 0, length: 200) == 0)
        #expect(DockletGlowProfile.feather(at: 200, length: 200) == 0)
    }

    @Test("The feather is 1 from 22 pt in to 22 pt before the far end")
    func featherIsFullAwayFromEnds() {
        #expect(DockletGlowProfile.feather(at: 22, length: 200) == 1)
        #expect(DockletGlowProfile.feather(at: 100, length: 200) == 1)
        #expect(DockletGlowProfile.feather(at: 178, length: 200) == 1)
    }

    @Test("The feather is half way at 11 pt in")
    func featherHalfWayAtEleven() {
        #expect(DockletGlowProfile.feather(at: 11, length: 200) == 0.5)
    }

    @Test("A tucked bar whose glow would start above 56 pt moves the glow top down to 56")
    func glowTopClampsToTopClearance() {
        let geometry = DockletGeometry(
            dockFrame: CGRect(x: 10, y: 100, width: 382, height: 62),
            bounds: screen
        )
        #expect(geometry.edgeGlowRect == CGRect(x: 378, y: 56, width: 24, height: 200))
    }

    @Test("On a short screen the glow shortens to keep 8 pt above the bottom")
    func glowShortensAboveBottomClearance() {
        let geometry = DockletGeometry(
            dockFrame: CGRect(x: 10, y: 600, width: 382, height: 62),
            bounds: CGRect(x: 0, y: 0, width: 402, height: 300)
        )
        #expect(geometry.edgeGlowRect == CGRect(x: 378, y: 235, width: 24, height: 57))
    }

    @Test("A grey keeps zero saturation")
    func greyStaysGrey() {
        let boosted = TabBarEdgeGlowView.saturationBoosted(UIColor(white: 0.5, alpha: 1))
        #expect(saturation(of: boosted) == 0)
    }

    @Test("A colour's saturation is multiplied by 1.3")
    func saturationMultiplied() {
        let color = UIColor(hue: 0.6, saturation: 0.5, brightness: 0.8, alpha: 1)
        #expect(abs(saturation(of: TabBarEdgeGlowView.saturationBoosted(color)) - 0.65) < 1e-4)
    }

    @Test("A boosted saturation is clamped at 1")
    func saturationClampedAtOne() {
        let color = UIColor(hue: 0.6, saturation: 0.9, brightness: 0.8, alpha: 1)
        #expect(saturation(of: TabBarEdgeGlowView.saturationBoosted(color)) == 1)
    }
}
