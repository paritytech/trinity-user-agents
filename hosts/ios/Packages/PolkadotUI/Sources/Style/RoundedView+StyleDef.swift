import Foundation
public import UIKit_iOS
import UIKit

public extension RoundedView {
    struct Style: Equatable {
        public var shadow: ShadowShapeView.Style?
        public var strokeWidth: CGFloat?
        public var strokeColor: UIColor?
        public var highlightedStrokeColor: UIColor?
        public var fillColor: UIColor
        public var highlightedFillColor: UIColor
        public var rounding: Rounding?

        public struct Rounding: Equatable {
            public let radius: CGFloat
            public let corners: UIRectCorner

            public init(radius: CGFloat, corners: UIRectCorner) {
                self.radius = radius
                self.corners = corners
            }
        }

        public init(
            shadowOpacity: Float? = nil,
            strokeWidth: CGFloat? = nil,
            strokeColor: UIColor? = nil,
            highlightedStrokeColor: UIColor? = nil,
            fillColor: UIColor,
            highlightedFillColor: UIColor,
            rounding: RoundedView.Style.Rounding? = nil
        ) {
            if let shadowOpacity {
                shadow = ShadowShapeView.Style(
                    shadowOpacity: shadowOpacity,
                    shadowColor: nil,
                    shadowRadius: nil,
                    shadowOffset: nil
                )
            } else {
                shadow = nil
            }
            self.strokeWidth = strokeWidth
            self.strokeColor = strokeColor
            self.highlightedStrokeColor = highlightedStrokeColor
            self.fillColor = fillColor
            self.highlightedFillColor = highlightedFillColor
            self.rounding = rounding
        }

        public init(
            shadow: ShadowShapeView.Style,
            strokeWidth: CGFloat? = nil,
            strokeColor: UIColor? = nil,
            highlightedStrokeColor: UIColor? = nil,
            fillColor: UIColor,
            highlightedFillColor: UIColor,
            rounding: RoundedView.Style.Rounding? = nil
        ) {
            self.shadow = shadow
            self.strokeWidth = strokeWidth
            self.strokeColor = strokeColor
            self.highlightedStrokeColor = highlightedStrokeColor
            self.fillColor = fillColor
            self.highlightedFillColor = highlightedFillColor
            self.rounding = rounding
        }
    }

    func apply(style: Style) {
        style.shadow.map { apply(style: $0) }
        style.strokeWidth.map { strokeWidth = $0 }
        style.strokeColor.map { strokeColor = $0 }
        style.highlightedStrokeColor.map { highlightedStrokeColor = $0 }

        fillColor = style.fillColor
        highlightedFillColor = style.highlightedFillColor

        style.rounding.map {
            roundingCorners = $0.corners
            cornerRadius = $0.radius
        }

        enableDynamicColorReapply()
    }
}
