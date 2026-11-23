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
             .textField,
             .slideToConfirm,
             .setOffset:
            return leafWidgetNode(resolver: resolver)

        case let .gesture(modifiers, props, children):
            return CustomMessageWidgetNode(
                content: .gesture(props.nodeProps, children: children.widgetNodes(resolver: resolver)),
                modifiers: modifiers.toNodeModifiers(resolver: resolver)
            )

        case let .materials(_, children):
            return CustomMessageWidgetNode(
                content: .box(.init(), children: children.widgetNodes(resolver: resolver)),
                modifiers: .empty
            )

        case let .box(modifiers, props, children):
            let boxProps = props.contentAlignment.map {
                CustomMessageWidgetNode.BoxProps(alignment: $0.toScale().swiftUIAlignment)
            }
            return CustomMessageWidgetNode(
                content: .box(boxProps ?? .init(), children: children.widgetNodes(resolver: resolver)),
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
             .string,
             .setOffset:
            // An offset update is a stream item, not part of a tree: inside one it draws nothing.
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

        case let .slideToConfirm(modifiers, props):
            return CustomMessageWidgetNode(
                content: .slideToConfirm(props.nodeProps(resolver: resolver)),
                modifiers: modifiers.toNodeModifiers(resolver: resolver)
            )

        case .gesture,
             .box,
             .column,
             .row,
             .materials:
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
        var shadow: CustomMessageWidgetNode.Shadow?
        var translate = CGSize.zero
        var font: CustomMessageWidgetNode.TextFont?
        var panFade: CustomMessageWidgetNode.PanFade?
        var shimmer: CustomMessageWidgetNode.Shimmer?
        var spinPeriod: TimeInterval?
        var exitFade: TimeInterval?

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
            case let .shadow(value):
                shadow = CustomMessageWidgetNode.Shadow(
                    color: Color(rgba: value.color),
                    x: CGFloat(value.x) / 100,
                    y: CGFloat(value.y) / 100,
                    blur: CGFloat(value.blur) / 100
                )
            case let .translate(value):
                translate = CGSize(width: CGFloat(value.x) / 100, height: CGFloat(value.y) / 100)
            case let .font(value):
                font = CustomMessageWidgetNode.TextFont(
                    size: CGFloat(value.size) / 100,
                    lineHeight: CGFloat(value.lineHeight) / 100,
                    weight: Int(value.weight),
                    color: Color(rgba: value.color),
                    letterSpacing: CGFloat(value.letterSpacing) / 100
                )
            case let .fadeWithPan(value):
                panFade = CustomMessageWidgetNode.PanFade(key: value.key, rate: CGFloat(value.ratePermille) / 1_000)
            case let .shimmer(value):
                shimmer = CustomMessageWidgetNode.Shimmer(
                    color: Color(rgba: value.color | 0xFF),
                    stops: value.stops.map { CGFloat($0) / 255 },
                    band: CGFloat(value.band) / 100,
                    lead: CGFloat(value.lead) / 100,
                    speed: CGFloat(value.speed) / 100,
                    rest: TimeInterval(value.restMs) / 1_000
                )
            case let .spin(periodMs):
                spinPeriod = periodMs > 0 ? TimeInterval(periodMs) / 1_000 : nil
            case let .exitFade(ms):
                exitFade = TimeInterval(ms) / 1_000
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
            shadow: shadow,
            translate: translate,
            font: font,
            panFade: panFade,
            shimmer: shimmer,
            spinPeriod: spinPeriod,
            exitFade: exitFade
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

private extension SlideToConfirmProps {
    func nodeProps(resolver: any WidgetDesignTokenResolving) -> CustomMessageWidgetNode.SlideToConfirmProps {
        let nodeVariant: CustomMessageWidgetNode.SlideToConfirmVariant =
            switch variant ?? .thumb {
            case .thumb: .thumb
            case .reveal: .reveal
            }
        return .init(
            text: text,
            secondaryText: secondaryText,
            confirmedText: confirmedText,
            variant: nodeVariant,
            accent: accent.map { resolver.color(for: $0.toScale()) },
            state: .init(
                isEnabled: enabled ?? true,
                isLoading: loading ?? false,
                isConfirmed: confirmed ?? false
            ),
            confirmAction: confirmAction
        )
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

private extension GestureProps {
    var nodeProps: CustomMessageWidgetNode.GestureProps {
        var value = CustomMessageWidgetNode.GestureProps { phase, position, fraction in
            let nativePhase: GesturePhase =
                switch phase {
                case .tap: .tap
                case .started: .started
                case .moved: .moved
                case .ended: .ended
                case .cancelled: .cancelled
                case .completed: .completed
                }
            // Positions travel in hundredths of a logical pixel, so a slow drag still reports every frame's move.
            return Data(encodeGestureEvent(event: GestureEvent(
                phase: nativePhase,
                x: Int32(clamping: Int((position.x * 100).rounded())),
                y: Int32(clamping: Int((position.y * 100).rounded())),
                fractionPermille: fraction
            )))
        }
        value.tapAction = tapAction
        value.pan = pan?.nodeProps
        value.accessibilityLabel = accessibilityLabel
        value.isEnabled = enabled ?? true
        value.hapticOnComplete = hapticOnComplete ?? false
        value.key = key
        value.feel = feel?.nodeFeel
        return value
    }
}

private extension PanFeel {
    var nodeFeel: CustomMessageWidgetNode.PanFeel {
        var value = CustomMessageWidgetNode.PanFeel()
        value.pressScale = CGFloat(pressScalePermille) / 1_000
        value.pressDuration = TimeInterval(pressMs) / 1_000
        value.unpressDuration = TimeInterval(unpressMs) / 1_000
        value.springOmega = CGFloat(springOmegaCenti) / 100
        value.springDamping = CGFloat(springDampingCenti) / 100
        value.approachDuration = TimeInterval(approachMs) / 1_000
        value.approachReference = max(CGFloat(approachReference) / 100, 1)
        value.pop = popPermille.map { CGFloat($0) / 1_000 }
        value.popOffset = CGFloat(popOffset) / 100
        return value
    }
}

extension Color {
    /// A colour given by value on the wire: `0xRRGGBBAA`, sRGB, straight alpha.
    init(rgba: UInt32) {
        self.init(
            .sRGB,
            red: Double((rgba >> 24) & 0xFF) / 255,
            green: Double((rgba >> 16) & 0xFF) / 255,
            blue: Double((rgba >> 8) & 0xFF) / 255,
            opacity: Double(rgba & 0xFF) / 255
        )
    }
}

private extension PanGesture {
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
