import BigInt
import TrUAPIHost
import PolkadotUI
import Products
import SwiftUI

extension RendererNode {
    /// `Materials` nodes are numbered depth first, a parent before its children, across the whole tree,
    /// the order of `ResolvedMaterials.nodes`. `Touch` nodes are numbered the same way among themselves.
    func toWidgetNode(resolver: any WidgetDesignTokenResolving) -> CustomMessageWidgetNode? {
        toWidgetNode(resolver: resolver, counter: MaterialsCounter(takesTouch: rendererTouchNodes(tree: self)))
    }

    /// How many entries `ResolvedMaterials.nodes` has for this tree.
    var materialsCount: Int {
        let counter = MaterialsCounter(takesTouch: [])
        counter.skip([self])
        return counter.count
    }

    /// The props of every `Touch` node, depth first.
    var touchProps: [TouchProps] {
        let own: [TouchProps] = if case let .touch(props, _) = self { [props] } else { [] }
        return own + children.flatMap(\.touchProps)
    }
}

private final class MaterialsCounter {
    private var next: UInt32 = 0
    private var nextTouch: UInt32 = 0
    private let takesTouch: [Bool]

    init(takesTouch: [Bool]) {
        self.takesTouch = takesTouch
    }

    var count: Int { Int(next) }

    func take() -> UInt32 {
        defer { next += 1 }
        return next
    }

    /// The next `Touch` node, and whether the core lets it take touch.
    func takeTouch() -> (index: UInt32, takesTouch: Bool) {
        defer { nextTouch += 1 }
        return (nextTouch, takesTouch.indices.contains(Int(nextTouch)) && takesTouch[Int(nextTouch)])
    }

    /// The core also numbers `Materials` and `Touch` nodes inside text and button children, which draw
    /// no widget.
    func skip(_ nodes: [RendererNode]) {
        for node in nodes {
            if case .materials = node { _ = take() }
            if case .touch = node { _ = takeTouch() }
            skip(node.children)
        }
    }
}

private extension RendererNode {
    var children: [RendererNode] {
        switch self {
        case let .materials(_, children),
             let .touch(_, children),
             let .gesture(_, _, children),
             let .box(_, _, children),
             let .column(_, _, children),
             let .row(_, _, children),
             let .text(_, _, children),
             let .button(_, _, children):
            children
        case .nil,
             .string,
             .image,
             .spacer,
             .textField,
             .setOffset,
             .extendMask:
            []
        }
    }

