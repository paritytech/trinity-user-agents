import UIKit

public extension UIAccessibilityIdentification {
    func accessibilityId(_ id: (any AccessibilityIdentifying)?) {
        accessibilityIdentifier = id?.rawValue
    }

    func accessibilityId(rawValue: String?) {
        accessibilityIdentifier = rawValue
    }
}
