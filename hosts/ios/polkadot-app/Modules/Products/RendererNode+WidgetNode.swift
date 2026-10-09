import BigInt
import TrUAPIHost
import PolkadotUI
import Products
import SwiftUI

extension RendererNode {
    func toWidgetNode(resolver: any WidgetDesignTokenResolving) -> CustomMessageWidgetNode? {
        switch self {
        case .nil,
             .string,
             .image,
             .spacer,
             .text,
             .button,
             .textField:
            return leafWidgetNode(resolver: resolver)

        case let .effect(props, children):
            return CustomMessageWidgetNode(
                content: .effect(props.nodeProps, children: children.widgetNodes(resolver: resolver)),
                modifiers: .empty
            )

        case let .box(modifiers, props, children):
            let alignment = props.contentAlignment.map { $0.toScale().swiftUIAlignment } ?? .center
            return CustomMessageWidgetNode(
                content: .box(.init(alignment: alignment), children: children.widgetNodes(resolver: resolver)),
                modifiers: modifiers.toNodeModifiers(resolver: resolver)
            )

        case let .column(modifiers, props, children):
            let alignment = props.horizontalAlignment.map { $0.toScale().swiftUIAlignment } ?? .leading
            let arrangement = props.verticalArrangement?.toScale().toNodeArrangement ?? .start
            return CustomMessageWidgetNode(
                content: .column(
                    .init(alignment: alignment, arrangement: arrangement),
                    children: children.widgetNodes(resolver: resolver)
                ),
                modifiers: modifiers.toNodeModifiers(resolver: resolver)
            )

        case let .row(modifiers, props, children):
            let alignment = props.verticalAlignment.map { $0.toScale().swiftUIAlignment } ?? .top
            let arrangement = props.horizontalArrangement?.toScale().toNodeArrangement ?? .start
            return CustomMessageWidgetNode(
                content: .row(
                    .init(alignment: alignment, arrangement: arrangement),
                    children: children.widgetNodes(resolver: resolver)
                ),
                modifiers: modifiers.toNodeModifiers(resolver: resolver)
            )
        }
    }
}

private extension RendererNode {
    /// Nodes that draw themselves, with no children to lay out.
    func leafWidgetNode(resolver: any WidgetDesignTokenResolving) -> CustomMessageWidgetNode? {
        switch self {
        case .nil,
             .string:
            return nil

        case let .image(modifiers, props):
            return CustomMessageWidgetNode(
                content: .image(props.nodeProps),
                modifiers: modifiers.toNodeModifiers(resolver: resolver)
            )

        case let .spacer(modifiers):
            return CustomMessageWidgetNode(
                content: .spacer,
                modifiers: modifiers.toNodeModifiers(resolver: resolver)
            )

        case let .text(modifiers, props, children):
            let text = children.compactMap { child -> String? in
                if case let .string(value) = child { return value }
                return nil
            }
            .joined()
            let labelStyle = (props.style?.toScale() ?? .bodyM).toLabelStyle
            let color = props.color.map { resolver.color(for: $0.toScale()) } ?? resolver.color(for: .textPrimary)
            return CustomMessageWidgetNode(
                content: .text(.init(text: text, labelStyle: labelStyle, color: color)),
                modifiers: modifiers.toNodeModifiers(resolver: resolver)
            )

        case let .button(modifiers, props, _):
            return CustomMessageWidgetNode(
                content: .button(.init(
                    text: props.text,
                    variant: (props.variant?.toScale() ?? .primary).toNodeVariant,
                    isEnabled: props.enabled ?? true,
                    isLoading: props.loading ?? false,
                    clickAction: props.clickAction
                )),
                modifiers: modifiers.toNodeModifiers(resolver: resolver)
            )

        case let .textField(modifiers, props):
            return CustomMessageWidgetNode(
                content: .textField(.init(
                    text: props.text,
                    placeholder: props.placeholder,
                    label: props.label,
                    isEnabled: props.enabled ?? true,
                    valueChangeAction: props.valueChangeAction
                )),
                modifiers: modifiers.toNodeModifiers(resolver: resolver)
            )

        case .box,
             .column,
             .row,
             .effect:
            return nil
        }
    }
}

