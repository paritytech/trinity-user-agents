import UIKit

/// The chrome's full-bleed root: transparent to touches except the folded bar's grab zone and,
/// while a panel is open, the area outside it that dismisses on tap.
final class TabBarChromePassthroughView: UIView {
    var isOutsideTapEnabled = false
    var foldGrabZone: CGRect = .zero

    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        let hitView = super.hitTest(point, with: event)

        guard hitView === self else {
            return hitView
        }

        if !foldGrabZone.isEmpty, foldGrabZone.contains(point) {
            return self
        }
        return isOutsideTapEnabled ? self : nil
    }
}
