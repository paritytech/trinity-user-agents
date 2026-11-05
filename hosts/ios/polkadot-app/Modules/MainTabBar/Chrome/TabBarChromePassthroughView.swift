import UIKit

/// The chrome's full-bleed root: transparent to touches except the tucked bar's grab zone and,
/// while a panel is open, the area outside it that dismisses on tap.
final class TabBarChromePassthroughView: UIView {
    var isOutsideTapEnabled = false
    var foldGrabZone: CGRect = .zero
    /// Reports a touch that begins in the grab zone.
    var onGrabZoneTouchBegan: (() -> Void)?
    /// Reports the end or cancellation of the touch that began in the grab zone.
    var onGrabZoneTouchEnded: (() -> Void)?
    private weak var grabZoneTouch: UITouch?

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

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesBegan(touches, with: event)
        guard let touch = touchInGrabZone(among: touches) else {
            return
        }
        grabZoneTouch = touch
        onGrabZoneTouchBegan?()
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesEnded(touches, with: event)
        releaseGrabZoneTouch(among: touches)
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesCancelled(touches, with: event)
        releaseGrabZoneTouch(among: touches)
    }
}

private extension TabBarChromePassthroughView {
    func touchInGrabZone(among touches: Set<UITouch>) -> UITouch? {
        guard !foldGrabZone.isEmpty else {
            return nil
        }
        return touches.first { foldGrabZone.contains($0.location(in: self)) }
    }

    func releaseGrabZoneTouch(among touches: Set<UITouch>) {
        guard let grabZoneTouch, touches.contains(grabZoneTouch) else {
            return
        }
        self.grabZoneTouch = nil
        onGrabZoneTouchEnded?()
    }
}
