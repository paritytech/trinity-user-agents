import CoreGraphics

/// Geometry and fixed visual values of the tab bar. The bar has one shape, so none of these are configurable.
public enum DSTabBarStyle {
    /// Height the bar occupies once it has settled. Hosts publish bottom clearance from it.
    public static let height: CGFloat = 60
    static let cornerRadius: CGFloat = 28
    static let itemInset: CGFloat = 6
    static let strokeWidth: CGFloat = 1

    /// Radius of the hole cut from the bar. The centre control sits inside it with a ring gap.
    static let notchRadius: CGFloat = 34
    /// Hole centre, measured down from the top of the bar row.
    static let notchCenterY: CGFloat = 16

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
