import CoreGraphics
import Foundation

/// The Edge glow's alpha across its band: blurred against the screen edge, and tapered into rounded ends along its
/// length. Pure maths, so it reads no UIKit state.
enum DockletGlowProfile {
    /// Width of the band in points, measured in from the screen edge.
    static let bandWidth: CGFloat = 24
    /// Length of the rounded taper at each end of the glow.
    static let endRadius: CGFloat = 22
    /// Weight and decay length, in points, of the sharp part of the profile.
    private static let sharpWeight: CGFloat = 0.5
    private static let sharpDecay: CGFloat = 4.2
    /// Weight and decay length, in points, of the soft part of the profile.
    private static let softWeight: CGFloat = 0.06
    private static let softDecay: CGFloat = 14
    /// Standard deviation, in points, of the Gaussian that blurs the profile.
    private static let blurSigma: CGFloat = 2.4
    /// Step, in points, at which the blur integrates the profile.
    private static let integrationStep: CGFloat = 0.05
    /// Distance, in points, between the cached blurred samples that `glowAlpha` interpolates between.
    private static let lookupStep: CGFloat = 0.25
    /// The blurred alpha at every `lookupStep` across the band, computed once.
    private static let blurredLookup: [CGFloat] = stride(from: 0, through: bandWidth, by: lookupStep).map {
        DockletGlowProfile.blurredAlpha(at: $0)
    }

    /// The unblurred alpha `distance` points in from the screen edge.
    static func alpha(at distance: CGFloat) -> CGFloat {
        sharpWeight * exp(-distance / sharpDecay)
            + softWeight * exp(-distance / softDecay) * (1 - distance / bandWidth)
    }

    /// The alpha `distance` points in from the screen edge after the blur. Light outside the band is transparent, so
    /// the edge is dimmer than the band's peak, which sits a few points inside it.
    static func blurredAlpha(at distance: CGFloat) -> CGFloat {
        stride(from: 0, through: bandWidth, by: integrationStep).reduce(0) { total, offset in
            total + alpha(at: offset) * gaussian(distance - offset) * integrationStep
        }
    }

    /// Distance in points from the screen edge to the glow's boundary at `position` down a glow of `length`. Within
    /// `endRadius` of either end, distance runs along a quarter ellipse, so the band narrows to nothing at its tip and
    /// its ends are rounded.
    static func taperedDistance(inFromEdge distance: CGFloat, along position: CGFloat, length: CGFloat) -> CGFloat {
        let fromNearerEnd = max(0, min(position, length - position))
        let radius = min(endRadius, length / 2)
        guard radius > 0 else {
            return bandWidth
        }
        guard fromNearerEnd < radius else {
            return distance
        }
        let inset = bandWidth * (radius - fromNearerEnd) / radius
        return hypot(distance, inset)
    }

    /// The glow's alpha `distance` points in from the screen edge, `position` points down a glow of `length`. It is the
    /// blurred alpha at the tapered distance, so it is 0 once that distance reaches `bandWidth`.
    static func glowAlpha(inFromEdge distance: CGFloat, along position: CGFloat, length: CGFloat) -> CGFloat {
        let tapered = taperedDistance(inFromEdge: distance, along: position, length: length)
        guard tapered < bandWidth else {
            return 0
        }
        let index = tapered / lookupStep
        let lower = Int(index)
        let fraction = index - CGFloat(lower)
        return blurredLookup[lower] + (blurredLookup[lower + 1] - blurredLookup[lower]) * fraction
    }

    /// The shimmer's strength at the same point as `glowAlpha`: full at the screen edge, fading linearly to 0 at the
    /// band's inner side.
    static func shimmerStrength(inFromEdge distance: CGFloat, along position: CGFloat, length: CGFloat) -> CGFloat {
        max(0, 1 - taperedDistance(inFromEdge: distance, along: position, length: length) / bandWidth)
    }
}

private extension DockletGlowProfile {
    static func gaussian(_ offset: CGFloat) -> CGFloat {
        exp(-offset * offset / (2 * blurSigma * blurSigma)) / (blurSigma * (2 * .pi).squareRoot())
    }
}
