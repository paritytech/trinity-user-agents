import SwiftUI

public extension CustomMessageWidgetNode {
    struct GestureProps {
        public var tapAction: String?
        public var pan: PanGesture?
        public var accessibilityLabel: String?
        public var isEnabled = true
        public var hapticOnComplete = false
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
