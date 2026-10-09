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
    /// Hole centre, measured down from the top edge of the bar.
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
}
