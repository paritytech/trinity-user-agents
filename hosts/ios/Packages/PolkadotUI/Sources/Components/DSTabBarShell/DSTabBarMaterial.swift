import SwiftUI
import DesignSystem

/// The bar's glass: before iOS 26, the variable blur under `DSTabBarStyle.Palette.glassFill`; on iOS 26 and
/// later, `DSTabBarLiquidGlass` in the bar's outline without the notch. The container's clip cuts the notch.
/// Under a rotated ancestor Liquid Glass evaluates its shape in an enlarged rect, so a notch in the glass shape
/// would land in the wrong place.
struct DSTabBarBarMaterial: View {
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        if #available(iOS 26, *) {
            DSTabBarLiquidGlass(shape: RoundedRectangle(cornerRadius: DSTabBarStyle.cornerRadius, style: .continuous))
        } else {
            DSTabBarVariableBlur(
                intensity: isDark ? DSTabBarStyle.barBlurIntensity : DSTabBarStyle.barBlurIntensityLight
            )
            .overlay { DSTabBarStyle.Palette.glassFill }
        }
    }

    private var isDark: Bool { colorScheme == .dark }
}

/// The centre control's glass: before iOS 26, the variable blur under `DSTabBarStyle.Palette.glassFill`, which
/// gives way to the open fill in dark themes as the panel opens; on iOS 26 and later, `DSTabBarLiquidGlass` in
/// the morph shape.
struct DSTabBarCenterMaterial: View {
    /// 0 for the closed button, 1 for the open panel.
    let openBlend: CGFloat
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        if #available(iOS 26, *) {
            DSTabBarLiquidGlass(shape: DSTabBarMorphShape())
        } else {
            DSTabBarVariableBlur(
                intensity: isDark ? DSTabBarStyle.centerBlurIntensity : DSTabBarStyle.centerBlurIntensityLight
            )
            .overlay {
                DSTabBarStyle.Palette.glassFill.opacity(1 - openBlend)

                if isDark {
                    Rectangle()
                        .fill(DSTabBarStyle.Palette.centerOpenFill)
                        .opacity(openBlend)
                }
            }
        }
    }

    private var isDark: Bool { colorScheme == .dark }
}

/// The tab bar's Liquid Glass on iOS 26 and later: frost under Liquid Glass tinted with the theme's main
/// surface, dimmed by `DSTabBarStyle.glassDimming`. The bar and the centre control both draw it, each in its
/// own shape, so they read as one surface.
@available(iOS 26, *)
private struct DSTabBarLiquidGlass<GlassShape: Shape>: View {
    let shape: GlassShape
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        ZStack {
            DSTabBarVariableBlur(
                intensity: colorScheme == .dark
                    ? DSTabBarStyle.glassFrostIntensity
                    : DSTabBarStyle.glassFrostIntensityLight
            )
            Color.clear.glassEffect(
                .regular.tint(Color.bgSurfaceMain.opacity(DSTabBarStyle.glassTintOpacity)),
                in: shape
            )
            Color.black.opacity(DSTabBarStyle.glassDimming)
        }
    }
}

/// The scrim behind the bar while the centre panel is open. The caller fades it with the panel's progress.
public struct DSTabBarDimLayer: View {
    public init() {}

    public var body: some View {
        DSTabBarVariableBlur(intensity: blurIntensity)
            .overlay(DSTabBarStyle.Palette.dimWash)
    }

    private var blurIntensity: Double {
        if #available(iOS 26, *) {
            DSTabBarStyle.dimBlurIntensityGlass
        } else {
            DSTabBarStyle.dimBlurIntensity
        }
    }
}
