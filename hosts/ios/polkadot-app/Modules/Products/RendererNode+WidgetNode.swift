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

        // Only a root effect reaches this case — `widgetNodes` splices nested ones
        // into their parent. It has one slot to fill, and a box would overlay
        // several children, since it renders as a `ZStack`.
        //

        // The effect itself is dropped: `Rainbow` has no counterpart on
        // `CustomMessageWidgetNode`, so the children draw untinted.
        case let .effect(_, children):
            let mapped = children.widgetNodes(resolver: resolver)
            guard mapped.count > 1 else { return mapped.first }
            return CustomMessageWidgetNode(
                content: .column(.init(alignment: .leading, arrangement: .start), children: mapped),
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

        // `ImageProps` carries a source, but `CustomMessageWidgetNode` has no image
        // case to draw it with. An empty box keeps the space its modifiers reserved,
        // rather than collapsing the layout around it.
        case let .image(modifiers, _):
            return CustomMessageWidgetNode(
                content: .box(.init(alignment: .center), children: []),
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
    /// The nodes this one contributes to its parent's children: at most one,
    /// except an `Effect`, which contributes its own. It decorates them without
    /// laying them out, so a container here would impose a layout the product
    /// never asked for. The effect itself is dropped here too.
    func widgetNodesInPlace(resolver: any WidgetDesignTokenResolving) -> [CustomMessageWidgetNode] {
        if case let .effect(_, children) = self {
            return children.widgetNodes(resolver: resolver)
        }

        return toWidgetNode(resolver: resolver).map { [$0] } ?? []
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
                width = CGFloat(value)
            case let .height(value):
                height = CGFloat(value)
            case let .minWidth(value):
                minWidth = CGFloat(value)
            case let .minHeight(value):
                minHeight = CGFloat(value)
            case let .fillWidth(enabled):
                fillWidth = enabled
            case let .fillHeight(enabled):
                fillHeight = enabled
            case let .opacity(value):
                // The core sends 0-255, SwiftUI takes 0-1.
                opacity = CGFloat(value) / 255
            // No slot on `CustomMessageWidgetNode.Modifiers`, and no compositing
            // to apply it to: the node draws normally against what is behind it.
            case .blendingMode:
                continue
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
            opacity: opacity
        )
    }
}

private extension Dimensions {
    /// Wire order is `(top, end, bottom, start)`; `bottom` defaults to `top` and
    /// `start` to `end`.
    var edgeInsets: EdgeInsets {
        EdgeInsets(
            top: CGFloat(top),
            leading: CGFloat(start ?? end),
            bottom: CGFloat(bottom ?? top),
            trailing: CGFloat(end)
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
