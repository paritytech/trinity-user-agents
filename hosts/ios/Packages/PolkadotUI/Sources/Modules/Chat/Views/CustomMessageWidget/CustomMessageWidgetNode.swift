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
        case gesture(GestureProps, children: [CustomMessageWidgetNode])
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

    /// A drop shadow as CSS `box-shadow` draws it: `blur` is the CSS blur length (twice the Gaussian's deviation).
    struct Shadow: Equatable {
        public let color: Color
        /// The shadow moved from the node: towards the end, and downwards.
        public let offset: CGSize
        public let blur: CGFloat

        public init(color: Color, offset: CGSize, blur: CGFloat) {
            self.color = color
            self.offset = offset
            self.blur = blur
        }
    }

    /// A text's type given by value: the design system's sans at this size, weight and line height.
    struct TextFont: Equatable {
        public let size: CGFloat
        public let lineHeight: CGFloat
        public let weight: Int
        public let color: Color
        public let letterSpacing: CGFloat

        public init(size: CGFloat, lineHeight: CGFloat, weight: Int, color: Color, letterSpacing: CGFloat) {
            self.size = size
            self.lineHeight = lineHeight
            self.weight = weight
            self.color = color
            self.letterSpacing = letterSpacing
        }
    }

    /// Opacity following a keyed pan's children along its travel: `1 - rate * progress`, clamped.
    struct PanFade: Equatable {
        public let key: String
        public let rate: CGFloat

        public init(key: String, rate: CGFloat) {
            self.key = key
            self.rate = rate
        }
    }

    /// A band of light sweeping a text's glyphs (see the renderer's `Shimmer`).
    struct Shimmer: Equatable {
        public let color: Color
        /// Alphas, 0 to 1, spread evenly across the band.
        public let stops: [CGFloat]
        public let band: CGFloat
        public let lead: CGFloat
        /// Points a second.
        public let speed: CGFloat
        public let rest: TimeInterval

        public init(color: Color, stops: [CGFloat], band: CGFloat, lead: CGFloat, speed: CGFloat, rest: TimeInterval) {
            self.color = color
            self.stops = stops
            self.band = band
            self.lead = lead
            self.speed = speed
            self.rest = rest
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
        public let shadow: Shadow?
        /// Where the node is drawn, moved from its place; the layout is unchanged.
        public let translate: CGSize
        public let font: TextFont?
        public let panFade: PanFade?
        public let shimmer: Shimmer?
        /// One turn every this long, while shown.
        public let spinPeriod: TimeInterval?
        /// How long the node fades out for once a tree drops it.
        public let exitFade: TimeInterval?
        /// Hides part of the node, or shows only part of it.
        public let mask: Mask?
        /// Plays the node in when it appears.
        public let enter: Enter?
        /// Moves parts of how the node is drawn on the host's clock, at most one per property.
        public let tracks: [Track]
        /// Sends particles over the node.
        public let emitter: Emitter?

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
            mask: nil
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
            shadow: Shadow? = nil,
            translate: CGSize = .zero,
            font: TextFont? = nil,
            panFade: PanFade? = nil,
            shimmer: Shimmer? = nil,
            spinPeriod: TimeInterval? = nil,
            exitFade: TimeInterval? = nil,
            mask: Mask? = nil,
            enter: Enter? = nil,
            tracks: [Track] = [],
            emitter: Emitter? = nil
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
            self.shadow = shadow
            self.translate = translate
            self.font = font
            self.panFade = panFade
            self.shimmer = shimmer
            self.spinPeriod = spinPeriod
            self.exitFade = exitFade
            self.mask = mask
            self.enter = enter
            self.tracks = tracks
            self.emitter = emitter
        }
    }
}
