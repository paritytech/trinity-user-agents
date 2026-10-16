import SwiftUI

// MARK: - Node

public struct CustomMessageWidgetNode {
    public let content: Content
    public let modifiers: Modifiers

    public init(content: Content, modifiers: Modifiers = .empty) {
        self.content = content
        self.modifiers = modifiers
    }
}

// MARK: - Content

public extension CustomMessageWidgetNode {
    enum Content {
        case box(BoxProps, children: [CustomMessageWidgetNode])
        case column(ColumnProps, children: [CustomMessageWidgetNode])
        case row(RowProps, children: [CustomMessageWidgetNode])
        case spacer
        case text(TextProps)
        case button(ButtonProps)
        case textField(TextFieldProps)
        case image(ImageProps)
        case effect(EffectProps, children: [CustomMessageWidgetNode])
    }
}

// MARK: - Props

public extension CustomMessageWidgetNode {
    struct BoxProps {
        public let alignment: Alignment

        public init(alignment: Alignment = .center) {
            self.alignment = alignment
        }
    }

    struct ColumnProps {
        public let alignment: HorizontalAlignment
        public let arrangement: Arrangement

        public init(alignment: HorizontalAlignment = .leading, arrangement: Arrangement = .start) {
            self.alignment = alignment
            self.arrangement = arrangement
        }
    }

    struct RowProps {
        public let alignment: VerticalAlignment
        public let arrangement: Arrangement

        public init(alignment: VerticalAlignment = .top, arrangement: Arrangement = .start) {
            self.alignment = alignment
            self.arrangement = arrangement
        }
    }

    struct TextProps {
        public let text: String
        public let labelStyle: LabelStyle
        public let color: Color

        public init(text: String, labelStyle: LabelStyle, color: Color) {
            self.text = text
            self.labelStyle = labelStyle
            self.color = color
        }
    }

    struct ButtonProps {
        public let text: String
        public let variant: ButtonVariant
        public let isEnabled: Bool
        public let isLoading: Bool
        public let clickAction: String?

        public init(
            text: String,
            variant: ButtonVariant = .primary,
            isEnabled: Bool = true,
            isLoading: Bool = false,
            clickAction: String? = nil
        ) {
            self.text = text
            self.variant = variant
            self.isEnabled = isEnabled
            self.isLoading = isLoading
            self.clickAction = clickAction
        }
    }

    struct TextFieldProps {
        public let text: String
        public let placeholder: String?
        public let label: String?
        public let isEnabled: Bool
        public let valueChangeAction: String?

        public init(
            text: String,
            placeholder: String? = nil,
            label: String? = nil,
            isEnabled: Bool = true,
            valueChangeAction: String? = nil
        ) {
            self.text = text
            self.placeholder = placeholder
            self.label = label
            self.isEnabled = isEnabled
            self.valueChangeAction = valueChangeAction
        }
    }
}

public extension CustomMessageWidgetNode {
    /// Where an image's bytes come from. The node names the source; fetching it
    /// belongs to whoever knows about product archives and the Bulletin gateway.
    enum ImageSource: Hashable {
        case bulletin(cid: String)
        case archive(path: String)
    }

    enum ImageFit {
        case none
        case fill
        case cover
        case contain
        case scaleDown
    }

    struct ImageProps {
        public let source: ImageSource
        public let fit: ImageFit

        public init(source: ImageSource, fit: ImageFit = .fill) {
            self.source = source
            self.fit = fit
        }
    }

    /// A host-drawn effect applied to the children beneath it.
    enum Effect {
        case rainbow
    }

    struct EffectProps {
        public let effect: Effect

        public init(effect: Effect) {
            self.effect = effect
        }
    }
}

// MARK: - Supporting Types

public extension CustomMessageWidgetNode {
    struct Background {
        public let color: Color
        public let shape: AnyShape?

        public init(color: Color, shape: AnyShape? = nil) {
            self.color = color
            self.shape = shape
        }
    }

    struct Border {
        public let color: Color
        public let width: CGFloat
        public let shape: AnyShape?

        public init(color: Color, width: CGFloat, shape: AnyShape? = nil) {
            self.color = color
            self.width = width
            self.shape = shape
        }
    }

    enum Arrangement {
        case start
        case end
        case center
        case spaceBetween
        case spaceAround
        case spaceEvenly

        var isSpaced: Bool {
            switch self {
            case .spaceBetween,
                 .spaceAround,
                 .spaceEvenly: true
            default: false
            }
        }
    }

    enum ButtonVariant {
        case primary
        case secondary
        case text
    }
}

// MARK: - Modifiers

public extension CustomMessageWidgetNode {
    struct Modifiers {
        public let padding: EdgeInsets
        public let margin: EdgeInsets
        public let background: Background?
        public let border: Border?
        public let width: CGFloat?
        public let height: CGFloat?
        public let minWidth: CGFloat?
        public let minHeight: CGFloat?
        public let fillWidth: Bool
        public let fillHeight: Bool
        /// `nil` is fully opaque, matching a node that carries no opacity modifier.
        public let opacity: CGFloat?
        public let blendingMode: BlendMode?

        public var hasWidthConstraint: Bool {
            width != nil || minWidth != nil || fillWidth
        }

        public var hasHeightConstraint: Bool {
            height != nil || minHeight != nil || fillHeight
        }

        public static let empty = Modifiers(
            padding: EdgeInsets(),
            margin: EdgeInsets(),
            background: nil,
            border: nil,
            width: nil,
            height: nil,
            minWidth: nil,
            minHeight: nil,
            fillWidth: false,
            fillHeight: false,
            opacity: nil,
            blendingMode: nil
        )

        public init(
            padding: EdgeInsets = EdgeInsets(),
            margin: EdgeInsets = EdgeInsets(),
            background: Background? = nil,
            border: Border? = nil,
            width: CGFloat? = nil,
            height: CGFloat? = nil,
            minWidth: CGFloat? = nil,
            minHeight: CGFloat? = nil,
            fillWidth: Bool = false,
            fillHeight: Bool = false,
            opacity: CGFloat? = nil,
            blendingMode: BlendMode? = nil
        ) {
            self.padding = padding
            self.margin = margin
            self.background = background
            self.border = border
            self.width = width
            self.height = height
            self.minWidth = minWidth
            self.minHeight = minHeight
            self.fillWidth = fillWidth
            self.fillHeight = fillHeight
            self.opacity = opacity
            self.blendingMode = blendingMode
        }
    }
}
