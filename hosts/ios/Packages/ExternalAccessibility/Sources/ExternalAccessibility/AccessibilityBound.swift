import UIKit

public protocol AccessibilityBound: UIView {
    var accessibilityBindings: [AccessibilityBinding] { get }
}

public struct AccessibilityBinding {
    let element: any UIAccessibilityIdentification
    let id: any AccessibilityIdentifying

    public init(_ element: any UIAccessibilityIdentification, _ id: any AccessibilityIdentifying) {
        self.element = element
        self.id = id
    }
}

public extension AccessibilityBound {
    func applyAccessibilityBindings() {
        accessibilityBindings.forEach { $0.element.accessibilityId($0.id) }
    }
}
