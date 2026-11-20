import UIKit

/// Relays display link frames to a closure. A display link retains its target, so the link is given this object
/// rather than the controller.
@MainActor
final class DisplayLinkFrameTarget: NSObject {
    private let onFrame: (CADisplayLink) -> Void

    init(onFrame: @escaping (CADisplayLink) -> Void) {
        self.onFrame = onFrame
    }

    @objc func frame(_ link: CADisplayLink) {
        onFrame(link)
    }
}
