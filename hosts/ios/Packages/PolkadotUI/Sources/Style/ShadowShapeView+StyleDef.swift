import Foundation
public import UIKit_iOS
import UIKit

public extension ShadowShapeView {
    struct Style: Equatable {
        public let shadowOpacity: Float?
        public let shadowColor: UIColor?
        public let shadowRadius: CGFloat?
        public let shadowOffset: CGSize?

        public init(
            shadowOpacity: Float?,
            shadowColor: UIColor?,
            shadowRadius: CGFloat?,
            shadowOffset: CGSize?
        ) {
            self.shadowOpacity = shadowOpacity
            self.shadowColor = shadowColor
            self.shadowRadius = shadowRadius
            self.shadowOffset = shadowOffset
        }
    }

    func apply(style: Style) {
        style.shadowOpacity.map { shadowOpacity = $0 }
        style.shadowColor.map { shadowColor = $0 }
        style.shadowRadius.map { shadowRadius = $0 }
        style.shadowOffset.map { shadowOffset = $0 }
    }
}
