import UIKit

public extension UINavigationController {
    /// Turns the stack's own back swipes off or on, for screens that play their own close instead of letting the
    /// swipe slide them away.
    func setSystemBackSwipesEnabled(_ enabled: Bool) {
        interactivePopGestureRecognizer?.isEnabled = enabled
        if #available(iOS 26, *) {
            interactiveContentPopGestureRecognizer?.isEnabled = enabled
        }
    }
}
