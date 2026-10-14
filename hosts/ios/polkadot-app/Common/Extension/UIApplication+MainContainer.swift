import UIKit
import UIKitExt

extension UIApplication {
    var mainTabBarController: MainTabBarViewController? {
        UIWindow.keyWindow?.rootViewController as? MainTabBarViewController
    }
}