private extension RendererNode {
    /// The nodes this one contributes to its parent's children: its own, or
    /// none when it does not draw.
    func widgetNodesInPlace(resolver: any WidgetDesignTokenResolving) -> [CustomMessageWidgetNode] {
        toWidgetNode(resolver: resolver).map { [$0] } ?? []
    }
}

private extension [RendererNode] {
    func widgetNodes(resolver: any WidgetDesignTokenResolving) -> [CustomMessageWidgetNode] {
        flatMap { $0.widgetNodesInPlace(resolver: resolver) }
    }
}

private extension [Modifier] {
    // swiftlint:disable:next cyclomatic_complexity
    func toNodeModifiers(resolver: any WidgetDesignTokenResolving) -> CustomMessageWidgetNode.Modifiers {
        var padding = EdgeInsets()
        var margin = EdgeInsets()
        var background: CustomMessageWidgetNode.Background?
        var border: CustomMessageWidgetNode.Border?
        var width: CGFloat?
        var height: CGFloat?
        var minWidth: CGFloat?
        var minHeight: CGFloat?
        var fillWidth = false
        var fillHeight = false
        var opacity: CGFloat?
        var blendingMode: BlendMode?

        for modifier in self {
            switch modifier {
            case let .padding(dimensions):
                padding = dimensions.edgeInsets
            case let .margin(dimensions):
                margin = dimensions.edgeInsets
            case let .background(value):
                background = CustomMessageWidgetNode.Background(
                    color: resolver.color(for: value.color.toScale()),
                    shape: value.shape.map { resolver.shape(for: $0.toScale()) }
                )
            case let .border(style):
                border = CustomMessageWidgetNode.Border(
                    color: resolver.color(for: style.color.toScale()),
                    width: CGFloat(style.width),
                    shape: style.shape.map { resolver.shape(for: $0.toScale()) }
                )
            case let .width(value):
                width = value.drawableLength
            case let .height(value):
                height = value.drawableLength
            case let .minWidth(value):
                minWidth = value.drawableLength
            case let .minHeight(value):
                minHeight = value.drawableLength
            case let .fillWidth(enabled):
                fillWidth = enabled
            case let .fillHeight(enabled):
                fillHeight = enabled
            case let .opacity(value):
                // The core sends 0-255, SwiftUI takes 0-1.
                opacity = CGFloat(value) / 255
            case let .blendingMode(mode):
                blendingMode = mode.blendMode
            }
        }

        return CustomMessageWidgetNode.Modifiers(
            padding: padding,
            margin: margin,
            background: background,
            border: border,
            width: width,
            height: height,
            minWidth: minWidth,
            minHeight: minHeight,
            fillWidth: fillWidth,
            fillHeight: fillHeight,
            opacity: opacity,
            blendingMode: blendingMode
        )
    }
}

private extension Size {
    /// Two orders of magnitude past the longest edge of any device. The decoder
    /// already refuses a size the protocol forbids, so anything still this large
    /// is within the protocol and simply undrawable, and a length SwiftUI cannot
    /// lay out would take the whole face down with it.
    static let maxDrawable: Size = 100_000

    var drawableLength: CGFloat { CGFloat(Swift.min(self, Self.maxDrawable)) }
}

private extension Dimensions {
    /// Wire order is `(top, end, bottom, start)`; `bottom` defaults to `top` and
    /// `start` to `end`.
    var edgeInsets: EdgeInsets {
        EdgeInsets(
            top: top.drawableLength,
            leading: (start ?? end).drawableLength,
            bottom: (bottom ?? top).drawableLength,
            trailing: end.drawableLength
        )
    }
}

// MARK: - Token translation

//
// The design-token resolver is defined over the SCALE token enums, so the core's
// leaf enums are translated into those rather than duplicating the resolver.

