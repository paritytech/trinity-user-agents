import DesignSystem
import SwiftUI

public struct DSIconButtonStyle: ButtonStyle {
    public enum Style {
        case primary
        case secondary
        case tertiary
        case destructive
        case success
        case ghost
    }

    public enum Shape {
        case rounded
        case pill
    }

    public enum Size {
        case extraLarge
        case mediumIncreased
        case medium
        case small
        case extraSmall
        case tiny
        case extraTiny
    }

    let style: Style
    let shape: Shape
    let size: Size
    let glass: Bool

    public init(style: Style, shape: Shape, size: Size, glass: Bool = false) {
        self.style = style
        self.shape = shape
        self.size = size
        self.glass = glass
    }

    @Environment(\.isEnabled) private var isEnabled

    public func makeBody(configuration: Configuration) -> some View {
        DSIconButtonLabel(
            style: style,
            shape: shape,
            size: size,
            glass: glass,
            isEnabled: isEnabled,
            isPressed: configuration.isPressed,
            label: configuration.label
        )
    }
}

// MARK: - Body

// When `glass` is set, adopts the system interactive glass on iOS 26 (the OS drives
// the press). Otherwise keeps the style fill: a springy scale on iOS 26, the legacy
// scale+fade below. Glass is opt-in — don't enable it on a button placed over other
// glass, since glass-on-glass is unsupported.
private struct DSIconButtonLabel<Label: View>: View {
    let style: DSIconButtonStyle.Style
    let shape: DSIconButtonStyle.Shape
    let size: DSIconButtonStyle.Size
    let glass: Bool
    let isEnabled: Bool
    let isPressed: Bool
    let label: Label

    var body: some View {
        if #available(iOS 26.0, *) {
            glassBody
        } else {
            legacyBody
        }
    }

    @available(iOS 26.0, *)
    @ViewBuilder
    private var glassBody: some View {
        if glass, isEnabled {
            icon
                .glassEffect(.regular.interactive(), in: clipShape)
                .contentShape(clipShape)
        } else {
            icon
                .background(fillColor, in: clipShape)
                .scaleEffect(isPressed ? 0.96 : 1)
                .animation(.spring(response: 0.3, dampingFraction: 0.6), value: isPressed)
        }
    }

    private var legacyBody: some View {
        icon
            .background(fillColor, in: clipShape)
            .scaleEffect(isPressed ? 0.92 : 1)
            .opacity(isPressed ? 0.7 : 1)
            .animation(.easeInOut(duration: 0.15), value: isPressed)
    }

    private var icon: some View {
        label
            .foregroundStyle(isEnabled ? style.iconColor : Color.fgDisabled)
            .frame(width: size.iconSize, height: size.iconSize)
            .frame(width: size.dimension, height: size.dimension)
    }

    private var clipShape: AnyShape {
        shape.clipShape(for: size)
    }

    private var fillColor: Color {
        isEnabled ? style.backgroundColor : style.disabledBackgroundColor
    }
}

// MARK: - API

public extension ButtonStyle where Self == DSIconButtonStyle {
    static func dsIcon(
        style: DSIconButtonStyle.Style,
        shape: DSIconButtonStyle.Shape,
        size: DSIconButtonStyle.Size,
        glass: Bool = false
    ) -> DSIconButtonStyle {
        DSIconButtonStyle(style: style, shape: shape, size: size, glass: glass)
    }
}

// MARK: - Style tokens

extension DSIconButtonStyle.Style {
    var backgroundColor: Color {
        switch self {
        case .primary: .bgActionPrimary
        case .secondary: .bgActionSecondary
        case .tertiary: .bgActionTertiary
        case .destructive: .bgStatusError
        case .success: .bgStatusSuccess
        case .ghost: .clear
        }
    }

    var disabledBackgroundColor: Color {
        switch self {
        case .ghost: .clear
        default: .bgActionDisabled
        }
    }

    var iconColor: Color {
        switch self {
        case .primary: .fgPrimaryInverted
        case .destructive,
             .success: .fgStaticWhite
        default: .fgPrimary
        }
    }
}

// MARK: - Shape tokens

extension DSIconButtonStyle.Shape {
    func clipShape(for size: DSIconButtonStyle.Size) -> AnyShape {
        switch self {
        case .pill:
            AnyShape(Capsule(style: .continuous))
        case .rounded:
            AnyShape(RoundedRectangle(
                cornerRadius: min(size.roundedCornerRadius, size.dimension / 2),
                style: .continuous
            ))
        }
    }
}

// MARK: - Size tokens

extension DSIconButtonStyle.Size {
    var dimension: CGFloat {
        switch self {
        case .extraLarge: 64
        case .mediumIncreased: 48
        case .medium: 44
        case .small: 36
        case .extraSmall: 32
        case .tiny: 24
        case .extraTiny: 20
        }
    }

    var iconSize: CGFloat {
        switch self {
        case .extraLarge: 32
        case .mediumIncreased: 24
        case .medium: 24
        case .small: 20
        case .extraSmall: 20
        case .tiny: 16
        case .extraTiny: 16
        }
    }

    var roundedCornerRadius: CGFloat {
        switch self {
        case .extraLarge: DSRadii.mediumIncreased
        case .mediumIncreased,
             .medium,
             .small: DSRadii.extraMedium
        case .extraSmall,
             .tiny,
             .extraTiny: DSRadii.smallIncreased
        }
    }
}
