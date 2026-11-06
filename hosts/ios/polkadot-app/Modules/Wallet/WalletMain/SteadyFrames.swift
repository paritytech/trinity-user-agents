import UIKit

/// Tells when the display has settled. The shortest of the first frame intervals is the baseline, and the screen is
/// steady once a run of intervals stays within `slack` times that baseline.
struct SteadyFrameRule {
    static let sampledFrames = 3
    static let slack: TimeInterval = 1.5

    private var shortestInterval = TimeInterval.infinity
    private var samples = 0
    private var steadyRun = 0

    /// Records one interval toward the baseline, and returns true once the baseline is known.
    mutating func sample(_ interval: TimeInterval) -> Bool {
        shortestInterval = min(shortestInterval, interval)
        samples += 1
        return samples >= Self.sampledFrames
    }

    /// Counts one interval toward steadiness, and returns true once enough intervals in a row are steady. Until the
    /// baseline is known, the interval is recorded as a sample instead.
    mutating func isSteady(after interval: TimeInterval) -> Bool {
        guard samples >= Self.sampledFrames else {
            _ = sample(interval)
            return false
        }

        steadyRun = interval <= shortestInterval * Self.slack ? steadyRun + 1 : 0
        return steadyRun >= Self.sampledFrames
    }
}

/// Follows display frames until a closure says the intervals between them have settled, or until a time cap passes.
@MainActor
final class FrameIntervalWatch {
    private var link: CADisplayLink?
    private var firstTimestamp: CFTimeInterval = 0
    private var previousTimestamp: CFTimeInterval?
    private var cap: TimeInterval = .infinity
    private var isSettled: (TimeInterval) -> Bool = { _ in false }
    private var completion: (() -> Void)?

    /// Calls `completion` once, when `isSettled` returns true for an interval or `cap` seconds have passed. Replaces
    /// any watch under way without calling its completion.
    func watch(
        cap: TimeInterval?,
        until isSettled: @escaping (TimeInterval) -> Bool,
        then completion: @escaping () -> Void
    ) {
        cancel()
        previousTimestamp = nil
        self.cap = cap ?? .infinity
        self.isSettled = isSettled
        self.completion = completion

        let target = DisplayLinkFrameTarget { [weak self] link in
            self?.tick(link.timestamp)
        }
        let displayLink = CADisplayLink(target: target, selector: #selector(DisplayLinkFrameTarget.frame(_:)))
        displayLink.add(to: .main, forMode: .common)
        link = displayLink
    }

    /// Stops the watch without calling its completion.
    func cancel() {
        link?.invalidate()
        link = nil
        completion = nil
    }

    private func tick(_ timestamp: CFTimeInterval) {
        guard let last = previousTimestamp else {
            firstTimestamp = timestamp
            previousTimestamp = timestamp
            return
        }

        previousTimestamp = timestamp
        let settled = isSettled(timestamp - last)
        guard settled || timestamp - firstTimestamp >= cap else { return }

        let finished = completion
        cancel()
        finished?()
    }
}
