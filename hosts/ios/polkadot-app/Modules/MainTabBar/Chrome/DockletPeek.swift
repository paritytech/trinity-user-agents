import CoreGraphics
import Foundation

/// The peek that shows a tucked Docklet is still there: the bar turns out along its hide path and back, so its top
/// end shows past the edge. It plays once per session, after the session's first tuck has landed. Time is passed in,
/// so the type reads no clock.
struct DockletPeek {
    /// Seconds from the tuck landing to the start of the peek.
    static let delaySeconds: TimeInterval = 0.7
    /// Seconds the peek takes to go out and back.
    static let durationSeconds: TimeInterval = 0.4
    /// Furthest the peek takes the hide progress back from tucked.
    static let depth: CGFloat = 0.14

    /// Set by the session's first tuck. It stays set after the peek plays or is cancelled, so the peek never repeats.
    private(set) var isArmed = false
    private var startedAt: TimeInterval?

    var isPlaying: Bool {
        startedAt != nil
    }

    /// Arms the peek. Returns false when an earlier tuck already armed it.
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

    /// Ends the peek at once, whether it is playing or not.
    mutating func stop() {
        startedAt = nil
    }

    /// Whether the peek has finished, or never started, at `time`.
    func isOver(at time: TimeInterval) -> Bool {
        guard let startedAt else {
            return true
        }
        return time - startedAt >= Self.durationSeconds
    }

    /// How far the hide progress is taken back from tucked at `time`. Zero when the peek is not playing.
    func progressDip(at time: TimeInterval) -> CGFloat {
        guard let startedAt else {
            return 0
        }
        return Self.progressDip(elapsed: time - startedAt)
    }

    /// The dip `elapsed` seconds into a peek: out and back on one cosine period, deepest at the midpoint.
    static func progressDip(elapsed: TimeInterval) -> CGFloat {
        let share = CGFloat(elapsed / durationSeconds)
        guard share > 0, share < 1 else {
            return 0
        }
        return depth * (1 - cos(2 * .pi * share)) / 2
    }
}
