import CoreGraphics
import DesignSystem
import SwiftUI
import UIKit

/// Geometry and fixed visual values of the tab bar. The bar has one shape, so none of these are configurable.
public enum DSTabBarStyle {
    /// Height of the bar row itself, not counting the gap below it.
    public static let height: CGFloat = 62

    /// Inset of the bar row from each side of the screen.
    public static let horizontalMargin: CGFloat = 10
    /// Widest the bar row grows. Wider screens leave the extra width to either side.
    public static let maxWidth: CGFloat = 440

    /// Smallest gap between the bar row and the bottom of the screen.
    private static let minimumBottomGap: CGFloat = 12
    /// The bottom safe area inset already clears part of the screen, so the gap shrinks by this much of it.
    private static let bottomGapInsetReduction: CGFloat = 7

    /// Gap between the bar row and the bottom of the screen, given the bottom safe area inset.
    public static func bottomGap(bottomSafeAreaInset: CGFloat) -> CGFloat {
        max(minimumBottomGap, bottomSafeAreaInset - bottomGapInsetReduction)
    }

    /// Width of the bar row in a host `availableWidth` wide: inset by the margins and capped at `maxWidth`.
    public static func rowWidth(inAvailableWidth availableWidth: CGFloat) -> CGFloat {
        min(max(0, availableWidth - horizontalMargin * 2), maxWidth)
    }

    /// Height the bar occupies once it has settled, which is what a host publishes as bottom
    /// clearance for its content.
    public static func restingHeight(bottomSafeAreaInset: CGFloat) -> CGFloat {
        height + bottomGap(bottomSafeAreaInset: bottomSafeAreaInset)
    }

    static var cornerRadius: CGFloat { height / 2 }
    static let itemInset: CGFloat = 4
    /// Vertical inset of the selection pill inside the bar row.
    static let selectionInset: CGFloat = 4
    /// Width of the selection pill. It is centred on its slot and may overflow the slot.
    static let selectionWidth: CGFloat = 80
    /// Length of the ease that carries the selection pill between slots and recolours the slot icons.
    static let selectionDuration: Double = 0.238
    /// The CSS ease the selection pill moves on. Public so a host's slot icons change colour on the same curve.
    public static let selectionCurve: Animation = .timingCurve(0.25, 0.1, 0.25, 1, duration: selectionDuration)
    /// Corner radius of the selection pill drawn behind a shelf view.
    static let shelfSelectionCornerRadius: CGFloat = DSRadii.mediumIncreased
    static let strokeWidth: CGFloat = 1

    /// Gap between the hole's edge and the centre control sitting in it.
    static let notchRingGap: CGFloat = 2
    /// Radius of the hole cut from the bar: the centre control plus the ring gap around it.
    static var notchRadius: CGFloat { centerButtonSize / 2 + notchRingGap }
    /// Hole centre, measured down from the top of the bar row.
    static let notchCenterY: CGFloat = 22
    /// How far the hole reaches above the bar row, into the shelf.
    static var notchOverhang: CGFloat { notchRadius - notchCenterY }
    /// Gap the two inner slots leave around the notch. It is the cutout's width at the bar's top edge.
    static let notchShoulderWidth: CGFloat = 74

    /// Whether a point in the bar row's coordinate space falls on the centre button, which the
    /// fold gesture must not claim.
    public static func isInCenterHole(_ point: CGPoint, inRowOfWidth width: CGFloat) -> Bool {
        let center = CGPoint(x: width / 2, y: notchCenterY)
        return hypot(point.x - center.x, point.y - center.y) <= notchRadius
    }

    /// Strength of the bar's variable blur. The public API cannot set the backdrop blur radius or saturation,
    /// so this is tuned by eye toward the brief's radius.
    static let barBlurIntensity: Double = 0.35
    /// Strength of the centre control's variable blur, tuned by eye like `barBlurIntensity`.
    static let centerBlurIntensity: Double = 0.6
    /// `barBlurIntensity` for light themes, which frost with the white material and no grey wash.
    static let barBlurIntensityLight: Double = 0.5
    /// `centerBlurIntensity` for light themes, which frost with the white material and no grey wash.
    static let centerBlurIntensityLight: Double = 0.7
    /// Strength of the scrim behind the bar while the centre panel is open, tuned by eye like `barBlurIntensity`.
    static let dimBlurIntensity: Double = 0.3

    /// Horizontal radius of the bar's wash ellipse as a fraction of the bar's width. Its vertical radius is the bar
    /// height.
    static let barWashRadiusFraction: CGFloat = 0.45

    /// Blur the bar row reaches as the centre panel opens fully. The slot items and selection pill take it.
    static let openItemBlurRadius: CGFloat = 3
    /// Opacity the bar row reaches as the centre panel opens fully.
    static let openItemOpacity: Double = 0.55
    /// How far the bar row rises as the centre panel opens fully.
    static let openBarRise: CGFloat = 2

    /// Response of the spring that grows and collapses the shelf. Damping sits just under critical,
    /// so the bar settles with at most a slight overshoot. Public because a host driving the shelf
    /// from UIKit has to match its settle.
    public static let shelfSpringResponse: Double = 0.18
    public static let shelfSpringDamping: Double = 0.9

    /// Length of the ease that settles the centre control open or closed. Public because a host
    /// driving the control from UIKit has to match the control's own settle.
    public static let centerDuration: Double = 0.329
    /// Control points of the cubic ease-out `1 − (1 − t)³` that the centre control settles on.
    private static let easeOutControl1 = CGPoint(x: 0.33, y: 1)
    private static let easeOutControl2 = CGPoint(x: 0.68, y: 1)
    /// The centre control's settle at `centerDuration`.
    public static let centerEaseOut: Animation = centerCurve(duration: centerDuration)

