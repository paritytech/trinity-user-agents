import UIKit

public protocol ToastDisplayable {
    func showToast(
        message: String,
        type: ToastView.ToastType,
        duration: TimeInterval
    )
}

private var currentToastKey = "currentToastKey"

public extension ToastDisplayable where Self: UIViewController {
    private var currentToast: ToastView? {
        get {
            withUnsafeMutablePointer(to: &currentToastKey) {
                objc_getAssociatedObject(self, $0) as? ToastView
            }
        }
        set {
            withUnsafeMutablePointer(to: &currentToastKey) {
                objc_setAssociatedObject(
                    self,
                    $0,
                    newValue,
                    .OBJC_ASSOCIATION_RETAIN_NONATOMIC
                )
            }
        }
    }

    func showToast(
        message: String,
        type: ToastView.ToastType,
        duration: TimeInterval = 2.0
    ) {
        currentToast?.hide()

        let toast = ToastView(message: message, type: type)
        currentToast = toast
        toast.show(in: view, duration: duration)
    }
}

extension UIViewController: ToastDisplayable {}
