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

    @Test("The blurred edge is about 0.194, and the peak sits inside the band, not at the edge")
    func blurredEdgeBelowInnerPeak() {
        let edge = DockletGlowProfile.blurredAlpha(at: 0)
        #expect(abs(edge - 0.194) < 0.005)
        #expect(DockletGlowProfile.blurredAlpha(at: 1.5) > edge)
        #expect(DockletGlowProfile.blurredAlpha(at: 3) > edge)
    }

    @Test("The blurred alpha falls strictly from 3 pt in to the band's inner side")
    func blurredAlphaFallsPastPeak() {
        let alphas = stride(from: CGFloat(3), through: 24, by: 1.5).map { DockletGlowProfile.blurredAlpha(at: $0) }
        #expect(zip(alphas, alphas.dropFirst()).allSatisfy { pair in pair.0 > pair.1 })
    }

    @Test("In the middle of a 200 pt glow the taper leaves the distance from the edge unchanged")
    func taperLeavesMiddleUnchanged() {
        #expect(DockletGlowProfile.taperedDistance(inFromEdge: 5, along: 100, length: 200) == 5)
    }

    @Test("At the tip the taper reaches the band's inner side, so the glow and the shimmer are gone")
    func tipIsFullyTapered() {
        let tipDistance = DockletGlowProfile.taperedDistance(inFromEdge: 0, along: 0, length: 200)
        #expect(tipDistance == DockletGlowProfile.bandWidth)
        #expect(DockletGlowProfile.glowAlpha(inFromEdge: 0, along: 0, length: 200) == 0)
        #expect(DockletGlowProfile.shimmerStrength(inFromEdge: 0, along: 0, length: 200) == 0)
    }

    @Test("The taper mirrors between the top and the bottom of a 200 pt glow, so the glow and the shimmer do too")
    func taperSymmetric() {
        for position in [CGFloat(0), 5, 11, 22, 60] {
            let distance = DockletGlowProfile.taperedDistance(inFromEdge: 3, along: position, length: 200)
            let mirroredDistance = DockletGlowProfile.taperedDistance(inFromEdge: 3, along: 200 - position, length: 200)
            #expect(distance == mirroredDistance)
        }
    }

    @Test("Half way into the taper the glow reaches less far in than mid-length, and the edge stays lit")
    func endsAreRounded() {
        let middle = DockletGlowProfile.glowAlpha(inFromEdge: 3, along: 100, length: 200)
        let taperedEnd = DockletGlowProfile.glowAlpha(inFromEdge: 3, along: 11, length: 200)
        #expect(taperedEnd < middle)
        #expect(DockletGlowProfile.glowAlpha(inFromEdge: 0, along: 11, length: 200) > 0)
    }

    @Test("The cached glow alpha matches the blurred alpha within 0.002 at the sampled distances")
    func cachedLookupMatchesBlur() {
        for distance in [CGFloat(0), 1.5, 3, 6, 12] {
            let cached = DockletGlowProfile.glowAlpha(inFromEdge: distance, along: 100, length: 200)
            #expect(abs(cached - DockletGlowProfile.blurredAlpha(at: distance)) < 0.002)
        }
    }

    @Test("A glow shorter than twice the taper keeps a finite taper and is dark at both tips")
    func shortGlowStaysFinite() {
        for position in stride(from: CGFloat(0), through: 20, by: 1) {
            for distance in [CGFloat(0), 5, 12] {
                #expect(DockletGlowProfile.taperedDistance(inFromEdge: distance, along: position, length: 20).isFinite)
            }
        }
        #expect(DockletGlowProfile.glowAlpha(inFromEdge: 0, along: 0, length: 20) == 0)
        #expect(DockletGlowProfile.glowAlpha(inFromEdge: 0, along: 20, length: 20) == 0)
    }

    @Test("A tucked bar whose glow would start above 56 pt moves the glow top down to 56")
    func glowTopClampsToTopClearance() {
        let geometry = DockletGeometry(
            dockFrame: CGRect(x: 10, y: 100, width: 382, height: 62),
            bounds: screen
        )
        #expect(DockletDrawbridge(geometry: geometry).edgeGlowRect == CGRect(x: 378, y: 56, width: 24, height: 200))
    }

    @Test("On a short screen the glow shortens to keep 8 pt above the bottom")
    func glowShortensAboveBottomClearance() {
        let geometry = DockletGeometry(
            dockFrame: CGRect(x: 10, y: 600, width: 382, height: 62),
            bounds: CGRect(x: 0, y: 0, width: 402, height: 300)
        )
        #expect(DockletDrawbridge(geometry: geometry).edgeGlowRect == CGRect(x: 378, y: 235, width: 24, height: 57))
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
