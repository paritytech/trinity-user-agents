import UIKit

extension UIViewController {
    var isEnteringScreen: Bool {
        isMovingToParent || navigationController?.isMovingToParent == true
            || isBeingPresented || navigationController?.isBeingPresented == true
    }
}
