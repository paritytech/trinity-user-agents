import CoreMotion
import QuartzCore
import SwiftUI

public extension CustomMessageWidgetNode {
    /// Moves a node by the device's tilt, a keyed pan's children, or both, scaled by its depth (the renderer's
    /// `Parallax`).
    struct Parallax: Equatable {
        /// Follows the device's tilt (the renderer's `TiltParallax`).
        public struct Tilt: Equatable {
            /// Points, the move at the clamp.
            public let travel: CGFloat
            /// Degrees, the tilt that moves the node all the way; zero never moves.
            public let maxTilt: Double
            /// The time constant with which the rest attitude follows the device's; zero keeps it.
            public let settle: TimeInterval

            public init(travel: CGFloat, maxTilt: Double, settle: TimeInterval) {
                self.travel = travel
                self.maxTilt = maxTilt
                self.settle = settle
            }
        }

        /// 1 moves with the sources, 0 not at all, negative the other way.
        public let depth: Double
        public let tilt: Tilt?
        /// The key of the gesture whose children's offset is followed.
        public let pan: String?

        public init(depth: Double, tilt: Tilt? = nil, pan: String? = nil) {
            self.depth = depth
            self.tilt = tilt
            self.pan = pan
        }

        /// The node's move along the layout direction and down, for a `tilt` from the rest attitude in degrees (end
        /// edge lowered, top edge lowered) and the pan's children drawn at `panOffset` (`Parallax::offset`).
        public func offset(tilt: CGVector, panOffset: CGSize) -> CGSize {
            var tilted = CGSize.zero
            if let follow = self.tilt, follow.maxTilt > 0 {
                let along = { (degrees: CGFloat) in follow.travel * min(max(degrees / follow.maxTilt, -1), 1) }
                tilted = CGSize(width: along(tilt.dx), height: along(tilt.dy))
            }
            let pan = pan == nil ? .zero : panOffset
            return CGSize(width: depth * (tilted.width + pan.width), height: depth * (tilted.height + pan.height))
        }
    }
}

/// The device's tilt from a rest attitude, in degrees: right edge lowered and top edge lowered, from gravity. The rest
/// attitude is the device's when a node appeared, and follows the device's own with a time constant.
struct TiltRest: Equatable {
    var rest: CGVector?

    /// The tilt now, from `current` (degrees, right edge and top edge lowered), with the rest moved `elapsed` towards
    /// it on a time constant of `settle` (zero keeps it).
    mutating func tilt(current: CGVector, elapsed: TimeInterval, settle: TimeInterval) -> CGVector {
        guard var rest else {
            rest = current
            return .zero
        }
        if settle > 0 {
            let share = 1 - exp(-max(elapsed, 0) / settle)
            rest.dx += (current.dx - rest.dx) * share
            rest.dy += (current.dy - rest.dy) * share
            self.rest = rest
        }
        return CGVector(dx: current.dx - rest.dx, dy: current.dy - rest.dy)
    }

    /// Degrees of tilt read from gravity in the device's frame (x towards its right edge, y towards its top, z out of
    /// the screen): across, the turn in the screen's plane; down, the turn of the top edge towards the ground.
    static func degrees(gravity: (x: Double, y: Double, z: Double)) -> CGVector {
        let across = asin(min(max(gravity.x, -1), 1))
        let down = atan2(gravity.y, -gravity.z)
        return CGVector(dx: across * 180 / .pi, dy: down * 180 / .pi)
    }
}

/// One reader of the device's motion for every node that follows the tilt, running while any of them is shown.
@MainActor
final class DeviceTilt {
    static let shared = DeviceTilt()

    private let manager = CMMotionManager()
    private var readers = 0

    var isAvailable: Bool { manager.isDeviceMotionAvailable }

    /// Degrees, right edge and top edge lowered; nil before the first reading or with no motion to read.
    var current: CGVector? {
        guard let gravity = manager.deviceMotion?.gravity else { return nil }
        return TiltRest.degrees(gravity: (gravity.x, gravity.y, gravity.z))
    }

    func start() {
        readers += 1
        guard readers == 1, manager.isDeviceMotionAvailable else { return }
        manager.deviceMotionUpdateInterval = 1.0 / 60
        manager.startDeviceMotionUpdates()
    }

    func stop() {
        readers = max(readers - 1, 0)
        if readers == 0 { manager.stopDeviceMotionUpdates() }
    }
}

/// A node's rest attitude and when it was last moved, kept across frames without drawing anything.
final class TiltTracker {
    var rest = TiltRest()
    var last: TimeInterval?

    func tilt(current: CGVector, at now: TimeInterval, settle: TimeInterval) -> CGVector {
        let elapsed = last.map { now - $0 } ?? 0
        last = now
        return rest.tilt(current: current, elapsed: elapsed, settle: settle)
    }
}

/// Moves a node by its parallax, after layout, as `Translate` does. The pan's part follows the gesture's children
/// however they move, animated with them; the tilt's part is read each frame, and adds nothing where the person asked
/// for less motion or the device has no motion to read.
struct NodeParallaxModifier: ViewModifier {
    let parallax: CustomMessageWidgetNode.Parallax?

    @Environment(\.gestureOffsets) private var offsets
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.layoutDirection) private var layoutDirection

    func body(content: Content) -> some View {
        if let parallax {
            let pan = parallax.offset(tilt: .zero, panOffset: Self.panOffset(key: parallax.pan, offsets: offsets))
            content
                .modifier(TiltParallaxModifier(
                    parallax: parallax, follows: !reduceMotion, mirrored: layoutDirection == .rightToLeft
                ))
                .offset(x: onScreen(pan.width), y: pan.height)
        } else {
            content
        }
    }

    /// Where the keyed gesture's children are drawn, along the layout direction and down: the host's drag and the
    /// worker's offset. A key that names no gesture adds nothing.
    @MainActor
    static func panOffset(key: String?, offsets: GestureOffsets?) -> CGSize {
        guard let key, let offsets else { return .zero }
        let drag = offsets.place(for: key).dragOffset
        let set = offsets.channel(for: key).offset ?? .zero
        return CGSize(width: drag.x + set.x, height: drag.y + set.y)
    }

    private func onScreen(_ along: CGFloat) -> CGFloat {
        layoutDirection == .rightToLeft ? -along : along
    }
}

private struct TiltParallaxModifier: ViewModifier {
    let parallax: CustomMessageWidgetNode.Parallax
    let follows: Bool
    let mirrored: Bool

    @State private var tracker = TiltTracker()

    func body(content: Content) -> some View {
        if let tilt = parallax.tilt, follows, tilt.maxTilt > 0, DeviceTilt.shared.isAvailable {
            TimelineView(.animation) { _ in
                let move = move(tilt)
                content.offset(x: mirrored ? -move.width : move.width, y: move.height)
            }
            .onAppear { DeviceTilt.shared.start() }
            .onDisappear { DeviceTilt.shared.stop() }
        } else {
            content
        }
    }

    /// The tilt's part of the move, along the layout direction: lowering the end edge moves the node towards the end.
    private func move(_ tilt: CustomMessageWidgetNode.Parallax.Tilt) -> CGSize {
        guard let current = DeviceTilt.shared.current else { return .zero }
        let measured = tracker.tilt(current: current, at: CACurrentMediaTime(), settle: tilt.settle)
        let along = CGVector(dx: mirrored ? -measured.dx : measured.dx, dy: measured.dy)
        return CustomMessageWidgetNode.Parallax(depth: parallax.depth, tilt: tilt).offset(tilt: along, panOffset: .zero)
    }
}
