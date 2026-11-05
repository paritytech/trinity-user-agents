import Foundation

/// A reason the Docklet stays shown. While any holds, the auto-hide does nothing.
enum DockletHideBlocker: Hashable {
    /// A search, scan or shelf panel is open.
    case panelOpen
    /// The keyboard is up.
    case keyboardUp
    /// A drag or a settle is moving the Docklet.
    case motionRunning
    /// The Docklet is not shown, so it is already tucked or hidden.
    case notShown
    /// A tip about the tucked Docklet is on screen.
    case tipShown
    /// VoiceOver is running. The auto-hide never tucks the Docklet while it is on.
    case voiceOver
}

/// When the Docklet tucks itself. The first outside touch tucks it at once. Every touch and key press restarts an
/// idle countdown, and the Docklet tucks when the countdown runs out. Times are passed in, so the type reads no clock.
struct DockletAutoHide {
    /// Seconds without a touch or key press after which the Docklet tucks.
    static let idleSeconds: TimeInterval = 3

    private var blockers: Set<DockletHideBlocker> = []
    /// The time of the latest touch or key press.
    private var lastActivity: TimeInterval
    /// The time of the outside touch whose hide is pending. That hide is due at once.
    private var pendingOutsideTouch: TimeInterval?

    init(startedAt time: TimeInterval) {
        lastActivity = time
    }

    /// The time the Docklet tucks, or nil while a blocker holds it.
    var deadline: TimeInterval? {
        guard blockers.isEmpty else {
            return nil
        }
        return pendingOutsideTouch ?? lastActivity + Self.idleSeconds
    }

    /// Whether the Docklet should tuck at `time`.
    func isDue(at time: TimeInterval) -> Bool {
        guard let deadline else {
            return false
        }
        return deadline <= time
    }

    /// A touch on the Docklet cancels a pending outside-touch hide. An outside touch starts one unless a blocker
    /// holds, and a further outside touch leaves that hide where it is.
    mutating func touch(isOnDocklet: Bool, at time: TimeInterval) {
        lastActivity = time
        if isOnDocklet {
            pendingOutsideTouch = nil
        } else if blockers.isEmpty, pendingOutsideTouch == nil {
            pendingOutsideTouch = time
        }
    }

    mutating func keyPress(at time: TimeInterval) {
        lastActivity = time
    }

    /// Holding a blocker drops a pending outside-touch hide. Clearing the last blocker restarts the idle countdown
    /// at `time`, so a blocked stretch never counts as idle time.
    mutating func setBlocked(_ blocker: DockletHideBlocker, _ isBlocked: Bool, at time: TimeInterval) {
        guard isBlocked else {
            if blockers.remove(blocker) != nil, blockers.isEmpty {
                lastActivity = time
            }
            return
        }
        blockers.insert(blocker)
        pendingOutsideTouch = nil
    }
}
