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
    }
}

// MARK: - Props

public extension CustomMessageWidgetNode {
    struct BoxProps {
        public let alignment: Alignment

        public init(alignment: Alignment = .topLeading) {
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

    enum ImageFit: Sendable {
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
        public let pan: Pan?
        /// Applied in order, each from its pan's value every frame.
        public let drives: [Drive]
        /// Distance of the viewer from the children this node rotates in 3D, in points.
        public let perspective: CGFloat?

        public var hasWidthConstraint: Bool {
            width != nil || minWidth != nil || fillWidth
        }

        public var hasHeightConstraint: Bool {
            height != nil || minHeight != nil || fillHeight
        }

        /// A fixed or filled width; a minimum alone is not one.
        public var hasOwnWidth: Bool {
            width != nil || fillWidth
        }

        public var hasOwnHeight: Bool {
            height != nil || fillHeight
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
            blendingMode: nil,
            pan: nil,
            drives: [],
            perspective: nil
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
            blendingMode: BlendMode? = nil,
            pan: Pan? = nil,
            drives: [Drive] = [],
            perspective: CGFloat? = nil
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
            self.pan = pan
            self.drives = drives
            self.perspective = perspective
        }
    }
}

// MARK: - Motion

public extension CustomMessageWidgetNode {
    /// A drag on a node that moves a value the host keeps, which comes to rest on whole steps.
    ///
    /// The value is a number of steps. A drag of `distance` times the node's size along `axis` moves it by one, and
    /// the product sees only where it comes to rest. `value` is the rest the product declares, applied when it
    /// changes between trees.
    struct Pan: Equatable {
        public enum Axis: Equatable {
            case horizontal
            case vertical
        }

        public let id: String
        public let axis: Axis
        /// Share of the node's size along `axis` that moves the value one step. Negative counts a leftward or
        /// upward drag as a positive step.
        public let distance: Double
        public let value: Int
        /// Steps one drag may move from where it began. Nil is unbounded.
        public let maxStepsPerGesture: Int?
        /// Drives read the value modulo this many steps. Nil does not wrap.
        public let period: Int?
        /// How a declared change is animated. Nil jumps.
        public let animation: PanAnimation?
        /// Where the pan reports its events. Nil reports nothing.
        public let actionId: String?
        /// The accessibility action that steps the value by one. Nil offers none.
        public let label: String?

        public init(
            id: String,
            axis: Axis,
            distance: Double,
            value: Int,
            maxStepsPerGesture: Int?,
            period: Int?,
            animation: PanAnimation?,
            actionId: String?,
            label: String?
        ) {
            self.id = id
            self.axis = axis
            self.distance = distance
            self.value = value
            self.maxStepsPerGesture = maxStepsPerGesture
            self.period = period
            self.animation = animation
            self.actionId = actionId
            self.label = label
        }
    }

    /// How a pan moves to a value a new tree declares. Durations are in seconds, bounce from 0 to 1.
    enum PanAnimation: Equatable {
        case spring(duration: Double, bounce: Double)
        case ease(duration: Double)
    }

    /// A property of a node computed every frame from a pan's value.
    struct Drive: Equatable {
        /// Rotations in degrees, scale as a factor, opacity from 0 to 1, translations in points.
        public enum Property: Equatable {
            case rotationX
            case rotationY
            case rotationZ
            case scale
            case opacity
            case translationX
            case translationY
        }

        /// The `Pan.id` the property follows.
        public let source: String
        public let property: Property
        /// Strictly ascending in `at`.
        public let keyframes: [Keyframe]

        public init(source: String, property: Property, keyframes: [Keyframe]) {
            self.source = source
            self.property = property
            self.keyframes = keyframes
        }
    }

    /// The value of a driven property at one pan value.
    struct Keyframe: Equatable {
        public enum Curve: Equatable {
            /// A straight line from the previous keyframe.
            case linear
            /// Holds the previous value until `at`, then jumps.
            case step
        }

        public let at: Double
        public let value: Double
        public let curve: Curve

        public init(at: Double, value: Double, curve: Curve) {
            self.at = at
            self.value = value
            self.curve = curve
        }
    }
}

/// What a pan tells the product, on the pan's action.
public enum WidgetPanEvent: Equatable {
    /// A finger took the pan.
    case began
    /// The value came to rest on this step.
    case settled(Int)
    /// Something else took the press before the value moved.
    case cancelled
}
