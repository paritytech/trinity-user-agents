import SwiftUI

/// Where a pan's value goes under a finger and after it lets go. Values are in steps, velocities in steps a second.
enum WidgetPanPhysics {
    /// How far ahead a release looks along its velocity to pick where to rest.
    static let flingProjection = 0.3
    /// Capped so a hard flick still rests on the next step instead of swinging past it.
    static let maxReleaseSpeed = 4.0
    /// The settle a finger's release runs.
    static let releaseSpring = Spring(duration: 0.4, bounce: 0.15)

    /// The value under a finger that has moved `travel` points along the pan's axis since it took the pan at
    /// `start`, over a node `extent` points long on that axis. It stays within the steps one drag may move from
    /// `origin`, the step the drag counts from.
    static func dragged(
        from start: Double,
        origin: Int,
        travel: Double,
        extent: Double,
        pan: CustomMessageWidgetNode.Pan
    ) -> Double {
        clamped(start + steps(travel, extent: extent, pan: pan), around: origin, pan: pan)
    }

    /// `pointsPerSecond` along the axis, in steps a second.
    static func velocity(_ pointsPerSecond: Double, extent: Double, pan: CustomMessageWidgetNode.Pan) -> Double {
        steps(pointsPerSecond, extent: extent, pan: pan)
    }

    /// The step a release at `value` moving at `velocity` comes to rest on, never further from `rest` than the pan
    /// allows one drag to go. Halfway rounds up.
    static func restTarget(
        value: Double,
        velocity: Double,
        from rest: Int,
        pan: CustomMessageWidgetNode.Pan
    ) -> Int {
        let speed = min(max(velocity, -maxReleaseSpeed), maxReleaseSpeed)
        return Int(clamped(Double(nearest(value + speed * flingProjection)), around: rest, pan: pan))
    }

    /// The step nearest to `value`. Halfway rounds up, so both hosts settle a tie the same way.
    static func nearest(_ value: Double) -> Int {
        Int((value + 0.5).rounded(.down))
    }
}

/// One run of a pan's value from `start` to `target`, sampled by time since it began.
struct WidgetPanSettle {
    enum Curve {
        case spring(Spring)
        case ease(duration: Double)
    }

    let start: Double
    let target: Int
    let velocity: Double
    let curve: Curve

    var duration: Double {
        switch curve {
        case let .spring(spring): spring.settlingDuration
        case let .ease(duration): duration
        }
    }

    func value(at time: Double) -> Double {
        guard time < duration else { return Double(target) }

        switch curve {
        case let .spring(spring):
            return spring.value(fromValue: start, toValue: Double(target), initialVelocity: velocity, time: time)
        case let .ease(duration):
            let progress = UnitCurve.easeInEaseOut.value(at: time / duration)
            return start + (Double(target) - start) * progress
        }
    }
}

extension WidgetPanSettle.Curve {
    init(_ animation: CustomMessageWidgetNode.PanAnimation) {
        switch animation {
        case let .spring(duration, bounce):
            self = .spring(Spring(duration: duration, bounce: bounce))
        case let .ease(duration):
            self = .ease(duration: duration)
        }
    }
}

private extension WidgetPanPhysics {
    static func steps(_ points: Double, extent: Double, pan: CustomMessageWidgetNode.Pan) -> Double {
        let pointsPerStep = extent * pan.distance
        guard pointsPerStep != 0 else { return 0 }

        return points / pointsPerStep
    }

    static func clamped(_ value: Double, around rest: Int, pan: CustomMessageWidgetNode.Pan) -> Double {
        guard let maxSteps = pan.maxStepsPerGesture else { return value }

        return min(max(value, Double(rest - maxSteps)), Double(rest + maxSteps))
    }
}
