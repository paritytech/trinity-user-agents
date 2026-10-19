import UIKit

@MainActor
protocol ModalPresenting: AnyObject {
    var presentedViewController: UIViewController? { get }
    func dismiss(animated flag: Bool, completion: (() -> Void)?)
}

extension UIViewController: ModalPresenting {}

@MainActor
enum PresentedPageDismissal {
    static func dismissingPresented(
        on presenter: ModalPresenting?,
        unless isPinned: (UIViewController) -> Bool = { _ in false },
        then navigate: @escaping () -> Void
    ) {
        guard let presenter, let presented = presenter.presentedViewController else {
            navigate()
            return
        }

        let presentedChain = sequence(first: presented, next: \.presentedViewController)
        guard !presentedChain.contains(where: isPinned) else {
            navigate()
            return
        }

        presenter.dismiss(animated: false, completion: navigate)
    }
}
