import BigInt
import PolkadotUI
import Products
import SwiftUI

extension ScaleWidget {
    func toWidgetNode(resolver: any WidgetDesignTokenResolving) -> CustomMessageWidgetNode? {
        switch self {
        case .nil,
             .string:
            nil
        case let .box(component):
            component.toBoxNode(resolver: resolver)
        case let .column(component):
            component.toColumnNode(resolver: resolver)
        case let .row(component):
            component.toRowNode(resolver: resolver)
        case let .spacer(component):
            component.toSpacerNode(resolver: resolver)
        case let .text(component):
            component.toTextNode(resolver: resolver)
        case let .button(component):
            component.toButtonNode(resolver: resolver)
        case let .textField(component):
            component.toTextFieldNode(resolver: resolver)
        }
    }
}

// MARK: - Component Mappers

private extension ScaleComponent where Props == ScaleBoxProps {
    func toBoxNode(resolver: any WidgetDesignTokenResolving) -> CustomMessageWidgetNode {
        let alignment = props.contentAlignment.value.map(\.swiftUIAlignment) ?? .center
        let children = children.compactMap { $0.toWidgetNode(resolver: resolver) }
        return CustomMessageWidgetNode(
            content: .box(.init(alignment: alignment), children: children),
            modifiers: modifiers.toNodeModifiers(resolver: resolver)
        )
    }
}

private extension ScaleComponent where Props == ScaleColumnProps {
    func toColumnNode(resolver: any WidgetDesignTokenResolving) -> CustomMessageWidgetNode {
        let alignment = props.horizontalAlignment.value.map(\.swiftUIAlignment) ?? .leading
        let arrangement = props.verticalArrangement.value?.toNodeArrangement ?? .start
        let children = children.compactMap { $0.toWidgetNode(resolver: resolver) }
        return CustomMessageWidgetNode(
            content: .column(.init(alignment: alignment, arrangement: arrangement), children: children),
            modifiers: modifiers.toNodeModifiers(resolver: resolver)
        )
    }
}

private extension ScaleComponent where Props == ScaleRowProps {
    func toRowNode(resolver: any WidgetDesignTokenResolving) -> CustomMessageWidgetNode {
        let alignment = props.verticalAlignment.value.map(\.swiftUIAlignment) ?? .top
        let arrangement = props.horizontalArrangement.value?.toNodeArrangement ?? .start
        let children = children.compactMap { $0.toWidgetNode(resolver: resolver) }
        return CustomMessageWidgetNode(
            content: .row(.init(alignment: alignment, arrangement: arrangement), children: children),
            modifiers: modifiers.toNodeModifiers(resolver: resolver)
        )
    }
}

private extension ScaleComponent where Props == ScaleVoidProps {
    func toSpacerNode(resolver: any WidgetDesignTokenResolving) -> CustomMessageWidgetNode {
        CustomMessageWidgetNode(
            content: .spacer,
            modifiers: modifiers.toNodeModifiers(resolver: resolver)
        )
    }
}

private extension ScaleComponent where Props == ScaleTextProps {
    func toTextNode(resolver: any WidgetDesignTokenResolving) -> CustomMessageWidgetNode {
        let text = children.compactMap { child -> String? in
            if case let .string(value) = child { return value }
            return nil
        }
        .joined()

        let style = props.style.value ?? .bodyM
        let labelStyle = style.toLabelStyle
        let color = props.color.value.map { resolver.color(for: $0) } ?? resolver.color(for: .textPrimary)

        return CustomMessageWidgetNode(
            content: .text(.init(text: text, labelStyle: labelStyle, color: color)),
            modifiers: modifiers.toNodeModifiers(resolver: resolver)
        )
    }
}