private extension TrUAPIHostShape {
    func toScale() -> ScaleShape {
        switch self {
        case let .rounded(radius): .rounded(BigUInt(radius))
        case .square: .rounded(0)
        case .circle: .circle
        }
    }
}

private extension ColorToken {
    func toScale() -> ScaleColorToken {
        switch self {
        case .fgPrimary: .textPrimary
        case .fgSecondary: .textSecondary
        case .fgTertiary: .textTertiary
        case .bgSurfaceMain: .backgroundPrimary
        case .bgSurfaceContainer: .backgroundSecondary
        case .bgSurfaceNested: .backgroundTertiary
        case .fgSuccess: .success
        case .fgError: .error
        case .fgWarning: .warning
        }
    }
}

private extension TypographyStyle {
    func toScale() -> ScaleTypographyStyle {
        switch self {
        case .headlineLarge: .titleXL
        case .titleMediumRegular: .headline
        case .bodyLargeRegular: .bodyM
        case .bodyMediumRegular: .bodyS
        case .bodySmallRegular: .caption
        }
    }
}

private extension ButtonVariant {
    func toScale() -> ScaleButtonVariant {
        switch self {
        case .primary: .primary
        case .secondary: .secondary
        case .text: .text
        }
    }
}

private extension ContentAlignment {
    func toScale() -> ScaleContentAlignment {
        switch self {
        case .topStart: .topStart
        case .topCenter: .topCenter
        case .topEnd: .topEnd
        case .centerStart: .centerStart
        case .center: .center
        case .centerEnd: .centerEnd
        case .bottomStart: .bottomStart
        case .bottomCenter: .bottomCenter
        case .bottomEnd: .bottomEnd
        }
    }
}

private extension TrUAPIHostHorizontalAlignment {
    func toScale() -> ScaleHorizontalAlignment {
        switch self {
        case .start: .start
        case .center: .center
        case .end: .end
        }
    }
}

private extension TrUAPIHostVerticalAlignment {
    func toScale() -> ScaleVerticalAlignment {
        switch self {
        case .top: .top
        case .center: .center
        case .bottom: .bottom
        }
    }
}

private extension Arrangement {
    func toScale() -> ScaleArrangement {
        switch self {
        case .start: .start
        case .end: .end
        case .center: .center
        case .spaceBetween: .spaceBetween
        case .spaceAround: .spaceAround
        case .spaceEvenly: .spaceEvenly
        }
    }
}

private extension ImageProps {
    var nodeProps: CustomMessageWidgetNode.ImageProps {
        let nodeSource: CustomMessageWidgetNode.ImageSource =
            switch source {
            case let .bulletin(cid): .bulletin(cid: cid)
            case let .archive(path): .archive(path: path)
            }
        // Unwrapped first on purpose: in a switch over an optional fit, `.none`
        // binds to `Optional.none` and the fit of that same name is never
        // matched, which the compiler reports as a non-exhaustive switch.
        let nodeFit: CustomMessageWidgetNode.ImageFit =
            if let fit {
                switch fit {
                case .none: .none
                case .fill: .fill
                case .cover: .cover
                case .contain: .contain
                case .scaleDown: .scaleDown
                }
            } else {
                .fill
            }
        return .init(source: nodeSource, fit: nodeFit)
    }
}

private extension EffectProps {
    var nodeProps: CustomMessageWidgetNode.EffectProps {
        let nodeEffect: CustomMessageWidgetNode.Effect =
            switch effect {
            case .rainbow: .rainbow
            }
        return .init(effect: nodeEffect)
    }
}

private extension BlendingMode {
    var blendMode: BlendMode {
        switch self {
        case .normal: .normal
        case .multiply: .multiply
        case .screen: .screen
        case .overlay: .overlay
        case .darken: .darken
        case .lighten: .lighten
        case .colorDodge: .colorDodge
        case .colorBurn: .colorBurn
        case .hardLight: .hardLight
        case .softLight: .softLight
        case .difference: .difference
        case .exclusion: .exclusion
        case .hue: .hue
        case .saturation: .saturation
        case .color: .color
        case .luminosity: .luminosity
        }
    }
}