    func toWidgetNode(
        resolver: any WidgetDesignTokenResolving,
        counter: MaterialsCounter
    ) -> CustomMessageWidgetNode? {
        switch self {
        case .nil,
             .string,
             .image,
             .spacer,
             .text,
             .button,
             .textField,
             .setOffset,
             .extendMask:
            counter.skip(children)
            return leafWidgetNode(resolver: resolver)

        case let .gesture(modifiers, props, children):
            return CustomMessageWidgetNode(
                content: .gesture(
                    props.nodeProps,
                    children: children.widgetNodes(resolver: resolver, counter: counter)
                ),
                modifiers: modifiers.toNodeModifiers(resolver: resolver)
            )

        case let .materials(_, children):
            let index = counter.take()
            return CustomMessageWidgetNode(
                content: .materials(
                    index: index,
                    children: children.widgetNodes(resolver: resolver, counter: counter)
                ),
                modifiers: .empty
            )

        case let .touch(props, children):
            let (index, takesTouch) = counter.takeTouch()
            return CustomMessageWidgetNode(
                content: .touch(
                    WidgetTouchNode(
                        index: index,
                        drag: props.drag == .horizontal ? .horizontal : .any,
                        takesTouch: takesTouch,
                        accessibilityActions: props.triggers.compactMap(\.accessibilityAction)
                    ),
                    children: children.widgetNodes(resolver: resolver, counter: counter)
                ),
                modifiers: .empty
            )

        case let .box(modifiers, props, children):
            let boxProps = props.contentAlignment.map {
                CustomMessageWidgetNode.BoxProps(alignment: $0.toScale().swiftUIAlignment)
            }
            return CustomMessageWidgetNode(
                content: .box(
                    boxProps ?? .init(),
                    children: children.widgetNodes(resolver: resolver, counter: counter)
                ),
                modifiers: modifiers.toNodeModifiers(resolver: resolver)
            )

        case let .column(modifiers, props, children):
            let alignment = props.horizontalAlignment.map { $0.toScale().swiftUIAlignment } ?? .leading
            let arrangement = props.verticalArrangement?.toScale().toNodeArrangement ?? .start
            return CustomMessageWidgetNode(
                content: .column(
                    .init(alignment: alignment, arrangement: arrangement),
                    children: children.widgetNodes(resolver: resolver, counter: counter)
                ),
                modifiers: modifiers.toNodeModifiers(resolver: resolver)
            )

        case let .row(modifiers, props, children):
            let alignment = props.verticalAlignment.map { $0.toScale().swiftUIAlignment } ?? .top
            let arrangement = props.horizontalArrangement?.toScale().toNodeArrangement ?? .start
            return CustomMessageWidgetNode(
                content: .row(
                    .init(alignment: alignment, arrangement: arrangement),
                    children: children.widgetNodes(resolver: resolver, counter: counter)
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
             .string,
             .setOffset,
             .extendMask:
            // Offset and mask updates are stream items, not part of a tree: inside one they draw nothing.
            return nil

        case let .image(modifiers, props):
            // An image this host cannot draw keeps the size its modifiers give it.
            return CustomMessageWidgetNode(
                content: props.nodeProps.map { .image($0) } ?? .box(.init(), children: []),
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

        case .gesture,
             .box,
             .column,
             .row,
             .materials,
             .touch:
            return nil
        }
    }
}

private extension TouchTrigger {
    /// A tap or a drag VoiceOver can stand in for. A trail has no single action to offer.
    var accessibilityAction: String? {
        if case .trailCovers = when { nil } else { action }
    }
}

private extension RendererNode {
    /// The nodes this one contributes to its parent's children: its own, or
    /// none when it does not draw.
    func widgetNodesInPlace(
        resolver: any WidgetDesignTokenResolving,
        counter: MaterialsCounter
    ) -> [CustomMessageWidgetNode] {
        toWidgetNode(resolver: resolver, counter: counter).map { [$0] } ?? []
    }
}

private extension [RendererNode] {
    func widgetNodes(
        resolver: any WidgetDesignTokenResolving,
        counter: MaterialsCounter
    ) -> [CustomMessageWidgetNode] {
        flatMap { $0.widgetNodesInPlace(resolver: resolver, counter: counter) }
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
        var style = NodeStyleByValue()
        var mask: CustomMessageWidgetNode.Mask?

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
            case let .fill(value):
                background = CustomMessageWidgetNode.Background(
                    color: Color(rgba: value.color),
                    shape: value.shape.map { resolver.shape(for: $0.toScale()) }
                )
            case .shadow,
                 .translate,
                 .font,
                 .fadeWithPan,
                 .shimmer,
                 .spin,
                 .exitFade:
                style.take(modifier)
            case let .mask(value):
                mask = value.widgetMask
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
            blendingMode: blendingMode,
            shadow: style.shadow,
            translate: style.translate,
            font: style.font,
            panFade: style.panFade,
            shimmer: style.shimmer,
            spinPeriod: style.spinPeriod,
            exitFade: style.exitFade,
            mask: mask
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

private extension ImageProps {
    var nodeProps: CustomMessageWidgetNode.ImageProps? {
        let nodeSource: CustomMessageWidgetNode.ImageSource
        switch source {
        case let .bulletin(cid): nodeSource = .bulletin(cid: cid)
        case let .archive(path): nodeSource = .archive(path: path)
        case .inline: return nil
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

extension PanGesture {
    var nodeProps: CustomMessageWidgetNode.PanGesture {
        var value = CustomMessageWidgetNode.PanGesture()
        switch axis {
        case .horizontal: value.axis = .horizontal
        case .vertical: value.axis = .vertical
        case .free: value.axis = .free
        }
        value.drag = drag?.nodeProps
        value.startAction = startAction
        value.moveAction = moveAction
        value.endAction = endAction
        value.moveInterval = TimeInterval(moveIntervalMs ?? 0) / 1_000
        value.reportsLocation = location ?? false
        return value
    }
}

private extension PanDrag {
    var nodeProps: CustomMessageWidgetNode.PanDrag {
        var value = CustomMessageWidgetNode.PanDrag()
        if case let .points(points) = travel { value.travelPoints = points.drawableLength }
        switch release {
        case .snapBack: value.release = .snapBack
        case .stay: value.release = .stay
        case let .complete(threshold): value.release = .complete(thresholdPercent: threshold)
        }
        value.holdAtEnd = holdAtEnd ?? true
        return value
    }
}