private extension ScaleComponent where Props == ScaleButtonProps {
    func toButtonNode(resolver: any WidgetDesignTokenResolving) -> CustomMessageWidgetNode {
        CustomMessageWidgetNode(
            content: .button(.init(
                text: props.text,
                variant: (props.variant.value ?? .primary).toNodeVariant,
                isEnabled: props.enabled.value ?? true,
                isLoading: props.loading.value ?? false,
                clickAction: props.clickAction.value
            )),
            modifiers: modifiers.toNodeModifiers(resolver: resolver)
        )
    }
}

private extension ScaleComponent where Props == ScaleTextFieldProps {
    func toTextFieldNode(resolver: any WidgetDesignTokenResolving) -> CustomMessageWidgetNode {
        CustomMessageWidgetNode(
            content: .textField(.init(
                text: props.text,
                placeholder: props.placeholder.value,
                label: props.label.value,
                isEnabled: props.enabled.value ?? true,
                valueChangeAction: props.valueChangeAction.value
            )),
            modifiers: modifiers.toNodeModifiers(resolver: resolver)
        )
    }
}

// MARK: - Modifier Mapping

private extension [ScaleModifier] {
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

        for modifier in self {
            switch modifier {
            case let .padding(dims):
                padding = dims.edgeInsets
            case let .margin(dims):
                margin = dims.edgeInsets
            case let .background(bgModel):
                background = CustomMessageWidgetNode.Background(
                    color: resolver.color(for: bgModel.color),
                    shape: bgModel.shape.value.map { resolver.shape(for: $0) }
                )
            case let .border(style):
                border = CustomMessageWidgetNode.Border(
                    color: resolver.color(for: style.color),
                    width: CGFloat(style.width),
                    shape: style.shape.value.map { resolver.shape(for: $0) }
                )
            case let .width(value): width = CGFloat(value)
            case let .height(value): height = CGFloat(value)
            case let .minWidth(value): minWidth = CGFloat(value)
            case let .minHeight(value): minHeight = CGFloat(value)
            case let .fillWidth(fill): fillWidth = fill
            case let .fillHeight(fill): fillHeight = fill
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
            fillHeight: fillHeight
        )
    }
}

// MARK: - Enum Mapping

extension ScaleArrangement {
    var toNodeArrangement: CustomMessageWidgetNode.Arrangement {
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

extension ScaleButtonVariant {
    var toNodeVariant: CustomMessageWidgetNode.ButtonVariant {
        switch self {
        case .primary: .primary
        case .secondary: .secondary
        case .text: .text
        }
    }
}

// MARK: - Alignment Conversions

extension ScaleContentAlignment {
    var swiftUIAlignment: Alignment {
        switch self {
        case .topStart: .topLeading
        case .topCenter: .top
        case .topEnd: .topTrailing
        case .centerStart: .leading
        case .center: .center
        case .centerEnd: .trailing
        case .bottomStart: .bottomLeading
        case .bottomCenter: .bottom
        case .bottomEnd: .bottomTrailing
        }
    }
}

extension ScaleHorizontalAlignment {
    var swiftUIAlignment: HorizontalAlignment {
        switch self {
        case .start: .leading
        case .center: .center
        case .end: .trailing
        }
    }
}

extension ScaleVerticalAlignment {
    var swiftUIAlignment: VerticalAlignment {
        switch self {
        case .top: .top
        case .center: .center
        case .bottom: .bottom
        }
    }
}

// MARK: - Typography Mapping

extension ScaleTypographyStyle {
    var toLabelStyle: PolkadotUI.LabelStyle {
        switch self {
        case .titleXL: .title32SemiBold()
        case .headline: .headline16Medium()
        case .bodyM: .body16Regular()
        case .bodyS: .body14Regular()
        case .caption: .caption12Regular()
        }
    }
}

// MARK: - ScaleDimensions → EdgeInsets

private extension ScaleDimensions {
    var edgeInsets: EdgeInsets {
        EdgeInsets(
            top: CGFloat(vertical),
            leading: CGFloat(start.value ?? horizontal),
            bottom: CGFloat(vertical),
            trailing: CGFloat(end.value ?? horizontal)
        )
    }
}
