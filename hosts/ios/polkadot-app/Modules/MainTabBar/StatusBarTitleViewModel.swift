import UIKit

struct StatusBarTitleViewModel {
    let title: String
    let showsLogo: Bool
    let menu: UIMenu?
}

@MainActor
protocol StatusBarTitleProviding: AnyObject {
    var statusBarTitle: String? { get }
    var onStatusBarTitleChanged: (() -> Void)? { get set }

    func makeStatusBarMenu() -> UIMenu
}
