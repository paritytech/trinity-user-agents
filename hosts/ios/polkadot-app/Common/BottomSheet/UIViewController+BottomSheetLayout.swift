import UIKit

extension UIViewController {
    func updateBottomSheetLayout(
        animated: Bool = true,
        updates: (() -> Void)? = nil
    ) {
        guard let presentationController else {
            updates?()
            return
        }

        let perform = { [weak self] in
            updates?()
            self?.view.layoutIfNeeded()
            self?.view.frame = presentationController.frameOfPresentedViewInContainerView
        }

        if animated {
            UIView.animate(
                withDuration: 0.25,
                delay: 0,
                options: [.curveEaseInOut],
                animations: perform
            )
        } else {
            perform()
        }
    }
}
