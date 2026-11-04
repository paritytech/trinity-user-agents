import CoreGraphics
import Foundation

/// The Drawbridge Path's measurements: the bar turns about a pivot just past its trailing end and slides out past the
/// right edge, leaving its Edge glow and Grab zone on that edge.
struct DockletDrawbridge {
    /// Distance the pivot sits past the bar's trailing end.
    private static let pivotGap: CGFloat = 14
    /// Distance the bar slides past the screen edge beyond its full tuck.
    private static let extraSlide: CGFloat = 60
    /// Width of the Edge glow strip on the trailing edge.
    private static let edgeGlowWidth: CGFloat = 24
    /// Width of the Grab zone on the trailing edge, which is wider than the glow so it is easier to catch.
    private static let grabZoneWidth: CGFloat = 47
    /// Height of the Edge glow and the Grab zone.
    private static let edgeZoneLength: CGFloat = 200
    /// Shortest drag that takes the bar from shown to tucked.
    private static let minimumHideTravel: CGFloat = 160
    /// Share of the bar's width that a full hide drag takes, before the minimum applies.
    private static let hideTravelShare: CGFloat = 0.75
    /// Multiple of the room between a drag's start and the trailing edge that a hide drag may take.
    private static let edgeTravelMultiple: CGFloat = 1.2
    /// Shortest drag a drag that starts near the trailing edge may take to hide the bar.
    private static let minimumEdgeTravel: CGFloat = 60
    /// Least distance the Edge glow and Grab zone keep below the top of the screen.
    private static let zoneTopClearance: CGFloat = 56
    /// Least distance the Edge glow and Grab zone keep above the bottom of the screen.
    private static let zoneBottomClearance: CGFloat = 8
    /// Rotation of the bar about the pivot when fully tucked, with the leading end going up.
    static let quarterTurn: CGFloat = .pi / 2
    /// Exponent of the cubic ease that drives the slide out.
    private static let slideExponent: CGFloat = 3

    /// The Docklet's resting frame and the chrome's bounds.
    let geometry: DockletGeometry

    private var dockFrame: CGRect {
        geometry.dockFrame
    }

    private var bounds: CGRect {
        geometry.bounds
    }

    /// The point the bar turns about, just past its trailing end.
    var pivot: CGPoint {
        CGPoint(x: dockFrame.maxX + Self.pivotGap, y: dockFrame.midY)
    }

    /// How far the bar travels past the trailing edge when it is fully tucked.
    var slideOut: CGFloat {
        bounds.width - dockFrame.maxX + Self.extraSlide
    }

    /// The Edge glow strip on the trailing edge, at the tucked bar's top.
    var edgeGlowRect: CGRect {
        CGRect(x: bounds.width - Self.edgeGlowWidth, y: glowTop, width: Self.edgeGlowWidth, height: zoneLength)
    }

    /// The Grab zone on the trailing edge, at the tucked bar's top.
    var grabZoneRect: CGRect {
        CGRect(x: bounds.width - Self.grabZoneWidth, y: glowTop, width: Self.grabZoneWidth, height: zoneLength)
    }

    /// The drag distance that takes the bar from shown to tucked, for a drag that starts at `touchX`. A drag that
    /// starts near the trailing edge needs less travel.
    func hideTravel(touchX: CGFloat) -> CGFloat {
        min(
            max(Self.minimumHideTravel, Self.hideTravelShare * dockFrame.width),
            max(Self.minimumEdgeTravel, Self.edgeTravelMultiple * (bounds.width - touchX))
        )
    }

    /// The pose at `progress` without a finger: a quarter turn about the pivot, then the slide out.
    func plainPose(progress: CGFloat) -> DockletPose {
        DockletPose(
            angle: Self.quarterTurn * progress,
            offset: CGVector(dx: slideOut * pow(max(0, progress), Self.slideExponent), dy: 0)
        )
    }

    /// The transform that maps the bar's resting frame to the screen for `pose`.
    func absolute(_ pose: DockletPose) -> CGAffineTransform {
        let hinge = pivot
        return CGAffineTransform(translationX: -hinge.x, y: -hinge.y)
            .concatenating(CGAffineTransform(rotationAngle: pose.angle))
            .concatenating(CGAffineTransform(translationX: hinge.x + pose.offset.dx, y: hinge.y + pose.offset.dy))
    }

    /// The lowest y of the bar's resting corners once `pose` is applied.
    func lowestY(_ pose: DockletPose) -> CGFloat {
        let mapping = absolute(pose)
        return [dockFrame.minX, dockFrame.maxX].reduce(-CGFloat.infinity) { lowest, cornerX in
            [dockFrame.minY, dockFrame.maxY].reduce(lowest) {
                max($0, CGPoint(x: cornerX, y: $1).applying(mapping).y)
            }
        }
    }

    /// The transform for a view whose anchor point is `anchor`, posed as `pose`.
    func transform(_ pose: DockletPose, anchor: CGPoint) -> CGAffineTransform {
        geometry.transform(mapping: absolute(pose), anchor: anchor)
    }

    /// The transform for a view whose anchor point is `anchor`, posed at `progress`. Shown is the identity. Tucked
    /// is a quarter turn about the pivot with the leading end going up, then the slide out.
    func transform(progress: CGFloat, anchor: CGPoint) -> CGAffineTransform {
        transform(plainPose(progress: progress), anchor: anchor)
    }

    /// The tucked bar's top, kept at least `zoneTopClearance` below the screen top.
    var glowTop: CGFloat {
        max(Self.zoneTopClearance, pivot.y - (pivot.x - dockFrame.minX))
    }

    /// `edgeZoneLength`, shortened only when the glow would reach below `zoneBottomClearance` above the screen bottom.
    private var zoneLength: CGFloat {
        min(Self.edgeZoneLength, max(0, bounds.height - Self.zoneBottomClearance - glowTop))
    }
}
