import CoreGraphics
import Foundation

/// The samples a drag records as the finger moves. Its release velocity and flick speed are read from them.
struct DockletDragSamples {
    struct Sample {
        let time: TimeInterval
        let progress: CGFloat
        let finger: CGPoint
    }

    /// Span, in seconds, of the samples whose slope gives a release's velocity.
    private static let velocityWindowSeconds: TimeInterval = 0.1
    /// Slowest, in points per second, that a trailing sample may move and still count toward the velocity.
    private static let trailingSpeedPoints: CGFloat = 150

    private var samples: [Sample]

    init(first: Sample) {
        samples = [first]
    }

    mutating func append(_ sample: Sample) {
        samples.append(sample)
    }

    /// Velocity in progress per second, from the slope over the last window of the moving samples. Trailing samples
    /// that barely moved are dropped first, so a pause before release keeps the velocity of the flick.
    func velocity(travel: CGFloat) -> CGFloat {
        let slowestSpeed = Self.trailingSpeedPoints / travel
        var moving = samples
        while moving.count >= 2 {
            let later = moving[moving.count - 1]
            let earlier = moving[moving.count - 2]
            let isSlow = abs(later.progress - earlier.progress) < slowestSpeed * CGFloat(later.time - earlier.time)
            guard isSlow else {
                break
            }
            moving.removeLast()
        }
        guard let span = Self.span(of: moving) else {
            return 0
        }
        return (span.last.progress - span.first.progress) / CGFloat(span.last.time - span.first.time)
    }

    /// Speed of the finger along `direction`, in points a second, over the same window the velocity uses. It is
    /// positive when the finger moves the way `direction` points.
    func speed(along direction: CGVector) -> CGFloat {
        guard let span = Self.span(of: samples) else {
            return 0
        }
        let first = span.first.finger
        let last = span.last.finger
        let length = hypot(direction.dx, direction.dy)
        let movedAlong = ((last.x - first.x) * direction.dx + (last.y - first.y) * direction.dy) / length
        return movedAlong / CGFloat(span.last.time - span.first.time)
    }
}

private extension DockletDragSamples {
    /// The first and last samples of the window that ends at the last sample, or nil when it holds fewer than two.
    static func span(of samples: [Sample]) -> (first: Sample, last: Sample)? {
        guard let last = samples.last else {
            return nil
        }
        let window = samples.filter { last.time - $0.time <= velocityWindowSeconds }
        guard let first = window.first, window.count >= 2, last.time > first.time else {
            return nil
        }
        return (first, last)
    }
}
