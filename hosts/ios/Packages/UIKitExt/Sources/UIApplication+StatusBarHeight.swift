import UIKit.UIApplication

public extension UIApplication {
    /// A static property that computes the height of the status bar for the active window scene.
    ///
    /// - Returns: The height of the status bar for the active window scene
    /// if it's available, otherwise returns 0.
    ///
    /// This implementation takes into account the deprecation of `windows` in iOS 15.0
    /// and uses the recommended `UIWindowScene.windows` instead.
    static var statusBarHeight: CGFloat {
        guard let windowScene = UIApplication.shared.connectedScenes
            .first(where: { $0.activationState == .foregroundActive }) as? UIWindowScene
        else {
            return 0
        }
        return windowScene.statusBarManager?.statusBarFrame.height ?? 0
    }
}
