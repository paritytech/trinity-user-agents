import BigInt
import DesignSystem
import PolkadotUI
import Products
import SwiftUI

struct WidgetDesignTokenResolver: WidgetDesignTokenResolving {
    // MARK: - Color

    func color(for token: ScaleColorToken) -> Color {
        switch token {
        case .textPrimary:
            Color(.fgPrimary)
        case .textSecondary:
            Color(.fgSecondary)
        case .textTertiary:
            Color(.fgTertiary)
        case .backgroundPrimary:
            Color(.bgSurfaceMain)
        case .backgroundSecondary:
            Color(.bgSurfaceContainer)
        case .backgroundTertiary:
            Color(.bgSurfaceNested)
        case .success:
            Color(.brandGreen)
        case .error:
            Color(.fgError)
        case .warning:
            Color(.fgWarning)
        }
    }

    // MARK: - Typography

    func font(for style: ScaleTypographyStyle) -> Font {
        switch style {
        case .titleXL:
            Font(UIFont.headlineSmall)
        case .headline:
            Font(UIFont.titleLarge)
        case .bodyM:
            Font(UIFont.bodyMedium)
        case .bodyS:
            Font(UIFont.paragraphSmall)
        case .caption:
            Font(UIFont.labelSmall)
        }
    }

    func labelStyle(for style: ScaleTypographyStyle) -> (font: Font, lineSpacing: CGFloat) {
        switch style {
        case .titleXL:
            (Font(UIFont.headlineSmall), lineSpacing: 32.0 - UIFont.headlineSmall.lineHeight)
        case .headline:
            (Font(UIFont.titleLarge), lineSpacing: 22.0 - UIFont.titleLarge.lineHeight)
        case .bodyM:
            (Font(UIFont.bodyMedium), lineSpacing: 20.0 - UIFont.bodyMedium.lineHeight)
        case .bodyS:
            (Font(UIFont.paragraphSmall), lineSpacing: 18.0 - UIFont.paragraphSmall.lineHeight)
        case .caption:
            (Font(UIFont.labelSmall), lineSpacing: 16.0 - UIFont.labelSmall.lineHeight)
        }
    }

    // MARK: - Shape

    func shape(for scaleShape: ScaleShape) -> AnyShape {
        switch scaleShape {
        case let .rounded(radius):
            AnyShape(RoundedRectangle(cornerRadius: CGFloat(radius)))
        case .circle:
            AnyShape(Capsule())
        }
    }

    func cornerRadius(for scaleShape: ScaleShape) -> CGFloat {
        switch scaleShape {
        case let .rounded(radius):
            CGFloat(radius)
        case .circle:
            .greatestFiniteMagnitude
        }
    }

    // MARK: - Button

    func buttonStyle(for variant: ScaleButtonVariant) -> (background: Color, foreground: Color) {
        switch variant {
        case .primary:
            (background: Color(.bgActionPrimary), foreground: Color(.fgPrimaryInverted))
        case .secondary:
            (background: Color(.fill6), foreground: Color(.fgPrimary))
        case .text:
            (background: .clear, foreground: Color(.fgPrimary))
        }
    }
}
