import SwiftUI

/// The bar's glass: the variable blur, under an elliptical wash lightest at the top centre in dark themes.
struct DSTabBarBarMaterial: View {
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        DSTabBarVariableBlur(intensity: isDark ? DSTabBarStyle.barBlurIntensity : DSTabBarStyle.barBlurIntensityLight)
            .overlay {
                if isDark {
                    wash
                }
            }
    }

    private var isDark: Bool { colorScheme == .dark }
}

/// The centre control's glass: the variable blur, under a vertical wash that settles to black as the panel opens
/// in dark themes.
struct DSTabBarCenterMaterial: View {
    /// 0 for the closed button, 1 for the open panel.
    let openBlend: CGFloat
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        DSTabBarVariableBlur(
            intensity: isDark ? DSTabBarStyle.centerBlurIntensity : DSTabBarStyle.centerBlurIntensityLight
        )
        .overlay {
            if isDark {
                LinearGradient(
                    colors: [DSTabBarStyle.Palette.glassLight, DSTabBarStyle.Palette.glassDark],
                    startPoint: .top,
                    endPoint: .bottom
                )
                .opacity(1 - openBlend)

                Rectangle()
                    .fill(DSTabBarStyle.Palette.centerOpenFill)
                    .opacity(openBlend)
            }
        }
    }

    private var isDark: Bool { colorScheme == .dark }
}

/// The scrim behind the bar while the centre panel is open. The caller fades it with the panel's progress.
public struct DSTabBarDimLayer: View {
    public init() {}

    public var body: some View {
        DSTabBarVariableBlur(intensity: DSTabBarStyle.dimBlurIntensity)
            .overlay(DSTabBarStyle.Palette.dimWash)
    }
}

private extension DSTabBarBarMaterial {
    /// The ellipse is centred on the top of the glass, bar and open shelf together, so the shelf reads as part
    /// of the bar. Its frame is as tall as the glass divided by the radius fraction, so the vertical radius spans
    /// the whole glass.
    var wash: some View {
        GeometryReader { proxy in
            Rectangle()
                .fill(
                    EllipticalGradient(
                        colors: [DSTabBarStyle.Palette.glassLight, DSTabBarStyle.Palette.glassDark],
                        center: .top,
                        startRadiusFraction: 0,
                        endRadiusFraction: DSTabBarStyle.barWashRadiusFraction
                    )
                )
                .frame(height: proxy.size.height / DSTabBarStyle.barWashRadiusFraction, alignment: .top)
        }
    }
}
