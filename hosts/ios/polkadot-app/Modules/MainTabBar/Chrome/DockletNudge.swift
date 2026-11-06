import CoreGraphics
import Foundation

/// The nudge that shows a tucked Docklet is still there: it moves 8 pt in from the edge and back. It plays once per
/// session, after the session's first tuck has landed. Time is passed in, so the type reads no clock.
struct DockletNudge {
    /// Seconds from the tuck landing to the start of the nudge.
    static let delaySeconds: TimeInterval = 0.7
    /// Seconds the nudge takes to go in and back out.
    static let durationSeconds: TimeInterval = 0.4
    /// Furthest the nudge moves the bar in from its tucked pose.
    static let peakInset: CGFloat = 8
    /// Share of the nudge at which it reaches its peak.
    private static let peakProgress: CGFloat = 0.5

    /// Set by the session's first tuck. It stays set after the nudge plays or is cancelled, so the nudge never repeats.
    private(set) var isArmed = false
    private var startedAt: TimeInterval?

    var isPlaying: Bool {
        startedAt != nil
    }

    /// Arms the nudge. Returns false when an earlier tuck already armed it.
    mutating func arm() -> Bool {
        guard !isArmed else {
            return false
        }
        isArmed = true
        return true
    }

    mutating func start(at time: TimeInterval) {
        startedAt = time
    }

    /// Ends the nudge at once, whether it is playing or not.
    mutating func stop() {
        startedAt = nil
    }

    /// Whether the nudge has finished, or never started, at `time`.
    func isOver(at time: TimeInterval) -> Bool {
        guard let startedAt else {
            return true
        }
        return time - startedAt >= Self.durationSeconds
    }

    /// How far the bar sits in from its tucked pose at `time`. Zero when the nudge is not playing.
    func inset(at time: TimeInterval) -> CGFloat {
        guard let startedAt else {
            return 0
        }
        return Self.inset(elapsed: time - startedAt)
    }

    /// The inset `elapsed` seconds into a nudge. It is zero at the start and the end, peaks at the midpoint, and eases
    /// in and out on each half.
    static func inset(elapsed: TimeInterval) -> CGFloat {
        let progress = CGFloat(elapsed / durationSeconds)
        guard progress > 0, progress < 1 else {
            return 0
        }
        let eased = progress < peakProgress
            ? (progress / peakProgress).smoothstepped
            : ((1 - progress) / (1 - peakProgress)).smoothstepped
        return peakInset * eased
    }
}
