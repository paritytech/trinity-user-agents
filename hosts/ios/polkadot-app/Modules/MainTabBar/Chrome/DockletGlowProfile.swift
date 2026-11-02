import CoreGraphics
import Foundation

/// The Edge glow's alpha across its band, blurred for the stops a gradient draws and feathered along its length.
/// Pure maths, so it reads no UIKit state.
enum DockletGlowProfile {
    /// Width of the band in points, measured in from the screen edge.
    static let bandWidth: CGFloat = 24
    /// Length of the smoothstep fade at each end of the glow.
    static let featherLength: CGFloat = 22
    /// Weight and decay length, in points, of the sharp part of the profile.
    private static let sharpWeight: CGFloat = 0.5
    private static let sharpDecay: CGFloat = 4.2
    /// Weight and decay length, in points, of the soft part of the profile.
    private static let softWeight: CGFloat = 0.06
    private static let softDecay: CGFloat = 14
    /// Standard deviation, in points, of the Gaussian that blurs the profile.
    private static let blurSigma: CGFloat = 2.4
    /// Distance, in points, between blurred stops.
    private static let stopSpacing: CGFloat = 1.5
    /// Step, in points, at which the blur integrates the profile.
    private static let integrationStep: CGFloat = 0.05
    /// Intervals between feather stops along the glow.
    private static let featherIntervals = 32

    /// A gradient stop. `location` runs from 0 to 1 and `alpha` is the glow's opacity there.
    struct Stop: Equatable {
        let location: CGFloat
        let alpha: CGFloat
    }

    /// The unblurred alpha `distance` points in from the screen edge.
    static func alpha(at distance: CGFloat) -> CGFloat {
        sharpWeight * exp(-distance / sharpDecay)
            + softWeight * exp(-distance / softDecay) * (1 - distance / bandWidth)
    }

    /// The alpha `distance` points in from the screen edge after the blur. Light continues past the edge, so the
    /// profile mirrors at the edge, and nothing lies beyond the band's inner side.
    static func blurredAlpha(at distance: CGFloat) -> CGFloat {
        stride(from: -bandWidth, through: bandWidth, by: integrationStep).reduce(0) { total, offset in
            total + alpha(at: abs(offset)) * gaussian(distance - offset) * integrationStep
        }
    }

    /// Stops for the blurred band, one every `stopSpacing` points from the screen edge inward.
    static func blurredStops() -> [Stop] {
        let intervals = Int(bandWidth / stopSpacing)
        return (0 ... intervals).map { index in
            Stop(
                location: CGFloat(index) / CGFloat(intervals),
                alpha: blurredAlpha(at: CGFloat(index) * stopSpacing)
            )
        }
    }

    /// The share of the alpha kept `offset` points down a glow of `length`, fading in at the top and out at the bottom.
    static func feather(at offset: CGFloat, length: CGFloat) -> CGFloat {
        smoothstep(offset / featherLength) * smoothstep((length - offset) / featherLength)
    }

    /// Stops for the feather along a glow of `length`, evenly spaced from its top to its bottom.
    static func featherStops(length: CGFloat) -> [Stop] {
        (0 ... featherIntervals).map { index in
            let location = CGFloat(index) / CGFloat(featherIntervals)
            return Stop(location: location, alpha: feather(at: location * length, length: length))
        }
    }
}

private extension DockletGlowProfile {
    static func gaussian(_ offset: CGFloat) -> CGFloat {
        exp(-offset * offset / (2 * blurSigma * blurSigma)) / (blurSigma * (2 * .pi).squareRoot())
    }

    static func smoothstep(_ fraction: CGFloat) -> CGFloat {
        let clamped = min(max(fraction, 0), 1)
        return clamped * clamped * (3 - 2 * clamped)
    }
}
