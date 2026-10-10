import UIKit

/// Holds the bar's hosted view at a fixed size and accepts touches only in the live region at its bottom.
/// The hosted view is never resized, so its coordinate space stays still while panels animate.
final class TabBarBarHostView: UIView {
    /// Height of the live region at the bottom, the bar row plus whatever the open panel adds.
    /// Touches above it pass through, so the oversized host does not swallow the app's content.
    var liveHeight: CGFloat = 0

    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        guard liveHeight > 0, bounds.contains(point), point.y >= bounds.height - liveHeight else {
            return nil
        }
        return super.hitTest(point, with: event)
    }
}
