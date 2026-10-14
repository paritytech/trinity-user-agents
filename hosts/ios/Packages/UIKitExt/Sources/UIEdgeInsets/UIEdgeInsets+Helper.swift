import UIKit.UIGeometry

public extension UIEdgeInsets {
    init(horizontal: CGFloat = 0, vertical: CGFloat = 0) {
        self.init(top: vertical, left: horizontal, bottom: vertical, right: horizontal)
    }

    init(
        top: CGFloat = 0,
        bottom: CGFloat = 0,
        left: CGFloat = 0,
        right: CGFloat = 0
    ) {
        self.init(top: top, left: left, bottom: bottom, right: right)
    }

    static func top(_ value: CGFloat) -> UIEdgeInsets {
        UIEdgeInsets(top: value, left: 0, bottom: 0, right: 0)
    }

    static func bottom(_ value: CGFloat) -> UIEdgeInsets {
        UIEdgeInsets(top: 0, left: 0, bottom: value, right: 0)
    }

    static func left(_ value: CGFloat) -> UIEdgeInsets {
        UIEdgeInsets(top: 0, left: value, bottom: 0, right: 0)
    }

    static func right(_ value: CGFloat) -> UIEdgeInsets {
        UIEdgeInsets(top: 0, left: 0, bottom: 0, right: value)
    }

    func top(_ value: CGFloat) -> UIEdgeInsets {
        UIEdgeInsets(top: value, left: left, bottom: bottom, right: right)
    }

    func bottom(_ value: CGFloat) -> UIEdgeInsets {
        UIEdgeInsets(top: top, left: left, bottom: value, right: right)
    }

    func left(_ value: CGFloat) -> UIEdgeInsets {
        UIEdgeInsets(top: top, left: value, bottom: bottom, right: right)
    }

    func right(_ value: CGFloat) -> UIEdgeInsets {
        UIEdgeInsets(top: top, left: left, bottom: bottom, right: value)
    }
}
