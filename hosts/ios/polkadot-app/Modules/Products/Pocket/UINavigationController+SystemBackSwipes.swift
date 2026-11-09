import UIKit

extension UINavigationController {
    /// Pocket item screens turn the stack's own back swipes off while they show, since those would slide the
    /// screen away rather than play the item's close.
    func setSystemBackSwipesEnabled(_ enabled: Bool) {
        interactivePopGestureRecognizer?.isEnabled = enabled
        if #available(iOS 26, *) {
            interactiveContentPopGestureRecognizer?.isEnabled = enabled
        }
    }
}
