import UIKit

@MainActor
public protocol TopmostChildProviding: AnyObject {
    var topmostChild: UIViewController? { get }
}

public extension UIWindow {
    class var topWindow: UIWindow? {
        UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap(\.windows)
            .first
    }

    class var keyWindow: UIWindow? {
        UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap(\.windows)
            .first(where: \.isKeyWindow)
    }

    var topmostViewController: UIViewController? {
        rootViewController?.topmostPresented
    }
}
