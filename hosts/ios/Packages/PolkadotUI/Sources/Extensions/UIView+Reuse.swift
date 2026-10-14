import UIKit

extension UIView {
    static var reuseIdentifier: String {
        NSStringFromClass(self)
    }
}
