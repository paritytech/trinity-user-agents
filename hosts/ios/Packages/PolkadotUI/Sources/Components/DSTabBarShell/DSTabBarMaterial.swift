import DesignSystem
import SwiftUI

/// The bar's blurred chrome: the variable blur under a contrast wash. The wash is the theme's
/// own foreground colour, so it lightens the bar on dark themes and darkens it on light ones.
struct DSTabBarMaterial: View {
    var body: some View {
        DSTabBarVariableBlur(intensity: DSTabBarStyle.blurIntensity)
            .overlay(Color.fgPrimary.opacity(DSTabBarStyle.tintOpacity))
    }
}
