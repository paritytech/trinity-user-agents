import SwiftUI

public protocol WidgetDesignTokenResolving {
    func color(for token: ScaleColorToken) -> Color
    func font(for style: ScaleTypographyStyle) -> Font
    func labelStyle(for style: ScaleTypographyStyle) -> (font: Font, lineSpacing: CGFloat)
    func shape(for scaleShape: ScaleShape) -> AnyShape
    func cornerRadius(for scaleShape: ScaleShape) -> CGFloat
    func buttonStyle(for variant: ScaleButtonVariant) -> (background: Color, foreground: Color)
}
