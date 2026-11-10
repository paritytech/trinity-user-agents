import UIKit

enum TabBarReselectionAction: Equatable {
    case popToRoot(UINavigationController)
    case scrollToTop(UIViewController)
    case ignore
}

@MainActor
enum TabBarReselectionPolicy {
    static func action(for controller: UIViewController?) -> TabBarReselectionAction {
        let navigation = controller as? UINavigationController
        let target = navigation?.topViewController ?? controller

        guard let target, target.presentedViewController == nil else {
            return .ignore
        }

        if let navigation, navigation.viewControllers.count > 1 {
            return .popToRoot(navigation)
        }

        return .scrollToTop(target)
    }
}
