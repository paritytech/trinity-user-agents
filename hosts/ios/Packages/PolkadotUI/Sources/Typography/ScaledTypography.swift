import DesignSystem
import SwiftUI
import UIKit

public extension View {
    /// The typography of a design-system style at the text size that the user picks (Dynamic Type). The style
    /// gives the size at the default text size, and the size grows and shrinks as the body text style does.
    func scaledTypography(_ style: TypographyStyle) -> some View {
        modifier(ScaledTypographyModifier(style: style))
    }
}

public enum ScaledTypography {
    /// The font of `style` at `dynamicTypeSize`.
    public static func font(for style: TypographyStyle, dynamicTypeSize: DynamicTypeSize) -> UIFont {
        let spec = style.resolvedSpec
        let base = TypographyFamiliesRegistry.makeFamily(UITraitCollection.current.appTypography)
            .font(family: spec.family, weight: spec.weight, size: spec.size)
        return UIFontMetrics(forTextStyle: .body).scaledFont(for: base, compatibleWith: traits(dynamicTypeSize))
    }

    /// The line height of `style` at `dynamicTypeSize`.
    public static func lineHeight(for style: TypographyStyle, dynamicTypeSize: DynamicTypeSize) -> CGFloat {
        UIFontMetrics(forTextStyle: .body).scaledValue(
            for: style.resolvedSpec.lineHeight,
            compatibleWith: traits(dynamicTypeSize)
        )
    }

    private static func traits(_ dynamicTypeSize: DynamicTypeSize) -> UITraitCollection {
        UITraitCollection(preferredContentSizeCategory: UIContentSizeCategory(dynamicTypeSize))
    }
}

private struct ScaledTypographyModifier: ViewModifier {
    let style: TypographyStyle

    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    func body(content: Content) -> some View {
        let font = ScaledTypography.font(for: style, dynamicTypeSize: dynamicTypeSize)
        let lineHeight = ScaledTypography.lineHeight(for: style, dynamicTypeSize: dynamicTypeSize)
        let lineSpacing = max(0, lineHeight - font.lineHeight)

        return content
            .font(Font(font))
            .tracking(style.resolvedSpec.tracking)
            .lineSpacing(lineSpacing)
            .padding(.vertical, lineSpacing / 2)
    }
}
