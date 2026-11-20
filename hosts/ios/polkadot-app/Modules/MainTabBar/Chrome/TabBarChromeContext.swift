import UIKit

struct TabBarChromeContext {
    let tabController: UIViewController?
    let contentController: UIViewController?
    let foldDerived: TabBarFoldDerived
    let screen: UIViewController?
}

extension TabBarChromeContext {
    static func spa(_ controller: UIViewController) -> TabBarChromeContext {
        TabBarChromeContext(
            tabController: controller,
            contentController: controller,
            foldDerived: .tucked,
            screen: controller
        )
    }
}
