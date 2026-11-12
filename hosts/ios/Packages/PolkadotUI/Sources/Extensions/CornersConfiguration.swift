import UIKit

public struct CornersConfiguration {
    public var topLeft: CGFloat
    public var topRight: CGFloat
    public var bottomRight: CGFloat
    public var bottomLeft: CGFloat

    public init(_ radius: CGFloat) {
        topLeft = radius
        topRight = radius
        bottomRight = radius
        bottomLeft = radius
    }

    public init(
        topLeft: CGFloat = 0,
        topRight: CGFloat = 0,
        bottomRight: CGFloat = 0,
        bottomLeft: CGFloat = 0
    ) {
        self.topLeft = topLeft
        self.topRight = topRight
        self.bottomRight = bottomRight
        self.bottomLeft = bottomLeft
    }

    public mutating func set(_ corners: UIRectCorner, _ radius: CGFloat) {
        if corners.contains(.topLeft) { topLeft = radius }
        if corners.contains(.topRight) { topRight = radius }
        if corners.contains(.bottomRight) { bottomRight = radius }
        if corners.contains(.bottomLeft) { bottomLeft = radius }
    }

    public func corners(_ corners: UIRectCorner, _ radius: CGFloat) -> CornersConfiguration {
        var copy = self
        copy.set(corners, radius)
        return copy
    }

    public static func all(_ radius: CGFloat) -> Self {
        .init(radius)
    }

    var allEqual: Bool {
        topLeft == topRight &&
            topRight == bottomRight &&
            bottomRight == bottomLeft
    }
}

public extension CornersConfiguration {
    static var zero: CornersConfiguration { .all(0) }

    static var evidenceMedia: CornersConfiguration {
        .all(16)
            .corners(.bottomRight, 4)
    }
}

extension CornersConfiguration: Equatable {
    public static func == (lhs: CornersConfiguration, rhs: CornersConfiguration) -> Bool {
        lhs.topLeft == rhs.topLeft
            && lhs.topRight == rhs.topRight
            && lhs.bottomRight == rhs.bottomRight
            && lhs.bottomLeft == rhs.bottomLeft
    }
}
