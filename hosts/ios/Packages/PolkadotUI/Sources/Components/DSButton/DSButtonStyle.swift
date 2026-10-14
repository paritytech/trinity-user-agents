import DesignSystem
import SwiftUI

// Text button matching Figma "Button v2". Shares the Style/Shape/Size axes with
// DSIconButtonStyle so the two siblings stay in lockstep.
public struct DSButtonStyle: ButtonStyle {
    public enum Style {
        case primary
        case secondary
        case tertiary
        case destructive
        case ghost
    }

    public enum Shape {
        case rounded
        case pill
    }

    public enum Size {
        case largeIncreased
        case large
        case mediumIncreased
        case medium
    }

    let style: Style
    let shape: Shape
    let size: Size

    public init(style: Style, shape: Shape, size: Size) {
        self.style = style
        self.shape = shape
        self.size = size
    }

    @Environment(\.isEnabled) private var isEnabled

    public func makeBody(configuration: Configuration) -> some View {
        DSButtonLabel(
            style: style,
            shape: shape,
            size: size,
            isEnabled: isEnabled,
            isPressed: configuration.isPressed,
            label: configuration.label
        )
    }
}

// MARK: - Body

private struct DSButtonLabel<Label: View>: View {
    let style: DSButtonStyle.Style
    let shape: DSButtonStyle.Shape
    let size: DSButtonStyle.Size
    let isEnabled: Bool
    let isPressed: Bool
    let label: Label

    var body: some View {
        label
            .typography(size.typography)
            .foregroundStyle(isEnabled ? style.foregroundColor : Color.fgDisabled)
            .padding(.vertical, size.verticalPadding)
            .padding(.horizontal, size.horizontalPadding)
            .background(fillColor, in: clipShape)
            .contentShape(clipShape)
            .scaleEffect(isPressed ? 0.98 : 1)
            .opacity(isPressed ? 0.85 : 1)
            .animation(.easeInOut(duration: 0.15), value: isPressed)
    }

    private var clipShape: AnyShape {
        shape.clipShape
    }

    private var fillColor: Color {
        isEnabled ? style.backgroundColor : style.disabledBackgroundColor
    }
}

// MARK: - API

public extension ButtonStyle where Self == DSButtonStyle {
    static func ds(
        style: DSButtonStyle.Style,
        shape: DSButtonStyle.Shape,
        size: DSButtonStyle.Size
    ) -> DSButtonStyle {
        DSButtonStyle(style: style, shape: shape, size: size)
    }
}

// MARK: - Style tokens

extension DSButtonStyle.Style {
    var backgroundColor: Color {
        switch self {
        case .primary: .bgActionPrimary
        case .secondary: .bgActionSecondary
        case .tertiary: .bgActionTertiary
        case .destructive: .bgStatusError
        case .ghost: .clear
        }
    }

    var disabledBackgroundColor: Color {
        switch self {
        case .ghost: .clear
        default: .bgActionDisabled
        }
    }

    var foregroundColor: Color {
        switch self {
        case .primary: .fgPrimaryInverted
        case .destructive: .fgStaticWhite
        default: .fgPrimary
        }
    }
}

// MARK: - Shape tokens

extension DSButtonStyle.Shape {
    var clipShape: AnyShape {
        switch self {
        case .pill:
            AnyShape(Capsule(style: .continuous))
        case .rounded:
            AnyShape(RoundedRectangle(cornerRadius: DSRadii.extraMedium, style: .continuous))
        }
    }
}

// MARK: - Size tokens

public extension DSButtonStyle.Size {
    var height: CGFloat {
        switch self {
        case .largeIncreased: 56
        case .large: 52
        case .mediumIncreased: 48
        case .medium: 44
        }
    }
}

public extension DSButtonStyle.Size {
    var verticalPadding: CGFloat {
        switch self {
        case .largeIncreased: DSSpacings.mediumIncreased
        case .large: DSSpacings.medium
        case .mediumIncreased: DSSpacings.extraMedium
        case .medium: DSSpacings.smallIncreased
        }
    }

    var horizontalPadding: CGFloat {
        DSSpacings.mediumIncreased
    }

    var typography: TypographyStyle {
        switch self {
        case .largeIncreased: .titleLarge
        case .large,
             .mediumIncreased,
             .medium: .titleMedium.emphasized
        }
    }

    var iconSize: CGFloat {
        switch self {
        case .largeIncreased: 20
        case .large,
             .mediumIncreased,
             .medium: 16
        }
    }
}
