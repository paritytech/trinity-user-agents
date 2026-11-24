import SwiftUI

public extension CustomMessageWidgetNode {
    struct GestureProps {
        public var tapAction: String?
        public var pan: PanGesture?
        public var accessibilityLabel: String?
        public var isEnabled = true
        public var hapticOnComplete = false
        /// Names the gesture for offset updates; a keyed pan's moves are paced per display frame.
        public var key: String?
        /// How the children move when pressed and released; nil keeps the host's own.
        public var feel: PanFeel?
        public let encodePayload: (GesturePhase, CGPoint, UInt16) -> Data

        public init(encodePayload: @escaping (GesturePhase, CGPoint, UInt16) -> Data) {
            self.encodePayload = encodePayload
        }
    }

    enum GesturePhase {
        case tap
        case started
        case moved
        case ended
        case cancelled
        case completed
    }

    enum PanAxis {
        case horizontal
        case vertical
        case free
    }

    struct PanGesture {
        public var axis: PanAxis = .horizontal
        public var drag: PanDrag?
        public var startAction: String?
        public var moveAction: String?
        public var endAction: String?
        public var moveInterval: TimeInterval = 0
        /// Events carry where the finger is inside the gesture (clamped to its frame) instead of how far it moved.
        public var reportsLocation = false

        public init() {}
    }

    struct PanDrag {
        public var travelPoints: CGFloat?
        public var release: PanRelease = .snapBack
        public var holdAtEnd = true

        public init() {}
    }

    enum PanRelease {
        case snapBack
        case stay
        case complete(thresholdPercent: UInt8)
    }
}

public extension CustomMessageWidgetNode {
    /// How a gesture's children move when pressed and released, by value (the renderer's `PanFeel`).
    struct PanFeel: Equatable {
        public var pressScale: CGFloat = 1
        /// Scale down the y axis while pressed; nil scales both axes by `pressScale`.
        public var pressScaleY: CGFloat?
        public var pressDuration: TimeInterval = 0
        public var unpressDuration: TimeInterval = 0
        /// The return spring, from rest: angular frequency (radians a second) and damping ratio.
        public var springOmega: CGFloat = 0
        public var springDamping: CGFloat = 1
        public var approachDuration: TimeInterval = 0
        public var approachReference: CGFloat = 1
        /// Scales after completion, one per 1/60 s.
        public var pop: [CGFloat] = []
        public var popOffset: CGFloat = 0

        public init() {}
    }
}
