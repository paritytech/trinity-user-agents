import UIKit

public extension UIView {
    /// Renders the part of the view's current on-screen hierarchy inside `rect`, in the view's own coordinates.
    ///
    /// Returns `nil` when the hierarchy cannot be drawn, for example while the view is off screen.
    func snapshot(of rect: CGRect) -> UIImage? {
        var captured = false
        let image = UIGraphicsImageRenderer(size: rect.size).image { _ in
            captured = drawHierarchy(
                in: CGRect(origin: CGPoint(x: -rect.minX, y: -rect.minY), size: bounds.size),
                afterScreenUpdates: false
            )
        }
        return captured ? image : nil
    }
}
