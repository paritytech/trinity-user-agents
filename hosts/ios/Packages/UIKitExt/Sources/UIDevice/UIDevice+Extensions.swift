import UIKit

public extension UIDevice {
    class var hasHomeButton: Bool {
        current.hasHomeButton
    }

    var hasHomeButton: Bool {
        guard let window = UIWindow.topWindow else { return false }

        return window.safeAreaInsets.bottom == .zero
    }
}
