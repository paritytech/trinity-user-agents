import Products
import SwiftUI

/// Design tokens are irrelevant to the mapping tests: they only need the
/// renderer tree to resolve, so every token collapses to a fixed value.
struct StubWidgetDesignTokenResolver: WidgetDesignTokenResolving {
    func color(for _: ScaleColorToken) -> Color { .clear }
    func font(for _: ScaleTypographyStyle) -> Font { .body }
    func labelStyle(for _: ScaleTypographyStyle) -> (font: Font, lineSpacing: CGFloat) {
        (.body, 0)
    }

    func shape(for _: ScaleShape) -> AnyShape { AnyShape(Rectangle()) }
    func cornerRadius(for _: ScaleShape) -> CGFloat { 0 }
    func buttonStyle(for _: ScaleButtonVariant) -> (background: Color, foreground: Color) {
        (.clear, .clear)
    }
}
