import PolkadotUI
import UIKit
import DesignSystem

protocol RootScreen: AnyObject {
    func setTitle(_ title: String)
}

extension RootScreen where Self: UIViewController {
    func setTitle(_ title: String) {
        let titleLabel: Label = .create { view in
            view.typography = .headlineSmall
            view.textColor = .fgPrimary
            view.text = title
        }

        setTitleView(titleLabel)
    }

    func setTitleView(_ view: UIView) {
        if #available(iOS 26.0, *) {
            navigationItem.titleView = view
            navigationItem.style = .browser
        } else {
            navigationItem.leftBarButtonItem = UIBarButtonItem(customView: view)
        }
    }

    var titleView: UIView? {
        if #available(iOS 26.0, *) {
            navigationItem.titleView
        } else {
            navigationItem.leftBarButtonItem?.customView
        }
    }
}
