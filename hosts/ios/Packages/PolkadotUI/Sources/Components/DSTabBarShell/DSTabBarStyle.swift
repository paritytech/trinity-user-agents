import CoreGraphics

/// Geometry and fixed visual values of the tab bar. The bar has one shape, so none of these are configurable.
public enum DSTabBarStyle {
    /// Height of the bar row itself, not counting the gap below it.
    public static let height: CGFloat = 60

    /// Gap between the bar row and the bottom of the screen.
    public static let bottomGap: CGFloat = 21
    /// Inset of the bar row from each side of the screen.
    public static let horizontalMargin: CGFloat = 21
    /// Widest the bar row grows. Wider screens leave the extra width to either side.
    public static let maxWidth: CGFloat = 500

    /// Height the bar occupies once it has settled, which is what a host publishes as bottom
    /// clearance for its content.
    public static var restingHeight: CGFloat { height + bottomGap }
    static let cornerRadius: CGFloat = 28
    static let itemInset: CGFloat = 6
    static let strokeWidth: CGFloat = 1

    /// Radius of the hole cut from the bar. The centre control sits inside it with a ring gap.
    static let notchRadius: CGFloat = 34
    /// Hole centre, measured down from the top of the bar row.
    static let notchCenterY: CGFloat = 16

    /// Whether a point in the bar row's coordinate space falls on the centre button, which the
    /// fold gesture must not claim.
    public static func isInCenterHole(_ point: CGPoint, inRowOfWidth width: CGFloat) -> Bool {
        let center = CGPoint(x: width / 2, y: notchCenterY)
        return hypot(point.x - center.x, point.y - center.y) <= notchRadius
    }

    /// Strength of the variable blur. A material property, not a colour.
    static let blurIntensity: Double = 0.35

    /// Opacity of the contrast wash over the blur. Slight enough to read as a wash rather than
    /// a fill, so the bar stays translucent in every theme.
    static let tintOpacity: Double = 0.018

    /// Opacity of the rim at the top of the bar, where it reads as catching the light.
    static let rimTopOpacity: Double = 0.14
    /// Opacity of the rim at the foot, where it has all but faded out.
    static let rimBottomOpacity: Double = 0.035

    static let shadowRadius: CGFloat = 18
    /// Reach of the shadow mask past the bar, so the blurred shadow is not cut off at the bar edge.
    static let shadowMaskSpread: CGFloat = shadowRadius * 4 + 40

    /// Response of the spring that grows and collapses the shelf. Damping sits just under critical,
    /// so the bar settles with at most a slight overshoot.
    static let shelfSpringResponse: Double = 0.38
    static let shelfSpringDamping: Double = 0.85

    /// Response of the spring that settles the centre control open or closed. Public because a host
    /// driving the control from UIKit has to match the control's own settle.
    public static let centerSpringResponse: Double = 0.42
    public static let centerSpringDamping: Double = 0.82

    /// Length of the eased shelf reveal under Reduce Motion, growth and fade together. Matches
    /// the centre control's.
    static let shelfFadeDuration: Double = 0.2

    /// Diameter of the centre control at rest. It sits in the hole with a ring gap.
    static let centerButtonSize: CGFloat = 60
    /// Gap between the open centre panel and the bar's side edges.
    static let centerPanelMargin: CGFloat = 10
    static let centerPanelCornerRadius: CGFloat = 24
    /// Concave blend where the lobe meets the panel's bottom edge.
    static let centerPanelFillet: CGFloat = 18

    /// Width the open centre panel takes in a bar row of `width`, inset from both sides.
    public static func centerPanelWidth(inRowOfWidth width: CGFloat) -> CGFloat {
        max(centerButtonSize, width - 2 * centerPanelMargin)
    }

    /// Height the open centre panel takes for content of `contentHeight`, grabber and lobe included.
    /// The body's corner arcs need vertical room of their own, so the panel never drops below them.
    public static func centerPanelHeight(contentHeight: CGFloat) -> CGFloat {
        let lobeRadius = centerButtonSize / 2
        let minimumHeight = centerPanelCornerRadius * 2 + lobeRadius
        return max(minimumHeight, contentHeight + lobeRadius + centerGrabberHeight)
    }

    /// How far the open centre panel stands above the bar row. Its foot sits at the lobe's centre
    /// plus the lobe radius, measured down from the top of the row.
    public static func centerPanelRise(contentHeight: CGFloat) -> CGFloat {
        centerPanelHeight(contentHeight: contentHeight) - (notchCenterY + centerButtonSize / 2)
    }

    /// Strength of the top-lit sheen, which makes the centre control read as a dome.
    static let centerSheenStrength: Double = 0.35
    /// Opacity of the frost wash that separates the centre control from the bar.
    static let centerFrostOpacity: Double = 0.18
    /// Opacity of the centre control's rim at its top edge.
    static let centerRimTopOpacity: Double = 0.252
    /// Opacity of the centre control's rim at its foot, where it has faded out.
    static let centerRimBottomOpacity: Double = 0.042

    static let centerGlyphSize: CGFloat = 22
    /// Room the grabber takes at the top of the panel. Counted in the panel's height as well as
    /// padded, or it eats into the content.
    static let centerGrabberHeight: CGFloat = 26
    static let centerGrabberWidth: CGFloat = 36
    static let centerGrabberThickness: CGFloat = 5
    static let centerGrabberOpacity: Double = 0.35

    static let centerShadowRadius: CGFloat = 10
    static let centerShadowY: CGFloat = 4
}