    /// The centre control's ease-out over `duration`, for a move that runs longer or shorter than a settle.
    public static func centerCurve(duration: Double) -> Animation {
        .timingCurve(
            Double(easeOutControl1.x),
            Double(easeOutControl1.y),
            Double(easeOutControl2.x),
            Double(easeOutControl2.y),
            duration: duration
        )
    }

    /// Length of the eased shelf reveal under Reduce Motion, growth and fade together.
    static let shelfFadeDuration: Double = 0.2
    /// Length of the centre control's cross-fade under Reduce Motion, where its geometry snaps.
    static let centerReduceMotionFade: Double = 0.22
    /// Share of a close's duration after which centre content starts to fade out, while the shape keeps moving.
    static let contentCloseFadeStart: Double = 0.3
    /// Share of a close's duration the centre content's fade out takes.
    static let contentCloseFadeLength: Double = 0.35
    /// How long a settle waits after the shape starts to open before centre content fades in.
    static let contentRevealDelay: Double = 0.224
    /// Length of the centre content's fade in.
    static let contentRevealDuration: Double = 0.141

    /// Diameter of the centre control at rest. It sits in the hole with a ring gap.
    static let centerButtonSize: CGFloat = 64
    static let centerPanelCornerRadius: CGFloat = 32
    /// Depth of the lobe hanging below the panel's body. It holds the centre glyph.
    static let centerLobeDepth: CGFloat = 31
    /// Half the lobe's width once the panel is 88 pt wide. Narrower panels scale it down with their width.
    static let centerLobeHalfWidth: CGFloat = 44

    /// Width the open centre panel takes in a bar row of `width`: the full width of the bar.
    public static func centerPanelWidth(inRowOfWidth width: CGFloat) -> CGFloat {
        max(centerButtonSize, width)
    }

    /// Height the open centre panel takes for content of `contentHeight`, lobe included. The grabber
    /// floats over the content, so the caller provides its clearance inside `contentHeight`.
    /// The body's corner arcs need vertical room of their own, so the panel never drops below them.
    public static func centerPanelHeight(contentHeight: CGFloat) -> CGFloat {
        let minimumHeight = centerPanelCornerRadius * 2 + centerLobeDepth
        return max(minimumHeight, contentHeight + centerLobeDepth)
    }

    /// Foot of the centre button, measured down from the top of the bar row. The open panel's lobe keeps it.
    public static var centerButtonFootFromRowTop: CGFloat { notchCenterY + centerButtonSize / 2 }

    /// How far the resting centre button stands above the bar row. A host's touch region has to reach this
    /// high, or the top of the button passes touches through.
    public static var centerButtonRestingRise: CGFloat { centerButtonSize / 2 - notchCenterY }

    /// How far the open centre panel stands above the bar row.
    public static func centerPanelRise(contentHeight: CGFloat) -> CGFloat {
        centerPanelHeight(contentHeight: contentHeight) - centerButtonFootFromRowTop
    }

    /// The tallest content a centre panel can host without standing more than `rise` above the bar row.
    /// The inverse of `centerPanelRise(contentHeight:)`, for a host that has to cap the panel.
    public static func centerContentHeight(fittingRise rise: CGFloat) -> CGFloat {
        max(0, rise + centerButtonFootFromRowTop - centerButtonSize / 2)
    }

    /// Runs `changes` on the centre control's ease-out and lays out `view` inside the same animation, so UIKit
    /// moves whatever `changes` touches in step with the control. Snaps under Reduce Motion.
    public static func animateCenterChange(in view: UIView, _ changes: @escaping () -> Void) {
        guard !UIAccessibility.isReduceMotionEnabled else {
            changes()
            return
        }
        let animator = UIViewPropertyAnimator(
            duration: centerDuration,
            controlPoint1: easeOutControl1,
            controlPoint2: easeOutControl2
        ) {
            changes()
            view.layoutIfNeeded()
        }
        animator.startAnimation()
    }

    /// Centre control progress at which its fill starts to blend from the closed button to the open panel.
    static let centerBlendStart: CGFloat = 0.2
    /// Centre control progress at which the blend from the closed button to the open panel completes.
    static let centerBlendEnd: CGFloat = 0.7

    /// Distance the grabber sits below the top of the open panel.
    static let centerGrabberTopInset: CGFloat = 6
    static let centerGrabberWidth: CGFloat = 36
    static let centerGrabberThickness: CGFloat = 5

    /// Distance the camera sits below the top of the open scan panel with search unfocused.
    public static let scanCameraTopInset: CGFloat = 19
    /// Side of the square camera in the open scan panel with search unfocused.
    public static let scanCameraSide: CGFloat = 110
    /// Gap from the camera's bottom edge to the search field's top edge.
    public static let scanFieldTopGap: CGFloat = 9
    /// Height of the search field in the open scan panel.
    public static let scanFieldHeight: CGFloat = 48
    /// Gap from the search field's bottom edge to the bottom of the panel body.
    public static let scanFieldBottomInset: CGFloat = 18

    /// Content height of the open scan panel with search unfocused. The same at every panel width.
    public static var scanStateHeight: CGFloat {
        scanCameraTopInset + scanCameraSide + scanFieldTopGap + scanFieldHeight + scanFieldBottomInset
    }
}
